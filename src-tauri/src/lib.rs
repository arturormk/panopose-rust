#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    env,
    ffi::OsString,
    io::Cursor,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, FixedOffset};
use image::{
    DynamicImage, GrayImage, ImageFormat, ImageReader, Luma, RgbaImage, imageops::FilterType,
};
use panopose_core::{
    APP_VERSION, AstronomyProvider, CelestialMarker, CelestialObject, EquirectangularMapping,
    Orientation, Project, StarMarker, StellariumLandscape,
    astronomy::{ApproximateAstronomyProvider, Observer},
    export::{
        ExportRequest, export_equirectangular_with_mask_and_progress,
        validate_equirectangular_dimensions, viewer_texture_pixel_center_to_alt_az,
    },
    stellarium_landscape_ini,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Serialize)]
struct ImageInfo {
    width: u32,
    height: u32,
    plausible_equirectangular: bool,
}

#[derive(Debug, Deserialize)]
struct AstronomyRequest {
    latitude_deg: f64,
    longitude_deg: f64,
    elevation_m: f64,
    time: DateTime<FixedOffset>,
    objects: Vec<CelestialObject>,
}

#[derive(Debug, Deserialize)]
struct StarRequest {
    latitude_deg: f64,
    longitude_deg: f64,
    elevation_m: f64,
    time: DateTime<FixedOffset>,
}

#[derive(Debug, Deserialize)]
struct ExportImageRequest {
    input: PathBuf,
    output: PathBuf,
    width: u32,
    height: u32,
    center_azimuth_deg: f64,
    yaw_deg: f64,
    pitch_deg: f64,
    roll_deg: f64,
    sky_removal: bool,
}

#[derive(Debug, Deserialize)]
struct ExportStellariumLandscapeRequest {
    input: PathBuf,
    output_zip: PathBuf,
    directory_name: String,
    texture_filename: String,
    landscape_name: String,
    author: String,
    description: String,
    width: u32,
    height: u32,
    yaw_deg: f64,
    pitch_deg: f64,
    roll_deg: f64,
    sky_removal: bool,
    latitude_deg: f64,
    longitude_deg: f64,
    elevation_m: f64,
}

#[derive(Debug, Deserialize)]
struct PreviewSkyRemovedRequest {
    input: PathBuf,
    max_width: u32,
    yaw_deg: f64,
    pitch_deg: f64,
    roll_deg: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ExportProgress {
    completed_rows: u32,
    total_rows: u32,
}

#[derive(Debug, Deserialize)]
struct WriteMetadataRequest {
    path: PathBuf,
    source_path: Option<PathBuf>,
    overwrite_existing: bool,
    yaw_deg: f64,
    pitch_deg: f64,
    roll_deg: f64,
    latitude_deg: f64,
    longitude_deg: f64,
    elevation_m: f64,
    capture_time: DateTime<FixedOffset>,
    reference_time: DateTime<FixedOffset>,
    timezone: String,
}

static SKYSEG_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const PANORAMA_BASE_YAW_DEG: f64 = -90.0;
const STELLARIUM_ANGLE_ROTATEZ_DEG: f64 = -90.0;

struct SkysegExportMasks {
    source_alpha_mask: GrayImage,
    corrected_sky_mask: GrayImage,
}

#[tauri::command]
fn new_project(name: String) -> Project {
    Project::new(name)
}

#[tauri::command]
fn inspect_image(path: PathBuf) -> Result<ImageInfo, String> {
    let reader = ImageReader::open(&path).map_err(|err| err.to_string())?;
    let dimensions = reader.into_dimensions().map_err(|err| err.to_string())?;
    Ok(ImageInfo {
        width: dimensions.0,
        height: dimensions.1,
        plausible_equirectangular: validate_equirectangular_dimensions(dimensions.0, dimensions.1)
            .is_ok(),
    })
}

#[tauri::command]
fn path_exists(path: PathBuf) -> bool {
    path.exists()
}

#[tauri::command]
fn read_file(path: PathBuf) -> Result<Vec<u8>, String> {
    std::fs::read(&path).map_err(|err| err.to_string())
}

#[tauri::command]
fn app_version() -> &'static str {
    APP_VERSION
}

#[tauri::command]
fn skyseg_available() -> bool {
    find_skyseg_ncnn_in_path_var(env::var_os("PATH")).is_some()
}

#[tauri::command]
fn astronomy_markers(request: AstronomyRequest) -> Vec<CelestialMarker> {
    ApproximateAstronomyProvider.markers(
        Observer {
            latitude_deg: request.latitude_deg,
            longitude_deg: request.longitude_deg,
            elevation_m: request.elevation_m,
        },
        request.time,
        &request.objects,
    )
}

#[tauri::command]
fn star_markers(request: StarRequest) -> Vec<StarMarker> {
    ApproximateAstronomyProvider.star_markers(
        Observer {
            latitude_deg: request.latitude_deg,
            longitude_deg: request.longitude_deg,
            elevation_m: request.elevation_m,
        },
        request.time,
    )
}

#[tauri::command]
async fn export_image(app: AppHandle, request: ExportImageRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = ImageReader::open(&request.input)
            .map_err(|err| err.to_string())?
            .decode()
            .map_err(|err| err.to_string())?;
        let export_request = ExportRequest {
            width: request.width,
            height: request.height,
            center_azimuth_deg: request.center_azimuth_deg,
            orientation: export_orientation_from_pose(
                request.yaw_deg,
                request.pitch_deg,
                request.roll_deg,
            ),
        };
        let skyseg_masks = if request.sky_removal {
            Some(skyseg_masks_for_export(&source, export_request)?)
        } else {
            None
        };
        let mut exported = export_equirectangular_with_mask_and_progress(
            &source,
            export_request,
            skyseg_masks.as_ref().map(|masks| &masks.source_alpha_mask),
            |completed_rows, total_rows| {
                let _ = app.emit(
                    "export-progress",
                    ExportProgress {
                        completed_rows,
                        total_rows,
                    },
                );
            },
        )
        .map_err(|err| err.to_string())?;
        if let Some(masks) = skyseg_masks {
            decontaminate_sky_edges(&mut exported, &masks.corrected_sky_mask);
        }
        let exported = finalize_exported_pano(exported);
        exported
            .save(&request.output)
            .map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
async fn export_stellarium_landscape(
    app: AppHandle,
    request: ExportStellariumLandscapeRequest,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let directory_name = validate_stellarium_directory_name(&request.directory_name)?;
        let texture_filename = validate_stellarium_texture_filename(&request.texture_filename)?;

        let source = ImageReader::open(&request.input)
            .map_err(|err| err.to_string())?
            .decode()
            .map_err(|err| err.to_string())?;
        let export_request = ExportRequest {
            width: request.width,
            height: request.height,
            center_azimuth_deg: 180.0,
            orientation: export_orientation_from_pose(
                request.yaw_deg,
                request.pitch_deg,
                request.roll_deg,
            ),
        };
        let skyseg_masks = if request.sky_removal {
            Some(skyseg_masks_for_export(&source, export_request)?)
        } else {
            None
        };
        let mut exported = export_equirectangular_with_mask_and_progress(
            &source,
            export_request,
            skyseg_masks.as_ref().map(|masks| &masks.source_alpha_mask),
            |completed_rows, total_rows| {
                let _ = app.emit(
                    "export-progress",
                    ExportProgress {
                        completed_rows,
                        total_rows,
                    },
                );
            },
        )
        .map_err(|err| err.to_string())?;
        if let Some(masks) = skyseg_masks {
            decontaminate_sky_edges(&mut exported, &masks.corrected_sky_mask);
        }
        let exported = finalize_exported_pano(exported);
        let mut texture_bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(exported)
            .write_to(&mut texture_bytes, ImageFormat::Png)
            .map_err(|err| err.to_string())?;

        let ini = stellarium_landscape_ini(&StellariumLandscape {
            name: request.landscape_name,
            author: request.author,
            description: request.description,
            maptex: texture_filename.clone(),
            angle_rotatez_deg: STELLARIUM_ANGLE_ROTATEZ_DEG,
            latitude_deg: request.latitude_deg,
            longitude_deg: request.longitude_deg,
            altitude_m: request.elevation_m,
        });
        let zip_bytes = build_stellarium_zip(
            &directory_name,
            &texture_filename,
            ini.as_bytes(),
            texture_bytes.get_ref(),
        )?;
        std::fs::write(&request.output_zip, zip_bytes)
            .map_err(|err| format!("failed to write {}: {err}", request.output_zip.display()))
    })
    .await
    .map_err(|err| err.to_string())?
}

fn validate_stellarium_directory_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
    {
        return Err("landscape directory name must be a plain directory name".to_string());
    }
    Ok(trimmed.to_string())
}

fn export_orientation_from_pose(yaw_deg: f64, pitch_deg: f64, roll_deg: f64) -> Orientation {
    let x = pitch_deg.to_radians();
    let y = (PANORAMA_BASE_YAW_DEG + yaw_deg).to_radians();
    let z = roll_deg.to_radians();

    let c1 = (x / 2.0).cos();
    let c2 = (y / 2.0).cos();
    let c3 = (z / 2.0).cos();
    let s1 = (x / 2.0).sin();
    let s2 = (y / 2.0).sin();
    let s3 = (z / 2.0).sin();

    Orientation {
        x: s1 * c2 * c3 + c1 * s2 * s3,
        y: c1 * s2 * c3 - s1 * c2 * s3,
        z: c1 * c2 * s3 - s1 * s2 * c3,
        w: c1 * c2 * c3 + s1 * s2 * s3,
    }
}

fn finalize_exported_pano(exported: RgbaImage) -> RgbaImage {
    image::imageops::flip_horizontal(&exported)
}

fn validate_stellarium_texture_filename(filename: &str) -> Result<String, String> {
    let trimmed = filename.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
    {
        return Err("texture filename must be a plain PNG filename".to_string());
    }
    if !trimmed.to_ascii_lowercase().ends_with(".png") {
        return Err("texture filename must end with .png".to_string());
    }
    Ok(trimmed.to_string())
}

fn build_stellarium_zip(
    directory_name: &str,
    texture_filename: &str,
    ini_bytes: &[u8],
    texture_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let directory_entry = format!("{directory_name}/");
    let ini_entry = format!("{directory_name}/landscape.ini");
    let texture_entry = format!("{directory_name}/{texture_filename}");
    let mut zip = Vec::new();
    let mut central_directory = Vec::new();

    add_stored_zip_entry(&mut zip, &mut central_directory, &directory_entry, &[])?;
    add_stored_zip_entry(&mut zip, &mut central_directory, &ini_entry, ini_bytes)?;
    add_stored_zip_entry(
        &mut zip,
        &mut central_directory,
        &texture_entry,
        texture_bytes,
    )?;

    let central_directory_offset = checked_u32(zip.len(), "ZIP central directory offset")?;
    let central_directory_size =
        checked_u32(central_directory.len(), "ZIP central directory size")?;
    zip.extend_from_slice(&central_directory);
    write_u32_le(&mut zip, 0x0605_4b50);
    write_u16_le(&mut zip, 0);
    write_u16_le(&mut zip, 0);
    write_u16_le(&mut zip, 3);
    write_u16_le(&mut zip, 3);
    write_u32_le(&mut zip, central_directory_size);
    write_u32_le(&mut zip, central_directory_offset);
    write_u16_le(&mut zip, 0);
    Ok(zip)
}

fn add_stored_zip_entry(
    zip: &mut Vec<u8>,
    central_directory: &mut Vec<u8>,
    name: &str,
    data: &[u8],
) -> Result<(), String> {
    let local_header_offset = checked_u32(zip.len(), "ZIP local header offset")?;
    let name_bytes = name.as_bytes();
    let name_len = checked_u16(name_bytes.len(), "ZIP entry name length")?;
    let data_len = checked_u32(data.len(), "ZIP entry data length")?;
    let crc = crc32(data);

    write_u32_le(zip, 0x0403_4b50);
    write_u16_le(zip, 20);
    write_u16_le(zip, 0);
    write_u16_le(zip, 0);
    write_u16_le(zip, 0);
    write_u16_le(zip, 0);
    write_u32_le(zip, crc);
    write_u32_le(zip, data_len);
    write_u32_le(zip, data_len);
    write_u16_le(zip, name_len);
    write_u16_le(zip, 0);
    zip.extend_from_slice(name_bytes);
    zip.extend_from_slice(data);

    write_u32_le(central_directory, 0x0201_4b50);
    write_u16_le(central_directory, 20);
    write_u16_le(central_directory, 20);
    write_u16_le(central_directory, 0);
    write_u16_le(central_directory, 0);
    write_u16_le(central_directory, 0);
    write_u16_le(central_directory, 0);
    write_u32_le(central_directory, crc);
    write_u32_le(central_directory, data_len);
    write_u32_le(central_directory, data_len);
    write_u16_le(central_directory, name_len);
    write_u16_le(central_directory, 0);
    write_u16_le(central_directory, 0);
    write_u16_le(central_directory, 0);
    write_u16_le(central_directory, 0);
    write_u32_le(
        central_directory,
        if name.ends_with('/') { 0x10 } else { 0 },
    );
    write_u32_le(central_directory, local_header_offset);
    central_directory.extend_from_slice(name_bytes);

    Ok(())
}

fn checked_u16(value: usize, label: &str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{label} is too large"))
}

fn checked_u32(value: usize, label: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("{label} is too large"))
}

fn write_u16_le(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u32_le(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn stellarium_zip_contains_directory_ini_and_texture_entries() {
        let zip = build_stellarium_zip("east-terrace", "east-terrace.png", b"ini", b"png")
            .expect("ZIP should be built");
        let text = String::from_utf8_lossy(&zip);

        assert!(text.contains("east-terrace/"));
        assert!(text.contains("east-terrace/landscape.ini"));
        assert!(text.contains("east-terrace/east-terrace.png"));
        assert!(zip.ends_with(&[0, 0]));
    }

    #[test]
    fn export_orientation_matches_threejs_yxz_euler() {
        let orientation = export_orientation_from_pose(34.0, 12.0, -7.0);

        assert_close(orientation.w, 0.87946870366100183);
        assert_close(orientation.x, 0.12062455744066670);
        assert_close(orientation.y, -0.46039452397221503);
        assert_close(orientation.z, -0.0046257669069801124);
    }

    #[test]
    fn stellarium_export_keeps_legacy_rotatez_offset() {
        assert_eq!(STELLARIUM_ANGLE_ROTATEZ_DEG, -90.0);
    }

    #[test]
    fn finalized_export_is_horizontally_flipped_once() {
        let image = RgbaImage::from_fn(3, 1, |x, _| image::Rgba([x as u8, 0, 0, 255]));
        let finalized = finalize_exported_pano(image);

        assert_eq!(finalized.get_pixel(0, 0).0[0], 2);
        assert_eq!(finalized.get_pixel(1, 0).0[0], 1);
        assert_eq!(finalized.get_pixel(2, 0).0[0], 0);
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "expected {expected}, got {actual}"
        );
    }
}

#[tauri::command]
async fn preview_sky_removed_image(request: PreviewSkyRemovedRequest) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = ImageReader::open(&request.input)
            .map_err(|err| err.to_string())?
            .decode()
            .map_err(|err| err.to_string())?;
        let width = if request.max_width > 0 && source.width() > request.max_width {
            request.max_width
        } else {
            source.width()
        };
        let height =
            ((source.height() as f64 * width as f64 / source.width() as f64).round() as u32).max(1);
        let orientation =
            export_orientation_from_pose(request.yaw_deg, request.pitch_deg, request.roll_deg);
        let corrected = export_equirectangular_with_mask_and_progress(
            &source,
            ExportRequest {
                width,
                height,
                center_azimuth_deg: 180.0,
                orientation,
            },
            None,
            |_, _| {},
        )
        .map_err(|err| err.to_string())?;
        let corrected_mask = skyseg_mask_for_image(&corrected)?;
        let source_mask =
            corrected_mask_to_source_space(&corrected_mask, orientation, width, height, 180.0);
        let mut preview = source.to_rgba8();
        if preview.width() != width || preview.height() != height {
            preview = image::imageops::resize(&preview, width, height, FilterType::Triangle);
        }
        apply_inverted_sky_mask_alpha(&mut preview, &DynamicImage::ImageLuma8(source_mask));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(preview)
            .write_to(&mut bytes, ImageFormat::Png)
            .map_err(|err| err.to_string())?;
        Ok(bytes.into_inner())
    })
    .await
    .map_err(|err| err.to_string())?
}

fn find_skyseg_ncnn_in_path_var(path: Option<OsString>) -> Option<PathBuf> {
    let path = path?;
    env::split_paths(&path)
        .map(|directory| directory.join("skyseg-ncnn"))
        .find(|candidate| candidate.is_file() && is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &PathBuf) -> bool {
    path.metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &PathBuf) -> bool {
    true
}

fn skyseg_masks_for_export(
    source: &DynamicImage,
    request: ExportRequest,
) -> Result<SkysegExportMasks, String> {
    let corrected = export_equirectangular_with_mask_and_progress(source, request, None, |_, _| {})
        .map_err(|err| err.to_string())?;
    let corrected_mask = skyseg_mask_for_image(&corrected)?;
    let source_mask = corrected_mask_to_source_space(
        &corrected_mask,
        request.orientation,
        source.width(),
        source.height(),
        request.center_azimuth_deg,
    );
    Ok(SkysegExportMasks {
        source_alpha_mask: sky_mask_to_alpha_mask(&source_mask),
        corrected_sky_mask: corrected_mask,
    })
}

fn skyseg_mask_for_image(image: &RgbaImage) -> Result<GrayImage, String> {
    let skyseg = find_skyseg_ncnn_in_path_var(env::var_os("PATH"))
        .ok_or_else(|| "skyseg-ncnn was not found on PATH".to_string())?;
    let temp_dir = unique_skyseg_temp_dir();
    std::fs::create_dir_all(&temp_dir)
        .map_err(|err| format!("failed to create skyseg temp directory: {err}"))?;
    let input_path = temp_dir.join("corrected-pano.png");
    let mask_path = temp_dir.join("mask.jpg");

    let result = (|| {
        image
            .save(&input_path)
            .map_err(|err| format!("failed to write skyseg input image: {err}"))?;
        let output = Command::new(&skyseg)
            .arg(&input_path)
            .arg(&mask_path)
            .output()
            .map_err(|err| format!("failed to run skyseg-ncnn: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "skyseg-ncnn failed with status {}: {}{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let mask = ImageReader::open(&mask_path)
            .map_err(|err| format!("failed to open skyseg mask: {err}"))?
            .decode()
            .map_err(|err| format!("failed to decode skyseg mask: {err}"))?;
        Ok(matching_mask_dimensions(
            &mask.to_luma8(),
            image.width(),
            image.height(),
        ))
    })();

    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

fn unique_skyseg_temp_dir() -> PathBuf {
    let count = SKYSEG_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    env::temp_dir().join(format!(
        "panopose-skyseg-{}-{millis}-{count}",
        std::process::id()
    ))
}

fn apply_inverted_sky_mask_alpha(image: &mut RgbaImage, mask: &DynamicImage) {
    let mask = mask.to_luma8();
    let mask = matching_mask_dimensions(&mask, image.width(), image.height());
    decontaminate_sky_edges(image, &mask);
    apply_sky_mask_as_alpha(image, &mask);
}

fn apply_sky_mask_as_alpha(image: &mut RgbaImage, sky_mask: &GrayImage) {
    let alpha_mask = sky_mask_to_alpha_mask(&matching_mask_dimensions(
        sky_mask,
        image.width(),
        image.height(),
    ));
    apply_alpha_mask(image, &alpha_mask);
}

fn apply_alpha_mask(image: &mut RgbaImage, alpha_mask: &GrayImage) {
    let alpha_mask = matching_mask_dimensions(alpha_mask, image.width(), image.height());
    for y in 0..image.height() {
        for x in 0..image.width() {
            let alpha = alpha_mask.get_pixel(x, y).0[0] as u16;
            let pixel = image.get_pixel_mut(x, y);
            pixel.0[3] = ((pixel.0[3] as u16 * alpha) / 255) as u8;
        }
    }
}

fn sky_mask_to_alpha_mask(sky_mask: &GrayImage) -> GrayImage {
    let mut alpha_mask = GrayImage::new(sky_mask.width(), sky_mask.height());
    for y in 0..sky_mask.height() {
        for x in 0..sky_mask.width() {
            alpha_mask.put_pixel(
                x,
                y,
                Luma([255u8.saturating_sub(sky_mask.get_pixel(x, y).0[0])]),
            );
        }
    }
    alpha_mask
}

fn decontaminate_sky_edges(image: &mut RgbaImage, mask: &GrayImage) {
    let mask = matching_mask_dimensions(mask, image.width(), image.height());
    let original = image.clone();
    for y in 0..image.height() {
        for x in 0..image.width() {
            let sky = mask.get_pixel(x, y).0[0] as u16;
            let alpha = 255u16.saturating_sub(sky);
            let pixel = image.get_pixel_mut(x, y);
            if (8..=247).contains(&alpha) {
                if let Some(sky_rgb) = find_nearby_solid_sky_color(&original, &mask, x, y) {
                    let corrected = remove_sky_color_from_edge_pixel(pixel.0, sky_rgb, alpha as u8);
                    pixel.0[0] = corrected[0];
                    pixel.0[1] = corrected[1];
                    pixel.0[2] = corrected[2];
                }
            }
        }
    }
}

fn corrected_mask_to_source_space(
    corrected_mask: &GrayImage,
    orientation: Orientation,
    source_width: u32,
    source_height: u32,
    corrected_center_azimuth_deg: f64,
) -> GrayImage {
    let corrected_mapping = EquirectangularMapping::new(
        corrected_mask.width(),
        corrected_mask.height(),
        corrected_center_azimuth_deg,
    );
    let mut source_mask = GrayImage::new(source_width, source_height);

    for y in 0..source_mask.height() {
        for x in 0..source_mask.width() {
            let source_alt_az =
                viewer_texture_pixel_center_to_alt_az(source_width, source_height, x, y);
            let world_alt_az = orientation.source_alt_az_to_world(source_alt_az);
            let (mx, my) = corrected_mapping.alt_az_to_pixel_f64(world_alt_az);
            source_mask.put_pixel(x, y, sample_wrapped_bilinear_gray(corrected_mask, mx, my));
        }
    }

    source_mask
}

fn sample_wrapped_bilinear_gray(image: &GrayImage, x: f64, y: f64) -> Luma<u8> {
    let width = image.width() as i64;
    let height = image.height() as i64;
    let x0 = x.floor();
    let y0 = y.floor();
    let tx = x - x0;
    let ty = y - y0;

    let y0i = (y0 as i64).clamp(0, height - 1);
    let y1i = (y0 as i64 + 1).clamp(0, height - 1);
    let x0i = (x0 as i64).rem_euclid(width);
    let x1i = (x0 as i64 + 1).rem_euclid(width);

    let p00 = image.get_pixel(x0i as u32, y0i as u32).0[0];
    let p10 = image.get_pixel(x1i as u32, y0i as u32).0[0];
    let p01 = image.get_pixel(x0i as u32, y1i as u32).0[0];
    let p11 = image.get_pixel(x1i as u32, y1i as u32).0[0];

    let top = lerp(p00 as f64, p10 as f64, tx);
    let bottom = lerp(p01 as f64, p11 as f64, tx);
    Luma([lerp(top, bottom, ty).round().clamp(0.0, 255.0) as u8])
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn find_nearby_solid_sky_color(
    image: &RgbaImage,
    mask: &GrayImage,
    x: u32,
    y: u32,
) -> Option<[u8; 3]> {
    const HORIZONTAL_SEARCH_RADIUS: i32 = 12;

    if y == 0 {
        return None;
    }

    let max_vertical_distance = (image.height() / 8).clamp(16, 128).min(y);
    for distance in 1..=max_vertical_distance {
        let sy = y - distance;
        for horizontal_distance in 0..=HORIZONTAL_SEARCH_RADIUS {
            let offsets = if horizontal_distance == 0 {
                [0, 0]
            } else {
                [-horizontal_distance, horizontal_distance]
            };
            for dx in offsets {
                if horizontal_distance == 0 && dx != 0 {
                    continue;
                }
                let sx = (x as i32 + dx).rem_euclid(image.width() as i32) as u32;
                if let Some(color) = solid_sky_patch_color(image, mask, sx, sy) {
                    return Some(color);
                }
            }
        }
    }

    None
}

fn solid_sky_patch_color(
    image: &RgbaImage,
    mask: &GrayImage,
    center_x: u32,
    center_y: u32,
) -> Option<[u8; 3]> {
    const SKY_THRESHOLD: u8 = 240;
    const PATCH_RADIUS: i32 = 2;
    const MIN_SKY_PIXELS: u32 = 9;
    const REQUIRED_SKY_NUMERATOR: u32 = 4;
    const REQUIRED_SKY_DENOMINATOR: u32 = 5;

    let mut available = 0u32;
    let mut sky_pixels = 0u32;
    let mut total = [0u32; 3];

    for dy in -PATCH_RADIUS..=PATCH_RADIUS {
        let y = center_y as i32 + dy;
        if y < 0 || y >= mask.height() as i32 {
            continue;
        }
        for dx in -PATCH_RADIUS..=PATCH_RADIUS {
            let x = (center_x as i32 + dx).rem_euclid(mask.width() as i32) as u32;
            available += 1;
            if mask.get_pixel(x, y as u32).0[0] >= SKY_THRESHOLD {
                let pixel = image.get_pixel(x, y as u32).0;
                total[0] += u32::from(pixel[0]);
                total[1] += u32::from(pixel[1]);
                total[2] += u32::from(pixel[2]);
                sky_pixels += 1;
            }
        }
    }

    if sky_pixels < MIN_SKY_PIXELS.min(available)
        || sky_pixels * REQUIRED_SKY_DENOMINATOR < available * REQUIRED_SKY_NUMERATOR
    {
        return None;
    }

    Some([
        (total[0] / sky_pixels) as u8,
        (total[1] / sky_pixels) as u8,
        (total[2] / sky_pixels) as u8,
    ])
}

fn remove_sky_color_from_edge_pixel(observed: [u8; 4], sky_rgb: [u8; 3], alpha: u8) -> [u8; 3] {
    let alpha = f64::from(alpha) / 255.0;
    let mut corrected = [0u8; 3];
    for channel in 0..3 {
        corrected[channel] =
            ((f64::from(observed[channel]) - f64::from(sky_rgb[channel]) * (1.0 - alpha)) / alpha)
                .round()
                .clamp(0.0, 255.0) as u8;
    }
    corrected
}

fn matching_mask_dimensions(mask: &GrayImage, width: u32, height: u32) -> GrayImage {
    if mask.width() == width && mask.height() == height {
        mask.clone()
    } else {
        image::imageops::resize(mask, width, height, FilterType::Triangle)
    }
}

#[tauri::command]
fn write_panopose_metadata(request: WriteMetadataRequest) -> Result<(), String> {
    if request.path.exists() && !request.overwrite_existing {
        return Err("target file already exists".to_string());
    }

    if !request.path.exists() {
        let source_path = request.source_path.as_ref().ok_or_else(|| {
            "target file does not exist and no source image is loaded".to_string()
        })?;
        std::fs::copy(source_path, &request.path)
            .map_err(|err| format!("failed to copy source image for Save As: {err}"))?;
    }

    let subject = format!(
        "PanoPose yaw={:.9}; pitch={:.9}; roll={:.9}; timezone={}",
        request.yaw_deg, request.pitch_deg, request.roll_deg, request.timezone
    );
    let description = serde_json::json!({
        "panopose": {
            "schema_version": 1,
            "yaw_deg": request.yaw_deg,
            "pitch_deg": request.pitch_deg,
            "roll_deg": request.roll_deg,
            "latitude_deg": request.latitude_deg,
            "longitude_deg": request.longitude_deg,
            "elevation_m": request.elevation_m,
            "capture_time": request.capture_time.to_rfc3339(),
            "reference_time": request.reference_time.to_rfc3339(),
            "timezone": request.timezone,
        }
    })
    .to_string();
    let gps_lat_ref = if request.latitude_deg < 0.0 { "S" } else { "N" };
    let gps_lon_ref = if request.longitude_deg < 0.0 {
        "W"
    } else {
        "E"
    };

    let output = Command::new("exiftool")
        .arg("-overwrite_original")
        .arg("-XMP-GPano:UsePanoramaViewer=True")
        .arg("-XMP-GPano:ProjectionType=equirectangular")
        .arg(format!("-XMP-GPano:PoseHeadingDegrees={}", request.yaw_deg))
        .arg(format!("-XMP-GPano:PosePitchDegrees={}", request.pitch_deg))
        .arg(format!("-XMP-GPano:PoseRollDegrees={}", request.roll_deg))
        .arg(format!("-XMP:Subject={subject}"))
        .arg(format!("-XMP:Description={description}"))
        .arg(format!(
            "-EXIF:DateTimeOriginal={}",
            request.capture_time.format("%Y:%m:%d %H:%M:%S")
        ))
        .arg(format!(
            "-EXIF:OffsetTimeOriginal={}",
            request.capture_time.format("%:z")
        ))
        .arg(format!("-EXIF:GPSLatitude={}", request.latitude_deg.abs()))
        .arg(format!("-EXIF:GPSLatitudeRef={gps_lat_ref}"))
        .arg(format!(
            "-EXIF:GPSLongitude={}",
            request.longitude_deg.abs()
        ))
        .arg(format!("-EXIF:GPSLongitudeRef={gps_lon_ref}"))
        .arg(format!("-EXIF:GPSAltitude={}", request.elevation_m.abs()))
        .arg(format!(
            "-EXIF:GPSAltitudeRef={}",
            if request.elevation_m < 0.0 { 1 } else { 0 }
        ))
        .arg(&request.path)
        .output()
        .map_err(|err| format!("failed to run exiftool: {err}"))?;

    if output.status.success() {
        verify_gpano_pose_written(&request.path)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "exiftool failed with status {}: {}{}",
            output.status, stdout, stderr
        ))
    }
}

fn verify_gpano_pose_written(path: &PathBuf) -> Result<(), String> {
    let output = Command::new("exiftool")
        .arg("-s3")
        .arg("-XMP-GPano:PoseHeadingDegrees")
        .arg("-XMP-GPano:PosePitchDegrees")
        .arg("-XMP-GPano:PoseRollDegrees")
        .arg(path)
        .output()
        .map_err(|err| format!("failed to verify GPano pose metadata: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "failed to verify GPano pose metadata: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let values = String::from_utf8_lossy(&output.stdout);
    if values
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        == 3
    {
        Ok(())
    } else {
        Err(format!(
            "exiftool completed, but GPano pose metadata was not written to {}",
            path.display()
        ))
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            new_project,
            inspect_image,
            path_exists,
            read_file,
            app_version,
            skyseg_available,
            astronomy_markers,
            star_markers,
            export_image,
            export_stellarium_landscape,
            preview_sky_removed_image,
            write_panopose_metadata
        ])
        .run(tauri::generate_context!())
        .expect("error while running PanoPose");
}

#[cfg(test)]
mod skyseg_tests {
    use super::*;
    use image::{Luma, Rgba};

    #[test]
    fn inverted_skyseg_mask_controls_alpha() {
        let mut image = RgbaImage::from_pixel(3, 1, Rgba([10, 20, 30, 255]));
        let mask = DynamicImage::ImageLuma8(GrayImage::from_vec(3, 1, vec![0, 128, 255]).unwrap());

        apply_inverted_sky_mask_alpha(&mut image, &mask);

        assert_eq!(image.get_pixel(0, 0).0[3], 255);
        assert_eq!(image.get_pixel(1, 0).0[3], 127);
        assert_eq!(image.get_pixel(2, 0).0[3], 0);
    }

    #[test]
    fn sky_mask_to_alpha_mask_inverts_skyseg_polarity() {
        let sky_mask = GrayImage::from_vec(3, 1, vec![0, 128, 255]).unwrap();
        let alpha_mask = sky_mask_to_alpha_mask(&sky_mask);

        assert_eq!(alpha_mask.get_pixel(0, 0).0[0], 255);
        assert_eq!(alpha_mask.get_pixel(1, 0).0[0], 127);
        assert_eq!(alpha_mask.get_pixel(2, 0).0[0], 0);
    }

    #[test]
    fn remapped_source_alpha_matches_corrected_mask_orientation() {
        let orientation = export_orientation_from_pose(90.0, 0.0, 0.0);
        let corrected_sky_mask =
            GrayImage::from_fn(16, 8, |x, _| if x < 8 { Luma([255]) } else { Luma([0]) });
        let source_sky_mask =
            corrected_mask_to_source_space(&corrected_sky_mask, orientation, 16, 8, 180.0);
        let source_alpha_mask = sky_mask_to_alpha_mask(&source_sky_mask);
        let source = DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 8, Rgba([1, 2, 3, 255])));
        let exported = export_equirectangular_with_mask_and_progress(
            &source,
            ExportRequest {
                width: 16,
                height: 8,
                center_azimuth_deg: 180.0,
                orientation,
            },
            Some(&source_alpha_mask),
            |_, _| {},
        )
        .unwrap();

        for y in 0..8 {
            for x in 0..16 {
                let expected_alpha = 255 - corrected_sky_mask.get_pixel(x, y).0[0];
                assert_eq!(
                    exported.get_pixel(x, y).0[3],
                    expected_alpha,
                    "alpha mismatch at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn inverted_skyseg_mask_multiplies_existing_alpha() {
        let mut image = RgbaImage::from_pixel(1, 1, Rgba([10, 20, 30, 128]));
        let mask = DynamicImage::ImageLuma8(GrayImage::from_pixel(1, 1, Luma([128])));

        apply_inverted_sky_mask_alpha(&mut image, &mask);

        assert_eq!(image.get_pixel(0, 0).0[3], 63);
    }

    #[test]
    fn decontaminates_partial_alpha_edge_with_sky_above() {
        let foreground = [80u8, 60, 40];
        let sky = [120u8, 180, 240];
        let alpha = 128u8;
        let observed = [
            mix_channel(foreground[0], sky[0], alpha),
            mix_channel(foreground[1], sky[1], alpha),
            mix_channel(foreground[2], sky[2], alpha),
            255,
        ];
        let mut image = RgbaImage::from_pixel(
            5,
            5,
            Rgba([foreground[0], foreground[1], foreground[2], 255]),
        );
        let mut mask = GrayImage::from_pixel(5, 5, Luma([0]));
        for y in 0..3 {
            for x in 0..5 {
                image.put_pixel(x, y, Rgba([sky[0], sky[1], sky[2], 255]));
                mask.put_pixel(x, y, Luma([255]));
            }
        }
        image.put_pixel(2, 3, Rgba(observed));
        mask.put_pixel(2, 3, Luma([127]));
        let mask = DynamicImage::ImageLuma8(mask);

        apply_inverted_sky_mask_alpha(&mut image, &mask);

        assert_rgb_near(&image.get_pixel(2, 3).0[0..3], &foreground, 1);
        assert_eq!(image.get_pixel(2, 3).0[3], 128);
    }

    #[test]
    fn isolated_sky_hole_above_edge_is_not_used_as_sky_color() {
        let mut image = RgbaImage::from_pixel(7, 7, Rgba([50, 60, 70, 255]));
        image.put_pixel(3, 5, Rgba([120, 180, 240, 255]));
        image.put_pixel(3, 6, Rgba([85, 120, 155, 255]));
        let mut mask = GrayImage::from_pixel(7, 7, Luma([0]));
        mask.put_pixel(3, 5, Luma([255]));
        mask.put_pixel(3, 6, Luma([127]));
        let mask = DynamicImage::ImageLuma8(mask);

        apply_inverted_sky_mask_alpha(&mut image, &mask);

        assert_eq!(&image.get_pixel(3, 6).0[0..3], &[85, 120, 155]);
        assert_eq!(image.get_pixel(3, 6).0[3], 128);
    }

    #[test]
    fn very_low_alpha_edge_keeps_rgb_stable() {
        let mut image = RgbaImage::from_pixel(1, 2, Rgba([10, 20, 30, 255]));
        image.put_pixel(0, 0, Rgba([100, 150, 200, 255]));
        image.put_pixel(0, 1, Rgba([12, 24, 36, 255]));
        let mask = DynamicImage::ImageLuma8(GrayImage::from_vec(1, 2, vec![255, 250]).unwrap());

        apply_inverted_sky_mask_alpha(&mut image, &mask);

        assert_eq!(&image.get_pixel(0, 1).0[0..3], &[12, 24, 36]);
        assert_eq!(image.get_pixel(0, 1).0[3], 5);
    }

    #[test]
    fn edge_without_sky_sample_keeps_rgb_unchanged() {
        let mut image = RgbaImage::from_pixel(1, 1, Rgba([50, 60, 70, 255]));
        let mask = DynamicImage::ImageLuma8(GrayImage::from_pixel(1, 1, Luma([128])));

        apply_inverted_sky_mask_alpha(&mut image, &mask);

        assert_eq!(&image.get_pixel(0, 0).0[0..3], &[50, 60, 70]);
        assert_eq!(image.get_pixel(0, 0).0[3], 127);
    }

    fn mix_channel(foreground: u8, sky: u8, alpha: u8) -> u8 {
        let alpha = f64::from(alpha) / 255.0;
        (f64::from(foreground) * alpha + f64::from(sky) * (1.0 - alpha)).round() as u8
    }

    fn assert_rgb_near(actual: &[u8], expected: &[u8; 3], tolerance: u8) {
        for channel in 0..3 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= tolerance,
                "channel {channel}: actual {}, expected {}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

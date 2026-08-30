use std::{io::Cursor, path::PathBuf, process::Command};

use chrono::{DateTime, FixedOffset};
use image::{DynamicImage, ImageFormat, ImageReader};
use panopose_core::{
    APP_VERSION, AstronomyProvider, CelestialMarker, CelestialObject, Orientation, Project,
    SkyRemovalSettings, StarMarker, StellariumLandscape,
    astronomy::{ApproximateAstronomyProvider, Observer},
    export::{
        ExportRequest, export_equirectangular_with_mask_and_progress,
        validate_equirectangular_dimensions,
    },
    sky_mask::{detect_sky_alpha_mask, preview_sky_removed},
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
    sky_removal: Option<SkyRemovalSettings>,
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
    sky_removal: Option<SkyRemovalSettings>,
    latitude_deg: f64,
    longitude_deg: f64,
    elevation_m: f64,
}

#[derive(Debug, Deserialize)]
struct PreviewSkyRemovedRequest {
    input: PathBuf,
    settings: SkyRemovalSettings,
    max_width: u32,
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
        let alpha_mask = if let Some(settings) = request.sky_removal {
            let _ = app.emit(
                "export-progress",
                ExportProgress {
                    completed_rows: 0,
                    total_rows: request.height,
                },
            );
            Some(detect_sky_alpha_mask(&source, settings).map_err(|err| err.to_string())?)
        } else {
            None
        };
        let exported = export_equirectangular_with_mask_and_progress(
            &source,
            ExportRequest {
                width: request.width,
                height: request.height,
                center_azimuth_deg: request.center_azimuth_deg,
                orientation: Orientation::from_yaw_pitch_roll_deg(
                    request.yaw_deg,
                    request.pitch_deg,
                    request.roll_deg,
                ),
            },
            alpha_mask.as_ref(),
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
        let alpha_mask = if let Some(settings) = request.sky_removal {
            let _ = app.emit(
                "export-progress",
                ExportProgress {
                    completed_rows: 0,
                    total_rows: request.height,
                },
            );
            Some(detect_sky_alpha_mask(&source, settings).map_err(|err| err.to_string())?)
        } else {
            None
        };
        let exported = export_equirectangular_with_mask_and_progress(
            &source,
            ExportRequest {
                width: request.width,
                height: request.height,
                center_azimuth_deg: 180.0,
                orientation: Orientation::from_yaw_pitch_roll_deg(
                    request.yaw_deg,
                    request.pitch_deg,
                    request.roll_deg,
                ),
            },
            alpha_mask.as_ref(),
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
        let mut texture_bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(exported)
            .write_to(&mut texture_bytes, ImageFormat::Png)
            .map_err(|err| err.to_string())?;

        let ini = stellarium_landscape_ini(&StellariumLandscape {
            name: request.landscape_name,
            author: request.author,
            description: request.description,
            maptex: texture_filename.clone(),
            angle_rotatez_deg: -90.0,
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
}

#[tauri::command]
async fn preview_sky_removed_image(request: PreviewSkyRemovedRequest) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = ImageReader::open(&request.input)
            .map_err(|err| err.to_string())?
            .decode()
            .map_err(|err| err.to_string())?;
        let preview = preview_sky_removed(&source, request.settings, request.max_width)
            .map_err(|err| err.to_string())?;
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(preview)
            .write_to(&mut bytes, ImageFormat::Png)
            .map_err(|err| err.to_string())?;
        Ok(bytes.into_inner())
    })
    .await
    .map_err(|err| err.to_string())?
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

use std::{io::Cursor, path::PathBuf, process::Command};

use chrono::{DateTime, FixedOffset};
use image::{DynamicImage, ImageFormat, ImageReader};
use panopose_core::{
    APP_VERSION, AstronomyProvider, CelestialMarker, CelestialObject, Orientation, Project,
    SkyRemovalSettings, StarMarker,
    astronomy::{ApproximateAstronomyProvider, Observer},
    export::{
        ExportRequest, export_equirectangular_with_mask_and_progress,
        validate_equirectangular_dimensions,
    },
    sky_mask::{detect_sky_alpha_mask, preview_sky_removed},
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
            preview_sky_removed_image,
            write_panopose_metadata
        ])
        .run(tauri::generate_context!())
        .expect("error while running PanoPose");
}

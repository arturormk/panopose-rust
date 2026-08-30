use std::path::PathBuf;

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::orientation::Orientation;

pub const PROJECT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    pub name: String,
    pub sites: Vec<Site>,
    pub export_preferences: ExportSettings,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            name: name.into(),
            sites: Vec::new(),
            export_preferences: ExportSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub elevation_m: f64,
    pub timezone: String,
    pub viewpoints: Vec<Viewpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewpoint {
    pub id: String,
    pub name: String,
    pub panoramas: Vec<Panorama>,
    pub astronomy: AstronomySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Panorama {
    pub id: String,
    pub name: String,
    pub image_path: PathBuf,
    pub image_fingerprint: Option<String>,
    pub orientation: Orientation,
    pub calibration_status: CalibrationStatus,
    pub calibration_provenance: Option<String>,
    pub capture_time: Option<CaptureTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationStatus {
    Uncalibrated,
    Approximate,
    Trusted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTime {
    Exact(DateTime<FixedOffset>),
    LocalWithoutOffset {
        value: NaiveDateTime,
        timezone_hint: Option<String>,
    },
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstronomySettings {
    pub selected_time: Option<DateTime<FixedOffset>>,
    pub enabled_objects: Vec<String>,
}

impl Default for AstronomySettings {
    fn default() -> Self {
        Self {
            selected_time: None,
            enabled_objects: vec!["sun".into(), "moon".into()],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ExportSettings {
    pub center_azimuth_deg: f64,
    pub width: u32,
    pub height: u32,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            center_azimuth_deg: 180.0,
            width: 8192,
            height: 4096,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_round_trips_json() {
        let project = Project::new("round-trip");
        let json = serde_json::to_string_pretty(&project).unwrap();
        let decoded: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.schema_version, PROJECT_SCHEMA_VERSION);
        assert_eq!(decoded.name, "round-trip");
        assert_eq!(decoded.export_preferences.center_azimuth_deg, 180.0);
    }
}

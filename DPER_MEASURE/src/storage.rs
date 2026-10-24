//! Profils de calibration persistants et sessions simples en fichiers standard.
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

use crate::geometry::{Measurement, Point};
use crate::reference::ReferenceDocument;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Calibration {
    #[serde(default)]
    pub id: String,
    pub device: String,
    #[serde(default)]
    pub device_id: String,
    pub resolution: [u32; 2],
    pub optical_setup: String,
    pub mm_per_pixel: f64,
    #[serde(default)]
    pub scale_x_mm_per_pixel: Option<f64>,
    #[serde(default)]
    pub scale_y_mm_per_pixel: Option<f64>,
    #[serde(default)]
    pub expanded_uncertainty_mm: Option<f64>,
    #[serde(default)]
    pub uncertainty_budget: Option<crate::uncertainty::UncertaintyBudget>,
    pub reference_mm: f64,
    pub reference_px: f64,
    #[serde(default)]
    pub reference_x_mm: Option<f64>,
    #[serde(default)]
    pub reference_y_mm: Option<f64>,
    #[serde(default)]
    pub verification_reference_mm: Option<f64>,
    #[serde(default)]
    pub verification_residual_mm: Option<f64>,
    /// Signed length residuals for the independent control segment measured at each image corner.
    #[serde(default)]
    pub corner_verification_residuals_mm: Vec<f64>,
    #[serde(default = "default_calibration_validation_status")]
    pub validation_status: String,
    #[serde(default)]
    pub validation_points_native_px: [Point; 2],
    /// The first corner control segment is stored in `validation_points_native_px` for older profiles.
    #[serde(default)]
    pub additional_corner_validation_segments_native_px: Vec<[Point; 2]>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub reference_image: String,
    #[serde(default = "default_calibration_reference_type")]
    pub reference_type: String,
    #[serde(default)]
    pub reference_points_native_px: [Point; 2],
    #[serde(default)]
    pub secondary_reference_points_native_px: [Point; 2],
    #[serde(default = "default_calibration_origin")]
    pub origin: String,
    #[serde(default)]
    pub created_at_unix_secs: u64,
}

fn default_calibration_reference_type() -> String { "segment".into() }
fn default_calibration_origin() -> String { "interface".into() }
fn default_calibration_validation_status() -> String { "unverified".into() }

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub calibrations: Vec<Calibration>,
    #[serde(default)]
    pub protocols: Vec<MeasurementProtocol>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeasurementProtocol {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub part_family: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub device_id: String,
    pub optical_setup: String,
    pub resolution: [u32; 2],
    pub calibration_profile_id: Option<String>,
    pub mm_per_pixel: Option<f64>,
    #[serde(default)]
    pub scale_x_mm_per_pixel: Option<f64>,
    #[serde(default)]
    pub scale_y_mm_per_pixel: Option<f64>,
    pub expanded_uncertainty_mm: Option<f64>,
    pub quality_thresholds: crate::quality::ImageQualityThresholds,
    pub display_correction: crate::quality::DisplayCorrection,
    #[serde(default)]
    pub expected_measurements: Vec<ProtocolCharacteristic>,
    pub display_precision_mm: u8,
    pub tolerance_rule: ToleranceRule,
    pub created_at_unix_secs: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProtocolCharacteristic {
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub reference: Option<ProtocolReferenceDimension>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProtocolReferenceDimension {
    pub label: String,
    pub nominal_mm: f64,
    pub tolerance_minus_mm: f64,
    pub tolerance_plus_mm: f64,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToleranceRule {
    ExpandedUncertaintyGuardBand,
}

pub const CURRENT_SCHEMA_VERSION: u32 = 12;

fn legacy_schema_version() -> u32 { 1 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    #[serde(default = "legacy_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub capture_id: String,
    #[serde(default)]
    pub image_file: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub optical_setup: String,
    #[serde(default)]
    pub calibration: Option<Calibration>,
    #[serde(default)]
    pub calibrations: Vec<Calibration>,
    #[serde(default)]
    pub measurements: Vec<Measurement>,
    #[serde(default)]
    pub annotations: Vec<crate::geometry::Annotation>,
    #[serde(default)]
    pub quality_thresholds: crate::quality::ImageQualityThresholds,
    #[serde(default)]
    pub quality_assessment: Option<crate::quality::ImageQualityAssessment>,
    #[serde(default)]
    pub display_correction: crate::quality::DisplayCorrection,
    #[serde(default)]
    pub protocol: Option<MeasurementProtocol>,
    #[serde(default)]
    pub reference_document: Option<ReferenceDocument>,
    #[serde(default)]
    pub measurement_audit_history: Vec<MeasurementAuditEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeasurementAuditValue {
    pub id: u64,
    pub label: String,
    pub kind: String,
    pub result: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeasurementAuditEvent {
    pub timestamp_unix_secs: u64,
    pub action: String,
    pub measurement_ids: Vec<u64>,
    pub before: Vec<MeasurementAuditValue>,
    pub after: Vec<MeasurementAuditValue>,
}

fn config_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
        .join("DPER_Measure").join("calibrations.json")
}

fn legacy_config_path() -> PathBuf {
    // Read the previous prototype's settings once, then persist them under the new product name.
    dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
        .join("MicroCraft").join("calibrations.json")
}

pub fn load_settings() -> Settings {
    let (bytes, loaded_legacy) = match fs::read(config_path()) {
        Ok(bytes) => (Some(bytes), false),
        Err(_) => (fs::read(legacy_config_path()).ok(), true),
    };
    let parsed_settings = bytes.and_then(|b| serde_json::from_slice::<Settings>(&b).ok());
    let loaded_settings = parsed_settings.is_some();
    let mut settings = parsed_settings.unwrap_or_default();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .unwrap_or_default().as_nanos();
    let mut migrated = false;
    for (index, calibration) in settings.calibrations.iter_mut().enumerate() {
        if calibration.id.is_empty() {
            calibration.id = format!("calibration-{stamp}-{index}");
            migrated = true;
        }
    }
    if migrated || (loaded_legacy && loaded_settings) { let _ = save_settings(&settings); }
    settings
}

pub fn save_settings(s: &Settings) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let bytes = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_session_json_receives_safe_defaults_for_new_fields() {
        let session: Session = serde_json::from_str(
            r#"{"image_file":"capture.png","measurements":[]}"#,
        ).unwrap();

        assert_eq!(session.schema_version, 1);
        assert_eq!(session.image_file, "capture.png");
        assert_eq!(session.capture_id, "");
        assert!(session.measurements.is_empty());
        assert!(session.calibration.is_none());
        assert!(session.protocol.is_none());
        assert!(session.reference_document.is_none());
        assert_eq!(session.quality_thresholds, crate::quality::ImageQualityThresholds::default());
        assert_eq!(session.display_correction, crate::quality::DisplayCorrection::default());
    }
}

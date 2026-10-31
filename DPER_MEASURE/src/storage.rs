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

pub const CURRENT_SCHEMA_VERSION: u32 = 13;

fn legacy_schema_version() -> u32 { 1 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub automated_inspection: Option<crate::dper_inspection::InspectionWorkspace>,
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
        assert!(session.automated_inspection.is_none());
        assert!(session.protocol.is_none());
        assert!(session.reference_document.is_none());
        assert_eq!(session.quality_thresholds, crate::quality::ImageQualityThresholds::default());
        assert_eq!(session.display_correction, crate::quality::DisplayCorrection::default());
    }

    #[test]
    fn session_roundtrip_keeps_inspection_state_but_requires_fresh_pixel_evidence() {
        use crate::{
            dper_drawing::parse_text_plan,
            dper_inspection::{assess_record, fingerprint, inspect_feature, FrameContext, InspectionWorkspace},
            dper_metrology::{ConformityStatus, GeometryState},
            dper_registration::{register_similarity, Correspondence},
            dper_vision::{InspectionGeometry, VisionOptions},
        };

        let image = image::RgbImage::from_fn(128, 128, |x, y| {
            let inside = (x as f64 - 64.0).hypot(y as f64 - 64.0) <= 25.0;
            image::Rgb([if inside { 220 } else { 20 }; 3])
        });
        let calibration: Calibration = serde_json::from_value(serde_json::json!({
            "id": "cal-persistence", "device": "test", "resolution": [128, 128],
            "optical_setup": "test", "mm_per_pixel": 0.1, "reference_mm": 10.0,
            "reference_px": 100.0, "expanded_uncertainty_mm": 0.002
        })).unwrap();
        let frame = FrameContext {
            capture_id: "capture-persistence".into(), calibration: Some(calibration), quality_pass: true,
        };
        let pairs = [Point { x: 10.0, y: 10.0 }, Point { x: 100.0, y: 10.0 },
            Point { x: 10.0, y: 100.0 }].map(|point| Correspondence { plan: point, image: point });
        let mut plan = parse_text_plan("Ø5 ±0.1 mm", "drawing-persistence");
        plan.characteristics[0].confirmed = true;
        let feature_id = plan.characteristics[0].id.clone();
        let mut workspace = InspectionWorkspace {
            plan: Some(plan), registration: Some(register_similarity(&pairs, 1.0).unwrap()),
            registration_capture_id: Some(frame.capture_id.clone()),
            registration_fingerprint: Some(fingerprint(&image)), records: Vec::new(),
            ..Default::default()
        };
        let record = inspect_feature(&workspace, &image, &frame, &feature_id, "Groupe persistant",
            &InspectionGeometry::Circle { center: Point { x: 64.0, y: 64.0 }, radius: 25.0 },
            &VisionOptions::default()).unwrap();
        assert!(!record.imported);
        let saved_outcome = record.outcome.clone();
        workspace.records.push(record);
        let mut session: Session = serde_json::from_str(r#"{"image_file":"capture.png"}"#).unwrap();
        session.schema_version = CURRENT_SCHEMA_VERSION;
        session.capture_id = frame.capture_id.clone();
        session.calibration = frame.calibration.clone();
        session.automated_inspection = Some(workspace);

        let json = serde_json::to_value(&session).unwrap();
        assert_eq!(json["automated_inspection"]["records"][0]["group"], "Groupe persistant");
        assert_eq!(json["automated_inspection"]["records"][0]["outcome"],
            serde_json::to_value(&saved_outcome).unwrap());
        assert_eq!(json["automated_inspection"]["records"][0]["imported"], false);

        let restored: Session = serde_json::from_value(json).unwrap();
        let restored_workspace = restored.automated_inspection.as_ref().unwrap();
        let restored_record = &restored_workspace.records[0];
        assert!(restored_record.imported);
        assert_eq!(restored_record.outcome, saved_outcome);
        assert_eq!(restored_workspace.registration_capture_id, Some(frame.capture_id.clone()));
        assert_eq!(restored_workspace.plan.as_ref().unwrap().characteristics[0].id, feature_id);
        let current_outcome = assess_record(restored_record, restored_workspace, &image, &frame, &[]);
        assert_ne!(current_outcome.geometry_state, GeometryState::Validated);
        assert_eq!(current_outcome.conformity_status, ConformityStatus::Indeterminate);
        assert!(current_outcome.blocking_reasons
            .contains(&"imported_evidence_requires_reassessment".into()));
    }
}

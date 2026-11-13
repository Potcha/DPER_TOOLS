//! Application-owned attachment and replay of uncertainty calculations.
//! A replayable calculation is distinct from a qualified total uncertainty.

use crate::{
    dper_inspection::{self, FrameContext, InspectionRecord, InspectionWorkspace},
    dper_metrology::GeometryState,
    geometry::Measurement,
    mcp_server::ApplyObservationUncertaintyArgs,
    observation_uncertainty::{self, ObservationUncertaintyInput},
    DperMeasureApp,
};
use image::RgbImage;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObservationUncertaintyProof {
    pub(crate) measurement_id: u64,
    pub(crate) measurement_revision: u64,
    pub(crate) input: ObservationUncertaintyInput,
    pub(crate) calculation: Value,
    pub(crate) context: Value,
    /// Saved calculations remain historical until applied again to live evidence.
    #[serde(skip_deserializing, default)]
    pub(crate) applied_in_session: bool,
}

fn observation_snapshot(record: &InspectionRecord) -> Result<Value, String> {
    let mut value = serde_json::to_value(record).map_err(|error| error.to_string())?;
    let object = value.as_object_mut().ok_or("Observation invalide.")?;
    object.remove("outcome");
    object.remove("decision_evidence");
    object.remove("observation_uncertainty");
    Ok(value)
}

fn fingerprint<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(crate::dper_binding::fingerprint_bytes(&bytes))
}

fn native_context<'a>(
    record: &InspectionRecord,
    workspace: &InspectionWorkspace,
    image: &RgbImage,
    frame: &FrameContext,
    measurements: &'a [Measurement],
) -> Result<(&'a Measurement, Value), String> {
    let id = record.measurement_id.ok_or("Enregistrez l'observation native avant son budget d'incertitude.")?;
    if record.imported || record.clearance.is_some() {
        return Err("Un contour observé courant est requis ; preuve importée ou cercle de dégagement dérivé refusé.".into());
    }
    let mut matches = measurements.iter().filter(|measurement| measurement.id == id);
    let measurement = matches.next().ok_or("Mesure native absente.")?;
    if matches.next().is_some() {
        return Err("Identité de mesure native ambiguë.".into());
    }
    let snapshot = observation_snapshot(record)?;
    let mut records = workspace.records.iter().filter(|candidate| candidate.measurement_id == Some(id));
    let stored = records.next().ok_or("Observation absente de l'inspection active.")?;
    if records.next().is_some() || observation_snapshot(stored)? != snapshot {
        return Err("Observation native ambiguë ou modifiée.".into());
    }
    if record.measurement_revision != Some(workspace.dependencies.revision(id))
        || workspace.dependencies.revision(id) == u64::MAX
        || workspace.dependencies.is_stale(id)
        || measurement.dependency_stale || measurement.is_construction || measurement.is_numeric_resize()
        || measurement.capture_id != frame.capture_id
    {
        return Err("La mesure native ou ses dépendances ont changé ; observez de nouveau.".into());
    }
    crate::measurement_graph::validate_sources(measurements, &frame.capture_id, &measurement.source_measurement_ids)?;
    if measurement.source_measurement_ids.iter().any(|parent|
        workspace.dependencies.is_stale(*parent) || workspace.dependencies.revision(*parent) == u64::MAX)
    {
        return Err("Une source native est périmée ; recalculez ses dépendances.".into());
    }
    let outcome = dper_inspection::assess_record(record, workspace, image, frame, measurements);
    if outcome.geometry_state != GeometryState::Validated {
        return Err("L'observation native doit être vérifiée par logiciel sur les pixels et sources courants.".into());
    }
    let calibration = frame.calibration.as_ref().ok_or("Une calibration indépendante active est requise.")?;
    if calibration.resolution != [image.width(), image.height()] {
        return Err("La calibration ne correspond pas à la résolution de l'image.".into());
    }
    // Exclude display-only changes. The graph generation detects edit/undo/redo.
    let measurement_snapshot = |measurement: &Measurement| json!({
        "id": measurement.id, "kind": measurement.kind, "points": measurement.points,
        "capture_id": measurement.capture_id, "calibration_id": measurement.calibration_id,
        "mm_per_pixel": measurement.mm_per_pixel, "mm_per_pixel_x": measurement.mm_per_pixel_x,
        "mm_per_pixel_y": measurement.mm_per_pixel_y,
        "is_construction": measurement.is_construction, "drawing_variant": measurement.drawing_variant,
        "source_measurement_ids": measurement.source_measurement_ids,
        "dependency_stale": measurement.dependency_stale,
        "expanded_uncertainty_mm": measurement.expanded_uncertainty_mm,
        "protocol_id": measurement.protocol_id, "protocol_version": measurement.protocol_version,
        "origin": measurement.origin, "method": measurement.method,
        "revision": workspace.dependencies.revision(measurement.id),
    });
    let parents = measurement.source_measurement_ids.iter().map(|id| {
        let parent = measurements.iter().find(|measurement| measurement.id == *id)
            .ok_or("Source native absente.")?;
        Ok(measurement_snapshot(parent))
    }).collect::<Result<Vec<_>, String>>()?;
    Ok((measurement, json!({
        "capture_id": frame.capture_id,
        "source_fingerprint": dper_inspection::fingerprint(image),
        "dimensions": [image.width(), image.height()],
        "quality_pass": frame.quality_pass,
        "calibration": calibration,
        "native_measurement": measurement_snapshot(measurement),
        "native_sources": parents,
        "observation_fingerprint": fingerprint(&snapshot)?,
        "plan_fingerprint": fingerprint(&workspace.plan)?,
        "localization_fingerprint": fingerprint(&(
            &workspace.registration, &workspace.registration_scope,
            &workspace.registration_capture_id, &workspace.registration_fingerprint,
            &workspace.bindings, &workspace.views, &workspace.occurrence_assignments,
        ))?,
    })))
}

pub(crate) fn validate_current(
    record: &InspectionRecord,
    workspace: &InspectionWorkspace,
    image: &RgbImage,
    frame: &FrameContext,
    measurements: &[Measurement],
) -> Result<(), String> {
    let proof = record.observation_uncertainty.as_ref().ok_or("Budget d'incertitude observée absent.")?;
    if !proof.applied_in_session {
        return Err("Budget historique : appliquez de nouveau le calcul à une observation courante.".into());
    }
    let (measurement, context) = native_context(record, workspace, image, frame, measurements)?;
    if proof.measurement_id != measurement.id
        || proof.measurement_revision != workspace.dependencies.revision(measurement.id)
        || proof.context != context
    {
        return Err("Le contexte ou les sources du budget d'incertitude ont changé.".into());
    }
    let calculation = observation_uncertainty::calculate(measurement,
        frame.calibration.as_ref().ok_or("Calibration active absente.")?, &proof.input)?;
    if serde_json::to_value(calculation).map_err(|error| error.to_string())? != proof.calculation {
        return Err("Le calcul d'incertitude enregistré ne correspond plus à son rejeu.".into());
    }
    Ok(())
}

impl DperMeasureApp {
    pub(crate) fn apply_observation_uncertainty(
        &mut self,
        args: ApplyObservationUncertaintyArgs,
    ) -> Result<Value, String> {
        self.inspection_guard(&args.capture_id, args.expected_revision)?;
        self.validate_dependency_sources(&args.capture_id, &[args.measurement_id])?;
        let image = self.inspection_image()?;
        let frame = self.inspection_frame();
        let mut current = self.current_inspection();
        // Live catalogues are transient application data, never restored from JSON.
        if self.current_drawing_source().is_ok() {
            current.current_drawing_catalogs = self.drawing_geometry_cache.clone();
        } else {
            current.current_drawing_catalogs.clear();
        }
        let mut matches = current.records.iter().enumerate().filter(|(_, record)|
            record.measurement_id == Some(args.measurement_id));
        let (index, record) = matches.next().ok_or("Aucune observation enregistrée pour cette mesure.")?;
        if matches.next().is_some() || record.outcome.geometry_state != GeometryState::Validated {
            return Err("Une observation native unique, courante et vérifiée par logiciel est requise.".into());
        }
        let (measurement, context) = native_context(record, &current, &image, &frame, &self.measurements)?;
        let calculation = observation_uncertainty::calculate(measurement,
            frame.calibration.as_ref().ok_or("Une calibration indépendante active est requise.")?, &args.input)?;
        let calculation = serde_json::to_value(calculation).map_err(|error| error.to_string())?;
        let proof = ObservationUncertaintyProof {
            measurement_id: args.measurement_id,
            measurement_revision: current.dependencies.revision(args.measurement_id),
            input: args.input, calculation: calculation.clone(), context,
            applied_in_session: true,
        };
        self.inspection.records[index].observation_uncertainty = Some(proof);
        self.inspection_panel_cache = None;
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        Ok(json!({
            "document_revision": self.document_revision,
            "measurement_id": args.measurement_id,
            "calculation": calculation,
            "attached_to_observation": true,
            "applied_to_measurement": false,
            "source_qualification_verified": false,
            "conformity_evaluated": false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dper_registration::{Correspondence, RegistrationReport, SimilarityTransform},
        dper_vision::{InspectionGeometry, VisionOptions}, geometry::{Point, Tool},
        mcp_server::InspectFeatureArgs, observation_uncertainty::{PropagationParameters, ScaleUncertainty}};
    use image::Rgb;

    fn fixture() -> (DperMeasureApp, u64) {
        let image = RgbImage::from_fn(128, 128, |x, y| {
            let distance = (x as f64 - 64.0).hypot(y as f64 - 64.0);
            Rgb([(20.0 + 200.0 * (distance - 24.5).clamp(0.0, 1.0)) as u8; 3])
        });
        let mut app = DperMeasureApp::new_with_services(eframe::egui::Context::default(), false);
        app.image = Some(crate::ImageData { width: 128, height: 128,
            rgb: image.as_raw().clone(), source: "uncertainty-synthetic.png".into() });
        let mut quality = crate::quality::assess_image_quality(image.as_raw(), 128, 128,
            &crate::quality::ImageQualityThresholds::default()).unwrap();
        quality.is_exploitable = true;
        app.quality_assessment = Some(quality);
        app.calibration = Some(serde_json::from_value(json!({
            "id": "cal", "device": "synthetic", "resolution": [128,128],
            "optical_setup": "test", "mm_per_pixel": 0.1,
            "reference_mm": 10.0, "reference_px": 100.0,
            "origin": "demonstration", "validation_status": "unverified",
        })).unwrap());
        app.mm_per_pixel = Some(0.1);
        let mut plan = crate::dper_drawing::parse_text_plan("Ø5 ±0.2 mm", "drawing");
        plan.characteristics[0].confirmed = true;
        let feature_id = plan.characteristics[0].id.clone();
        app.inspection.plan = Some(plan);
        app.inspection.registration = Some(RegistrationReport {
            transform: SimilarityTransform { scale: 1.0, rotation_rad: 0.0, translation: Point::default() },
            inlier_indices: vec![0, 1, 2],
            correspondences: [Point { x: 10.0, y: 10.0 }, Point { x: 20.0, y: 10.0 }, Point { x: 10.0, y: 20.0 }]
                .map(|point| Correspondence { plan: point, image: point }).to_vec(),
            residuals_px: vec![0.0; 3], threshold_px: Some(1.0), rms_error_px: 0.0, max_error_px: 0.0,
        });
        app.inspection.registration_capture_id = Some(app.capture_id.clone());
        app.inspection.registration_fingerprint = Some(dper_inspection::fingerprint(&image));
        app.perform_inspection(InspectFeatureArgs {
            capture_id: app.capture_id.clone(), expected_revision: app.document_revision,
            feature_id, group: "part".into(),
            geometry_in_drawing: InspectionGeometry::Circle { center: Point { x: 64.0, y: 64.0 }, radius: 25.0 },
            source_measurement_ids: Vec::new(), options: Some(VisionOptions::default()), commit: true,
        }, None, None).unwrap();
        let id = app.measurements.last().unwrap().id;
        assert_eq!(app.measurements.last().unwrap().kind, Tool::Circle);
        (app, id)
    }

    fn args(app: &DperMeasureApp, measurement_id: u64) -> ApplyObservationUncertaintyArgs {
        ApplyObservationUncertaintyArgs { capture_id: app.capture_id.clone(), expected_revision: app.document_revision,
            measurement_id, input: ObservationUncertaintyInput::Diameter {
                standard_uncertainty_radius_px: 0.2, radius_source: "Protocole synthétique".into(),
                scale: ScaleUncertainty { relative_standard_uncertainty: 0.01, source: "Échelle synthétique".into() },
                propagation: PropagationParameters { coverage_factor: 2.0,
                    correlation_matrix: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
                    correlation_justification: "Indépendance synthétique".into() },
            } }
    }

    fn validate(app: &DperMeasureApp) -> Result<(), String> {
        validate_current(app.inspection.records.last().unwrap(), &app.inspection,
            &app.inspection_image()?, &app.inspection_frame(), &app.measurements)
    }

    #[test]
    fn dpm15_application_attaches_replayable_budget_without_qualification_or_native_u() {
        let (mut app, id) = fixture();
        let native_u = app.measurements[0].expanded_uncertainty_mm;
        let before = app.current_inspection().records.last().unwrap().outcome.clone();
        app.apply_observation_uncertainty(args(&app, id)).unwrap();
        assert!(validate(&app).is_ok());
        assert_eq!(app.measurements[0].expanded_uncertainty_mm, native_u);
        let after = app.current_inspection().records.last().unwrap().outcome.clone();
        assert_eq!(after.conformity_status, before.conformity_status);
        assert_eq!(after.blocking_reasons, before.blocking_reasons);
        let serialized = serde_json::to_value(&app.inspection).unwrap();
        let imported: InspectionWorkspace = serde_json::from_value(serialized).unwrap();
        assert!(!imported.records.last().unwrap().observation_uncertainty.as_ref().unwrap().applied_in_session);
        assert!(validate_current(imported.records.last().unwrap(), &imported,
            &app.inspection_image().unwrap(), &app.inspection_frame(), &app.measurements).is_err());
    }

    #[test]
    fn dpm15_application_refuses_stale_revision_import_and_ambiguous_observation() {
        let (mut app, id) = fixture();
        let mut stale = args(&app, id);
        stale.expected_revision = stale.expected_revision.wrapping_sub(1);
        assert!(app.apply_observation_uncertainty(stale).is_err());
        app.inspection.records[0].imported = true;
        assert!(app.apply_observation_uncertainty(args(&app, id)).is_err());
        app.inspection.records[0].imported = false;
        app.inspection.records.push(app.inspection.records[0].clone());
        assert!(app.apply_observation_uncertainty(args(&app, id)).is_err());
    }

    #[test]
    fn dpm15_replay_detects_pixel_calibration_geometry_revision_and_budget_changes() {
        let (mut app, id) = fixture();
        app.apply_observation_uncertainty(args(&app, id)).unwrap();
        assert!(validate(&app).is_ok());
        let mut record = app.inspection.records[0].clone();
        record.observation_uncertainty.as_mut().unwrap().calculation["expanded_uncertainty"] = json!(123.0);
        assert!(validate_current(&record, &app.inspection, &app.inspection_image().unwrap(),
            &app.inspection_frame(), &app.measurements).is_err());
        let mut pixels = app.inspection_image().unwrap();
        pixels.put_pixel(0, 0, Rgb([255; 3]));
        assert!(validate_current(&app.inspection.records[0], &app.inspection, &pixels,
            &app.inspection_frame(), &app.measurements).is_err());
        let mut frame = app.inspection_frame();
        frame.calibration.as_mut().unwrap().optical_setup = "other".into();
        assert!(validate_current(&app.inspection.records[0], &app.inspection, &app.inspection_image().unwrap(),
            &frame, &app.measurements).is_err());
        app.measurements[0].points[0].x += 1.0;
        assert!(validate(&app).is_err());
        app.measurements[0].points[0].x -= 1.0;
        app.inspection.dependencies.invalidate(&app.measurements, &[id]);
        assert!(validate(&app).is_err());
    }
}

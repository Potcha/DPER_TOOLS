//! Replay-gated native pixel diagnostics and their construction dependencies.

use crate::{
    clearance_inspection::{input_context, measurement_snapshot},
    dper_inspection::{self, FrameContext, InspectionRecord, InspectionWorkspace},
    dper_metrology::{ConformityStatus, GeometryState, ValidationOutcome},
    geometry::Measurement,
    mcp_server::InspectNativeDiagnosticArgs,
    native_diagnostic::{self, NativeDiagnosticObservation, NativeDiagnosticProof},
    DperMeasureApp,
};
use image::RgbImage;
use serde_json::{json, Value};

fn value_mm(observed: &NativeDiagnosticObservation, scale: Option<f64>) -> Result<Option<f64>, String> {
    let value = observed.length_px.zip(scale).map(|(length, scale)| length * scale);
    if value.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        return Err("La conversion de la longueur est invalide.".into());
    }
    Ok(value)
}

impl DperMeasureApp {
    pub(crate) fn perform_native_diagnostic(&mut self, args: InspectNativeDiagnosticArgs) -> Result<Value, String> {
        self.inspection_guard(&args.capture_id, args.expected_revision)?;
        if args.feature_id.trim().is_empty() || args.feature_id.chars().count() > 200
            || args.group.trim().is_empty() || args.group.chars().count() > 200
            || args.source_measurement_ids.len() > 64
        {
            return Err("Identifiant et groupe requis (200 caractères maximum), avec au plus 64 sources.".into());
        }
        self.validate_dependency_sources(&args.capture_id, &args.source_measurement_ids)?;
        let image = self.inspection_image()?;
        let frame = self.inspection_frame();
        if !frame.quality_pass { return Err("La qualité de l'image ne permet pas l'inspection.".into()); }
        let scale = dper_inspection::isotropic_scale(frame.calibration.as_ref(), &image)?;
        if let Some(profile) = &frame.calibration {
            if self.calibration_history.iter().find(|candidate| candidate.id == profile.id)
                .is_some_and(|stored| stored.mm_per_pixel != profile.mm_per_pixel
                    || stored.scale_x_mm_per_pixel != profile.scale_x_mm_per_pixel
                    || stored.scale_y_mm_per_pixel != profile.scale_y_mm_per_pixel)
            {
                return Err("Le profil actif et son historique d'étalonnage divergent ; réactivez le profil.".into());
            }
        }
        let context = input_context(&self.inspection, &image, &frame, &self.measurements, &args.source_measurement_ids)?;
        let observed = native_diagnostic::observe(&image, &args.request)?;
        let value = value_mm(&observed, scale)?;
        let mut record = InspectionRecord {
            feature_id: args.feature_id.clone(), drawing_binding_id: None, occurrence: None,
            group: args.group.clone(), capture_id: frame.capture_id.clone(),
            source_fingerprint: dper_inspection::fingerprint(&image),
            calibration_id: frame.calibration.as_ref().map(|profile| profile.id.clone()),
            calibration_fingerprint: dper_inspection::calibration_fingerprint(frame.calibration.as_ref()),
            result: observed.presentation.clone(), clearance: None, observation_uncertainty: None,
            diagnostic: Some(NativeDiagnosticProof { request: args.request.clone(), result: observed.result.clone(),
                sources: args.source_measurement_ids.clone(), context: json!({"inputs": context, "constructions": []}),
                preview_revision: args.expected_revision.wrapping_add(1) }),
            vision_options: None, value_mm: value, decision_evidence: None,
            outcome: ValidationOutcome { geometry_state: GeometryState::Measured,
                conformity_status: ConformityStatus::Indeterminate, blocking_reasons: Vec::new() },
            measurement_id: None, measurement_revision: None, measurement_points: observed.points.clone(), imported: false,
        };
        if args.commit {
            let mut previews = self.inspection.records.iter().filter(|candidate|
                !candidate.imported && candidate.measurement_id.is_none()
                    && candidate.feature_id == args.feature_id && candidate.group == args.group
                    && candidate.diagnostic.as_ref().is_some_and(|proof| proof.preview_revision == args.expected_revision));
            let preview = previews.next().ok_or("Calculez d'abord un aperçu identique dans le contexte courant.")?;
            if previews.next().is_some() { return Err("Aperçu ambigu ; recalculez le diagnostic.".into()); }
            let proof = preview.diagnostic.as_ref().unwrap();
            let current = record.diagnostic.as_ref().unwrap();
            if proof.sources != current.sources || proof.context != current.context
                || proof.result != current.result
                || serde_json::to_value(&proof.request).ok() != serde_json::to_value(&current.request).ok()
                || preview.capture_id != record.capture_id || preview.source_fingerprint != record.source_fingerprint
                || preview.calibration_id != record.calibration_id || preview.calibration_fingerprint != record.calibration_fingerprint
                || preview.measurement_points != record.measurement_points || preview.value_mm != record.value_mm
                || serde_json::to_value(&preview.result).ok() != serde_json::to_value(&record.result).ok()
                || preview.clearance.is_some() || preview.drawing_binding_id.is_some() || preview.occurrence.is_some()
                || preview.vision_options.is_some() || preview.decision_evidence.is_some()
            {
                return Err("La demande, ses sources ou son contexte ont changé ; recalculez l'aperçu.".into());
            }
            record.diagnostic.as_mut().unwrap().preview_revision = proof.preview_revision;
            let mut parents = args.source_measurement_ids.clone();
            let mut constructions = Vec::new();
            for construction in &observed.constructions {
                let native = self.add_measurement_with_construction(construction.kind, construction.points.clone(),
                    Some(format!("{} · {} · {}", args.group, args.feature_id, construction.role)),
                    frame.capture_id.clone(), record.calibration_id.clone(), scale,
                    "construction".into(), Some("Support observé ; étendue physique non certifiée.".into()),
                    true, args.source_measurement_ids.clone());
                parents.push(native.id);
                constructions.push(json!({"role": construction.role, "snapshot": measurement_snapshot(&native, &self.inspection)}));
            }
            record.diagnostic.as_mut().unwrap().context["constructions"] = Value::Array(constructions);
            let native = self.add_measurement_with_construction(observed.kind, observed.points,
                Some(format!("{} · {}", args.group, args.feature_id)),
                frame.capture_id.clone(), record.calibration_id.clone(), scale,
                "vision".into(), Some("Diagnostic pixel local ; qualification physique et incertitude non établies.".into()),
                observed.is_construction, parents);
            record.measurement_id = Some(native.id);
            record.measurement_revision = Some(self.inspection.dependencies.revision(native.id));
            record.diagnostic.as_mut().unwrap().context["native"] = measurement_snapshot(&native, &self.inspection);
        }
        record.outcome = assess_native_diagnostic(&record, &self.inspection, &image, &frame, &self.measurements);
        self.inspection.records.push(record.clone());
        if self.inspection.records.len() > 1000 { self.inspection.records.remove(0); }
        self.inspection_panel_cache = None;
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        Ok(json!({"document_revision": self.document_revision, "record": record}))
    }
}

/// Only application-owned replay and current construction snapshots confer
/// software validation. Stored flags never confer physical conformity.
pub(crate) fn assess_native_diagnostic(
    record: &InspectionRecord, workspace: &InspectionWorkspace, image: &RgbImage,
    frame: &FrameContext, measurements: &[Measurement],
) -> ValidationOutcome {
    let mut reasons = Vec::<String>::new();
    let mut block = |reason: &str| { if !reasons.iter().any(|existing| existing == reason) { reasons.push(reason.into()); } };
    let Some(proof) = &record.diagnostic else {
        return ValidationOutcome { geometry_state: GeometryState::Proposed,
            conformity_status: ConformityStatus::Indeterminate,
            blocking_reasons: vec!["native_diagnostic_proof_missing".into(), "native_diagnostic_physical_conformity_unqualified".into()] };
    };
    if record.imported { block("imported_evidence_requires_reassessment"); }
    if record.capture_id != frame.capture_id { block("capture_mismatch"); }
    if record.source_fingerprint != dper_inspection::fingerprint(image) { block("source_pixels_changed"); }
    if !frame.quality_pass { block("image_quality_failed"); }
    if record.calibration_id != frame.calibration.as_ref().map(|profile| profile.id.clone())
        || record.calibration_fingerprint != dper_inspection::calibration_fingerprint(frame.calibration.as_ref())
    { block("calibration_changed"); }
    let scale = dper_inspection::isotropic_scale(frame.calibration.as_ref(), image);
    if scale.is_err() { block("calibration_changed"); }
    let effective_scale = scale.ok().flatten();
    match input_context(workspace, image, frame, measurements, &proof.sources) {
        Ok(current) if proof.context.get("inputs") == Some(&current) => (),
        _ => block("measurement_dependencies_changed"),
    }
    let observed = native_diagnostic::observe(image, &proof.request);
    let mut expected_parents = proof.sources.clone();
    if let Ok(observed) = &observed {
        if proof.result != observed.result { block("native_diagnostic_observation_changed"); }
        if record.measurement_points != observed.points
            || serde_json::to_value(&record.result).ok() != serde_json::to_value(&observed.presentation).ok()
            || record.vision_options.is_some() || record.clearance.is_some()
            || record.drawing_binding_id.is_some() || record.occurrence.is_some() || record.decision_evidence.is_some()
        { block("inspection_points_changed"); }
        if value_mm(observed, effective_scale).ok() != Some(record.value_mm) { block("physical_value_changed"); }
        if let Some(stored) = proof.context.get("constructions").and_then(Value::as_array) {
            if record.measurement_id.is_some() && stored.len() != observed.constructions.len() {
                block("measurement_dependencies_changed");
            }
            for (stored, expected) in stored.iter().zip(&observed.constructions) {
                let id = stored.pointer("/snapshot/id").and_then(Value::as_u64);
                let mut candidates = measurements.iter().filter(|candidate| Some(candidate.id) == id);
                let candidate = candidates.next();
                if candidates.next().is_some() || stored.get("role").and_then(Value::as_str) != Some(expected.role)
                    || !candidate.is_some_and(|candidate| candidate.kind == expected.kind
                        && candidate.points == expected.points && candidate.is_construction
                        && candidate.capture_id == frame.capture_id && candidate.calibration_id == record.calibration_id
                        && candidate.source_measurement_ids == proof.sources && candidate.origin == "construction"
                        && !candidate.dependency_stale && !workspace.dependencies.is_stale(candidate.id)
                        && workspace.dependencies.revision(candidate.id) != u64::MAX
                        && stored.get("snapshot") == Some(&measurement_snapshot(candidate, workspace)))
                { block("measurement_dependencies_changed"); }
                if let Some(id) = id { expected_parents.push(id); }
            }
        } else { block("measurement_dependencies_changed"); }
    } else { block("native_diagnostic_observation_changed"); }
    let mut native_matches = measurements.iter().filter(|candidate| Some(candidate.id) == record.measurement_id);
    match native_matches.next() {
        None => block("measurement_missing"),
        Some(native) => {
            if native_matches.next().is_some() { block("measurement_identity_ambiguous"); }
            if record.measurement_revision != Some(workspace.dependencies.revision(native.id))
                || workspace.dependencies.revision(native.id) == u64::MAX
            { block("measurement_revision_changed"); }
            if native.dependency_stale || workspace.dependencies.is_stale(native.id)
                || native.source_measurement_ids != expected_parents
            { block("measurement_dependencies_changed"); }
            if !observed.as_ref().is_ok_and(|observed| native.kind == observed.kind
                && native.is_construction == observed.is_construction && native.points == observed.points)
                || native.capture_id != record.capture_id || native.calibration_id != record.calibration_id
                || native.origin != "vision"
                || proof.context.get("native") != Some(&measurement_snapshot(native, workspace))
            { block("measurement_geometry_changed"); }
            let expected_x = frame.calibration.as_ref().map(|profile| profile.scale_x_mm_per_pixel.unwrap_or(profile.mm_per_pixel));
            let expected_y = frame.calibration.as_ref().map(|profile| profile.scale_y_mm_per_pixel.unwrap_or(profile.mm_per_pixel));
            if native.mm_per_pixel != effective_scale || native.mm_per_pixel_x != expected_x
                || native.mm_per_pixel_y != expected_y || native.expanded_uncertainty_mm.is_some()
            { block("measurement_calibration_changed"); }
        }
    }
    let geometry_state = if reasons.is_empty() { GeometryState::Validated } else { GeometryState::Measured };
    reasons.push("native_diagnostic_physical_conformity_unqualified".into());
    ValidationOutcome { geometry_state, conformity_status: ConformityStatus::Indeterminate, blocking_reasons: reasons }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dper_vision::{LineSearch, StraightEdgeSearch}, geometry::{Point, Tool}, native_diagnostic::NativeDiagnosticRequest};

    fn fixture() -> (DperMeasureApp, InspectNativeDiagnosticArgs) {
        let image = RgbImage::from_fn(256, 256, |x, y| {
            let along = x as f64 - 67.5;
            let across = y as f64 - 126.5;
            let intensity = 30.0 + 190.0 * (0.5 + 0.5 * (along.min(112.0 - along).min(across) / 0.65).tanh());
            image::Rgb([intensity.round() as u8; 3])
        });
        let point = |x: f64, y: f64| Point { x: 67.5 + x, y: 126.5 + y };
        let line = |start, end| LineSearch { start, end, options: Default::default() };
        let request = NativeDiagnosticRequest::StraightEdgeEndpoints { search: StraightEdgeSearch {
            edge: line(point(3.0, 1.3), point(109.0, 1.3)),
            start_boundary: line(point(-1.2, 3.0), point(-1.2, 38.0)),
            end_boundary: line(point(113.2, 3.0), point(113.2, 38.0)),
        } };
        let mut app = DperMeasureApp::new_with_services(eframe::egui::Context::default(), false);
        app.image = Some(crate::ImageData { width: 256, height: 256, rgb: image.as_raw().clone(), source: "native-diagnostic-synthetic.png".into() });
        let mut quality = crate::quality::assess_image_quality(image.as_raw(), 256, 256,
            &crate::quality::ImageQualityThresholds::default()).unwrap();
        quality.is_exploitable = true;
        app.quality_assessment = Some(quality);
        let source = app.add_measurement_with_construction(Tool::Coordinate, vec![point(20.0, 20.0)],
            Some("Source construction".into()), app.capture_id.clone(), None, None,
            "interface".into(), None, true, Vec::new());
        let args = InspectNativeDiagnosticArgs { capture_id: app.capture_id.clone(), expected_revision: app.document_revision,
            feature_id: "visible-edge".into(), group: "part-A".into(), request,
            source_measurement_ids: vec![source.id], commit: false };
        (app, args)
    }

    fn commit(app: &mut DperMeasureApp, args: &mut InspectNativeDiagnosticArgs) -> InspectionRecord {
        args.commit = false;
        args.expected_revision = app.document_revision;
        app.perform_native_diagnostic(args.clone()).unwrap();
        args.commit = true;
        args.expected_revision = app.document_revision;
        app.perform_native_diagnostic(args.clone()).unwrap();
        app.inspection.records.last().unwrap().clone()
    }

    fn assess(app: &DperMeasureApp, record: &InspectionRecord) -> ValidationOutcome {
        assess_native_diagnostic(record, &app.inspection, &app.inspection_image().unwrap(),
            &app.inspection_frame(), &app.measurements)
    }

    #[test]
    fn native_commit_requires_unchanged_preview_request_group_sources_and_revision() {
        let (mut app, mut args) = fixture();
        args.commit = true;
        let initial_count = app.measurements.len();
        assert!(app.perform_native_diagnostic(args.clone()).is_err());
        assert_eq!(app.measurements.len(), initial_count);
        args.commit = false;
        app.perform_native_diagnostic(args.clone()).unwrap();
        args.commit = true;
        assert!(app.perform_native_diagnostic(args.clone()).is_err());
        args.expected_revision = app.document_revision;
        let mut changed = args.clone();
        changed.group = "other-part".into();
        assert!(app.perform_native_diagnostic(changed).is_err());
        let mut changed = args.clone();
        changed.source_measurement_ids.clear();
        assert!(app.perform_native_diagnostic(changed).is_err());
        let mut changed = args.clone();
        let NativeDiagnosticRequest::StraightEdgeEndpoints { search } = &mut changed.request else { panic!() };
        search.edge.options.minimum_gradient += 0.1;
        assert!(app.perform_native_diagnostic(changed).is_err());
        assert_eq!(app.measurements.len(), initial_count);
        app.perform_native_diagnostic(args).unwrap();
        assert_eq!(app.measurements.len(), initial_count + 4);
        let record = app.inspection.records.last().unwrap();
        assert!(matches!(record.outcome.geometry_state, GeometryState::Validated));
        assert!(matches!(record.outcome.conformity_status, ConformityStatus::Indeterminate));
        let native = app.measurements.iter().find(|item| Some(item.id) == record.measurement_id).unwrap();
        assert_eq!(native.kind, Tool::Distance);
        assert_eq!(native.source_measurement_ids.len(), 4);
        assert!(!native.is_construction);
        assert!(record.value_mm.is_none());
    }

    #[test]
    fn native_replay_rejects_changed_sources_constructions_points_pixels_and_imported_claims() {
        let (mut app, mut args) = fixture();
        let record = commit(&mut app, &mut args);
        assert!(matches!(assess(&app, &record).geometry_state, GeometryState::Validated));
        let mut tampered = record.clone();
        tampered.measurement_points[0].x += 1.0;
        assert!(assess(&app, &tampered).blocking_reasons.iter().any(|reason| reason == "inspection_points_changed"));
        let mut tampered = record.clone();
        tampered.diagnostic.as_mut().unwrap().result["length_px"] = json!(12345.0);
        assert!(assess(&app, &tampered).blocking_reasons.iter().any(|reason| reason == "native_diagnostic_observation_changed"));
        let imported: InspectionRecord = serde_json::from_value(serde_json::to_value(&record).unwrap()).unwrap();
        assert!(assess(&app, &imported).blocking_reasons.iter().any(|reason| reason == "imported_evidence_requires_reassessment"));
        let construction_id = record.diagnostic.as_ref().unwrap().context["constructions"][0]["snapshot"]["id"].as_u64().unwrap();
        let index = app.measurements.iter().position(|item| item.id == construction_id).unwrap();
        let original = app.measurements[index].points.clone();
        app.measurements[index].points[0].x += 0.25;
        assert!(assess(&app, &record).blocking_reasons.iter().any(|reason| reason == "measurement_dependencies_changed"));
        app.measurements[index].points = original;
        let source_index = app.measurements.iter().position(|item| item.id == args.source_measurement_ids[0]).unwrap();
        app.measurements[source_index].points[0].y += 0.25;
        assert!(assess(&app, &record).blocking_reasons.iter().any(|reason| reason == "measurement_dependencies_changed"));
        app.image.as_mut().unwrap().rgb[0] ^= 1;
        assert!(assess(&app, &record).blocking_reasons.iter().any(|reason| reason == "source_pixels_changed"));
    }

    #[test]
    fn native_preview_context_detects_unrevisioned_source_edits() {
        let (mut app, mut args) = fixture();
        app.perform_native_diagnostic(args.clone()).unwrap();
        let source = app.measurements.iter_mut().find(|item| item.id == args.source_measurement_ids[0]).unwrap();
        source.points[0].x += 0.25;
        args.commit = true;
        args.expected_revision = app.document_revision;
        let count = app.measurements.len();
        assert!(app.perform_native_diagnostic(args).is_err());
        assert_eq!(app.measurements.len(), count);
    }
}

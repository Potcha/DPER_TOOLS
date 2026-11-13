//! Application-owned clearance previews, native constructions and replay gates.
//! The derived circle is presentation of a fixed-center minimum, never evidence
//! of an observed circular contour or a physical conformity decision.

use crate::{
    dper_clearance::{self, ClearanceReferenceEvidence, ClearanceResult},
    dper_inspection::{self, ClearanceProof, FrameContext, InspectionRecord, InspectionWorkspace},
    dper_metrology::{ConformityStatus, GeometryState, ValidationOutcome},
    dper_vision::{InspectionGeometry, InspectionResult, VisionEvidence},
    geometry::{Measurement, Point, Tool},
    mcp_server::InspectClearanceArgs,
    DperMeasureApp,
};
use image::RgbImage;
use serde_json::{json, Value};

pub(crate) fn measurement_snapshot(measurement: &Measurement, workspace: &InspectionWorkspace) -> Value {
    // Display-only edits do not change a source. Revisions cover edit/undo/redo
    // and snapshots also detect changes outside the normal edit handlers.
    json!({
        "id": measurement.id, "kind": measurement.kind, "points": measurement.points,
        "capture_id": measurement.capture_id, "calibration_id": measurement.calibration_id,
        "mm_per_pixel": measurement.mm_per_pixel, "mm_per_pixel_x": measurement.mm_per_pixel_x,
        "mm_per_pixel_y": measurement.mm_per_pixel_y,
        "is_construction": measurement.is_construction, "drawing_variant": measurement.drawing_variant,
        "source_measurement_ids": measurement.source_measurement_ids,
        "dependency_stale": measurement.dependency_stale,
        "expanded_uncertainty_mm": measurement.expanded_uncertainty_mm,
        "protocol_id": measurement.protocol_id, "protocol_version": measurement.protocol_version,
        "origin": measurement.origin,
        "revision": workspace.dependencies.revision(measurement.id),
        "graph_stale": workspace.dependencies.is_stale(measurement.id),
    })
}

pub(crate) fn input_context(
    workspace: &InspectionWorkspace, image: &RgbImage, frame: &FrameContext,
    measurements: &[Measurement], sources: &[u64],
) -> Result<Value, String> {
    crate::measurement_graph::validate_sources(measurements, &frame.capture_id, sources)?;
    let mut snapshots = Vec::with_capacity(sources.len());
    for id in sources {
        let measurement = measurements.iter().find(|measurement| measurement.id == *id)
            .ok_or("Une source de construction est absente.")?;
        if workspace.dependencies.is_stale(*id) || workspace.dependencies.revision(*id) == u64::MAX {
            return Err("Une source de construction doit être recalculée.".into());
        }
        snapshots.push(measurement_snapshot(measurement, workspace));
    }
    Ok(json!({ "capture_id": frame.capture_id,
        "source_fingerprint": dper_inspection::fingerprint(image),
        "dimensions": [image.width(), image.height()],
        "calibration": frame.calibration, "quality_pass": frame.quality_pass,
        "sources": snapshots }))
}

fn circle_points(result: &ClearanceResult) -> Vec<Point> {
    vec![result.center, Point { x: result.center.x + result.radius_px, y: result.center.y }]
}

fn circle_inside_image(result: &ClearanceResult, image: &RgbImage) -> bool {
    let radius = result.radius_px;
    radius.is_finite() && radius > 0.0
        && result.center.x - radius >= 0.0 && result.center.y - radius >= 0.0
        && result.center.x + radius <= image.width().saturating_sub(1) as f64
        && result.center.y + radius <= image.height().saturating_sub(1) as f64
}

fn circle_presentation(result: &ClearanceResult) -> InspectionResult {
    InspectionResult {
        geometry: InspectionGeometry::Circle { center: result.center, radius: result.radius_px },
        // No samples belong to a circle: independent line support evidence is
        // retained exclusively in ClearanceProof. Never synthesize circle data.
        evidence: VisionEvidence {
            accepted_points: Vec::new(), rejected_points: Vec::new(), sample_count: 0,
            rms_residual_px: 0.0, max_residual_px: 0.0, coverage_fraction: 0.0,
            endpoint_extent_verified: false,
        },
    }
}

fn construction_geometries(result: &ClearanceResult) -> Vec<(String, Tool, Vec<Point>)> {
    let mut geometries = Vec::new();
    match &result.reference {
        ClearanceReferenceEvidence::ObservedRectangle { corners, .. } => {
            geometries.push(("reference_rectangle".into(), Tool::Polygon, corners.to_vec()));
            geometries.push(("reference_diagonal_1".into(), Tool::Distance, vec![corners[0], corners[2]]));
            geometries.push(("reference_diagonal_2".into(), Tool::Distance, vec![corners[1], corners[3]]));
        }
        ClearanceReferenceEvidence::SuppliedCenter { center } => {
            geometries.push(("supplied_center".into(), Tool::Coordinate, vec![*center]));
        }
    }
    for stop in &result.stops {
        if let InspectionGeometry::Line { start, end } = stop.observed_geometry {
            geometries.push((format!("stop:{}", stop.id), Tool::Distance, vec![start, end]));
        }
    }
    geometries
}

impl DperMeasureApp {
    pub(crate) fn perform_clearance(&mut self, args: InspectClearanceArgs) -> Result<Value, String> {
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
        // The native constructor resolves axis scales from calibration history.
        // Refuse inconsistent same-ID history before creating any geometry.
        if let Some(profile) = &frame.calibration {
            if self.calibration_history.iter().find(|candidate| candidate.id == profile.id)
                .is_some_and(|stored| stored.mm_per_pixel != profile.mm_per_pixel
                    || stored.scale_x_mm_per_pixel != profile.scale_x_mm_per_pixel
                    || stored.scale_y_mm_per_pixel != profile.scale_y_mm_per_pixel)
            {
                return Err("Le profil actif et son historique d'étalonnage divergent ; réactivez le profil avant le contrôle.".into());
            }
        }
        let context = input_context(&self.inspection, &image, &frame, &self.measurements, &args.source_measurement_ids)?;
        let result = dper_clearance::inspect_clearance(&image, &args.request)?;
        if !circle_inside_image(&result, &image) {
            return Err("Le cercle dérivé dépasse l'image source ; le dégagement complet ne peut pas être présenté dans ce champ.".into());
        }
        let value_mm = scale.map(|scale| result.diameter_px * scale);
        if value_mm.is_some_and(|value| !value.is_finite() || value <= 0.0) {
            return Err("La conversion du diamètre donne une valeur physique non finie ou invalide.".into());
        }
        let proof = ClearanceProof {
            request: args.request.clone(), result: result.clone(),
            sources: args.source_measurement_ids.clone(),
            context: json!({"inputs": context, "constructions": []}),
            preview_revision: self.document_revision.wrapping_add(1),
        };
        let mut record = InspectionRecord {
            feature_id: args.feature_id.clone(), drawing_binding_id: None, occurrence: None, group: args.group.clone(),
            capture_id: frame.capture_id.clone(), source_fingerprint: dper_inspection::fingerprint(&image),
            calibration_id: frame.calibration.as_ref().map(|profile| profile.id.clone()),
            calibration_fingerprint: dper_inspection::calibration_fingerprint(frame.calibration.as_ref()),
            result: circle_presentation(&result), clearance: Some(proof), diagnostic: None, observation_uncertainty: None, vision_options: None,
            value_mm, decision_evidence: None,
            outcome: ValidationOutcome { geometry_state: GeometryState::Measured,
                conformity_status: ConformityStatus::Indeterminate, blocking_reasons: Vec::new() },
            measurement_id: None, measurement_revision: None,
            measurement_points: circle_points(&result), imported: false,
        };
        if args.commit {
            let mut matches = self.inspection.records.iter().filter(|candidate|
                !candidate.imported && candidate.measurement_id.is_none()
                    && candidate.feature_id == args.feature_id && candidate.group == args.group
                    && candidate.clearance.as_ref().is_some_and(|proof| proof.preview_revision == args.expected_revision));
            let preview = matches.next().ok_or("Calculez d'abord un aperçu avec la même demande, puis enregistrez-le sans modifier le contexte.")?;
            if matches.next().is_some() { return Err("Aperçu ambigu ; recalculez le dégagement.".into()); }
            let preview = preview.clearance.as_ref().ok_or("Preuve d'aperçu absente.")?;
            let current = record.clearance.as_ref().unwrap();
            if preview.sources != current.sources
                || serde_json::to_value(&preview.request).ok() != serde_json::to_value(&current.request).ok()
                || serde_json::to_value(&preview.result).ok() != serde_json::to_value(&current.result).ok()
                || preview.context != current.context
            {
                return Err("La demande, ses sources ou son contexte ont changé depuis l'aperçu ; recalculez-le.".into());
            }
            record.clearance.as_mut().unwrap().preview_revision = preview.preview_revision;
            let mut parents = args.source_measurement_ids.clone();
            let mut constructions = Vec::new();
            let mut reference_id = None;
            for (role, kind, points) in construction_geometries(&result) {
                let source_ids = if role.starts_with("reference_diagonal") {
                    vec![reference_id.ok_or("Construction du rectangle absente.")?]
                } else { args.source_measurement_ids.clone() };
                let measurement = self.add_measurement_with_construction(kind, points,
                    Some(format!("{} · {} · {}", args.group, args.feature_id, role)),
                    frame.capture_id.clone(), record.calibration_id.clone(),
                    scale,
                    "construction".into(), Some("Construction du dégagement à centre fixe ; portée locale observée, extrémités physiques non certifiées.".into()),
                    true, source_ids);
                if role == "reference_rectangle" { reference_id = Some(measurement.id); }
                parents.push(measurement.id);
                constructions.push(json!({"role": role,
                    "snapshot": measurement_snapshot(&measurement, &self.inspection)}));
            }
            record.clearance.as_mut().unwrap().context["constructions"] = Value::Array(constructions);
            let native = self.add_measurement_with_construction(Tool::Circle, record.measurement_points.clone(),
                Some(format!("{} · {} · dégagement à centre fixe", args.group, args.feature_id)),
                frame.capture_id.clone(), record.calibration_id.clone(),
                scale,
                "vision".into(), Some("Cercle dérivé : minimum des distances aux supports de butées observés, centre fixe ; aucun ajustement de contour circulaire ; conformité physique indéterminée.".into()),
                false, parents);
            record.measurement_id = Some(native.id);
            record.measurement_revision = Some(self.inspection.dependencies.revision(native.id));
        }
        record.outcome = assess_clearance(&record, &self.inspection, &image, &frame, &self.measurements);
        self.inspection.records.push(record.clone());
        if self.inspection.records.len() > 1000 { self.inspection.records.remove(0); }
        self.inspection_panel_cache = None;
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        Ok(json!({"document_revision": self.document_revision, "record": record}))
    }
}

/// Recompute software authority from live pixels and application-owned sources.
/// Persisted statuses, nominal dimensions and generic circle samples are unused.
pub(crate) fn assess_clearance(
    record: &InspectionRecord, workspace: &InspectionWorkspace, image: &RgbImage,
    frame: &FrameContext, measurements: &[Measurement],
) -> ValidationOutcome {
    let mut reasons = Vec::<String>::new();
    let mut block = |reason: &str| { if !reasons.iter().any(|existing| existing == reason) { reasons.push(reason.into()); } };
    let Some(proof) = &record.clearance else {
        return ValidationOutcome { geometry_state: GeometryState::Proposed,
            conformity_status: ConformityStatus::Indeterminate,
            blocking_reasons: vec!["clearance_proof_missing".into(), "clearance_physical_conformity_unqualified".into()] };
    };
    if record.imported { block("imported_evidence_requires_reassessment"); }
    if record.capture_id != frame.capture_id { block("capture_mismatch"); }
    if record.source_fingerprint != dper_inspection::fingerprint(image) { block("source_pixels_changed"); }
    if !frame.quality_pass { block("image_quality_failed"); }
    if !circle_inside_image(&proof.result, image) { block("clearance_outside_source_image"); }
    if record.calibration_id != frame.calibration.as_ref().map(|profile| profile.id.clone())
        || record.calibration_fingerprint != dper_inspection::calibration_fingerprint(frame.calibration.as_ref())
    { block("calibration_changed"); }
    let scale = dper_inspection::isotropic_scale(frame.calibration.as_ref(), image);
    if scale.is_err() { block("calibration_changed"); }
    let effective_scale = scale.ok().flatten();
    let value_mm = effective_scale.map(|scale| proof.result.diameter_px * scale);
    if value_mm != record.value_mm { block("physical_value_changed"); }
    match input_context(workspace, image, frame, measurements, &proof.sources) {
        Ok(current) if proof.context.get("inputs") == Some(&current) => (),
        _ => block("measurement_dependencies_changed"),
    }
    match dper_clearance::inspect_clearance(image, &proof.request) {
        Ok(current) if serde_json::to_value(&current).ok() == serde_json::to_value(&proof.result).ok() => (),
        _ => block("clearance_observation_changed"),
    }
    if record.measurement_points != circle_points(&proof.result)
        || serde_json::to_value(&record.result).ok() != serde_json::to_value(circle_presentation(&proof.result)).ok()
        || record.vision_options.is_some() || record.drawing_binding_id.is_some() || record.occurrence.is_some()
    { block("inspection_points_changed"); }
    let mut expected_parents = proof.sources.clone();
    let expected_constructions = construction_geometries(&proof.result);
    if let Some(stored) = proof.context.get("constructions").and_then(Value::as_array) {
        if record.measurement_id.is_some() && stored.len() != expected_constructions.len() {
            block("measurement_dependencies_changed");
        }
        let mut reference_id = None;
        for (stored, (role, kind, points)) in stored.iter().zip(expected_constructions.iter()) {
            let id = stored.get("snapshot").and_then(|snapshot| snapshot.get("id")).and_then(Value::as_u64);
            let mut candidates = measurements.iter().filter(|measurement| Some(measurement.id) == id);
            let candidate = candidates.next();
            let expected_sources = if role.starts_with("reference_diagonal") {
                reference_id.map(|id| vec![id]).unwrap_or_default()
            } else { proof.sources.clone() };
            if stored.get("role").and_then(Value::as_str) != Some(role.as_str())
                || candidates.next().is_some()
                || !candidate.is_some_and(|measurement| measurement.is_construction
                    && measurement.capture_id == frame.capture_id && measurement.kind == *kind
                    && measurement.points == *points && measurement.source_measurement_ids == expected_sources
                    && !measurement.dependency_stale && !workspace.dependencies.is_stale(measurement.id)
                    && stored.get("snapshot") == Some(&measurement_snapshot(measurement, workspace)))
            { block("measurement_dependencies_changed"); }
            if let Some(id) = id {
                expected_parents.push(id);
                if role == "reference_rectangle" { reference_id = Some(id); }
            }
        }
    } else { block("measurement_dependencies_changed"); }
    let mut native_matches = measurements.iter().filter(|measurement| Some(measurement.id) == record.measurement_id);
    match native_matches.next() {
        None => block("measurement_missing"),
        Some(measurement) => {
            if native_matches.next().is_some() { block("measurement_identity_ambiguous"); }
            if record.measurement_revision != Some(workspace.dependencies.revision(measurement.id))
                || workspace.dependencies.revision(measurement.id) == u64::MAX
            { block("measurement_revision_changed"); }
            if measurement.dependency_stale || workspace.dependencies.is_stale(measurement.id)
                || measurement.source_measurement_ids != expected_parents
            { block("measurement_dependencies_changed"); }
            if measurement.kind != Tool::Circle || measurement.is_construction
                || measurement.points != record.measurement_points
                || measurement.capture_id != record.capture_id || measurement.origin != "vision"
                || measurement.calibration_id != record.calibration_id
            { block("measurement_geometry_changed"); }
            let expected_base = effective_scale;
            let expected_x = frame.calibration.as_ref().map(|profile| profile.scale_x_mm_per_pixel.unwrap_or(profile.mm_per_pixel));
            let expected_y = frame.calibration.as_ref().map(|profile| profile.scale_y_mm_per_pixel.unwrap_or(profile.mm_per_pixel));
            if measurement.mm_per_pixel != expected_base || measurement.mm_per_pixel_x != expected_x
                || measurement.mm_per_pixel_y != expected_y || measurement.expanded_uncertainty_mm.is_some()
            { block("measurement_calibration_changed"); }
        }
    }
    let geometry_state = if reasons.is_empty() { GeometryState::Validated } else { GeometryState::Measured };
    reasons.push("clearance_physical_conformity_unqualified".into());
    ValidationOutcome { geometry_state, conformity_status: ConformityStatus::Indeterminate, blocking_reasons: reasons }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dper_clearance::{ClearanceReference, ClearanceRequest, ClearanceStop, ClearanceStrip};
    use crate::dper_vision::{EdgePolarity, VisionOptions};
    use image::Rgb;

    fn fixture() -> (DperMeasureApp, InspectClearanceArgs) {
        let center = Point { x: 140.3, y: 139.7 };
        let step = |value: f64| 0.5 + 0.5 * (value / 0.65).tanh();
        let normals = [Point { x: 1.0, y: 0.0 }, Point { x: 0.0, y: 1.0 },
            Point { x: -1.0, y: 0.0 }, Point { x: 0.0, y: -1.0 }];
        let distances = [60.4, 68.2, 70.6, 65.3];
        let image = RgbImage::from_fn(280, 280, |x, y| {
            let dx = x as f64 - center.x;
            let dy = y as f64 - center.y;
            let mut material = step(16.0 - dx.abs()) * step(11.0 - dy.abs());
            for (normal, distance) in normals.iter().zip(distances) {
                let across = dx * normal.x + dy * normal.y - distance;
                let along = dx * normal.y - dy * normal.x;
                material = material.max(step(across) * step(14.0 - across) * step(28.0 - along.abs()));
            }
            Rgb([(20.0 + 210.0 * material).round().clamp(0.0, 255.0) as u8; 3])
        });
        let strip = |normal: Point, distance: f64, half_span: f64, polarity| {
            let midpoint = Point { x: center.x + normal.x * (distance + 0.8), y: center.y + normal.y * (distance + 0.8) };
            let tangent = Point { x: normal.y, y: -normal.x };
            ClearanceStrip { start: Point { x: midpoint.x - tangent.x * half_span, y: midpoint.y - tangent.y * half_span },
                end: Point { x: midpoint.x + tangent.x * half_span, y: midpoint.y + tangent.y * half_span }, polarity }
        };
        let request = ClearanceRequest {
            reference: ClearanceReference::ObservedRectangle { edges: std::array::from_fn(|index|
                strip(normals[index], if index % 2 == 0 { 16.0 } else { 11.0 },
                    if index % 2 == 0 { 7.0 } else { 12.0 }, EdgePolarity::Falling)) },
            stops: (0..4).map(|index| ClearanceStop { id: format!("pad-{index}"),
                strip: strip(normals[index], distances[index], 22.0, EdgePolarity::Rising) }).collect(),
            options: VisionOptions::default(),
        };
        let mut app = DperMeasureApp::new_with_services(eframe::egui::Context::default(), false);
        app.image = Some(crate::ImageData { width: 280, height: 280,
            rgb: image.as_raw().clone(), source: "clearance-adapter-synthetic.png".into() });
        let mut quality = crate::quality::assess_image_quality(image.as_raw(), 280, 280,
            &crate::quality::ImageQualityThresholds::default()).unwrap();
        // This fixture isolates the adapter; physical optics are not qualified.
        quality.is_exploitable = true;
        app.quality_assessment = Some(quality);
        app.calibration = Some(serde_json::from_value(json!({
            "id": "clearance-demonstration-profile", "device": "synthetic",
            "resolution": [280,280], "optical_setup": "fixture", "mm_per_pixel": 0.1,
            "reference_mm": 10.0, "reference_px": 100.0,
            "origin": "demonstration", "validation_status": "unverified",
        })).unwrap());
        app.mm_per_pixel = Some(0.1);
        let source = app.add_measurement_with_construction(Tool::Coordinate, vec![center],
            Some("External reference construction".into()), app.capture_id.clone(),
            None, None, "interface".into(), None, true, Vec::new());
        let args = InspectClearanceArgs { capture_id: app.capture_id.clone(),
            expected_revision: app.document_revision, feature_id: "clearance-fixture".into(),
            group: "part-A".into(), request, source_measurement_ids: vec![source.id], commit: false };
        (app, args)
    }

    fn preview_then_commit(app: &mut DperMeasureApp, args: &mut InspectClearanceArgs) -> InspectionRecord {
        args.commit = false;
        args.expected_revision = app.document_revision;
        app.perform_clearance(args.clone()).unwrap();
        args.commit = true;
        args.expected_revision = app.document_revision;
        app.perform_clearance(args.clone()).unwrap();
        app.inspection.records.last().unwrap().clone()
    }

    fn assess(app: &DperMeasureApp, record: &InspectionRecord) -> ValidationOutcome {
        assess_clearance(record, &app.inspection, &app.inspection_image().unwrap(),
            &app.inspection_frame(), &app.measurements)
    }

    #[test]
    fn commit_requires_same_preview_revision_request_group_and_sources() {
        let (mut app, mut args) = fixture();
        let original_revision = app.document_revision;
        args.commit = true;
        assert!(app.perform_clearance(args.clone()).is_err());
        assert_eq!(app.document_revision, original_revision);
        assert_eq!(app.measurements.len(), 1);
        args.commit = false;
        app.perform_clearance(args.clone()).unwrap();
        args.commit = true;
        // The pre-preview revision cannot authorize the commit.
        assert!(app.perform_clearance(args.clone()).is_err());
        args.expected_revision = app.document_revision;
        args.request.options.search_radius_px = 7.5;
        assert!(app.perform_clearance(args.clone()).is_err());
        args.request.options.search_radius_px = 8.0;
        args.group = "part-B".into();
        assert!(app.perform_clearance(args.clone()).is_err());
        args.group = "part-A".into();
        let source = args.source_measurement_ids.pop().unwrap();
        assert!(app.perform_clearance(args.clone()).is_err());
        args.source_measurement_ids.push(source);
        assert_eq!(app.measurements.len(), 1);
        app.perform_clearance(args).unwrap();
        assert_eq!(app.measurements.len(), 9); // root + rectangle + diagonals + 4 stops + circle
    }

    #[test]
    fn committed_clearance_has_native_dependencies_and_distinct_pixel_evidence() {
        let (mut app, mut args) = fixture();
        let record = preview_then_commit(&mut app, &mut args);
        let outcome = assess(&app, &record);
        assert_eq!(outcome.geometry_state, GeometryState::Validated);
        assert_eq!(outcome.conformity_status, ConformityStatus::Indeterminate);
        assert_eq!(outcome.blocking_reasons, vec!["clearance_physical_conformity_unqualified"]);
        let proof = record.clearance.as_ref().unwrap();
        assert!((proof.result.radius_px - 60.4).abs() < 0.2);
        assert_eq!(record.value_mm, Some(proof.result.diameter_px * 0.1));
        assert!(record.result.evidence.accepted_points.is_empty());
        assert_eq!(record.result.evidence.sample_count, 0);
        assert!(proof.result.stops.iter().all(|stop| stop.evidence.accepted_points.len() >= 8));
        let circle = app.measurements.iter().find(|measurement| Some(measurement.id) == record.measurement_id).unwrap();
        assert_eq!(circle.kind, Tool::Circle);
        assert_eq!(circle.source_measurement_ids.len(), 8);
        assert_eq!(circle.points[0], proof.result.center);
        assert!(circle.expanded_uncertainty_mm.is_none());
        assert_eq!(app.measurements.iter().filter(|measurement| measurement.is_construction).count(), 8);
        crate::measurement_graph::validate_graph(&app.measurements).unwrap();
    }

    #[test]
    fn changed_external_sources_constructions_and_native_circle_revoke_authority() {
        let (mut app, mut args) = fixture();
        let record = preview_then_commit(&mut app, &mut args);
        let source_id = args.source_measurement_ids[0];
        let original = app.measurements.clone();
        app.measurements.iter_mut().find(|measurement| measurement.id == source_id).unwrap().points[0].x += 1.0;
        assert!(assess(&app, &record).blocking_reasons.contains(&"measurement_dependencies_changed".into()));
        app.measurements = original.clone();
        let stop = app.measurements.iter_mut().find(|measurement| measurement.label.contains("stop:pad-0")).unwrap();
        stop.points[0].y += 0.5;
        assert!(assess(&app, &record).blocking_reasons.contains(&"measurement_dependencies_changed".into()));
        app.measurements = original.clone();
        app.measurements.iter_mut().find(|measurement| Some(measurement.id) == record.measurement_id).unwrap().points[1].x += 0.5;
        assert!(assess(&app, &record).blocking_reasons.contains(&"measurement_geometry_changed".into()));
        app.measurements = original;
        app.inspection.dependencies.invalidate(&app.measurements, &[source_id]);
        assert_ne!(assess(&app, &record).geometry_state, GeometryState::Validated);
    }

    #[test]
    fn imported_changed_pixels_calibration_and_forged_clearance_proof_are_not_authority() {
        let (mut app, mut args) = fixture();
        let record = preview_then_commit(&mut app, &mut args);
        let imported: InspectionRecord = serde_json::from_value(serde_json::to_value(&record).unwrap()).unwrap();
        assert!(imported.imported);
        assert_ne!(assess(&app, &imported).geometry_state, GeometryState::Validated);
        let mut forged = record.clone();
        forged.clearance.as_mut().unwrap().result.radius_px += 1.0;
        assert!(assess(&app, &forged).blocking_reasons.contains(&"clearance_observation_changed".into()));
        let mut disguised = serde_json::to_value(&record).unwrap();
        disguised.as_object_mut().unwrap().remove("clearance");
        let disguised: InspectionRecord = serde_json::from_value(disguised).unwrap();
        let outcome = dper_inspection::assess_record(&disguised, &app.inspection,
            &app.inspection_image().unwrap(), &app.inspection_frame(), &app.measurements);
        assert_ne!(outcome.geometry_state, GeometryState::Validated);
        app.image.as_mut().unwrap().rgb[0] ^= 1;
        assert!(assess(&app, &record).blocking_reasons.contains(&"source_pixels_changed".into()));
        app.image.as_mut().unwrap().rgb[0] ^= 1;
        app.calibration.as_mut().unwrap().reference_mm += 0.1;
        assert!(assess(&app, &record).blocking_reasons.contains(&"calibration_changed".into()));
    }

    #[test]
    fn missing_calibration_keeps_pixels_but_anisotropy_refuses_a_single_physical_diameter() {
        let (mut app, mut args) = fixture();
        app.calibration.as_mut().unwrap().scale_x_mm_per_pixel = Some(0.11);
        assert!(app.perform_clearance(args.clone()).is_err());
        assert_eq!(app.measurements.len(), 1);
        app.calibration = None;
        app.mm_per_pixel = None;
        let record = preview_then_commit(&mut app, &mut args);
        assert_eq!(record.value_mm, None);
        assert_eq!(assess(&app, &record).geometry_state, GeometryState::Validated);
        assert_eq!(assess(&app, &record).conformity_status, ConformityStatus::Indeterminate);
        let native = app.measurements.iter().find(|measurement| Some(measurement.id) == record.measurement_id).unwrap();
        assert_eq!(native.mm_per_pixel, None);
        assert_eq!(native.mm_per_pixel_x, None);
        assert_eq!(native.mm_per_pixel_y, None);
    }

    #[test]
    fn explicit_isotropic_axes_without_history_match_the_native_and_report_diameter() {
        let (mut app, mut args) = fixture();
        let profile = app.calibration.as_mut().unwrap();
        profile.scale_x_mm_per_pixel = Some(0.2);
        profile.scale_y_mm_per_pixel = Some(0.2);
        // The legacy scalar intentionally differs. The actual isotropic XY
        // scale must govern both report conversion and native presentation.
        assert_eq!(profile.mm_per_pixel, 0.1);
        app.calibration_history.clear();
        let record = preview_then_commit(&mut app, &mut args);
        assert_eq!(assess(&app, &record).geometry_state, GeometryState::Validated);
        let native = app.measurements.iter().find(|measurement| Some(measurement.id) == record.measurement_id).unwrap();
        let metric = crate::geometry::metric_points(native).unwrap();
        let native_diameter_mm = 2.0 * metric[0].dist(metric[1]);
        assert!((record.value_mm.unwrap() - native_diameter_mm).abs() < 1e-12);
        assert_eq!(native.mm_per_pixel, Some(0.2));
    }

    #[test]
    fn enclosing_observed_strips_cannot_authorize_a_circle_outside_the_source_frame() {
        let (mut app, mut args) = fixture();
        let center = Point { x: 20.0, y: 140.0 };
        let normals = [Point { x: 1.0, y: 0.0 },
            Point { x: -0.2, y: 0.96_f64.sqrt() },
            Point { x: -0.2, y: -0.96_f64.sqrt() }];
        let step = |value: f64| 0.5 + 0.5 * (value / 0.65).tanh();
        let image = RgbImage::from_fn(280, 280, |x, y| {
            let dx = x as f64 - center.x;
            let dy = y as f64 - center.y;
            let mut material = 0.0_f64;
            for normal in normals {
                let across = dx * normal.x + dy * normal.y - 60.0;
                let along = dx * normal.y - dy * normal.x;
                material = material.max(step(across) * step(14.0 - across) * step(9.0 - along.abs()));
            }
            Rgb([(20.0 + 210.0 * material).round().clamp(0.0, 255.0) as u8; 3])
        });
        app.image.as_mut().unwrap().rgb = image.as_raw().clone();
        args.request.reference = ClearanceReference::SuppliedCenter { center };
        args.request.stops = normals.iter().enumerate().map(|(index, normal)| {
            let midpoint = Point { x: center.x + normal.x * 60.8, y: center.y + normal.y * 60.8 };
            let tangent = Point { x: normal.y, y: -normal.x };
            ClearanceStop { id: format!("frame-pad-{index}"), strip: ClearanceStrip {
                start: Point { x: midpoint.x - tangent.x * 5.0, y: midpoint.y - tangent.y * 5.0 },
                end: Point { x: midpoint.x + tangent.x * 5.0, y: midpoint.y + tangent.y * 5.0 },
                polarity: EdgePolarity::Rising,
            } }
        }).collect();
        // All three finite local supports are genuinely observable and enclose
        // the center angularly; the full derived circle still leaves the frame.
        let observed = dper_clearance::inspect_clearance(&image, &args.request).unwrap();
        assert!(!circle_inside_image(&observed, &image));
        let before_revision = app.document_revision;
        let refusal = app.perform_clearance(args).unwrap_err();
        assert!(refusal.contains("dépasse l'image source"));
        assert_eq!(app.measurements.len(), 1);
        assert!(app.inspection.records.is_empty());
        assert_eq!(app.document_revision, before_revision);
    }
}

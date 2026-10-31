//! Pure orchestration of drawing interpretation, localization and observed contours.
//! A registration scale localizes search profiles; only an independent calibration
//! converts observed pixels to physical units. Persisted evidence is always imported.
use crate::{
    dper_drawing::{
        DrawingCharacteristic, DrawingControlPlan, DrawingDimensionKind, DrawingMeasurability,
    },
    dper_metrology::{
        self, ConformityStatus, GeometryEvidence, GeometryState, Tolerance, ValidationInput,
        ValidationOutcome, ValidationThresholds,
    },
    dper_registration::RegistrationReport,
    dper_vision::{self, InspectionGeometry, InspectionResult, VisionOptions},
    geometry::{Measurement, Point, Tool},
    storage::Calibration,
};
use image::RgbImage;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct InspectionWorkspace {
    // Never trust persisted generations to grant current validation on import.
    #[serde(default, skip_deserializing)]
    pub dependencies: crate::measurement_graph::DependencyGraph,
    pub plan: Option<DrawingControlPlan>,
    pub views: Vec<crate::dper_binding::DrawingView>,
    pub bindings: Vec<crate::dper_binding::BindingSelection>,
    pub registration: Option<RegistrationReport>,
    pub registration_scope: Option<DrawingRegistrationScope>,
    pub registration_capture_id: Option<String>,
    pub registration_fingerprint: Option<u64>,
    pub records: Vec<InspectionRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DrawingRegistrationScope {
    pub document_fingerprint: String,
    pub page: u32,
    pub view_id: String,
}

#[derive(Clone, Debug)]
pub struct FrameContext {
    pub capture_id: String,
    pub calibration: Option<Calibration>,
    pub quality_pass: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InspectionRecord {
    pub feature_id: String,
    #[serde(default)]
    pub drawing_binding_id: Option<String>,
    pub group: String,
    pub capture_id: String,
    pub source_fingerprint: u64,
    pub calibration_id: Option<String>,
    /// Detects a changed profile even when its ID has been reused.
    #[serde(default)]
    pub calibration_fingerprint: Option<u64>,
    pub result: InspectionResult,
    pub value_mm: Option<f64>,
    pub outcome: ValidationOutcome,
    pub measurement_id: Option<u64>,
    #[serde(default)]
    pub measurement_revision: Option<u64>,
    /// Presentation geometry from the robust fit, not additional observed samples.
    /// Actual contour samples remain in result.evidence.accepted_points.
    pub measurement_points: Vec<Point>,
    /// A JSON field cannot grant current-session pixel evidence to itself.
    #[serde(skip_deserializing, default = "imported_by_default")]
    pub imported: bool,
}

fn imported_by_default() -> bool {
    true
}

fn hash_bytes(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes
        .into_iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

/// Stable FNV-1a over dimensions and the original RGB pixels, independent of process.
pub fn fingerprint(image: &RgbImage) -> u64 {
    hash_bytes(
        image
            .width()
            .to_le_bytes()
            .into_iter()
            .chain(image.height().to_le_bytes())
            .chain(image.as_raw().iter().copied()),
    )
}

fn calibration_fingerprint(calibration: Option<&Calibration>) -> Option<u64> {
    calibration
        .and_then(|profile| serde_json::to_vec(profile).ok())
        .map(hash_bytes)
}

fn feature<'a>(
    workspace: &'a InspectionWorkspace,
    id: &str,
) -> Result<&'a DrawingCharacteristic, String> {
    let plan = workspace
        .plan
        .as_ref()
        .ok_or("Aucun plan de contrôle actif.")?;
    let mut matches = plan
        .characteristics
        .iter()
        .filter(|candidate| candidate.id == id);
    let candidate = matches
        .next()
        .ok_or("Caractéristique absente du plan de contrôle.")?;
    if id.trim().is_empty() || matches.next().is_some() {
        return Err("Identifiant de caractéristique vide ou ambigu.".into());
    }
    if candidate.measurability == DrawingMeasurability::NotMeasurable2d {
        return Err(format!(
            "Caractéristique non mesurable sur une seule image 2D : {}",
            candidate.reasons.join("; ")
        ));
    }
    Ok(candidate)
}

fn registration_with_fingerprint<'a>(
    workspace: &'a InspectionWorkspace,
    frame: &FrameContext,
    source_fingerprint: u64,
) -> Result<&'a RegistrationReport, String> {
    if frame.capture_id.trim().is_empty()
        || workspace.registration_capture_id.as_deref() != Some(frame.capture_id.as_str())
        || workspace.registration_fingerprint != Some(source_fingerprint)
    {
        return Err("Recalage périmé : capture ou pixels source différents.".into());
    }
    let report = workspace
        .registration
        .as_ref()
        .ok_or("Recalage plan/image absent.")?;
    report.transform.inverse()?;
    if report.inlier_indices.len() < 3
        || report
            .inlier_indices
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            < 3
        || !report.rms_error_px.is_finite()
        || report.rms_error_px < 0.0
        || !report.max_error_px.is_finite()
        || report.max_error_px < report.rms_error_px
    {
        return Err("Les preuves du recalage sont invalides ou incomplètes.".into());
    }
    Ok(report)
}

fn project(report: &RegistrationReport, geometry: &InspectionGeometry) -> InspectionGeometry {
    let transform = report.transform;
    match *geometry {
        InspectionGeometry::Line { start, end } => InspectionGeometry::Line {
            start: transform.apply(start),
            end: transform.apply(end),
        },
        InspectionGeometry::Circle { center, radius } => InspectionGeometry::Circle {
            center: transform.apply(center),
            radius: radius * transform.scale,
        },
        InspectionGeometry::Arc {
            center,
            radius,
            start_rad,
            sweep_rad,
        } => InspectionGeometry::Arc {
            center: transform.apply(center),
            radius: radius * transform.scale,
            start_rad: start_rad + transform.rotation_rad,
            sweep_rad,
        },
    }
}

fn check_geometry_type(
    candidate: &DrawingCharacteristic,
    geometry: &InspectionGeometry,
) -> Result<(), String> {
    match (candidate.kind, geometry) {
        (DrawingDimensionKind::Diameter | DrawingDimensionKind::Radius,
            InspectionGeometry::Circle { .. } | InspectionGeometry::Arc { .. }) => Ok(()),
        (DrawingDimensionKind::Linear, InspectionGeometry::Line { .. }) => Ok(()),
        (DrawingDimensionKind::Linear, _) => Err("La longueur entre extrémités ou entre contours demande une méthode dédiée ; non prise en charge ici.".into()),
        (DrawingDimensionKind::Angle, _) => Err("Le contrôle d'un angle entre deux supports n'est pas encore pris en charge.".into()),
        (DrawingDimensionKind::Unknown | DrawingDimensionKind::Geometric, _) => Err("Le type ou les références géométriques de cette caractéristique restent à interpréter.".into()),
        _ => Err("Le type de géométrie demandé ne correspond pas à la caractéristique du plan.".into()),
    }
}

fn isotropic_scale(
    calibration: Option<&Calibration>,
    image: &RgbImage,
) -> Result<Option<f64>, String> {
    let Some(profile) = calibration else {
        return Ok(None);
    };
    let x = profile.scale_x_mm_per_pixel.unwrap_or(profile.mm_per_pixel);
    let y = profile.scale_y_mm_per_pixel.unwrap_or(profile.mm_per_pixel);
    if profile.id.trim().is_empty()
        || profile.resolution != [image.width(), image.height()]
        || !profile.mm_per_pixel.is_finite()
        || profile.mm_per_pixel <= 0.0
        || !x.is_finite()
        || x <= 0.0
        || !y.is_finite()
        || y <= 0.0
        || !profile.reference_mm.is_finite()
        || profile.reference_mm <= 0.0
        || !profile.reference_px.is_finite()
        || profile.reference_px <= 0.0
    {
        return Err("Calibration invalide ou incompatible avec la résolution source.".into());
    }
    if (x - y).abs() > x.max(y) * 1e-9 {
        return Err("Calibration anisotrope : un cercle en pixels ne permet pas ce contrôle de rayon ou diamètre.".into());
    }
    Ok(Some(x))
}

fn physical_value(
    candidate: &DrawingCharacteristic,
    result: &InspectionResult,
    image: &RgbImage,
    frame: &FrameContext,
) -> Result<Option<f64>, String> {
    match result.geometry {
        InspectionGeometry::Circle { radius, .. } | InspectionGeometry::Arc { radius, .. } => {
            let Some(scale) = isotropic_scale(frame.calibration.as_ref(), image)? else {
                return Ok(None);
            };
            let multiplier = match candidate.kind {
                DrawingDimensionKind::Diameter => 2.0,
                DrawingDimensionKind::Radius => 1.0,
                _ => return Err("La caractéristique n'est ni un rayon ni un diamètre.".into()),
            };
            let value = radius * scale * multiplier;
            if !value.is_finite() || value <= 0.0 {
                return Err("Dimension physique invalide.".into());
            }
            Ok(Some(value))
        }
        // A fitted support's extent is not a pair of detected part endpoints.
        InspectionGeometry::Line { .. } => Ok(None),
    }
}

fn tolerance(candidate: &DrawingCharacteristic) -> Option<Tolerance> {
    if !candidate.confirmed {
        return None;
    }
    let nominal = candidate.nominal_mm();
    let (lower, upper) = candidate.limits_mm()?;
    if lower > upper {
        return None;
    }
    Some(Tolerance {
        nominal,
        lower,
        upper,
    })
}

fn evaluate_record(
    record: &InspectionRecord,
    candidate: Option<&DrawingCharacteristic>,
    frame: &FrameContext,
    current_value: Option<f64>,
) -> ValidationOutcome {
    let evidence = &record.result.evidence;
    let mut outcome = dper_metrology::evaluate(&ValidationInput {
        source_capture_id: record.capture_id.clone(),
        current_capture_id: frame.capture_id.clone(),
        calibration_id: record.calibration_id.clone(),
        calibration_qualified: false,
        // Reference uncertainty is not the total uncertainty of this measurement.
        expanded_uncertainty: None,
        quality_pass: frame.quality_pass,
        evidence: Some(GeometryEvidence {
            sample_count: evidence.accepted_points.len(),
            fit_rms: Some(evidence.rms_residual_px),
            fit_max: Some(evidence.max_residual_px),
            coverage: Some(evidence.coverage_fraction),
            imported: record.imported,
        }),
        observable_in_2d: candidate
            .is_some_and(|feature| feature.measurability != DrawingMeasurability::NotMeasurable2d),
        tolerance: candidate.and_then(tolerance),
        measured_value: current_value,
        thresholds: ValidationThresholds::default(),
    });
    if let Some(candidate) = candidate {
        if !candidate.confirmed {
            add_reason(&mut outcome, "drawing_feature_unconfirmed", false);
        }
        if candidate.confirmed && candidate.tolerance.is_some() && tolerance(candidate).is_none() {
            add_reason(&mut outcome, "tolerance_unit_or_limits_unresolved", false);
        }
    }
    if matches!(record.result.geometry, InspectionGeometry::Line { .. }) {
        add_reason(&mut outcome, "line_endpoints_unverified", true);
    }
    outcome
}

fn add_reason(outcome: &mut ValidationOutcome, reason: &str, geometry_blocked: bool) {
    if !outcome
        .blocking_reasons
        .iter()
        .any(|existing| existing == reason)
    {
        outcome.blocking_reasons.push(reason.into());
    }
    if geometry_blocked {
        if outcome.geometry_state == GeometryState::Validated {
            outcome.geometry_state = GeometryState::Measured;
        }
        if outcome.conformity_status != ConformityStatus::NotApplicable {
            outcome.conformity_status = ConformityStatus::Indeterminate;
        }
    }
}

fn native_measurement_points(result: &InspectionResult) -> Vec<Point> {
    match result.geometry {
        InspectionGeometry::Line { start, end } => vec![start, end],
        InspectionGeometry::Circle { center, radius }
        | InspectionGeometry::Arc { center, radius, .. } => {
            // Preserve the orthogonal robust fit exactly. Passing its observed
            // samples to native CircleFit would perform a different algebraic fit.
            vec![
                center,
                Point {
                    x: center.x + radius,
                    y: center.y,
                },
            ]
        }
    }
}

/// Detect actual contour samples near drawing-localized search profiles.
pub fn inspect_feature(
    workspace: &InspectionWorkspace,
    image: &RgbImage,
    frame: &FrameContext,
    feature_id: &str,
    group: &str,
    geometry_in_drawing: &InspectionGeometry,
    options: &VisionOptions,
) -> Result<InspectionRecord, String> {
    if group.trim().is_empty() {
        return Err("Le groupe de contrôle doit être renseigné.".into());
    }
    if !frame.quality_pass {
        return Err("La qualité de l'image ne permet pas l'inspection.".into());
    }
    let candidate = feature(workspace, feature_id)?;
    check_geometry_type(candidate, geometry_in_drawing)?;
    let source_fingerprint = fingerprint(image);
    let report = registration_with_fingerprint(workspace, frame, source_fingerprint)?;
    if !matches!(geometry_in_drawing, InspectionGeometry::Line { .. }) {
        isotropic_scale(frame.calibration.as_ref(), image)?;
    }
    let expected = project(report, geometry_in_drawing);
    let result = dper_vision::inspect_geometry(image, &expected, options)?;
    let value_mm = physical_value(candidate, &result, image, frame)?;
    let measurement_points = native_measurement_points(&result);
    let mut record = InspectionRecord {
        feature_id: feature_id.into(),
        drawing_binding_id: None,
        group: group.trim().into(),
        capture_id: frame.capture_id.clone(),
        source_fingerprint,
        calibration_id: frame.calibration.as_ref().map(|profile| profile.id.clone()),
        calibration_fingerprint: calibration_fingerprint(frame.calibration.as_ref()),
        result,
        value_mm,
        outcome: ValidationOutcome {
            geometry_state: GeometryState::Proposed,
            conformity_status: ConformityStatus::Indeterminate,
            blocking_reasons: Vec::new(),
        },
        measurement_id: None,
        measurement_revision: None,
        measurement_points,
        imported: false,
    };
    record.outcome = evaluate_record(&record, Some(candidate), frame, value_mm);
    Ok(record)
}

/// Reassess gates against current pixels, profile and stored measurement geometry.
/// Saved statuses and deserialized evidence never grant current validation.
pub fn assess_record(
    record: &InspectionRecord,
    workspace: &InspectionWorkspace,
    image: &RgbImage,
    frame: &FrameContext,
    measurements: &[Measurement],
) -> ValidationOutcome {
    assess_record_with_fingerprint(
        record,
        workspace,
        image,
        frame,
        measurements,
        fingerprint(image),
    )
}

/// Batch reassessment with one fingerprint calculation for the immutable source image.
/// This crate-internal entry point requires `source_fingerprint = fingerprint(image)`
/// computed locally for this snapshot. Never forward an MCP/JSON-supplied fingerprint.
pub(crate) fn assess_record_with_fingerprint(
    record: &InspectionRecord,
    workspace: &InspectionWorkspace,
    image: &RgbImage,
    frame: &FrameContext,
    measurements: &[Measurement],
    source_fingerprint: u64,
) -> ValidationOutcome {
    let candidate = feature(workspace, &record.feature_id).ok();
    let current_value = candidate.and_then(|candidate| {
        physical_value(candidate, &record.result, image, frame)
            .ok()
            .flatten()
    });
    let mut outcome = evaluate_record(record, candidate, frame, current_value);
    if let Some(binding_id) = &record.drawing_binding_id {
        let mut selections = workspace.bindings.iter().filter(|selection| selection.candidate.id == *binding_id);
        let selected = selections.next();
        let unique_selection = selected.is_some() && selections.next().is_none()
            && workspace.bindings.iter().filter(|selection| selection.candidate.feature_id == record.feature_id).count() == 1;
        let binding_current = selected.is_some_and(|selection| {
            let binding = &selection.candidate;
            let mut views = workspace.views.iter().filter(|view| view.id == binding.view_id);
            let view = views.next();
            let unique_view = view.is_some() && views.next().is_none();
            !binding_id.trim().is_empty()
                && !binding.view_id.trim().is_empty()
                && !binding.geometry_id.trim().is_empty()
                && unique_selection
                && binding.feature_id == record.feature_id
                && !binding.document_fingerprint.trim().is_empty()
                && workspace.plan.as_ref().and_then(|plan| plan.source_fingerprint.as_deref())
                    == Some(binding.document_fingerprint.as_str())
                && workspace.registration_scope.as_ref().is_some_and(|scope|
                    scope.document_fingerprint == binding.document_fingerprint
                        && scope.page == binding.page && scope.view_id == binding.view_id)
                && binding.page > 0
                && unique_view
                && view.is_some_and(|view|
                    view.page == binding.page && view.document_fingerprint == binding.document_fingerprint
                        && view.feature_ids.iter().any(|id| id == &binding.feature_id)
                        && view.geometry_ids.iter().any(|id| id == &binding.geometry_id))
                && candidate.is_some_and(|feature|
                    feature.source.page == Some(binding.page)
                        && crate::dper_binding::feature_fingerprint(feature).is_ok_and(|fingerprint|
                        fingerprint == binding.feature_fingerprint))
        });
        if !binding_current {
            add_reason(&mut outcome, "drawing_binding_changed", true);
            outcome.conformity_status = ConformityStatus::Indeterminate;
        }
        if selected.is_some_and(|selection| !selection.reviewed) {
            add_reason(&mut outcome, "drawing_binding_unreviewed", true);
            outcome.conformity_status = ConformityStatus::Indeterminate;
        }
    }
    if candidate.is_none() {
        add_reason(
            &mut outcome,
            "drawing_feature_missing_or_unobservable",
            true,
        );
    }
    if candidate
        .is_some_and(|candidate| check_geometry_type(candidate, &record.result.geometry).is_err())
    {
        add_reason(&mut outcome, "drawing_geometry_type_changed", true);
    }
    if record.source_fingerprint != source_fingerprint {
        add_reason(&mut outcome, "source_pixels_changed", true);
    }
    if registration_with_fingerprint(workspace, frame, source_fingerprint).is_err() {
        add_reason(&mut outcome, "registration_missing_or_stale", true);
    }
    let current_calibration_id = frame.calibration.as_ref().map(|profile| profile.id.clone());
    if record.calibration_id != current_calibration_id
        || record.calibration_fingerprint != calibration_fingerprint(frame.calibration.as_ref())
    {
        add_reason(&mut outcome, "calibration_changed", true);
    }
    if current_value != record.value_mm {
        add_reason(&mut outcome, "physical_value_changed", true);
    }
    let consistent_points = record.measurement_points == native_measurement_points(&record.result);
    if !consistent_points {
        add_reason(&mut outcome, "inspection_points_changed", true);
    }
    let measured = record
        .measurement_id
        .and_then(|id| measurements.iter().find(|measurement| measurement.id == id));
    match measured {
        None => add_reason(&mut outcome, "measurement_missing", true),
        Some(measurement) => {
            if workspace.dependencies.revision(measurement.id) == u64::MAX
                || record.measurement_revision != Some(workspace.dependencies.revision(measurement.id)) {
                add_reason(&mut outcome, "measurement_revision_changed", true);
            }
            if measurement.dependency_stale || workspace.dependencies.is_stale(measurement.id) {
                add_reason(&mut outcome, "measurement_dependencies_changed", true);
            }
            let kind_matches = match record.result.geometry {
                InspectionGeometry::Line { .. } => measurement.kind == Tool::Distance,
                _ => measurement.kind == Tool::Circle,
            };
            if measurement.capture_id != record.capture_id
                || measurement.capture_id != frame.capture_id
                || measurement.calibration_id != record.calibration_id
                || measurement.is_construction
                || !kind_matches
                || measurement.points != record.measurement_points
            {
                add_reason(&mut outcome, "measurement_geometry_changed", true);
            }
            let scales_match = match frame.calibration.as_ref() {
                Some(profile) => {
                    measurement.mm_per_pixel == Some(profile.mm_per_pixel)
                        && measurement.mm_per_pixel_x.or(measurement.mm_per_pixel)
                            == Some(profile.scale_x_mm_per_pixel.unwrap_or(profile.mm_per_pixel))
                        && measurement.mm_per_pixel_y.or(measurement.mm_per_pixel)
                            == Some(profile.scale_y_mm_per_pixel.unwrap_or(profile.mm_per_pixel))
                }
                None => {
                    measurement.mm_per_pixel.is_none()
                        && measurement.mm_per_pixel_x.is_none()
                        && measurement.mm_per_pixel_y.is_none()
                }
            };
            if !scales_match {
                add_reason(&mut outcome, "measurement_calibration_changed", true);
            }
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dper_drawing::parse_text_plan, dper_registration::SimilarityTransform};
    use image::Rgb;

    fn disk() -> RgbImage {
        RgbImage::from_fn(128, 128, |x, y| {
            let distance = (x as f64 - 64.0).hypot(y as f64 - 64.0);
            let intensity = (20.0 + 200.0 * (distance - 24.5).clamp(0.0, 1.0)) as u8;
            Rgb([intensity; 3])
        })
    }

    fn frame() -> FrameContext {
        FrameContext { capture_id: "capture-test".into(), quality_pass: true,
            calibration: Some(serde_json::from_value(serde_json::json!({
                "id": "cal-test", "device": "test", "resolution": [128,128], "optical_setup": "test",
                "mm_per_pixel": 0.1, "reference_mm": 10.0, "reference_px": 100.0,
                "expanded_uncertainty_mm": 0.002
            })).unwrap()) }
    }

    fn workspace(image: &RgbImage, text: &str, scale: f64) -> InspectionWorkspace {
        let mut plan = parse_text_plan(text, "drawing");
        plan.characteristics[0].confirmed = true;
        InspectionWorkspace {
            plan: Some(plan),
            registration: Some(RegistrationReport {
                transform: SimilarityTransform {
                    scale,
                    rotation_rad: 0.0,
                    translation: Point::default(),
                },
                inlier_indices: vec![0, 1, 2],
                rms_error_px: 0.0,
                max_error_px: 0.0,
            }),
            registration_capture_id: Some("capture-test".into()),
            registration_fingerprint: Some(fingerprint(image)),
            records: Vec::new(),
            ..Default::default()
        }
    }

    fn measurement(record: &InspectionRecord) -> Measurement {
        Measurement {
            id: 1,
            kind: Tool::Circle,
            is_construction: false, source_measurement_ids: Vec::new(), dependency_stale: false,
            points: record.measurement_points.clone(),
            label: "inspection".into(),
            label_position_native_px: None,
            capture_id: record.capture_id.clone(),
            calibration_id: record.calibration_id.clone(),
            mm_per_pixel: Some(0.1),
            mm_per_pixel_x: None,
            mm_per_pixel_y: None,
            expanded_uncertainty_mm: None,
            origin: "inspection".into(),
            method: None,
            protocol_id: None,
            protocol_version: None,
        }
    }

    fn circle(
        workspace: &InspectionWorkspace,
        image: &RgbImage,
        frame: &FrameContext,
        scale: f64,
    ) -> InspectionRecord {
        inspect_feature(
            workspace,
            image,
            frame,
            "drawing-1",
            "group-1",
            &InspectionGeometry::Circle {
                center: Point {
                    x: 64.0 / scale,
                    y: 64.0 / scale,
                },
                radius: 25.0 / scale,
            },
            &VisionOptions::default(),
        )
        .unwrap()
    }

    fn bound_fixture() -> (RgbImage, InspectionWorkspace, FrameContext, InspectionRecord, Measurement) {
        use crate::dper_binding::{binding_candidates, define_view, select_binding};
        use crate::dper_pdf_layout::{PdfBox, PdfPageGeometry};
        let image = disk();
        let frame = frame();
        let mut workspace = workspace(&image, "Ø5 ±0.2 mm", 1.0);
        let document_fingerprint = crate::dper_source::fingerprint_bytes(b"synthetic drawing source");
        let plan = workspace.plan.as_mut().unwrap();
        plan.source_fingerprint = Some(document_fingerprint.clone());
        plan.page_geometry = vec![PdfPageGeometry { page: 1, width_pt: 128.0, height_pt: 128.0 }];
        plan.characteristics[0].source.page = Some(1);
        plan.characteristics[0].source.pdf_bbox = Some(PdfBox { x_min: 5.0, y_min: 108.0, x_max: 80.0, y_max: 120.0 });
        let catalog = crate::dper_pdf_geometry::parse_svg_geometry(
            r#"<svg width="128pt" height="128pt" viewBox="0 0 128 128"><circle cx="64" cy="64" r="25" fill="none" stroke="black"/></svg>"#, 1).unwrap();
        let view = define_view(plan, &document_fingerprint, &catalog, "Vue synthétique explicite",
            &[plan.characteristics[0].id.clone()], &[catalog.primitives[0].id.clone()]).unwrap();
        let candidates = binding_candidates(plan, &document_fingerprint, &catalog, &view,
            &plan.characteristics[0].id).unwrap();
        let mut selection = select_binding(&candidates, &candidates[0].id, &catalog,
            "Source synthétique : texte et cercle de la page 1 revus dans cette vue.").unwrap();
        assert!(!selection.reviewed);
        // Simulate the current-session UI review, never a deserialized/MCP claim.
        selection.reviewed = true;
        workspace.registration_scope = Some(DrawingRegistrationScope {
            document_fingerprint, page: view.page, view_id: view.id.clone(),
        });
        workspace.views.push(view);
        let binding_id = selection.candidate.id.clone();
        workspace.bindings.push(selection);
        let mut record = circle(&workspace, &image, &frame, 1.0);
        record.drawing_binding_id = Some(binding_id);
        record.measurement_id = Some(1);
        record.measurement_revision = Some(0);
        let measured = measurement(&record);
        (image, workspace, frame, record, measured)
    }

    #[test]
    fn identical_restored_geometry_cannot_reuse_an_older_observation_generation() {
        let (image, mut workspace, frame, mut record, measured) = bound_fixture();
        workspace.dependencies.mark_created(measured.id);
        record.measurement_revision = Some(workspace.dependencies.revision(measured.id));
        assert_eq!(assess_record(&record, &workspace, &image, &frame, &[measured.clone()]).geometry_state,
            GeometryState::Validated);
        workspace.dependencies.invalidate(&[measured.clone()], &[measured.id]);
        let restored = assess_record(&record, &workspace, &image, &frame, &[measured.clone()]);
        assert_ne!(restored.geometry_state, GeometryState::Validated);
        assert!(restored.blocking_reasons.contains(&"measurement_revision_changed".into()));
        // Exhaustion is also fail-closed, even if an old stamp happens to equal MAX.
        workspace.dependencies = serde_json::from_value(serde_json::json!({
            "versions": {"1": u64::MAX}, "stale": []
        })).unwrap();
        record.measurement_revision = Some(u64::MAX);
        assert_ne!(assess_record(&record, &workspace, &image, &frame, &[measured]).geometry_state,
            GeometryState::Validated);
    }

    #[test]
    fn current_bound_contour_requires_current_session_review() {
        let (image, mut workspace, frame, record, measured) = bound_fixture();
        let reviewed = assess_record(&record, &workspace, &image, &frame, &[measured.clone()]);
        assert_eq!(reviewed.geometry_state, GeometryState::Validated);
        assert_eq!(reviewed.conformity_status, ConformityStatus::Indeterminate);
        assert!(!reviewed.blocking_reasons.iter().any(|reason| reason.starts_with("drawing_binding_")));
        workspace.bindings[0].reviewed = false;
        let unreviewed = assess_record(&record, &workspace, &image, &frame, &[measured]);
        assert_ne!(unreviewed.geometry_state, GeometryState::Validated);
        assert!(unreviewed.blocking_reasons.contains(&"drawing_binding_unreviewed".into()));
        workspace.plan.as_mut().unwrap().characteristics[0].confirmed = false;
        let unconfirmed = assess_record(&record, &workspace, &image, &frame, &[]);
        assert_eq!(unconfirmed.conformity_status, ConformityStatus::Indeterminate);
    }

    #[test]
    fn changed_binding_source_feature_page_or_view_invalidates_observed_contour() {
        let (image, workspace, frame, record, measured) = bound_fixture();
        let assert_changed = |changed: InspectionWorkspace| {
            let outcome = assess_record(&record, &changed, &image, &frame, &[measured.clone()]);
            assert_ne!(outcome.geometry_state, GeometryState::Validated);
            assert!(outcome.blocking_reasons.contains(&"drawing_binding_changed".into()));
        };
        let mut changed = workspace.clone();
        changed.registration_scope.as_mut().unwrap().page = 2;
        assert_changed(changed);
        let mut changed = workspace.clone();
        changed.views[0].page = 2;
        assert_changed(changed);
        let mut changed = workspace.clone();
        changed.plan.as_mut().unwrap().source_fingerprint = Some("different-document".into());
        assert_changed(changed);
        let mut changed = workspace.clone();
        changed.plan.as_mut().unwrap().characteristics[0].source.exact_text.push_str(" changed");
        assert_changed(changed);
        let mut changed = workspace.clone();
        changed.bindings.clear();
        assert_changed(changed);
        let mut changed = workspace.clone();
        changed.bindings.push(changed.bindings[0].clone());
        assert_changed(changed);
        let mut changed = workspace.clone();
        changed.registration_scope = None;
        assert_changed(changed);
        let mut changed = workspace.clone();
        changed.views.clear();
        assert_changed(changed);
        let mut changed = workspace.clone();
        changed.views[0].geometry_ids.clear();
        assert_changed(changed);
        let mut changed_record = record;
        changed_record.drawing_binding_id = Some("old-binding-id".into());
        assert!(assess_record(&changed_record, &workspace, &image, &frame, &[measured])
            .blocking_reasons.contains(&"drawing_binding_changed".into()));
    }

    #[test]
    fn imported_binding_cannot_restore_current_session_review() {
        let (image, workspace, frame, record, measured) = bound_fixture();
        let json = serde_json::to_value(&workspace).unwrap();
        assert_eq!(json["bindings"][0]["reviewed"], true);
        let imported: InspectionWorkspace = serde_json::from_value(json).unwrap();
        assert!(!imported.bindings[0].reviewed);
        // Keep the record fresh here to isolate the binding review guard from record import.
        let outcome = assess_record(&record, &imported, &image, &frame, &[measured]);
        assert_ne!(outcome.geometry_state, GeometryState::Validated);
        assert!(outcome.blocking_reasons.contains(&"drawing_binding_unreviewed".into()));
    }

    #[test]
    fn real_contour_uses_independent_scale_and_never_claims_qualified_conformity() {
        let image = disk();
        let workspace = workspace(&image, "Ø50 ±1 mm", 2.0);
        let record = circle(&workspace, &image, &frame(), 2.0);
        assert!((record.value_mm.unwrap() - 5.0).abs() < 0.1);
        assert!(record.result.evidence.accepted_points.len() >= 60);
        assert_eq!(
            record.outcome.conformity_status,
            ConformityStatus::Indeterminate
        );
        assert!(record
            .outcome
            .blocking_reasons
            .iter()
            .any(|reason| reason == "calibration_unqualified"));
        assert!(record
            .outcome
            .blocking_reasons
            .iter()
            .any(|reason| reason == "uncertainty_unknown"));
        assert!(!record.imported);
    }

    #[test]
    fn rejects_stale_localization_and_anisotropic_circle_conversion() {
        let image = disk();
        let mut workspace = workspace(&image, "Ø5 mm", 1.0);
        let mut frame = frame();
        frame.calibration.as_mut().unwrap().scale_y_mm_per_pixel = Some(0.2);
        let geometry = InspectionGeometry::Circle {
            center: Point { x: 64.0, y: 64.0 },
            radius: 25.0,
        };
        assert!(inspect_feature(
            &workspace,
            &image,
            &frame,
            "drawing-1",
            "group",
            &geometry,
            &VisionOptions::default()
        )
        .unwrap_err()
        .contains("anisotrope"));
        frame = self::frame();
        workspace.registration_capture_id = Some("old-capture".into());
        assert!(inspect_feature(
            &workspace,
            &image,
            &frame,
            "drawing-1",
            "group",
            &geometry,
            &VisionOptions::default()
        )
        .is_err());
    }

    #[test]
    fn a_line_support_does_not_invent_detected_endpoints_or_a_length() {
        let image = RgbImage::from_fn(128, 128, |_, y| Rgb([if y < 64 { 20 } else { 220 }; 3]));
        let workspace = workspace(&image, "Longueur 9 ±1 mm", 1.0);
        let record = inspect_feature(
            &workspace,
            &image,
            &frame(),
            "drawing-1",
            "group",
            &InspectionGeometry::Line {
                start: Point { x: 20.0, y: 64.0 },
                end: Point { x: 108.0, y: 64.0 },
            },
            &VisionOptions::default(),
        )
        .unwrap();
        assert!(record.value_mm.is_none());
        assert!(!record.result.evidence.endpoint_extent_verified);
        assert!(record
            .outcome
            .blocking_reasons
            .iter()
            .any(|reason| reason == "line_endpoints_unverified"));
        assert_ne!(record.outcome.geometry_state, GeometryState::Validated);
    }

    #[test]
    fn reassessment_rejects_changed_pixels_profile_points_and_deserialized_claims() {
        let image = disk();
        let workspace = workspace(&image, "Ø5 ±0,2 mm", 1.0);
        let frame = frame();
        let mut record = circle(&workspace, &image, &frame, 1.0);
        record.measurement_id = Some(1);
        record.measurement_revision = Some(0);
        let measured = measurement(&record);
        let original = assess_record(&record, &workspace, &image, &frame, &[measured.clone()]);
        assert_eq!(original.geometry_state, GeometryState::Validated);
        let mut changed_image = image.clone();
        changed_image.put_pixel(0, 0, Rgb([1, 2, 3]));
        assert!(assess_record(
            &record,
            &workspace,
            &changed_image,
            &frame,
            &[measured.clone()]
        )
        .blocking_reasons
        .contains(&"source_pixels_changed".into()));
        let mut changed_frame = frame.clone();
        changed_frame.calibration.as_mut().unwrap().mm_per_pixel = 0.2;
        assert!(assess_record(
            &record,
            &workspace,
            &image,
            &changed_frame,
            &[measured.clone()]
        )
        .blocking_reasons
        .contains(&"calibration_changed".into()));
        let mut changed_measurement = measured.clone();
        changed_measurement.points[0].x += 1.0;
        assert!(
            assess_record(&record, &workspace, &image, &frame, &[changed_measurement])
                .blocking_reasons
                .contains(&"measurement_geometry_changed".into())
        );
        let mut recalibrated_measurement = measured.clone();
        recalibrated_measurement.mm_per_pixel = Some(0.2);
        assert!(assess_record(
            &record,
            &workspace,
            &image,
            &frame,
            &[recalibrated_measurement]
        )
        .blocking_reasons
        .contains(&"measurement_calibration_changed".into()));
        let imported: InspectionRecord =
            serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        assert!(imported.imported);
        assert!(
            assess_record(&imported, &workspace, &image, &frame, &[measured])
                .blocking_reasons
                .contains(&"imported_evidence_requires_reassessment".into())
        );
    }

    #[test]
    fn batch_fingerprint_reassessment_matches_wrapper_and_rejects_changed_source() {
        let image = disk();
        let workspace = workspace(&image, "Ø5 ±0,2 mm", 1.0);
        let frame = frame();
        let mut record = circle(&workspace, &image, &frame, 1.0);
        record.measurement_id = Some(1);
        record.measurement_revision = Some(0);
        let measurements = [measurement(&record)];
        let expected = assess_record(&record, &workspace, &image, &frame, &measurements);
        assert_eq!(
            assess_record_with_fingerprint(
                &record,
                &workspace,
                &image,
                &frame,
                &measurements,
                fingerprint(&image)
            ),
            expected
        );
        let mut changed = image.clone();
        changed.put_pixel(0, 0, Rgb([1, 2, 3]));
        let actual = assess_record_with_fingerprint(
            &record,
            &workspace,
            &changed,
            &frame,
            &measurements,
            fingerprint(&changed),
        );
        assert_eq!(
            actual,
            assess_record(&record, &workspace, &changed, &frame, &measurements)
        );
        assert!(actual
            .blocking_reasons
            .contains(&"source_pixels_changed".into()));
        assert!(actual
            .blocking_reasons
            .contains(&"registration_missing_or_stale".into()));
    }

    #[test]
    fn noisy_arc_native_circle_preserves_robust_radius_without_algebraic_refit() {
        let image = RgbImage::from_fn(128, 128, |x, y| {
            let dx = x as f64 - 64.0;
            let dy = y as f64 - 64.0;
            let boundary = 25.0 + 0.25 * (9.0 * dy.atan2(dx)).sin();
            let intensity = (20.0 + 200.0 * (dx.hypot(dy) - boundary + 0.5).clamp(0.0, 1.0)) as u8;
            Rgb([intensity; 3])
        });
        let workspace = workspace(&image, "R2,5 ±0,2 mm", 1.0);
        let frame = frame();
        let mut record = inspect_feature(
            &workspace,
            &image,
            &frame,
            "drawing-1",
            "arc",
            &InspectionGeometry::Arc {
                center: Point { x: 64.0, y: 64.0 },
                radius: 25.0,
                start_rad: 0.1,
                sweep_rad: std::f64::consts::PI * 0.8,
            },
            &VisionOptions::default(),
        )
        .unwrap();
        let InspectionGeometry::Arc { center, radius, .. } = record.result.geometry else {
            panic!("An arc inspection must retain its observed arc geometry");
        };
        assert_eq!(record.measurement_points.len(), 2);
        assert_eq!(record.measurement_points[0], center);
        assert!(record.result.evidence.accepted_points.len() > 50);
        let native_radius = record.measurement_points[0].dist(record.measurement_points[1]);
        assert!((native_radius - radius).abs() < 1e-12);
        assert!((record.value_mm.unwrap() - radius * 0.1).abs() < 1e-12);
        record.measurement_id = Some(1);
        record.measurement_revision = Some(0);
        let native = measurement(&record);
        assert_eq!(native.kind, Tool::Circle);
        let native_diameter_mm = crate::reference::measurement_mm(&native).unwrap();
        assert!((native_diameter_mm / 2.0 - record.value_mm.unwrap()).abs() < 1e-12);
        let mut wrong_kind = native;
        wrong_kind.kind = Tool::CircleFit;
        assert!(
            assess_record(&record, &workspace, &image, &frame, &[wrong_kind])
                .blocking_reasons
                .contains(&"measurement_geometry_changed".into())
        );
    }
}

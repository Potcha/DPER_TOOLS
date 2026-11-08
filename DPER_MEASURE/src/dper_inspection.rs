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
    /// Current-session extraction only. Saved projects cannot restore it.
    #[serde(skip)]
    pub(crate) current_drawing_catalogs: Vec<crate::dper_pdf_geometry::DrawingGeometryCatalog>,
    pub bindings: Vec<crate::dper_binding::BindingSelection>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub occurrence_assignments: Vec<crate::dper_binding::OccurrenceAssignment>,
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

/// An operator-supplied, already established basis for this measurand. No
/// uncertainty is inferred from calibration U, drawing nominal or fit residuals.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct DecisionBasis {
    pub qualification_reference: String,
    pub qualification_scope: String,
    pub expanded_uncertainty_mm: f64,
    pub uncertainty_source: String,
    pub reviewer: String,
}

impl DecisionBasis {
    fn validate(&self) -> Result<(), String> {
        for (label, value, maximum) in [
            ("Référence de qualification", self.qualification_reference.as_str(), 500),
            ("Domaine de qualification", self.qualification_scope.as_str(), 2_000),
            ("Source de l'incertitude totale", self.uncertainty_source.as_str(), 2_000),
            ("Responsable de la revue", self.reviewer.as_str(), 200),
        ] {
            if value.trim().is_empty() || value.chars().count() > maximum {
                return Err(format!("{label} obligatoire, avec au maximum {maximum} caractères."));
            }
        }
        if !self.expanded_uncertainty_mm.is_finite() || self.expanded_uncertainty_mm <= 0.0 {
            return Err("L'incertitude élargie totale de cette caractéristique doit être finie et strictement positive, en mm.".into());
        }
        Ok(())
    }

    fn fingerprint(&self) -> Option<u64> {
        serde_json::to_vec(self).ok().map(hash_bytes)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InspectionDecisionEvidence {
    pub basis: DecisionBasis,
    pub capture_id: String,
    pub source_fingerprint: u64,
    pub calibration_fingerprint: u64,
    pub feature_fingerprint: String,
    pub plan_source_fingerprint: u64,
    pub localization_fingerprint: u64,
    pub measurement_id: u64,
    pub measurement_revision: u64,
    pub basis_fingerprint: u64,
    /// A saved claim never grants a new session the authority of native review.
    #[serde(skip_deserializing, default)]
    pub reviewed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClearanceProof {
    pub request: crate::dper_clearance::ClearanceRequest,
    pub result: crate::dper_clearance::ClearanceResult,
    #[serde(default)]
    pub sources: Vec<u64>,
    #[serde(default)]
    pub context: serde_json::Value,
    #[serde(default)]
    pub preview_revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InspectionRecord {
    pub feature_id: String,
    #[serde(default)]
    pub drawing_binding_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrenceProof>,
    pub group: String,
    pub capture_id: String,
    pub source_fingerprint: u64,
    pub calibration_id: Option<String>,
    /// Detects a changed profile even when its ID has been reused.
    #[serde(default)]
    pub calibration_fingerprint: Option<u64>,
    pub result: InspectionResult,
    /// A derived clearance circle is not a fitted circular contour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<ClearanceProof>,
    /// Actual search settings retained with observed pixels. Older history has
    /// unknown settings; never substitute today's defaults for missing evidence.
    #[serde(default)]
    pub vision_options: Option<VisionOptions>,
    pub value_mm: Option<f64>,
    #[serde(default)]
    pub decision_evidence: Option<InspectionDecisionEvidence>,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OccurrenceProof {
    pub member: crate::dper_binding::OccurrenceMember,
    pub assignment_fingerprint: String,
    pub view_id: String,
    pub preview_revision: u64,
    pub source_measurement_ids: Vec<u64>,
}

pub(crate) fn resolve_occurrence(
    workspace: &InspectionWorkspace,
    feature_id: &str,
    occurrence_id: &str,
    catalogs: &[crate::dper_pdf_geometry::DrawingGeometryCatalog],
) -> Result<(OccurrenceProof, Vec<InspectionGeometry>), String> {
    use crate::dper_binding;
    let mut assignments = workspace.occurrence_assignments.iter().filter(|item| item.feature_id == feature_id);
    let assignment = assignments.next().ok_or("Affectez d'abord les occurrences aux cercles du plan.")?;
    if assignments.next().is_some() || !assignment.selected_in_session {
        return Err("Affectation des occurrences ambiguë ou historique.".into());
    }
    let view = dper_binding::unique_view(&workspace.views, &assignment.view_id)?;
    let scope = DrawingRegistrationScope { document_fingerprint: assignment.document_fingerprint.clone(),
        page: assignment.page, view_id: view.id.clone() };
    if workspace.registration_scope.as_ref() != Some(&scope) {
        return Err("Établissez le recalage sur la vue des occurrences.".into());
    }
    let mut catalogs = catalogs.iter().filter(|item| item.geometry.page == scope.page
        && item.document_fingerprint == scope.document_fingerprint);
    let catalog = catalogs.next().ok_or("Catalogue courant de la vue absent.")?;
    if catalogs.next().is_some() {
        return Err("Catalogue de la vue ambigu.".into());
    }
    let plan = workspace.plan.as_ref().ok_or("Aucun plan actif.")?;
    if plan.source_fingerprint.as_deref() != Some(scope.document_fingerprint.as_str()) {
        return Err("Source de l'affectation modifiée.".into());
    }
    let ids = assignment.members.iter().map(|member| member.candidate_id.clone()).collect::<Vec<_>>();
    let regenerated = dper_binding::assign_occurrences(plan, &catalog.document_fingerprint, &catalog.geometry,
        view, feature_id, &ids, &assignment.justification)?;
    if &regenerated != assignment {
        return Err("Les sources de l'affectation ont changé.".into());
    }
    let member = assignment.members.iter().find(|member| member.id == occurrence_id)
        .ok_or("Occurrence absente de l'affectation courante.")?;
    let bytes = serde_json::to_vec(assignment).map_err(|error| error.to_string())?;
    let geometries = assignment.members.iter().map(|member| {
        let primitive = catalog.geometry.primitives.iter().find(|item| item.id == member.geometry_id)
            .ok_or("Cercle de l'occurrence absent du catalogue.")?;
        dper_binding::circle_geometry(primitive)
    }).collect::<Result<Vec<_>, _>>()?;
    Ok((OccurrenceProof { member: member.clone(), assignment_fingerprint: dper_binding::fingerprint_bytes(&bytes),
        view_id: view.id.clone(), preview_revision: 0, source_measurement_ids: Vec::new() }, geometries))
}

impl OccurrenceProof {
    pub(crate) fn same_assignment(&self, other: &Self) -> bool {
        self.member == other.member && self.assignment_fingerprint == other.assignment_fingerprint
            && self.view_id == other.view_id
    }
}

impl InspectionRecord {
    pub(crate) fn target_label(&self) -> String {
        match &self.occurrence {
            Some(proof) => format!("{} · occurrence {}", self.feature_id, proof.member.number),
            None => self.feature_id.clone(),
        }
    }
}

impl InspectionWorkspace {
    pub(crate) fn apply_dependency_flags(&self, measurements: &mut [Measurement]) {
        let stale_occurrences = self.records.iter().filter_map(|record| {
            let id = record.measurement_id?;
            (record.occurrence.is_some() && (record.imported
                || record.measurement_revision != Some(self.dependencies.revision(id))))
                .then_some((record.capture_id.as_str(), id))
        }).collect::<std::collections::BTreeSet<_>>();
        for measurement in measurements {
            measurement.dependency_stale = (!measurement.source_measurement_ids.is_empty()
                && self.dependencies.is_stale(measurement.id))
                || stale_occurrences.contains(&(measurement.capture_id.as_str(), measurement.id));
        }
    }
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

pub(crate) fn calibration_fingerprint(calibration: Option<&Calibration>) -> Option<u64> {
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
    catalogs: &[crate::dper_pdf_geometry::DrawingGeometryCatalog],
) -> Result<&'a RegistrationReport, String> {
    if let Some(scope) = &workspace.registration_scope {
        let plan = workspace.plan.as_ref().ok_or("Plan du recalage absent.")?;
        if plan.source_fingerprint.as_deref() != Some(scope.document_fingerprint.as_str()) {
            return Err("Source du recalage modifiée.".into());
        }
        let view = crate::dper_binding::unique_view(&workspace.views, &scope.view_id)?;
        let mut matches = catalogs.iter().filter(|catalog|
            catalog.geometry.page == scope.page && catalog.document_fingerprint == scope.document_fingerprint);
        let catalog = matches.next().ok_or("Catalogue courant du recalage absent.")?;
        if matches.next().is_some() || view.page != scope.page {
            return Err("Périmètre de recalage ambigu ou page incompatible.".into());
        }
        crate::dper_binding::validate_view(plan, &scope.document_fingerprint, &catalog.geometry, view)?;
    }
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

pub(crate) fn isotropic_scale(
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

fn confirmed_tolerance(candidate: &DrawingCharacteristic) -> Option<Tolerance> {
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

pub(crate) fn observation_suspension(record: &InspectionRecord, candidate: &DrawingCharacteristic) -> Option<&'static str> {
    crate::drawing_structure::suspension_reason(candidate).filter(|reason|
        *reason != "drawing_occurrences_unresolved" || record.occurrence.is_none())
}

pub(crate) fn observation_tolerance(record: &InspectionRecord, candidate: &DrawingCharacteristic) -> Option<Tolerance> {
    if observation_suspension(record, candidate).is_some() {
        return None;
    }
    confirmed_tolerance(candidate)
}

fn evaluate_record(
    record: &InspectionRecord,
    candidate: Option<&DrawingCharacteristic>,
    workspace: &InspectionWorkspace,
    frame: &FrameContext,
    current_value: Option<f64>,
) -> ValidationOutcome {
    let evidence = &record.result.evidence;
    let (decision_basis, decision_reason) = current_decision_basis(record, candidate, workspace, frame);
    let mut outcome = dper_metrology::evaluate(&ValidationInput {
        source_capture_id: record.capture_id.clone(),
        current_capture_id: frame.capture_id.clone(),
        calibration_id: record.calibration_id.clone(),
        calibration_qualified: decision_basis.is_some(),
        expanded_uncertainty: decision_basis.map(|basis| basis.expanded_uncertainty_mm),
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
        tolerance: candidate.and_then(|candidate| observation_tolerance(record, candidate)),
        measured_value: current_value,
        thresholds: ValidationThresholds::default(),
    });
    if let Some(reason) = decision_reason {
        add_reason(&mut outcome, reason, false);
    }
    if let Some(candidate) = candidate {
        let unresolved_structure = observation_suspension(record, candidate);
        if let Some(reason) = unresolved_structure {
            add_reason(&mut outcome, reason, true);
            outcome.conformity_status = ConformityStatus::Indeterminate;
        }
        if record.occurrence.is_some() && decision_basis.is_none() {
            add_reason(&mut outcome, "drawing_occurrence_group_unassessed", false);
            outcome.conformity_status = ConformityStatus::Indeterminate;
        }
        if candidate.has_unresolved_unit_context() {
            add_reason(&mut outcome, "drawing_unit_context_unresolved", false);
        }
        if !candidate.confirmed {
            add_reason(&mut outcome, "drawing_feature_unconfirmed", false);
        }
        if unresolved_structure.is_none() && candidate.confirmed && candidate.tolerance.is_some() && confirmed_tolerance(candidate).is_none() {
            add_reason(&mut outcome, "tolerance_unit_or_limits_unresolved", false);
        }
    }
    if matches!(record.result.geometry, InspectionGeometry::Line { .. }) {
        add_reason(&mut outcome, "line_endpoints_unverified", true);
    }
    outcome
}

fn current_decision_basis<'a>(
    record: &'a InspectionRecord,
    candidate: Option<&DrawingCharacteristic>,
    workspace: &InspectionWorkspace,
    frame: &FrameContext,
) -> (Option<&'a DecisionBasis>, Option<&'static str>) {
    let Some(decision) = &record.decision_evidence else { return (None, None); };
    if !decision.reviewed || record.imported {
        return (None, Some("decision_evidence_unreviewed"));
    }
    if decision.basis.validate().is_err() {
        return (None, Some("decision_evidence_invalid"));
    }
    let mut stored = workspace.records.iter().filter(|stored|
        stored.measurement_id == Some(decision.measurement_id));
    let observation_current = stored.next().is_some_and(|stored|
        observation_fingerprint(stored).is_some()
            && observation_fingerprint(stored) == observation_fingerprint(record))
        && stored.next().is_none();
    let feature_fingerprint = candidate.and_then(|feature|
        crate::dper_binding::feature_fingerprint(feature).ok());
    if !observation_current || decision.capture_id != record.capture_id || decision.capture_id != frame.capture_id
        || decision.source_fingerprint != record.source_fingerprint
        || Some(decision.calibration_fingerprint) != record.calibration_fingerprint
        || Some(decision.calibration_fingerprint) != calibration_fingerprint(frame.calibration.as_ref())
        || feature_fingerprint.as_deref() != Some(decision.feature_fingerprint.as_str())
        || plan_source_fingerprint(workspace) != Some(decision.plan_source_fingerprint)
        || localization_fingerprint(workspace) != Some(decision.localization_fingerprint)
        || record.measurement_id != Some(decision.measurement_id)
        || record.measurement_revision != Some(decision.measurement_revision)
        || decision.basis.fingerprint() != Some(decision.basis_fingerprint)
    {
        return (None, Some("decision_evidence_changed"));
    }
    (Some(&decision.basis), None)
}

fn plan_source_fingerprint(workspace: &InspectionWorkspace) -> Option<u64> {
    let plan = workspace.plan.as_ref()?;
    serde_json::to_vec(&(
        plan.schema_version, &plan.source_name, &plan.source_file, &plan.source_fingerprint,
        &plan.unit_context,
    )).ok().map(hash_bytes)
}

fn localization_fingerprint(workspace: &InspectionWorkspace) -> Option<u64> {
    serde_json::to_vec(&(
        &workspace.registration, &workspace.registration_scope,
        &workspace.registration_capture_id, &workspace.registration_fingerprint,
        &workspace.bindings, &workspace.views,
    )).ok().map(hash_bytes)
}

fn observation_fingerprint(record: &InspectionRecord) -> Option<u64> {
    let mut value = serde_json::to_value(record).ok()?;
    // Verdict and decision documentation may change after the observation;
    // every observation identity, sample and native association remains bound.
    let object = value.as_object_mut()?;
    object.remove("outcome");
    object.remove("decision_evidence");
    serde_json::to_vec(&value).ok().map(hash_bytes)
}

/// Called only after an explicit native operator review. The application checks
/// its current document revision and drawing file before calling this helper.
/// Fingerprints and generations are derived locally, never supplied by a client.
/// Before review, the application owns observation invalidation when registration
/// or drawing bindings change. This additional scope suspends the physical
/// decision if that localization context changes after the attestation.
pub(crate) fn attach_decision_evidence(
    record: &mut InspectionRecord,
    workspace: &InspectionWorkspace,
    image: &RgbImage,
    frame: &FrameContext,
    measurements: &[Measurement],
    basis: DecisionBasis,
) -> Result<(), String> {
    attach_decision_evidence_with_catalogs(record, workspace, image, frame, measurements,
        basis, &workspace.current_drawing_catalogs)
}

pub(crate) fn attach_decision_evidence_with_catalogs(
    record: &mut InspectionRecord, workspace: &InspectionWorkspace, image: &RgbImage,
    frame: &FrameContext, measurements: &[Measurement], mut basis: DecisionBasis,
    catalogs: &[crate::dper_pdf_geometry::DrawingGeometryCatalog],
) -> Result<(), String> {
    basis.qualification_reference = basis.qualification_reference.trim().into();
    basis.qualification_scope = basis.qualification_scope.trim().into();
    basis.uncertainty_source = basis.uncertainty_source.trim().into();
    basis.reviewer = basis.reviewer.trim().into();
    basis.validate()?;
    let candidate = feature(workspace, &record.feature_id)?;
    if let Some(reason) = observation_suspension(record, candidate) {
        return Err(dper_metrology::reason_label(reason).into());
    }
    if confirmed_tolerance(candidate).is_none() {
        return Err("Confirmez une exigence avec des limites de tolérance valides avant la revue de décision.".into());
    }
    let measurement_id = record.measurement_id.ok_or("Enregistrez une observation native avant la revue de décision.")?;
    if measurements.iter().filter(|measurement| measurement.id == measurement_id).count() != 1 {
        return Err("La mesure native doit être présente et unique.".into());
    }
    let mut stored = workspace.records.iter().filter(|stored| stored.measurement_id == Some(measurement_id));
    let stored_record = stored.next().ok_or("L'observation enregistrée est absente de l'inspection active.")?;
    if stored.next().is_some() || observation_fingerprint(stored_record).is_none()
        || observation_fingerprint(stored_record) != observation_fingerprint(record)
    {
        return Err("L'observation enregistrée est ambiguë ou a changé avant sa revue.".into());
    }
    let outcome = assess_record_with_catalogs(record, workspace, image, frame, measurements, fingerprint(image), catalogs);
    if record.imported || outcome.geometry_state != GeometryState::Validated
        || outcome.blocking_reasons.iter().any(|reason| !matches!(reason.as_str(),
            "calibration_unqualified" | "uncertainty_unknown"
                | "drawing_occurrence_group_unassessed"
                | "decision_evidence_unreviewed" | "decision_evidence_changed"
                | "decision_evidence_invalid" | "uncertainty_interval_overlaps_limit"))
    {
        return Err("Une observation actuelle, enregistrée et vérifiée par logiciel est requise ; réobservez toute preuve importée ou périmée.".into());
    }
    let calibration_fingerprint = calibration_fingerprint(frame.calibration.as_ref())
        .ok_or("Un étalonnage indépendant actif est requis.".to_string())?;
    let feature_fingerprint = crate::dper_binding::feature_fingerprint(candidate)?;
    let plan_source_fingerprint = plan_source_fingerprint(workspace)
        .ok_or("L'identité du plan actif ne peut pas être établie.".to_string())?;
    let localization_fingerprint = localization_fingerprint(workspace)
        .ok_or("Le contexte de recalage et d'association ne peut pas être identifié.".to_string())?;
    let basis_fingerprint = basis.fingerprint().ok_or("Impossible d'identifier la base de décision.".to_string())?;
    record.decision_evidence = Some(InspectionDecisionEvidence {
        basis, capture_id: frame.capture_id.clone(), source_fingerprint: fingerprint(image),
        calibration_fingerprint, feature_fingerprint, plan_source_fingerprint,
        localization_fingerprint, measurement_id,
        measurement_revision: workspace.dependencies.revision(measurement_id),
        basis_fingerprint, reviewed: true,
    });
    record.outcome = assess_record_with_catalogs(record, workspace, image, frame, measurements, fingerprint(image), catalogs);
    Ok(())
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

/// Decimal JSON parsing and a subsequent center + radius / unit conversion can
/// differ by a floating-point ULP. This allowance only avoids a false edit reason
/// for historical imports; imported evidence still cannot become validated.
/// Live observation identity and every source/calibration/generation gate stay exact.
fn recomputed_number_matches(stored: f64, recomputed: f64, imported: bool) -> bool {
    stored.is_finite() && recomputed.is_finite()
        && (stored == recomputed || (imported
            && (stored - recomputed).abs()
                <= 2.0 * f64::EPSILON * stored.abs().max(recomputed.abs())))
}

fn recomputed_points_match(stored: &[Point], recomputed: &[Point], imported: bool) -> bool {
    stored.len() == recomputed.len() && stored.iter().zip(recomputed).all(|(stored, recomputed)|
        recomputed_number_matches(stored.x, recomputed.x, imported)
            && recomputed_number_matches(stored.y, recomputed.y, imported))
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
    inspect_feature_with_catalogs(workspace, image, frame, feature_id, group,
        geometry_in_drawing, options, &workspace.current_drawing_catalogs)
}

pub(crate) fn inspect_feature_with_catalogs(
    workspace: &InspectionWorkspace, image: &RgbImage, frame: &FrameContext,
    feature_id: &str, group: &str, geometry_in_drawing: &InspectionGeometry,
    options: &VisionOptions, catalogs: &[crate::dper_pdf_geometry::DrawingGeometryCatalog],
) -> Result<InspectionRecord, String> {
    inspect_localized_with_catalogs(workspace, image, frame, feature_id, group, geometry_in_drawing, options, catalogs, None)
}

pub(crate) fn inspect_localized_with_catalogs(
    workspace: &InspectionWorkspace, image: &RgbImage, frame: &FrameContext,
    feature_id: &str, group: &str, geometry_in_drawing: &InspectionGeometry,
    options: &VisionOptions, catalogs: &[crate::dper_pdf_geometry::DrawingGeometryCatalog],
    occurrence: Option<(OccurrenceProof, Vec<InspectionGeometry>)>,
) -> Result<InspectionRecord, String> {
    if group.trim().is_empty() {
        return Err("Le groupe de contrôle doit être renseigné.".into());
    }
    if !frame.quality_pass {
        return Err("La qualité de l'image ne permet pas l'inspection.".into());
    }
    let candidate = feature(workspace, feature_id)?;
    if let Some(reason) = crate::drawing_structure::suspension_reason(candidate) {
        if reason != "drawing_occurrences_unresolved" || occurrence.is_none() {
            return Err(dper_metrology::reason_label(reason).into());
        }
    }
    check_geometry_type(candidate, geometry_in_drawing)?;
    let source_fingerprint = fingerprint(image);
    let report = registration_with_fingerprint(workspace, frame, source_fingerprint, catalogs)?;
    if !matches!(geometry_in_drawing, InspectionGeometry::Line { .. }) {
        isotropic_scale(frame.calibration.as_ref(), image)?;
    }
    let expected = project(report, geometry_in_drawing);
    if let Some((proof, siblings)) = &occurrence {
        let InspectionGeometry::Circle { center, radius } = expected else {
            return Err("Un cercle est requis pour observer l'occurrence.".into());
        };
        // Disjoint search disks, including the derivative and interpolation margins.
        for (index, sibling) in siblings.iter().enumerate() {
            if index + 1 == proof.member.number as usize { continue; }
            if let InspectionGeometry::Circle { center: other, radius: other_radius } = project(report, sibling) {
                let separation = (center.x - other.x).hypot(center.y - other.y);
                if separation <= radius + other_radius + 2.0 * (options.search_radius_px + 2.0) {
                    return Err("Les zones de recherche se recouvrent. Réduisez la largeur de recherche ou vérifiez le recalage.".into());
                }
            }
        }
    }
    let result = dper_vision::inspect_geometry(image, &expected, options)?;
    if occurrence.is_some() {
        if let (InspectionGeometry::Circle { center, radius }, InspectionGeometry::Circle { center: fitted, radius: fit_radius }) =
            (&expected, &result.geometry) {
            let shift = (center.x - fitted.x).hypot(center.y - fitted.y);
            if shift + (radius - fit_radius).abs() > options.search_radius_px + 1.0 {
                return Err("Le cercle ajusté s'écarte de la zone attribuée à cette occurrence.".into());
            }
        }
    }
    let value_mm = physical_value(candidate, &result, image, frame)?;
    let measurement_points = native_measurement_points(&result);
    let mut record = InspectionRecord {
        feature_id: feature_id.into(),
        drawing_binding_id: None,
        occurrence: occurrence.map(|(proof, _)| proof),
        group: group.trim().into(),
        capture_id: frame.capture_id.clone(),
        source_fingerprint,
        calibration_id: frame.calibration.as_ref().map(|profile| profile.id.clone()),
        calibration_fingerprint: calibration_fingerprint(frame.calibration.as_ref()),
        result,
        clearance: None,
        vision_options: Some(options.clone()),
        value_mm,
        decision_evidence: None,
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
    record.outcome = evaluate_record(&record, Some(candidate), workspace, frame, value_mm);
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
    assess_record_with_catalogs(record, workspace, image, frame, measurements,
        source_fingerprint, &workspace.current_drawing_catalogs)
}

/// Borrow live extraction data; never clone large catalogues for reassessment.
pub(crate) fn assess_record_with_catalogs(
    record: &InspectionRecord, workspace: &InspectionWorkspace, image: &RgbImage,
    frame: &FrameContext, measurements: &[Measurement], source_fingerprint: u64,
    current_catalogs: &[crate::dper_pdf_geometry::DrawingGeometryCatalog],
) -> ValidationOutcome {
    if record.clearance.is_some() {
        return crate::clearance_inspection::assess_clearance(record, workspace, image, frame, measurements);
    }
    let candidate = feature(workspace, &record.feature_id).ok();
    let current_value = candidate.and_then(|candidate| {
        physical_value(candidate, &record.result, image, frame)
            .ok()
            .flatten()
    });
    let mut outcome = evaluate_record(record, candidate, workspace, frame, current_value);
    if let Some(proof) = &record.occurrence {
        let valid = !record.imported && record.drawing_binding_id.is_none()
            && resolve_occurrence(workspace, &record.feature_id, &proof.member.id, current_catalogs)
                .is_ok_and(|(current, _)| proof.same_assignment(&current));
        if !valid {
            add_reason(&mut outcome, "drawing_occurrence_assignment_changed", true);
        }
    }
    if let Some(binding_id) = &record.drawing_binding_id {
        let mut selections = workspace.bindings.iter().filter(|selection| selection.candidate.id == *binding_id);
        let selected = selections.next();
        let unique_selection = selected.is_some() && selections.next().is_none()
            && workspace.bindings.iter().filter(|selection| selection.candidate.feature_id == record.feature_id).count() == 1;
        let binding_current = selected.is_some_and(|selection| {
            let binding = &selection.candidate;
            let view = crate::dper_binding::unique_view(&workspace.views, &binding.view_id).ok();
            let source_current = workspace.plan.as_ref().zip(view).is_some_and(|(plan, view)| {
                let catalogs: Vec<_> = current_catalogs.iter().filter(|catalog|
                    catalog.geometry.page == view.page && catalog.document_fingerprint == binding.document_fingerprint).collect();
                catalogs.len() == 1 && crate::dper_binding::resolve_binding(plan,
                    &catalogs[0].document_fingerprint, &catalogs[0].geometry, view, selection).is_ok()
            });
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
                && source_current
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
    if registration_with_fingerprint(workspace, frame, source_fingerprint, current_catalogs).is_err() {
        add_reason(&mut outcome, "registration_missing_or_stale", true);
    }
    let current_calibration_id = frame.calibration.as_ref().map(|profile| profile.id.clone());
    if record.calibration_id != current_calibration_id
        || record.calibration_fingerprint != calibration_fingerprint(frame.calibration.as_ref())
    {
        add_reason(&mut outcome, "calibration_changed", true);
    }
    let consistent_value = match (record.value_mm, current_value) {
        (Some(stored), Some(recomputed)) => recomputed_number_matches(stored, recomputed, record.imported),
        (None, None) => true,
        _ => false,
    };
    if !consistent_value {
        add_reason(&mut outcome, "physical_value_changed", true);
    }
    let consistent_points = recomputed_points_match(&record.measurement_points,
        &native_measurement_points(&record.result), record.imported);
    if !consistent_points {
        add_reason(&mut outcome, "inspection_points_changed", true);
    }
    let measured = record
        .measurement_id
        .and_then(|id| measurements.iter().find(|measurement| measurement.id == id));
    if record.measurement_id.is_some_and(|id|
        measurements.iter().filter(|measurement| measurement.id == id).count() > 1) {
        add_reason(&mut outcome, "measurement_identity_ambiguous", true);
    }
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

    mod decision_tests {
        use super::*;
        include!("inspection_decision_tests.rs");
    }

    mod roundtrip_tests {
        use super::*;
        include!("inspection_roundtrip_tests.rs");
    }

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
                correspondences: [Point { x: 10.0, y: 10.0 }, Point { x: 20.0, y: 10.0 }, Point { x: 10.0, y: 20.0 }]
                    .map(|plan| crate::dper_registration::Correspondence {
                        plan, image: Point { x: plan.x * scale, y: plan.y * scale },
                    }).to_vec(),
                residuals_px: vec![0.0; 3],
                threshold_px: Some(1.0),
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
        Measurement { drawing_variant: None,
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

    #[test]
    fn documentary_conflict_blocks_tolerance_even_when_feature_was_confirmed() {
        let image = disk();
        let frame = frame();
        let workspace = workspace(&image, "Units: mm / inches\nØ5 ±0.2", 1.0);
        let record = circle(&workspace, &image, &frame, 1.0);
        assert!(observation_tolerance(&record, &workspace.plan.as_ref().unwrap().characteristics[0]).is_none());
        assert!(record.outcome.blocking_reasons.iter().any(|reason| reason == "drawing_unit_context_unresolved"));
        assert_ne!(record.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Conforming);
        // Observing the physical contour remains possible without an invented nominal unit.
        assert!(record.value_mm.is_some());
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
        workspace.current_drawing_catalogs.push(crate::dper_pdf_geometry::DrawingGeometryCatalog {
            document_fingerprint: workspace.plan.as_ref().unwrap().source_fingerprint.clone().unwrap(),
            geometry: catalog,
        });
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
    fn drawing_structure_suspends_existing_observation_and_refuses_new_inspection() {
        let (image, workspace, frame, record, measured) = bound_fixture();
        assert_eq!(assess_record(&record, &workspace, &image, &frame, std::slice::from_ref(&measured)).geometry_state,
            GeometryState::Validated);
        for (repetition, reason) in [(Some(4), "drawing_occurrences_unresolved"), (None, "drawing_relation_unresolved")] {
            let mut changed = workspace.clone();
            let feature = &mut changed.plan.as_mut().unwrap().characteristics[0];
            feature.confirmed = true;
            feature.repetition = repetition;
            if repetition.is_none() {
                feature.relations.push(crate::dper_drawing::DrawingRelation {
                    kind: "datum".into(), target_id: "A".into(), source_text: "Référence A indiquée sur le plan".into(),
                });
            }
            assert!(observation_tolerance(&record, feature).is_none());
            let outcome = assess_record(&record, &changed, &image, &frame, std::slice::from_ref(&measured));
            assert_eq!(outcome.conformity_status, ConformityStatus::Indeterminate);
            assert!(outcome.blocking_reasons.iter().any(|code| code == reason));
            let refused = inspect_feature_with_catalogs(&changed, &image, &frame, &record.feature_id, "Groupe témoin",
                &record.result.geometry, &VisionOptions::default(), &changed.current_drawing_catalogs).unwrap_err();
            assert_eq!(refused, dper_metrology::reason_label(reason));
        }
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
    fn view_scope_registration_is_checked_without_a_binding_in_pure_observation_and_reassessment() {
        let (image, mut workspace, frame, mut record, measured) = bound_fixture();
        workspace.bindings.clear();
        record.drawing_binding_id = None;
        workspace.views[0].source_proof = None;
        let geometry = InspectionGeometry::Circle { center: Point { x: 64.0, y: 64.0 }, radius: 25.0 };
        assert!(inspect_feature(&workspace, &image, &frame, &record.feature_id,
            "Unbound stale scope", &geometry, &VisionOptions::default()).is_err());
        let stale = assess_record(&record, &workspace, &image, &frame, &[measured.clone()]);
        assert!(stale.blocking_reasons.contains(&"registration_missing_or_stale".into()));
        assert_ne!(stale.geometry_state, GeometryState::Validated);
        workspace.registration_scope = None;
        workspace.current_drawing_catalogs.clear();
        assert!(inspect_feature(&workspace, &image, &frame, &record.feature_id,
            "Unscoped mode", &geometry, &VisionOptions::default()).is_ok());
    }

    #[test]
    fn view_scope_proof_and_live_catalog_are_required_for_an_established_observation() {
        let (image, workspace, frame, record, measured) = bound_fixture();
        assert_eq!(assess_record(&record, &workspace, &image, &frame, &[measured.clone()]).geometry_state,
            GeometryState::Validated);
        for mutation in 0..5 {
            let mut changed = workspace.clone();
            match mutation {
                0 => changed.views[0].source_proof = None,
                1 => changed.current_drawing_catalogs.clear(),
                2 => changed.current_drawing_catalogs[0].geometry.flattening_tolerance_pt *= 2.0,
                3 => changed.views.push(changed.views[0].clone()),
                _ => changed.current_drawing_catalogs[0].geometry.primitives[0].source.path_data.push(' '),
            }
            let outcome = assess_record(&record, &changed, &image, &frame, &[measured.clone()]);
            assert_ne!(outcome.geometry_state, GeometryState::Validated, "mutation {mutation}");
            assert!(outcome.blocking_reasons.contains(&"drawing_binding_changed".into()));
            assert_eq!(outcome.conformity_status, ConformityStatus::Indeterminate);
        }
        let restored: InspectionWorkspace = serde_json::from_value(serde_json::to_value(&workspace).unwrap()).unwrap();
        assert!(restored.current_drawing_catalogs.is_empty());
        assert_ne!(assess_record(&record, &restored, &image, &frame, &[measured]).geometry_state, GeometryState::Validated);
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

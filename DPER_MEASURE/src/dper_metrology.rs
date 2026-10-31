//! Conservative software assessment of a proposed image-based measurement.
//!
//! This module does not detect contours or establish physical traceability. Callers must
//! calculate evidence from the current source pixels, never from a nominal drawing or a
//! repeated construction. Passing these gates means software validation only. It cannot
//! recover hidden faces, depth or a true dimension outside an independently justified 2D
//! measurement plane. Default thresholds are provisional, not an optical qualification.
//!
//! `measured_value`, tolerance bounds and `expanded_uncertainty` use the same measurand
//! unit. In particular, a calibration uncertainty in mm must not be reused as an angular
//! uncertainty in degrees. Fit residuals and their thresholds are in native image pixels.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryState {
    Proposed,
    Measured,
    Validated,
}

impl GeometryState {
    pub fn label(self) -> &'static str {
        match self { Self::Proposed => "proposée", Self::Measured => "mesurée, à vérifier", Self::Validated => "vérifiée par logiciel" }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformityStatus {
    Conforming,
    Nonconforming,
    Indeterminate,
    NotApplicable,
}

impl ConformityStatus {
    pub fn label(self) -> &'static str {
        match self { Self::Conforming => "conforme", Self::Nonconforming => "hors tolérance", Self::Indeterminate => "indéterminée", Self::NotApplicable => "non évaluée" }
    }
}

pub fn reason_label(reason: &str) -> &str {
    match reason {
        "measurement_revision_changed" => "mesure modifiée ou restaurée : nouvelle observation requise",
        "measurement_dependencies_changed" => "dépendances modifiées : recalcul requis",
        "calibration_missing" => "étalonnage absent",
        "calibration_unqualified" => "montage de mesure non qualifié",
        "uncertainty_unknown" => "incertitude totale non renseignée",
        "uncertainty_invalid" | "uncertainty_non_finite" => "incertitude invalide",
        "tolerance_missing" => "tolérance non confirmée ou absente",
        "tolerance_invalid" => "limites de tolérance invalides",
        "drawing_feature_unconfirmed" => "interprétation du plan à confirmer",
        "drawing_binding_unreviewed" => "association entre cote et géométrie du plan à vérifier",
        "drawing_binding_changed" => "association, vue ou sources du plan modifiées",
        "drawing_source_changed" => "fichier PDF source modifié ou indisponible",
        "measurement_missing" => "mesure native absente ou aperçu non enregistré",
        "image_quality_failed" => "qualité d'image insuffisante",
        "capture_mismatch" => "capture différente de la source observée",
        "not_observable_in_2d" => "caractéristique non vérifiable sur cette image 2D",
        "imported_evidence_requires_reassessment" => "preuves historiques à réévaluer sur l'image",
        "contour_fit_failed" => "écarts d'ajustement du contour trop importants",
        "contour_coverage_failed" => "couverture du contour insuffisante",
        "insufficient_observed_samples" => "nombre de points observés insuffisant",
        "geometry_evidence_missing" | "geometry_evidence_incomplete" | "geometry_evidence_invalid" => "preuves géométriques absentes ou insuffisantes",
        "thresholds_invalid" => "seuils de validation invalides",
        "source_pixels_changed" => "pixels source modifiés",
        "calibration_changed" | "measurement_calibration_changed" => "étalonnage modifié",
        "measurement_geometry_changed" | "inspection_points_changed" => "géométrie modifiée après observation",
        "physical_value_changed" => "valeur physique modifiée après observation",
        "registration_missing_or_stale" => "recalage absent ou périmé",
        "drawing_feature_missing_or_unobservable" => "caractéristique du plan absente ou non observable",
        "drawing_geometry_type_changed" => "type de caractéristique modifié",
        "line_endpoints_unverified" => "extrémités réelles non mesurées",
        other => other,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeometryEvidence {
    /// Number of independently observed contour/profile samples, not constructed points.
    pub sample_count: usize,
    pub fit_rms: Option<f64>,
    pub fit_max: Option<f64>,
    /// Fraction of the support required by the method, on a scale from zero to one.
    pub coverage: Option<f64>,
    /// Imported claims require fresh evaluation against the active image before validation.
    pub imported: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Tolerance {
    /// Optional drawing nominal; explicit MIN/MAX bounds need no nominal.
    /// Signed deviations may legitimately place it outside the acceptance interval.
    pub nominal: Option<f64>,
    /// Absolute acceptance bounds, not deviations from the nominal value.
    pub lower: f64,
    pub upper: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ValidationThresholds {
    pub min_samples: usize,
    pub max_fit_rms: f64,
    pub max_fit_max: f64,
    pub min_coverage: f64,
}

impl Default for ValidationThresholds {
    fn default() -> Self {
        Self { min_samples: 8, max_fit_rms: 0.5, max_fit_max: 1.5, min_coverage: 0.75 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationInput {
    pub source_capture_id: String,
    pub current_capture_id: String,
    pub calibration_id: Option<String>,
    /// Independent qualification for the current setup and measurement plane.
    pub calibration_qualified: bool,
    /// Expanded measurement uncertainty in the unit of `measured_value`; unknown is None.
    pub expanded_uncertainty: Option<f64>,
    pub quality_pass: bool,
    pub evidence: Option<GeometryEvidence>,
    pub observable_in_2d: bool,
    pub tolerance: Option<Tolerance>,
    pub measured_value: Option<f64>,
    pub thresholds: ValidationThresholds,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationOutcome {
    pub geometry_state: GeometryState,
    pub conformity_status: ConformityStatus,
    /// Stable reason codes, suitable for localized UI messages and MCP clients.
    pub blocking_reasons: Vec<String>,
}

/// Evaluate current evidence without mutating a session or trusting a saved state.
///
/// Geometry validation and conformity are separate: an uncertainty/calibration omission
/// blocks the conformity verdict even when current pixel evidence passes geometry gates.
/// A missing tolerance always yields `NotApplicable`, never conformity. The conservative
/// decision rule accepts only an uncertainty interval fully inside the bounds, rejects
/// only one fully outside, and leaves overlapping intervals indeterminate.
pub fn evaluate(input: &ValidationInput) -> ValidationOutcome {
    let mut reasons = Vec::<String>::new();
    let mut geometry_blocked = false;
    let mut conformity_blocked = false;
    let mut block_geometry = |reason: &str| {
        geometry_blocked = true;
        reasons.push(reason.to_owned());
    };

    if input.source_capture_id.trim().is_empty()
        || input.current_capture_id.trim().is_empty()
        || input.source_capture_id != input.current_capture_id
    {
        block_geometry("capture_mismatch");
    }
    if !input.quality_pass { block_geometry("image_quality_failed"); }
    if !input.observable_in_2d { block_geometry("not_observable_in_2d"); }
    if input.measured_value.is_none() { block_geometry("measurement_missing"); }
    if input.measured_value.is_some_and(|value| !value.is_finite()) {
        block_geometry("measurement_non_finite");
    }

    let thresholds = input.thresholds;
    let thresholds_valid = thresholds.min_samples > 0
        && thresholds.max_fit_rms.is_finite() && thresholds.max_fit_rms >= 0.0
        && thresholds.max_fit_max.is_finite() && thresholds.max_fit_max >= thresholds.max_fit_rms
        && thresholds.min_coverage.is_finite() && (0.0..=1.0).contains(&thresholds.min_coverage)
        && thresholds.min_coverage > 0.0;
    if !thresholds_valid { block_geometry("thresholds_invalid"); }

    match &input.evidence {
        None => block_geometry("geometry_evidence_missing"),
        Some(evidence) => {
            if evidence.imported { block_geometry("imported_evidence_requires_reassessment"); }
            if evidence.sample_count == 0 || evidence.sample_count < thresholds.min_samples {
                block_geometry("insufficient_observed_samples");
            }
            match (evidence.fit_rms, evidence.fit_max, evidence.coverage) {
                (Some(rms), Some(max), Some(coverage)) => {
                    if !rms.is_finite() || !max.is_finite() || !coverage.is_finite()
                        || rms < 0.0 || max < rms || !(0.0..=1.0).contains(&coverage)
                    {
                        block_geometry("geometry_evidence_invalid");
                    } else if thresholds_valid {
                        if rms > thresholds.max_fit_rms || max > thresholds.max_fit_max {
                            block_geometry("contour_fit_failed");
                        }
                        if coverage < thresholds.min_coverage { block_geometry("contour_coverage_failed"); }
                    }
                }
                _ => block_geometry("geometry_evidence_incomplete"),
            }
        }
    }

    // Invalid non-finite input must never allow a software-validated geometry either.
    if input.expanded_uncertainty.is_some_and(|value| !value.is_finite()) {
        block_geometry("uncertainty_non_finite");
    }
    if input.tolerance.is_some_and(|tolerance| {
        tolerance.nominal.is_some_and(|value| !value.is_finite())
            || !tolerance.lower.is_finite() || !tolerance.upper.is_finite()
            || tolerance.lower > tolerance.upper
    }) {
        block_geometry("tolerance_invalid");
    }

    if input.calibration_id.as_ref().is_none_or(|id| id.trim().is_empty()) {
        conformity_blocked = true;
        reasons.push("calibration_missing".into());
    }
    if !input.calibration_qualified {
        conformity_blocked = true;
        reasons.push("calibration_unqualified".into());
    }
    match input.expanded_uncertainty {
        None => {
            conformity_blocked = true;
            reasons.push("uncertainty_unknown".into());
        }
        Some(value) if !value.is_finite() || value <= 0.0 => {
            conformity_blocked = true;
            if value.is_finite() { reasons.push("uncertainty_invalid".into()); }
        }
        Some(_) => {}
    }

    let geometry_state = if geometry_blocked {
        if input.measured_value.is_some_and(f64::is_finite) { GeometryState::Measured }
        else { GeometryState::Proposed }
    } else { GeometryState::Validated };

    let conformity_status = match (input.tolerance, input.measured_value, input.expanded_uncertainty) {
        (None, _, _) => {
            reasons.push("tolerance_missing".into());
            ConformityStatus::NotApplicable
        }
        (Some(tolerance), Some(value), Some(uncertainty)) if !geometry_blocked && !conformity_blocked => {
            let interval_lower = value - uncertainty;
            let interval_upper = value + uncertainty;
            if !interval_lower.is_finite() || !interval_upper.is_finite() {
                reasons.push("uncertainty_interval_non_finite".into());
                ConformityStatus::Indeterminate
            } else if interval_lower >= tolerance.lower && interval_upper <= tolerance.upper {
                ConformityStatus::Conforming
            } else if interval_upper < tolerance.lower || interval_lower > tolerance.upper {
                ConformityStatus::Nonconforming
            } else {
                reasons.push("uncertainty_interval_overlaps_limit".into());
                ConformityStatus::Indeterminate
            }
        }
        _ => ConformityStatus::Indeterminate,
    };

    ValidationOutcome { geometry_state, conformity_status, blocking_reasons: reasons }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(value: f64, lower: f64, upper: f64, uncertainty: f64) -> ValidationInput {
        ValidationInput {
            source_capture_id: "capture-a".into(), current_capture_id: "capture-a".into(),
            calibration_id: Some("qualified-a".into()), calibration_qualified: true,
            expanded_uncertainty: Some(uncertainty), quality_pass: true,
            evidence: Some(GeometryEvidence { sample_count: 24, fit_rms: Some(0.2),
                fit_max: Some(0.6), coverage: Some(0.9), imported: false }),
            observable_in_2d: true, tolerance: Some(Tolerance { nominal: Some((lower + upper) / 2.0), lower, upper }),
            measured_value: Some(value), thresholds: ValidationThresholds::default(),
        }
    }

    #[test]
    fn scalar_rule_handles_lengths_diameters_and_angles_in_their_own_units() {
        for case in [input(0.12, 0.10, 0.14, 0.005), input(120.0, 119.0, 121.0, 0.2),
            input(8.0, 7.9, 8.1, 0.02), input(90.0, 89.0, 91.0, 0.1)] {
            let outcome = evaluate(&case);
            assert_eq!(outcome.geometry_state, GeometryState::Validated);
            assert_eq!(outcome.conformity_status, ConformityStatus::Conforming);
            assert!(outcome.blocking_reasons.is_empty());
        }
    }

    #[test]
    fn guard_band_keeps_boundary_overlap_indeterminate_and_rejects_disjoint_intervals() {
        assert_eq!(evaluate(&input(10.0, 9.0, 11.0, 1.0)).conformity_status, ConformityStatus::Conforming);
        for value in [9.0, 11.0, 8.9, 11.1] {
            assert_eq!(evaluate(&input(value, 9.0, 11.0, 0.2)).conformity_status, ConformityStatus::Indeterminate);
        }
        for value in [8.0, 12.0] {
            assert_eq!(evaluate(&input(value, 9.0, 11.0, 0.2)).conformity_status, ConformityStatus::Nonconforming);
        }
    }

    #[test]
    fn missing_uncertainty_and_unqualified_calibration_block_conformity_only() {
        let mut case = input(10.0, 9.0, 11.0, 0.2);
        case.expanded_uncertainty = None;
        assert_eq!(evaluate(&case).geometry_state, GeometryState::Validated);
        assert_eq!(evaluate(&case).conformity_status, ConformityStatus::Indeterminate);
        case.expanded_uncertainty = Some(0.2);
        case.calibration_qualified = false;
        assert_eq!(evaluate(&case).conformity_status, ConformityStatus::Indeterminate);
        case.calibration_qualified = true;
        case.calibration_id = None;
        assert_eq!(evaluate(&case).conformity_status, ConformityStatus::Indeterminate);
    }

    #[test]
    fn drawn_or_imported_geometry_cannot_validate_itself() {
        let mut case = input(10.0, 9.0, 11.0, 0.2);
        case.evidence = None;
        assert_eq!(evaluate(&case).geometry_state, GeometryState::Measured);
        assert_eq!(evaluate(&case).conformity_status, ConformityStatus::Indeterminate);
        case.evidence = Some(GeometryEvidence { sample_count: 0, fit_rms: Some(0.0),
            fit_max: Some(0.0), coverage: Some(1.0), imported: false });
        assert_ne!(evaluate(&case).geometry_state, GeometryState::Validated);
        case.evidence.as_mut().unwrap().sample_count = 24;
        case.evidence.as_mut().unwrap().imported = true;
        assert_ne!(evaluate(&case).geometry_state, GeometryState::Validated);
    }

    #[test]
    fn misleading_contour_and_small_exact_arc_do_not_pass() {
        let mut case = input(10.0, 9.0, 11.0, 0.2);
        case.evidence.as_mut().unwrap().fit_max = Some(3.0);
        assert!(evaluate(&case).blocking_reasons.iter().any(|reason| reason == "contour_fit_failed"));
        case.evidence.as_mut().unwrap().fit_max = Some(0.6);
        case.evidence.as_mut().unwrap().coverage = Some(0.15);
        assert!(evaluate(&case).blocking_reasons.iter().any(|reason| reason == "contour_coverage_failed"));
    }

    #[test]
    fn missing_capture_quality_or_2d_observability_prevents_validation() {
        let mut case = input(10.0, 9.0, 11.0, 0.2);
        case.source_capture_id = "old-capture".into();
        assert_ne!(evaluate(&case).geometry_state, GeometryState::Validated);
        case.source_capture_id = case.current_capture_id.clone();
        case.quality_pass = false;
        assert_ne!(evaluate(&case).geometry_state, GeometryState::Validated);
        case.quality_pass = true;
        case.observable_in_2d = false;
        assert_ne!(evaluate(&case).geometry_state, GeometryState::Validated);
    }

    #[test]
    fn non_finite_values_and_invalid_intervals_never_validate() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut case = input(10.0, 9.0, 11.0, 0.2);
            case.measured_value = Some(bad);
            assert_ne!(evaluate(&case).geometry_state, GeometryState::Validated);
            case.measured_value = Some(10.0);
            case.expanded_uncertainty = Some(bad);
            assert_ne!(evaluate(&case).geometry_state, GeometryState::Validated);
            case.expanded_uncertainty = Some(0.2);
            case.evidence.as_mut().unwrap().fit_rms = Some(bad);
            assert_ne!(evaluate(&case).geometry_state, GeometryState::Validated);
            case.evidence.as_mut().unwrap().fit_rms = Some(0.2);
            case.tolerance.as_mut().unwrap().nominal = Some(bad);
            assert_ne!(evaluate(&case).geometry_state, GeometryState::Validated);
        }
        let mut case = input(10.0, 9.0, 11.0, 0.2);
        case.tolerance.as_mut().unwrap().upper = 8.0;
        assert_eq!(evaluate(&case).conformity_status, ConformityStatus::Indeterminate);
    }

    #[test]
    fn absent_tolerance_never_produces_a_verdict_and_missing_value_stays_proposed() {
        let mut case = input(10.0, 9.0, 11.0, 0.2);
        case.tolerance = None;
        assert_eq!(evaluate(&case).conformity_status, ConformityStatus::NotApplicable);
        case.measured_value = None;
        assert_eq!(evaluate(&case).geometry_state, GeometryState::Proposed);
    }

    #[test]
    fn explicit_min_max_limits_do_not_require_a_nominal() {
        let mut case = input(10.15, 10.1, 10.2, 0.01);
        case.tolerance.as_mut().unwrap().nominal = None;
        let outcome = evaluate(&case);
        assert_eq!(outcome.geometry_state, GeometryState::Validated);
        assert_eq!(outcome.conformity_status, ConformityStatus::Conforming);
        assert!(outcome.blocking_reasons.is_empty());
        case.measured_value = Some(10.3);
        assert_eq!(evaluate(&case).conformity_status, ConformityStatus::Nonconforming);
    }

    #[test]
    fn signed_deviations_may_place_nominal_outside_acceptance_bounds() {
        let mut case = input(10.15, 10.1, 10.2, 0.01);
        case.tolerance.as_mut().unwrap().nominal = Some(10.0);
        let outcome = evaluate(&case);
        assert_eq!(outcome.geometry_state, GeometryState::Validated);
        assert_eq!(outcome.conformity_status, ConformityStatus::Conforming);
        assert!(outcome.blocking_reasons.is_empty());
        case.measured_value = Some(10.0);
        assert_eq!(evaluate(&case).conformity_status, ConformityStatus::Nonconforming);
    }

    #[test]
    fn uncertainty_interval_overflow_is_indeterminate() {
        let case = input(f64::MAX, 1.0, f64::MAX, f64::MAX);
        assert_eq!(evaluate(&case).conformity_status, ConformityStatus::Indeterminate);
    }
}

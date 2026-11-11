//! First-order propagation for a circular arc with explicit input uncertainties.

use crate::correlated_uncertainty::{CorrelatedUncertaintyBudget, CorrelatedUncertaintyResult};
use crate::distance_uncertainty::UncertainIsotropicScale;
use crate::uncertainty::{SensitivityComponent, UncertaintyUnit};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArcLengthUncertaintyRequest {
    pub radius_px: f64,
    pub standard_uncertainty_radius_px: f64,
    pub radius_source: String,
    pub sweep_deg: f64,
    pub standard_uncertainty_sweep_deg: f64,
    pub sweep_source: String,
    pub scale: UncertainIsotropicScale,
    pub coverage_factor: f64,
    /// Complete matrix in radius_px, sweep_deg, relative_scale order.
    pub correlation_matrix: Vec<Vec<f64>>,
    pub correlation_justification: String,
}

#[derive(Debug, Serialize)]
pub struct ArcLengthUncertaintyResult {
    pub request: ArcLengthUncertaintyRequest,
    pub arc_length_px: f64,
    pub arc_length_mm: f64,
    pub formula: &'static str,
    pub input_order: [&'static str; 3],
    pub budget: CorrelatedUncertaintyResult,
    pub status: &'static str,
    pub applied_to_measurement: bool,
    pub source_qualification_verified: bool,
    pub conformity_evaluated: bool,
    pub input_provenance_verified: bool,
    pub limitations: [&'static str; 2],
}

impl ArcLengthUncertaintyRequest {
    pub fn calculate(&self) -> Result<ArcLengthUncertaintyResult, String> {
        let radius = self.radius_px;
        let scale = self.scale.millimeters_per_pixel;
        if !radius.is_finite() || radius <= 0.0 || !scale.is_finite() || scale <= 0.0 {
            return Err("Le rayon et l’échelle isotrope doivent être finis et strictement positifs.".into());
        }
        if !(0.0 < self.sweep_deg && self.sweep_deg < 360.0) {
            return Err("L’ouverture doit être strictement comprise entre 0 et 360 degrés.".into());
        }

        let radians_per_degree = 1.0_f64.to_radians();
        let sweep_rad = self.sweep_deg.to_radians();
        let arc_length_px = radius * sweep_rad;
        let arc_length_mm = arc_length_px * scale;
        let radius_sensitivity = sweep_rad * scale;
        let sweep_sensitivity = (radius * radians_per_degree) * scale;
        if [arc_length_px, arc_length_mm, radius_sensitivity, sweep_sensitivity]
            .iter()
            .any(|&value| !value.is_finite() || value <= 0.0)
        {
            return Err("La longueur ou ses sensibilités dépassent les limites numériques.".into());
        }

        let input_order = ["radius_px", "sweep_deg", "relative_scale"];
        let budget = CorrelatedUncertaintyBudget {
            output_unit: UncertaintyUnit::Millimeter,
            coverage_factor: self.coverage_factor,
            correlation_justification: self.correlation_justification.clone(),
            components: vec![
                SensitivityComponent {
                    name: input_order[0].into(),
                    input_unit: UncertaintyUnit::Pixel,
                    standard_uncertainty: self.standard_uncertainty_radius_px,
                    sensitivity_coefficient: radius_sensitivity,
                    source: self.radius_source.clone(),
                },
                SensitivityComponent {
                    name: input_order[1].into(),
                    input_unit: UncertaintyUnit::Degree,
                    standard_uncertainty: self.standard_uncertainty_sweep_deg,
                    sensitivity_coefficient: sweep_sensitivity,
                    source: self.sweep_source.clone(),
                },
                SensitivityComponent {
                    name: input_order[2].into(),
                    input_unit: UncertaintyUnit::Dimensionless,
                    standard_uncertainty: self.scale.relative_standard_uncertainty,
                    sensitivity_coefficient: arc_length_mm,
                    source: self.scale.source.clone(),
                },
            ],
            correlation_matrix: self.correlation_matrix.clone(),
        }
        .calculate()?;

        Ok(ArcLengthUncertaintyResult {
            request: self.clone(),
            arc_length_px,
            arc_length_mm,
            formula: "L = r * theta_deg * (pi/180) * s",
            input_order,
            budget,
            status: "calculation_only",
            applied_to_measurement: false,
            source_qualification_verified: false,
            conformity_evaluated: false,
            input_provenance_verified: false,
            limitations: [
                "Propagation au premier ordre avec échelle isotrope et ouverture non signée fournie. Distorsion, anisotropie et effets non linéaires hors modèle.",
                "Rayon, ouverture et incertitudes déclarés par l’appelant. La proximité des bornes 0° et 360° au regard de l’incertitude n’est pas évaluée.",
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(rho: f64) -> ArcLengthUncertaintyRequest {
        serde_json::from_str(&format!(r#"{{
            "radius_px": 100, "standard_uncertainty_radius_px": 1, "radius_source": "Rayon",
            "sweep_deg": 90, "standard_uncertainty_sweep_deg": 0.9, "sweep_source": "Ouverture",
            "scale": {{"millimeters_per_pixel": 0.01, "relative_standard_uncertainty": 0.01, "source": "Échelle"}},
            "coverage_factor": 2,
            "correlation_matrix": [[1,{rho},0],[{rho},1,0],[0,0,1]],
            "correlation_justification": "Données synthétiques"
        }}"#)).unwrap()
    }

    #[test]
    fn arc_length_uncertainty_propagates_the_three_inputs() {
        let quarter_turn = std::f64::consts::FRAC_PI_2;
        for (rho, variance_factor) in [(0.0, 3.0_f64), (0.5, 4.0), (-0.5, 2.0)] {
            let request = example(rho);
            let result = request.calculate().unwrap();
            assert_eq!(result.request, request);
            assert!((result.arc_length_px - 100.0 * quarter_turn).abs() < 1e-12);
            assert!((result.arc_length_mm - quarter_turn).abs() < 1e-14);
            assert_eq!(result.input_order, ["radius_px", "sweep_deg", "relative_scale"]);
            assert_eq!(result.budget.components[1].input_unit, UncertaintyUnit::Degree);
            let expected = 2.0 * quarter_turn * 0.01 * variance_factor.sqrt();
            assert!((result.budget.expanded_uncertainty - expected).abs() < 1e-14);
            assert_eq!(result.status, "calculation_only");
            assert!(!result.applied_to_measurement && !result.source_qualification_verified);
            assert!(!result.conformity_evaluated && !result.input_provenance_verified);
        }
    }

    #[test]
    fn arc_length_uncertainty_rejects_invalid_openings_and_accepts_major_arcs() {
        for sweep in [-90.0, 0.0, 360.0, 361.0, f64::NAN, f64::INFINITY] {
            let mut request = example(0.0);
            request.sweep_deg = sweep;
            assert!(request.calculate().is_err());
        }
        let mut request = example(0.0);
        request.sweep_deg = 270.0;
        assert!((request.calculate().unwrap().arc_length_mm - 1.5 * std::f64::consts::PI).abs() < 1e-14);
    }

    #[test]
    fn arc_length_uncertainty_rejects_invalid_geometry_and_budget() {
        let mut request = example(0.0);
        request.radius_px = f64::MAX;
        request.sweep_deg = 270.0;
        assert!(request.calculate().is_err());

        let mut request = example(0.0);
        request.scale.millimeters_per_pixel = 0.0;
        assert!(request.calculate().is_err());

        let mut request = example(0.0);
        request.standard_uncertainty_sweep_deg = -0.1;
        assert!(request.calculate().is_err());

        let mut request = example(0.0);
        request.sweep_source.clear();
        assert!(request.calculate().is_err());

        assert!(example(1.01).calculate().is_err());
    }
}

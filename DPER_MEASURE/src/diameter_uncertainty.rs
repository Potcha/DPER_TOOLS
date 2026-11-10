//! First-order propagation for a supplied radius and isotropic scale.

use crate::correlated_uncertainty::{CorrelatedUncertaintyBudget, CorrelatedUncertaintyResult};
use crate::distance_uncertainty::UncertainIsotropicScale;
use crate::uncertainty::{SensitivityComponent, UncertaintyUnit};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiameterUncertaintyRequest {
    pub radius_px: f64,
    pub standard_uncertainty_radius_px: f64,
    pub radius_source: String,
    pub scale: UncertainIsotropicScale,
    pub coverage_factor: f64,
    /// Complete 2 by 2 matrix in radius_px, relative_scale order.
    pub correlation_matrix: Vec<Vec<f64>>,
    pub correlation_justification: String,
}

#[derive(Debug, Serialize)]
pub struct DiameterUncertaintyResult {
    pub request: DiameterUncertaintyRequest,
    pub diameter_px: f64,
    pub diameter_mm: f64,
    pub formula: &'static str,
    pub input_order: [&'static str; 2],
    pub budget: CorrelatedUncertaintyResult,
    pub status: &'static str,
    pub applied_to_measurement: bool,
    pub source_qualification_verified: bool,
    pub conformity_evaluated: bool,
    pub input_provenance_verified: bool,
    pub limitations: [&'static str; 2],
}

impl DiameterUncertaintyRequest {
    pub fn calculate(&self) -> Result<DiameterUncertaintyResult, String> {
        if !self.radius_px.is_finite() || self.radius_px <= 0.0 {
            return Err("Le rayon fourni en pixels doit être fini et strictement positif.".into());
        }
        let scale = self.scale.millimeters_per_pixel;
        if !scale.is_finite() || scale <= 0.0 {
            return Err("L'échelle isotrope doit être finie et strictement positive en mm/pixel.".into());
        }
        let diameter_px = 2.0 * self.radius_px;
        let diameter_mm = diameter_px * scale;
        let radius_sensitivity = 2.0 * scale;
        if !diameter_px.is_finite()
            || !diameter_mm.is_finite()
            || diameter_mm == 0.0
            || !radius_sensitivity.is_finite()
        {
            return Err("Le diamètre ou ses sensibilités dépassent les limites numériques.".into());
        }

        let input_order = ["radius_px", "relative_scale"];
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
                    input_unit: UncertaintyUnit::Dimensionless,
                    standard_uncertainty: self.scale.relative_standard_uncertainty,
                    sensitivity_coefficient: diameter_mm,
                    source: self.scale.source.clone(),
                },
            ],
            correlation_matrix: self.correlation_matrix.clone(),
        }
        .calculate()?;

        Ok(DiameterUncertaintyResult {
            request: self.clone(),
            diameter_px,
            diameter_mm,
            formula: "D = 2*r*s ; c = [2*s, D] pour une perturbation fractionnaire de s.",
            input_order,
            budget,
            status: "calculation_only",
            applied_to_measurement: false,
            source_qualification_verified: false,
            conformity_evaluated: false,
            input_provenance_verified: false,
            limitations: [
                "Propagation au premier ordre avec échelle isotrope. Anisotropie, distorsion et termes d'ordre supérieur hors modèle.",
                "Le rayon et son incertitude sont fournis séparément. Aucun ajustement de contour, aucune estimation d'incertitude depuis les résidus et aucune qualification des entrées.",
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(rho: f64) -> DiameterUncertaintyRequest {
        DiameterUncertaintyRequest {
            radius_px: 200.0,
            standard_uncertainty_radius_px: 0.2,
            radius_source: "Rayon synthétique".into(),
            scale: UncertainIsotropicScale {
                millimeters_per_pixel: 0.01,
                relative_standard_uncertainty: 0.01,
                source: "Échelle synthétique".into(),
            },
            coverage_factor: 2.0,
            correlation_matrix: vec![vec![1.0, rho], vec![rho, 1.0]],
            correlation_justification: "Modèle de corrélation synthétique".into(),
        }
    }

    #[test]
    fn diameter_uncertainty_propagates_both_contributions_and_their_correlation() {
        for (rho, variance) in [(0.0, 0.001616_f64), (0.5, 0.001776), (-0.5, 0.001456)] {
            let request = example(rho);
            let result = request.calculate().unwrap();
            assert_eq!(result.request, request);
            assert_eq!(result.diameter_px, 400.0);
            assert_eq!(result.diameter_mm, 4.0);
            assert_eq!(result.input_order, ["radius_px", "relative_scale"]);
            let budget = result.budget;
            assert_eq!(budget.output_unit, UncertaintyUnit::Millimeter);
            assert_eq!(budget.components[0].input_unit, UncertaintyUnit::Pixel);
            assert_eq!(budget.components[1].input_unit, UncertaintyUnit::Dimensionless);
            assert_eq!(budget.components[0].sensitivity_coefficient, 0.02);
            assert_eq!(budget.components[1].sensitivity_coefficient, 4.0);
            assert!((budget.combined_standard_uncertainty - variance.sqrt()).abs() < 1e-14);
            assert!((budget.expanded_uncertainty - 2.0 * variance.sqrt()).abs() < 1e-14);
            assert_eq!(result.status, "calculation_only");
            assert!(!result.applied_to_measurement && !result.source_qualification_verified);
            assert!(!result.conformity_evaluated && !result.input_provenance_verified);
        }
    }

    #[test]
    fn diameter_uncertainty_rejects_invalid_geometry_and_numeric_range() {
        for radius in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            let mut request = example(0.0);
            request.radius_px = radius;
            assert!(request.calculate().is_err());
        }
        for scale in [0.0, -1.0, f64::NAN, f64::MAX] {
            let mut request = example(0.0);
            request.scale.millimeters_per_pixel = scale;
            assert!(request.calculate().is_err());
        }
        let mut request = example(0.0);
        request.radius_px = f64::MIN_POSITIVE;
        request.scale.millimeters_per_pixel = f64::MIN_POSITIVE;
        assert!(request.calculate().is_err());
    }

    #[test]
    fn diameter_uncertainty_rejects_invalid_budget_and_exact_cancellation() {
        let mut invalid_matrix = example(0.5);
        invalid_matrix.correlation_matrix[1][0] = 0.0;
        assert!(invalid_matrix.calculate().is_err());
        let mut negative_uncertainty = example(0.0);
        negative_uncertainty.standard_uncertainty_radius_px = -0.2;
        assert!(negative_uncertainty.calculate().is_err());
        let mut cancellation = example(-1.0);
        cancellation.standard_uncertainty_radius_px = 2.0;
        assert!(cancellation.calculate().is_err());
    }

    #[test]
    fn diameter_uncertainty_rejects_nominal_and_nested_authority_fields() {
        let mut json = serde_json::to_value(example(0.0)).unwrap();
        json["nominal_mm"] = serde_json::json!(4);
        assert!(serde_json::from_value::<DiameterUncertaintyRequest>(json).is_err());
        let mut json = serde_json::to_value(example(0.0)).unwrap();
        json["scale"]["qualified"] = serde_json::json!(true);
        assert!(serde_json::from_value::<DiameterUncertaintyRequest>(json).is_err());
    }
}

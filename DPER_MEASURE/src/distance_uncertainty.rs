//! First-order propagation for an explicitly supplied isotropic image scale.
//! This module never reads or changes a measurement, calibration or session.
use crate::correlated_uncertainty::{CorrelatedUncertaintyBudget, CorrelatedUncertaintyResult};
use crate::geometry::Point;
use crate::uncertainty::{SensitivityComponent, UncertaintyUnit};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

// geometry::Point is shared by other APIs; keep strict input handling local.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct StrictPoint {
    x: f64,
    y: f64,
}

fn deserialize_point<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Point, D::Error> {
    let point = StrictPoint::deserialize(deserializer)?;
    Ok(Point { x: point.x, y: point.y })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UncertainImagePoint {
    #[serde(deserialize_with = "deserialize_point")]
    #[schemars(with = "StrictPoint")]
    pub position_px: Point,
    pub standard_uncertainty_x_px: f64,
    pub standard_uncertainty_y_px: f64,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UncertainIsotropicScale {
    pub millimeters_per_pixel: f64,
    /// Dimensionless standard uncertainty of the relative scale perturbation.
    pub relative_standard_uncertainty: f64,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DistanceUncertaintyRequest {
    pub first_point: UncertainImagePoint,
    pub second_point: UncertainImagePoint,
    pub scale: UncertainIsotropicScale,
    pub coverage_factor: f64,
    /// Complete 5 by 5 matrix in x1, y1, x2, y2, relative_scale order.
    pub correlation_matrix: Vec<Vec<f64>>,
    pub correlation_justification: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DistanceUncertaintyResult {
    pub request: DistanceUncertaintyRequest,
    pub distance_px: f64,
    pub distance_mm: f64,
    pub model: String,
    pub formula: String,
    pub input_order: [String; 5],
    pub budget: CorrelatedUncertaintyResult,
    pub status: String,
    pub applied_to_measurement: bool,
    pub source_qualification_verified: bool,
    pub conformity_evaluated: bool,
    pub input_provenance_verified: bool,
    pub limitations: Vec<String>,
}

impl DistanceUncertaintyRequest {
    pub fn calculate(&self) -> Result<DistanceUncertaintyResult, String> {
        for point in [&self.first_point.position_px, &self.second_point.position_px] {
            if [point.x, point.y].iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
                return Err("Les coordonnées doivent être finies et comprises entre -1e12 et 1e12 pixels.".into());
            }
        }
        let s = self.scale.millimeters_per_pixel;
        if !s.is_finite() || s <= 0.0 {
            return Err("L'échelle isotrope explicite doit être finie et strictement positive en mm/pixel.".into());
        }
        let dx = self.second_point.position_px.x - self.first_point.position_px.x;
        let dy = self.second_point.position_px.y - self.first_point.position_px.y;
        let distance_px = dx.hypot(dy);
        if !distance_px.is_finite() || distance_px <= 1e-9 {
            return Err("La distance doit être strictement supérieure à 1e-9 pixel pour cette propagation au premier ordre.".into());
        }
        let distance_mm = s * distance_px;
        // Divide first: s*dx can overflow even when s*(dx/r) is representable.
        let cx = s * (dx / distance_px);
        let cy = s * (dy / distance_px);
        if !distance_mm.is_finite() || distance_mm == 0.0
            || !cx.is_finite() || !cy.is_finite()
            || (dx != 0.0 && cx == 0.0) || (dy != 0.0 && cy == 0.0)
        {
            return Err("La distance ou ses dérivées dépassent les limites numériques (débordement ou sous-flux).".into());
        }
        let input_order = ["x1", "y1", "x2", "y2", "relative_scale"].map(String::from);
        let coefficients = [-cx, -cy, cx, cy, distance_mm];
        let uncertainties = [
            self.first_point.standard_uncertainty_x_px,
            self.first_point.standard_uncertainty_y_px,
            self.second_point.standard_uncertainty_x_px,
            self.second_point.standard_uncertainty_y_px,
            self.scale.relative_standard_uncertainty,
        ];
        let sources = [&self.first_point.source, &self.first_point.source,
            &self.second_point.source, &self.second_point.source, &self.scale.source];
        let budget = CorrelatedUncertaintyBudget {
            output_unit: UncertaintyUnit::Millimeter,
            coverage_factor: self.coverage_factor,
            correlation_justification: self.correlation_justification.clone(),
            components: (0..5).map(|i| SensitivityComponent {
                name: input_order[i].clone(),
                input_unit: if i == 4 { UncertaintyUnit::Dimensionless } else { UncertaintyUnit::Pixel },
                standard_uncertainty: uncertainties[i],
                sensitivity_coefficient: coefficients[i],
                source: sources[i].clone(),
            }).collect(),
            correlation_matrix: self.correlation_matrix.clone(),
        }.calculate()?;
        Ok(DistanceUncertaintyResult {
            request: self.clone(), distance_px, distance_mm,
            model: "Distance euclidienne avec échelle isotrope explicite ; propagation au premier ordre.".into(),
            formula: "L = s * hypot(x2-x1, y2-y1) ; c = [-s*dx/r, -s*dy/r, s*dx/r, s*dy/r, L] ; relative_scale est une perturbation fractionnaire sans dimension.".into(),
            input_order, budget, status: "calculation_only".into(),
            applied_to_measurement: false,
            source_qualification_verified: false,
            conformity_evaluated: false,
            input_provenance_verified: false,
            limitations: vec![
                "Linéarisation au premier ordre uniquement ; validité de cette approximation non évaluée, notamment près d'une distance nulle.".into(),
                "Échelle isotrope uniquement ; aucune anisotropie, distorsion optique ou autre erreur non fournie n'est modélisée.".into(),
                "Valeurs, sources et corrélations fournies par l'appelant ; aucune qualification cachée, vérification de provenance ou qualification métrologique déduite.".into(),
                "Calcul isolé, non appliqué à une mesure ; aucune lecture de calibration/session ni décision de conformité. Les limites numériques du budget corrélé s'appliquent.".into(),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> DistanceUncertaintyRequest {
        let point = |x, y, source: &str| UncertainImagePoint {
            position_px: Point { x, y }, standard_uncertainty_x_px: 0.2,
            standard_uncertainty_y_px: 0.2, source: source.into(),
        };
        DistanceUncertaintyRequest {
            first_point: point(0.0, 0.0, "Point A fourni"),
            second_point: point(30.0, 40.0, "Point B fourni"),
            scale: UncertainIsotropicScale { millimeters_per_pixel: 0.01,
                relative_standard_uncertainty: 0.01, source: "Échelle fournie".into() },
            coverage_factor: 2.0,
            correlation_matrix: (0..5).map(|i| (0..5).map(|j| if i == j { 1.0 } else { 0.0 }).collect()).collect(),
            correlation_justification: "Entrées indépendantes selon le protocole fourni".into(),
        }
    }

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-14, "{actual} != {expected}");
    }

    #[test]
    fn analytic_distance_signed_sensitivities_and_units() {
        let result = request().calculate().unwrap();
        close(result.distance_px, 50.0);
        close(result.distance_mm, 0.5);
        close(result.budget.combined_standard_uncertainty, 0.000033_f64.sqrt());
        close(result.budget.expanded_uncertainty, 2.0 * 0.000033_f64.sqrt());
        for (i, expected) in [-0.006, -0.008, 0.006, 0.008, 0.5].iter().enumerate() {
            let component = &result.budget.components[i];
            close(component.sensitivity_coefficient, *expected);
            assert_eq!(component.input_unit, if i == 4 { UncertaintyUnit::Dimensionless } else { UncertaintyUnit::Pixel });
            assert_eq!(component.contribution_unit, UncertaintyUnit::Millimeter);
        }
    }

    #[test]
    fn coordinate_correlation_preserves_signed_propagation() {
        let mut input = request();
        input.correlation_matrix[0][2] = 0.5;
        input.correlation_matrix[2][0] = 0.5;
        // 2 * rho * (-0.006*0.2) * (0.006*0.2) = -1.44e-6 mm².
        close(input.calculate().unwrap().budget.combined_standard_uncertainty.powi(2), 31.56e-6);
    }

    #[test]
    fn reversal_rotation_and_translation_preserve_isotropic_independent_result() {
        let input = request();
        let expected = input.calculate().unwrap();
        let mut reversed = input.clone();
        std::mem::swap(&mut reversed.first_point, &mut reversed.second_point);
        let mut rotated = input.clone();
        rotated.first_point.position_px = Point { x: 200.0, y: -80.0 };
        rotated.second_point.position_px = Point { x: 160.0, y: -50.0 };
        for transformed in [reversed, rotated] {
            let actual = transformed.calculate().unwrap();
            close(actual.distance_mm, expected.distance_mm);
            close(actual.budget.combined_standard_uncertainty, expected.budget.combined_standard_uncertainty);
        }
    }

    #[test]
    fn perfectly_correlated_translation_cancels_positions_but_retains_scale() {
        let mut input = request();
        for i in 0..4 { for j in 0..4 { input.correlation_matrix[i][j] = 1.0; } }
        close(input.calculate().unwrap().budget.combined_standard_uncertainty, 0.005);
    }

    #[test]
    fn rejects_degenerate_out_of_range_and_nonfinite_geometry() {
        for (x, y) in [(0.0, 0.0), (1e-9, 0.0), (f64::NAN, 1.0),
            (f64::INFINITY, 1.0), (1e12 + 1.0, 1.0)] {
            let mut input = request();
            input.second_point.position_px = Point { x, y };
            assert!(input.calculate().is_err());
        }
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX, f64::from_bits(1)] {
            let mut input = request();
            input.scale.millimeters_per_pixel = value;
            assert!(input.calculate().is_err());
        }
    }

    #[test]
    fn rejects_uncertainties_coverage_and_missing_source_text() {
        for value in [-0.1, f64::NAN, f64::INFINITY] {
            for field in 0..5 {
                let mut input = request();
                match field {
                    0 => input.first_point.standard_uncertainty_x_px = value,
                    1 => input.first_point.standard_uncertainty_y_px = value,
                    2 => input.second_point.standard_uncertainty_x_px = value,
                    3 => input.second_point.standard_uncertainty_y_px = value,
                    _ => input.scale.relative_standard_uncertainty = value,
                }
                assert!(input.calculate().is_err());
            }
        }
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut input = request(); input.coverage_factor = value;
            assert!(input.calculate().is_err());
        }
        for field in 0..4 {
            let mut input = request();
            match field {
                0 => input.first_point.source = " ".into(),
                1 => input.second_point.source = " ".into(),
                2 => input.scale.source = " ".into(),
                _ => input.correlation_justification = " ".into(),
            }
            assert!(input.calculate().is_err());
        }
    }

    #[test]
    fn rejects_invalid_complete_matrices_and_indefinite_correlations() {
        for field in 0..5 {
            let mut input = request();
            match field {
                0 => { input.correlation_matrix.pop(); },
                1 => input.correlation_matrix[0][0] = 0.99,
                2 => input.correlation_matrix[0][1] = 0.1,
                3 => input.correlation_matrix[0][1] = f64::NAN,
                _ => {
                    for (i, j, value) in [(0, 1, 0.9), (0, 2, 0.9), (1, 2, -0.9)] {
                        input.correlation_matrix[i][j] = value;
                        input.correlation_matrix[j][i] = value;
                    }
                }
            }
            assert!(input.calculate().is_err());
        }
    }

    #[test]
    fn snapshots_sources_and_explicitly_disclaims_authority() {
        let input = request();
        let result = input.calculate().unwrap();
        assert_eq!(result.request, input);
        assert_eq!(result.budget.components[0].source, input.first_point.source);
        assert_eq!(result.budget.components[2].source, input.second_point.source);
        assert_eq!(result.budget.components[4].source, input.scale.source);
        assert_eq!(result.status, "calculation_only");
        assert!(!result.applied_to_measurement && !result.source_qualification_verified
            && !result.conformity_evaluated && !result.input_provenance_verified);
        assert!(result.limitations.iter().any(|s| s.contains("premier ordre")));
        assert!(result.limitations.iter().any(|s| s.contains("distorsion")));
    }

    #[test]
    fn strict_request_schema_rejects_injected_authority_including_nested_points() {
        let original = serde_json::to_value(request()).unwrap();
        for path in [vec![], vec!["scale"], vec!["first_point"], vec!["second_point", "position_px"]] {
            let mut value = original.clone();
            let mut target = &mut value;
            for key in path { target = &mut target[key]; }
            target.as_object_mut().unwrap().insert("source_qualification_verified".into(), true.into());
            assert!(serde_json::from_value::<DistanceUncertaintyRequest>(value).is_err());
        }
        let schema = serde_json::to_value(schemars::schema_for!(DistanceUncertaintyRequest)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["$defs"]["StrictPoint"]["additionalProperties"], false);
    }
}

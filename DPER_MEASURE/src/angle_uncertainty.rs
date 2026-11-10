//! Isolated first-order propagation for the unsigned minor three-point angle.
//! No measurement, calibration or session is read or changed.
use crate::correlated_uncertainty::{CorrelatedUncertaintyBudget, CorrelatedUncertaintyResult};
use crate::distance_uncertainty::UncertainImagePoint;
use crate::uncertainty::{SensitivityComponent, UncertaintyUnit};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AngleUncertaintyRequest {
    pub first_point: UncertainImagePoint,
    pub vertex: UncertainImagePoint,
    pub second_point: UncertainImagePoint,
    pub coverage_factor: f64,
    /// Complete matrix in x1, y1, xv, yv, x2, y2 order.
    pub correlation_matrix: Vec<Vec<f64>>,
    pub correlation_justification: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AngleUncertaintyResult {
    pub request: AngleUncertaintyRequest,
    pub angle_degrees: f64,
    pub first_ray_length_px: f64,
    pub second_ray_length_px: f64,
    pub model: String,
    pub formula: String,
    pub input_order: [String; 6],
    pub budget: CorrelatedUncertaintyResult,
    pub status: String,
    pub applied_to_measurement: bool,
    pub source_qualification_verified: bool,
    pub conformity_evaluated: bool,
    pub input_provenance_verified: bool,
    pub limitations: Vec<String>,
}

impl AngleUncertaintyRequest {
    pub fn calculate(&self) -> Result<AngleUncertaintyResult, String> {
        for point in [&self.first_point, &self.vertex, &self.second_point] {
            if [point.position_px.x, point.position_px.y]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1e12)
            {
                return Err(
                    "Les coordonnées doivent être finies et comprises entre -1e12 et 1e12 pixels."
                        .into(),
                );
            }
        }
        let ax = self.first_point.position_px.x - self.vertex.position_px.x;
        let ay = self.first_point.position_px.y - self.vertex.position_px.y;
        let bx = self.second_point.position_px.x - self.vertex.position_px.x;
        let by = self.second_point.position_px.y - self.vertex.position_px.y;
        let ra = ax.hypot(ay);
        let rb = bx.hypot(by);
        if ra <= 1e-9 || rb <= 1e-9 || !ra.is_finite() || !rb.is_finite() {
            return Err("Chaque rayon doit être strictement supérieur à 1e-9 pixel.".into());
        }
        // Coordinate and ray bounds keep these products finite. atan2 avoids
        // acos's loss of resolution close to a straight or zero angle.
        let cross = ax * by - ay * bx;
        let dot = ax * bx + ay * by;
        // A small relative margin accounts for rounding in both differences
        // and products. It is a numerical guard, not a metrology threshold.
        if cross.abs() <= 8.0 * f64::EPSILON * ra * rb {
            return Err("L'angle mineur est indiscernable d'un angle colinéaire à la résolution numérique disponible.".into());
        }
        let angle_degrees = cross.abs().atan2(dot).to_degrees();
        if !angle_degrees.is_finite() || angle_degrees <= 0.0 || angle_degrees >= 180.0 {
            return Err("L'angle est indiscernable de 0 ou 180 degrés à la résolution numérique disponible.".into());
        }
        let orientation = cross.signum();
        let degrees = 180.0 / std::f64::consts::PI;
        let a2 = ra * ra;
        let b2 = rb * rb;
        // For theta = |arg(b)-arg(a)| locally, multiply the signed-angle
        // derivatives by sign(cross). Vertex motion changes both rays.
        let c1x = orientation * (ay / a2) * degrees;
        let c1y = -orientation * (ax / a2) * degrees;
        let c2x = -orientation * (by / b2) * degrees;
        let c2y = orientation * (bx / b2) * degrees;
        let coefficients = [c1x, c1y, -(c1x + c2x), -(c1y + c2y), c2x, c2y];
        if coefficients.iter().any(|c| !c.is_finite())
            || [(ay, c1x), (ax, c1y), (by, c2x), (bx, c2y)]
                .iter()
                .any(|(coordinate, coefficient)| *coordinate != 0.0 && *coefficient == 0.0)
        {
            return Err("Les dérivées de l'angle dépassent les limites numériques (débordement ou sous-flux).".into());
        }
        let input_order = ["x1", "y1", "xv", "yv", "x2", "y2"].map(String::from);
        let uncertainties = [
            self.first_point.standard_uncertainty_x_px,
            self.first_point.standard_uncertainty_y_px,
            self.vertex.standard_uncertainty_x_px,
            self.vertex.standard_uncertainty_y_px,
            self.second_point.standard_uncertainty_x_px,
            self.second_point.standard_uncertainty_y_px,
        ];
        let sources = [
            &self.first_point.source,
            &self.first_point.source,
            &self.vertex.source,
            &self.vertex.source,
            &self.second_point.source,
            &self.second_point.source,
        ];
        let budget = CorrelatedUncertaintyBudget {
            output_unit: UncertaintyUnit::Degree,
            coverage_factor: self.coverage_factor,
            correlation_justification: self.correlation_justification.clone(),
            components: (0..6)
                .map(|i| SensitivityComponent {
                    name: input_order[i].clone(),
                    input_unit: UncertaintyUnit::Pixel,
                    standard_uncertainty: uncertainties[i],
                    sensitivity_coefficient: coefficients[i],
                    source: sources[i].clone(),
                })
                .collect(),
            correlation_matrix: self.correlation_matrix.clone(),
        }
        .calculate()?;
        Ok(AngleUncertaintyResult {
            request: self.clone(), angle_degrees, first_ray_length_px: ra, second_ray_length_px: rb,
            model: "Angle mineur non signé de trois points ; propagation au premier ordre en degrés.".into(),
            formula: "a=P1-V, b=P2-V ; theta=atan2(|ax*by-ay*bx|, ax*bx+ay*by)*180/pi ; h=sign(ax*by-ay*bx) ; c1=h*(ay,-ax)/|a|²*180/pi ; c2=h*(-by,bx)/|b|²*180/pi ; cv=-(c1+c2).".into(),
            input_order, budget, status: "calculation_only".into(),
            applied_to_measurement: false, source_qualification_verified: false,
            conformity_evaluated: false, input_provenance_verified: false,
            limitations: vec![
                "Linéarisation au premier ordre uniquement ; validité de l'approximation non évaluée, notamment près de rayons nuls ou d'angles colinéaires.".into(),
                "Une échelle isotrope commune s'annule dans l'angle ; aucune échelle de calibration n'est lue ou demandée. Anisotropie et distorsion optique non modélisées.".into(),
                "Coordonnées, sources et corrélations fournies par l'appelant ; aucune qualification cachée ni vérification de provenance ou qualification métrologique déduite.".into(),
                "Calcul isolé, non appliqué à une mesure ; aucune lecture de session ni décision de conformité. Les limites numériques du budget corrélé s'appliquent.".into(),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn request() -> AngleUncertaintyRequest {
        let point = |x, y, source: &str| UncertainImagePoint {
            position_px: Point { x, y },
            standard_uncertainty_x_px: 0.2,
            standard_uncertainty_y_px: 0.2,
            source: source.into(),
        };
        AngleUncertaintyRequest {
            first_point: point(10.0, 0.0, "Premier point fourni"),
            vertex: point(0.0, 0.0, "Sommet fourni"),
            second_point: point(0.0, 20.0, "Second point fourni"),
            coverage_factor: 2.0,
            correlation_matrix: (0..6)
                .map(|i| (0..6).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
                .collect(),
            correlation_justification: "Indépendance selon le protocole fourni".into(),
        }
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() < tolerance,
            "{actual} != {expected}"
        );
    }

    fn coordinate(input: &mut AngleUncertaintyRequest, i: usize) -> &mut f64 {
        match i {
            0 => &mut input.first_point.position_px.x,
            1 => &mut input.first_point.position_px.y,
            2 => &mut input.vertex.position_px.x,
            3 => &mut input.vertex.position_px.y,
            4 => &mut input.second_point.position_px.x,
            _ => &mut input.second_point.position_px.y,
        }
    }

    #[test]
    fn analytic_right_angle_signed_coefficients_and_degree_budget() {
        let result = request().calculate().unwrap();
        let d = 180.0 / std::f64::consts::PI;
        close(result.angle_degrees, 90.0, 1e-12);
        close(result.first_ray_length_px, 10.0, 1e-12);
        close(result.second_ray_length_px, 20.0, 1e-12);
        for (component, expected) in result.budget.components.iter().zip([
            0.0,
            -d / 10.0,
            d / 20.0,
            d / 10.0,
            -d / 20.0,
            0.0,
        ]) {
            close(component.sensitivity_coefficient, expected, 1e-12);
            assert_eq!(component.input_unit, UncertaintyUnit::Pixel);
            assert_eq!(component.contribution_unit, UncertaintyUnit::Degree);
        }
        // u² = .2² * (2/10²+2/20²) * (180/pi)².
        let uc = (0.04 * 0.025_f64).sqrt() * d;
        close(result.budget.combined_standard_uncertainty, uc, 1e-12);
        close(result.budget.expanded_uncertainty, 2.0 * uc, 1e-12);
    }

    #[test]
    fn all_six_analytical_derivatives_match_finite_differences_both_orientations() {
        for sign in [-1.0, 1.0] {
            let mut input = request();
            input.first_point.position_px = Point { x: 13.0, y: 4.0 };
            input.vertex.position_px = Point { x: 1.0, y: -2.0 };
            input.second_point.position_px = Point {
                x: -5.0,
                y: sign * 17.0,
            };
            let result = input.calculate().unwrap();
            for i in 0..6 {
                let mut plus = input.clone();
                let mut minus = input.clone();
                *coordinate(&mut plus, i) += 1e-5;
                *coordinate(&mut minus, i) -= 1e-5;
                let finite_difference = (plus.calculate().unwrap().angle_degrees
                    - minus.calculate().unwrap().angle_degrees)
                    / 2e-5;
                close(
                    result.budget.components[i].sensitivity_coefficient,
                    finite_difference,
                    2e-8,
                );
            }
        }
    }

    #[test]
    fn rotation_translation_and_isotropic_scaling_preserve_propagation() {
        let input = request();
        let expected = input.calculate().unwrap();
        let mut transformed = input.clone();
        for point in [
            &mut transformed.first_point,
            &mut transformed.vertex,
            &mut transformed.second_point,
        ] {
            let p = point.position_px;
            point.position_px = Point {
                x: 100.0 - 3.0 * p.y,
                y: -50.0 + 3.0 * p.x,
            };
            point.standard_uncertainty_x_px *= 3.0;
            point.standard_uncertainty_y_px *= 3.0;
        }
        let actual = transformed.calculate().unwrap();
        close(actual.angle_degrees, expected.angle_degrees, 1e-12);
        close(
            actual.budget.combined_standard_uncertainty,
            expected.budget.combined_standard_uncertainty,
            1e-12,
        );
        let mut reversed = input;
        std::mem::swap(&mut reversed.first_point, &mut reversed.second_point);
        close(
            reversed.calculate().unwrap().angle_degrees,
            expected.angle_degrees,
            1e-12,
        );
    }

    #[test]
    fn coordinate_correlations_keep_signs_and_cancel_common_translation() {
        let mut input = request();
        input.correlation_matrix[1][3] = 0.5;
        input.correlation_matrix[3][1] = 0.5;
        let d = 180.0 / std::f64::consts::PI;
        close(
            input
                .calculate()
                .unwrap()
                .budget
                .combined_standard_uncertainty
                .powi(2),
            0.0006 * d * d,
            1e-12,
        );
        // Perfect common translation gives exactly zero angle uncertainty,
        // which the existing correlated engine refuses instead of certifying.
        input.correlation_matrix = (0..6)
            .map(|i| {
                (0..6)
                    .map(|j| if i % 2 == j % 2 { 1.0 } else { 0.0 })
                    .collect()
            })
            .collect();
        assert!(input.calculate().is_err());
    }

    #[test]
    fn rejects_zero_tiny_and_collinear_rays() {
        let e = f64::EPSILON / 2.0;
        let mut collinear = request();
        collinear.vertex.position_px = Point { x: e, y: 3.0 * e };
        collinear.first_point.position_px = Point { x: 1.0, y: 3.0 };
        collinear.second_point.position_px = Point { x: 0.75, y: 2.25 };
        assert!(collinear.calculate().unwrap_err().contains("colinéaire"));
        for (x, y) in [(0.0, 0.0), (1e-9, 0.0), (0.0, 10.0), (0.0, -10.0)] {
            let mut input = request();
            input.first_point.position_px = Point { x, y };
            assert!(input.calculate().is_err());
        }
        let mut input = request();
        input.second_point.position_px = input.vertex.position_px;
        assert!(input.calculate().is_err());
    }

    #[test]
    fn rejects_nonfinite_out_of_bounds_and_invalid_uncertainties() {
        for i in 0..6 {
            for value in [f64::NAN, f64::INFINITY, 1e12 + 1.0, -1e12 - 1.0] {
                let mut input = request();
                *coordinate(&mut input, i) = value;
                assert!(input.calculate().is_err());
            }
        }
        for value in [-0.1, f64::NAN, f64::INFINITY] {
            for i in 0..6 {
                let mut input = request();
                let point = match i / 2 {
                    0 => &mut input.first_point,
                    1 => &mut input.vertex,
                    _ => &mut input.second_point,
                };
                if i % 2 == 0 {
                    point.standard_uncertainty_x_px = value;
                } else {
                    point.standard_uncertainty_y_px = value;
                }
                assert!(input.calculate().is_err());
            }
        }
    }

    #[test]
    fn rejects_invalid_matrix_coverage_and_source_text() {
        for i in 0..9 {
            let mut input = request();
            match i {
                0 => {
                    input.correlation_matrix.pop();
                }
                1 => input.correlation_matrix[0][1] = 0.5,
                2 => input.correlation_matrix[0][0] = 0.99,
                3 => input.coverage_factor = 0.0,
                4 => input.coverage_factor = f64::NAN,
                5 => input.first_point.source = " ".into(),
                6 => input.vertex.source = " ".into(),
                7 => input.second_point.source = " ".into(),
                _ => input.correlation_justification = " ".into(),
            }
            assert!(input.calculate().is_err());
        }
    }

    #[test]
    fn retains_input_sources_and_explicit_calculation_only_authority() {
        let input = request();
        let result = input.calculate().unwrap();
        assert_eq!(result.request, input);
        assert_eq!(result.budget.components[0].source, input.first_point.source);
        assert_eq!(result.budget.components[2].source, input.vertex.source);
        assert_eq!(
            result.budget.components[4].source,
            input.second_point.source
        );
        assert_eq!(result.status, "calculation_only");
        assert!(
            !result.applied_to_measurement
                && !result.source_qualification_verified
                && !result.conformity_evaluated
                && !result.input_provenance_verified
        );
        assert!(result
            .limitations
            .iter()
            .any(|s| s.contains("validité") && s.contains("non évaluée")));
        assert!(result
            .limitations
            .iter()
            .any(|s| s.contains("Anisotropie") && s.contains("distorsion")));
    }

    #[test]
    fn strict_schema_and_json_reject_scale_and_authority_injection() {
        let original = serde_json::to_value(request()).unwrap();
        for (path, field) in [
            (vec![], "scale"),
            (vec![], "conformity_evaluated"),
            (vec!["vertex"], "source_qualification_verified"),
            (vec!["first_point", "position_px"], "calibration"),
        ] {
            let mut value = original.clone();
            let mut target = &mut value;
            for key in path {
                target = &mut target[key];
            }
            target
                .as_object_mut()
                .unwrap()
                .insert(field.into(), true.into());
            assert!(serde_json::from_value::<AngleUncertaintyRequest>(value).is_err());
        }
        let schema = serde_json::to_value(schemars::schema_for!(AngleUncertaintyRequest)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["$defs"]["StrictPoint"]["additionalProperties"],
            false
        );
        assert!(schema["properties"].get("scale").is_none());
    }
}

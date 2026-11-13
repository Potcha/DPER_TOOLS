//! Bind existing propagation engines to application-owned native geometry.
//! The caller verifies the observation, capture and dependency revisions first.
//! This module calculates evidence only; it never qualifies or changes a result.

use crate::{
    correlated_uncertainty::CorrelatedUncertaintyResult,
    distance_uncertainty::{UncertainImagePoint, UncertainIsotropicScale},
    geometry::{self, Measurement, Point, Tool},
    storage::Calibration,
    uncertainty::UncertaintyUnit,
};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PointUncertainty {
    pub standard_uncertainty_x_px: f64,
    pub standard_uncertainty_y_px: f64,
    pub source: String,
}

impl PointUncertainty {
    fn at(&self, position_px: Point) -> UncertainImagePoint {
        UncertainImagePoint {
            position_px,
            standard_uncertainty_x_px: self.standard_uncertainty_x_px,
            standard_uncertainty_y_px: self.standard_uncertainty_y_px,
            source: self.source.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScaleUncertainty {
    pub relative_standard_uncertainty: f64,
    pub source: String,
}

impl ScaleUncertainty {
    fn at(&self, millimeters_per_pixel: f64) -> UncertainIsotropicScale {
        UncertainIsotropicScale {
            millimeters_per_pixel,
            relative_standard_uncertainty: self.relative_standard_uncertainty,
            source: self.source.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PropagationParameters {
    pub coverage_factor: f64,
    /// Complete matrix in the corresponding existing engine's input order.
    pub correlation_matrix: Vec<Vec<f64>>,
    pub correlation_justification: String,
}

/// Only uncertainty parameters are client inputs. No coordinates, radius,
/// angle, calibration value or qualification assertion can be supplied here.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ObservationUncertaintyInput {
    Distance {
        points: [PointUncertainty; 2],
        scale: ScaleUncertainty,
        propagation: PropagationParameters,
    },
    Angle {
        /// First ray point, vertex, second ray point, in native measurement order.
        points: [PointUncertainty; 3],
        propagation: PropagationParameters,
    },
    Diameter {
        standard_uncertainty_radius_px: f64,
        radius_source: String,
        scale: ScaleUncertainty,
        propagation: PropagationParameters,
    },
    ArcLength {
        standard_uncertainty_radius_px: f64,
        radius_source: String,
        standard_uncertainty_sweep_deg: f64,
        sweep_source: String,
        scale: ScaleUncertainty,
        propagation: PropagationParameters,
    },
}

#[derive(Debug, Serialize)]
pub(crate) struct ObservationUncertaintyCalculation {
    pub expanded_uncertainty: f64,
    pub output_unit: UncertaintyUnit,
    pub model: String,
    pub evidence: serde_json::Value,
}

fn isotropic_scale(measurement: &Measurement, calibration: &Calibration) -> Result<f64, String> {
    let scale = calibration.mm_per_pixel;
    if calibration.id.trim().is_empty()
        || measurement.calibration_id.as_deref() != Some(calibration.id.as_str())
        || !scale.is_finite() || scale <= 0.0
        || calibration.scale_x_mm_per_pixel.unwrap_or(scale) != scale
        || calibration.scale_y_mm_per_pixel.unwrap_or(scale) != scale
        || measurement.mm_per_pixel != Some(scale)
        || measurement.mm_per_pixel_x.unwrap_or(scale) != scale
        || measurement.mm_per_pixel_y.unwrap_or(scale) != scale
    {
        return Err("Une calibration isotrope courante, identique à celle de la mesure, est requise.".into());
    }
    Ok(scale)
}

fn finish<T: Serialize>(
    measurement: &Measurement,
    calibration: &Calibration,
    input: &ObservationUncertaintyInput,
    engine_result: &T,
    budget: &CorrelatedUncertaintyResult,
    model: &str,
) -> Result<ObservationUncertaintyCalculation, String> {
    let engine_result = serde_json::to_value(engine_result).map_err(|error| error.to_string())?;
    Ok(ObservationUncertaintyCalculation {
        expanded_uncertainty: budget.expanded_uncertainty,
        output_unit: budget.output_unit,
        model: model.into(),
        evidence: serde_json::json!({
            "schema_version": 1,
            "measurement_id": measurement.id,
            "capture_id": measurement.capture_id,
            "tool": measurement.kind,
            "native_points": measurement.points,
            "calibration_id": calibration.id,
            "isotropic_mm_per_pixel": calibration.mm_per_pixel,
            "uncertainty_parameters": input,
            "engine_result": engine_result,
            "geometry_source": "native_measurement",
            "status": "calculation_only",
            "applied_to_measurement": false,
            "source_qualification_verified": false,
            "conformity_evaluated": false,
            "limitations": [
                "Les incertitudes-types, leurs sources et les corrélations restent déclarées par l'opérateur ; aucune qualification n'en est déduite.",
                "La fraîcheur de l'observation native, de la capture et des dépendances doit être vérifiée par l'appelant.",
                "Aucun résidu d'ajustement, nominal ou statut de calibration n'est converti en incertitude."
            ]
        }),
    })
}

pub(crate) fn calculate(
    measurement: &Measurement,
    calibration: &Calibration,
    input: &ObservationUncertaintyInput,
) -> Result<ObservationUncertaintyCalculation, String> {
    if measurement.is_construction || measurement.is_numeric_resize() || measurement.dependency_stale {
        return Err("Une mesure observée courante est requise ; construction, géométrie imposée ou dépendance périmée refusée.".into());
    }
    if measurement.points.iter().any(|point|
        !point.x.is_finite() || !point.y.is_finite() || point.x.abs() > 1e12 || point.y.abs() > 1e12)
    {
        return Err("Coordonnées natives invalides ou hors limites numériques.".into());
    }
    let scale_value = isotropic_scale(measurement, calibration)?;
    match (input, measurement.kind, measurement.points.as_slice()) {
        (ObservationUncertaintyInput::Distance { points, scale, propagation }, Tool::Distance, [first, second]) => {
            let result = crate::distance_uncertainty::DistanceUncertaintyRequest {
                first_point: points[0].at(*first),
                second_point: points[1].at(*second),
                scale: scale.at(scale_value),
                coverage_factor: propagation.coverage_factor,
                correlation_matrix: propagation.correlation_matrix.clone(),
                correlation_justification: propagation.correlation_justification.clone(),
            }.calculate()?;
            finish(measurement, calibration, input, &result, &result.budget, "distance")
        }
        (ObservationUncertaintyInput::Angle { points, propagation }, Tool::Angle, [first, vertex, second]) => {
            let result = crate::angle_uncertainty::AngleUncertaintyRequest {
                first_point: points[0].at(*first),
                vertex: points[1].at(*vertex),
                second_point: points[2].at(*second),
                coverage_factor: propagation.coverage_factor,
                correlation_matrix: propagation.correlation_matrix.clone(),
                correlation_justification: propagation.correlation_justification.clone(),
            }.calculate()?;
            finish(measurement, calibration, input, &result, &result.budget, "angle")
        }
        (ObservationUncertaintyInput::Diameter { standard_uncertainty_radius_px, radius_source, scale, propagation },
            Tool::Circle | Tool::CircleFit, points) => {
            let radius_px = match (measurement.kind, points) {
                (Tool::Circle, [center, edge]) => center.dist(*edge),
                (Tool::CircleFit, points) if points.len() >= 3 => geometry::fit_circle(points)
                    .ok_or("Ajustement circulaire natif invalide.")?.radius,
                _ => return Err("Nombre de points incompatible avec le cercle natif.".into()),
            };
            let result = crate::diameter_uncertainty::DiameterUncertaintyRequest {
                radius_px,
                standard_uncertainty_radius_px: *standard_uncertainty_radius_px,
                radius_source: radius_source.clone(),
                scale: scale.at(scale_value),
                coverage_factor: propagation.coverage_factor,
                correlation_matrix: propagation.correlation_matrix.clone(),
                correlation_justification: propagation.correlation_justification.clone(),
            }.calculate()?;
            finish(measurement, calibration, input, &result, &result.budget, "diameter")
        }
        (ObservationUncertaintyInput::ArcLength { standard_uncertainty_radius_px, radius_source,
            standard_uncertainty_sweep_deg, sweep_source, scale, propagation }, Tool::Arc, [_, _, _]) => {
            let arc = geometry::arc_geometry(&measurement.points).ok_or("Arc natif invalide.")?;
            let result = crate::arc_length_uncertainty::ArcLengthUncertaintyRequest {
                radius_px: arc.radius,
                standard_uncertainty_radius_px: *standard_uncertainty_radius_px,
                radius_source: radius_source.clone(),
                sweep_deg: arc.sweep_radians.abs().to_degrees(),
                standard_uncertainty_sweep_deg: *standard_uncertainty_sweep_deg,
                sweep_source: sweep_source.clone(),
                scale: scale.at(scale_value),
                coverage_factor: propagation.coverage_factor,
                correlation_matrix: propagation.correlation_matrix.clone(),
                correlation_justification: propagation.correlation_justification.clone(),
            }.calculate()?;
            finish(measurement, calibration, input, &result, &result.budget, "arc_length")
        }
        _ => Err("Outil, nombre de points ou modèle d'incertitude non pris en charge pour cette mesure native.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(kind: Tool, points: &[[f64; 2]]) -> (Measurement, Calibration) {
        let measurement = serde_json::from_value(serde_json::json!({
            "id": 12, "kind": kind, "points": points.iter().map(|point|
                serde_json::json!({"x": point[0], "y": point[1]})).collect::<Vec<_>>(),
            "label": "Observation", "capture_id": "capture", "calibration_id": "cal",
            "mm_per_pixel": 0.01
        })).unwrap();
        let calibration = serde_json::from_value(serde_json::json!({
            "id": "cal", "device": "test", "resolution": [640, 480],
            "optical_setup": "test", "mm_per_pixel": 0.01,
            "reference_mm": 1.0, "reference_px": 100.0
        })).unwrap();
        (measurement, calibration)
    }

    fn parameters(size: usize) -> PropagationParameters {
        PropagationParameters {
            coverage_factor: 2.0,
            correlation_matrix: (0..size).map(|i| (0..size).map(|j|
                if i == j { 1.0 } else { 0.0 }).collect()).collect(),
            correlation_justification: "Indépendance déclarée pour ce calcul synthétique".into(),
        }
    }

    fn point_uncertainty() -> PointUncertainty {
        PointUncertainty { standard_uncertainty_x_px: 0.2,
            standard_uncertainty_y_px: 0.2, source: "Point observé".into() }
    }

    fn scale_uncertainty() -> ScaleUncertainty {
        ScaleUncertainty { relative_standard_uncertainty: 0.01, source: "Échelle".into() }
    }

    fn distance_input() -> ObservationUncertaintyInput {
        ObservationUncertaintyInput::Distance {
            points: [point_uncertainty(), point_uncertainty()],
            scale: scale_uncertainty(), propagation: parameters(5),
        }
    }

    #[test]
    fn dpm15_distance_reuses_native_geometry_and_existing_engine() {
        let (measurement, calibration) = fixture(Tool::Distance, &[[0.0, 0.0], [30.0, 40.0]]);
        let result = calculate(&measurement, &calibration, &distance_input()).unwrap();
        let expected = crate::distance_uncertainty::DistanceUncertaintyRequest {
            first_point: point_uncertainty().at(measurement.points[0]),
            second_point: point_uncertainty().at(measurement.points[1]),
            scale: scale_uncertainty().at(calibration.mm_per_pixel),
            coverage_factor: 2.0, correlation_matrix: parameters(5).correlation_matrix,
            correlation_justification: parameters(5).correlation_justification,
        }.calculate().unwrap();
        assert_eq!(result.expanded_uncertainty, expected.budget.expanded_uncertainty);
        assert_eq!(result.output_unit, UncertaintyUnit::Millimeter);
        assert_eq!(result.evidence["engine_result"]["distance_mm"], 0.5);
        assert_eq!(result.evidence["source_qualification_verified"], false);
        assert_eq!(result.evidence["applied_to_measurement"], false);
        let mut moved = measurement.clone();
        moved.points[1].x += 10.0;
        let changed = calculate(&moved, &calibration, &distance_input()).unwrap();
        assert_ne!(result.expanded_uncertainty, changed.expanded_uncertainty);
    }

    #[test]
    fn dpm15_angle_diameter_and_major_arc_use_native_geometry() {
        let (angle, calibration) = fixture(Tool::Angle, &[[10.0, 0.0], [0.0, 0.0], [0.0, 20.0]]);
        let input = ObservationUncertaintyInput::Angle {
            points: [point_uncertainty(), point_uncertainty(), point_uncertainty()],
            propagation: parameters(6),
        };
        let result = calculate(&angle, &calibration, &input).unwrap();
        assert_eq!(result.output_unit, UncertaintyUnit::Degree);
        assert_eq!(result.evidence["engine_result"]["angle_degrees"], 90.0);
        let diameter = ObservationUncertaintyInput::Diameter {
            standard_uncertainty_radius_px: 0.2, radius_source: "Rayon".into(),
            scale: scale_uncertainty(), propagation: parameters(2),
        };
        let (circle, _) = fixture(Tool::Circle, &[[0.0, 0.0], [100.0, 0.0]]);
        let result = calculate(&circle, &calibration, &diameter).unwrap();
        assert_eq!(result.evidence["engine_result"]["diameter_mm"], 2.0);
        let (fit, _) = fixture(Tool::CircleFit, &[[100.0, 0.0], [0.0, 100.0], [-100.0, 0.0], [0.0, -100.0]]);
        let fitted = calculate(&fit, &calibration, &diameter).unwrap();
        assert!((fitted.expanded_uncertainty - result.expanded_uncertainty).abs() < 1e-12);
        let arc_input = ObservationUncertaintyInput::ArcLength {
            standard_uncertainty_radius_px: 0.2, radius_source: "Rayon".into(),
            standard_uncertainty_sweep_deg: 0.1, sweep_source: "Ouverture".into(),
            scale: scale_uncertainty(), propagation: parameters(3),
        };
        let (arc, _) = fixture(Tool::Arc, &[[100.0, 0.0], [-100.0, 0.0], [0.0, 100.0]]);
        let result = calculate(&arc, &calibration, &arc_input).unwrap();
        assert!((result.evidence["engine_result"]["request"]["sweep_deg"].as_f64().unwrap() - 270.0).abs() < 1e-10);
        assert!((result.evidence["engine_result"]["arc_length_mm"].as_f64().unwrap()
            - 1.5 * std::f64::consts::PI).abs() < 1e-10);
    }

    #[test]
    fn dpm15_refuses_imposed_construction_stale_anisotropic_and_unsupported_inputs() {
        let (measurement, calibration) = fixture(Tool::Distance, &[[0.0, 0.0], [30.0, 40.0]]);
        for mutation in 0..6 {
            let mut changed = measurement.clone();
            match mutation {
                0 => changed.is_construction = true,
                1 => changed.origin = "numeric_resize".into(),
                2 => changed.method = Some("[dper:numeric_resize:v1]".into()),
                3 => changed.dependency_stale = true,
                4 => changed.kind = Tool::Polyline,
                _ => changed.points.push(Point { x: 1.0, y: 2.0 }),
            }
            assert!(calculate(&changed, &calibration, &distance_input()).is_err());
        }
        for mutation in 0..3 {
            let mut changed = calibration.clone();
            match mutation {
                0 => changed.scale_x_mm_per_pixel = Some(0.02),
                1 => changed.id = "other".into(),
                _ => changed.mm_per_pixel = f64::NAN,
            }
            assert!(calculate(&measurement, &changed, &distance_input()).is_err());
        }
        let mut input = distance_input();
        if let ObservationUncertaintyInput::Distance { propagation, .. } = &mut input {
            propagation.correlation_matrix[0][1] = 2.0;
        }
        assert!(calculate(&measurement, &calibration, &input).is_err());
    }

    #[test]
    fn dpm15_client_cannot_supply_geometry_scale_or_qualification() {
        let valid = serde_json::to_value(distance_input()).unwrap();
        for field in ["position_px", "radius_px", "millimeters_per_pixel", "qualified", "source_qualification_verified"] {
            let mut json = valid.clone();
            json[field] = serde_json::json!(true);
            assert!(serde_json::from_value::<ObservationUncertaintyInput>(json).is_err(), "{field}");
        }
        let mut nested = valid.clone();
        nested["points"][0]["position_px"] = serde_json::json!({"x": 10.0, "y": 20.0});
        assert!(serde_json::from_value::<ObservationUncertaintyInput>(nested).is_err());
        let mut nested = valid;
        nested["scale"]["millimeters_per_pixel"] = serde_json::json!(0.5);
        assert!(serde_json::from_value::<ObservationUncertaintyInput>(nested).is_err());
    }
}

//! Native presentation of bounded pixel diagnostics. The complete diagnostic
//! remains the authority: presentation points do not create observed samples.

use crate::{
    dper_vision::{self, ArcEndpointSearch, InspectionGeometry, InspectionResult, LineSearch, StraightEdgeSearch},
    geometry::{Point, Tool},
};
use image::RgbImage;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeDiagnosticRequest {
    StraightEdgeEndpoints { search: StraightEdgeSearch },
    ArcEndpoints { search: ArcEndpointSearch },
    LineIntersection { searches: [LineSearch; 2] },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeDiagnosticProof {
    pub request: NativeDiagnosticRequest,
    /// Full engine output, including every independently observed support and
    /// the interior/exterior probes for endpoint diagnostics.
    pub result: Value,
    #[serde(default)]
    pub sources: Vec<u64>,
    #[serde(default)]
    pub context: Value,
    #[serde(default)]
    pub preview_revision: u64,
}

#[derive(Clone, Debug)]
pub struct NativeDiagnosticConstruction {
    pub role: &'static str,
    pub kind: Tool,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug)]
pub struct NativeDiagnosticObservation {
    pub result: Value,
    pub kind: Tool,
    pub points: Vec<Point>,
    /// For an intersection this is its first observed support, not a claim
    /// that the intersection itself is a fitted line or physical endpoint.
    pub presentation: InspectionResult,
    pub constructions: Vec<NativeDiagnosticConstruction>,
    pub length_px: Option<f64>,
    pub is_construction: bool,
}

fn support(role: &'static str, result: &InspectionResult) -> Result<NativeDiagnosticConstruction, String> {
    let InspectionGeometry::Line { start, end } = result.geometry else {
        return Err("Une construction de support droit est requise.".into());
    };
    Ok(NativeDiagnosticConstruction { role, kind: Tool::Distance, points: vec![start, end] })
}

fn arc_points(center: Point, radius: f64, start: Point, end: Point, sweep: f64) -> Vec<Point> {
    let start_rad = (start.y - center.y).atan2(start.x - center.x);
    let middle_rad = start_rad + sweep * 0.5;
    vec![start, Point { x: center.x + radius * middle_rad.cos(), y: center.y + radius * middle_rad.sin() }, end]
}

pub fn observe(image: &RgbImage, request: &NativeDiagnosticRequest) -> Result<NativeDiagnosticObservation, String> {
    match request {
        NativeDiagnosticRequest::StraightEdgeEndpoints { search } => {
            let diagnostic = dper_vision::inspect_straight_edge_endpoints(image, search)?;
            let result = serde_json::to_value(&diagnostic).map_err(|error| error.to_string())?;
            let constructions = vec![support("edge_support", &diagnostic.edge_support)?,
                support("start_boundary", &diagnostic.start_boundary_support)?,
                support("end_boundary", &diagnostic.end_boundary_support)?];
            let points = vec![diagnostic.start.intersection_px, diagnostic.end.intersection_px];
            let mut presentation = diagnostic.edge_support;
            presentation.geometry = InspectionGeometry::Line { start: points[0], end: points[1] };
            presentation.evidence.endpoint_extent_verified = true;
            Ok(NativeDiagnosticObservation { result, kind: Tool::Distance, points, presentation,
                constructions, length_px: Some(diagnostic.length_px), is_construction: false })
        }
        NativeDiagnosticRequest::ArcEndpoints { search } => {
            let diagnostic = dper_vision::inspect_arc_endpoints(image, search)?;
            let result = serde_json::to_value(&diagnostic).map_err(|error| error.to_string())?;
            let InspectionGeometry::Arc { center, radius, .. } = diagnostic.arc_support.geometry else {
                return Err("Le diagnostic ne contient pas de support d'arc.".into());
            };
            let points = arc_points(center, radius, diagnostic.start.intersection_px,
                diagnostic.end.intersection_px, diagnostic.sweep_rad);
            let mut constructions = vec![support("start_boundary", &diagnostic.boundary_supports[0])?,
                support("end_boundary", &diagnostic.boundary_supports[1])?];
            // A circle records only the fitted circular support, never its
            // unverified original angular search extent.
            constructions.insert(0, NativeDiagnosticConstruction { role: "arc_support", kind: Tool::Circle,
                points: vec![center, Point { x: center.x + radius, y: center.y }] });
            let mut presentation = diagnostic.arc_support;
            presentation.geometry = InspectionGeometry::Arc { center, radius,
                start_rad: (points[0].y - center.y).atan2(points[0].x - center.x), sweep_rad: diagnostic.sweep_rad };
            presentation.evidence.endpoint_extent_verified = true;
            Ok(NativeDiagnosticObservation { result, kind: Tool::Arc, points, presentation,
                constructions, length_px: Some(diagnostic.arc_length_px), is_construction: false })
        }
        NativeDiagnosticRequest::LineIntersection { searches } => {
            let diagnostic = dper_vision::inspect_line_intersection(image, searches)?;
            let result = serde_json::to_value(&diagnostic).map_err(|error| error.to_string())?;
            let constructions = vec![support("first_support", &diagnostic.supports[0])?,
                support("second_support", &diagnostic.supports[1])?];
            Ok(NativeDiagnosticObservation { result, kind: Tool::Coordinate,
                points: vec![diagnostic.intersection_px], presentation: diagnostic.supports[0].clone(),
                constructions, length_px: None, is_construction: true })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::arc_geometry;
    use std::f64::consts::PI;

    fn rasterize(intensity: impl Fn(f64, f64) -> f64) -> RgbImage {
        RgbImage::from_fn(256, 256, |x, y| image::Rgb([intensity(x as f64, y as f64).round() as u8; 3]))
    }

    fn line(start: Point, end: Point) -> LineSearch {
        LineSearch { start, end, options: Default::default() }
    }

    #[test]
    fn native_segment_uses_verified_visible_endpoints_and_retains_all_supports() {
        let point = |x: f64, y: f64| Point { x: 67.5 + x, y: 126.5 + y };
        let image = rasterize(|x, y| {
            let along = x - 67.5;
            let across = y - 126.5;
            30.0 + 190.0 * (0.5 + 0.5 * (along.min(112.0 - along).min(across) / 0.65).tanh())
        });
        let request = NativeDiagnosticRequest::StraightEdgeEndpoints { search: StraightEdgeSearch {
            edge: line(point(3.0, 1.3), point(109.0, 1.3)),
            start_boundary: line(point(-1.2, 3.0), point(-1.2, 38.0)),
            end_boundary: line(point(113.2, 3.0), point(113.2, 38.0)),
        } };
        let observed = observe(&image, &request).unwrap();
        assert_eq!(observed.kind, Tool::Distance);
        assert!(!observed.is_construction);
        assert!(observed.presentation.evidence.endpoint_extent_verified);
        assert!(observed.points[0].dist(point(0.0, 0.0)) < 0.35);
        assert!(observed.points[1].dist(point(112.0, 0.0)) < 0.35);
        assert!((observed.length_px.unwrap() - 112.0).abs() < 0.5);
        assert_eq!(observed.constructions.len(), 3);
        assert_eq!(observed.result["edge_support"]["evidence"]["endpoint_extent_verified"], false);
    }

    #[test]
    fn native_arc_uses_measured_opening_instead_of_fitting_a_complete_circle() {
        let center = Point { x: 123.5, y: 126.5 };
        let point = |x: f64, y: f64| Point { x: center.x + x, y: center.y + y };
        let image = rasterize(|x, y| {
            let x = x - center.x;
            let y = y - center.y;
            30.0 + 190.0 * (0.5 + 0.5 * (x.min(y).min(60.0 - x.hypot(y)) / 0.65).tanh())
        });
        let request = NativeDiagnosticRequest::ArcEndpoints { search: ArcEndpointSearch {
            center: point(0.6, -0.6), radius: 61.0, start_rad: 0.05, sweep_rad: PI / 2.0 - 0.1,
            options: Default::default(),
            start_boundary: line(point(57.0, -1.3), point(30.0, -1.3)),
            end_boundary: line(point(-1.3, 30.0), point(-1.3, 57.0)),
        } };
        let observed = observe(&image, &request).unwrap();
        assert_eq!(observed.kind, Tool::Arc);
        assert_eq!(observed.points.len(), 3);
        let arc = arc_geometry(&observed.points).unwrap();
        assert!((arc.radius * arc.sweep_radians.abs() - observed.length_px.unwrap()).abs() < 1e-7);
        assert!(observed.points[0].dist(point(60.0, 0.0)) < 0.35);
        assert!(observed.points[2].dist(point(0.0, 60.0)) < 0.35);
        assert!(observed.presentation.evidence.endpoint_extent_verified);
        assert_eq!(observed.result["arc_support"]["evidence"]["endpoint_extent_verified"], false);
        assert_eq!(observed.constructions.len(), 3);
    }

    #[test]
    fn native_arc_points_preserve_major_arcs_and_both_sweep_signs() {
        let center = Point { x: 80.0, y: 90.0 };
        let radius = 25.0;
        for start_rad in [-2.8_f64, 0.2, 2.8] {
            for sweep in [-1.5 * PI, -0.5 * PI, 0.5 * PI, 1.5 * PI] {
                let point = |angle: f64| Point { x: center.x + radius * angle.cos(), y: center.y + radius * angle.sin() };
                let points = arc_points(center, radius, point(start_rad), point(start_rad + sweep), sweep);
                let actual = arc_geometry(&points).unwrap();
                assert!(actual.center.dist(center) < 1e-9);
                assert!((actual.radius - radius).abs() < 1e-9);
                assert!((actual.sweep_radians - sweep).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn intersection_is_a_native_coordinate_with_observed_supports() {
        let image = RgbImage::from_fn(200, 200, |x, y| {
            let step = |value: f64| 0.5 + 0.5 * (value / 0.65).tanh();
            let intensity = 30.0 + 190.0 * step((x as f64 - 100.3).min(y as f64 - 99.7));
            image::Rgb([intensity.round() as u8; 3])
        });
        let line = |a, b| LineSearch { start: a, end: b, options: Default::default() };
        let request = NativeDiagnosticRequest::LineIntersection { searches: [
            line(Point { x: 101.5, y: 103.0 }, Point { x: 101.5, y: 160.0 }),
            line(Point { x: 103.0, y: 101.0 }, Point { x: 160.0, y: 101.0 }),
        ] };
        let observed = observe(&image, &request).unwrap();
        assert_eq!(observed.kind, Tool::Coordinate);
        assert!(observed.is_construction);
        assert_eq!(observed.points.len(), 1);
        assert!(observed.points[0].dist(Point { x: 100.3, y: 99.7 }) < 0.35);
        assert!(observed.length_px.is_none());
        assert_eq!(observed.constructions.len(), 2);
        assert!(!observed.presentation.evidence.endpoint_extent_verified);
        let restored: NativeDiagnosticRequest = serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), serde_json::to_value(request).unwrap());
        assert!(observed.result["supports"].as_array().unwrap().len() == 2);
    }
}

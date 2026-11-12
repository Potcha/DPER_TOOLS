//! Directed, bounded contour inspection in native source-image pixels.
//!
//! Expected geometry only locates search profiles. Actual accepted image edges
//! determine the fitted support line or radius. No physical scale is inferred.
//! Line/arc endpoints are the observed sample span, not verified part endpoints.

use crate::geometry::{fit_circle, CircleFit, Point};
use image::RgbImage;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub enum InspectionGeometry {
    Line {
        start: Point,
        end: Point,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    Arc {
        center: Point,
        radius: f64,
        start_rad: f64,
        sweep_rad: f64,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub enum EdgePolarity {
    Any,
    /// Intensity increases along the search normal.
    Rising,
    Falling,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VisionOptions {
    pub sample_count: usize,
    pub search_radius_px: f64,
    /// Central intensity derivative in grayscale levels per pixel (not Sobel units).
    pub minimum_gradient: f64,
    pub polarity: EdgePolarity,
    pub minimum_coverage: f64,
    pub outlier_threshold_px: f64,
    pub maximum_rms_px: f64,
    pub minimum_arc_sweep_rad: f64,
}

impl Default for VisionOptions {
    fn default() -> Self {
        Self {
            sample_count: 96,
            search_radius_px: 8.0,
            minimum_gradient: 12.0,
            polarity: EdgePolarity::Any,
            minimum_coverage: 0.7,
            outlier_threshold_px: 1.5,
            maximum_rms_px: 0.8,
            minimum_arc_sweep_rad: PI / 3.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VisionEvidence {
    pub accepted_points: Vec<Point>,
    /// Expected positions for missing/ambiguous profiles; detected fit outliers.
    pub rejected_points: Vec<Point>,
    pub sample_count: usize,
    pub rms_residual_px: f64,
    pub max_residual_px: f64,
    pub coverage_fraction: f64,
    /// Always false for line/arc support inspection: no endpoint detector ran.
    pub endpoint_extent_verified: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InspectionResult {
    pub geometry: InspectionGeometry,
    pub evidence: VisionEvidence,
}

#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineSearch {
    pub start: Point,
    pub end: Point,
    #[serde(default)]
    pub options: VisionOptions,
}

impl LineSearch {
    fn inspect(&self, image: &RgbImage) -> Result<InspectionResult, String> {
        inspect_geometry(
            image,
            &InspectionGeometry::Line { start: self.start, end: self.end },
            &self.options,
        )
    }
}

#[derive(Debug, Serialize)]
pub struct LineIntersection {
    pub intersection_px: Point,
    pub acute_angle_deg: f64,
    pub nearest_observation_px: [f64; 2],
    pub supports: [InspectionResult; 2],
}

/// Construct a pixel-space intersection from two independently observed supports.
/// The angle and nearest-sample limits bound conditioning and unsupported
/// extension. Neither proves that a physical edge terminates at this point.
pub fn inspect_line_intersection(
    image: &RgbImage,
    searches: &[LineSearch; 2],
) -> Result<LineIntersection, String> {
    let supports = [
        searches[0].inspect(image).map_err(|error| format!("Premier bord : {error}"))?,
        searches[1].inspect(image).map_err(|error| format!("Second bord : {error}"))?,
    ];
    let IntersectionEvidence { intersection_px, acute_angle_deg, nearest_observation_px } =
        intersect_observed_supports(image, supports.each_ref())?;
    Ok(LineIntersection { intersection_px, acute_angle_deg, nearest_observation_px, supports })
}

const MAX_SAMPLE_DISTANCE_PX: f64 = 4.0;
const ENDPOINT_PROBE_DISTANCES_PX: [f64; 3] = [3.0, 5.0, 7.0];
const MAX_INTERIOR_PROBE_OFFSET_PX: f64 = 1.5;

#[derive(Debug, Serialize)]
pub struct IntersectionEvidence {
    pub intersection_px: Point,
    pub acute_angle_deg: f64,
    pub nearest_observation_px: [f64; 2],
}

fn intersect_observed_supports(
    image: &RgbImage,
    supports: [&InspectionResult; 2],
) -> Result<IntersectionEvidence, String> {
    const MIN_ANGLE_DEG: f64 = 15.0;

    let [InspectionGeometry::Line { start: a, end: b }, InspectionGeometry::Line { start: c, end: d }] =
        supports.map(|support| &support.geometry)
    else {
        return Err("Deux supports droits sont requis.".into());
    };
    let first_length = a.dist(*b);
    let second_length = c.dist(*d);
    let u = Point { x: (b.x - a.x) / first_length, y: (b.y - a.y) / first_length };
    let v = Point { x: (d.x - c.x) / second_length, y: (d.y - c.y) / second_length };
    let determinant = u.x * v.y - u.y * v.x;
    let acute_angle_deg = determinant.abs().min(1.0).asin().to_degrees();
    if acute_angle_deg < MIN_ANGLE_DEG {
        return Err(format!("Supports presque parallèles : angle inférieur à {MIN_ANGLE_DEG}°."));
    }

    let along = ((c.x - a.x) * v.y - (c.y - a.y) * v.x) / determinant;
    let point = Point { x: a.x + along * u.x, y: a.y + along * u.y };
    if !finite_point(point) || point.x < 0.0 || point.y < 0.0
        || point.x > f64::from(image.width() - 1) || point.y > f64::from(image.height() - 1)
    {
        return Err("Intersection hors de l'image.".into());
    }
    let nearest_observation_px = supports.map(|support| {
        support.evidence.accepted_points.iter().map(|p| p.dist(point)).fold(f64::INFINITY, f64::min)
    });
    if nearest_observation_px.iter().any(|&distance| distance > MAX_SAMPLE_DISTANCE_PX) {
        return Err(format!("Intersection insuffisamment étayée : un bord n'est pas observé à moins de {MAX_SAMPLE_DISTANCE_PX} px."));
    }
    Ok(IntersectionEvidence { intersection_px: point, acute_angle_deg, nearest_observation_px })
}

#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StraightEdgeSearch {
    pub edge: LineSearch,
    pub start_boundary: LineSearch,
    pub end_boundary: LineSearch,
}

#[derive(Debug, Serialize)]
pub struct StraightEdgeEndpoints {
    pub start: IntersectionEvidence,
    pub end: IntersectionEvidence,
    pub length_px: f64,
    pub exterior_probe_centers_px: [[Point; 3]; 2],
    pub exterior_probe_radius_px: f64,
    pub exterior_minimum_gradient: f64,
    pub interior_observed_points_px: [[Point; 3]; 2],
    pub interior_maximum_offset_px: f64,
    pub edge_support: InspectionResult,
    pub start_boundary_support: InspectionResult,
    pub end_boundary_support: InspectionResult,
}

/// Estimate the visible endpoints of a straight edge bounded by two observed
/// transverse edges. Missing continuation is evidence only in the sampled
/// exterior profiles. It cannot exclude a physical edge hidden by occlusion.
pub fn inspect_straight_edge_endpoints(
    image: &RgbImage,
    search: &StraightEdgeSearch,
) -> Result<StraightEdgeEndpoints, String> {
    let edge_support = search.edge.inspect(image).map_err(|error| format!("Bord principal : {error}"))?;
    let start_boundary_support = search.start_boundary.inspect(image)
        .map_err(|error| format!("Bord transverse de début : {error}"))?;
    let end_boundary_support = search.end_boundary.inspect(image)
        .map_err(|error| format!("Bord transverse de fin : {error}"))?;
    let start = intersect_observed_supports(image, [&edge_support, &start_boundary_support])
        .map_err(|error| format!("Début du bord : {error}"))?;
    let end = intersect_observed_supports(image, [&edge_support, &end_boundary_support])
        .map_err(|error| format!("Fin du bord : {error}"))?;
    let InspectionGeometry::Line { start: first, end: last } = edge_support.geometry else {
        return Err("Un support droit principal est requis.".into());
    };
    let span = first.dist(last);
    let direction = Point { x: (last.x - first.x) / span, y: (last.y - first.y) / span };
    let length_px = (end.intersection_px.x - start.intersection_px.x) * direction.x
        + (end.intersection_px.y - start.intersection_px.y) * direction.y;
    if length_px < 8.0 || start.intersection_px.dist(first) > MAX_SAMPLE_DISTANCE_PX
        || end.intersection_px.dist(last) > MAX_SAMPLE_DISTANCE_PX
    {
        return Err("Les intersections ne délimitent pas les deux extrémités opposées du support observé.".into());
    }

    let options = VisionOptions {
        search_radius_px: 2.0,
        polarity: EdgePolarity::Any,
        ..search.edge.options.clone()
    };
    let exterior_probe_centers_px = [
        require_no_local_continuation(image, start.intersection_px, direction, -1.0, &options)
            .map_err(|error| format!("Début du bord : {error}"))?,
        require_no_local_continuation(image, end.intersection_px, direction, 1.0, &options)
            .map_err(|error| format!("Fin du bord : {error}"))?,
    ];
    let normal = Point { x: -direction.y, y: direction.x };
    let interior = |endpoint: Point, inward: f64| {
        ENDPOINT_PROBE_DISTANCES_PX.map(|distance| (Point {
            x: endpoint.x + direction.x * inward * distance,
            y: endpoint.y + direction.y * inward * distance,
        }, normal))
    };
    let interior_observed_points_px = [
        observe_endpoint_interior(image, interior(start.intersection_px, 1.0), &options)
            .map_err(|error| format!("Début du bord : {error}"))?,
        observe_endpoint_interior(image, interior(end.intersection_px, -1.0), &options)
            .map_err(|error| format!("Fin du bord : {error}"))?,
    ];
    Ok(StraightEdgeEndpoints {
        start, end, length_px, exterior_probe_centers_px,
        exterior_probe_radius_px: options.search_radius_px,
        exterior_minimum_gradient: options.minimum_gradient,
        interior_observed_points_px,
        interior_maximum_offset_px: MAX_INTERIOR_PROBE_OFFSET_PX,
        edge_support, start_boundary_support, end_boundary_support,
    })
}

fn observe_endpoint_interior(
    image: &RgbImage,
    profiles: [(Point, Point); 3],
    options: &VisionOptions,
) -> Result<[Point; 3], &'static str> {
    let mut observed = [Point::default(); 3];
    for (index, (center, normal)) in profiles.into_iter().enumerate() {
        if !endpoint_profile_in_image(image, center, options) {
            return Err("Profil intérieur de l'extrémité hors de l'image.");
        }
        match directed_edge(image, center, normal, options) {
            ProfileEdge::Found(point) if point.dist(center) <= MAX_INTERIOR_PROBE_OFFSET_PX => {
                observed[index] = point;
            }
            _ => return Err("Contour intérieur de l'extrémité absent, décalé ou ambigu."),
        }
    }
    Ok(observed)
}

fn endpoint_profile_in_image(image: &RgbImage, point: Point, options: &VisionOptions) -> bool {
    // Includes search offsets, neighboring derivative samples and tangential averaging.
    let margin = options.search_radius_px + 2.0;
    point.x >= margin && point.y >= margin
        && point.x + margin <= f64::from(image.width() - 1)
        && point.y + margin <= f64::from(image.height() - 1)
}

fn require_no_local_continuation(
    image: &RgbImage,
    endpoint: Point,
    direction: Point,
    side: f64,
    options: &VisionOptions,
) -> Result<[Point; 3], &'static str> {
    let normal = Point { x: -direction.y, y: direction.x };
    let probes = ENDPOINT_PROBE_DISTANCES_PX.map(|distance| Point {
        x: endpoint.x + direction.x * side * distance,
        y: endpoint.y + direction.y * side * distance,
    });
    for probe in probes {
        if !endpoint_profile_in_image(image, probe, options) {
            return Err("Zone de vérification du prolongement hors de l'image.");
        }
        if !matches!(directed_edge(image, probe, normal, options), ProfileEdge::Missing) {
            return Err("Un bord se prolonge ou reste ambigu au-delà de l'intersection.");
        }
    }
    Ok(probes)
}

#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArcEndpointSearch {
    pub center: Point,
    pub radius: f64,
    pub start_rad: f64,
    pub sweep_rad: f64,
    #[serde(default)]
    pub options: VisionOptions,
    pub start_boundary: LineSearch,
    pub end_boundary: LineSearch,
}

#[derive(Debug, Serialize)]
pub struct ArcEndpoints {
    pub start: IntersectionEvidence,
    pub end: IntersectionEvidence,
    pub sweep_rad: f64,
    pub arc_length_px: f64,
    pub exterior_probe_centers_px: [[Point; 3]; 2],
    pub exterior_probe_radius_px: f64,
    pub exterior_minimum_gradient: f64,
    pub interior_observed_points_px: [[Point; 3]; 2],
    pub interior_maximum_offset_px: f64,
    pub arc_support: InspectionResult,
    pub boundary_supports: [InspectionResult; 2],
}

/// Estimate visible arc endpoints using independently observed transverse edges.
/// Missing exterior transitions establish only local visibility at the probes.
pub fn inspect_arc_endpoints(
    image: &RgbImage,
    search: &ArcEndpointSearch,
) -> Result<ArcEndpoints, String> {
    let arc_support = inspect_geometry(image, &InspectionGeometry::Arc {
        center: search.center, radius: search.radius,
        start_rad: search.start_rad, sweep_rad: search.sweep_rad,
    }, &search.options)?;
    let boundary_supports = [
        search.start_boundary.inspect(image).map_err(|e| format!("Limite de début : {e}"))?,
        search.end_boundary.inspect(image).map_err(|e| format!("Limite de fin : {e}"))?,
    ];
    let InspectionGeometry::Arc { center, radius, start_rad, sweep_rad } = arc_support.geometry else {
        return Err("Un support d'arc est requis.".into());
    };
    let circle_point = |angle: f64| Point {
        x: center.x + radius * angle.cos(), y: center.y + radius * angle.sin(),
    };
    let start = intersect_arc_boundary(image, &arc_support, &boundary_supports[0], circle_point(start_rad))?;
    let end = intersect_arc_boundary(image, &arc_support, &boundary_supports[1], circle_point(start_rad + sweep_rad))?;
    let start_angle = (start.intersection_px.y - center.y).atan2(start.intersection_px.x - center.x);
    let end_angle = (end.intersection_px.y - center.y).atan2(end.intersection_px.x - center.x);
    let sign = sweep_rad.signum();
    let measured_sweep = sign * (sign * (end_angle - start_angle)).rem_euclid(TAU);
    let angle_delta = |delta: f64| delta.sin().atan2(delta.cos());
    let start_delta = angle_delta(start_angle - start_rad);
    let end_delta = angle_delta(end_angle - start_rad - sweep_rad);
    let unwrapped_sweep = sweep_rad + end_delta - start_delta;
    if (measured_sweep - sweep_rad).abs() * radius > 8.0
        || measured_sweep.abs() < search.options.minimum_arc_sweep_rad
        || measured_sweep.abs() >= TAU - 1e-6
        || start_delta.abs() > PI / 4.0 || end_delta.abs() > PI / 4.0
        || (measured_sweep - unwrapped_sweep).abs() > 1e-6
        || radius * measured_sweep.abs() < 8.0
        || radius * (TAU - measured_sweep.abs()) <= 14.0
    {
        return Err("Les limites ne délimitent pas l'ouverture observée de l'arc.".into());
    }
    let probe_options = VisionOptions {
        search_radius_px: 2.0, polarity: EdgePolarity::Any, ..search.options.clone()
    };
    // Both ends share exactly the same bounds, polarity and gradient policy.
    let mut exterior_probe_centers_px = [[Point::default(); 3]; 2];
    let mut interior_observed_points_px = [[Point::default(); 3]; 2];
    for (side, (angle, outward)) in [(start_angle, -sign), (end_angle, sign)].into_iter().enumerate() {
        let profile = |distance: f64| {
            let sample_angle = angle + outward * distance / radius;
            (circle_point(sample_angle), Point { x: sample_angle.cos(), y: sample_angle.sin() })
        };
        for (index, distance) in ENDPOINT_PROBE_DISTANCES_PX.into_iter().enumerate() {
            let (point, normal) = profile(distance);
            if !endpoint_profile_in_image(image, point, &probe_options) {
                return Err("Profil extérieur d'arc hors de l'image.".into());
            }
            if !matches!(directed_edge(image, point, normal, &probe_options), ProfileEdge::Missing) {
                return Err("L'arc se prolonge ou reste ambigu au-delà de sa limite.".into());
            }
            exterior_probe_centers_px[side][index] = point;
        }
        interior_observed_points_px[side] = observe_endpoint_interior(
            image, ENDPOINT_PROBE_DISTANCES_PX.map(|distance| profile(-distance)), &probe_options,
        ).map_err(|error| format!("{} de l'arc : {error}", if side == 0 { "Début" } else { "Fin" }))?;
    }
    Ok(ArcEndpoints {
        start, end, sweep_rad: measured_sweep, arc_length_px: radius * measured_sweep.abs(),
        exterior_probe_centers_px, exterior_probe_radius_px: probe_options.search_radius_px,
        exterior_minimum_gradient: probe_options.minimum_gradient,
        interior_observed_points_px, interior_maximum_offset_px: MAX_INTERIOR_PROBE_OFFSET_PX,
        arc_support, boundary_supports,
    })
}

fn intersect_arc_boundary(
    image: &RgbImage,
    arc: &InspectionResult,
    boundary: &InspectionResult,
    near: Point,
) -> Result<IntersectionEvidence, String> {
    let InspectionGeometry::Arc { center, radius, .. } = arc.geometry else {
        return Err("Support circulaire absent.".into());
    };
    let InspectionGeometry::Line { start, end } = boundary.geometry else {
        return Err("Limite droite absente.".into());
    };
    let length = start.dist(end);
    let u = Point { x: (end.x - start.x) / length, y: (end.y - start.y) / length };
    let projection = (center.x - start.x) * u.x + (center.y - start.y) * u.y;
    let foot = Point { x: start.x + projection * u.x, y: start.y + projection * u.y };
    let squared = radius * radius - center.dist(foot).powi(2);
    if squared <= 0.0 {
        return Err("La limite ne coupe pas l'arc ou lui est tangente.".into());
    }
    let mut candidates = Vec::new();
    for side in [-1.0, 1.0] {
        let point = Point { x: foot.x + side * squared.sqrt() * u.x, y: foot.y + side * squared.sqrt() * u.y };
        if point.dist(near) > MAX_SAMPLE_DISTANCE_PX { continue; }
        let nearest_observation_px = [arc, boundary].map(|support|
            support.evidence.accepted_points.iter().map(|p| p.dist(point)).fold(f64::INFINITY, f64::min));
        if nearest_observation_px.iter().any(|&distance| distance > MAX_SAMPLE_DISTANCE_PX) { continue; }
        let normal = Point { x: (point.x - center.x) / radius, y: (point.y - center.y) / radius };
        let acute_angle_deg = (normal.x * u.x + normal.y * u.y).abs().min(1.0).asin().to_degrees();
        if acute_angle_deg < 15.0 || point.x < 0.0 || point.y < 0.0
            || point.x > f64::from(image.width() - 1) || point.y > f64::from(image.height() - 1)
        { continue; }
        candidates.push(IntersectionEvidence { intersection_px: point, acute_angle_deg, nearest_observation_px });
    }
    if candidates.len() != 1 {
        return Err("Intersection arc / limite absente, ambiguë ou insuffisamment étayée.".into());
    }
    Ok(candidates.remove(0))
}

#[derive(Clone, Copy)]
struct EdgeSample {
    index: usize,
    point: Point,
}

enum ProfileEdge {
    Found(Point),
    Missing,
    Ambiguous,
}

/// Inspect a line, circle or arc near a supplied pixel-coordinate localization.
///
/// Ambiguous profiles are excluded, just like missing image support; nominal
/// geometry never supplies a replacement measurement. Errors explicitly with
/// insufficient unambiguous coverage, weak edges, unstable arc
/// support or excessive fit residuals. RMS is a fit diagnostic, not a calibrated
/// measurement uncertainty. Work is bounded by 256 profiles and a 32px search.
pub fn inspect_geometry(
    image: &RgbImage,
    expected: &InspectionGeometry,
    options: &VisionOptions,
) -> Result<InspectionResult, String> {
    validate(image, expected, options)?;
    let mut detected = Vec::new();
    let mut rejected = Vec::new();
    let mut ambiguous_profiles = 0;
    for index in 0..options.sample_count {
        let (point, normal) = profile_position(expected, index, options.sample_count);
        match directed_edge(image, point, normal, options) {
            ProfileEdge::Found(point) => detected.push(EdgeSample { index, point }),
            ProfileEdge::Missing => rejected.push(point),
            ProfileEdge::Ambiguous => {
                ambiguous_profiles += 1;
                rejected.push(point);
            }
        }
    }
    let minimum_points = match expected {
        InspectionGeometry::Line { .. } => 8,
        InspectionGeometry::Circle { .. } => 16,
        InspectionGeometry::Arc { .. } => 12,
    };
    let required = ((options.sample_count as f64 * options.minimum_coverage).ceil() as usize)
        .max(minimum_points);
    if detected.len() < required {
        if ambiguous_profiles > 0 {
            return Err(format!(
                "Contour ambigu : {} profils ambigus, {} profils non ambigus détectés sur {} ({} requis).",
                ambiguous_profiles, detected.len(), options.sample_count, required
            ));
        }
        return Err(format!(
            "Contour insuffisant : {} profils détectés sur {} ({} requis).",
            detected.len(),
            options.sample_count,
            required
        ));
    }

    let (geometry, inliers, residuals) = match expected {
        InspectionGeometry::Line { start, end } => {
            let (line, inliers) = robust_line(&detected, options.outlier_threshold_px)
                .ok_or("Impossible d'ajuster une droite stable aux bords détectés.")?;
            let mut direction = line.direction;
            if direction.x * (end.x - start.x) + direction.y * (end.y - start.y) < 0.0 {
                direction.x = -direction.x;
                direction.y = -direction.y;
            }
            let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
            for &index in &inliers {
                let point = detected[index].point;
                let along = (point.x - line.origin.x) * direction.x
                    + (point.y - line.origin.y) * direction.y;
                low = low.min(along);
                high = high.max(along);
            }
            if high - low < start.dist(*end) * options.minimum_coverage {
                return Err("Les bords de droite sont trop concentrés sur une petite zone.".into());
            }
            let measured = InspectionGeometry::Line {
                start: Point {
                    x: line.origin.x + low * direction.x,
                    y: line.origin.y + low * direction.y,
                },
                end: Point {
                    x: line.origin.x + high * direction.x,
                    y: line.origin.y + high * direction.y,
                },
            };
            let residuals = inliers
                .iter()
                .map(|&index| line.residual(detected[index].point))
                .collect();
            (measured, inliers, residuals)
        }
        InspectionGeometry::Circle { center, radius }
        | InspectionGeometry::Arc { center, radius, .. } => {
            let (circle, inliers) = robust_circle(&detected, options.outlier_threshold_px)
                .ok_or("Impossible d'ajuster un cercle stable aux bords détectés.")?;
            // Prevent an almost-straight arc from producing a huge, plausible
            // algebraic circle outside the directed search neighborhood.
            if circle.center.dist(*center) > 2.0 * options.search_radius_px
                || (circle.radius - radius).abs() > 2.0 * options.search_radius_px
                || circle.radius < 2.0
            {
                return Err("Le cercle ajusté est instable ou sort de la zone recherchée.".into());
            }
            let measured = match expected {
                InspectionGeometry::Arc { sweep_rad, .. } => {
                    let first = detected[*inliers.first().ok_or("Arc vide.")?];
                    let last = detected[*inliers.last().ok_or("Arc vide.")?];
                    let observed_fraction =
                        (last.index - first.index) as f64 / (options.sample_count - 1) as f64;
                    if observed_fraction * sweep_rad.abs() < options.minimum_arc_sweep_rad - 1e-9 {
                        return Err("L'arc observé est trop court pour un rayon fiable.".into());
                    }
                    let start_rad =
                        (first.point.y - circle.center.y).atan2(first.point.x - circle.center.x);
                    let end_rad =
                        (last.point.y - circle.center.y).atan2(last.point.x - circle.center.x);
                    let measured_sweep = if *sweep_rad > 0.0 {
                        (end_rad - start_rad).rem_euclid(TAU)
                    } else {
                        -(start_rad - end_rad).rem_euclid(TAU)
                    };
                    if measured_sweep.abs() < options.minimum_arc_sweep_rad * 0.9 {
                        return Err(
                            "La courbure observée ne permet pas de confirmer le rayon de l'arc."
                                .into(),
                        );
                    }
                    InspectionGeometry::Arc {
                        center: circle.center,
                        radius: circle.radius,
                        start_rad,
                        sweep_rad: measured_sweep,
                    }
                }
                _ => {
                    let mut sectors = [false; 8];
                    for &index in &inliers {
                        sectors[(detected[index].index * 8 / options.sample_count).min(7)] = true;
                    }
                    if sectors.iter().filter(|&&present| present).count() < 6 {
                        return Err("Le cercle est observé sur trop peu de secteurs.".into());
                    }
                    InspectionGeometry::Circle {
                        center: circle.center,
                        radius: circle.radius,
                    }
                }
            };
            let residuals = inliers
                .iter()
                .map(|&index| (detected[index].point.dist(circle.center) - circle.radius).abs())
                .collect();
            (measured, inliers, residuals)
        }
    };
    let residuals: Vec<f64> = residuals;
    if inliers.len() < required {
        return Err(format!(
            "Contour refusé après rejet des points aberrants : {} profils cohérents, {} requis ; {} profils ambigus exclus.",
            inliers.len(),
            required,
            ambiguous_profiles
        ));
    }
    let rms_residual_px =
        (residuals.iter().map(|value| value * value).sum::<f64>() / residuals.len() as f64).sqrt();
    let max_residual_px = residuals.iter().copied().fold(0.0, f64::max);
    if !rms_residual_px.is_finite() || rms_residual_px > options.maximum_rms_px {
        return Err(format!(
            "Contour trop irrégulier : résidu RMS {rms_residual_px:.3} px."
        ));
    }
    let mut accepted_points = Vec::with_capacity(inliers.len());
    for (index, sample) in detected.iter().enumerate() {
        if inliers.binary_search(&index).is_ok() {
            accepted_points.push(sample.point);
        } else {
            rejected.push(sample.point);
        }
    }
    let coverage_fraction = accepted_points.len() as f64 / options.sample_count as f64;
    Ok(InspectionResult {
        geometry,
        evidence: VisionEvidence {
            accepted_points,
            rejected_points: rejected,
            sample_count: options.sample_count,
            rms_residual_px,
            max_residual_px,
            coverage_fraction,
            endpoint_extent_verified: false,
        },
    })
}

fn finite_point(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.x.abs() <= 1e9 && point.y.abs() <= 1e9
}

fn validate(
    image: &RgbImage,
    geometry: &InspectionGeometry,
    options: &VisionOptions,
) -> Result<(), String> {
    if image.width() < 5
        || image.height() < 5
        || image.width() as u64 * image.height() as u64 > 30_000_000
    {
        return Err("Image trop petite ou supérieure au budget de 30 millions de pixels.".into());
    }
    if !(16..=256).contains(&options.sample_count)
        || !options.search_radius_px.is_finite()
        || !(1.0..=32.0).contains(&options.search_radius_px)
        || !options.minimum_gradient.is_finite()
        || !(0.1..=255.0).contains(&options.minimum_gradient)
        || !options.minimum_coverage.is_finite()
        || !(0.5..=1.0).contains(&options.minimum_coverage)
        || !options.outlier_threshold_px.is_finite()
        || !(0.05..=5.0).contains(&options.outlier_threshold_px)
        || !options.maximum_rms_px.is_finite()
        || options.maximum_rms_px <= 0.0
        || options.maximum_rms_px > options.outlier_threshold_px
        || !options.minimum_arc_sweep_rad.is_finite()
        || !(PI / 3.0..=PI).contains(&options.minimum_arc_sweep_rad)
    {
        return Err("Options de recherche de contour invalides.".into());
    }
    match geometry {
        InspectionGeometry::Line { start, end } => {
            if !finite_point(*start) || !finite_point(*end) || start.dist(*end) < 8.0 {
                return Err("La droite attendue doit avoir deux points distincts et une longueur d'au moins 8 px.".into());
            }
        }
        InspectionGeometry::Circle { center, radius }
        | InspectionGeometry::Arc { center, radius, .. } => {
            if !finite_point(*center) || !radius.is_finite() || *radius < 3.0 || *radius > 1e9 {
                return Err("Centre ou rayon attendu invalide.".into());
            }
        }
    }
    if let InspectionGeometry::Arc {
        start_rad,
        sweep_rad,
        ..
    } = geometry
    {
        if !start_rad.is_finite()
            || !sweep_rad.is_finite()
            || sweep_rad.abs() < options.minimum_arc_sweep_rad
            || sweep_rad.abs() >= TAU - 1e-6
        {
            return Err("L'arc attendu doit couvrir au moins 60 degrés et moins d'un tour.".into());
        }
    }
    Ok(())
}

fn profile_position(geometry: &InspectionGeometry, index: usize, count: usize) -> (Point, Point) {
    match *geometry {
        InspectionGeometry::Line { start, end } => {
            let t = index as f64 / (count - 1) as f64;
            let length = start.dist(end);
            (
                Point {
                    x: start.x + t * (end.x - start.x),
                    y: start.y + t * (end.y - start.y),
                },
                Point {
                    x: -(end.y - start.y) / length,
                    y: (end.x - start.x) / length,
                },
            )
        }
        InspectionGeometry::Circle { center, radius } => {
            let angle = index as f64 * TAU / count as f64;
            let normal = Point {
                x: angle.cos(),
                y: angle.sin(),
            };
            (
                Point {
                    x: center.x + radius * normal.x,
                    y: center.y + radius * normal.y,
                },
                normal,
            )
        }
        InspectionGeometry::Arc {
            center,
            radius,
            start_rad,
            sweep_rad,
        } => {
            let angle = start_rad + index as f64 * sweep_rad / (count - 1) as f64;
            let normal = Point {
                x: angle.cos(),
                y: angle.sin(),
            };
            (
                Point {
                    x: center.x + radius * normal.x,
                    y: center.y + radius * normal.y,
                },
                normal,
            )
        }
    }
}

fn gray(image: &RgbImage, point: Point) -> Option<f64> {
    if !finite_point(point)
        || point.x < 0.0
        || point.y < 0.0
        || point.x > (image.width() - 1) as f64
        || point.y > (image.height() - 1) as f64
    {
        return None;
    }
    let x = point.x.floor() as u32;
    let y = point.y.floor() as u32;
    let next_x = (x + 1).min(image.width() - 1);
    let next_y = (y + 1).min(image.height() - 1);
    let intensity = |x, y| {
        let pixel = image.get_pixel(x, y);
        0.299 * pixel[0] as f64 + 0.587 * pixel[1] as f64 + 0.114 * pixel[2] as f64
    };
    let fx = point.x - x as f64;
    let fy = point.y - y as f64;
    Some(
        (intensity(x, y) * (1.0 - fx) + intensity(next_x, y) * fx) * (1.0 - fy)
            + (intensity(x, next_y) * (1.0 - fx) + intensity(next_x, next_y) * fx) * fy,
    )
}

fn directed_edge(
    image: &RgbImage,
    expected: Point,
    normal: Point,
    options: &VisionOptions,
) -> ProfileEdge {
    let tangent = Point {
        x: -normal.y,
        y: normal.x,
    };
    let gradient = |offset: f64| -> Option<f64> {
        let mut derivative = 0.0;
        for along in [-0.6, 0.0, 0.6] {
            let sample = |normal_offset: f64| {
                gray(
                    image,
                    Point {
                        x: expected.x + normal.x * (offset + normal_offset) + tangent.x * along,
                        y: expected.y + normal.y * (offset + normal_offset) + tangent.y * along,
                    },
                )
            };
            derivative += (sample(0.75)? - sample(-0.75)?) / 1.5;
        }
        Some(derivative / 3.0)
    };
    let step = 0.5;
    let half_steps = (options.search_radius_px / step).floor() as i32;
    let mut peaks: Vec<(f64, f64)> = Vec::new();
    for index in -half_steps..=half_steps {
        let offset = index as f64 * step;
        let Some(current) = gradient(offset) else {
            continue;
        };
        if current.abs() < options.minimum_gradient {
            continue;
        }
        if (options.polarity == EdgePolarity::Rising && current <= 0.0)
            || (options.polarity == EdgePolarity::Falling && current >= 0.0)
        {
            continue;
        }
        let Some(before) = gradient(offset - step).map(f64::abs) else {
            continue;
        };
        let Some(after) = gradient(offset + step).map(f64::abs) else {
            continue;
        };
        let strength = current.abs();
        if strength < before || strength < after || (strength == before && strength == after) {
            continue;
        }
        let curvature = before - 2.0 * strength + after;
        let correction = if curvature < -1e-9 {
            (0.5 * (before - after) / curvature).clamp(-0.75, 0.75) * step
        } else {
            0.0
        };
        let refined_offset = offset + correction;
        if refined_offset.abs() > options.search_radius_px {
            continue;
        }
        // Adjacent samples on the same peak can both pass the local-maximum
        // check (e.g. a two-sample plateau). They are one edge, not two choices.
        if let Some(previous) = peaks.last_mut() {
            if (refined_offset - previous.0).abs() <= step + 1e-6 {
                if strength > previous.1 {
                    *previous = (refined_offset, strength);
                }
                continue;
            }
        }
        peaks.push((refined_offset, strength));
    }
    peaks.sort_by(|left, right| left.0.abs().total_cmp(&right.0.abs()));
    let Some(&(offset, _)) = peaks.first() else {
        return ProfileEdge::Missing;
    };
    // Fixed conservative ambiguity policy, independent of client tolerances:
    // after polarity filtering, distinct threshold-passing peaks within 1 px
    // of the same distance to the localization are equally plausible. One
    // pixel allows for rasterization and the 0.5 px derivative sampling; it is
    // not a calibrated measurement uncertainty. Peak
    // strength alone cannot identify the intended physical contour (glare may
    // be stronger). A clearly closer peak still wins over a distant neighbor.
    // This is a localization decision, not a photometric defect classifier or
    // a guarantee that a single visible edge is the true part boundary.
    if peaks
        .iter()
        .skip(1)
        .any(|&(other, _)| other.abs() - offset.abs() <= 1.0 + 1e-6)
    {
        return ProfileEdge::Ambiguous;
    }
    ProfileEdge::Found(Point {
        x: expected.x + normal.x * offset,
        y: expected.y + normal.y * offset,
    })
}

#[derive(Clone, Copy)]
struct SupportLine {
    origin: Point,
    direction: Point,
}

impl SupportLine {
    fn residual(self, point: Point) -> f64 {
        ((point.x - self.origin.x) * self.direction.y
            - (point.y - self.origin.y) * self.direction.x)
            .abs()
    }
}

fn line_from_points(first: Point, second: Point) -> Option<SupportLine> {
    let length = first.dist(second);
    if length < 1e-6 {
        return None;
    }
    Some(SupportLine {
        origin: first,
        direction: Point {
            x: (second.x - first.x) / length,
            y: (second.y - first.y) / length,
        },
    })
}

fn fit_line(samples: &[EdgeSample], indices: &[usize]) -> Option<SupportLine> {
    if indices.len() < 2 {
        return None;
    }
    let count = indices.len() as f64;
    let origin = Point {
        x: indices
            .iter()
            .map(|&index| samples[index].point.x / count)
            .sum(),
        y: indices
            .iter()
            .map(|&index| samples[index].point.y / count)
            .sum(),
    };
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for &index in indices {
        let x = samples[index].point.x - origin.x;
        let y = samples[index].point.y - origin.y;
        xx += x * x;
        yy += y * y;
        xy += x * y;
    }
    if xx + yy < 1e-9 {
        return None;
    }
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
    Some(SupportLine {
        origin,
        direction: Point {
            x: angle.cos(),
            y: angle.sin(),
        },
    })
}

fn select_inliers(
    samples: &[EdgeSample],
    threshold: f64,
    residual: impl Fn(Point) -> f64,
) -> (Vec<usize>, f64) {
    let mut indices = Vec::new();
    let mut cost = 0.0;
    for (index, sample) in samples.iter().enumerate() {
        let error = residual(sample.point);
        if error.is_finite() && error <= threshold {
            indices.push(index);
            cost += error * error;
        }
    }
    (indices, cost)
}

fn robust_line(samples: &[EdgeSample], threshold: f64) -> Option<(SupportLine, Vec<usize>)> {
    let mut best = Vec::new();
    let mut best_cost = f64::INFINITY;
    for first in 0..samples.len() {
        for second in first + 1..samples.len() {
            let Some(line) = line_from_points(samples[first].point, samples[second].point) else {
                continue;
            };
            let (inliers, cost) = select_inliers(samples, threshold, |point| line.residual(point));
            if inliers.len() > best.len() || (inliers.len() == best.len() && cost < best_cost) {
                best = inliers;
                best_cost = cost;
            }
        }
    }
    let mut line = fit_line(samples, &best)?;
    for _ in 0..5 {
        let (inliers, _) = select_inliers(samples, threshold, |point| line.residual(point));
        if inliers == best {
            break;
        }
        best = inliers;
        line = fit_line(samples, &best)?;
    }
    let (inliers, _) = select_inliers(samples, threshold, |point| line.residual(point));
    Some((line, inliers))
}

fn robust_circle(samples: &[EdgeSample], threshold: f64) -> Option<(CircleFit, Vec<usize>)> {
    if samples.len() < 3 {
        return None;
    }
    let mut best = Vec::new();
    let mut best_cost = f64::INFINITY;
    let mut state = 0x6a09_e667_f3bc_c909_u64 ^ samples.len() as u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state as usize % samples.len()
    };
    for iteration in 0..512 {
        let indices = if iteration == 0 {
            [0, samples.len() / 2, samples.len() - 1]
        } else {
            [next(), next(), next()]
        };
        if indices[0] == indices[1] || indices[0] == indices[2] || indices[1] == indices[2] {
            continue;
        }
        let points = indices.map(|index| samples[index].point);
        let Some(circle) = fit_circle(&points) else {
            continue;
        };
        let (inliers, cost) = select_inliers(samples, threshold, |point| {
            (point.dist(circle.center) - circle.radius).abs()
        });
        if inliers.len() > best.len() || (inliers.len() == best.len() && cost < best_cost) {
            best = inliers;
            best_cost = cost;
        }
    }
    let mut circle = refine_circle(samples, &best)?;
    for _ in 0..5 {
        let (inliers, _) = select_inliers(samples, threshold, |point| {
            (point.dist(circle.center) - circle.radius).abs()
        });
        if inliers == best {
            break;
        }
        best = inliers;
        circle = refine_circle(samples, &best)?;
    }
    let (inliers, _) = select_inliers(samples, threshold, |point| {
        (point.dist(circle.center) - circle.radius).abs()
    });
    Some((circle, inliers))
}

fn refine_circle(samples: &[EdgeSample], indices: &[usize]) -> Option<CircleFit> {
    let points = indices
        .iter()
        .map(|&index| samples[index].point)
        .collect::<Vec<_>>();
    let mut circle = fit_circle(&points)?;
    // Orthogonal-distance refinement removes algebraic radius bias. The 3x3
    // system is tiny; reject ill-conditioned normal equations explicitly.
    for _ in 0..12 {
        let mut system = [[0.0; 4]; 3];
        for point in &points {
            let dx = circle.center.x - point.x;
            let dy = circle.center.y - point.y;
            let distance = dx.hypot(dy);
            if distance <= 1e-9 {
                return None;
            }
            let residual = distance - circle.radius;
            let jacobian = [dx / distance, dy / distance, -1.0];
            for row in 0..3 {
                for column in 0..3 {
                    system[row][column] += jacobian[row] * jacobian[column];
                }
                system[row][3] -= jacobian[row] * residual;
            }
        }
        let delta = solve_three(system)?;
        circle.center.x += delta[0];
        circle.center.y += delta[1];
        circle.radius += delta[2];
        if !finite_point(circle.center) || !circle.radius.is_finite() || circle.radius <= 0.0 {
            return None;
        }
        if delta.iter().map(|value| value * value).sum::<f64>() < 1e-16 {
            break;
        }
    }
    let residuals = points
        .iter()
        .map(|point| (point.dist(circle.center) - circle.radius).abs())
        .collect::<Vec<_>>();
    circle.rms_radial_error =
        (residuals.iter().map(|error| error * error).sum::<f64>() / points.len() as f64).sqrt();
    circle.max_radial_error = residuals.iter().copied().fold(0.0, f64::max);
    Some(circle)
}

fn solve_three(mut system: [[f64; 4]; 3]) -> Option<[f64; 3]> {
    let scale = system
        .iter()
        .flat_map(|row| row[..3].iter())
        .map(|value| value.abs())
        .fold(0.0, f64::max);
    for pivot in 0..3 {
        let best = (pivot..3).max_by(|&left, &right| {
            system[left][pivot]
                .abs()
                .total_cmp(&system[right][pivot].abs())
        })?;
        if system[best][pivot].abs() < scale * 1e-8 {
            return None;
        }
        system.swap(pivot, best);
        let divisor = system[pivot][pivot];
        for column in pivot..4 {
            system[pivot][column] /= divisor;
        }
        for row in 0..3 {
            if row == pivot {
                continue;
            }
            let factor = system[row][pivot];
            for column in pivot..4 {
                system[row][column] -= factor * system[pivot][column];
            }
        }
    }
    Some([system[0][3], system[1][3], system[2][3]])
}

#[cfg(test)]
#[path = "dper_vision_adversarial_tests.rs"]
mod adversarial_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    // Smooth analytic rasterization exercises subpixel localization without
    // sharing the production edge detector or fitting implementation.
    fn rasterize(intensity: impl Fn(f64, f64) -> f64) -> RgbImage {
        RgbImage::from_fn(256, 256, |x, y| {
            let value = intensity(x as f64, y as f64).round().clamp(0.0, 255.0) as u8;
            Rgb([value; 3])
        })
    }

    fn step(distance: f64) -> f64 {
        0.5 + 0.5 * (distance / 0.7).tanh()
    }

    #[test]
    fn circles_measure_actual_radius_under_translation_scale_and_noise() {
        for (center, radius) in [
            (Point { x: 87.35, y: 91.7 }, 26.4),
            (Point { x: 152.2, y: 139.6 }, 47.8),
            (Point { x: 120.6, y: 112.1 }, 65.3),
        ] {
            let image = rasterize(|x, y| {
                let distance = (x - center.x).hypot(y - center.y) - radius;
                25.0 + 205.0 * step(distance) + ((x * 13.0 + y * 7.0).sin() * 1.5)
            });
            // Deliberately incorrect nominal radius: fitting must recover the
            // image radius rather than preserving this search localization.
            let expected = InspectionGeometry::Circle {
                center: Point {
                    x: center.x + 1.4,
                    y: center.y - 0.9,
                },
                radius: radius + 2.0,
            };
            let result = inspect_geometry(&image, &expected, &VisionOptions::default()).unwrap();
            let InspectionGeometry::Circle {
                center: actual,
                radius: measured,
            } = result.geometry
            else {
                panic!()
            };
            assert!(
                actual.dist(center) < 0.15,
                "center {:?}, expected {:?}",
                actual,
                center
            );
            assert!(
                (measured - radius).abs() < 0.15,
                "radius {measured}, expected {radius}"
            );
            assert!(result.evidence.coverage_fraction > 0.95);
            assert!(result.evidence.rms_residual_px < 0.2);
        }
    }

    #[test]
    fn lines_are_rotation_and_translation_invariant() {
        for angle in [0.0_f64, 0.37, 1.4, 2.7] {
            let center = Point { x: 120.3, y: 131.7 };
            let tangent = Point {
                x: angle.cos(),
                y: angle.sin(),
            };
            let normal = Point {
                x: -tangent.y,
                y: tangent.x,
            };
            let image = rasterize(|x, y| {
                30.0 + 190.0 * step((x - center.x) * normal.x + (y - center.y) * normal.y)
            });
            let expected = InspectionGeometry::Line {
                start: Point {
                    x: center.x - 65.0 * tangent.x + 1.3 * normal.x,
                    y: center.y - 65.0 * tangent.y + 1.3 * normal.y,
                },
                end: Point {
                    x: center.x + 65.0 * tangent.x + 1.3 * normal.x,
                    y: center.y + 65.0 * tangent.y + 1.3 * normal.y,
                },
            };
            let result = inspect_geometry(&image, &expected, &VisionOptions::default()).unwrap();
            let InspectionGeometry::Line { start, end } = result.geometry else {
                panic!()
            };
            for point in [start, end] {
                assert!(
                    ((point.x - center.x) * normal.x + (point.y - center.y) * normal.y).abs()
                        < 0.12
                );
            }
            assert!(result.evidence.rms_residual_px < 0.15);
            assert!(!result.evidence.endpoint_extent_verified);
        }
    }

    fn line_search(start: Point, end: Point) -> LineSearch {
        LineSearch { start, end, options: VisionOptions::default() }
    }

    fn quarter_arc_search(center: Point, angle: f64) -> ArcEndpointSearch {
        let point = |x: f64, y: f64| Point {
            x: center.x + x * angle.cos() - y * angle.sin(),
            y: center.y + x * angle.sin() + y * angle.cos(),
        };
        ArcEndpointSearch {
            center: Point { x: center.x + 0.6, y: center.y - 0.6 },
            radius: 61.0, start_rad: angle + 0.05, sweep_rad: PI / 2.0 - 0.1,
            options: VisionOptions::default(),
            start_boundary: line_search(point(57.0, -1.3), point(30.0, -1.3)),
            end_boundary: line_search(point(-1.3, 30.0), point(-1.3, 57.0)),
        }
    }

    #[test]
    fn arc_endpoints_recover_quarter_arcs_under_rotations_and_both_sweep_signs() {
        let center = Point { x: 123.5, y: 126.5 };
        for angle in [0.0_f64, 0.45, 2.8, 5.8] {
            let image = rasterize(|x, y| {
                let x = x - center.x;
                let y = y - center.y;
                let along = x * angle.cos() + y * angle.sin();
                let across = -x * angle.sin() + y * angle.cos();
                30.0 + 190.0 * step(along.min(across).min(60.0 - x.hypot(y)))
            });
            let mut search = quarter_arc_search(center, angle);
            let result = inspect_arc_endpoints(&image, &search).unwrap();
            let expected_start = Point { x: center.x + 60.0 * angle.cos(), y: center.y + 60.0 * angle.sin() };
            let expected_end = Point { x: center.x - 60.0 * angle.sin(), y: center.y + 60.0 * angle.cos() };
            assert!(result.start.intersection_px.dist(expected_start) < 0.35, "{result:?}");
            assert!(result.end.intersection_px.dist(expected_end) < 0.35, "{result:?}");
            assert!((result.sweep_rad - PI / 2.0).abs() < 0.01);
            assert!((result.arc_length_px - 30.0 * PI).abs() < 0.5);
            assert!(result.interior_observed_points_px[0][0].dist(expected_start) < 3.5);
            assert!(result.interior_observed_points_px[1][2].dist(expected_end) < 7.5);
            assert!(!result.arc_support.evidence.endpoint_extent_verified);
            search.start_rad += search.sweep_rad;
            search.sweep_rad = -search.sweep_rad;
            std::mem::swap(&mut search.start_boundary, &mut search.end_boundary);
            let reversed = inspect_arc_endpoints(&image, &search).unwrap();
            assert!(reversed.start.intersection_px.dist(result.end.intersection_px) < 0.001);
            assert!((reversed.sweep_rad + result.sweep_rad).abs() < 0.001);
        }
    }

    #[test]
    fn arc_endpoints_refuse_missing_boundaries_and_continuation_of_either_polarity() {
        let center = Point { x: 123.5, y: 126.5 };
        let search = quarter_arc_search(center, 0.0);
        let disk = rasterize(|x, y| 30.0 + 190.0 * step(60.0 - (x-center.x).hypot(y-center.y)));
        assert!(inspect_arc_endpoints(&disk, &search).is_err());
        let image = rasterize(|x, y| {
            let x = x - center.x;
            let y = y - center.y;
            30.0 + 190.0 * (1.0 - (step(x.min(y)) - step(60.0 - x.hypot(y))).abs())
        });
        let error = inspect_arc_endpoints(&image, &search).unwrap_err();
        assert!(error.contains("se prolonge ou reste ambigu"), "{error}");
        let mut short = search;
        short.sweep_rad = 0.5;
        assert!(inspect_arc_endpoints(&image, &short).is_err());
    }

    #[test]
    fn arc_endpoints_refuse_a_locally_masked_inner_contour() {
        let center = Point { x: 123.5, y: 126.5 };
        let search = quarter_arc_search(center, 0.0);
        let image = rasterize(|x, y| {
            let x = x - center.x;
            let y = y - center.y;
            let radius = x.hypot(y);
            if (5.0..=9.5).contains(&y) && x > 54.0 && (55.0..=65.0).contains(&radius) {
                return 30.0;
            }
            30.0 + 190.0 * step(x.min(y).min(60.0 - radius))
        });
        let error = inspect_arc_endpoints(&image, &search).unwrap_err();
        assert!(error.contains("Contour intérieur"), "{error}");
    }

    #[test]
    fn arc_boundary_rejects_two_nearby_roots_and_tangent_supports() {
        let center = Point { x: 80.0, y: 80.0 };
        let radius = 6.0;
        let offset = radius * (20.0_f64.to_radians()).cos();
        let half = (radius * radius - offset * offset).sqrt();
        let a = Point { x: 80.0 - half, y: 80.0 + offset };
        let b = Point { x: 80.0 + half, y: 80.0 + offset };
        let evidence = VisionEvidence {
            accepted_points: vec![a, b], rejected_points: Vec::new(), sample_count: 2,
            rms_residual_px: 0.0, max_residual_px: 0.0,
            coverage_fraction: 1.0, endpoint_extent_verified: false,
        };
        let arc = InspectionResult {
            geometry: InspectionGeometry::Arc { center, radius, start_rad: 0.0, sweep_rad: PI },
            evidence: evidence.clone(),
        };
        let boundary = InspectionResult {
            geometry: InspectionGeometry::Line { start: a, end: b }, evidence,
        };
        let image = RgbImage::new(160, 160);
        assert!(intersect_arc_boundary(&image, &arc, &boundary, Point { x: 80.0, y: a.y }).is_err());
        let tangent = InspectionResult {
            geometry: InspectionGeometry::Line {
                start: Point { x: 70.0, y: 86.0 }, end: Point { x: 90.0, y: 86.0 },
            }, evidence: boundary.evidence,
        };
        assert!(intersect_arc_boundary(&image, &arc, &tangent, Point { x: 80.0, y: 86.0 }).is_err());
    }

    fn bounded_edge_search(point: impl Fn(f64, f64) -> Point, width: f64) -> StraightEdgeSearch {
        StraightEdgeSearch {
            edge: line_search(point(3.0, 1.3), point(width - 3.0, 1.3)),
            start_boundary: line_search(point(-1.2, 3.0), point(-1.2, 38.0)),
            end_boundary: line_search(point(width + 1.2, 3.0), point(width + 1.2, 38.0)),
        }
    }

    #[test]
    fn straight_edge_endpoints_recover_visible_corners_beyond_the_search_span() {
        let center = Point { x: 123.5, y: 126.5 };
        for angle in [0.0_f64, 0.45, 1.55, 2.7] {
            let (sine, cosine) = angle.sin_cos();
            let point = |x: f64, y: f64| Point {
                x: center.x + (x - 56.0) * cosine - y * sine,
                y: center.y + (x - 56.0) * sine + y * cosine,
            };
            let image = rasterize(|x, y| {
                let x = x - center.x;
                let y = y - center.y;
                let along = x * cosine + y * sine + 56.0;
                let across = -x * sine + y * cosine;
                30.0 + 190.0 * step(along.min(112.0 - along).min(across))
            });
            let mut search = bounded_edge_search(point, 112.0);
            let result = inspect_straight_edge_endpoints(&image, &search).unwrap();
            assert!(result.start.intersection_px.dist(point(0.0, 0.0)) < 0.25, "{result:?}");
            assert!(result.end.intersection_px.dist(point(112.0, 0.0)) < 0.25, "{result:?}");
            assert!((result.length_px - 112.0).abs() < 0.35);
            assert!(result.interior_observed_points_px[0][0].dist(point(3.0, 0.0)) < 0.35);
            assert!(result.interior_observed_points_px[1][2].dist(point(105.0, 0.0)) < 0.35);
            assert!(!result.edge_support.evidence.endpoint_extent_verified);
            std::mem::swap(&mut search.edge.start, &mut search.edge.end);
            std::mem::swap(&mut search.start_boundary, &mut search.end_boundary);
            let reversed = inspect_straight_edge_endpoints(&image, &search).unwrap();
            assert!(reversed.start.intersection_px.dist(result.end.intersection_px) < 1e-6);
        }
    }

    #[test]
    fn straight_edge_endpoints_refuse_missing_misordered_or_unobservable_boundaries() {
        let search = bounded_edge_search(|x, y| Point { x: 50.5 + x, y: 90.5 + y }, 140.0);
        let image = rasterize(|x, y| 30.0 + 190.0 * step((x - 50.5).min(y - 90.5)));
        let error = inspect_straight_edge_endpoints(&image, &search).unwrap_err();
        assert!(error.starts_with("Bord transverse de fin"), "{error}");

        let mut swapped = search;
        std::mem::swap(&mut swapped.start_boundary, &mut swapped.end_boundary);
        let image = rasterize(|x, y| 30.0 + 190.0 * step((x - 50.5).min(190.5 - x).min(y - 90.5)));
        let error = inspect_straight_edge_endpoints(&image, &swapped).unwrap_err();
        assert!(error.contains("extrémités opposées"), "{error}");

        let search = bounded_edge_search(|x, y| Point { x: 8.5 + x, y: 90.5 + y }, 140.0);
        let image = rasterize(|x, y| 30.0 + 190.0 * step((x - 8.5).min(148.5 - x).min(y - 90.5)));
        let error = inspect_straight_edge_endpoints(&image, &search).unwrap_err();
        assert!(error.contains("prolongement hors de l'image"), "{error}");
    }

    #[test]
    fn straight_edge_endpoints_refuse_a_locally_masked_inner_contour() {
        let search = bounded_edge_search(|x, y| Point { x: 50.5 + x, y: 90.5 + y }, 140.0);
        let image = rasterize(|x, y| {
            if (56.0..=61.0).contains(&x) && (87.0..=96.0).contains(&y) {
                return 30.0;
            }
            30.0 + 190.0 * step((x - 50.5).min(190.5 - x).min(y - 90.5))
        });
        let error = inspect_straight_edge_endpoints(&image, &search).unwrap_err();
        assert!(error.contains("Contour intérieur"), "{error}");
    }

    #[test]
    fn straight_edge_endpoints_refuse_continuation_even_at_reversed_polarity() {
        let mut search = bounded_edge_search(|x, y| Point { x: 50.5 + x, y: 90.5 + y }, 140.0);
        search.edge.options.polarity = EdgePolarity::Rising;
        let image = rasterize(|x, y| {
            let inside = (x - 50.5).min(190.5 - x);
            30.0 + 190.0 * (1.0 - (step(inside) - step(y - 90.5)).abs())
        });
        let error = inspect_straight_edge_endpoints(&image, &search).unwrap_err();
        assert!(error.contains("se prolonge ou reste ambigu"), "{error}");
    }

    #[test]
    fn line_intersection_uses_pixels_near_the_crossing_at_multiple_rotations() {
        let center = Point { x: 120.5, y: 130.5 };
        for angle in [0.0_f64, 0.4, 1.5, 2.8] {
            let (sine, cosine) = angle.sin_cos();
            let point = |x: f64, y: f64| Point {
                x: center.x + x * cosine - y * sine,
                y: center.y + x * sine + y * cosine,
            };
            let image = rasterize(|x, y| {
                let x = x - center.x;
                let y = y - center.y;
                30.0 + 190.0 * step((x * cosine + y * sine).min(-x * sine + y * cosine))
            });
            let first = line_search(point(65.0, 1.4), point(3.0, 1.4));
            let second = line_search(point(-1.2, 3.0), point(-1.2, 65.0));
            for searches in [[first.clone(), second.clone()], [second, first]] {
                let result = inspect_line_intersection(&image, &searches).unwrap();
                assert!(result.intersection_px.dist(center) < 0.25, "{result:?}");
                assert!((result.acute_angle_deg - 90.0).abs() < 0.25);
                assert!(result.nearest_observation_px.iter().all(|&distance| (2.0..=4.0).contains(&distance)));
                assert!(result.supports.iter().all(|support| !support.evidence.endpoint_extent_verified));
            }
        }
    }

    #[test]
    fn line_intersection_refuses_a_hidden_or_distant_crossing() {
        let first = line_search(Point { x: 136.0, y: 131.0 }, Point { x: 220.0, y: 131.0 });
        let second = line_search(Point { x: 120.0, y: 147.0 }, Point { x: 120.0, y: 220.0 });
        let image = rasterize(|x, y| 30.0 + 190.0 * step((x - 120.5).min(y - 130.5)));
        let error = inspect_line_intersection(&image, &[first.clone(), second.clone()]).unwrap_err();
        assert!(error.contains("insuffisamment étayée"), "{error}");

        let image = rasterize(|_, y| 30.0 + 190.0 * step(y - 130.5));
        let error = inspect_line_intersection(&image, &[first, second]).unwrap_err();
        assert!(error.starts_with("Second bord"), "{error}");
    }

    #[test]
    fn line_intersection_refuses_nearly_parallel_supports_and_out_of_frame_crossings() {
        let nearly_parallel = rasterize(|x, y| {
            30.0 + 190.0 * step((y - 60.5).min(60.5 + 0.17 * x - y))
        });
        let horizontal = line_search(Point { x: 110.0, y: 60.0 }, Point { x: 210.0, y: 60.0 });
        let shallow = line_search(Point { x: 110.0, y: 79.2 }, Point { x: 210.0, y: 96.2 });
        let error = inspect_line_intersection(&nearly_parallel, &[horizontal, shallow]).unwrap_err();
        assert!(error.contains("presque parallèles"), "{error}");

        let outside = rasterize(|x, y| 30.0 + 190.0 * step((y - 60.5).min(x + 80.5 - y)));
        let horizontal = line_search(Point { x: 30.0, y: 60.0 }, Point { x: 110.0, y: 60.0 });
        let diagonal = line_search(Point { x: 30.0, y: 110.5 }, Point { x: 110.0, y: 190.5 });
        let error = inspect_line_intersection(&outside, &[horizontal, diagonal]).unwrap_err();
        assert!(error.contains("hors de l'image"), "{error}");
    }

    #[test]
    fn directed_profiles_prefer_nearby_edge_over_stronger_neighbor() {
        let image = rasterize(|x, _| 15.0 + 60.0 * step(x - 91.35) + 170.0 * step(x - 97.0));
        let geometry = InspectionGeometry::Line {
            start: Point { x: 92.0, y: 30.0 },
            end: Point { x: 92.0, y: 225.0 },
        };
        let result = inspect_geometry(&image, &geometry, &VisionOptions::default()).unwrap();
        let InspectionGeometry::Line { start, end } = result.geometry else {
            panic!()
        };
        assert!((start.x - 91.35).abs() < 0.15);
        assert!((end.x - 91.35).abs() < 0.15);
    }

    #[test]
    fn arcs_recover_radius_with_both_sweep_directions_and_refuse_short_arcs() {
        let center = Point { x: 123.4, y: 118.7 };
        let image =
            rasterize(|x, y| 20.0 + 210.0 * step((x - center.x).hypot(y - center.y) - 53.6));
        for sweep_rad in [2.4, -2.4] {
            let expected = InspectionGeometry::Arc {
                center: Point {
                    x: center.x + 1.0,
                    y: center.y,
                },
                radius: 55.0,
                start_rad: 2.9,
                sweep_rad,
            };
            let result = inspect_geometry(&image, &expected, &VisionOptions::default()).unwrap();
            let InspectionGeometry::Arc {
                radius,
                sweep_rad: actual_sweep,
                ..
            } = result.geometry
            else {
                panic!()
            };
            assert!((radius - 53.6).abs() < 0.25);
            assert_eq!(actual_sweep.signum(), sweep_rad.signum());
        }
        let short = InspectionGeometry::Arc {
            center,
            radius: 53.6,
            start_rad: 0.0,
            sweep_rad: 0.3,
        };
        assert!(inspect_geometry(&image, &short, &VisionOptions::default()).is_err());
    }

    #[test]
    fn robust_fit_rejects_local_contamination_but_missing_contours_fail() {
        let geometry = InspectionGeometry::Line {
            start: Point { x: 90.5, y: 20.0 },
            end: Point { x: 90.5, y: 235.0 },
        };
        let image = rasterize(|x, y| {
            let edge = if (90.0..120.0).contains(&y) {
                96.0
            } else {
                90.3
            };
            20.0 + 210.0 * step(x - edge)
        });
        let result = inspect_geometry(&image, &geometry, &VisionOptions::default()).unwrap();
        assert!(result.evidence.coverage_fraction > 0.8);
        assert!(!result.evidence.rejected_points.is_empty());
        let InspectionGeometry::Line { start, .. } = result.geometry else {
            panic!()
        };
        assert!((start.x - 90.3).abs() < 0.15);
        let partial = rasterize(|x, y| {
            if y > 110.0 {
                30.0
            } else {
                20.0 + 210.0 * step(x - 90.3)
            }
        });
        assert!(inspect_geometry(&partial, &geometry, &VisionOptions::default()).is_err());
        let empty = RgbImage::from_pixel(256, 256, Rgb([80; 3]));
        assert!(inspect_geometry(&empty, &geometry, &VisionOptions::default()).is_err());
    }

    #[test]
    fn irregular_contours_and_invalid_inputs_are_refused() {
        let geometry = InspectionGeometry::Line {
            start: Point { x: 90.5, y: 20.0 },
            end: Point { x: 90.5, y: 235.0 },
        };
        let image = rasterize(|x, y| 20.0 + 210.0 * step(x - 90.3 - 2.0 * (y * 0.2).sin()));
        assert!(inspect_geometry(&image, &geometry, &VisionOptions::default()).is_err());
        let options = VisionOptions {
            maximum_rms_px: f64::NAN,
            ..VisionOptions::default()
        };
        assert!(inspect_geometry(&image, &geometry, &options).is_err());
        let invalid = InspectionGeometry::Circle {
            center: Point {
                x: f64::NAN,
                y: 0.0,
            },
            radius: 4.0,
        };
        assert!(inspect_geometry(&image, &invalid, &VisionOptions::default()).is_err());
    }
}

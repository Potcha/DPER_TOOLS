//! Noyau de mesure : toutes les coordonnées sont en pixels natifs de l'image.
//! Le facteur de calibration est exprimé en millimètres par pixel.
use serde::{Deserialize, Serialize};
use rmcp::schemars;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq, schemars::JsonSchema)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn dist(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Tool {
    Distance,
    Polyline,
    Angle,
    Circle,
    Rectangle,
    Polygon,
    PointLineDistance,
    ParallelDistance,
    CircleFit,
    Ellipse,
    Arc,
    Coordinate,
    ObjectCount,
}
impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Distance => "Distance",
            Self::Polyline => "Ligne brisée",
            Self::Angle => "Angle (3 points)",
            Self::Circle => "Cercle (centre + bord)",
            Self::Rectangle => "Rectangle",
            Self::Polygon => "Polygone (surface)",
            Self::PointLineDistance => "Perpendiculaire point-ligne",
            Self::ParallelDistance => "Distance entre parallèles",
            Self::CircleFit => "Cercle ajusté",
            Self::Ellipse => "Ellipse",
            Self::Arc => "Arc (3 points)",
            Self::Coordinate => "Coordonnées",
            Self::ObjectCount => "Comptage manuel",
        }
    }
    pub fn short_label(self) -> &'static str {
        match self {
            Self::PointLineDistance => "Point-ligne",
            Self::ParallelDistance => "Parallèles",
            Self::CircleFit => "Cercle ajusté",
            Self::Coordinate => "Coordonnées",
            Self::ObjectCount => "Compter objets",
            _ => self.label(),
        }
    }
    pub fn instruction(self) -> &'static str {
        match self {
            Self::Distance => "Cliquez les deux extrémités de la distance.",
            Self::Polyline => "Cliquez les sommets, puis terminez le tracé.",
            Self::Angle => "Cliquez le sommet, puis un point sur chaque branche.",
            Self::Circle => "Cliquez le centre, puis un point du bord.",
            Self::Rectangle => "Cliquez deux coins opposés.",
            Self::Polygon => "Cliquez les sommets, puis terminez le tracé.",
            Self::PointLineDistance => "Cliquez le point mesuré, puis deux points de la droite.",
            Self::ParallelDistance => "Cliquez deux points sur chaque droite.",
            Self::CircleFit => "Cliquez au moins trois points du contour, puis terminez.",
            Self::Ellipse => "Cliquez le centre et les extrémités des deux demi-axes conjugués.",
            Self::Arc => "Cliquez le début, un point sur l’arc, puis la fin.",
            Self::Coordinate => "Cliquez le point dont vous voulez relever les coordonnées.",
            Self::ObjectCount => "Cliquez une fois sur chaque objet à compter, puis terminez le tracé.",
        }
    }
    pub fn required_points(self) -> Option<usize> {
        match self {
            Self::Distance | Self::Circle | Self::Rectangle => Some(2),
            Self::Angle | Self::PointLineDistance | Self::Ellipse | Self::Arc => Some(3),
            Self::ParallelDistance => Some(4),
            Self::Coordinate => Some(1),
            Self::Polyline | Self::Polygon | Self::CircleFit | Self::ObjectCount => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleFit {
    pub center: Point,
    pub radius: f64,
    pub rms_radial_error: f64,
    pub max_radial_error: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcGeometry {
    pub center: Point,
    pub radius: f64,
    pub start_angle: f64,
    /// Positive values travel counter-clockwise in image coordinates.
    pub sweep_radians: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipseGeometry {
    pub center: Point,
    pub first_axis: Point,
    pub second_axis: Point,
    pub semi_major: f64,
    pub semi_minor: f64,
    pub area: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Measurement {
    pub id: u64,
    pub kind: Tool,
    pub points: Vec<Point>,
    pub label: String,
    /// Optional top-left position of the displayed label in native image pixels.
    #[serde(default)]
    pub label_position_native_px: Option<Point>,
    #[serde(default)]
    pub capture_id: String,
    #[serde(default)]
    pub calibration_id: Option<String>,
    #[serde(default)]
    pub mm_per_pixel: Option<f64>,
    #[serde(default)]
    pub mm_per_pixel_x: Option<f64>,
    #[serde(default)]
    pub mm_per_pixel_y: Option<f64>,
    /// Expanded measurement uncertainty U, in millimetres; absent means no formal verdict.
    #[serde(default)]
    pub expanded_uncertainty_mm: Option<f64>,
    /// Origin is `interface` for operator-created marks and `mcp` for AI-created marks.
    #[serde(default = "default_measurement_origin")]
    pub origin: String,
    /// Optional method or inspection note supplied when the measurement was created.
    #[serde(default)]
    pub method: Option<String>,
    /// Versioned protocol applied when this result was created.
    #[serde(default)]
    pub protocol_id: Option<String>,
    #[serde(default)]
    pub protocol_version: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub text: String,
    pub target: AnnotationTarget,
    #[serde(default)]
    pub created_at_unix_secs: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AnnotationTarget {
    Measurement { measurement_id: u64 },
    Point { point: Point },
    Zone { first: Point, second: Point },
}

fn default_measurement_origin() -> String { "interface".into() }

pub fn polyline_length(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| w[0].dist(w[1])).sum()
}

pub fn polygon_area(pts: &[Point]) -> f64 {
    if pts.len() < 3 { return 0.0; }
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let j = (i + 1) % pts.len();
        sum += pts[i].x * pts[j].y - pts[j].x * pts[i].y;
    }
    sum.abs() * 0.5
}

/// Returns whether the current points describe a measurable, non-degenerate shape.
/// The small epsilon is in native image pixels (or squared pixels for area).
pub fn is_nondegenerate(kind: Tool, pts: &[Point]) -> bool {
    const EPSILON: f64 = 1e-9;
    match kind {
        Tool::Distance => pts.len() >= 2 && pts[0].dist(pts[1]) > EPSILON,
        Tool::Polyline => pts.len() >= 2 && polyline_length(pts) > EPSILON,
        Tool::Angle => pts.len() >= 3 && angle_deg(pts[0], pts[1], pts[2]).is_some(),
        Tool::Circle => pts.len() >= 2 && pts[0].dist(pts[1]) > EPSILON,
        Tool::Rectangle => pts.len() >= 2
            && (pts[0].x - pts[1].x).abs() > EPSILON
            && (pts[0].y - pts[1].y).abs() > EPSILON,
        Tool::Polygon => pts.len() >= 3 && polygon_area(pts) > EPSILON,
        Tool::PointLineDistance => pts.len() >= 3 && pts[1].dist(pts[2]) > EPSILON,
        Tool::ParallelDistance => pts.len() >= 4
            && pts[0].dist(pts[1]) > EPSILON && pts[2].dist(pts[3]) > EPSILON,
        Tool::CircleFit => pts.len() >= 3 && fit_circle(pts).is_some(),
        Tool::Ellipse => pts.len() >= 3 && ellipse_geometry(pts).is_some(),
        Tool::Arc => pts.len() >= 3 && arc_geometry(pts).is_some(),
        Tool::Coordinate => pts.len() == 1,
        Tool::ObjectCount => !pts.is_empty(),
    }
}

/// Algebraic least-squares circle fit for three or more image points.
pub fn fit_circle(points: &[Point]) -> Option<CircleFit> {
    if points.len() < 3 || points.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()) {
        return None;
    }
    let origin = Point {
        x: points.iter().map(|point| point.x).sum::<f64>() / points.len() as f64,
        y: points.iter().map(|point| point.y).sum::<f64>() / points.len() as f64,
    };
    let mut matrix = [[0.0; 4]; 3];
    for point in points {
        let x = point.x - origin.x;
        let y = point.y - origin.y;
        let z = x * x + y * y;
        matrix[0][0] += x * x;
        matrix[0][1] += x * y;
        matrix[0][2] += x;
        matrix[0][3] -= x * z;
        matrix[1][0] += x * y;
        matrix[1][1] += y * y;
        matrix[1][2] += y;
        matrix[1][3] -= y * z;
        matrix[2][0] += x;
        matrix[2][1] += y;
        matrix[2][2] += 1.0;
        matrix[2][3] -= z;
    }
    for pivot in 0..3 {
        let best = (pivot..3).max_by(|left, right|
            matrix[*left][pivot].abs().total_cmp(&matrix[*right][pivot].abs()))?;
        if matrix[best][pivot].abs() <= 1e-12 { return None; }
        matrix.swap(pivot, best);
        let divisor = matrix[pivot][pivot];
        for column in pivot..4 { matrix[pivot][column] /= divisor; }
        for row in 0..3 {
            if row == pivot { continue; }
            let factor = matrix[row][pivot];
            for column in pivot..4 { matrix[row][column] -= factor * matrix[pivot][column]; }
        }
    }
    let d = matrix[0][3];
    let e = matrix[1][3];
    let f = matrix[2][3];
    let center = Point { x: origin.x - d * 0.5, y: origin.y - e * 0.5 };
    let radius_squared = (d * d + e * e) * 0.25 - f;
    if !radius_squared.is_finite() || radius_squared <= 1e-12 { return None; }
    let radius = radius_squared.sqrt();
    let residuals = points.iter().map(|point| center.dist(*point) - radius).collect::<Vec<_>>();
    let rms_radial_error = (residuals.iter().map(|error| error * error).sum::<f64>()
        / residuals.len() as f64).sqrt();
    let max_radial_error = residuals.iter().map(|error| error.abs()).fold(0.0, f64::max);
    Some(CircleFit { center, radius, rms_radial_error, max_radial_error })
}

pub fn point_line_distance(point: Point, first: Point, second: Point) -> Option<f64> {
    let dx = second.x - first.x;
    let dy = second.y - first.y;
    let length = dx.hypot(dy);
    if !length.is_finite() || length <= 1e-12 { return None; }
    Some(((point.x - first.x) * dy - (point.y - first.y) * dx).abs() / length)
}

/// Returns the perpendicular separation of two lines and their angular mismatch.
pub fn parallel_line_distance(first_a: Point, first_b: Point, second_a: Point, second_b: Point)
    -> Option<(f64, f64)> {
    let first_dx = first_b.x - first_a.x;
    let first_dy = first_b.y - first_a.y;
    let second_dx = second_b.x - second_a.x;
    let second_dy = second_b.y - second_a.y;
    let first_length = first_dx.hypot(first_dy);
    let second_length = second_dx.hypot(second_dy);
    if first_length <= 1e-12 || second_length <= 1e-12 { return None; }
    let dot = first_dx * second_dx + first_dy * second_dy;
    let cross = first_dx * second_dy - first_dy * second_dx;
    let angle_error = cross.abs().atan2(dot.abs()).to_degrees();
    let midpoint_a = Point { x: (first_a.x + first_b.x) * 0.5, y: (first_a.y + first_b.y) * 0.5 };
    let midpoint_b = Point { x: (second_a.x + second_b.x) * 0.5, y: (second_a.y + second_b.y) * 0.5 };
    let separation = ((midpoint_b.x - midpoint_a.x) * first_dy
        - (midpoint_b.y - midpoint_a.y) * first_dx).abs() / first_length;
    (separation.is_finite() && angle_error.is_finite()).then_some((separation, angle_error))
}

pub fn ellipse_geometry(points: &[Point]) -> Option<EllipseGeometry> {
    if points.len() < 3 { return None; }
    let center = points[0];
    let first_axis = Point { x: points[1].x - center.x, y: points[1].y - center.y };
    let second_axis = Point { x: points[2].x - center.x, y: points[2].y - center.y };
    let first_length = first_axis.x.hypot(first_axis.y);
    let second_length = second_axis.x.hypot(second_axis.y);
    let determinant = first_axis.x * second_axis.y - first_axis.y * second_axis.x;
    let trace = first_axis.x.powi(2) + first_axis.y.powi(2)
        + second_axis.x.powi(2) + second_axis.y.powi(2);
    if first_length <= 1e-12 || second_length <= 1e-12 || determinant.abs() <= 1e-12 { return None; }
    let discriminant = (trace * trace - 4.0 * determinant * determinant).max(0.0).sqrt();
    let eigen_max = (trace + discriminant) * 0.5;
    let eigen_min = (trace - discriminant) * 0.5;
    if eigen_min <= 0.0 { return None; }
    Some(EllipseGeometry {
        center, first_axis, second_axis,
        semi_major: eigen_max.sqrt(), semi_minor: eigen_min.sqrt(),
        area: std::f64::consts::PI * determinant.abs(),
    })
}

pub fn arc_geometry(points: &[Point]) -> Option<ArcGeometry> {
    if points.len() < 3 { return None; }
    let circle = fit_circle(&points[..3])?;
    let angle = |point: Point| (point.y - circle.center.y).atan2(point.x - circle.center.x);
    let start = angle(points[0]);
    let middle = angle(points[1]);
    let end = angle(points[2]);
    let ccw_end = (end - start).rem_euclid(std::f64::consts::TAU);
    let ccw_middle = (middle - start).rem_euclid(std::f64::consts::TAU);
    let sweep = if ccw_middle <= ccw_end { ccw_end } else { ccw_end - std::f64::consts::TAU };
    (sweep.abs() > 1e-10).then_some(ArcGeometry {
        center: circle.center, radius: circle.radius, start_angle: start, sweep_radians: sweep,
    })
}

/// Sample a curved measurement in native image coordinates for drawing/export.
pub fn sampled_curve(kind: Tool, points: &[Point], segments: usize) -> Option<Vec<Point>> {
    let segments = segments.max(8);
    let (center, radius, start, sweep, ellipse_axes) = match kind {
        Tool::Circle if points.len() >= 2 => (points[0], points[0].dist(points[1]), 0.0,
            std::f64::consts::TAU, None),
        Tool::CircleFit => {
            let circle = fit_circle(points)?;
            (circle.center, circle.radius, 0.0, std::f64::consts::TAU, None)
        }
        Tool::Arc => {
            let arc = arc_geometry(points)?;
            (arc.center, arc.radius, arc.start_angle, arc.sweep_radians, None)
        }
        Tool::Ellipse => {
            let ellipse = ellipse_geometry(points)?;
            (ellipse.center, 0.0, 0.0, std::f64::consts::TAU,
                Some((ellipse.first_axis, ellipse.second_axis)))
        }
        _ => return None,
    };
    Some((0..=segments).map(|index| {
        let angle = start + sweep * index as f64 / segments as f64;
        if let Some((first, second)) = ellipse_axes {
            Point {
                x: center.x + first.x * angle.cos() + second.x * angle.sin(),
                y: center.y + first.y * angle.cos() + second.y * angle.sin(),
            }
        } else {
            Point { x: center.x + radius * angle.cos(), y: center.y + radius * angle.sin() }
        }
    }).collect())
}

/// Returns whether the measured points or any derived drawing extend beyond the source image.
pub fn exceeds_image_bounds(kind: Tool, points: &[Point], width: u32, height: u32) -> bool {
    let outside = |point: Point| {
        !point.x.is_finite() || !point.y.is_finite()
            || point.x < 0.0 || point.y < 0.0
            || point.x > width as f64 || point.y > height as f64
    };
    if width == 0 || height == 0 || points.iter().copied().any(outside) { return true; }

    match kind {
        Tool::Circle if points.len() >= 2 => {
            let radius = points[0].dist(points[1]);
            if [
                Point { x: points[0].x - radius, y: points[0].y },
                Point { x: points[0].x + radius, y: points[0].y },
                Point { x: points[0].x, y: points[0].y - radius },
                Point { x: points[0].x, y: points[0].y + radius },
            ].into_iter().any(outside) { return true; }
        }
        Tool::CircleFit if points.len() >= 3 => {
            if let Some(circle) = fit_circle(points) {
                if [
                    Point { x: circle.center.x - circle.radius, y: circle.center.y },
                    Point { x: circle.center.x + circle.radius, y: circle.center.y },
                    Point { x: circle.center.x, y: circle.center.y - circle.radius },
                    Point { x: circle.center.x, y: circle.center.y + circle.radius },
                ].into_iter().any(outside) { return true; }
            }
        }
        Tool::Ellipse if points.len() >= 3 => {
            if let Some(ellipse) = ellipse_geometry(points) {
                let radius_x = ellipse.first_axis.x.hypot(ellipse.second_axis.x);
                let radius_y = ellipse.first_axis.y.hypot(ellipse.second_axis.y);
                if outside(Point { x: ellipse.center.x - radius_x, y: ellipse.center.y })
                    || outside(Point { x: ellipse.center.x + radius_x, y: ellipse.center.y })
                    || outside(Point { x: ellipse.center.x, y: ellipse.center.y - radius_y })
                    || outside(Point { x: ellipse.center.x, y: ellipse.center.y + radius_y })
                { return true; }
            }
        }
        Tool::Arc if points.len() >= 3 => {
            if let Some(arc) = arc_geometry(points) {
                for angle in [0.0, std::f64::consts::FRAC_PI_2,
                    std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2] {
                    let progress = if arc.sweep_radians >= 0.0 {
                        (angle - arc.start_angle).rem_euclid(std::f64::consts::TAU)
                    } else {
                        (arc.start_angle - angle).rem_euclid(std::f64::consts::TAU)
                    };
                    if progress <= arc.sweep_radians.abs() + 1e-12
                        && outside(Point { x: arc.center.x + arc.radius * angle.cos(),
                            y: arc.center.y + arc.radius * angle.sin() })
                    { return true; }
                }
            }
        }
        _ => {}
    }

    match kind {
        Tool::PointLineDistance if points.len() >= 3 => {
            let axis = Point { x: points[2].x - points[1].x, y: points[2].y - points[1].y };
            let length_squared = axis.x * axis.x + axis.y * axis.y;
            if length_squared <= f64::EPSILON { return false; }
            let relative = Point { x: points[0].x - points[1].x, y: points[0].y - points[1].y };
            let factor = (relative.x * axis.x + relative.y * axis.y) / length_squared;
            outside(Point { x: points[1].x + axis.x * factor, y: points[1].y + axis.y * factor })
        }
        Tool::ParallelDistance if points.len() >= 4 => {
            let axis = Point { x: points[1].x - points[0].x, y: points[1].y - points[0].y };
            let length = axis.x.hypot(axis.y);
            if length <= f64::EPSILON { return false; }
            let normal = Point { x: -axis.y / length, y: axis.x / length };
            let first_midpoint = Point {
                x: (points[0].x + points[1].x) * 0.5,
                y: (points[0].y + points[1].y) * 0.5,
            };
            let second_midpoint = Point {
                x: (points[2].x + points[3].x) * 0.5,
                y: (points[2].y + points[3].y) * 0.5,
            };
            let separation = (second_midpoint.x - first_midpoint.x) * normal.x
                + (second_midpoint.y - first_midpoint.y) * normal.y;
            outside(Point {
                x: first_midpoint.x + normal.x * separation,
                y: first_midpoint.y + normal.y * separation,
            })
        }
        _ => false,
    }
}

pub fn angle_deg(a: Point, vertex: Point, b: Point) -> Option<f64> {
    let u = (a.x - vertex.x, a.y - vertex.y);
    let v = (b.x - vertex.x, b.y - vertex.y);
    let norms = u.0.hypot(u.1) * v.0.hypot(v.1);
    if norms <= 1e-12 { return None; }
    let cos = ((u.0 * v.0 + u.1 * v.1) / norms).clamp(-1.0, 1.0);
    Some(cos.acos().to_degrees())
}

pub fn result(m: &Measurement) -> String {
    let p = &m.points;
    // None means this measurement was made without calibration; do not inherit
    // a calibration that may have been added to the same image later.
    let scale = m.mm_per_pixel.filter(|v| *v > 0.0 && v.is_finite());
    let scale_x = m.mm_per_pixel_x.filter(|v| *v > 0.0 && v.is_finite()).or(scale);
    let scale_y = m.mm_per_pixel_y.filter(|v| *v > 0.0 && v.is_finite()).or(scale);
    let calibrated = scale_x.is_some() && scale_y.is_some();
    let points = if calibrated { p.iter().copied().map(|point| metric_point(m, point)).collect::<Vec<_>>() }
        else { p.clone() };
    let len = |px: f64| if calibrated { format!("{px:.3} mm") }
        else { format!("{px:.1} px (non étalonné)") };
    let area = |px2: f64| if calibrated { format!("{px2:.4} mm²") }
        else { format!("{px2:.1} px² (non étalonné)") };
    match m.kind {
        Tool::Distance if points.len() >= 2 => len(points[0].dist(points[1])),
        Tool::Polyline if points.len() >= 2 => len(polyline_length(&points)),
        Tool::Angle if points.len() >= 3 => angle_deg(points[0], points[1], points[2])
            .map(|d| format!("{:.2}°", d)).unwrap_or_else(|| "Angle indéfini".into()),
        Tool::Circle if points.len() >= 2 => format!("Ø {}", len(2.0 * points[0].dist(points[1]))),
        Tool::Rectangle if points.len() >= 2 => {
            let dx = (points[0].x - points[1].x).abs();
            let dy = (points[0].y - points[1].y).abs();
            format!("L {} × H {} · S {}", len(dx), len(dy), area(dx * dy))
        }
        Tool::Polygon if points.len() >= 3 => area(polygon_area(&points)),
        Tool::PointLineDistance if points.len() >= 3 => point_line_distance(points[0], points[1], points[2])
            .map(len).unwrap_or_else(|| "Droite indéfinie".into()),
        Tool::ParallelDistance if points.len() >= 4 => parallel_line_distance(points[0], points[1], points[2], points[3])
            .map(|(distance, angle)| format!("Δ⊥ {} · Δθ {:.3}°", len(distance), angle))
            .unwrap_or_else(|| "Droites indéfinies".into()),
        Tool::CircleFit if points.len() >= 3 => fit_circle(&points)
            .map(|circle| format!("Ø {} · erreur radiale RMS {}", len(circle.radius * 2.0), len(circle.rms_radial_error)))
            .unwrap_or_else(|| "Cercle indéfini".into()),
        Tool::Ellipse if points.len() >= 3 => ellipse_geometry(&points)
            .map(|ellipse| format!("A {} · axes {} / {}", area(ellipse.area), len(ellipse.semi_major * 2.0), len(ellipse.semi_minor * 2.0)))
            .unwrap_or_else(|| "Ellipse indéfinie".into()),
        Tool::Arc if points.len() >= 3 => arc_geometry(&points)
            .map(|arc| format!("Arc {} · {:.2}°", len(arc.radius * arc.sweep_radians.abs()), arc.sweep_radians.abs().to_degrees()))
            .unwrap_or_else(|| "Arc indéfini".into()),
        Tool::Coordinate if !points.is_empty() => match calibrated {
            true => format!("X {:.3} mm · Y {:.3} mm · ({:.1}; {:.1}) px", points[0].x, points[0].y, p[0].x, p[0].y),
            false => format!("X {:.1} px · Y {:.1} px", p[0].x, p[0].y),
        },
        Tool::ObjectCount if !points.is_empty() => {
            let count = points.len();
            format!("{count} objet{}", if count == 1 { "" } else { "s" })
        }
        _ => "Mesure incomplète".into(),
    }
}

pub fn metric_point(measurement: &Measurement, point: Point) -> Point {
    let scale = measurement.mm_per_pixel.filter(|value| value.is_finite() && *value > 0.0);
    let scale_x = measurement.mm_per_pixel_x.filter(|value| value.is_finite() && *value > 0.0).or(scale);
    let scale_y = measurement.mm_per_pixel_y.filter(|value| value.is_finite() && *value > 0.0).or(scale);
    Point { x: point.x * scale_x.unwrap_or(1.0), y: point.y * scale_y.unwrap_or(1.0) }
}

pub fn metric_points(measurement: &Measurement) -> Option<Vec<Point>> {
    let fallback = measurement.mm_per_pixel.filter(|value| value.is_finite() && *value > 0.0);
    let scale_x = measurement.mm_per_pixel_x.filter(|value| value.is_finite() && *value > 0.0).or(fallback);
    let scale_y = measurement.mm_per_pixel_y.filter(|value| value.is_finite() && *value > 0.0).or(fallback);
    let calibrated = scale_x.is_some() && scale_y.is_some();
    calibrated.then(|| measurement.points.iter().copied()
        .map(|point| metric_point(measurement, point)).collect())
}

pub fn calibration_mm_per_px(a: Point, b: Point, real_mm: f64) -> Result<f64, &'static str> {
    let px = a.dist(b);
    if real_mm <= 0.0 || !real_mm.is_finite() { return Err("Distance réelle invalide"); }
    if px < 2.0 || !px.is_finite() { return Err("Référence trop courte"); }
    Ok(real_mm / px)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_cm_reference() {
        let s = calibration_mm_per_px(Point {x: 0.0, y:0.0}, Point {x: 500.0, y:0.0}, 10.0).unwrap();
        assert!((s - 0.02).abs() < 1e-10);
    }
    #[test]
    fn right_angle() {
        let a = angle_deg(Point{x:1.0,y:0.0},Point{x:0.0,y:0.0},Point{x:0.0,y:1.0}).unwrap();
        assert!((a - 90.0).abs() < 1e-8);
    }
    #[test]
    fn square_area() {
        let p = [Point{x:0.0,y:0.0}, Point{x:10.0,y:0.0},Point{x:10.0,y:10.0}, Point{x:0.0,y:10.0}];
        assert!((polygon_area(&p) - 100.0).abs() < 1e-8);
    }
    #[test]
    fn zero_calibration_rejected() {
        assert!(calibration_mm_per_px(Point{x:0.0,y:0.0},Point{x:0.0,y:0.0},10.0).is_err());
    }

    #[test]
    fn advanced_geometry_tools_return_expected_dimensions() {
        let circle_points = [
            Point { x: 5.0, y: 2.0 }, Point { x: 3.0, y: 4.0 },
            Point { x: 1.0, y: 2.0 }, Point { x: 3.0, y: 0.0 },
        ];
        let circle = fit_circle(&circle_points).unwrap();
        assert!((circle.center.x - 3.0).abs() < 1e-10);
        assert!((circle.center.y - 2.0).abs() < 1e-10);
        assert!((circle.radius - 2.0).abs() < 1e-10);
        assert!(circle.max_radial_error < 1e-10);

        let ellipse = ellipse_geometry(&[
            Point { x: 0.0, y: 0.0 }, Point { x: 2.0, y: 0.0 }, Point { x: 0.0, y: 1.0 },
        ]).unwrap();
        assert!((ellipse.semi_major - 2.0).abs() < 1e-10);
        assert!((ellipse.semi_minor - 1.0).abs() < 1e-10);
        assert!((ellipse.area - 2.0 * std::f64::consts::PI).abs() < 1e-10);

        let arc = arc_geometry(&[
            Point { x: 1.0, y: 0.0 }, Point { x: 0.0, y: 1.0 }, Point { x: -1.0, y: 0.0 },
        ]).unwrap();
        assert!((arc.radius * arc.sweep_radians.abs() - std::f64::consts::PI).abs() < 1e-10);
    }

    #[test]
    fn perpendicular_parallel_and_image_bounds_checks_are_consistent() {
        let point = Point { x: 3.0, y: 2.0 };
        assert_eq!(point_line_distance(point, Point { x: 0.0, y: 0.0 }, Point { x: 5.0, y: 0.0 }), Some(2.0));
        let (separation, angle) = parallel_line_distance(
            Point { x: 0.0, y: 0.0 }, Point { x: 5.0, y: 0.0 },
            Point { x: 0.0, y: 2.0 }, Point { x: 5.0, y: 2.0 },
        ).unwrap();
        assert!((separation - 2.0).abs() < 1e-10);
        assert!(angle < 1e-10);
        assert!(point_line_distance(point, Point { x: 1.0, y: 1.0 }, Point { x: 1.0, y: 1.0 }).is_none());
        assert!(parallel_line_distance(
            Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: 0.0 },
            Point { x: 0.0, y: 2.0 }, Point { x: 5.0, y: 2.0 },
        ).is_none());

        assert!(!exceeds_image_bounds(Tool::Circle,
            &[Point { x: 2.0, y: 2.0 }, Point { x: 4.0, y: 2.0 }], 8, 8));
        assert!(exceeds_image_bounds(Tool::Circle,
            &[Point { x: 1.0, y: 1.0 }, Point { x: 3.0, y: 1.0 }], 8, 8));
    }
}

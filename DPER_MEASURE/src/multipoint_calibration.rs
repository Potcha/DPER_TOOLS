use crate::geometry::{angle_deg, Point};
use serde::{Deserialize, Serialize};

pub const PROFILE_TYPE: &str = "multipoint_xy_center_corner";
pub const REQUIRED_POINTS: usize = 27;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Position { Center, TopLeft, TopRight, BottomLeft, BottomRight }

impl Position {
    fn label(self) -> &'static str {
        match self {
            Self::Center => "Centre",
            Self::TopLeft => "Coin haut gauche",
            Self::TopRight => "Coin haut droit",
            Self::BottomLeft => "Coin bas gauche",
            Self::BottomRight => "Coin bas droit",
        }
    }

    fn contains(self, point: Point, [width, height]: [u32; 2]) -> bool {
        let (x, y) = (point.x / f64::from(width), point.y / f64::from(height));
        match self {
            Self::Center => (0.30..=0.70).contains(&x) && (0.30..=0.70).contains(&y),
            Self::TopLeft => x <= 0.35 && y <= 0.35,
            Self::TopRight => x >= 0.65 && y <= 0.35,
            Self::BottomLeft => x <= 0.35 && y >= 0.65,
            Self::BottomRight => x >= 0.65 && y >= 0.65,
        }
    }
}

const POSITIONS: [Position; 5] = [
    Position::Center, Position::TopLeft, Position::TopRight, Position::BottomLeft, Position::BottomRight,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis { X, Y, Angle }

impl Axis {
    fn label(self) -> &'static str {
        match self { Self::X => "X", Self::Y => "Y", Self::Angle => "Angle" }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub position: Position,
    pub axis: Axis,
    pub points_native_px: Vec<Point>,
    pub reference: f64,
    pub measured: f64,
    pub deviation: f64,
}

impl Check {
    pub fn label(&self) -> String {
        let unit = if self.axis == Axis::Angle { "°" } else { "mm" };
        format!("{} · {} · référence {:.4} {unit} · mesuré {:.4} {unit} · écart {:+.4} {unit}",
            self.position.label(), self.axis.label(), self.reference, self.measured, self.deviation)
    }
}

fn length_check(index: usize) -> (Position, Axis) {
    (POSITIONS[index / 2], if index % 2 == 0 { Axis::X } else { Axis::Y })
}

pub fn can_save(point_count: usize) -> bool {
    (4..=24).contains(&point_count) && point_count % 2 == 0 || point_count == REQUIRED_POINTS
}

pub fn next_step(point_count: usize) -> String {
    match point_count {
        0..=3 => format!("Centre · étalon {} · point {}", if point_count < 2 { "X" } else { "Y" }, point_count % 2 + 1),
        4..=23 => {
            let (position, axis) = length_check((point_count - 4) / 2);
            format!("{} · contrôle indépendant {} · point {}", position.label(), axis.label(), point_count % 2 + 1)
        }
        24..=26 => format!("Centre · angle connu · {}", ["premier bras", "sommet", "second bras"][point_count - 24]),
        _ => "Tous les points sont saisis.".into(),
    }
}

pub fn collect_checks(
    points: &[Point], dimensions: [u32; 2], scale: [f64; 2], length_mm: f64, angle_degrees: f64,
) -> Result<Vec<Check>, String> {
    let mut checks = Vec::new();
    for (index, segment) in points[4..points.len().min(24)].chunks_exact(2).enumerate() {
        let (position, axis) = length_check(index);
        let [a, b] = [segment[0], segment[1]];
        let center = Point { x: (a.x + b.x) * 0.5, y: (a.y + b.y) * 0.5 };
        let (dx, dy) = ((a.x - b.x).abs(), (a.y - b.y).abs());
        let (along, drift) = if axis == Axis::X { (dx, dy) } else { (dy, dx) };
        if along < 2.0 || drift > along * 0.10 || !position.contains(center, dimensions) {
            return Err(format!("Contrôle {} · {} : placez un segment d’au moins 2 px dans la zone et sur l’axe indiqués.",
                position.label(), axis.label()));
        }
        if [&points[..2], &points[2..4]].iter().any(|reference|
            *reference == segment || (*reference)[0] == b && (*reference)[1] == a)
        {
            return Err("Choisissez un segment de contrôle distinct du segment d’étalonnage.".into());
        }
        let measured = (dx * scale[0]).hypot(dy * scale[1]);
        checks.push(Check { position, axis, points_native_px: segment.to_vec(),
            reference: length_mm, measured, deviation: measured - length_mm });
    }
    if let [a, vertex, b] = &points[points.len().min(24)..] {
        if !Position::Center.contains(*vertex, dimensions)
            || vertex.dist(*a) < 2.0 || vertex.dist(*b) < 2.0
        {
            return Err("Placez le sommet de l’angle au centre, avec deux bras d’au moins 2 px.".into());
        }
        let metric = |p: Point| Point { x: (p.x - vertex.x) * scale[0], y: (p.y - vertex.y) * scale[1] };
        let measured = angle_deg(metric(*a), Point::default(), metric(*b))
            .ok_or("L’angle de contrôle est indéterminé.")?;
        checks.push(Check { position: Position::Center, axis: Axis::Angle, points_native_px: vec![*a, *vertex, *b],
            reference: angle_degrees, measured, deviation: measured - angle_degrees });
    }
    if checks.iter().any(|check| !check.measured.is_finite() || !check.deviation.is_finite()) {
        return Err("Le contrôle multipoint produit un écart non fini.".into());
    }
    Ok(checks)
}

pub fn status(checks: &[Check], dimensions: [u32; 2], length_limit: Option<f64>, angle_limit: Option<f64>) -> &'static str {
    let valid_limit = |limit: Option<f64>| limit.filter(|value| value.is_finite() && *value > 0.0);
    let length_limit = valid_limit(length_limit);
    let angle_limit = valid_limit(angle_limit);
    if checks.iter().any(|check| {
        let limit = if check.axis == Axis::Angle { angle_limit } else { length_limit };
        limit.is_some_and(|value| (check.measured - check.reference).abs() > value)
    }) { return "failed"; }
    let covers = |position, axis| checks.iter().filter(|check| {
        check.position == position && check.axis == axis
            && check.reference.is_finite() && check.reference > 0.0
            && check.measured.is_finite() && check.measured >= 0.0
            && check.points_native_px.iter().all(|point|
                (0.0..=f64::from(dimensions[0])).contains(&point.x)
                && (0.0..=f64::from(dimensions[1])).contains(&point.y))
            && (check.deviation - (check.measured - check.reference)).abs()
                <= 1e-10 * check.reference.max(check.measured).max(1.0)
            && check.points_native_px.len() == if axis == Axis::Angle { 3 } else { 2 }
    }).count() == 1;
    let complete = checks.len() == 11 && POSITIONS.iter().all(|&position|
        covers(position, Axis::X) && covers(position, Axis::Y)) && covers(Position::Center, Axis::Angle);
    if !complete { "incomplete" }
    else if length_limit.is_some() && angle_limit.is_some() { "verified" }
    else { "unverified" }
}

//! Construction methods converted to the existing measurement representation.
//! Every point and radius is expressed in native image pixels.
use crate::geometry::{arc_geometry, ellipse_geometry, fit_circle, is_nondegenerate,
    polygon_area, polyline_length, Point, Tool};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

const EPS: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawingVariant {
    Line,
    MidpointLine,
    Rectangle2,
    Rectangle3,
    CenterRectangle,
    RoundedRectangle,
    CircleCenter,
    CircleDiameter,
    Circle3,
    Circle2Tangent,
    Circle3Tangent,
    Arc3,
    ArcCenter,
    ArcTangent,
    Standard(Tool),
}

impl DrawingVariant {
    pub fn for_tool(tool: Tool) -> Self {
        match tool {
            Tool::Distance => Self::Line, Tool::Rectangle => Self::Rectangle2,
            Tool::Circle => Self::CircleCenter, Tool::Arc => Self::Arc3,
            other => Self::Standard(other),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "Ligne — 2 extrémités",
            Self::MidpointLine => "Ligne — milieu et extrémité",
            Self::Rectangle2 => "Rectangle — 2 coins",
            Self::Rectangle3 => "Rectangle orienté — 3 points",
            Self::CenterRectangle => "Rectangle — centre et coin",
            Self::RoundedRectangle => "Rectangle arrondi",
            Self::CircleCenter => "Cercle — centre et bord",
            Self::CircleDiameter => "Cercle — diamètre",
            Self::Circle3 => "Cercle — 3 points",
            Self::Circle2Tangent => "Cercle — tangent à 2 droites",
            Self::Circle3Tangent => "Cercle — inscrit entre 3 droites",
            Self::Arc3 => "Arc — 3 points",
            Self::ArcCenter => "Arc — centre, début et direction de fin",
            Self::ArcTangent => "Arc — début, tangente et fin",
            Self::Standard(tool) => tool.label(),
        }
    }

    pub fn instruction(self) -> &'static str {
        match self {
            Self::Line => "Cliquez les deux extrémités.",
            Self::MidpointLine => "Cliquez le milieu, puis une extrémité ; l’autre est symétrique.",
            Self::Rectangle2 => "Cliquez deux coins opposés du rectangle aligné sur l’image.",
            Self::Rectangle3 => "Cliquez deux coins voisins, puis un point sur le côté opposé. La hauteur est projetée perpendiculairement au premier côté.",
            Self::CenterRectangle => "Cliquez le centre, puis un coin du rectangle aligné sur l’image.",
            Self::RoundedRectangle => "Réglez le rayon en pixels, puis cliquez deux coins opposés. Le rayon doit être compris entre 0 et la moitié du petit côté.",
            Self::CircleCenter => "Cliquez le centre, puis un point du bord.",
            Self::CircleDiameter => "Cliquez les deux extrémités du diamètre.",
            Self::Circle3 => "Cliquez trois points distincts non alignés du bord.",
            Self::Circle2Tangent => "Réglez le rayon en pixels, puis cliquez deux points par droite sécante. Parmi les quatre solutions, le centre le plus proche du milieu des quatre clics est retenu ; les droites sont prolongées.",
            Self::Circle3Tangent => "Cliquez deux points par droite. Les trois droites prolongées doivent former un triangle ; son cercle inscrit est retenu.",
            Self::Arc3 => "Cliquez le début, un point sur l’arc, puis la fin.",
            Self::ArcCenter => "Cliquez le centre, le début, puis la direction de fin. La fin est projetée au rayon initial ; l’arc le plus court est retenu (demi-tour positif dans les coordonnées de l’image).",
            Self::ArcTangent => "Cliquez le début, un point donnant la direction de départ, puis la fin. L’arc suit la tangente orientée au départ.",
            Self::Standard(tool) => tool.instruction(),
        }
    }

    pub fn required_points(self) -> Option<usize> {
        match self {
            Self::Line | Self::MidpointLine | Self::Rectangle2 | Self::CenterRectangle
            | Self::RoundedRectangle | Self::CircleCenter | Self::CircleDiameter => Some(2),
            Self::Rectangle3 | Self::Circle3 | Self::Arc3 | Self::ArcCenter
            | Self::ArcTangent => Some(3),
            Self::Circle2Tangent => Some(4),
            Self::Circle3Tangent => Some(6),
            Self::Standard(tool) => tool.required_points(),
        }
    }

    /// The canonical tool produced by this construction, including polygons for
    /// oriented and rounded rectangles.
    pub fn base_tool(self) -> Tool {
        match self {
            Self::Line | Self::MidpointLine => Tool::Distance,
            Self::Rectangle2 | Self::CenterRectangle => Tool::Rectangle,
            Self::Rectangle3 | Self::RoundedRectangle => Tool::Polygon,
            Self::CircleCenter | Self::CircleDiameter | Self::Circle3
            | Self::Circle2Tangent | Self::Circle3Tangent => Tool::Circle,
            Self::Arc3 | Self::ArcCenter | Self::ArcTangent => Tool::Arc,
            Self::Standard(tool) => tool,
        }
    }

    /// `parameter` supplies a radius only for RoundedRectangle and Circle2Tangent.
    /// The resulting polygon is implicitly closed by the canonical Polygon tool.
    pub fn resolve(self, points: &[Point], parameter: f64) -> Result<(Tool, Vec<Point>), String> {
        finite_points(points)?;
        if let Some(count) = self.required_points() {
            if points.len() != count {
                return Err(format!("Ce mode nécessite exactement {count} points."));
            }
        }
        let resolved = match self {
            Self::Line | Self::Rectangle2 | Self::CircleCenter | Self::Arc3
            | Self::Standard(_) => points.to_vec(),
            Self::MidpointLine | Self::CenterRectangle => {
                vec![add(points[0], sub(points[0], points[1])), points[1]]
            }
            Self::Rectangle3 => {
                let first = points[0];
                let second = points[1];
                let direction = unit(sub(second, first))?;
                let normal = perpendicular(direction);
                let height = dot(sub(points[2], second), normal);
                if !height.is_finite() || height.abs() <= EPS {
                    return Err("Le troisième point doit définir une hauteur non nulle.".into());
                }
                let offset = scale(normal, height);
                vec![first, second, add(second, offset), add(first, offset)]
            }
            Self::RoundedRectangle => rounded_rectangle(points[0], points[1], parameter)?,
            Self::CircleDiameter => {
                let center = midpoint(points[0], points[1]);
                vec![center, points[0]]
            }
            Self::Circle3 => {
                let circle = fit_circle(points)
                    .ok_or("Les trois points ne définissent pas un cercle.".to_string())?;
                circle_points(circle.center, circle.radius)
            }
            Self::Circle2Tangent => circle_two_tangents(points, parameter)?,
            Self::Circle3Tangent => circle_three_tangents(points)?,
            Self::ArcCenter => {
                let center = points[0];
                let start = points[1];
                let radius = start.dist(center);
                if !radius.is_finite() || radius <= EPS {
                    return Err("Le début doit être distinct du centre.".into());
                }
                let end_direction = unit(sub(points[2], center))?;
                let end = add(center, scale(end_direction, radius));
                let start_angle = angle(sub(start, center));
                let mut sweep = (angle(end_direction) - start_angle).rem_euclid(TAU);
                if sweep > PI { sweep -= TAU; }
                arc_points(center, radius, start, end, start_angle, sweep)?
            }
            Self::ArcTangent => {
                let start = points[0];
                let tangent = unit(sub(points[1], start))?;
                let chord = sub(points[2], start);
                let chord_length = length(chord);
                if !chord_length.is_finite() || chord_length <= EPS {
                    return Err("Le début et la fin doivent être distincts.".into());
                }
                let normal = perpendicular(tangent);
                let normal_fraction = dot(scale(chord, 1.0 / chord_length), normal);
                if normal_fraction.abs() <= EPS {
                    return Err("La fin est sur la tangente : aucun arc circulaire fini.".into());
                }
                let signed_radius = chord_length / (2.0 * normal_fraction);
                let center = add(start, scale(normal, signed_radius));
                let start_angle = angle(sub(start, center));
                let end_angle = angle(sub(points[2], center));
                let sweep = if signed_radius > 0.0 {
                    (end_angle - start_angle).rem_euclid(TAU)
                } else {
                    -(start_angle - end_angle).rem_euclid(TAU)
                };
                arc_points(center, signed_radius.abs(), start, points[2], start_angle, sweep)?
            }
        };
        let tool = self.base_tool();
        finite_points(&resolved)?;
        if !is_nondegenerate(tool, &resolved) {
            return Err("Les points décrivent une construction dégénérée ou trop petite.".into());
        }
        // Finite coordinates alone do not prevent a length or area overflowing.
        let finite_dimensions = match tool {
            Tool::Distance => resolved[0].dist(resolved[1]).is_finite(),
            Tool::Circle => {
                let radius = resolved[0].dist(resolved[1]);
                radius.is_finite() && (PI * radius * radius).is_finite()
            }
            Tool::Polyline => polyline_length(&resolved).is_finite(),
            Tool::Polygon => polygon_area(&resolved).is_finite(),
            Tool::Rectangle => ((resolved[1].x - resolved[0].x)
                * (resolved[1].y - resolved[0].y)).is_finite(),
            Tool::CircleFit => fit_circle(&resolved).map_or(false, |circle|
                circle.radius.is_finite() && circle.center.x.is_finite()
                    && circle.center.y.is_finite()
                    && (PI * circle.radius * circle.radius).is_finite()),
            Tool::Ellipse => ellipse_geometry(&resolved).map_or(false, |ellipse|
                ellipse.semi_major.is_finite() && ellipse.semi_minor.is_finite()
                    && ellipse.area.is_finite()),
            _ => resolved.windows(2).all(|pair| pair[0].dist(pair[1]).is_finite()),
        };
        if !finite_dimensions {
            return Err("Les dimensions calculées sont trop grandes.".into());
        }
        if tool == Tool::Arc {
            let arc = arc_geometry(&resolved)
                .ok_or("L’arc ne peut pas être calculé avec ces points.".to_string())?;
            if !arc.radius.is_finite() || !arc.sweep_radians.is_finite()
                || !arc.center.x.is_finite() || !arc.center.y.is_finite() {
                return Err("Les dimensions calculées de l’arc sont trop grandes.".into());
            }
        }
        Ok((tool, resolved))
    }
}

fn finite_points(points: &[Point]) -> Result<(), String> {
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        Err("Toutes les coordonnées doivent être finies.".into())
    } else { Ok(()) }
}

fn add(a: Point, b: Point) -> Point { Point { x: a.x + b.x, y: a.y + b.y } }
fn sub(a: Point, b: Point) -> Point { Point { x: a.x - b.x, y: a.y - b.y } }
fn scale(p: Point, factor: f64) -> Point { Point { x: p.x * factor, y: p.y * factor } }
fn midpoint(a: Point, b: Point) -> Point { add(scale(a, 0.5), scale(b, 0.5)) }
fn dot(a: Point, b: Point) -> f64 { a.x * b.x + a.y * b.y }
fn cross(a: Point, b: Point) -> f64 { a.x * b.y - a.y * b.x }
fn perpendicular(p: Point) -> Point { Point { x: -p.y, y: p.x } }
fn length(p: Point) -> f64 { p.x.hypot(p.y) }
fn angle(p: Point) -> f64 { p.y.atan2(p.x) }
fn unit(p: Point) -> Result<Point, String> {
    let len = length(p);
    if !len.is_finite() || len <= EPS {
        Err("Deux points distincts sont nécessaires pour définir une direction.".into())
    } else { Ok(scale(p, 1.0 / len)) }
}
fn circle_points(center: Point, radius: f64) -> Vec<Point> {
    vec![center, Point { x: center.x + radius, y: center.y }]
}

fn rounded_rectangle(first: Point, second: Point, radius: f64) -> Result<Vec<Point>, String> {
    let left = first.x.min(second.x);
    let right = first.x.max(second.x);
    let top = first.y.min(second.y);
    let bottom = first.y.max(second.y);
    let width = right - left;
    let height = bottom - top;
    if !width.is_finite() || !height.is_finite() || width <= EPS || height <= EPS {
        return Err("Le rectangle doit avoir une largeur et une hauteur non nulles.".into());
    }
    if !radius.is_finite() || radius < 0.0 || radius > width.min(height) * 0.5 {
        return Err("Le rayon doit être positif ou nul et ne pas dépasser la moitié du petit côté.".into());
    }
    if radius <= EPS {
        return Ok(vec![Point { x: left, y: top }, Point { x: right, y: top },
            Point { x: right, y: bottom }, Point { x: left, y: bottom }]);
    }
    // Fixed, bounded tessellation: 24 segments per quarter-circle.
    let corners = [
        (Point { x: right - radius, y: top + radius }, -FRAC_PI_2),
        (Point { x: right - radius, y: bottom - radius }, 0.0),
        (Point { x: left + radius, y: bottom - radius }, FRAC_PI_2),
        (Point { x: left + radius, y: top + radius }, PI),
    ];
    let mut result = Vec::with_capacity(100);
    for (center, start) in corners {
        for step in 0..=24 {
            let theta = start + FRAC_PI_2 * step as f64 / 24.0;
            result.push(add(center, Point { x: radius * theta.cos(), y: radius * theta.sin() }));
        }
    }
    Ok(result)
}

/// Intersection of infinite lines represented by a point and a unit direction.
fn intersection(a: Point, u: Point, b: Point, v: Point) -> Result<Point, String> {
    let determinant = cross(u, v);
    if determinant.abs() <= EPS {
        return Err("Les droites doivent être sécantes et suffisamment distinctes.".into());
    }
    let point = add(a, scale(u, cross(sub(b, a), v) / determinant));
    finite_points(&[point])?;
    Ok(point)
}

fn circle_two_tangents(points: &[Point], radius: f64) -> Result<Vec<Point>, String> {
    if !radius.is_finite() || radius <= EPS {
        return Err("Le rayon du cercle doit être strictement positif et fini.".into());
    }
    let first = unit(sub(points[1], points[0]))?;
    let second = unit(sub(points[3], points[2]))?;
    if cross(first, second).abs() <= EPS {
        return Err("Deux droites parallèles ou presque parallèles ne définissent pas un centre unique avec ce mode ; utilisez deux droites sécantes.".into());
    }
    let first_normal = perpendicular(first);
    let second_normal = perpendicular(second);
    let target = midpoint(midpoint(points[0], points[1]), midpoint(points[2], points[3]));
    let mut best: Option<(Point, f64)> = None;
    // Stable signs make equal-score cases reproducible. Reversing a line's
    // clicked direction can change only a precisely tied selection.
    for first_sign in [1.0, -1.0] {
        for second_sign in [1.0, -1.0] {
            let candidate = intersection(
                add(points[0], scale(first_normal, first_sign * radius)), first,
                add(points[2], scale(second_normal, second_sign * radius)), second)?;
            let distance = candidate.dist(target);
            if distance.is_finite() && best.map_or(true, |(_, score)| distance < score) {
                best = Some((candidate, distance));
            }
        }
    }
    let center = best.ok_or("Aucun centre de cercle fini n’a pu être calculé.".to_string())?.0;
    Ok(circle_points(center, radius))
}

fn circle_three_tangents(points: &[Point]) -> Result<Vec<Point>, String> {
    let u = unit(sub(points[1], points[0]))?;
    let v = unit(sub(points[3], points[2]))?;
    let w = unit(sub(points[5], points[4]))?;
    let a = intersection(points[0], u, points[2], v)?;
    let b = intersection(points[2], v, points[4], w)?;
    let c = intersection(points[4], w, points[0], u)?;
    let lengths = [b.dist(c), c.dist(a), a.dist(b)];
    let maximum = lengths.into_iter().fold(0.0_f64, f64::max);
    if !maximum.is_finite() || lengths.iter().any(|len| *len <= EPS) {
        return Err("Les trois droites doivent former un triangle non dégénéré.".into());
    }
    // Scaled barycentric weights avoid overflow in the triangle perimeter.
    let weights = lengths.map(|len| len / maximum);
    let total = weights.iter().sum::<f64>();
    let center = add(add(scale(a, weights[0] / total), scale(b, weights[1] / total)),
        scale(c, weights[2] / total));
    let radius = cross(sub(center, points[0]), u).abs();
    if !radius.is_finite() || radius <= EPS {
        return Err("Les trois droites ne définissent pas un cercle inscrit mesurable.".into());
    }
    Ok(circle_points(center, radius))
}

fn arc_points(center: Point, radius: f64, start: Point, end: Point,
    start_angle: f64, sweep: f64) -> Result<Vec<Point>, String> {
    if !radius.is_finite() || radius <= EPS || !sweep.is_finite() || sweep.abs() <= EPS {
        return Err("L’arc doit avoir un rayon et un balayage non nuls et finis.".into());
    }
    let theta = start_angle + sweep * 0.5;
    let middle = add(center, Point { x: radius * theta.cos(), y: radius * theta.sin() });
    Ok(vec![start, middle, end])
}


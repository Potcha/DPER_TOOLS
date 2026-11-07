//! Fixed-center clearance from independently observed, finite line supports.
//!
//! All coordinates are native source-image pixels. This is neither a circle fit
//! nor an optimizer for the center. The supplied strips localize image searches;
//! no expected dimension or nominal circle is used as a measurement.

use crate::dper_vision::{
    inspect_geometry, EdgePolarity, InspectionGeometry, InspectionResult, VisionEvidence,
    VisionOptions,
};
use crate::geometry::Point;
use image::RgbImage;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClearanceStrip {
    pub start: Point,
    pub end: Point,
    /// Required signed transition along the left normal of start -> end.
    /// `Any` is refused: the operator must identify the facing edge contrast.
    pub polarity: EdgePolarity,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClearanceReference {
    /// Four boundary searches in cyclic order (clockwise or counterclockwise).
    /// Their independently fitted lines construct the reference rectangle.
    ObservedRectangle { edges: [ClearanceStrip; 4] },
    /// An explicitly constructed center, not an image-observed reference.
    SuppliedCenter { center: Point },
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClearanceStop {
    pub id: String,
    pub strip: ClearanceStrip,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClearanceRequest {
    pub reference: ClearanceReference,
    /// 2..=16 independent facing supports. An enclosing arrangement requires
    /// at least three nonparallel directions; two stops therefore fail closed.
    pub stops: Vec<ClearanceStop>,
    pub options: VisionOptions,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClearanceReferenceEvidence {
    ObservedRectangle {
        corners: [Point; 4],
        edges: Vec<InspectionResult>,
        center_construction: String,
    },
    SuppliedCenter { center: Point },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClearanceStopResult {
    pub id: String,
    pub closest_point: Point,
    pub distance_px: f64,
    /// Additional radial clearance beyond the limiting centered circle.
    pub gap_px: f64,
    pub observed_geometry: InspectionGeometry,
    pub evidence: VisionEvidence,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClearanceResult {
    pub center: Point,
    pub radius_px: f64,
    pub diameter_px: f64,
    pub limiting_stop_id: String,
    pub stops: Vec<ClearanceStopResult>,
    pub reference: ClearanceReferenceEvidence,
    pub limitations: Vec<String>,
}

/// Measure the minimum Euclidean distance from an immutable reference center
/// to the observed facing support of every declared stop. A contact must be
/// safely inside its observed span and locally supported on both sides.
/// Missing, weak, competing or wrong-polarity image edges fail closed.
pub fn inspect_clearance(
    image: &RgbImage,
    request: &ClearanceRequest,
) -> Result<ClearanceResult, String> {
    if !(2..=16).contains(&request.stops.len()) {
        return Err("Le dégagement exige entre 2 et 16 butées déclarées.".into());
    }
    for (index, stop) in request.stops.iter().enumerate() {
        if stop.id.trim().is_empty()
            || stop.id.len() > 128
            || request.stops[..index].iter().any(|other| other.id == stop.id)
        {
            return Err("Chaque butée exige un identifiant unique non vide (128 octets maximum).".into());
        }
    }
    let (center, reference) = match &request.reference {
        ClearanceReference::SuppliedCenter { center } => {
            if !inside(image, *center) {
                return Err("Le centre construit doit être fini et situé dans l'image source.".into());
            }
            (*center, ClearanceReferenceEvidence::SuppliedCenter { center: *center })
        }
        ClearanceReference::ObservedRectangle { edges } => {
            let mut observed = Vec::with_capacity(4);
            for (index, edge) in edges.iter().enumerate() {
                observed.push(inspect_strip(image, edge, &request.options)
                    .map_err(|error| format!("Référence, côté {} : {error}", index + 1))?);
            }
            let (center, corners) = rectangle_center(image, &observed)?;
            (center, ClearanceReferenceEvidence::ObservedRectangle {
                corners,
                edges: observed,
                center_construction: "Intersection des diagonales des quatre coins construits à partir des droites observées ; cohérence avec leur moyenne vérifiée.".into(),
            })
        }
    };
    let mut stops = Vec::with_capacity(request.stops.len());
    let mut angles = Vec::with_capacity(request.stops.len());
    for stop in &request.stops {
        let observed = inspect_strip(image, &stop.strip, &request.options)
            .map_err(|error| format!("Butée {} : {error}", stop.id))?;
        let (start, end) = line_points(&observed.geometry)?;
        let direction = unit(sub(end, start))?;
        let along = dot(sub(center, start), direction);
        let length = start.dist(end);
        let spacing = stop.strip.start.dist(stop.strip.end)
            / (request.options.sample_count - 1) as f64;
        let margin = (2.0 * spacing).max(2.0).max(3.0 * observed.evidence.rms_residual_px);
        if along <= margin || along >= length - margin {
            return Err(format!("Butée {} : le contact sort de la portée observée sûre ; les extrémités physiques ne sont pas vérifiées.", stop.id));
        }
        let closest_point = add(start, scale(direction, along));
        // A globally sufficient fit must not bridge a missing region at the
        // actual contact. Require nearby observed support on either side.
        let local_reach = (2.0 * spacing).max(2.0);
        let mut before = false;
        let mut after = false;
        for point in &observed.evidence.accepted_points {
            let offset = dot(sub(*point, closest_point), direction);
            before |= (-local_reach..=-0.01).contains(&offset);
            after |= (0.01..=local_reach).contains(&offset);
        }
        if !before || !after {
            return Err(format!("Butée {} : absence de support image local de part et d'autre du contact.", stop.id));
        }
        let distance_px = center.dist(closest_point);
        if !distance_px.is_finite() || distance_px <= 1.0 {
            return Err(format!("Butée {} : centre sur la frontière ou dégagement non résolu.", stop.id));
        }
        angles.push((closest_point.y - center.y).atan2(closest_point.x - center.x).rem_euclid(TAU));
        stops.push(ClearanceStopResult {
            id: stop.id.clone(), closest_point, distance_px, gap_px: 0.0,
            observed_geometry: observed.geometry, evidence: observed.evidence,
        });
    }
    angles.sort_by(f64::total_cmp);
    let largest_gap = (0..angles.len()).map(|index| {
        let next = if index + 1 == angles.len() { angles[0] + TAU } else { angles[index + 1] };
        next - angles[index]
    }).fold(0.0, f64::max);
    // A semicircle of directions leaves the setup open. The tiny numerical
    // margin also refuses a nearly degenerate opposing-line arrangement.
    if angles.len() < 3 || largest_gap >= PI - 1e-3 {
        return Err("Butées insuffisamment enveloppantes : au moins trois directions non parallèles et aucun secteur ouvert de 180° sont requis.".into());
    }
    let limiting_index = stops.iter().enumerate()
        .min_by(|(_, left), (_, right)| left.distance_px.total_cmp(&right.distance_px))
        .map(|(index, _)| index).ok_or("Aucune butée observée.")?;
    let radius_px = stops[limiting_index].distance_px;
    let limiting_stop_id = stops[limiting_index].id.clone();
    for stop in &mut stops {
        stop.gap_px = (stop.distance_px - radius_px).max(0.0);
    }
    Ok(ClearanceResult {
        center, radius_px, diameter_px: 2.0 * radius_px, limiting_stop_id, stops, reference,
        limitations: vec![
            "Dégagement limité aux butées et bandes de recherche explicitement déclarées ; l'absence d'autres obstacles n'est pas vérifiée.".into(),
            "Les contacts utilisent des supports droits locaux observés, sans certification des extrémités physiques ni des zones hors bandes.".into(),
            "La polarité et l'unicité du contraste sont contrôlées ; une image seule ne certifie pas la nature physique d'un bord isolé (trou, reflet ou matière).".into(),
            "Résultats en pixels source ; les résidus d'ajustement ne constituent pas une incertitude métrologique.".into(),
        ],
    })
}

fn inspect_strip(image: &RgbImage, strip: &ClearanceStrip, options: &VisionOptions) -> Result<InspectionResult, String> {
    if strip.polarity == EdgePolarity::Any {
        return Err("La polarité explicite du bord faisant face au centre est requise.".into());
    }
    let mut directed = options.clone();
    directed.polarity = strip.polarity;
    let expected = InspectionGeometry::Line { start: strip.start, end: strip.end };
    let observed = inspect_geometry(image, &expected, &directed)?;
    // Clearance is stricter than generic localization: a nearby second edge
    // (even an opposite-polarity reflection/hole boundary) is not silently
    // selected away. Check every accepted profile across the full strip.
    let tangent = unit(sub(strip.end, strip.start))?;
    let normal = Point { x: -tangent.y, y: tangent.x };
    for accepted in &observed.evidence.accepted_points {
        let anchor = add(strip.start, scale(tangent, dot(sub(*accepted, strip.start), tangent)));
        verify_single_transition(image, anchor, tangent, normal, &directed)?;
    }
    Ok(observed)
}

fn verify_single_transition(image: &RgbImage, anchor: Point, tangent: Point, normal: Point, options: &VisionOptions) -> Result<(), String> {
    let half_steps = (options.search_radius_px * 2.0).floor() as i32;
    let mut active_sign = 0_i32;
    let mut previous_active = -1000;
    let mut transitions = 0;
    let expected_sign = if options.polarity == EdgePolarity::Rising { 1 } else { -1 };
    for index in -half_steps..=half_steps {
        let offset = index as f64 * 0.5;
        let mut derivative = 0.0;
        for along in [-0.6, 0.0, 0.6] {
            let base = add(add(anchor, scale(normal, offset)), scale(tangent, along));
            let low = gray(image, add(base, scale(normal, -0.75)))
                .ok_or("La bande de recherche sort de l'image source.")?;
            let high = gray(image, add(base, scale(normal, 0.75)))
                .ok_or("La bande de recherche sort de l'image source.")?;
            derivative += (high - low) / 4.5;
        }
        if derivative.abs() < options.minimum_gradient { continue; }
        let sign = if derivative > 0.0 { 1 } else { -1 };
        if index - previous_active > 2 || sign != active_sign {
            transitions += 1;
        }
        if transitions > 1 || sign != expected_sign {
            return Err("Contraste ambigu : plusieurs transitions ou une transition opposée dans la bande (reflet, trou ou bord voisin possible).".into());
        }
        previous_active = index;
        active_sign = sign;
    }
    if transitions != 1 {
        return Err("Contraste insuffisant dans la bande observée.".into());
    }
    Ok(())
}

fn rectangle_center(image: &RgbImage, edges: &[InspectionResult]) -> Result<(Point, [Point; 4]), String> {
    let mut lines = Vec::with_capacity(4);
    for edge in edges { lines.push(line_points(&edge.geometry)?); }
    let mut corners = [Point::default(); 4];
    for index in 0..4 {
        corners[index] = intersection(lines[(index + 3) % 4], lines[index])
            .ok_or("Référence instable : côtés adjacents parallèles.")?;
        if !inside(image, corners[index]) {
            return Err("Un coin construit de la référence sort de l'image source.".into());
        }
    }
    let mut directions = [Point::default(); 4];
    let mut lengths = [0.0; 4];
    let mut turn_sign = 0.0_f64;
    for index in 0..4 {
        let side = sub(corners[(index + 1) % 4], corners[index]);
        lengths[index] = side.x.hypot(side.y);
        if lengths[index] < 8.0 { return Err("Référence rectangulaire trop petite ou dégénérée.".into()); }
        directions[index] = unit(side)?;
    }
    let max_rms = edges.iter().map(|edge| edge.evidence.rms_residual_px).fold(0.0, f64::max);
    let shortest = lengths.iter().copied().fold(f64::INFINITY, f64::min);
    let angular_tolerance = (0.025 + 3.0 * max_rms / shortest).min(0.08);
    for index in 0..4 {
        let next = directions[(index + 1) % 4];
        let turn = cross(directions[index], next);
        if index == 0 { turn_sign = turn.signum(); }
        if turn * turn_sign <= 0.0 || dot(directions[index], next).abs() > angular_tolerance {
            return Err("Les quatre côtés observés ne forment pas un rectangle convexe orthogonal dans l'ordre déclaré.".into());
        }
        if cross(directions[index], directions[(index + 2) % 4]).abs() > angular_tolerance {
            return Err("Les côtés opposés de la référence ne sont pas parallèles.".into());
        }
    }
    let center = intersection((corners[0], corners[2]), (corners[1], corners[3]))
        .ok_or("Diagonales de référence instables.")?;
    let mean = scale(corners.iter().copied().fold(Point::default(), add), 0.25);
    let tolerance = (3.0 * max_rms).max(0.5);
    if center.dist(mean) > tolerance
        || scale(add(corners[0], corners[2]), 0.5).dist(center) > tolerance
        || scale(add(corners[1], corners[3]), 0.5).dist(center) > tolerance
        || !inside(image, center)
    {
        return Err("Centre de référence incohérent entre les quatre coins et les diagonales.".into());
    }
    Ok((center, corners))
}

fn line_points(geometry: &InspectionGeometry) -> Result<(Point, Point), String> {
    match geometry {
        InspectionGeometry::Line { start, end } => Ok((*start, *end)),
        _ => Err("Un support droit observé est requis.".into()),
    }
}

fn intersection(first: (Point, Point), second: (Point, Point)) -> Option<Point> {
    let a = sub(first.1, first.0);
    let b = sub(second.1, second.0);
    let denominator = cross(a, b);
    if !denominator.is_finite() || denominator.abs() < 1e-6 * a.x.hypot(a.y) * b.x.hypot(b.y) { return None; }
    let point = add(first.0, scale(a, cross(sub(second.0, first.0), b) / denominator));
    if point.x.is_finite() && point.y.is_finite() { Some(point) } else { None }
}
fn inside(image: &RgbImage, point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.x >= 0.0 && point.y >= 0.0
        && point.x <= image.width().saturating_sub(1) as f64
        && point.y <= image.height().saturating_sub(1) as f64
}
fn add(a: Point, b: Point) -> Point { Point { x: a.x + b.x, y: a.y + b.y } }
fn sub(a: Point, b: Point) -> Point { Point { x: a.x - b.x, y: a.y - b.y } }
fn scale(a: Point, factor: f64) -> Point { Point { x: a.x * factor, y: a.y * factor } }
fn dot(a: Point, b: Point) -> f64 { a.x * b.x + a.y * b.y }
fn cross(a: Point, b: Point) -> f64 { a.x * b.y - a.y * b.x }
fn unit(a: Point) -> Result<Point, String> {
    let length = a.x.hypot(a.y);
    if !length.is_finite() || length < 1e-6 { return Err("Support droit dégénéré.".into()); }
    Ok(scale(a, 1.0 / length))
}
fn gray(image: &RgbImage, point: Point) -> Option<f64> {
    if !inside(image, point) { return None; }
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
    Some((intensity(x, y) * (1.0 - fx) + intensity(next_x, y) * fx) * (1.0 - fy)
        + (intensity(x, next_y) * (1.0 - fx) + intensity(next_x, next_y) * fx) * fy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[derive(Clone)]
    struct TestStop { normal: Point, distance: f64, half_span: f64 }

    fn transition(distance: f64) -> f64 { 0.5 + 0.5 * (distance / 0.65).tanh() }

    fn frame(angle: f64) -> (Point, Point) {
        (Point { x: angle.cos(), y: angle.sin() }, Point { x: -angle.sin(), y: angle.cos() })
    }

    fn scene(center: Point, reference_angle: Option<f64>, stops: &[TestStop], defect: u8) -> RgbImage {
        let mut image = RgbImage::new(280, 280);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let offset = sub(Point { x: x as f64, y: y as f64 }, center);
            let mut occupancy = 0.0_f64;
            if let Some(angle) = reference_angle {
                let (u, v) = frame(angle);
                occupancy = transition(16.0 - dot(offset, u).abs())
                    * transition(11.0 - dot(offset, v).abs());
            }
            for (index, stop) in stops.iter().enumerate() {
                if defect == 1 && index == 0 { continue; }
                let tangent = Point { x: stop.normal.y, y: -stop.normal.x };
                let across = dot(offset, stop.normal) - stop.distance;
                let along = dot(offset, tangent);
                let mut material = transition(across) * transition(12.0 - across)
                    * transition(stop.half_span - along.abs());
                if index == 0 {
                    if defect == 2 { material *= 0.025; }
                    // Bright-dark-bright transitions imitate a reflection or
                    // a hole wall in a seemingly plausible facing strip.
                    if defect == 3 { material *= 1.0 - transition(across - 3.0) * transition(6.0 - across); }
                    if defect == 4 { material *= transition(along.abs() - 5.0); }
                }
                occupancy = occupancy.max(material);
            }
            let value = (20.0 + 210.0 * occupancy + (x as f64 * 0.8 + y as f64 * 0.4).sin() * 0.6)
                .round().clamp(0.0, 255.0) as u8;
            *pixel = Rgb([value; 3]);
        }
        image
    }

    fn stops(count: usize, angle: f64, distances: &[f64]) -> Vec<TestStop> {
        (0..count).map(|index| {
            let angle = angle + index as f64 * TAU / count as f64;
            TestStop { normal: frame(angle).0, distance: distances[index], half_span: 28.0 }
        }).collect()
    }

    fn request(center: Point, stops: &[TestStop]) -> ClearanceRequest {
        ClearanceRequest {
            reference: ClearanceReference::SuppliedCenter { center },
            stops: stops.iter().enumerate().map(|(index, stop)| {
                let tangent = Point { x: stop.normal.y, y: -stop.normal.x };
                let midpoint = add(center, scale(stop.normal, stop.distance + 1.3));
                ClearanceStop {
                    id: format!("stop-{index}"),
                    strip: ClearanceStrip {
                        start: add(midpoint, scale(tangent, -22.0)),
                        end: add(midpoint, scale(tangent, 22.0)),
                        polarity: EdgePolarity::Rising,
                    },
                }
            }).collect(),
            options: VisionOptions::default(),
        }
    }

    fn rectangle_reference(center: Point, angle: f64) -> ClearanceReference {
        let (u, v) = frame(angle);
        let normals = [u, v, scale(u, -1.0), scale(v, -1.0)];
        let edges = std::array::from_fn(|index| {
            let normal = normals[index];
            let tangent = Point { x: normal.y, y: -normal.x };
            let (distance, half_span) = if index % 2 == 0 { (16.0, 7.0) } else { (11.0, 12.0) };
            let midpoint = add(center, scale(normal, distance + 0.8));
            ClearanceStrip {
                start: add(midpoint, scale(tangent, -half_span)),
                end: add(midpoint, scale(tangent, half_span)),
                polarity: EdgePolarity::Falling,
            }
        });
        ClearanceReference::ObservedRectangle { edges }
    }

    #[test]
    fn three_and_four_disjoint_stops_measure_euclidean_clearance_at_varied_angles() {
        let center = Point { x: 138.3, y: 141.7 };
        for (count, angle, distances) in [
            (3, 0.29, vec![56.4, 70.2, 64.8]),
            (3, 1.14, vec![61.7, 58.3, 69.2]),
            (4, 0.47, vec![65.1, 72.2, 59.6, 68.4]),
            (4, 1.32, vec![71.1, 63.7, 67.9, 60.2]),
        ] {
            let setup = stops(count, angle, &distances);
            let result = inspect_clearance(&scene(center, None, &setup, 0), &request(center, &setup)).unwrap();
            let minimum = distances.iter().copied().fold(f64::INFINITY, f64::min);
            assert_eq!(result.center, center);
            assert!((result.radius_px - minimum).abs() < 0.2, "radius={}, expected={minimum}", result.radius_px);
            assert_eq!(result.diameter_px, 2.0 * result.radius_px);
            for (stop, expected) in result.stops.iter().zip(distances) {
                assert!((stop.distance_px - expected).abs() < 0.2);
                assert!((stop.gap_px - (expected - minimum)).abs() < 0.25);
                assert!(stop.evidence.coverage_fraction > 0.95);
                assert!(!stop.evidence.endpoint_extent_verified);
            }
        }
    }

    #[test]
    fn observed_rotated_rectangle_constructs_immutable_center_from_actual_edges() {
        for angle in [0.0, 0.37, 1.08] {
            let center = Point { x: 136.4, y: 139.2 };
            let setup = stops(4, angle + 0.19, &[58.2, 70.4, 65.7, 62.6]);
            let mut query = request(center, &setup);
            query.reference = rectangle_reference(center, angle);
            let result = inspect_clearance(&scene(center, Some(angle), &setup, 0), &query).unwrap();
            assert!(result.center.dist(center) < 0.2, "center={:?}", result.center);
            assert!((result.radius_px - 58.2).abs() < 0.25);
            let ClearanceReferenceEvidence::ObservedRectangle { corners, edges, .. } = result.reference else { panic!() };
            assert_eq!(edges.len(), 4);
            assert!(scale(corners.into_iter().fold(Point::default(), add), 0.25).dist(result.center) < 0.2);
        }
    }

    #[test]
    fn center_is_not_optimized_toward_asymmetric_stops() {
        let original = Point { x: 138.4, y: 137.2 };
        let setup = stops(4, 0.43, &[52.8, 70.0, 74.0, 63.1]);
        let image = scene(original, None, &setup, 0);
        let mut query = request(original, &setup);
        let shifted = add(original, Point { x: 3.1, y: -2.4 });
        query.reference = ClearanceReference::SuppliedCenter { center: shifted };
        let result = inspect_clearance(&image, &query).unwrap();
        let expected = setup.iter().map(|stop| stop.distance - dot(sub(shifted, original), stop.normal))
            .fold(f64::INFINITY, f64::min);
        assert_eq!(result.center, shifted);
        assert!((result.radius_px - expected).abs() < 0.2);
        assert!((result.radius_px - 52.8).abs() > 1.0);
    }

    #[test]
    fn missing_weak_competing_and_missing_contact_edges_are_refused() {
        let center = Point { x: 139.4, y: 137.1 };
        let setup = stops(4, 0.31, &[58.4, 63.1, 68.3, 71.7]);
        let query = request(center, &setup);
        for defect in [1, 2, 3, 4] {
            assert!(inspect_clearance(&scene(center, None, &setup, defect), &query).is_err(), "defect {defect} accepted");
        }
        let mut wrong_polarity = query.clone();
        wrong_polarity.stops[0].strip.polarity = EdgePolarity::Falling;
        assert!(inspect_clearance(&scene(center, None, &setup, 0), &wrong_polarity).is_err());
        wrong_polarity.stops[0].strip.polarity = EdgePolarity::Any;
        assert!(inspect_clearance(&scene(center, None, &setup, 0), &wrong_polarity).is_err());
    }

    #[test]
    fn open_arrangements_endpoint_projection_and_invalid_reference_are_refused() {
        let center = Point { x: 139.1, y: 137.4 };
        let setup = stops(4, 0.2, &[59.4, 66.1, 69.7, 72.2]);
        let image = scene(center, None, &setup, 0);
        let mut query = request(center, &setup);
        query.stops.truncate(2);
        assert!(inspect_clearance(&image, &query).is_err());
        query = request(center, &setup);
        let first = query.stops[0].strip.clone();
        let tangent = unit(sub(first.end, first.start)).unwrap();
        // Keep real observed support, but omit the neighborhood of the actual
        // closest point. The line extension must not become a measurement.
        query.stops[0].strip.start = add(first.start, scale(tangent, 23.0));
        assert!(inspect_clearance(&image, &query).is_err());
        query = request(center, &setup);
        query.reference = ClearanceReference::SuppliedCenter { center: Point { x: f64::NAN, y: 0.0 } };
        assert!(inspect_clearance(&image, &query).is_err());
        query.reference = rectangle_reference(center, 0.0);
        // No reference rectangle is present in this image.
        assert!(inspect_clearance(&image, &query).is_err());
    }

    #[test]
    fn cyclic_order_and_rectangle_orthogonality_are_checked() {
        let center = Point { x: 139.2, y: 138.8 };
        let setup = stops(3, 0.2, &[59.4, 66.1, 69.7]);
        let image = scene(center, Some(0.31), &setup, 0);
        let mut query = request(center, &setup);
        let ClearanceReference::ObservedRectangle { mut edges } = rectangle_reference(center, 0.31) else { panic!() };
        edges.swap(1, 2);
        query.reference = ClearanceReference::ObservedRectangle { edges };
        assert!(inspect_clearance(&image, &query).is_err());
        // Independently exercise the geometry contract with a cleanly fitted
        // parallelogram: image evidence alone cannot authorize a rectangle.
        let points = [Point { x: 50.0, y: 50.0 }, Point { x: 90.0, y: 50.0 }, Point { x: 98.0, y: 80.0 }, Point { x: 58.0, y: 80.0 }];
        let measured: Vec<_> = (0..4).map(|index| InspectionResult {
            geometry: InspectionGeometry::Line { start: points[index], end: points[(index + 1) % 4] },
            evidence: VisionEvidence {
                accepted_points: vec![], rejected_points: vec![], sample_count: 96,
                rms_residual_px: 0.05, max_residual_px: 0.1, coverage_fraction: 1.0,
                endpoint_extent_verified: false,
            },
        }).collect();
        assert!(rectangle_center(&image, &measured).is_err());
    }
}

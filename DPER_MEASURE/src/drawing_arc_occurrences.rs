use crate::dper_pdf_geometry::{SvgGeometryDocument, VectorPrimitiveKind};
use crate::dper_pdf_layout::PdfBox;
use crate::dper_vision::CircularSector;
use crate::geometry::{fit_circle, CircleFit, Point};
use serde::Serialize;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

#[derive(Debug, Serialize)]
pub(crate) struct ArcOccurrence<'a> {
    pub center_pt: Point,
    pub radius_pt: f64,
    pub source_ids: Vec<&'a str>,
    pub state: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sectors: Vec<CircularSector>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub housing: Option<SquareHousing<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SquareHousing<'a> {
    pub center_pt: Point,
    pub bounds_pt: PdfBox,
    pub source_ids: Vec<&'a str>,
}

struct ArcSupport<'a> {
    fit: CircleFit,
    sector: usize,
    source_id: &'a str,
    points: &'a [Point],
}

fn observed_sector(points: &[Point], center: Point) -> Option<CircularSector> {
    let angle = |point: Point| (point.y - center.y).atan2(point.x - center.x);
    let initial = angle(*points.first()?);
    let last = angle(*points.last()?);
    let signed_sweep = (last - initial + PI).rem_euclid(TAU) - PI;
    let sweep_rad = signed_sweep.abs();
    if !(PI / 15.0..=FRAC_PI_2).contains(&sweep_rad) {
        return None;
    }
    let direction = signed_sweep.signum();
    if points.windows(2).any(|pair| {
        let step = (angle(pair[1]) - angle(pair[0]) + PI).rem_euclid(TAU) - PI;
        step * direction <= 0.0
    }) {
        return None;
    }
    let start_rad = if direction > 0.0 { initial } else { last };
    Some(CircularSector { start_rad: start_rad.rem_euclid(TAU), sweep_rad })
}

// Identify the separate straight sides of a housing. An arc center only
// limits the search; the housing center is calculated from its four corners.
fn square_housing<'a>(
    geometry: &'a SvgGeometryDocument,
    view: PdfBox,
    fit: &CircleFit,
    tolerance: f64,
) -> Option<SquareHousing<'a>> {
    let mut horizontal = Vec::new();
    let mut vertical = Vec::new();
    for line in &geometry.primitives {
        if line.kind != VectorPrimitiveKind::Line || line.style.stroke.is_none()
            || line.style.stroke_dasharray.is_some()
        {
            continue;
        }
        let [a, b] = line.points.as_slice() else { continue };
        if a.dist(*b) <= tolerance {
            continue;
        }
        for point in [a, b] {
            let dx = (point.x - fit.center.x).abs();
            let dy = (point.y - fit.center.y).abs();
            if dx <= fit.radius || dy <= fit.radius
                || dx >= 2.0 * fit.radius || dy >= 2.0 * fit.radius
                || point.x < view.x_min || point.x > view.x_max
                || point.y < view.y_min || point.y > view.y_max
            {
                continue;
            }
            if (a.y - b.y).abs() <= tolerance {
                horizontal.push((*point, line.id.as_str()));
            } else if (a.x - b.x).abs() <= tolerance {
                vertical.push((*point, line.id.as_str()));
            }
        }
    }

    let mut corners: [Option<(Point, &str, &str)>; 4] = [None; 4];
    for (h, h_id) in horizontal {
        for &(v, v_id) in &vertical {
            if h.dist(v) > tolerance {
                continue;
            }
            let corner = Point { x: v.x, y: h.y };
            let index = usize::from(corner.x > fit.center.x)
                + 2 * usize::from(corner.y > fit.center.y);
            match corners[index] {
                Some((previous, _, _)) if previous.dist(corner) > tolerance => return None,
                Some(_) => {}
                None => corners[index] = Some((corner, h_id, v_id)),
            }
        }
    }
    let [top_left, top_right, bottom_left, bottom_right] = corners;
    let [tl, tr, bl, br] = [top_left?.0, top_right?.0, bottom_left?.0, bottom_right?.0];
    let bounds_pt = PdfBox {
        x_min: (tl.x + bl.x) / 2.0,
        y_min: (tl.y + tr.y) / 2.0,
        x_max: (tr.x + br.x) / 2.0,
        y_max: (bl.y + br.y) / 2.0,
    };
    let center_pt = Point {
        x: (bounds_pt.x_min + bounds_pt.x_max) / 2.0,
        y: (bounds_pt.y_min + bounds_pt.y_max) / 2.0,
    };
    let width = bounds_pt.x_max - bounds_pt.x_min;
    let height = bounds_pt.y_max - bounds_pt.y_min;
    if [(tl.x, bl.x), (tr.x, br.x), (tl.y, tr.y), (bl.y, br.y)]
        .iter().any(|(a, b)| (a - b).abs() > tolerance)
        || (width - height).abs() > tolerance
        || center_pt.dist(fit.center) > tolerance
    {
        return None;
    }
    let mut source_ids: Vec<_> = corners.into_iter().flatten()
        .flat_map(|(_, horizontal, vertical)| [horizontal, vertical]).collect();
    source_ids.sort_unstable();
    source_ids.dedup();
    Some(SquareHousing { center_pt, bounds_pt, source_ids })
}

pub(crate) fn propose_arc_occurrences(
    geometry: &SvgGeometryDocument,
    view: PdfBox,
) -> Result<Vec<ArcOccurrence<'_>>, String> {
    let contains = |rect: PdfBox, other: PdfBox| {
        rect.x_min <= other.x_min && rect.y_min <= other.y_min
            && other.x_max <= rect.x_max && other.y_max <= rect.y_max
    };
    let page = PdfBox {
        x_min: 0.0, y_min: 0.0, x_max: geometry.width_pt, y_max: geometry.height_pt,
    };
    if !contains(page, view) || view.x_min >= view.x_max || view.y_min >= view.y_max {
        return Err("Le rectangle de la vue doit être compris dans la page PDF.".into());
    }

    // PDF export flattens arcs, and fitting their short spans amplifies center
    // uncertainty. Use a wider search threshold before fitting all four spans.
    let radial_tolerance = 2.0 * geometry.flattening_tolerance_pt;
    let fit_search = 8.0 * radial_tolerance;
    let mut clusters: Vec<Vec<ArcSupport<'_>>> = Vec::new();
    for primitive in &geometry.primitives {
        if primitive.kind != VectorPrimitiveKind::Polyline || primitive.closed
            || primitive.points.len() < 5 || primitive.style.stroke.is_none()
            || primitive.style.stroke_dasharray.is_some() || !contains(view, primitive.bbox)
        {
            continue;
        }
        let Some(fit) = fit_circle(&primitive.points) else { continue };
        let first = primitive.points[0];
        let last = primitive.points[primitive.points.len() - 1];
        let chord_ratio = first.dist(last) / fit.radius;
        if fit.max_radial_error > radial_tolerance || fit.radius < 4.0 * fit_search
            || !(0.3..=0.9).contains(&chord_ratio)
        {
            continue;
        }
        let middle = primitive.points[primitive.points.len() / 2];
        let angle = (middle.y - fit.center.y).atan2(middle.x - fit.center.x) / FRAC_PI_2;
        let sector = angle.round();
        if (angle - sector).abs() > 0.22 {
            continue;
        }
        let support = ArcSupport {
            fit, sector: (sector as i32).rem_euclid(4) as usize,
            source_id: &primitive.id, points: &primitive.points,
        };
        match clusters.iter_mut().find(|cluster| {
            cluster[0].fit.center.dist(support.fit.center) <= fit_search
                && (cluster[0].fit.radius - support.fit.radius).abs() <= fit_search
        }) {
            Some(cluster) => cluster.push(support),
            None => clusters.push(vec![support]),
        }
    }

    let mut occurrences = Vec::new();
    for cluster in clusters {
        let mut sectors = [0; 4];
        for support in &cluster {
            sectors[support.sector] += 1;
        }
        if sectors.iter().filter(|count| **count > 0).count() < 3 {
            continue;
        }
        let points = cluster.iter().flat_map(|support| support.points.iter().copied()).collect::<Vec<_>>();
        let Some(fit) = fit_circle(&points) else { continue };
        let observed = if sectors == [1; 4] && fit.max_radial_error <= radial_tolerance {
            cluster.iter().map(|support| observed_sector(support.points, fit.center))
                .collect::<Option<Vec<_>>>().unwrap_or_default()
        } else { Vec::new() };
        let state = if observed.is_empty() { "ambiguous" } else { "candidate" };
        occurrences.push(ArcOccurrence {
            center_pt: fit.center,
            radius_pt: fit.radius,
            source_ids: cluster.iter().map(|support| support.source_id).collect(),
            state,
            sectors: observed,
            housing: square_housing(geometry, view, &fit, radial_tolerance),
        });
    }
    occurrences.sort_by(|left, right| left.center_pt.y.total_cmp(&right.center_pt.y)
        .then_with(|| left.center_pt.x.total_cmp(&right.center_pt.x)));
    Ok(occurrences)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dper_pdf_geometry::parse_svg_geometry;

    #[test]
    fn llt_pln_004_groups_arcs_by_center_and_keeps_incomplete_support_visible() {
        let mut svg = String::from(r#"<svg width="160pt" height="90pt" viewBox="0 0 160 90">"#);
        for (x, sectors) in [(25.0, 4), (65.0, 3), (125.0, 4)] {
            for sector in 0..sectors {
                let mut path = String::new();
                for (index, offset) in [-0.3, -0.15, 0.0, 0.15, 0.3].into_iter().enumerate() {
                    let angle = f64::from(sector) * FRAC_PI_2 + offset;
                    let command = if index == 0 { "M" } else { "L" };
                    path.push_str(&format!("{command} {} {} ", x + 10.0 * angle.cos(), 40.0 + 10.0 * angle.sin()));
                }
                svg.push_str(&format!(r#"<path stroke="black" fill="none" d="{path}"/>"#));
            }
        }
        // Use individual strokes, as in the source drawing.
        for (x1, y1, x2, y2) in [
            (11, 26, 21, 26), (29, 26, 39, 26), (39, 26, 39, 36), (39, 44, 39, 54),
            (39, 54, 29, 54), (21, 54, 11, 54), (11, 54, 11, 44), (11, 36, 11, 26),
        ] {
            svg.push_str(&format!(r#"<path stroke="black" fill="none" d="M {x1} {y1} L {x2} {y2}"/>"#));
        }
        svg.push_str("</svg>");
        let catalog = parse_svg_geometry(&svg, 1).unwrap();
        let bounds = PdfBox { x_min: 0.0, y_min: 0.0, x_max: 95.0, y_max: 90.0 };
        let occurrences = propose_arc_occurrences(&catalog, bounds).unwrap();
        assert_eq!(occurrences.len(), 2);
        let complete = occurrences.iter().find(|item| item.state == "candidate").unwrap();
        let incomplete = occurrences.iter().find(|item| item.state == "ambiguous").unwrap();
        assert!(complete.center_pt.dist(Point { x: 25.0, y: 40.0 }) < 0.01);
        assert!(incomplete.center_pt.dist(Point { x: 65.0, y: 40.0 }) < 0.01);
        assert_eq!(complete.source_ids.len(), 4);
        assert_eq!(incomplete.source_ids.len(), 3);
        let housing = complete.housing.as_ref().unwrap();
        assert_eq!(housing.source_ids.len(), 8);
        assert!(housing.center_pt.dist(Point { x: 25.0, y: 40.0 }) < 0.01);
        assert!(incomplete.housing.is_none());
    }
}

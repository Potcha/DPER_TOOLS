use crate::{
    circle_landmark_matching::landmark_luma,
    dper_pdf_geometry::{VectorPrimitive, VectorPrimitiveKind},
    geometry::{point_line_distance, polygon_area, Point},
    landmark_matching::{match_landmarks, LandmarkMatchingRequest, NamedLandmark},
};
use image::RgbImage;
use serde::Serialize;
use serde_json::{json, Value};

const MIN_SIDE_PX: usize = 18;
const MAX_PHOTO_CONTOURS: usize = 12;
const MAX_ASSIGNMENTS: usize = 30_000;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Outline {
    id: String,
    vertices: Vec<Point>,
}

pub(crate) fn plan_outline(primitive: &VectorPrimitive) -> Option<Outline> {
    if primitive.kind != VectorPrimitiveKind::ClosedPolygon || !primitive.closed
        || primitive.style.stroke_dasharray.as_deref().is_some_and(|dash| {
            let dash = dash.trim();
            !dash.is_empty() && dash != "none"
        })
    {
        return None;
    }
    let hull = convex_hull(primitive.points.clone());
    let area = polygon_area(&primitive.points);
    let hull_area = polygon_area(&hull);
    if !(3..=6).contains(&hull.len()) || hull_area <= 0.0
        || (area - hull_area).abs() > hull_area * 0.01
    {
        return None;
    }
    Some(Outline { id: primitive.id.clone(), vertices: hull })
}

pub(crate) fn recognize_polygon_views(
    image: &RgbImage,
    views: &[(String, Vec<Outline>)],
    minimum_supports: usize,
    minimum_coverage: f64,
    threshold_px: f64,
) -> Result<Value, String> {
    if !(3..=6).contains(&minimum_supports)
        || !(0.0..=1.0).contains(&minimum_coverage)
        || !(0.01..=32.0).contains(&threshold_px)
    {
        return Err("Paramètres de reconnaissance hors limites.".into());
    }
    let (threshold, outlines) = photo_outlines(image)?;
    let image_area = f64::from(image.width()) * f64::from(image.height());
    let outlines = outlines.into_iter()
        .filter(|outline| outline.vertices.len() >= minimum_supports)
        .map(|outline| {
            let coverage = polygon_area(&outline.vertices) / image_area;
            (outline, coverage)
        }).collect::<Vec<_>>();
    let mut accepted = Vec::new();
    let mut result_views = Vec::with_capacity(views.len());
    let mut evaluated = 0;
    for (view_id, candidates) in views {
        let mut hypotheses = Vec::new();
        for (observed, coverage) in &outlines {
            if *coverage < minimum_coverage {
                continue;
            }
            for plan in candidates.iter().filter(|plan| plan.vertices.len() == observed.vertices.len()) {
                evaluated += (1..=observed.vertices.len()).product::<usize>();
                if evaluated > MAX_ASSIGNMENTS {
                    return Err("Trop de correspondances de contours. Réduisez le périmètre des vues.".into());
                }
                let report = match_landmarks(&LandmarkMatchingRequest {
                    plan_landmarks: named_corners(plan),
                    image_landmarks: named_corners(observed),
                    plan_source: "Sommets du contour vectoriel PDF".into(),
                    image_source: "Sommets du contour visible de la capture".into(),
                    threshold_px,
                })?;
                for mut hypothesis in report.hypotheses {
                    hypothesis.candidate_id = format!("{}-{}-{}", plan.id, observed.id, hypothesis.candidate_id);
                    accepted.push((view_id, observed.vertices.len(), *coverage));
                    hypotheses.push(hypothesis);
                }
            }
        }
        result_views.push(json!({
            "view_id": view_id, "plan_outlines": candidates, "hypotheses": hypotheses
        }));
    }
    let best_coverage = outlines.iter().map(|(_, coverage)| *coverage).fold(0.0, f64::max);
    let (state, code, message) = match accepted.len() {
        1 => ("unique_hypothesis", None, None),
        0 if outlines.is_empty() => ("no_match", Some("insufficient_supports"), Some("Aucun contour polygonal fermé exploitable dans la capture.")),
        0 if best_coverage < minimum_coverage => ("no_match", Some("insufficient_coverage"), Some("Les contours polygonaux couvrent une zone trop petite de la capture.")),
        0 => ("no_match", Some("no_compatible_view"), Some("Aucune vue ne correspond aux contours polygonaux avec le seuil choisi.")),
        _ => ("indeterminate", Some("multiple_hypotheses"), Some("Vue indéterminée : plusieurs correspondances de contours sont compatibles.")),
    };
    let unique = accepted.first().filter(|_| accepted.len() == 1);
    Ok(json!({
        "state": state,
        "method": "polygon_contours",
        "refusal": code.zip(message).map(|(code, message)| json!({"code": code, "message": message})),
        "proposed_view_id": unique.map(|(view_id, _, _)| view_id),
        "evaluated_assignments": evaluated,
        "support_count": unique.map_or_else(|| outlines.iter().map(|(shape, _)| shape.vertices.len()).max().unwrap_or(0), |(_, count, _)| *count),
        "coverage": unique.map_or(best_coverage, |(_, _, coverage)| *coverage),
        "minimum_supports": minimum_supports,
        "minimum_coverage": minimum_coverage,
        "threshold_px": threshold_px,
        "segmentation_threshold": threshold,
        "image_outlines": outlines.into_iter().map(|(outline, _)| outline).collect::<Vec<_>>(),
        "views": result_views,
    }))
}

fn named_corners(outline: &Outline) -> Vec<NamedLandmark> {
    outline.vertices.iter().enumerate().map(|(index, &position)| NamedLandmark {
        id: format!("{}-sommet-{}", outline.id, index + 1),
        position,
    }).collect()
}

fn photo_outlines(image: &RgbImage) -> Result<(u8, Vec<Outline>), String> {
    let (gray, threshold) = landmark_luma(image)?;
    let (width, height) = (gray.width() as usize, gray.height() as usize);
    let mut labels: Vec<_> = gray.as_raw().iter().map(|&value| u8::from(value > threshold)).collect();
    let mut rows = vec![(width, 0); height];
    let mut queue = Vec::new();
    let mut outlines = Vec::new();
    for start in 0..labels.len() {
        let polarity = labels[start];
        if polarity == 2 {
            continue;
        }
        labels[start] = 2;
        queue.push(start);
        let (mut left, mut top) = (width, height);
        let (mut right, mut bottom, mut pixels) = (0, 0, 0);
        while let Some(index) = queue.pop() {
            let (x, y) = (index % width, index / width);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
            pixels += 1;
            rows[y].0 = rows[y].0.min(x);
            rows[y].1 = rows[y].1.max(x);
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < width && ny < height {
                    let neighbor = ny * width + nx;
                    if labels[neighbor] == polarity {
                        labels[neighbor] = 2;
                        queue.push(neighbor);
                    }
                }
            }
        }
        let extent = &mut rows[top..=bottom];
        let span = (right - left + 1).min(bottom - top + 1);
        if span >= MIN_SIDE_PX && left > 0 && top > 0 && right + 1 < width && bottom + 1 < height {
            let extremes = extent.iter().enumerate().flat_map(|(row, &(low, high))| {
                let y = (row + top) as f64;
                [Point { x: low as f64, y }, Point { x: high as f64, y }]
            }).collect();
            let hull = convex_hull(extremes);
            let hull_area = polygon_area(&hull);
            // A large void or a concavity is not supported by its convex hull.
            if hull_area > 0.0 && pixels as f64 >= 0.94 * hull_area {
                let vertices = simplify_pixel_hull(hull);
                if (3..=6).contains(&vertices.len())
                    && polygon_area(&vertices) >= 0.97 * hull_area
                {
                    if outlines.len() == MAX_PHOTO_CONTOURS {
                        return Err("Trop de contours visibles pour départager les vues. Recadrez la photo.".into());
                    }
                    outlines.push(Outline { id: format!("photo-contour-{}", outlines.len() + 1), vertices });
                }
            }
        }
        extent.fill((width, 0));
    }
    Ok((threshold, outlines))
}

fn convex_hull(mut points: Vec<Point>) -> Vec<Point> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if points.len() < 3 {
        return points;
    }
    fn half(points: impl Iterator<Item = Point>) -> Vec<Point> {
        let mut hull: Vec<Point> = Vec::new();
        for point in points {
            while hull.len() >= 2 {
                let a = hull[hull.len() - 2];
                let b = hull[hull.len() - 1];
                let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
                if cross > 0.0 {
                    break;
                }
                hull.pop();
            }
            hull.push(point);
        }
        hull.pop();
        hull
    }
    let mut hull = half(points.iter().copied());
    hull.extend(half(points.iter().rev().copied()));
    hull
}

fn simplify_pixel_hull(mut hull: Vec<Point>) -> Vec<Point> {
    while hull.len() > 3 {
        let len = hull.len();
        let nearest = (0..len).filter_map(|i| {
            point_line_distance(hull[i], hull[(i + len - 1) % len], hull[(i + 1) % len])
                .map(|distance| (i, distance))
        }).min_by(|(_, a), (_, b)| a.total_cmp(b));
        let Some((index, distance)) = nearest else { break; };
        if distance > 1.15 {
            break;
        }
        hull.remove(index);
    }
    hull
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn outline(id: &str, vertices: &[(f64, f64)]) -> Outline {
        Outline {
            id: id.into(),
            vertices: vertices.iter().map(|&(x, y)| Point { x, y }).collect(),
        }
    }

    fn capture(outline: &Outline, angle: f64, width: u32, height: u32) -> RgbImage {
        let (sin, cos) = angle.sin_cos();
        let vertices: Vec<_> = outline.vertices.iter().map(|p| Point {
            x: (p.x * cos - p.y * sin) * 1.7 + f64::from(width) * 0.46,
            y: (p.x * sin + p.y * cos) * 1.7 + f64::from(height) * 0.44,
        }).collect();
        RgbImage::from_fn(width, height, |x, y| {
            let (x, y) = (f64::from(x), f64::from(y));
            let inside = vertices.iter().zip(vertices.iter().cycle().skip(1)).all(|(a, b)| {
                (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x) >= 0.0
            });
            Rgb([if inside { 35 } else { 215 }; 3])
        })
    }

    #[test]
    fn llt_aut_003_01_matches_oriented_polygon_contours_and_preserves_repeated_ambiguity() {
        let top = outline("top", &[(-30., -20.), (20., -20.), (33., 14.), (-20., 25.)]);
        let side = outline("side", &[(-30., -20.), (30., -20.), (22., 18.), (2., 27.), (-28., 18.)]);
        let views = vec![
            ("dessus".into(), vec![top.clone()]),
            ("profil".into(), vec![side.clone()]),
        ];
        for (shape, angle, size, expected) in [
            (&top, 0.55, (260, 210), "dessus"),
            (&top, -0.85, (165, 150), "dessus"),
            (&side, 0.25, (240, 200), "profil"),
        ] {
            let photo = capture(shape, angle, size.0, size.1);
            let result = recognize_polygon_views(&photo, &views, 4, 0.05, 2.0).unwrap();
            assert_eq!(result["proposed_view_id"], expected, "{angle} rad, {size:?}: {result}");
        }
        let photo = capture(&top, 0.55, 260, 210);
        let mut repeated = top.clone();
        repeated.id = "copie".into();
        let ambiguous = recognize_polygon_views(
            &photo, &[("dessus".into(), vec![top, repeated])], 4, 0.05, 2.0,
        ).unwrap();
        assert_eq!(ambiguous["state"], "indeterminate", "{ambiguous}");
        assert!(ambiguous["proposed_view_id"].is_null());
        let too_small = recognize_polygon_views(&photo, &views, 4, 0.9, 2.0).unwrap();
        assert_eq!(too_small["refusal"]["code"], "insufficient_coverage");
    }
}

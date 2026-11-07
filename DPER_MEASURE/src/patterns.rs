//! Geometric copies in native image pixels. Originals are never modified.
use crate::geometry::{Measurement, Point, Tool};

const MAX_GENERATED: usize = 1_000;
const MAX_GENERATED_POINTS: usize = 1_000_000;
const COPY_NOTE: &str = "Copie géométrique à ajuster aux pixels";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Mirror,
    Rectangular,
    Circular,
}

/// Positions, axis endpoints and translation steps use native image pixels.
#[derive(Clone, Debug)]
pub struct PatternOptions {
    pub rows: u32,
    pub cols: u32,
    pub step_x: f64,
    pub step_y: f64,
    /// Includes the original position, which is omitted from the returned copies.
    pub count: u32,
    /// Total sweep; a full turn distributes positions without duplicating the first.
    pub angle_degrees: f64,
    pub center: Point,
    pub axis_start: Point,
    pub axis_end: Point,
}

impl Default for PatternOptions {
    fn default() -> Self {
        Self {
            rows: 2,
            cols: 2,
            step_x: 100.0,
            step_y: 100.0,
            count: 6,
            angle_degrees: 360.0,
            center: Point::default(),
            axis_start: Point::default(),
            axis_end: Point { x: 0.0, y: 100.0 },
        }
    }
}

#[derive(Clone, Copy)]
struct Transform {
    xx: f64,
    xy: f64,
    yx: f64,
    yy: f64,
    origin: Point,
    offset: Point,
}

impl Transform {
    fn apply(self, point: Point) -> Point {
        let x = point.x - self.origin.x;
        let y = point.y - self.origin.y;
        Point {
            x: self.origin.x + self.xx * x + self.xy * y + self.offset.x,
            y: self.origin.y + self.yx * x + self.yy * y + self.offset.y,
        }
    }

    fn keeps_rectangle_axis_aligned(self) -> bool {
        (self.xy.abs() <= 1e-12 && self.yx.abs() <= 1e-12)
            || (self.xx.abs() <= 1e-12 && self.yy.abs() <= 1e-12)
    }
}

fn finite(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

/// Returns copies only. The caller assigns fresh IDs and selects sources from
/// the active capture before calling this function.
pub fn generate(
    sources: &[Measurement],
    kind: PatternKind,
    options: &PatternOptions,
) -> Result<Vec<Measurement>, String> {
    if sources.is_empty() {
        return Err("Sélectionnez au moins un tracé à répéter.".into());
    }
    for source in sources {
        if source.points.is_empty()
            || source.points.iter().any(|point| !finite(*point))
            || source.label_position_native_px.is_some_and(|point| !finite(point))
        {
            return Err("Un tracé source contient des coordonnées absentes ou non finies.".into());
        }
        if source.kind == Tool::Rectangle && source.points.len() != 2 {
            return Err("Un rectangle source doit être défini par deux coins opposés.".into());
        }
    }

    let copies_per_source = match kind {
        PatternKind::Mirror => 1,
        PatternKind::Rectangular => {
            if options.rows == 0 || options.cols == 0
                || options.rows > MAX_GENERATED as u32 + 1
                || options.cols > MAX_GENERATED as u32 + 1
            {
                return Err("Le nombre de lignes et de colonnes doit être compris entre 1 et 1001.".into());
            }
            if !options.step_x.is_finite() || !options.step_y.is_finite() {
                return Err("Les pas de répétition doivent être finis.".into());
            }
            if (options.cols > 1 && options.step_x == 0.0)
                || (options.rows > 1 && options.step_y == 0.0)
            {
                return Err("Le pas doit être non nul sur chaque axe répété.".into());
            }
            let positions = u64::from(options.rows) * u64::from(options.cols);
            if positions < 2 {
                return Err("La grille doit comporter au moins deux positions.".into());
            }
            (positions - 1) as usize
        }
        PatternKind::Circular => {
            if !(2..=MAX_GENERATED as u32 + 1).contains(&options.count) {
                return Err("Le nombre de positions doit être compris entre 2 et 1001.".into());
            }
            if !finite(options.center) || !options.angle_degrees.is_finite()
                || options.angle_degrees.abs() <= 1e-9
                || options.angle_degrees.abs() > 360.0 + 1e-9
            {
                return Err("Indiquez un centre fini et un angle non nul compris entre -360° et 360°.".into());
            }
            (options.count - 1) as usize
        }
    };
    let total = sources.len().checked_mul(copies_per_source)
        .filter(|total| *total <= MAX_GENERATED)
        .ok_or_else(|| "La répétition est limitée à 1000 nouveaux tracés.".to_owned())?;
    // Rectangle copies can require four corners instead of two.
    let source_points = sources.iter().try_fold(0usize, |sum, source| {
        sum.checked_add(if source.kind == Tool::Rectangle { 4 } else { source.points.len() })
    });
    if source_points.and_then(|count| count.checked_mul(copies_per_source))
        .is_none_or(|count| count > MAX_GENERATED_POINTS)
    {
        return Err("La répétition dépasse la limite de points autorisée.".into());
    }

    let mut result = Vec::with_capacity(total);
    match kind {
        PatternKind::Mirror => {
            if !finite(options.axis_start) || !finite(options.axis_end) {
                return Err("Les deux points de l’axe doivent être finis.".into());
            }
            let dx = options.axis_end.x - options.axis_start.x;
            let dy = options.axis_end.y - options.axis_start.y;
            let length = dx.hypot(dy);
            if !length.is_finite() || length <= 1e-9 {
                return Err("Définissez l’axe de symétrie avec deux points distincts.".into());
            }
            let ux = dx / length;
            let uy = dy / length;
            let transform = Transform {
                xx: 2.0 * ux * ux - 1.0,
                xy: 2.0 * ux * uy,
                yx: 2.0 * ux * uy,
                yy: 2.0 * uy * uy - 1.0,
                origin: options.axis_start,
                offset: Point::default(),
            };
            append_copies(&mut result, sources, transform, "symétrie")?;
        }
        PatternKind::Rectangular => {
            for row in 0..options.rows {
                for col in 0..options.cols {
                    if row == 0 && col == 0 { continue; }
                    let transform = Transform {
                        xx: 1.0, xy: 0.0, yx: 0.0, yy: 1.0,
                        origin: Point::default(),
                        offset: Point {
                            x: f64::from(col) * options.step_x,
                            y: f64::from(row) * options.step_y,
                        },
                    };
                    append_copies(&mut result, sources, transform,
                        &format!("répétition {}, {}", row + 1, col + 1))?;
                }
            }
        }
        PatternKind::Circular => {
            let full_turn = (options.angle_degrees.abs() - 360.0).abs() <= 1e-9;
            let divisor = if full_turn { options.count } else { options.count - 1 };
            let step = options.angle_degrees.to_radians() / f64::from(divisor);
            for index in 1..options.count {
                let (sin, cos) = (step * f64::from(index)).sin_cos();
                let transform = Transform {
                    xx: cos, xy: -sin, yx: sin, yy: cos,
                    origin: options.center,
                    offset: Point::default(),
                };
                append_copies(&mut result, sources, transform,
                    &format!("répétition circulaire {}", index + 1))?;
            }
        }
    }
    Ok(result)
}

fn append_copies(
    output: &mut Vec<Measurement>,
    sources: &[Measurement],
    transform: Transform,
    suffix: &str,
) -> Result<(), String> {
    for source in sources {
        let mut copy = source.clone();
        if source.kind == Tool::Rectangle && !transform.keeps_rectangle_axis_aligned() {
            let first = source.points[0];
            let second = source.points[1];
            copy.points = vec![
                first,
                Point { x: second.x, y: first.y },
                second,
                Point { x: first.x, y: second.y },
            ];
            copy.kind = Tool::Polygon;
            copy.drawing_variant = Some("Rectangle — répétition".into());
        }
        for point in &mut copy.points {
            *point = transform.apply(*point);
        }
        copy.label_position_native_px = source.label_position_native_px
            .map(|point| transform.apply(point));
        if copy.points.iter().any(|point| !finite(*point))
            || copy.label_position_native_px.is_some_and(|point| !finite(point))
        {
            return Err("La répétition produit des coordonnées non finies ; réduisez ses paramètres.".into());
        }
        copy.id = 0;
        copy.origin = "pattern".into();
        // A geometric copy is derived from its source, never a new pixel observation.
        copy.source_measurement_ids = vec![source.id];
        copy.dependency_stale = source.dependency_stale;
        copy.expanded_uncertainty_mm = None;
        copy.label = format!("{} — {}", source.label, suffix);
        copy.method = Some(match source.method.as_deref().filter(|method| !method.is_empty()) {
            Some(method) => format!("{COPY_NOTE}. {method}"),
            None => COPY_NOTE.into(),
        });
        output.push(copy);
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: u64, construction: bool) -> Measurement {
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": "Polygon", "capture_id": "synthetic-pattern-grid",
            "points": [{"x": 100.0, "y": 100.0}, {"x": 120.0, "y": 110.0},
                {"x": 110.0, "y": 130.0}, {"x": 90.0, "y": 120.0}],
            "drawing_variant": "Rectangle orienté — 3 points", "label": format!("M{id}"),
            "is_construction": construction, "origin": "interface",
            "label_position_native_px": {"x": 120.0, "y": 140.0},
            "mm_per_pixel": 0.1, "expanded_uncertainty_mm": 0.025
        })).unwrap()
    }

    #[test]
    fn rectangular_grid_rejects_zero_step_only_on_repeated_axes() {
        let sources = [source(1, false)];
        for (rows, cols, step_x, step_y) in [
            (1, 4, 0.0, 10.0), (4, 1, 10.0, 0.0),
            (4, 4, 0.0, 10.0), (4, 4, 10.0, -0.0),
        ] {
            let options = PatternOptions { rows, cols, step_x, step_y, ..Default::default() };
            let error = generate(&sources, PatternKind::Rectangular, &options).unwrap_err();
            assert!(error.contains("non nul"));
        }
        for (rows, cols, step_x, step_y) in [(1, 4, 10.0, 0.0), (4, 1, 0.0, -10.0)] {
            let options = PatternOptions { rows, cols, step_x, step_y, ..Default::default() };
            assert_eq!(generate(&sources, PatternKind::Rectangular, &options).unwrap().len(), 3);
        }
    }

    #[test]
    fn four_by_four_grid_preserves_every_source_at_each_row_major_position_in_both_directions() {
        let sources = [source(11, false), source(22, true)];
        let originals = serde_json::to_value(&sources).unwrap();
        for (step_x, step_y) in [(30.0, 40.0), (-30.0, -40.0)] {
            let options = PatternOptions { rows: 4, cols: 4, step_x, step_y, ..Default::default() };
            let copies = generate(&sources, PatternKind::Rectangular, &options).unwrap();
            assert_eq!(copies.len(), 15 * sources.len());
            let mut cursor = 0;
            for row in 0..4 {
                for col in 0..4 {
                    if row == 0 && col == 0 { continue; }
                    for original in &sources {
                        let copy = &copies[cursor];
                        cursor += 1;
                        let delta = Point { x: f64::from(col) * step_x, y: f64::from(row) * step_y };
                        let expected = original.points.iter().map(|p| Point {
                            x: p.x + delta.x, y: p.y + delta.y,
                        }).collect::<Vec<_>>();
                        assert_eq!(copy.points, expected);
                        let label = original.label_position_native_px.unwrap();
                        assert_eq!(copy.label_position_native_px, Some(Point {
                            x: label.x + delta.x, y: label.y + delta.y,
                        }));
                        assert_eq!(copy.id, 0, "The application must assign a fresh ID");
                        assert_eq!(copy.capture_id, original.capture_id);
                        assert_eq!(copy.kind, original.kind);
                        assert_eq!(copy.drawing_variant, original.drawing_variant);
                        assert_eq!(copy.is_construction, original.is_construction);
                        assert_eq!(copy.source_measurement_ids, [original.id]);
                        assert_eq!(copy.expanded_uncertainty_mm, None);
                        assert_eq!(copy.origin, "pattern");
                        assert!(copy.method.as_deref().unwrap().starts_with(COPY_NOTE));
                        if copy.is_construction {
                            assert!(crate::geometry::result(copy).starts_with("Construction"));
                            assert_eq!(crate::reference::measurement_mm(copy), None);
                        }
                        let value = serde_json::to_value(copy).unwrap();
                        for authority in ["nominal_mm", "tolerance_minus_mm", "tolerance_plus_mm",
                            "evidence", "geometry_state", "decision"] {
                            assert!(value.get(authority).is_none(), "A copy must not contain {authority}");
                        }
                        let restored: Measurement = serde_json::from_value(value).unwrap();
                        assert_eq!(restored.points, expected);
                        assert_eq!(restored.drawing_variant, copy.drawing_variant);
                        assert_eq!(restored.is_construction, copy.is_construction);
                        assert_eq!(restored.source_measurement_ids, copy.source_measurement_ids);
                        assert_eq!(restored.expanded_uncertainty_mm, None);
                    }
                }
            }
            assert_eq!(cursor, copies.len());
        }
        assert_eq!(serde_json::to_value(&sources).unwrap(), originals, "Sources remain untouched");
    }
}

//! Numeric resizing of existing native geometry; never uses drawing nominals.
use crate::geometry::{Measurement, Point, Tool};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor { First, Last, Center }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dimensions {
    pub primary: f64,
    pub secondary: Option<f64>,
    pub calibrated: bool,
}

pub fn parse_value(text: &str) -> Result<f64, String> {
    let value = text.trim().replace(',', ".").parse::<f64>()
        .map_err(|_| "Saisissez un nombre positif (point ou virgule décimale).".to_owned())?;
    positive(value)?;
    Ok(value)
}

fn positive(value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 { Ok(()) }
    else { Err("La dimension doit être finie et strictement positive.".into()) }
}
fn sub(a: Point, b: Point) -> Point { Point { x: a.x-b.x, y: a.y-b.y } }
fn add(a: Point, b: Point) -> Point { Point { x: a.x+b.x, y: a.y+b.y } }
fn mul(a: Point, n: f64) -> Point { Point { x: a.x*n, y: a.y*n } }
fn length(p: Point) -> f64 { p.x.hypot(p.y) }
fn finite(p: Point) -> bool { p.x.is_finite() && p.y.is_finite() }

fn scales(m: &Measurement) -> (f64, f64, bool) {
    let valid = |value: Option<f64>| value.filter(|v| v.is_finite() && *v > 0.0);
    let fallback = valid(m.mm_per_pixel);
    match (valid(m.mm_per_pixel_x).or(fallback), valid(m.mm_per_pixel_y).or(fallback)) {
        (Some(x), Some(y)) => (x, y, true),
        _ => (1.0, 1.0, false),
    }
}
fn metric_length(p: Point, m: &Measurement) -> f64 {
    let (x, y, _) = scales(m);
    (p.x*x).hypot(p.y*y)
}

enum Shape { Segment, Circle, Rectangle { u: Point, v: Point } }
fn shape(m: &Measurement) -> Result<Shape, String> {
    if m.points.iter().any(|p| !finite(*p)) {
        return Err("Les coordonnées de la géométrie doivent être finies.".into());
    }
    let p = &m.points;
    match m.kind {
        Tool::Distance | Tool::Circle if p.len() == 2 => {
            positive(length(sub(p[1], p[0])))?;
            Ok(if m.kind == Tool::Circle { Shape::Circle } else { Shape::Segment })
        }
        Tool::Rectangle if p.len() == 2 => {
            let delta = sub(p[1], p[0]);
            positive(delta.x.abs())?;
            positive(delta.y.abs())?;
            Ok(Shape::Rectangle { u: Point { x: delta.x, y: 0.0 }, v: Point { x: 0.0, y: delta.y } })
        }
        Tool::Polygon if p.len() == 4 && m.drawing_variant.as_deref().is_some_and(|s| s.starts_with("Rectangle")) => {
            let u = sub(p[1], p[0]);
            let v = sub(p[3], p[0]);
            let a = length(u);
            let b = length(v);
            positive(a)?;
            positive(b)?;
            let dot = (u.x/a)*(v.x/b) + (u.y/a)*(v.y/b);
            let expected = add(p[0], add(u, v));
            let closure_error = length(sub(expected, p[2]));
            if !dot.is_finite() || dot.abs() > 1e-8 || !closure_error.is_finite()
                || closure_error > a.max(b)*1e-8 {
                return Err("Les quatre sommets ne forment pas un rectangle réel.".into());
            }
            Ok(Shape::Rectangle { u, v })
        }
        _ => Err("Redimensionnement disponible pour un segment à deux points, un cercle centre/bord ou un rectangle.".into()),
    }
}

pub fn dimensions(m: &Measurement, circle_radius: bool) -> Result<Dimensions, String> {
    let (_, _, calibrated) = scales(m);
    let (primary, secondary) = match shape(m)? {
        Shape::Segment => (metric_length(sub(m.points[1], m.points[0]), m), None),
        Shape::Circle => (metric_length(sub(m.points[1], m.points[0]), m) * if circle_radius { 1.0 } else { 2.0 }, None),
        Shape::Rectangle { u, v } => (metric_length(u, m), Some(metric_length(v, m))),
    };
    positive(primary)?;
    if let Some(value) = secondary { positive(value)?; }
    Ok(Dimensions { primary, secondary, calibrated })
}

pub fn resize(m: &Measurement, primary: f64, secondary: Option<f64>, anchor: Anchor,
    circle_radius: bool) -> Result<Vec<Point>, String> {
    positive(primary)?;
    let existing = dimensions(m, circle_radius)?;
    let p = &m.points;
    let result = match shape(m)? {
        Shape::Segment => {
            if secondary.is_some() { return Err("Un segment possède une seule dimension.".into()); }
            let vector = mul(sub(p[1], p[0]), primary / existing.primary);
            match anchor {
                Anchor::First => vec![p[0], add(p[0], vector)],
                Anchor::Last => vec![sub(p[1], vector), p[1]],
                Anchor::Center => {
                    let center = add(mul(p[0], 0.5), mul(p[1], 0.5));
                    vec![sub(center, mul(vector, 0.5)), add(center, mul(vector, 0.5))]
                }
            }
        }
        Shape::Circle => {
            if secondary.is_some() { return Err("Un cercle possède une seule dimension (rayon ou diamètre).".into()); }
            // The centre always remains fixed, independently of the anchor selector.
            vec![p[0], add(p[0], mul(sub(p[1], p[0]), primary / existing.primary))]
        }
        Shape::Rectangle { u, v } => {
            let height = secondary.ok_or_else(|| "Saisissez la largeur et la hauteur du rectangle.".to_owned())?;
            positive(height)?;
            let u = mul(u, primary / existing.primary);
            let v = mul(v, height / existing.secondary.unwrap());
            let diagonal = add(u, v);
            let last = if m.kind == Tool::Rectangle { p[1] } else { p[2] };
            let first = match anchor {
                Anchor::First => p[0],
                Anchor::Last => sub(last, diagonal),
                Anchor::Center => sub(add(mul(p[0], 0.5), mul(last, 0.5)), mul(diagonal, 0.5)),
            };
            if m.kind == Tool::Rectangle { vec![first, add(first, diagonal)] }
            else { vec![first, add(first, u), add(first, diagonal), add(first, v)] }
        }
    };
    if result.iter().any(|p| !finite(*p)) {
        return Err("Le redimensionnement dépasse les coordonnées finies autorisées.".into());
    }
    let mut resized = m.clone();
    resized.points = result.clone();
    let final_dimensions = dimensions(&resized, circle_radius)?;
    // Ensure tiny requested sizes did not disappear through floating point rounding.
    positive(final_dimensions.primary)?;
    if let Some(value) = final_dimensions.secondary { positive(value)?; }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn measurement(kind: Tool, points: &[[f64; 2]]) -> Measurement {
        serde_json::from_value(serde_json::json!({ "id": 1, "kind": kind, "label": "Synthetic",
            "points": points.iter().map(|p| Point { x:p[0], y:p[1] }).collect::<Vec<_>>() })).unwrap()
    }
    fn close(a: f64, b: f64) { assert!((a-b).abs() < 1e-8, "{a} != {b}"); }
    fn point(a: Point, b: Point) { close(a.x,b.x); close(a.y,b.y); }
    fn with_points(m: &Measurement, points: Vec<Point>) -> Measurement {
        let mut result=m.clone(); result.points=points; result
    }
    #[test]
    fn segment_direction_and_all_anchors_survive_resize() {
        let m=measurement(Tool::Distance,&[[10.0,20.0],[13.0,24.0]]);
        for (anchor, first,last) in [(Anchor::First,[10.0,20.0],[16.0,28.0]),
            (Anchor::Last,[7.0,16.0],[13.0,24.0]),(Anchor::Center,[8.5,18.0],[14.5,26.0])] {
            let p=resize(&m,10.0,None,anchor,false).unwrap();
            point(p[0],Point{x:first[0],y:first[1]}); point(p[1],Point{x:last[0],y:last[1]});
            close(dimensions(&with_points(&m,p),false).unwrap().primary,10.0);
        }
    }
    #[test]
    fn anisotropic_segment_and_fallback_use_the_measurements_own_scales() {
        let mut m=measurement(Tool::Distance,&[[0.0,0.0],[3.0,4.0]]);
        m.mm_per_pixel=Some(0.1); m.mm_per_pixel_x=Some(0.2); m.mm_per_pixel_y=Some(0.4);
        let d=dimensions(&m,false).unwrap(); assert!(d.calibrated); close(d.primary,0.6_f64.hypot(1.6));
        let p=resize(&m,d.primary*2.0,None,Anchor::First,false).unwrap(); point(p[1],Point{x:6.0,y:8.0});
        m.mm_per_pixel_y=None;
        close(dimensions(&m,false).unwrap().primary,0.6_f64.hypot(0.4));
        m.mm_per_pixel=None;
        assert!(!dimensions(&m,false).unwrap().calibrated); close(dimensions(&m,false).unwrap().primary,5.0);
    }
    #[test]
    fn circle_radius_diameter_and_anisotropy_preserve_center_and_edge_direction() {
        let mut m=measurement(Tool::Circle,&[[10.0,20.0],[13.0,24.0]]);
        m.mm_per_pixel_x=Some(2.0); m.mm_per_pixel_y=Some(3.0);
        let radius=6.0_f64.hypot(12.0);
        close(dimensions(&m,true).unwrap().primary,radius);
        close(dimensions(&m,false).unwrap().primary,radius*2.0);
        for (is_radius,wanted) in [(true,radius*2.0),(false,radius*4.0)] {
            let p=resize(&m,wanted,None,Anchor::Last,is_radius).unwrap();
            point(p[0],m.points[0]); point(p[1],Point{x:16.0,y:28.0});
        }
    }
    #[test]
    fn reversed_rectangle_corners_preserve_signs_and_all_anchors() {
        let mut m=measurement(Tool::Rectangle,&[[80.0,20.0],[20.0,60.0]]);
        m.mm_per_pixel_x=Some(0.5); m.mm_per_pixel_y=Some(2.0);
        let d=dimensions(&m,false).unwrap(); close(d.primary,30.0); close(d.secondary.unwrap(),80.0);
        for anchor in [Anchor::First,Anchor::Last,Anchor::Center] {
            let p=resize(&m,15.0,Some(40.0),anchor,false).unwrap();
            close(p[1].x-p[0].x,-30.0); close(p[1].y-p[0].y,20.0);
            match anchor { Anchor::First=>point(p[0],m.points[0]), Anchor::Last=>point(p[1],m.points[1]),
                Anchor::Center=>point(mul(add(p[0],p[1]),0.5),Point{x:50.0,y:40.0}) }
        }
    }
    #[test]
    fn oriented_rectangle_preserves_orientation_and_opposite_or_center_anchor() {
        let mut m=measurement(Tool::Polygon,&[[10.0,10.0],[14.0,13.0],[8.0,21.0],[4.0,18.0]]);
        m.drawing_variant=Some("Rectangle orienté — 3 points".into());
        m.mm_per_pixel_x=Some(2.0); m.mm_per_pixel_y=Some(3.0);
        let original=dimensions(&m,false).unwrap();
        for anchor in [Anchor::First,Anchor::Last,Anchor::Center] {
            let p=resize(&m,original.primary*2.0,Some(original.secondary.unwrap()*0.5),anchor,false).unwrap();
            point(sub(p[1],p[0]),Point{x:8.0,y:6.0}); point(sub(p[3],p[0]),Point{x:-3.0,y:4.0});
            match anchor { Anchor::First=>point(p[0],m.points[0]),Anchor::Last=>point(p[2],m.points[2]),
                Anchor::Center=>point(mul(add(p[0],p[2]),0.5),mul(add(m.points[0],m.points[2]),0.5)) }
            let d=dimensions(&with_points(&m,p),false).unwrap(); close(d.primary,original.primary*2.0);
            close(d.secondary.unwrap(),original.secondary.unwrap()*0.5);
        }
    }
    #[test]
    fn parser_accepts_decimal_comma_and_rejects_nonpositive_or_nonfinite() {
        close(parse_value(" 12,5 ").unwrap(),12.5); close(parse_value("1.25").unwrap(),1.25);
        for text in ["NaN","inf","-inf","0","-2","","1,2,3"] { assert!(parse_value(text).is_err()); }
    }
    #[test]
    fn rejects_unsupported_degenerate_nonrectangular_and_invalid_requests() {
        for m in [measurement(Tool::Angle,&[[0.0,0.0],[1.0,0.0],[1.0,1.0]]),
            measurement(Tool::Distance,&[[1.0,1.0],[1.0,1.0]]),
            measurement(Tool::Rectangle,&[[0.0,0.0],[0.0,1.0]])] { assert!(dimensions(&m,false).is_err()); }
        let mut polygon=measurement(Tool::Polygon,&[[0.0,0.0],[4.0,0.0],[3.0,3.0],[0.0,2.0]]);
        polygon.drawing_variant=Some("Rectangle orienté".into()); assert!(dimensions(&polygon,false).is_err());
        let m=measurement(Tool::Distance,&[[0.0,0.0],[1.0,0.0]]);
        for invalid in [0.0,-1.0,f64::NAN,f64::INFINITY] { assert!(resize(&m,invalid,None,Anchor::First,false).is_err()); }
        assert!(resize(&m,2.0,Some(1.0),Anchor::First,false).is_err());
        let mut nonfinite=m.clone(); nonfinite.points[0].x=f64::NAN; assert!(dimensions(&nonfinite,false).is_err());
        let rect=measurement(Tool::Rectangle,&[[0.0,0.0],[4.0,5.0]]);
        assert!(resize(&rect,2.0,None,Anchor::First,false).is_err());
        assert!(resize(&rect,2.0,Some(f64::INFINITY),Anchor::First,false).is_err());
    }
}

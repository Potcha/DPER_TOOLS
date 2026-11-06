//! Screen-space primitives for measurement overlays. These do not alter measurement data.
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

pub const CYAN: Color32 = Color32::from_rgb(0, 216, 235);
pub const YELLOW: Color32 = Color32::from_rgb(255, 221, 0);
pub const CONSTRUCTION: Color32 = Color32::from_rgb(255, 174, 66);
pub const STROKE_WIDTH: f32 = 3.0;
pub const SELECTED_STROKE_WIDTH: f32 = 4.0;
pub const GUIDE_WIDTH: f32 = 2.0;
const OUTLINE: Color32 = Color32::from_rgba_unmultiplied_const(3, 12, 22, 220);

fn finite_point(p: Pos2) -> bool { p.x.is_finite() && p.y.is_finite() }
fn unit_delta(a: Pos2, b: Pos2) -> Option<(Vec2, f32)> {
    if !finite_point(a) || !finite_point(b) { return None; }
    let delta = b - a;
    let length = delta.length();
    if !length.is_finite() || length <= 0.001 { return None; }
    Some((delta / length, length))
}

pub fn segment(painter: &Painter, a: Pos2, b: Pos2, color: Color32, width: f32) {
    if unit_delta(a, b).is_some() && width.is_finite() && width > 0.0 {
        painter.line_segment([a, b], Stroke::new(width + 1.8, OUTLINE));
        painter.line_segment([a, b], Stroke::new(width, color));
    }
}

pub fn dashed_segment(painter: &Painter, a: Pos2, b: Pos2, color: Color32, width: f32, dash: f32, gap: f32) {
    let Some((direction, length)) = unit_delta(a, b) else { return; };
    if !width.is_finite() || width <= 0.0 || !dash.is_finite() || dash <= 0.0 || !gap.is_finite() || gap < 0.0 { return; }
    let period = dash + gap;
    if !period.is_finite() || period <= 0.0 { return; }
    // Limit geometry even for extreme zoom/coordinates; distribute the bounded dashes across the full segment.
    let step = period.max(length / 4096.0);
    let count = ((length / step).ceil() as usize).min(4096);
    for i in 0..count {
        let start = i as f32 * step;
        let end = (start + dash).min(length);
        if end > start { segment(painter, a + direction * start, a + direction * end, color, width.max(GUIDE_WIDTH)); }
    }
}

/// `pointing` is the direction from the base toward the apex at `tip`.
pub fn arrowhead(painter: &Painter, tip: Pos2, pointing: Vec2, color: Color32, length: f32) {
    if !finite_point(tip) || !pointing.x.is_finite() || !pointing.y.is_finite()
        || !length.is_finite() || length <= 0.0 { return; }
    let magnitude = pointing.length();
    if !magnitude.is_finite() || magnitude <= 0.001 { return; }
    let direction = pointing / magnitude;
    let normal = Vec2::new(-direction.y, direction.x);
    let base = tip - direction * length;
    let corners = vec![tip, base + normal * (length * 0.46), base - normal * (length * 0.46)];
    if corners.iter().all(|&point| finite_point(point)) {
        painter.add(egui::Shape::convex_polygon(corners, color, Stroke::new(1.2, OUTLINE)));
    }
}

pub fn dimension(painter: &Painter, a: Pos2, b: Pos2, color: Color32, width: f32) {
    let Some((direction, length)) = unit_delta(a, b) else { return; };
    if !width.is_finite() || width <= 0.0 { return; }
    segment(painter, a, b, color, width.max(STROKE_WIDTH));
    let head_length = 14.0_f32.min(length * 0.35);
    arrowhead(painter, a, -direction, color, head_length);
    arrowhead(painter, b, direction, color, head_length);
}

pub fn extension(painter: &Painter, anchor: Pos2, dimension_point: Pos2, color: Color32) {
    let Some((direction, length)) = unit_delta(anchor, dimension_point) else { return; };
    let end = dimension_point + direction * 6.0_f32.min(length * 0.25);
    dashed_segment(painter, anchor, end, color, GUIDE_WIDTH, 5.0, 4.0);
}

pub fn center_mark(painter: &Painter, center: Pos2, rx: f32, ry: f32) {
    if !finite_point(center) || !rx.is_finite() || !ry.is_finite() || rx <= 0.0 || ry <= 0.0 { return; }
    let x = Vec2::new(rx, 0.0);
    let y = Vec2::new(0.0, ry);
    dashed_segment(painter, center - x, center + x, CONSTRUCTION, GUIDE_WIDTH, 7.0, 5.0);
    dashed_segment(painter, center - y, center + y, CONSTRUCTION, GUIDE_WIDTH, 7.0, 5.0);
    let red = Color32::from_rgb(239, 55, 55);
    let cross = 6.0_f32.min(rx).min(ry);
    segment(painter, center - Vec2::new(cross, 0.0), center + Vec2::new(cross, 0.0), red, STROKE_WIDTH);
    segment(painter, center - Vec2::new(0.0, cross), center + Vec2::new(0.0, cross), red, STROKE_WIDTH);
    let tick = 4.0_f32.min(rx).min(ry);
    for point in [center - x, center + x] {
        segment(painter, point - Vec2::new(0.0, tick), point + Vec2::new(0.0, tick), CONSTRUCTION, GUIDE_WIDTH);
    }
    for point in [center - y, center + y] {
        segment(painter, point - Vec2::new(tick, 0.0), point + Vec2::new(tick, 0.0), CONSTRUCTION, GUIDE_WIDTH);
    }
}

pub struct AngleGeometry {
    pub points: Vec<Pos2>,
    pub label_anchor: Pos2,
}

pub fn angle_geometry(a: Pos2, vertex: Pos2, b: Pos2) -> Option<AngleGeometry> {
    let (first, first_length) = unit_delta(vertex, a)?;
    let (second, second_length) = unit_delta(vertex, b)?;
    let start = first.y.atan2(first.x);
    let end = second.y.atan2(second.x);
    let delta = (end - start + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    if !delta.is_finite() || delta.abs() <= 0.0001 { return None; }
    let radius = 90.0_f32.min(first_length.min(second_length) * 0.35);
    if !radius.is_finite() || radius <= 0.001 { return None; }
    let points: Vec<_> = (0..=48).map(|step| {
        let angle = start + delta * step as f32 / 48.0;
        vertex + Vec2::new(angle.cos(), angle.sin()) * radius
    }).collect();
    let mid = start + delta * 0.5;
    let label_anchor = vertex + Vec2::new(mid.cos(), mid.sin()) * radius;
    if !finite_point(label_anchor) || !points.iter().all(|&point| finite_point(point)) { return None; }
    Some(AngleGeometry { points, label_anchor })
}

pub fn angle_arc(painter: &Painter, a: Pos2, vertex: Pos2, b: Pos2) -> Option<Pos2> {
    let geometry = angle_geometry(a, vertex, b)?;
    // Preserve dash phase across the sampled polyline.
    let mut distance = 0.0_f32;
    for pair in geometry.points.windows(2) {
        let Some((direction, length)) = unit_delta(pair[0], pair[1]) else { continue; };
        let mut offset = 0.0;
        while offset < length {
            let phase = (distance + offset).rem_euclid(8.0);
            let remaining = if phase < 4.0 { 4.0 - phase } else { 8.0 - phase };
            let next = (offset + remaining).min(length);
            if phase < 4.0 { segment(painter, pair[0] + direction * offset, pair[0] + direction * next, YELLOW, STROKE_WIDTH); }
            if next <= offset { break; }
            offset = next;
        }
        distance += length;
    }
    for point in [geometry.points[0], *geometry.points.last()?] {
        if let Some((radial, _)) = unit_delta(vertex, point) {
            segment(painter, point - radial * 3.5, point + radial * 3.5, YELLOW, STROKE_WIDTH);
        }
    }
    Some(geometry.label_anchor)
}

pub fn leader(painter: &Painter, anchor: Pos2, card_rect: Rect, color: Color32) {
    if !finite_point(anchor) || !finite_point(card_rect.min) || !finite_point(card_rect.max)
        || !card_rect.is_positive() || card_rect.contains(anchor) { return; }
    let nearest = Pos2::new(anchor.x.clamp(card_rect.left(), card_rect.right()),
                            anchor.y.clamp(card_rect.top(), card_rect.bottom()));
    segment(painter, anchor, nearest, color, GUIDE_WIDTH);
}

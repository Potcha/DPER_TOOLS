//! Intersection of native-image measurement traces with a selection rectangle.
use crate::geometry::{self, Measurement, Point, Tool};

#[derive(Clone, Copy)]
struct Bounds { left: f64, top: f64, right: f64, bottom: f64 }
impl Bounds {
    fn new(raw: [f64; 4]) -> Option<Self> {
        if !raw.iter().all(|v| v.is_finite()) { return None; }
        let [x0, y0, x1, y1] = raw;
        Some(Self { left: x0.min(x1), top: y0.min(y1), right: x0.max(x1), bottom: y0.max(y1) })
    }
    fn contains(self, p: Point) -> bool {
        finite(p) && p.x >= self.left && p.x <= self.right && p.y >= self.top && p.y <= self.bottom
    }
    fn segment(self, a: Point, b: Point) -> bool {
        if !finite(a) || !finite(b) { return false; }
        if self.contains(a) || self.contains(b) { return true; }
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        if !dx.is_finite() || !dy.is_finite() { return false; }
        let mut lo: f64 = 0.0;
        let mut hi: f64 = 1.0;
        for (p, q) in [(-dx, a.x - self.left), (dx, self.right - a.x),
                       (-dy, a.y - self.top), (dy, self.bottom - a.y)] {
            if !q.is_finite() { return false; }
            if p == 0.0 {
                if q < 0.0 { return false; }
            } else {
                let t = q / p;
                if p < 0.0 { lo = lo.max(t); } else { hi = hi.min(t); }
                if lo > hi { return false; }
            }
        }
        true
    }
    fn polyline(self, points: &[Point], closed: bool) -> bool {
        points.windows(2).any(|pair| self.segment(pair[0], pair[1]))
            || (closed && points.len() >= 2 && self.segment(points[points.len()-1], points[0]))
    }
    fn circle(self, center: Point, radius: f64) -> bool {
        if !finite(center) || !radius.is_finite() || radius <= 0.0 { return false; }
        let nearest_x = center.x.clamp(self.left, self.right);
        let nearest_y = center.y.clamp(self.top, self.bottom);
        let nearest = (nearest_x - center.x).hypot(nearest_y - center.y);
        let farthest_x = (self.left - center.x).abs().max((self.right - center.x).abs());
        let farthest_y = (self.top - center.y).abs().max((self.bottom - center.y).abs());
        let farthest = farthest_x.hypot(farthest_y);
        nearest <= radius && radius <= farthest
    }
}
fn finite(p: Point) -> bool { p.x.is_finite() && p.y.is_finite() }
fn midpoint(a: Point, b: Point) -> Point { Point { x: a.x * 0.5 + b.x * 0.5, y: a.y * 0.5 + b.y * 0.5 } }

/// Intersect a segment with bounds `[x0, y0, x1, y1]` in either drag direction.
pub fn segment_intersects_selection(a: Point, b: Point, bounds: [f64; 4]) -> bool {
    Bounds::new(bounds).is_some_and(|area| area.segment(a, b))
}

/// Bounds are `[x0, y0, x1, y1]` in native image pixels, in either drag direction.
/// Intersects the drawn trace (including dimension construction lines), not the filled area.
pub fn intersects_selection(m: &Measurement, area_native_bounds: [f64; 4]) -> bool {
    let Some(area) = Bounds::new(area_native_bounds) else { return false; };
    let points = &m.points;
    if points.is_empty() || points.iter().copied().any(|p| !finite(p)) { return false; }
    match m.kind {
        Tool::Coordinate | Tool::ObjectCount => points.iter().copied().any(|p| area.contains(p)),
        Tool::Distance | Tool::Polyline | Tool::Angle => area.polyline(points, false),
        Tool::Polygon => area.polyline(points, true),
        Tool::Rectangle if points.len() >= 2 => {
            let a = points[0]; let b = points[1];
            let corners = [a, Point { x: b.x, y: a.y }, b, Point { x: a.x, y: b.y }];
            area.polyline(&corners, true)
        }
        Tool::Circle if points.len() >= 2 => {
            let center = points[0]; let edge = points[1];
            let opposite = Point { x: 2.0 * center.x - edge.x, y: 2.0 * center.y - edge.y };
            area.circle(center, center.dist(edge)) || area.segment(opposite, edge)
        }
        Tool::PointLineDistance if points.len() >= 3 => {
            let (point, a, b) = (points[0], points[1], points[2]);
            let dx = b.x - a.x; let dy = b.y - a.y;
            let length_sq = dx * dx + dy * dy;
            if !length_sq.is_finite() || length_sq <= f64::EPSILON { return area.segment(a,b); }
            let t = ((point.x-a.x)*dx + (point.y-a.y)*dy) / length_sq;
            let projection = Point { x: a.x+t*dx, y: a.y+t*dy };
            area.segment(a,b) || area.segment(point,projection)
        }
        Tool::ParallelDistance if points.len() >= 4 => {
            let (a,b,c,d) = (points[0],points[1],points[2],points[3]);
            if area.segment(a,b) || area.segment(c,d) { return true; }
            let dx=b.x-a.x; let dy=b.y-a.y; let length=dx.hypot(dy);
            if !length.is_finite() || length <= f64::EPSILON { return false; }
            let normal=Point{x:-dy/length,y:dx/length};
            let first=midpoint(a,b); let second=midpoint(c,d);
            let separation=(second.x-first.x)*normal.x+(second.y-first.y)*normal.y;
            let projection=Point{x:first.x+normal.x*separation,y:first.y+normal.y*separation};
            area.segment(first,projection)
        }
        Tool::CircleFit | Tool::Ellipse | Tool::Arc => {
            let Some(curve) = geometry::sampled_curve(m.kind, points, 96) else { return false; };
            if area.polyline(&curve, false) { return true; }
            // The displayed diameter/axes are also part of the drawn measurement.
            if m.kind == Tool::CircleFit {
                if let Some(fit) = geometry::fit_circle(points) {
                    let dx=points[0].x-fit.center.x; let dy=points[0].y-fit.center.y;
                    let length=dx.hypot(dy);
                    let (ux,uy)=if length>1e-9 {(dx/length,dy/length)} else {(std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2)};
                    let a=Point{x:fit.center.x-ux*fit.radius,y:fit.center.y-uy*fit.radius};
                    let b=Point{x:fit.center.x+ux*fit.radius,y:fit.center.y+uy*fit.radius};
                    return area.segment(a,b);
                }
            } else if m.kind == Tool::Ellipse {
                if let Some(ellipse) = geometry::ellipse_geometry(points) {
                    return [ellipse.first_axis,ellipse.second_axis].into_iter().any(|axis| {
                        area.segment(Point{x:ellipse.center.x-axis.x,y:ellipse.center.y-axis.y},
                                     Point{x:ellipse.center.x+axis.x,y:ellipse.center.y+axis.y})
                    });
                }
            }
            false
        }
        _ => false,
    }
}


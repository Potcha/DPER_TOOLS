//! Directed, bounded contour inspection in native source-image pixels.
//!
//! Expected geometry only locates search profiles. Actual accepted image edges
//! determine the fitted support line or radius. No physical scale is inferred.
//! Line/arc endpoints are the observed sample span, not verified part endpoints.

use crate::geometry::{fit_circle, CircleFit, Point};
use image::RgbImage;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub enum InspectionGeometry {
    Line {
        start: Point,
        end: Point,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    Arc {
        center: Point,
        radius: f64,
        start_rad: f64,
        sweep_rad: f64,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub enum EdgePolarity {
    Any,
    /// Intensity increases along the search normal.
    Rising,
    Falling,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VisionOptions {
    pub sample_count: usize,
    pub search_radius_px: f64,
    /// Central intensity derivative in grayscale levels per pixel (not Sobel units).
    pub minimum_gradient: f64,
    pub polarity: EdgePolarity,
    pub minimum_coverage: f64,
    pub outlier_threshold_px: f64,
    pub maximum_rms_px: f64,
    pub minimum_arc_sweep_rad: f64,
}

impl Default for VisionOptions {
    fn default() -> Self {
        Self {
            sample_count: 96,
            search_radius_px: 8.0,
            minimum_gradient: 12.0,
            polarity: EdgePolarity::Any,
            minimum_coverage: 0.7,
            outlier_threshold_px: 1.5,
            maximum_rms_px: 0.8,
            minimum_arc_sweep_rad: PI / 3.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VisionEvidence {
    pub accepted_points: Vec<Point>,
    /// Expected positions for missing/ambiguous profiles; detected fit outliers.
    pub rejected_points: Vec<Point>,
    pub sample_count: usize,
    pub rms_residual_px: f64,
    pub max_residual_px: f64,
    pub coverage_fraction: f64,
    /// Always false for line/arc support inspection: no endpoint detector ran.
    pub endpoint_extent_verified: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InspectionResult {
    pub geometry: InspectionGeometry,
    pub evidence: VisionEvidence,
}

#[derive(Clone, Copy)]
struct EdgeSample {
    index: usize,
    point: Point,
}

enum ProfileEdge {
    Found(Point),
    Missing,
    Ambiguous,
}

/// Inspect a line, circle or arc near a supplied pixel-coordinate localization.
///
/// Ambiguous profiles are excluded, just like missing image support; nominal
/// geometry never supplies a replacement measurement. Errors explicitly with
/// insufficient unambiguous coverage, weak edges, unstable arc
/// support or excessive fit residuals. RMS is a fit diagnostic, not a calibrated
/// measurement uncertainty. Work is bounded by 256 profiles and a 32px search.
pub fn inspect_geometry(
    image: &RgbImage,
    expected: &InspectionGeometry,
    options: &VisionOptions,
) -> Result<InspectionResult, String> {
    validate(image, expected, options)?;
    let mut detected = Vec::new();
    let mut rejected = Vec::new();
    let mut ambiguous_profiles = 0;
    for index in 0..options.sample_count {
        let (point, normal) = profile_position(expected, index, options.sample_count);
        match directed_edge(image, point, normal, options) {
            ProfileEdge::Found(point) => detected.push(EdgeSample { index, point }),
            ProfileEdge::Missing => rejected.push(point),
            ProfileEdge::Ambiguous => {
                ambiguous_profiles += 1;
                rejected.push(point);
            }
        }
    }
    let minimum_points = match expected {
        InspectionGeometry::Line { .. } => 8,
        InspectionGeometry::Circle { .. } => 16,
        InspectionGeometry::Arc { .. } => 12,
    };
    let required = ((options.sample_count as f64 * options.minimum_coverage).ceil() as usize)
        .max(minimum_points);
    if detected.len() < required {
        if ambiguous_profiles > 0 {
            return Err(format!(
                "Contour ambigu : {} profils ambigus, {} profils non ambigus détectés sur {} ({} requis).",
                ambiguous_profiles, detected.len(), options.sample_count, required
            ));
        }
        return Err(format!(
            "Contour insuffisant : {} profils détectés sur {} ({} requis).",
            detected.len(),
            options.sample_count,
            required
        ));
    }

    let (geometry, inliers, residuals) = match expected {
        InspectionGeometry::Line { start, end } => {
            let (line, inliers) = robust_line(&detected, options.outlier_threshold_px)
                .ok_or("Impossible d'ajuster une droite stable aux bords détectés.")?;
            let mut direction = line.direction;
            if direction.x * (end.x - start.x) + direction.y * (end.y - start.y) < 0.0 {
                direction.x = -direction.x;
                direction.y = -direction.y;
            }
            let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
            for &index in &inliers {
                let point = detected[index].point;
                let along = (point.x - line.origin.x) * direction.x
                    + (point.y - line.origin.y) * direction.y;
                low = low.min(along);
                high = high.max(along);
            }
            if high - low < start.dist(*end) * options.minimum_coverage {
                return Err("Les bords de droite sont trop concentrés sur une petite zone.".into());
            }
            let measured = InspectionGeometry::Line {
                start: Point {
                    x: line.origin.x + low * direction.x,
                    y: line.origin.y + low * direction.y,
                },
                end: Point {
                    x: line.origin.x + high * direction.x,
                    y: line.origin.y + high * direction.y,
                },
            };
            let residuals = inliers
                .iter()
                .map(|&index| line.residual(detected[index].point))
                .collect();
            (measured, inliers, residuals)
        }
        InspectionGeometry::Circle { center, radius }
        | InspectionGeometry::Arc { center, radius, .. } => {
            let (circle, inliers) = robust_circle(&detected, options.outlier_threshold_px)
                .ok_or("Impossible d'ajuster un cercle stable aux bords détectés.")?;
            // Prevent an almost-straight arc from producing a huge, plausible
            // algebraic circle outside the directed search neighborhood.
            if circle.center.dist(*center) > 2.0 * options.search_radius_px
                || (circle.radius - radius).abs() > 2.0 * options.search_radius_px
                || circle.radius < 2.0
            {
                return Err("Le cercle ajusté est instable ou sort de la zone recherchée.".into());
            }
            let measured = match expected {
                InspectionGeometry::Arc { sweep_rad, .. } => {
                    let first = detected[*inliers.first().ok_or("Arc vide.")?];
                    let last = detected[*inliers.last().ok_or("Arc vide.")?];
                    let observed_fraction =
                        (last.index - first.index) as f64 / (options.sample_count - 1) as f64;
                    if observed_fraction * sweep_rad.abs() < options.minimum_arc_sweep_rad - 1e-9 {
                        return Err("L'arc observé est trop court pour un rayon fiable.".into());
                    }
                    let start_rad =
                        (first.point.y - circle.center.y).atan2(first.point.x - circle.center.x);
                    let end_rad =
                        (last.point.y - circle.center.y).atan2(last.point.x - circle.center.x);
                    let measured_sweep = if *sweep_rad > 0.0 {
                        (end_rad - start_rad).rem_euclid(TAU)
                    } else {
                        -(start_rad - end_rad).rem_euclid(TAU)
                    };
                    if measured_sweep.abs() < options.minimum_arc_sweep_rad * 0.9 {
                        return Err(
                            "La courbure observée ne permet pas de confirmer le rayon de l'arc."
                                .into(),
                        );
                    }
                    InspectionGeometry::Arc {
                        center: circle.center,
                        radius: circle.radius,
                        start_rad,
                        sweep_rad: measured_sweep,
                    }
                }
                _ => {
                    let mut sectors = [false; 8];
                    for &index in &inliers {
                        sectors[(detected[index].index * 8 / options.sample_count).min(7)] = true;
                    }
                    if sectors.iter().filter(|&&present| present).count() < 6 {
                        return Err("Le cercle est observé sur trop peu de secteurs.".into());
                    }
                    InspectionGeometry::Circle {
                        center: circle.center,
                        radius: circle.radius,
                    }
                }
            };
            let residuals = inliers
                .iter()
                .map(|&index| (detected[index].point.dist(circle.center) - circle.radius).abs())
                .collect();
            (measured, inliers, residuals)
        }
    };
    let residuals: Vec<f64> = residuals;
    if inliers.len() < required {
        return Err(format!(
            "Contour refusé après rejet des points aberrants : {} profils cohérents, {} requis ; {} profils ambigus exclus.",
            inliers.len(),
            required,
            ambiguous_profiles
        ));
    }
    let rms_residual_px =
        (residuals.iter().map(|value| value * value).sum::<f64>() / residuals.len() as f64).sqrt();
    let max_residual_px = residuals.iter().copied().fold(0.0, f64::max);
    if !rms_residual_px.is_finite() || rms_residual_px > options.maximum_rms_px {
        return Err(format!(
            "Contour trop irrégulier : résidu RMS {rms_residual_px:.3} px."
        ));
    }
    let mut accepted_points = Vec::with_capacity(inliers.len());
    for (index, sample) in detected.iter().enumerate() {
        if inliers.binary_search(&index).is_ok() {
            accepted_points.push(sample.point);
        } else {
            rejected.push(sample.point);
        }
    }
    let coverage_fraction = accepted_points.len() as f64 / options.sample_count as f64;
    Ok(InspectionResult {
        geometry,
        evidence: VisionEvidence {
            accepted_points,
            rejected_points: rejected,
            sample_count: options.sample_count,
            rms_residual_px,
            max_residual_px,
            coverage_fraction,
            endpoint_extent_verified: false,
        },
    })
}

fn finite_point(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.x.abs() <= 1e9 && point.y.abs() <= 1e9
}

fn validate(
    image: &RgbImage,
    geometry: &InspectionGeometry,
    options: &VisionOptions,
) -> Result<(), String> {
    if image.width() < 5
        || image.height() < 5
        || image.width() as u64 * image.height() as u64 > 30_000_000
    {
        return Err("Image trop petite ou supérieure au budget de 30 millions de pixels.".into());
    }
    if !(16..=256).contains(&options.sample_count)
        || !options.search_radius_px.is_finite()
        || !(1.0..=32.0).contains(&options.search_radius_px)
        || !options.minimum_gradient.is_finite()
        || !(0.1..=255.0).contains(&options.minimum_gradient)
        || !options.minimum_coverage.is_finite()
        || !(0.5..=1.0).contains(&options.minimum_coverage)
        || !options.outlier_threshold_px.is_finite()
        || !(0.05..=5.0).contains(&options.outlier_threshold_px)
        || !options.maximum_rms_px.is_finite()
        || options.maximum_rms_px <= 0.0
        || options.maximum_rms_px > options.outlier_threshold_px
        || !options.minimum_arc_sweep_rad.is_finite()
        || !(PI / 3.0..=PI).contains(&options.minimum_arc_sweep_rad)
    {
        return Err("Options de recherche de contour invalides.".into());
    }
    match geometry {
        InspectionGeometry::Line { start, end } => {
            if !finite_point(*start) || !finite_point(*end) || start.dist(*end) < 8.0 {
                return Err("La droite attendue doit avoir deux points distincts et une longueur d'au moins 8 px.".into());
            }
        }
        InspectionGeometry::Circle { center, radius }
        | InspectionGeometry::Arc { center, radius, .. } => {
            if !finite_point(*center) || !radius.is_finite() || *radius < 3.0 || *radius > 1e9 {
                return Err("Centre ou rayon attendu invalide.".into());
            }
        }
    }
    if let InspectionGeometry::Arc {
        start_rad,
        sweep_rad,
        ..
    } = geometry
    {
        if !start_rad.is_finite()
            || !sweep_rad.is_finite()
            || sweep_rad.abs() < options.minimum_arc_sweep_rad
            || sweep_rad.abs() >= TAU - 1e-6
        {
            return Err("L'arc attendu doit couvrir au moins 60 degrés et moins d'un tour.".into());
        }
    }
    Ok(())
}

fn profile_position(geometry: &InspectionGeometry, index: usize, count: usize) -> (Point, Point) {
    match *geometry {
        InspectionGeometry::Line { start, end } => {
            let t = index as f64 / (count - 1) as f64;
            let length = start.dist(end);
            (
                Point {
                    x: start.x + t * (end.x - start.x),
                    y: start.y + t * (end.y - start.y),
                },
                Point {
                    x: -(end.y - start.y) / length,
                    y: (end.x - start.x) / length,
                },
            )
        }
        InspectionGeometry::Circle { center, radius } => {
            let angle = index as f64 * TAU / count as f64;
            let normal = Point {
                x: angle.cos(),
                y: angle.sin(),
            };
            (
                Point {
                    x: center.x + radius * normal.x,
                    y: center.y + radius * normal.y,
                },
                normal,
            )
        }
        InspectionGeometry::Arc {
            center,
            radius,
            start_rad,
            sweep_rad,
        } => {
            let angle = start_rad + index as f64 * sweep_rad / (count - 1) as f64;
            let normal = Point {
                x: angle.cos(),
                y: angle.sin(),
            };
            (
                Point {
                    x: center.x + radius * normal.x,
                    y: center.y + radius * normal.y,
                },
                normal,
            )
        }
    }
}

fn gray(image: &RgbImage, point: Point) -> Option<f64> {
    if !finite_point(point)
        || point.x < 0.0
        || point.y < 0.0
        || point.x > (image.width() - 1) as f64
        || point.y > (image.height() - 1) as f64
    {
        return None;
    }
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
    Some(
        (intensity(x, y) * (1.0 - fx) + intensity(next_x, y) * fx) * (1.0 - fy)
            + (intensity(x, next_y) * (1.0 - fx) + intensity(next_x, next_y) * fx) * fy,
    )
}

fn directed_edge(
    image: &RgbImage,
    expected: Point,
    normal: Point,
    options: &VisionOptions,
) -> ProfileEdge {
    let tangent = Point {
        x: -normal.y,
        y: normal.x,
    };
    let gradient = |offset: f64| -> Option<f64> {
        let mut derivative = 0.0;
        for along in [-0.6, 0.0, 0.6] {
            let sample = |normal_offset: f64| {
                gray(
                    image,
                    Point {
                        x: expected.x + normal.x * (offset + normal_offset) + tangent.x * along,
                        y: expected.y + normal.y * (offset + normal_offset) + tangent.y * along,
                    },
                )
            };
            derivative += (sample(0.75)? - sample(-0.75)?) / 1.5;
        }
        Some(derivative / 3.0)
    };
    let step = 0.5;
    let half_steps = (options.search_radius_px / step).floor() as i32;
    let mut peaks: Vec<(f64, f64)> = Vec::new();
    for index in -half_steps..=half_steps {
        let offset = index as f64 * step;
        let Some(current) = gradient(offset) else {
            continue;
        };
        if current.abs() < options.minimum_gradient {
            continue;
        }
        if (options.polarity == EdgePolarity::Rising && current <= 0.0)
            || (options.polarity == EdgePolarity::Falling && current >= 0.0)
        {
            continue;
        }
        let Some(before) = gradient(offset - step).map(f64::abs) else {
            continue;
        };
        let Some(after) = gradient(offset + step).map(f64::abs) else {
            continue;
        };
        let strength = current.abs();
        if strength < before || strength < after || (strength == before && strength == after) {
            continue;
        }
        let curvature = before - 2.0 * strength + after;
        let correction = if curvature < -1e-9 {
            (0.5 * (before - after) / curvature).clamp(-0.75, 0.75) * step
        } else {
            0.0
        };
        let refined_offset = offset + correction;
        if refined_offset.abs() > options.search_radius_px {
            continue;
        }
        // Adjacent samples on the same peak can both pass the local-maximum
        // check (e.g. a two-sample plateau). They are one edge, not two choices.
        if let Some(previous) = peaks.last_mut() {
            if (refined_offset - previous.0).abs() <= step + 1e-6 {
                if strength > previous.1 {
                    *previous = (refined_offset, strength);
                }
                continue;
            }
        }
        peaks.push((refined_offset, strength));
    }
    peaks.sort_by(|left, right| left.0.abs().total_cmp(&right.0.abs()));
    let Some(&(offset, _)) = peaks.first() else {
        return ProfileEdge::Missing;
    };
    // Fixed conservative ambiguity policy, independent of client tolerances:
    // after polarity filtering, distinct threshold-passing peaks within 1 px
    // of the same distance to the localization are equally plausible. One
    // pixel allows for rasterization and the 0.5 px derivative sampling; it is
    // not a calibrated measurement uncertainty. Peak
    // strength alone cannot identify the intended physical contour (glare may
    // be stronger). A clearly closer peak still wins over a distant neighbor.
    // This is a localization decision, not a photometric defect classifier or
    // a guarantee that a single visible edge is the true part boundary.
    if peaks
        .iter()
        .skip(1)
        .any(|&(other, _)| other.abs() - offset.abs() <= 1.0 + 1e-6)
    {
        return ProfileEdge::Ambiguous;
    }
    ProfileEdge::Found(Point {
        x: expected.x + normal.x * offset,
        y: expected.y + normal.y * offset,
    })
}

#[derive(Clone, Copy)]
struct SupportLine {
    origin: Point,
    direction: Point,
}

impl SupportLine {
    fn residual(self, point: Point) -> f64 {
        ((point.x - self.origin.x) * self.direction.y
            - (point.y - self.origin.y) * self.direction.x)
            .abs()
    }
}

fn line_from_points(first: Point, second: Point) -> Option<SupportLine> {
    let length = first.dist(second);
    if length < 1e-6 {
        return None;
    }
    Some(SupportLine {
        origin: first,
        direction: Point {
            x: (second.x - first.x) / length,
            y: (second.y - first.y) / length,
        },
    })
}

fn fit_line(samples: &[EdgeSample], indices: &[usize]) -> Option<SupportLine> {
    if indices.len() < 2 {
        return None;
    }
    let count = indices.len() as f64;
    let origin = Point {
        x: indices
            .iter()
            .map(|&index| samples[index].point.x / count)
            .sum(),
        y: indices
            .iter()
            .map(|&index| samples[index].point.y / count)
            .sum(),
    };
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for &index in indices {
        let x = samples[index].point.x - origin.x;
        let y = samples[index].point.y - origin.y;
        xx += x * x;
        yy += y * y;
        xy += x * y;
    }
    if xx + yy < 1e-9 {
        return None;
    }
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
    Some(SupportLine {
        origin,
        direction: Point {
            x: angle.cos(),
            y: angle.sin(),
        },
    })
}

fn select_inliers(
    samples: &[EdgeSample],
    threshold: f64,
    residual: impl Fn(Point) -> f64,
) -> (Vec<usize>, f64) {
    let mut indices = Vec::new();
    let mut cost = 0.0;
    for (index, sample) in samples.iter().enumerate() {
        let error = residual(sample.point);
        if error.is_finite() && error <= threshold {
            indices.push(index);
            cost += error * error;
        }
    }
    (indices, cost)
}

fn robust_line(samples: &[EdgeSample], threshold: f64) -> Option<(SupportLine, Vec<usize>)> {
    let mut best = Vec::new();
    let mut best_cost = f64::INFINITY;
    for first in 0..samples.len() {
        for second in first + 1..samples.len() {
            let Some(line) = line_from_points(samples[first].point, samples[second].point) else {
                continue;
            };
            let (inliers, cost) = select_inliers(samples, threshold, |point| line.residual(point));
            if inliers.len() > best.len() || (inliers.len() == best.len() && cost < best_cost) {
                best = inliers;
                best_cost = cost;
            }
        }
    }
    let mut line = fit_line(samples, &best)?;
    for _ in 0..5 {
        let (inliers, _) = select_inliers(samples, threshold, |point| line.residual(point));
        if inliers == best {
            break;
        }
        best = inliers;
        line = fit_line(samples, &best)?;
    }
    let (inliers, _) = select_inliers(samples, threshold, |point| line.residual(point));
    Some((line, inliers))
}

fn robust_circle(samples: &[EdgeSample], threshold: f64) -> Option<(CircleFit, Vec<usize>)> {
    if samples.len() < 3 {
        return None;
    }
    let mut best = Vec::new();
    let mut best_cost = f64::INFINITY;
    let mut state = 0x6a09_e667_f3bc_c909_u64 ^ samples.len() as u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state as usize % samples.len()
    };
    for iteration in 0..512 {
        let indices = if iteration == 0 {
            [0, samples.len() / 2, samples.len() - 1]
        } else {
            [next(), next(), next()]
        };
        if indices[0] == indices[1] || indices[0] == indices[2] || indices[1] == indices[2] {
            continue;
        }
        let points = indices.map(|index| samples[index].point);
        let Some(circle) = fit_circle(&points) else {
            continue;
        };
        let (inliers, cost) = select_inliers(samples, threshold, |point| {
            (point.dist(circle.center) - circle.radius).abs()
        });
        if inliers.len() > best.len() || (inliers.len() == best.len() && cost < best_cost) {
            best = inliers;
            best_cost = cost;
        }
    }
    let mut circle = refine_circle(samples, &best)?;
    for _ in 0..5 {
        let (inliers, _) = select_inliers(samples, threshold, |point| {
            (point.dist(circle.center) - circle.radius).abs()
        });
        if inliers == best {
            break;
        }
        best = inliers;
        circle = refine_circle(samples, &best)?;
    }
    let (inliers, _) = select_inliers(samples, threshold, |point| {
        (point.dist(circle.center) - circle.radius).abs()
    });
    Some((circle, inliers))
}

fn refine_circle(samples: &[EdgeSample], indices: &[usize]) -> Option<CircleFit> {
    let points = indices
        .iter()
        .map(|&index| samples[index].point)
        .collect::<Vec<_>>();
    let mut circle = fit_circle(&points)?;
    // Orthogonal-distance refinement removes algebraic radius bias. The 3x3
    // system is tiny; reject ill-conditioned normal equations explicitly.
    for _ in 0..12 {
        let mut system = [[0.0; 4]; 3];
        for point in &points {
            let dx = circle.center.x - point.x;
            let dy = circle.center.y - point.y;
            let distance = dx.hypot(dy);
            if distance <= 1e-9 {
                return None;
            }
            let residual = distance - circle.radius;
            let jacobian = [dx / distance, dy / distance, -1.0];
            for row in 0..3 {
                for column in 0..3 {
                    system[row][column] += jacobian[row] * jacobian[column];
                }
                system[row][3] -= jacobian[row] * residual;
            }
        }
        let delta = solve_three(system)?;
        circle.center.x += delta[0];
        circle.center.y += delta[1];
        circle.radius += delta[2];
        if !finite_point(circle.center) || !circle.radius.is_finite() || circle.radius <= 0.0 {
            return None;
        }
        if delta.iter().map(|value| value * value).sum::<f64>() < 1e-16 {
            break;
        }
    }
    let residuals = points
        .iter()
        .map(|point| (point.dist(circle.center) - circle.radius).abs())
        .collect::<Vec<_>>();
    circle.rms_radial_error =
        (residuals.iter().map(|error| error * error).sum::<f64>() / points.len() as f64).sqrt();
    circle.max_radial_error = residuals.iter().copied().fold(0.0, f64::max);
    Some(circle)
}

fn solve_three(mut system: [[f64; 4]; 3]) -> Option<[f64; 3]> {
    let scale = system
        .iter()
        .flat_map(|row| row[..3].iter())
        .map(|value| value.abs())
        .fold(0.0, f64::max);
    for pivot in 0..3 {
        let best = (pivot..3).max_by(|&left, &right| {
            system[left][pivot]
                .abs()
                .total_cmp(&system[right][pivot].abs())
        })?;
        if system[best][pivot].abs() < scale * 1e-8 {
            return None;
        }
        system.swap(pivot, best);
        let divisor = system[pivot][pivot];
        for column in pivot..4 {
            system[pivot][column] /= divisor;
        }
        for row in 0..3 {
            if row == pivot {
                continue;
            }
            let factor = system[row][pivot];
            for column in pivot..4 {
                system[row][column] -= factor * system[pivot][column];
            }
        }
    }
    Some([system[0][3], system[1][3], system[2][3]])
}

#[cfg(test)]
#[path = "dper_vision_adversarial_tests.rs"]
mod adversarial_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    // Smooth analytic rasterization exercises subpixel localization without
    // sharing the production edge detector or fitting implementation.
    fn rasterize(intensity: impl Fn(f64, f64) -> f64) -> RgbImage {
        RgbImage::from_fn(256, 256, |x, y| {
            let value = intensity(x as f64, y as f64).round().clamp(0.0, 255.0) as u8;
            Rgb([value; 3])
        })
    }

    fn step(distance: f64) -> f64 {
        0.5 + 0.5 * (distance / 0.7).tanh()
    }

    #[test]
    fn circles_measure_actual_radius_under_translation_scale_and_noise() {
        for (center, radius) in [
            (Point { x: 87.35, y: 91.7 }, 26.4),
            (Point { x: 152.2, y: 139.6 }, 47.8),
            (Point { x: 120.6, y: 112.1 }, 65.3),
        ] {
            let image = rasterize(|x, y| {
                let distance = (x - center.x).hypot(y - center.y) - radius;
                25.0 + 205.0 * step(distance) + ((x * 13.0 + y * 7.0).sin() * 1.5)
            });
            // Deliberately incorrect nominal radius: fitting must recover the
            // image radius rather than preserving this search localization.
            let expected = InspectionGeometry::Circle {
                center: Point {
                    x: center.x + 1.4,
                    y: center.y - 0.9,
                },
                radius: radius + 2.0,
            };
            let result = inspect_geometry(&image, &expected, &VisionOptions::default()).unwrap();
            let InspectionGeometry::Circle {
                center: actual,
                radius: measured,
            } = result.geometry
            else {
                panic!()
            };
            assert!(
                actual.dist(center) < 0.15,
                "center {:?}, expected {:?}",
                actual,
                center
            );
            assert!(
                (measured - radius).abs() < 0.15,
                "radius {measured}, expected {radius}"
            );
            assert!(result.evidence.coverage_fraction > 0.95);
            assert!(result.evidence.rms_residual_px < 0.2);
        }
    }

    #[test]
    fn lines_are_rotation_and_translation_invariant() {
        for angle in [0.0_f64, 0.37, 1.4, 2.7] {
            let center = Point { x: 120.3, y: 131.7 };
            let tangent = Point {
                x: angle.cos(),
                y: angle.sin(),
            };
            let normal = Point {
                x: -tangent.y,
                y: tangent.x,
            };
            let image = rasterize(|x, y| {
                30.0 + 190.0 * step((x - center.x) * normal.x + (y - center.y) * normal.y)
            });
            let expected = InspectionGeometry::Line {
                start: Point {
                    x: center.x - 65.0 * tangent.x + 1.3 * normal.x,
                    y: center.y - 65.0 * tangent.y + 1.3 * normal.y,
                },
                end: Point {
                    x: center.x + 65.0 * tangent.x + 1.3 * normal.x,
                    y: center.y + 65.0 * tangent.y + 1.3 * normal.y,
                },
            };
            let result = inspect_geometry(&image, &expected, &VisionOptions::default()).unwrap();
            let InspectionGeometry::Line { start, end } = result.geometry else {
                panic!()
            };
            for point in [start, end] {
                assert!(
                    ((point.x - center.x) * normal.x + (point.y - center.y) * normal.y).abs()
                        < 0.12
                );
            }
            assert!(result.evidence.rms_residual_px < 0.15);
            assert!(!result.evidence.endpoint_extent_verified);
        }
    }

    #[test]
    fn directed_profiles_prefer_nearby_edge_over_stronger_neighbor() {
        let image = rasterize(|x, _| 15.0 + 60.0 * step(x - 91.35) + 170.0 * step(x - 97.0));
        let geometry = InspectionGeometry::Line {
            start: Point { x: 92.0, y: 30.0 },
            end: Point { x: 92.0, y: 225.0 },
        };
        let result = inspect_geometry(&image, &geometry, &VisionOptions::default()).unwrap();
        let InspectionGeometry::Line { start, end } = result.geometry else {
            panic!()
        };
        assert!((start.x - 91.35).abs() < 0.15);
        assert!((end.x - 91.35).abs() < 0.15);
    }

    #[test]
    fn arcs_recover_radius_with_both_sweep_directions_and_refuse_short_arcs() {
        let center = Point { x: 123.4, y: 118.7 };
        let image =
            rasterize(|x, y| 20.0 + 210.0 * step((x - center.x).hypot(y - center.y) - 53.6));
        for sweep_rad in [2.4, -2.4] {
            let expected = InspectionGeometry::Arc {
                center: Point {
                    x: center.x + 1.0,
                    y: center.y,
                },
                radius: 55.0,
                start_rad: 2.9,
                sweep_rad,
            };
            let result = inspect_geometry(&image, &expected, &VisionOptions::default()).unwrap();
            let InspectionGeometry::Arc {
                radius,
                sweep_rad: actual_sweep,
                ..
            } = result.geometry
            else {
                panic!()
            };
            assert!((radius - 53.6).abs() < 0.25);
            assert_eq!(actual_sweep.signum(), sweep_rad.signum());
        }
        let short = InspectionGeometry::Arc {
            center,
            radius: 53.6,
            start_rad: 0.0,
            sweep_rad: 0.3,
        };
        assert!(inspect_geometry(&image, &short, &VisionOptions::default()).is_err());
    }

    #[test]
    fn robust_fit_rejects_local_contamination_but_missing_contours_fail() {
        let geometry = InspectionGeometry::Line {
            start: Point { x: 90.5, y: 20.0 },
            end: Point { x: 90.5, y: 235.0 },
        };
        let image = rasterize(|x, y| {
            let edge = if (90.0..120.0).contains(&y) {
                96.0
            } else {
                90.3
            };
            20.0 + 210.0 * step(x - edge)
        });
        let result = inspect_geometry(&image, &geometry, &VisionOptions::default()).unwrap();
        assert!(result.evidence.coverage_fraction > 0.8);
        assert!(!result.evidence.rejected_points.is_empty());
        let InspectionGeometry::Line { start, .. } = result.geometry else {
            panic!()
        };
        assert!((start.x - 90.3).abs() < 0.15);
        let partial = rasterize(|x, y| {
            if y > 110.0 {
                30.0
            } else {
                20.0 + 210.0 * step(x - 90.3)
            }
        });
        assert!(inspect_geometry(&partial, &geometry, &VisionOptions::default()).is_err());
        let empty = RgbImage::from_pixel(256, 256, Rgb([80; 3]));
        assert!(inspect_geometry(&empty, &geometry, &VisionOptions::default()).is_err());
    }

    #[test]
    fn irregular_contours_and_invalid_inputs_are_refused() {
        let geometry = InspectionGeometry::Line {
            start: Point { x: 90.5, y: 20.0 },
            end: Point { x: 90.5, y: 235.0 },
        };
        let image = rasterize(|x, y| 20.0 + 210.0 * step(x - 90.3 - 2.0 * (y * 0.2).sin()));
        assert!(inspect_geometry(&image, &geometry, &VisionOptions::default()).is_err());
        let options = VisionOptions {
            maximum_rms_px: f64::NAN,
            ..VisionOptions::default()
        };
        assert!(inspect_geometry(&image, &geometry, &options).is_err());
        let invalid = InspectionGeometry::Circle {
            center: Point {
                x: f64::NAN,
                y: 0.0,
            },
            radius: 4.0,
        };
        assert!(inspect_geometry(&image, &invalid, &VisionOptions::default()).is_err());
    }
}

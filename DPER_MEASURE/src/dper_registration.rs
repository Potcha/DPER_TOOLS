//! Plan-to-image localization from explicit corresponding landmarks.
//!
//! The fitted scale locates a plan in a photograph. It must never replace an
//! independently acquired image calibration when reporting physical dimensions.

use crate::geometry::Point;
use rmcp::schemars;
use serde::{Deserialize, Serialize};

const MAX_PAIRS: usize = 256;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct Correspondence {
    pub plan: Point,
    pub image: Point,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SimilarityTransform {
    pub scale: f64,
    pub rotation_rad: f64,
    pub translation: Point,
}

impl SimilarityTransform {
    pub fn apply(&self, point: Point) -> Point {
        let (sin, cos) = self.rotation_rad.sin_cos();
        Point {
            x: self.scale * (cos * point.x - sin * point.y) + self.translation.x,
            y: self.scale * (sin * point.x + cos * point.y) + self.translation.y,
        }
    }

    pub fn inverse(&self) -> Result<Self, String> {
        if !self.scale.is_finite()
            || self.scale <= 1e-12
            || !self.rotation_rad.is_finite()
            || !finite_point(self.translation)
        {
            return Err("Transformation de recalage non inversible.".into());
        }
        let (sin, cos) = self.rotation_rad.sin_cos();
        let inverse = Self {
            scale: self.scale.recip(),
            rotation_rad: -self.rotation_rad,
            translation: Point {
                x: -(cos * self.translation.x + sin * self.translation.y) / self.scale,
                y: (sin * self.translation.x - cos * self.translation.y) / self.scale,
            },
        };
        if !finite_point(inverse.translation) || !inverse.scale.is_finite() {
            return Err("L'inverse du recalage dépasse la précision numérique disponible.".into());
        }
        Ok(inverse)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegistrationReport {
    pub transform: SimilarityTransform,
    pub inlier_indices: Vec<usize>,
    /// Forward landmark residuals, in native target-image pixels.
    pub rms_error_px: f64,
    pub max_error_px: f64,
}

/// Deterministic two-landmark RANSAC, followed by a least-squares similarity.
///
/// Requires at least three distinct consistent landmarks and 60% support.
/// Collinear landmarks are permissible for a similarity (not a homography).
/// Reflections, anisotropic scaling and perspective are deliberately excluded.
pub fn register_similarity(
    pairs: &[Correspondence],
    threshold_px: f64,
) -> Result<RegistrationReport, String> {
    if !(3..=MAX_PAIRS).contains(&pairs.len()) {
        return Err(format!(
            "Le recalage demande de 3 à {MAX_PAIRS} correspondances explicites."
        ));
    }
    if !threshold_px.is_finite() || threshold_px <= 0.0 || threshold_px > 1e6 {
        return Err("Le seuil de recalage doit être positif et exprimé en pixels.".into());
    }
    if pairs
        .iter()
        .any(|pair| !finite_point(pair.plan) || !finite_point(pair.image))
    {
        return Err("Les correspondances contiennent une coordonnée invalide ou excessive.".into());
    }

    for first in 0..pairs.len() {
        for second in first + 1..pairs.len() {
            if pairs[first].plan.dist(pairs[second].plan) <= 1e-9
                || pairs[first].image.dist(pairs[second].image) <= 1e-9
            {
                return Err(
                    "Chaque repère doit être distinct dans le plan et dans l'image.".into(),
                );
            }
        }
    }

    let mut best_indices = Vec::new();
    let mut best_cost = f64::INFINITY;
    for first in 0..pairs.len() {
        for second in first + 1..pairs.len() {
            let Some(transform) = fit_similarity(pairs, &[first, second]) else {
                continue;
            };
            let (indices, cost) = classify(pairs, &transform, threshold_px);
            if indices.len() > best_indices.len()
                || (indices.len() == best_indices.len() && cost < best_cost)
            {
                best_indices = indices;
                best_cost = cost;
            }
        }
    }
    let minimum_support = (pairs.len() * 3).div_ceil(5).max(3);
    if best_indices.len() < minimum_support {
        return Err("Recalage refusé : trop peu de repères cohérents.".into());
    }
    // Refit and reclassify until membership stabilizes. A final independent
    // classification ensures the returned indices belong to the returned fit.
    let mut transform = fit_similarity(pairs, &best_indices)
        .ok_or("Les repères retenus sont confondus ou dégénérés.")?;
    for _ in 0..8 {
        let (indices, _) = classify(pairs, &transform, threshold_px);
        if indices.len() < minimum_support {
            return Err("Le réajustement du recalage perd trop de repères.".into());
        }
        if indices == best_indices {
            break;
        }
        best_indices = indices;
        transform =
            fit_similarity(pairs, &best_indices).ok_or("Le recalage réajusté est dégénéré.")?;
    }
    let (inlier_indices, squared_error) = classify(pairs, &transform, threshold_px);
    if inlier_indices.len() < minimum_support || distinct_support(pairs, &inlier_indices) < 3 {
        return Err("Le recalage nécessite trois repères distincts et cohérents.".into());
    }
    let rms_error_px = (squared_error / inlier_indices.len() as f64).sqrt();
    let max_error_px = inlier_indices
        .iter()
        .map(|&index| transform.apply(pairs[index].plan).dist(pairs[index].image))
        .fold(0.0, f64::max);
    if !rms_error_px.is_finite() || rms_error_px > threshold_px * 0.8 {
        return Err("Les résidus du recalage sont trop élevés.".into());
    }
    Ok(RegistrationReport {
        transform,
        inlier_indices,
        rms_error_px,
        max_error_px,
    })
}

fn finite_point(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.x.abs() <= 1e12 && point.y.abs() <= 1e12
}

fn distinct_support(pairs: &[Correspondence], indices: &[usize]) -> usize {
    let mut distinct: Vec<Point> = Vec::new();
    for &index in indices {
        if distinct
            .iter()
            .all(|point| point.dist(pairs[index].plan) > 1e-9)
        {
            distinct.push(pairs[index].plan);
        }
    }
    distinct.len()
}

fn classify(
    pairs: &[Correspondence],
    transform: &SimilarityTransform,
    threshold: f64,
) -> (Vec<usize>, f64) {
    let mut indices = Vec::new();
    let mut squared_error = 0.0;
    for (index, pair) in pairs.iter().enumerate() {
        let error = transform.apply(pair.plan).dist(pair.image);
        if error.is_finite() && error <= threshold {
            indices.push(index);
            squared_error += error * error;
        }
    }
    (indices, squared_error)
}

fn fit_similarity(pairs: &[Correspondence], indices: &[usize]) -> Option<SimilarityTransform> {
    if indices.len() < 2 {
        return None;
    }
    let count = indices.len() as f64;
    let first = pairs[*indices.first()?];
    // Reference-relative means preserve small landmark offsets at large origins.
    let mut plan_mean = Point::default();
    let mut image_mean = Point::default();
    for &index in indices {
        plan_mean.x += (pairs[index].plan.x - first.plan.x) / count;
        plan_mean.y += (pairs[index].plan.y - first.plan.y) / count;
        image_mean.x += (pairs[index].image.x - first.image.x) / count;
        image_mean.y += (pairs[index].image.y - first.image.y) / count;
    }
    plan_mean.x += first.plan.x;
    plan_mean.y += first.plan.y;
    image_mean.x += first.image.x;
    image_mean.y += first.image.y;
    let (mut dot, mut cross, mut norm) = (0.0, 0.0, 0.0);
    for &index in indices {
        let p = Point {
            x: pairs[index].plan.x - plan_mean.x,
            y: pairs[index].plan.y - plan_mean.y,
        };
        let q = Point {
            x: pairs[index].image.x - image_mean.x,
            y: pairs[index].image.y - image_mean.y,
        };
        dot += p.x * q.x + p.y * q.y;
        cross += p.x * q.y - p.y * q.x;
        norm += p.x * p.x + p.y * p.y;
    }
    if !norm.is_finite() || norm <= 1e-18 {
        return None;
    }
    let a = dot / norm;
    let b = cross / norm;
    let scale = a.hypot(b);
    if !scale.is_finite() || scale <= 1e-12 {
        return None;
    }
    let translation = Point {
        x: image_mean.x - a * plan_mean.x + b * plan_mean.y,
        y: image_mean.y - b * plan_mean.x - a * plan_mean.y,
    };
    if !finite_point(translation) {
        return None;
    }
    Some(SimilarityTransform {
        scale,
        rotation_rad: b.atan2(a),
        translation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: [Point; 6] = [
        Point { x: 0.0, y: 0.0 },
        Point { x: 30.0, y: 0.0 },
        Point { x: 10.0, y: 20.0 },
        Point { x: -7.0, y: 11.0 },
        Point { x: 25.0, y: -12.0 },
        Point { x: -15.0, y: -6.0 },
    ];

    #[test]
    fn similarity_recovers_translation_rotation_and_scale_and_inverse() {
        for scale in [0.25, 1.0, 4.7] {
            for rotation_rad in [-2.4, 0.0, 0.73, 2.9] {
                let expected = SimilarityTransform {
                    scale,
                    rotation_rad,
                    translation: Point { x: 123.4, y: -67.8 },
                };
                let pairs = PLAN.map(|plan| Correspondence {
                    plan,
                    image: expected.apply(plan),
                });
                let report = register_similarity(&pairs, 0.01).unwrap();
                assert_eq!(report.inlier_indices.len(), PLAN.len());
                assert!(report.rms_error_px < 1e-10);
                let inverse = report.transform.inverse().unwrap();
                for point in PLAN {
                    assert!(report.transform.apply(point).dist(expected.apply(point)) < 1e-9);
                    assert!(inverse.apply(report.transform.apply(point)).dist(point) < 1e-9);
                }
            }
        }
    }

    #[test]
    fn robust_registration_rejects_outliers_and_tolerates_noise() {
        let expected = SimilarityTransform {
            scale: 2.3,
            rotation_rad: 0.62,
            translation: Point { x: 72.0, y: 35.0 },
        };
        let mut pairs = PLAN
            .iter()
            .enumerate()
            .map(|(index, &plan)| {
                let mut image = expected.apply(plan);
                image.x += (index as f64 * 1.7).sin() * 0.04;
                image.y += (index as f64 * 2.1).cos() * 0.04;
                Correspondence { plan, image }
            })
            .collect::<Vec<_>>();
        pairs.push(Correspondence {
            plan: Point { x: 13.0, y: 8.0 },
            image: Point { x: -99.0, y: 400.0 },
        });
        pairs.push(Correspondence {
            plan: Point { x: 21.0, y: -9.0 },
            image: Point {
                x: 999.0,
                y: -100.0,
            },
        });
        let report = register_similarity(&pairs, 0.2).unwrap();
        assert_eq!(report.inlier_indices, (0..6).collect::<Vec<_>>());
        assert!(report.rms_error_px < 0.08);
        for point in PLAN {
            assert!(report.transform.apply(point).dist(expected.apply(point)) < 0.08);
        }
    }

    #[test]
    fn registration_refuses_degenerate_invalid_and_inconsistent_landmarks() {
        let duplicate = Correspondence {
            plan: PLAN[0],
            image: PLAN[1],
        };
        assert!(register_similarity(&[duplicate; 4], 1.0).is_err());
        let mut pairs = PLAN.map(|plan| Correspondence { plan, image: plan });
        assert!(register_similarity(&pairs, f64::NAN).is_err());
        pairs[0].image.x = f64::NAN;
        assert!(register_similarity(&pairs, 1.0).is_err());
        let pairs = PLAN
            .iter()
            .enumerate()
            .map(|(index, &plan)| Correspondence {
                plan,
                image: Point {
                    x: index as f64 * 33.0,
                    y: (index as f64 * 2.3).sin() * 70.0,
                },
            })
            .collect::<Vec<_>>();
        assert!(register_similarity(&pairs, 0.01).is_err());
    }
}


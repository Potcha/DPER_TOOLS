use crate::dper_registration::{Correspondence, fit_similarity};
use crate::geometry::Point;
use crate::imaging::{multiply_homographies, project_homography, solve_homography};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceGeometry {
    SinglePlane,
    MultipleDepths,
    Unknown,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PerspectiveRequest {
    pub anchors: [Correspondence; 4],
    pub controls: [Correspondence; 4],
    pub max_residual_px: f64,
    pub surface_geometry: SurfaceGeometry,
}

#[derive(Serialize)]
pub struct PerspectiveDiagnostic {
    anchors: [Correspondence; 4],
    controls: [Correspondence; 4],
    plan_to_image: [f64; 9],
    homography: Residuals,
    similarity: Residuals,
    max_residual_px: f64,
    input_provenance_verified: bool,
    physical_qualification: bool,
}

#[derive(Serialize)]
struct Residuals {
    per_control_px: [f64; 4],
    rms_px: f64,
    max_px: f64,
    within_threshold: bool,
}

type Quad = [(f64, f64); 4];
const UNIT_SQUARE: Quad = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

pub fn inspect(request: &PerspectiveRequest) -> Result<PerspectiveDiagnostic, String> {
    if !matches!(request.surface_geometry, SurfaceGeometry::SinglePlane) {
        return Err("Déclarez un seul plan. Plusieurs profondeurs ou une surface inconnue sont hors domaine.".into());
    }
    let threshold = request.max_residual_px;
    if !threshold.is_finite() || threshold <= 0.0 || threshold > 1e6 {
        return Err(
            "Le seuil de résidu doit être positif et exprimé en pixels natifs (maximum 1e6)."
                .into(),
        );
    }
    let (plan, image) = coordinates(request.anchors)?;
    let (plan_controls, image_controls) = coordinates(request.controls)?;
    if winding(plan)? != winding(image)? {
        return Err("L'ordre des ancres indique une réflexion entre les deux repères.".into());
    }
    check_distribution(plan, plan_controls)?;
    check_distribution(image, image_controls)?;
    let transform = homography(plan, image)?;
    let similarity = fit_similarity(&request.anchors, &[0, 1, 2, 3])
        .ok_or("Impossible d'ajuster une similitude aux ancres.")?;
    let homography_residuals = residuals(request.controls, threshold, |point| {
        project_homography(&transform, (point.x, point.y)).map(|(x, y)| Point { x, y })
    })?;
    let similarity_residuals = residuals(request.controls, threshold, |point| {
        Some(similarity.apply(point))
    })?;
    Ok(PerspectiveDiagnostic {
        anchors: request.anchors,
        controls: request.controls,
        plan_to_image: transform,
        homography: homography_residuals,
        similarity: similarity_residuals,
        max_residual_px: threshold,
        input_provenance_verified: false,
        physical_qualification: false,
    })
}

fn coordinates(pairs: [Correspondence; 4]) -> Result<(Quad, Quad), String> {
    let plan = pairs.map(|pair| (pair.plan.x, pair.plan.y));
    let image = pairs.map(|pair| (pair.image.x, pair.image.y));
    if plan
        .iter()
        .chain(&image)
        .any(|&(x, y)| !(0.0..=1e6).contains(&x) || !(0.0..=1e6).contains(&y))
    {
        return Err("Les coordonnées doivent être comprises entre 0 et 1e6.".into());
    }
    Ok((plan, image))
}

fn winding(quad: Quad) -> Result<bool, String> {
    let edges: Quad = std::array::from_fn(|i| {
        let (x, y) = quad[i];
        let (next_x, next_y) = quad[(i + 1) % 4];
        (next_x - x, next_y - y)
    });
    let size = edges.iter().map(|&(x, y)| x.hypot(y)).fold(0.0, f64::max);
    let min_cross = (size * size * 1e-4).max(1e-9);
    let turns: [f64; 4] = std::array::from_fn(|i| {
        let (x, y) = edges[i];
        let (next_x, next_y) = edges[(i + 1) % 4];
        x * next_y - y * next_x
    });
    if turns.iter().all(|&turn| turn > min_cross) {
        Ok(true)
    } else if turns.iter().all(|&turn| turn < -min_cross) {
        Ok(false)
    } else {
        Err("Placez les quatre ancres dans l'ordre du pourtour d'un quadrilatère convexe non dégénéré.".into())
    }
}

fn homography(source: Quad, target: Quad) -> Result<[f64; 9], String> {
    let origin = source
        .iter()
        .fold((0.0, 0.0), |(x, y), &(u, v)| (x + u / 4.0, y + v / 4.0));
    let centered = source.map(|(x, y)| (x - origin.0, y - origin.1));
    let [a, b, c, d, e, f, g, h] = solve_homography(centered, target)?;
    let denominators = centered.map(|(x, y)| g * x + h * y + 1.0);
    let largest = denominators.iter().copied().fold(1.0, f64::max);
    if !denominators
        .iter()
        .all(|&value| value.is_finite() && value > largest * 1e-4)
    {
        return Err("La projection approche ou traverse l'horizon dans la zone d'ancres.".into());
    }
    let translation = [1.0, 0.0, -origin.0, 0.0, 1.0, -origin.1, 0.0, 0.0, 1.0];
    multiply_homographies(&[a, b, c, d, e, f, g, h, 1.0], &translation)
        .ok_or_else(|| "La matrice de projection dépasse le domaine numérique.".into())
}

fn check_distribution(anchors: Quad, controls: Quad) -> Result<(), String> {
    let normalize = homography(anchors, UNIT_SQUARE)?;
    let band = |value| match value {
        t if (0.05..=0.35).contains(&t) => Some(0),
        t if (0.65..=0.95).contains(&t) => Some(1),
        _ => None,
    };
    let mut occupied = [false; 4];
    for point in controls {
        let slot = project_homography(&normalize, point)
            .and_then(|(x, y)| Some(band(x)? + 2 * band(y)?))
            .ok_or("Les contrôles doivent être à l'intérieur, dans les quatre zones distinctes proches des coins.")?;
        if std::mem::replace(&mut occupied[slot], true) {
            return Err("Répartissez les quatre contrôles dans des zones distinctes.".into());
        }
    }
    Ok(())
}

fn residuals(
    controls: [Correspondence; 4],
    threshold: f64,
    apply: impl Fn(Point) -> Option<Point>,
) -> Result<Residuals, String> {
    let mut distances = [0.0; 4];
    for (distance, pair) in distances.iter_mut().zip(controls) {
        *distance = apply(pair.plan)
            .ok_or("Projection d'un contrôle impossible.")?
            .dist(pair.image);
        if !distance.is_finite() {
            return Err("Résidu de contrôle non calculable.".into());
        }
    }
    let max_px = distances.iter().copied().fold(0.0, f64::max);
    Ok(Residuals {
        per_control_px: distances,
        rms_px: (distances.iter().map(|x| x * x).sum::<f64>() / 4.0).sqrt(),
        max_px,
        within_threshold: max_px <= threshold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(transform: [f64; 9]) -> PerspectiveRequest {
        let pairs = |points: Quad| {
            points.map(|(x, y)| {
                let (u, v) = project_homography(&transform, (x, y)).unwrap();
                Correspondence {
                    plan: Point { x, y },
                    image: Point { x: u, y: v },
                }
            })
        };
        PerspectiveRequest {
            anchors: pairs([(0.0, 0.0), (200.0, 0.0), (200.0, 200.0), (0.0, 200.0)]),
            controls: pairs([(40.0, 40.0), (160.0, 40.0), (160.0, 160.0), (40.0, 160.0)]),
            max_residual_px: 0.25,
            surface_geometry: SurfaceGeometry::SinglePlane,
        }
    }

    #[test]
    fn projective_controls_distinguish_homography_from_similarity() {
        let request = fixture([2.0, 0.0, 25.0, 0.0, 2.0, 40.0, 0.002, 0.001, 1.0]);
        let result = inspect(&request).unwrap();
        assert!(result.homography.max_px < 1e-8);
        assert!(!result.similarity.within_threshold);
        assert!(!result.input_provenance_verified && !result.physical_qualification);
    }

    #[test]
    fn similarity_is_also_tested_on_the_same_independent_controls() {
        let mut request = fixture([2.0, 0.0, 25.0, 0.0, 2.0, 40.0, 0.0, 0.0, 1.0]);
        let result = inspect(&request).unwrap();
        assert!(result.homography.within_threshold && result.similarity.within_threshold);
        request.controls[0].image.x += 3.0;
        let result = inspect(&request).unwrap();
        assert!((result.homography.max_px - 3.0).abs() < 1e-8);
        assert!(!result.homography.within_threshold);
    }

    #[test]
    fn rejects_unverified_plane_geometry_and_nonindependent_controls() {
        let mut request = fixture([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        request.surface_geometry = SurfaceGeometry::MultipleDepths;
        assert!(inspect(&request).err().unwrap().contains("profondeurs"));
        request.surface_geometry = SurfaceGeometry::Unknown;
        assert!(inspect(&request).err().unwrap().contains("inconnue"));
        request.surface_geometry = SurfaceGeometry::SinglePlane;
        request.controls[1] = request.controls[0];
        assert!(
            inspect(&request)
                .err()
                .unwrap()
                .contains("zones distinctes")
        );
        request.controls[0] = request.anchors[0];
        assert!(inspect(&request).err().unwrap().contains("l'intérieur"));
    }

    #[test]
    fn rejects_reflections_and_crossed_or_degenerate_anchors() {
        let request = fixture([-1.0, 0.0, 250.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        assert!(inspect(&request).err().unwrap().contains("réflexion"));
        let mut request = fixture([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        request.anchors.swap(1, 2);
        assert!(inspect(&request).err().unwrap().contains("convexe"));
        request.anchors[2] = request.anchors[1];
        assert!(inspect(&request).err().unwrap().contains("convexe"));
    }
}

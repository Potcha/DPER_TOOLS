//! Calibration Brown-Conrady autonome à partir de vues d'un damier plan.
//!
//! Les coins doivent être fournis dans l'ordre ligne-major : de gauche à droite
//! sur la première ligne, puis les lignes suivantes. La calibration optimise
//! conjointement les paramètres de caméra, les coefficients de distorsion et
//! la pose de chaque vue en minimisant les erreurs de reprojection.

use crate::{geometry::Point, imaging::RadialDistortionCalibration};
use std::f64::consts::PI;

const GLOBAL_PARAMETER_COUNT: usize = 9;
const PARAMETERS_PER_VIEW: usize = 6;
const DISTORTION_PARAMETER_SCALE: f64 = 0.1;
const MIN_VIEW_COUNT: usize = 5;
const MAX_LM_ITERATIONS: usize = 70;

#[derive(Clone, Debug)]
pub struct CameraViewPose {
    /// Rotation de Rodrigues, en radians.
    pub rotation_vector_rad: [f64; 3],
    /// Translation de la grille par rapport à la caméra, en millimètres.
    pub translation_mm: [f64; 3],
}

#[derive(Clone, Debug)]
pub struct DistortionCalibrationResult {
    /// Profil directement consommable par `imaging::radial_distortion_correct`.
    pub profile: RadialDistortionCalibration,
    /// RMS des erreurs 2D euclidiennes, en pixels.
    pub reprojection_rms_px: f64,
    /// Plus grande erreur 2D euclidienne, en pixels.
    pub reprojection_max_px: f64,
    /// RMS par vue, en pixels.
    pub per_view_rms_px: Vec<f64>,
    pub view_poses: Vec<CameraViewPose>,
    pub observed_width_fraction: f64,
    pub observed_height_fraction: f64,
    pub observed_corner_radius_fraction: f64,
    pub accepted_iterations: usize,
}

/// Calibre le modèle Brown-Conrady OpenCV à partir de coins de damier connus.
///
/// `inner_corners_cols` et `inner_corners_rows` désignent le nombre
/// d'intersections internes. Chaque entrée de `views` contient exactement
/// `inner_corners_cols * inner_corners_rows` points image, dans l'ordre
/// ligne-major. `square_spacing_mm` est le pas physique entre intersections;
/// il sert à exprimer les poses retournées en millimètres.
///
/// Au moins cinq vues inclinées et une couverture significative du capteur
/// sont exigées. L'erreur de reprojection décrit l'ajustement géométrique; elle
/// ne constitue pas une incertitude métrologique ni une qualification physique.
pub fn calibrate_brown_conrady(
    image_width: u32,
    image_height: u32,
    inner_corners_cols: usize,
    inner_corners_rows: usize,
    square_spacing_mm: f64,
    views: &[Vec<Point>],
) -> Result<DistortionCalibrationResult, String> {
    validate_inputs(
        image_width,
        image_height,
        inner_corners_cols,
        inner_corners_rows,
        square_spacing_mm,
        views,
    )?;

    let width = image_width as f64;
    let height = image_height as f64;
    let image_scale = width.max(height);
    let center_x = (width - 1.0) * 0.5;
    let center_y = (height - 1.0) * 0.5;
    let board_span_squares = (inner_corners_cols - 1).max(inner_corners_rows - 1) as f64;
    let board_span_mm = board_span_squares * square_spacing_mm;
    let objects = board_points(inner_corners_cols, inner_corners_rows, board_span_squares);

    let (coverage_width, coverage_height, coverage_radius) =
        validate_spatial_coverage(image_width, image_height, views)?;

    let mut homographies = Vec::with_capacity(views.len());
    for view in views {
        homographies.push(estimate_homography(
            &objects,
            view,
            center_x,
            center_y,
            image_scale,
        )?);
    }

    let zhang_guess = estimate_zhang_intrinsics(&homographies, image_scale, center_x, center_y);
    let (fx, fy, cx, cy) = zhang_guess
        .filter(|&(fx, fy, cx, cy)| {
            plausible_intrinsics(fx, fy, cx, cy, image_width, image_height, image_scale)
        })
        .unwrap_or((
            image_scale * 1.2,
            image_scale * 1.2,
            center_x,
            center_y,
        ));

    let mut initial_poses = Vec::with_capacity(homographies.len());
    for homography in &homographies {
        initial_poses.push(decompose_homography(
            homography,
            fx / image_scale,
            fy / image_scale,
            (cx - center_x) / image_scale,
            (cy - center_y) / image_scale,
        )?);
    }
    validate_pose_diversity(&initial_poses)?;

    let mut parameters = initial_parameters(
        image_width,
        image_height,
        image_scale,
        center_x,
        center_y,
        fx,
        fy,
        cx,
        cy,
        &initial_poses,
    );
    let bounds = parameter_bounds(image_width, image_height, views.len());
    clamp_parameters(&mut parameters, &bounds);

    let mut residuals = residual_vector(
        &parameters,
        &objects,
        views,
        image_width,
        image_height,
        image_scale,
        center_x,
        center_y,
    )?;
    let mut cost = squared_norm(&residuals) * 0.5;
    let mut damping = 1.0e-3;
    let mut accepted_iterations = 0;
    let mut converged = false;

    for _iteration in 0..MAX_LM_ITERATIONS {
        let jacobian = numerical_jacobian(
            &parameters,
            &residuals,
            &objects,
            views,
            image_width,
            image_height,
            image_scale,
            center_x,
            center_y,
        )?;
        let (normal, gradient) = normal_equations(&jacobian, &residuals);
        let gradient_max = gradient.iter().map(|value| value.abs()).fold(0.0, f64::max);
        if gradient_max < 1.0e-8 {
            converged = true;
            break;
        }

        let mut accepted_this_iteration = false;
        for _attempt in 0..14 {
            let mut damped = normal.clone();
            for index in 0..damped.len() {
                damped[index][index] += damping * normal[index][index].max(1.0e-12);
            }
            let rhs: Vec<f64> = gradient.iter().map(|value| -*value).collect();
            let Some((delta, _pivot_ratio)) = solve_linear_system(damped, rhs) else {
                damping *= 10.0;
                if damping > 1.0e18 {
                    break;
                }
                continue;
            };

            let mut candidate = parameters.clone();
            for (value, change) in candidate.iter_mut().zip(delta.iter()) {
                *value += *change;
            }
            clamp_parameters(&mut candidate, &bounds);
            let step_norm = euclidean_norm_difference(&candidate, &parameters);
            if step_norm <= 1.0e-11 * (1.0 + euclidean_norm(&parameters)) {
                converged = true;
                accepted_this_iteration = true;
                break;
            }

            let candidate_residuals = match residual_vector(
                &candidate,
                &objects,
                views,
                image_width,
                image_height,
                image_scale,
                center_x,
                center_y,
            ) {
                Ok(candidate_residuals) => candidate_residuals,
                Err(_) => {
                    damping *= 10.0;
                    continue;
                }
            };
            let candidate_cost = squared_norm(&candidate_residuals) * 0.5;
            if candidate_cost < cost {
                let relative_improvement = (cost - candidate_cost) / cost.max(1.0e-24);
                parameters = candidate;
                residuals = candidate_residuals;
                cost = candidate_cost;
                damping = (damping * 0.3).max(1.0e-12);
                accepted_iterations += 1;
                accepted_this_iteration = true;
                if step_norm < 1.0e-8 * (1.0 + euclidean_norm(&parameters))
                    || relative_improvement < 1.0e-12
                {
                    converged = true;
                }
                break;
            }

            damping *= 10.0;
        }

        if converged {
            break;
        }
        if !accepted_this_iteration {
            return Err(
                "La calibration Brown-Conrady est instable : le solveur ne trouve plus de diminution de l'erreur.".into(),
            );
        }
    }

    if !converged {
        return Err(
            "La calibration Brown-Conrady est instable : le solveur n'a pas convergé dans la limite d'itérations.".into(),
        );
    }

    let final_jacobian = numerical_jacobian(
        &parameters,
        &residuals,
        &objects,
        views,
        image_width,
        image_height,
        image_scale,
        center_x,
        center_y,
    )?;
    let (final_normal, _) = normal_equations(&final_jacobian, &residuals);
    validate_solution_rank(&final_normal)?;

    let decoded = decode_parameters(&parameters, image_scale, center_x, center_y, views.len())?;
    let profile = RadialDistortionCalibration {
        image_width,
        image_height,
        fx_px: decoded.fx,
        fy_px: decoded.fy,
        cx_px: decoded.cx,
        cy_px: decoded.cy,
        k1: decoded.k1,
        k2: decoded.k2,
        k3: decoded.k3,
        p1: decoded.p1,
        p2: decoded.p2,
    };
    validate_profile(&profile)?;

    let mut squared_error_by_view = vec![0.0; views.len()];
    let mut max_error = 0.0_f64;
    for (view_index, view) in views.iter().enumerate() {
        for (corner_index, _observed) in view.iter().enumerate() {
            let residual_index = 2 * (view_index * objects.len() + corner_index);
            let error = residuals[residual_index].hypot(residuals[residual_index + 1]);
            squared_error_by_view[view_index] += error * error;
            max_error = max_error.max(error);
        }
    }
    let point_count = views.len() * objects.len();
    let rms = (squared_norm(&residuals) / point_count as f64).sqrt();
    let per_view_rms = squared_error_by_view
        .into_iter()
        .map(|sum| (sum / objects.len() as f64).sqrt())
        .collect();

    let poses = decoded
        .poses
        .into_iter()
        .map(|pose| CameraViewPose {
            rotation_vector_rad: pose.rotation,
            translation_mm: [
                pose.translation[0] * board_span_mm,
                pose.translation[1] * board_span_mm,
                pose.translation[2] * board_span_mm,
            ],
        })
        .collect();

    Ok(DistortionCalibrationResult {
        profile,
        reprojection_rms_px: rms,
        reprojection_max_px: max_error,
        per_view_rms_px: per_view_rms,
        view_poses: poses,
        observed_width_fraction: coverage_width,
        observed_height_fraction: coverage_height,
        observed_corner_radius_fraction: coverage_radius,
        accepted_iterations,
    })
}

#[derive(Clone, Copy, Debug)]
struct Pose {
    rotation: [f64; 3],
    translation: [f64; 3],
}

#[derive(Clone)]
struct DecodedParameters {
    fx: f64,
    fy: f64,
    cx: f64,
    cy: f64,
    k1: f64,
    k2: f64,
    k3: f64,
    p1: f64,
    p2: f64,
    poses: Vec<Pose>,
}

fn validate_inputs(
    image_width: u32,
    image_height: u32,
    cols: usize,
    rows: usize,
    spacing_mm: f64,
    views: &[Vec<Point>],
) -> Result<(), String> {
    if image_width < 32 || image_height < 32 {
        return Err("La résolution de calibration doit être au moins de 32 × 32 pixels.".into());
    }
    if cols < 3 || rows < 3 {
        return Err("Le damier doit comporter au moins 3 × 3 intersections internes.".into());
    }
    let point_count = cols
        .checked_mul(rows)
        .filter(|count| *count <= 500)
        .ok_or_else(|| "Le nombre d'intersections internes est invalide ou trop élevé.".to_owned())?;
    if !spacing_mm.is_finite() || spacing_mm <= 0.0 {
        return Err("Le pas du damier doit être un nombre fini strictement positif en millimètres.".into());
    }
    if views.len() < MIN_VIEW_COUNT {
        return Err(format!(
            "Au moins {MIN_VIEW_COUNT} vues du damier sont requises pour estimer la distorsion."
        ));
    }
    if views.len() > 20 {
        return Err("La calibration est limitée à 20 vues du damier.".into());
    }
    if point_count.saturating_mul(views.len()) > 2_000 {
        return Err("La calibration est limitée à 2 000 observations de coins pour le solveur local.".into());
    }
    let max_x = (image_width - 1) as f64;
    let max_y = (image_height - 1) as f64;
    for (view_index, view) in views.iter().enumerate() {
        if view.len() != point_count {
            return Err(format!(
                "La vue {} contient {} points; {} intersections en ordre ligne-major sont attendues.",
                view_index + 1,
                view.len(),
                point_count
            ));
        }
        for (point_index, point) in view.iter().enumerate() {
            if !point.x.is_finite()
                || !point.y.is_finite()
                || point.x < 0.0
                || point.y < 0.0
                || point.x > max_x
                || point.y > max_y
            {
                return Err(format!(
                    "Le point {} de la vue {} est hors image ou non fini.",
                    point_index + 1,
                    view_index + 1
                ));
            }
        }
    }
    Ok(())
}

fn board_points(cols: usize, rows: usize, span: f64) -> Vec<[f64; 2]> {
    let center_col = (cols - 1) as f64 * 0.5;
    let center_row = (rows - 1) as f64 * 0.5;
    let mut points = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            points.push([
                (col as f64 - center_col) / span,
                (row as f64 - center_row) / span,
            ]);
        }
    }
    points
}

fn validate_spatial_coverage(
    image_width: u32,
    image_height: u32,
    views: &[Vec<Point>],
) -> Result<(f64, f64, f64), String> {
    let width = (image_width - 1) as f64;
    let height = (image_height - 1) as f64;
    let center_x = width * 0.5;
    let center_y = height * 0.5;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut quadrants = [false; 4];
    let mut quadrant_radius = [0.0_f64; 4];
    let mut max_radius = 0.0_f64;
    for point in views.iter().flatten() {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
        let quadrant = match (point.x >= center_x, point.y >= center_y) {
            (false, false) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (true, true) => 3,
        };
        let radius = (point.x - center_x).hypot(point.y - center_y);
        quadrants[quadrant] = true;
        quadrant_radius[quadrant] = quadrant_radius[quadrant].max(radius);
        max_radius = max_radius.max(radius);
    }
    let width_fraction = (max_x - min_x) / width.max(1.0);
    let height_fraction = (max_y - min_y) / height.max(1.0);
    let sensor_corner_radius = center_x.hypot(center_y);
    let radius_fraction = max_radius / sensor_corner_radius.max(1.0);
    if width_fraction < 0.35 || height_fraction < 0.35 {
        return Err(format!(
            "Couverture du damier insuffisante : étendue observée {:.0}% × {:.0}% de l'image; au moins 35% dans chaque axe sont exigés.",
            width_fraction * 100.0,
            height_fraction * 100.0
        ));
    }
    if quadrants.iter().any(|present| !present) {
        return Err("Les points du damier doivent couvrir les quatre quadrants de l'image.".into());
    }
    if radius_fraction < 0.70 || quadrant_radius.iter().any(|radius| radius / sensor_corner_radius.max(1.0) < 0.30) {
        return Err(
            "Couverture radiale insuffisante : déplacez le damier vers les bords et les quatre coins de l'image.".into(),
        );
    }
    Ok((width_fraction, height_fraction, radius_fraction))
}

fn estimate_homography(
    objects: &[[f64; 2]],
    observed: &[Point],
    center_x: f64,
    center_y: f64,
    image_scale: f64,
) -> Result<[f64; 9], String> {
    let mut normal = vec![vec![0.0; 9]; 9];
    for (object, pixel) in objects.iter().zip(observed.iter()) {
        let x = object[0];
        let y = object[1];
        let u = (pixel.x - center_x) / image_scale;
        let v = (pixel.y - center_y) / image_scale;
        let row_u = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, -u];
        let row_v = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, -v];
        accumulate_outer_product(&mut normal, &row_u);
        accumulate_outer_product(&mut normal, &row_v);
    }
    let (values, vectors) = symmetric_eigen(normal)?;
    let smallest = values
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)
        .ok_or_else(|| "Impossible de calculer l'homographie du damier.".to_owned())?;
    let mut homography = [0.0; 9];
    for row in 0..9 {
        homography[row] = vectors[row][smallest];
    }
    let norm = homography.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !norm.is_finite() || norm <= 1.0e-14 {
        return Err("L'homographie d'une vue du damier est dégénérée.".into());
    }
    for value in &mut homography {
        *value /= norm;
    }
    Ok(homography)
}

fn estimate_zhang_intrinsics(
    homographies: &[[f64; 9]],
    image_scale: f64,
    center_x: f64,
    center_y: f64,
) -> Option<(f64, f64, f64, f64)> {
    let mut normal = vec![vec![0.0; 6]; 6];
    for homography in homographies {
        let h1 = [homography[0], homography[3], homography[6]];
        let h2 = [homography[1], homography[4], homography[7]];
        let v12 = zhang_v(&h1, &h2);
        let v11 = zhang_v(&h1, &h1);
        let v22 = zhang_v(&h2, &h2);
        let difference: [f64; 6] = std::array::from_fn(|index| v11[index] - v22[index]);
        accumulate_outer_product(&mut normal, &v12);
        accumulate_outer_product(&mut normal, &difference);
    }
    let (values, vectors) = symmetric_eigen(normal).ok()?;
    let smallest = values
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)?;
    let mut b: [f64; 6] = std::array::from_fn(|index| vectors[index][smallest]);
    if b[0] < 0.0 {
        for value in &mut b {
            *value = -*value;
        }
    }
    if b[0] <= 1.0e-14 {
        return None;
    }
    let denominator = b[0] * b[2] - b[1] * b[1];
    if denominator <= 1.0e-14 {
        return None;
    }
    let cy_normalized = (b[1] * b[3] - b[0] * b[4]) / denominator;
    let lambda = b[5] - (b[3] * b[3] + cy_normalized * (b[1] * b[3] - b[0] * b[4])) / b[0];
    if !lambda.is_finite() || lambda <= 1.0e-14 {
        return None;
    }
    let fx_normalized = (lambda / b[0]).sqrt();
    let fy_normalized = (lambda * b[0] / denominator).sqrt();
    let skew_normalized = -b[1] * fx_normalized * fx_normalized * fy_normalized / lambda;
    let cx_normalized = skew_normalized * cy_normalized / fy_normalized
        - b[3] * fx_normalized * fx_normalized / lambda;
    let fx = fx_normalized * image_scale;
    let fy = fy_normalized * image_scale;
    let cx = center_x + cx_normalized * image_scale;
    let cy = center_y + cy_normalized * image_scale;
    if [fx, fy, cx, cy].iter().all(|value| value.is_finite()) {
        Some((fx, fy, cx, cy))
    } else {
        None
    }
}

fn zhang_v(first: &[f64; 3], second: &[f64; 3]) -> [f64; 6] {
    [
        first[0] * second[0],
        first[0] * second[1] + first[1] * second[0],
        first[1] * second[1],
        first[2] * second[0] + first[0] * second[2],
        first[2] * second[1] + first[1] * second[2],
        first[2] * second[2],
    ]
}

fn plausible_intrinsics(
    fx: f64,
    fy: f64,
    cx: f64,
    cy: f64,
    width: u32,
    height: u32,
    scale: f64,
) -> bool {
    fx.is_finite()
        && fy.is_finite()
        && cx.is_finite()
        && cy.is_finite()
        && fx >= 0.08 * scale
        && fy >= 0.08 * scale
        && fx <= 40.0 * scale
        && fy <= 40.0 * scale
        && cx >= 0.0
        && cx < width as f64
        && cy >= 0.0
        && cy < height as f64
}

fn decompose_homography(
    h: &[f64; 9],
    fx_normalized: f64,
    fy_normalized: f64,
    cx_normalized: f64,
    cy_normalized: f64,
) -> Result<Pose, String> {
    let h1 = [h[0], h[3], h[6]];
    let h2 = [h[1], h[4], h[7]];
    let h3 = [h[2], h[5], h[8]];
    let inverse_k = |value: [f64; 3]| {
        [
            (value[0] - cx_normalized * value[2]) / fx_normalized,
            (value[1] - cy_normalized * value[2]) / fy_normalized,
            value[2],
        ]
    };
    let a1 = inverse_k(h1);
    let a2 = inverse_k(h2);
    let a3 = inverse_k(h3);
    let scale = 2.0 / (norm3(a1) + norm3(a2)).max(1.0e-14);
    let sign = if scale * a3[2] >= 0.0 { 1.0 } else { -1.0 };
    let factor = scale * sign;
    let mut r1 = scale3(a1, factor);
    let r2_raw = scale3(a2, factor);
    let mut translation = scale3(a3, factor);
    r1 = normalize3(r1).ok_or_else(|| "La pose initiale d'une vue est dégénérée.".to_owned())?;
    let r2_orthogonal = sub3(r2_raw, scale3(r1, dot3(r1, r2_raw)));
    let mut r2 = normalize3(r2_orthogonal)
        .ok_or_else(|| "La pose initiale d'une vue est dégénérée.".to_owned())?;
    let mut r3 = normalize3(cross3(r1, r2))
        .ok_or_else(|| "La pose initiale d'une vue est dégénérée.".to_owned())?;
    r2 = normalize3(cross3(r3, r1))
        .ok_or_else(|| "La pose initiale d'une vue est dégénérée.".to_owned())?;
    if translation[2] <= 0.0 {
        r1 = scale3(r1, -1.0);
        r2 = scale3(r2, -1.0);
        r3 = normalize3(cross3(r1, r2))
            .ok_or_else(|| "La pose initiale d'une vue est dégénérée.".to_owned())?;
        translation = scale3(translation, -1.0);
    }
    if !translation.iter().all(|value| value.is_finite()) || translation[2] <= 1.0e-8 {
        return Err("La pose initiale d'une vue place le damier derrière la caméra.".into());
    }
    let rotation_matrix = [
        r1[0], r2[0], r3[0], r1[1], r2[1], r3[1], r1[2], r2[2], r3[2],
    ];
    let rotation = matrix_to_rotation_vector(rotation_matrix)?;
    Ok(Pose {
        rotation,
        translation,
    })
}

fn validate_pose_diversity(poses: &[Pose]) -> Result<(), String> {
    let normals: Vec<[f64; 3]> = poses
        .iter()
        .map(|pose| {
            let rotation = rotation_matrix(pose.rotation);
            [rotation[2], rotation[5], rotation[8]]
        })
        .collect();
    let mut maximum_pair_angle = 0.0_f64;
    let mut tilted_count = 0;
    for (index, normal) in normals.iter().enumerate() {
        let tilt = normal[2].abs().clamp(0.0, 1.0).acos();
        if tilt >= 4.0_f64.to_radians() {
            tilted_count += 1;
        }
        for other in &normals[index + 1..] {
            let angle = dot3(*normal, *other).abs().clamp(0.0, 1.0).acos();
            maximum_pair_angle = maximum_pair_angle.max(angle);
        }
    }
    if tilted_count < 2 || maximum_pair_angle < 8.0_f64.to_radians() {
        return Err(
            "Les vues sont trop frontales ou trop semblables; inclinez le damier différemment dans au moins deux vues.".into(),
        );
    }
    Ok(())
}

fn initial_parameters(
    width: u32,
    height: u32,
    image_scale: f64,
    center_x: f64,
    center_y: f64,
    fx: f64,
    fy: f64,
    cx: f64,
    cy: f64,
    poses: &[Pose],
) -> Vec<f64> {
    let mut parameters = vec![0.0; GLOBAL_PARAMETER_COUNT + PARAMETERS_PER_VIEW * poses.len()];
    parameters[0] = (fx / image_scale).ln();
    parameters[1] = (fy / image_scale).ln();
    parameters[2] = (cx - center_x) / image_scale;
    parameters[3] = (cy - center_y) / image_scale;
    for (index, pose) in poses.iter().enumerate() {
        let base = GLOBAL_PARAMETER_COUNT + PARAMETERS_PER_VIEW * index;
        parameters[base..base + 3].copy_from_slice(&pose.rotation);
        parameters[base + 3..base + 6].copy_from_slice(&pose.translation);
    }
    let _ = (width, height);
    parameters
}

fn parameter_bounds(width: u32, height: u32, view_count: usize) -> Vec<(f64, f64)> {
    let mut bounds = vec![(f64::NEG_INFINITY, f64::INFINITY); GLOBAL_PARAMETER_COUNT + 6 * view_count];
    bounds[0] = (0.08_f64.ln(), 40.0_f64.ln());
    bounds[1] = (0.08_f64.ln(), 40.0_f64.ln());
    let width_scale = width.max(height) as f64;
    let height_scale = width_scale;
    let center_x = (width as f64 - 1.0) * 0.5;
    let center_y = (height as f64 - 1.0) * 0.5;
    bounds[2] = (-center_x / width_scale, ((width - 1) as f64 - center_x) / width_scale);
    bounds[3] = (-center_y / height_scale, ((height - 1) as f64 - center_y) / height_scale);
    for index in 4..GLOBAL_PARAMETER_COUNT {
        bounds[index] = (-50.0, 50.0);
    }
    for view_index in 0..view_count {
        let base = GLOBAL_PARAMETER_COUNT + PARAMETERS_PER_VIEW * view_index;
        for index in 0..3 {
            bounds[base + index] = (-PI, PI);
        }
        bounds[base + 3] = (-1000.0, 1000.0);
        bounds[base + 4] = (-1000.0, 1000.0);
        bounds[base + 5] = (1.0e-3, 100_000.0);
    }
    bounds
}

fn clamp_parameters(parameters: &mut [f64], bounds: &[(f64, f64)]) {
    for (parameter, &(minimum, maximum)) in parameters.iter_mut().zip(bounds.iter()) {
        *parameter = parameter.clamp(minimum, maximum);
    }
}

fn decode_parameters(
    parameters: &[f64],
    image_scale: f64,
    center_x: f64,
    center_y: f64,
    view_count: usize,
) -> Result<DecodedParameters, String> {
    if parameters.len() != GLOBAL_PARAMETER_COUNT + PARAMETERS_PER_VIEW * view_count {
        return Err("Nombre de paramètres internes incohérent.".into());
    }
    let decoded = DecodedParameters {
        fx: parameters[0].exp() * image_scale,
        fy: parameters[1].exp() * image_scale,
        cx: center_x + parameters[2] * image_scale,
        cy: center_y + parameters[3] * image_scale,
        k1: parameters[4] * DISTORTION_PARAMETER_SCALE,
        k2: parameters[5] * DISTORTION_PARAMETER_SCALE,
        k3: parameters[6] * DISTORTION_PARAMETER_SCALE,
        p1: parameters[7] * DISTORTION_PARAMETER_SCALE,
        p2: parameters[8] * DISTORTION_PARAMETER_SCALE,
        poses: (0..view_count)
            .map(|index| {
                let base = GLOBAL_PARAMETER_COUNT + PARAMETERS_PER_VIEW * index;
                Pose {
                    rotation: [parameters[base], parameters[base + 1], parameters[base + 2]],
                    translation: [parameters[base + 3], parameters[base + 4], parameters[base + 5]],
                }
            })
            .collect(),
    };
    let global = [
        decoded.fx,
        decoded.fy,
        decoded.cx,
        decoded.cy,
        decoded.k1,
        decoded.k2,
        decoded.k3,
        decoded.p1,
        decoded.p2,
    ];
    if global.iter().any(|value| !value.is_finite())
        || decoded.fx <= 0.0
        || decoded.fy <= 0.0
        || decoded.poses.iter().any(|pose| {
            pose.rotation.iter().chain(pose.translation.iter()).any(|value| !value.is_finite())
                || pose.translation[2] <= 0.0
        })
    {
        return Err("Le solveur a produit des paramètres non finis ou incohérents.".into());
    }
    Ok(decoded)
}

#[allow(clippy::too_many_arguments)]
fn residual_vector(
    parameters: &[f64],
    objects: &[[f64; 2]],
    views: &[Vec<Point>],
    image_width: u32,
    image_height: u32,
    image_scale: f64,
    center_x: f64,
    center_y: f64,
) -> Result<Vec<f64>, String> {
    let decoded = decode_parameters(parameters, image_scale, center_x, center_y, views.len())?;
    let mut residuals = Vec::with_capacity(2 * objects.len() * views.len());
    for (view_index, view) in views.iter().enumerate() {
        let rotation = rotation_matrix(decoded.poses[view_index].rotation);
        let translation = decoded.poses[view_index].translation;
        for (object, observed) in objects.iter().zip(view.iter()) {
            let camera = transform_point(rotation, translation, *object);
            if !camera[2].is_finite() || camera[2] <= 1.0e-8 {
                return Err("Une pose estimée place un coin du damier derrière la caméra.".into());
            }
            let x = camera[0] / camera[2];
            let y = camera[1] / camera[2];
            let radius_squared = x * x + y * y;
            let radial = 1.0
                + decoded.k1 * radius_squared
                + decoded.k2 * radius_squared * radius_squared
                + decoded.k3 * radius_squared * radius_squared * radius_squared;
            let distorted_x = x * radial
                + 2.0 * decoded.p1 * x * y
                + decoded.p2 * (radius_squared + 2.0 * x * x);
            let distorted_y = y * radial
                + decoded.p1 * (radius_squared + 2.0 * y * y)
                + 2.0 * decoded.p2 * x * y;
            let projected_x = decoded.fx * distorted_x + decoded.cx;
            let projected_y = decoded.fy * distorted_y + decoded.cy;
            if !projected_x.is_finite() || !projected_y.is_finite() {
                return Err("Une reprojection du solveur n'est pas finie.".into());
            }
            residuals.push(projected_x - observed.x);
            residuals.push(projected_y - observed.y);
        }
    }
    let _ = (image_width, image_height);
    Ok(residuals)
}

#[allow(clippy::too_many_arguments)]
fn numerical_jacobian(
    parameters: &[f64],
    current_residuals: &[f64],
    objects: &[[f64; 2]],
    views: &[Vec<Point>],
    image_width: u32,
    image_height: u32,
    image_scale: f64,
    center_x: f64,
    center_y: f64,
) -> Result<Vec<Vec<f64>>, String> {
    let mut jacobian = vec![vec![0.0; parameters.len()]; current_residuals.len()];
    for parameter_index in 0..parameters.len() {
        let step = 1.0e-5 * (1.0 + parameters[parameter_index].abs());
        let mut plus = parameters.to_vec();
        let mut minus = parameters.to_vec();
        plus[parameter_index] += step;
        minus[parameter_index] -= step;
        let plus_residuals = residual_vector(
            &plus,
            objects,
            views,
            image_width,
            image_height,
            image_scale,
            center_x,
            center_y,
        )?;
        let minus_residuals = residual_vector(
            &minus,
            objects,
            views,
            image_width,
            image_height,
            image_scale,
            center_x,
            center_y,
        )?;
        for residual_index in 0..current_residuals.len() {
            jacobian[residual_index][parameter_index] =
                (plus_residuals[residual_index] - minus_residuals[residual_index]) / (2.0 * step);
        }
    }
    Ok(jacobian)
}

fn normal_equations(jacobian: &[Vec<f64>], residuals: &[f64]) -> (Vec<Vec<f64>>, Vec<f64>) {
    let parameter_count = jacobian.first().map_or(0, Vec::len);
    let mut normal = vec![vec![0.0; parameter_count]; parameter_count];
    let mut gradient = vec![0.0; parameter_count];
    for (row, residual) in jacobian.iter().zip(residuals.iter()) {
        for i in 0..parameter_count {
            gradient[i] += row[i] * residual;
            for j in 0..=i {
                normal[i][j] += row[i] * row[j];
            }
        }
    }
    for i in 0..parameter_count {
        for j in 0..i {
            normal[j][i] = normal[i][j];
        }
    }
    (normal, gradient)
}

fn validate_solution_rank(normal: &[Vec<f64>]) -> Result<(), String> {
    let (eigenvalues, _) = symmetric_eigen(normal.to_vec())
        .map_err(|_| "La solution est instable : le système de reprojection est singulier.".to_owned())?;
    let largest = eigenvalues.iter().copied().fold(0.0_f64, f64::max);
    let smallest = eigenvalues.iter().copied().fold(f64::INFINITY, f64::min);
    if !largest.is_finite() || !smallest.is_finite() || largest <= 0.0 || smallest <= largest * 1.0e-14 {
        return Err(
            "La solution est instable ou sous-déterminée; ajoutez des vues inclinées et mieux réparties dans le champ.".into(),
        );
    }
    Ok(())
}

fn validate_profile(profile: &RadialDistortionCalibration) -> Result<(), String> {
    let width = profile.image_width as f64;
    let height = profile.image_height as f64;
    if !profile.fx_px.is_finite()
        || !profile.fy_px.is_finite()
        || !profile.cx_px.is_finite()
        || !profile.cy_px.is_finite()
        || profile.fx_px <= 0.0
        || profile.fy_px <= 0.0
        || profile.cx_px < 0.0
        || profile.cx_px >= width
        || profile.cy_px < 0.0
        || profile.cy_px >= height
        || [profile.k1, profile.k2, profile.k3, profile.p1, profile.p2]
            .iter()
            .any(|value| !value.is_finite() || value.abs() > 5.0)
    {
        return Err("Le profil estimé contient des paramètres hors bornes.".into());
    }
    let min_x = -profile.cx_px / profile.fx_px;
    let max_x = (width - 1.0 - profile.cx_px) / profile.fx_px;
    let min_y = -profile.cy_px / profile.fy_px;
    let max_y = (height - 1.0 - profile.cy_px) / profile.fy_px;
    for grid_y in 0..=24 {
        let y = min_y + (max_y - min_y) * grid_y as f64 / 24.0;
        for grid_x in 0..=24 {
            let x = min_x + (max_x - min_x) * grid_x as f64 / 24.0;
            let radius_squared = x * x + y * y;
            let radius_fourth = radius_squared * radius_squared;
            let radial = 1.0 + profile.k1 * radius_squared
                + profile.k2 * radius_fourth
                + profile.k3 * radius_fourth * radius_squared;
            let slope = profile.k1 + 2.0 * profile.k2 * radius_squared
                + 3.0 * profile.k3 * radius_fourth;
            let derivative = 1.0 + 3.0 * profile.k1 * radius_squared
                + 5.0 * profile.k2 * radius_fourth
                + 7.0 * profile.k3 * radius_fourth * radius_squared;
            let dr_dx = 2.0 * x * slope;
            let dr_dy = 2.0 * y * slope;
            let j00 = radial + x * dr_dx + 2.0 * profile.p1 * y + 6.0 * profile.p2 * x;
            let j01 = x * dr_dy + 2.0 * profile.p1 * x + 2.0 * profile.p2 * y;
            let j10 = y * dr_dx + 2.0 * profile.p1 * x + 2.0 * profile.p2 * y;
            let j11 = radial + y * dr_dy + 6.0 * profile.p1 * y + 2.0 * profile.p2 * x;
            let determinant = j00 * j11 - j01 * j10;
            if !radial.is_finite()
                || !derivative.is_finite()
                || !determinant.is_finite()
                || radial <= 0.0
                || derivative <= 0.0
                || determinant <= 1.0e-8
            {
                return Err(
                    "Le profil estimé n'est pas monotone sur toute l'image; la correction optique serait instable.".into(),
                );
            }
        }
    }
    Ok(())
}

fn accumulate_outer_product(matrix: &mut [Vec<f64>], row: &[f64]) {
    for i in 0..row.len() {
        for j in 0..row.len() {
            matrix[i][j] += row[i] * row[j];
        }
    }
}

fn symmetric_eigen(mut matrix: Vec<Vec<f64>>) -> Result<(Vec<f64>, Vec<Vec<f64>>), String> {
    let size = matrix.len();
    if size == 0 || matrix.iter().any(|row| row.len() != size) {
        return Err("Matrice symétrique vide ou non carrée.".into());
    }
    if matrix.iter().flatten().any(|value| !value.is_finite()) {
        return Err("Matrice de calibration non finie.".into());
    }
    let mut vectors = vec![vec![0.0; size]; size];
    for index in 0..size {
        vectors[index][index] = 1.0;
    }
    let max_sweeps = 100 * size * size;
    for _ in 0..max_sweeps {
        let mut p = 0;
        let mut q = 1.min(size - 1);
        let mut largest_off_diagonal = 0.0_f64;
        let mut diagonal_scale = 0.0_f64;
        for row in 0..size {
            diagonal_scale = diagonal_scale.max(matrix[row][row].abs());
            for col in row + 1..size {
                if matrix[row][col].abs() > largest_off_diagonal {
                    largest_off_diagonal = matrix[row][col].abs();
                    p = row;
                    q = col;
                }
            }
        }
        if largest_off_diagonal <= 1.0e-14 * diagonal_scale.max(1.0) {
            break;
        }
        let app = matrix[p][p];
        let aqq = matrix[q][q];
        let apq = matrix[p][q];
        let angle = 0.5 * (2.0 * apq).atan2(aqq - app);
        let cosine = angle.cos();
        let sine = angle.sin();
        matrix[p][p] = cosine * cosine * app - 2.0 * sine * cosine * apq + sine * sine * aqq;
        matrix[q][q] = sine * sine * app + 2.0 * sine * cosine * apq + cosine * cosine * aqq;
        matrix[p][q] = 0.0;
        matrix[q][p] = 0.0;
        for index in 0..size {
            if index != p && index != q {
                let aip = matrix[index][p];
                let aiq = matrix[index][q];
                matrix[index][p] = cosine * aip - sine * aiq;
                matrix[p][index] = matrix[index][p];
                matrix[index][q] = sine * aip + cosine * aiq;
                matrix[q][index] = matrix[index][q];
            }
            let vip = vectors[index][p];
            let viq = vectors[index][q];
            vectors[index][p] = cosine * vip - sine * viq;
            vectors[index][q] = sine * vip + cosine * viq;
        }
    }
    let eigenvalues: Vec<f64> = (0..size).map(|index| matrix[index][index]).collect();
    if eigenvalues.iter().any(|value| !value.is_finite())
        || vectors.iter().flatten().any(|value| !value.is_finite())
    {
        return Err("Décomposition propre non finie.".into());
    }
    Ok((eigenvalues, vectors))
}

fn solve_linear_system(
    mut matrix: Vec<Vec<f64>>,
    mut rhs: Vec<f64>,
) -> Option<(Vec<f64>, f64)> {
    let size = matrix.len();
    if size == 0 || rhs.len() != size || matrix.iter().any(|row| row.len() != size) {
        return None;
    }
    let matrix_scale = matrix.iter().flatten().map(|value| value.abs()).fold(0.0, f64::max);
    if !matrix_scale.is_finite() || matrix_scale == 0.0 {
        return None;
    }
    let mut min_pivot = f64::INFINITY;
    let mut max_pivot = 0.0_f64;
    for pivot_column in 0..size {
        let pivot_row = (pivot_column..size)
            .max_by(|left, right| {
                matrix[*left][pivot_column]
                    .abs()
                    .total_cmp(&matrix[*right][pivot_column].abs())
            })?;
        let pivot = matrix[pivot_row][pivot_column].abs();
        if !pivot.is_finite() || pivot <= matrix_scale * 1.0e-15 {
            return None;
        }
        min_pivot = min_pivot.min(pivot);
        max_pivot = max_pivot.max(pivot);
        matrix.swap(pivot_column, pivot_row);
        rhs.swap(pivot_column, pivot_row);
        let divisor = matrix[pivot_column][pivot_column];
        for row in (pivot_column + 1)..size {
            let factor = matrix[row][pivot_column] / divisor;
            matrix[row][pivot_column] = 0.0;
            for col in (pivot_column + 1)..size {
                matrix[row][col] -= factor * matrix[pivot_column][col];
            }
            rhs[row] -= factor * rhs[pivot_column];
        }
    }
    let mut solution = vec![0.0; size];
    for row in (0..size).rev() {
        let mut value = rhs[row];
        for col in row + 1..size {
            value -= matrix[row][col] * solution[col];
        }
        solution[row] = value / matrix[row][row];
        if !solution[row].is_finite() {
            return None;
        }
    }
    Some((solution, min_pivot / max_pivot.max(f64::MIN_POSITIVE)))
}

fn rotation_matrix(vector: [f64; 3]) -> [f64; 9] {
    let theta_squared = dot3(vector, vector);
    let (a, b) = if theta_squared < 1.0e-12 {
        (1.0 - theta_squared / 6.0, 0.5 - theta_squared / 24.0)
    } else {
        let theta = theta_squared.sqrt();
        (theta.sin() / theta, (1.0 - theta.cos()) / theta_squared)
    };
    let [x, y, z] = vector;
    let k = [0.0, -z, y, z, 0.0, -x, -y, x, 0.0];
    let k_squared = multiply_3x3(k, k);
    let mut result = [0.0; 9];
    for index in 0..9 {
        result[index] = (if index % 4 == 0 { 1.0 } else { 0.0 }) + a * k[index] + b * k_squared[index];
    }
    result
}

fn matrix_to_rotation_vector(matrix: [f64; 9]) -> Result<[f64; 3], String> {
    let cosine = ((matrix[0] + matrix[4] + matrix[8] - 1.0) * 0.5).clamp(-1.0, 1.0);
    let angle = cosine.acos();
    if !angle.is_finite() {
        return Err("La rotation initiale n'est pas finie.".into());
    }
    if angle < 1.0e-7 {
        return Ok([
            0.5 * (matrix[7] - matrix[5]),
            0.5 * (matrix[2] - matrix[6]),
            0.5 * (matrix[3] - matrix[1]),
        ]);
    }
    if PI - angle < 1.0e-4 {
        let mut axis = [
            ((matrix[0] + 1.0) * 0.5).max(0.0).sqrt(),
            ((matrix[4] + 1.0) * 0.5).max(0.0).sqrt(),
            ((matrix[8] + 1.0) * 0.5).max(0.0).sqrt(),
        ];
        if axis[0] >= axis[1] && axis[0] >= axis[2] && axis[0] > 1.0e-8 {
            axis[1] = (matrix[1] + matrix[3]) / (4.0 * axis[0]);
            axis[2] = (matrix[2] + matrix[6]) / (4.0 * axis[0]);
        } else if axis[1] >= axis[2] && axis[1] > 1.0e-8 {
            axis[0] = (matrix[1] + matrix[3]) / (4.0 * axis[1]);
            axis[2] = (matrix[5] + matrix[7]) / (4.0 * axis[1]);
        } else if axis[2] > 1.0e-8 {
            axis[0] = (matrix[2] + matrix[6]) / (4.0 * axis[2]);
            axis[1] = (matrix[5] + matrix[7]) / (4.0 * axis[2]);
        }
        let axis = normalize3(axis)
            .ok_or_else(|| "L'axe de la rotation initiale est dégénéré.".to_owned())?;
        return Ok(scale3(axis, angle));
    }
    let factor = angle / (2.0 * angle.sin());
    Ok([
        factor * (matrix[7] - matrix[5]),
        factor * (matrix[2] - matrix[6]),
        factor * (matrix[3] - matrix[1]),
    ])
}

fn transform_point(rotation: [f64; 9], translation: [f64; 3], point: [f64; 2]) -> [f64; 3] {
    [
        rotation[0] * point[0] + rotation[1] * point[1] + translation[0],
        rotation[3] * point[0] + rotation[4] * point[1] + translation[1],
        rotation[6] * point[0] + rotation[7] * point[1] + translation[2],
    ]
}

fn multiply_3x3(left: [f64; 9], right: [f64; 9]) -> [f64; 9] {
    let mut result = [0.0; 9];
    for row in 0..3 {
        for col in 0..3 {
            for inner in 0..3 {
                result[row * 3 + col] += left[row * 3 + inner] * right[inner * 3 + col];
            }
        }
    }
    result
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn norm3(value: [f64; 3]) -> f64 {
    dot3(value, value).sqrt()
}

fn normalize3(value: [f64; 3]) -> Option<[f64; 3]> {
    let length = norm3(value);
    if !length.is_finite() || length <= 1.0e-14 {
        None
    } else {
        Some(scale3(value, 1.0 / length))
    }
}

fn scale3(value: [f64; 3], scale: f64) -> [f64; 3] {
    [value[0] * scale, value[1] * scale, value[2] * scale]
}

fn sub3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn cross3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn squared_norm(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum()
}

fn euclidean_norm(values: &[f64]) -> f64 {
    squared_norm(values).sqrt()
}

fn euclidean_norm_difference(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right.iter())
        .map(|(left, right)| (left - right) * (left - right))
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_views() -> Vec<Vec<Point>> {
        let width = 1280_u32;
        let height = 960_u32;
        let cols = 7_usize;
        let rows = 6_usize;
        let fx = 900.0;
        let fy = 910.0;
        let cx = (width as f64 - 1.0) * 0.5;
        let cy = (height as f64 - 1.0) * 0.5;
        let k1 = 0.035;
        let k2 = -0.008;
        let k3 = 0.001;
        let p1 = 0.0005;
        let p2 = -0.0004;
        let objects = board_points(cols, rows, (cols - 1) as f64);
        let views = [
            ([0.14, -0.12, 0.02], 475.0, 375.0),
            ([-0.18, 0.15, -0.03], 805.0, 585.0),
            ([0.30, 0.10, 0.05], 640.0, 480.0),
            ([-0.24, -0.31, -0.04], 640.0, 480.0),
            ([0.08, 0.35, 0.12], 640.0, 480.0),
        ];

        views.into_iter().map(|(rotation_vector, center_x, center_y)| {
            let depth = 1.3;
            let translation = [
                (center_x - cx) * depth / fx,
                (center_y - cy) * depth / fy,
                depth,
            ];
            let rotation = rotation_matrix(rotation_vector);
            objects.iter().map(|object| {
                let camera = transform_point(rotation, translation, *object);
                let x = camera[0] / camera[2];
                let y = camera[1] / camera[2];
                let radius_squared = x * x + y * y;
                let radial = 1.0 + k1 * radius_squared
                    + k2 * radius_squared.powi(2) + k3 * radius_squared.powi(3);
                let distorted_x = x * radial + 2.0 * p1 * x * y
                    + p2 * (radius_squared + 2.0 * x * x);
                let distorted_y = y * radial + p1 * (radius_squared + 2.0 * y * y)
                    + 2.0 * p2 * x * y;
                Point { x: fx * distorted_x + cx, y: fy * distorted_y + cy }
            }).collect()
        }).collect()
    }

    #[test]
    fn requires_five_ordered_views_with_valid_native_coordinates() {
        let width = 1280;
        let height = 960;
        let valid_view = vec![Point { x: 400.0, y: 300.0 }; 42];

        let too_few = vec![valid_view.clone(); 4];
        assert!(calibrate_brown_conrady(width, height, 7, 6, 3.0, &too_few)
            .unwrap_err().contains("Au moins 5 vues"));

        let wrong_count = vec![vec![Point { x: 400.0, y: 300.0 }; 41]; 5];
        assert!(calibrate_brown_conrady(width, height, 7, 6, 3.0, &wrong_count)
            .unwrap_err().contains("intersections en ordre ligne-major"));

        let outside = vec![vec![Point { x: 1280.0, y: 300.0 }; 42]; 5];
        assert!(calibrate_brown_conrady(width, height, 7, 6, 3.0, &outside)
            .unwrap_err().contains("hors image"));
    }

    #[test]
    fn recovers_a_brown_conrady_profile_from_synthetic_tilted_views() {
        let result = calibrate_brown_conrady(
            1280, 960, 7, 6, 3.0, &synthetic_views(),
        ).unwrap();

        assert!(result.reprojection_rms_px.is_finite());
        assert!(result.reprojection_rms_px < 0.5);
        assert!(result.reprojection_max_px < 2.0);
        assert_eq!(result.per_view_rms_px.len(), 5);
        assert_eq!(result.view_poses.len(), 5);
        assert_eq!(result.profile.image_width, 1280);
        assert_eq!(result.profile.image_height, 960);
        assert!(result.accepted_iterations > 0);
    }
}

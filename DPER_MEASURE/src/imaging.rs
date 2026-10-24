//! Local, non-destructive image derivation helpers.

use image::{Rgb, RgbImage};
use serde::Deserialize;

const MAX_OUTPUT_PIXELS: u64 = 12_000_000;
const MAX_STACK_INPUT_PIXELS: u64 = 30_000_000;

/// Camera distortion calibration in pixel coordinates.
/// Coefficients use normalized image coordinates and the OpenCV k1/k2/k3/p1/p2 model.
#[derive(Clone, Debug, Deserialize)]
pub struct RadialDistortionCalibration {
    pub image_width: u32,
    pub image_height: u32,
    pub fx_px: f64,
    pub fy_px: f64,
    pub cx_px: f64,
    pub cy_px: f64,
    pub k1: f64,
    #[serde(default)]
    pub k2: f64,
    #[serde(default)]
    pub k3: f64,
    #[serde(default)]
    pub p1: f64,
    #[serde(default)]
    pub p2: f64,
}

pub fn flat_field_correct(
    source: &RgbImage,
    flat_field: &RgbImage,
    dark_frame: Option<&RgbImage>,
) -> Result<RgbImage, String> {
    ensure_same_dimensions(source, flat_field)?;
    if let Some(dark) = dark_frame { ensure_same_dimensions(source, dark)?; }
    let mut means = [0.0_f64; 3];
    let pixel_count = (flat_field.width() as u64) * (flat_field.height() as u64);
    if pixel_count == 0 { return Err("Image flat-field vide.".into()); }
    if pixel_count > MAX_OUTPUT_PIXELS { return Err("Image trop grande pour la correction flat-field locale.".into()); }
    for (x, y, pixel) in flat_field.enumerate_pixels() {
        for channel in 0..3 {
            let dark = dark_frame.map(|frame| frame.get_pixel(x, y)[channel]).unwrap_or(0);
            means[channel] += pixel[channel].saturating_sub(dark) as f64;
        }
    }
    for mean in &mut means { *mean /= pixel_count as f64; }
    if means.iter().any(|mean| !mean.is_finite() || *mean < 2.0) {
        return Err("Le champ uniforme est trop sombre pour calculer une correction stable.".into());
    }

    let mut corrected = RgbImage::new(source.width(), source.height());
    for y in 0..source.height() {
        for x in 0..source.width() {
            let input = source.get_pixel(x, y);
            let flat = flat_field.get_pixel(x, y);
            let dark = dark_frame.map(|frame| frame.get_pixel(x, y));
            let mut output = [0_u8; 3];
            for channel in 0..3 {
                let dark_value = dark.map(|pixel| pixel[channel]).unwrap_or(0);
                let signal = input[channel].saturating_sub(dark_value) as f64;
                let local_flat = flat[channel].saturating_sub(dark_value) as f64;
                let gain = means[channel] / local_flat.max(2.0);
                output[channel] = (signal * gain).clamp(0.0, 255.0).round() as u8;
            }
            corrected.put_pixel(x, y, Rgb(output));
        }
    }
    Ok(corrected)
}

pub fn radial_distortion_correct(
    source: &RgbImage,
    calibration: &RadialDistortionCalibration,
) -> Result<RgbImage, String> {
    let (width, height) = source.dimensions();
    let _pixels = (width as u64).checked_mul(height as u64)
        .filter(|count| *count > 0 && *count <= MAX_OUTPUT_PIXELS)
        .ok_or_else(|| "Image trop grande pour la correction radiale locale.".to_owned())?;
    if calibration.image_width != width || calibration.image_height != height {
        return Err(format!("Le profil optique cible {}×{} px, mais l'image fait {}×{} px.",
            calibration.image_width, calibration.image_height, width, height));
    }
    let intrinsics = [calibration.fx_px, calibration.fy_px, calibration.cx_px, calibration.cy_px];
    let coefficients = [calibration.k1, calibration.k2, calibration.k3, calibration.p1, calibration.p2];
    if intrinsics.iter().any(|value| !value.is_finite())
        || calibration.fx_px <= 0.0 || calibration.fy_px <= 0.0
        || calibration.cx_px < 0.0 || calibration.cy_px < 0.0
        || calibration.cx_px >= width as f64 || calibration.cy_px >= height as f64
        || coefficients.iter().any(|value| !value.is_finite())
    {
        return Err("Le profil de distorsion contient des paramètres invalides.".into());
    }
    let max_x = (calibration.cx_px / calibration.fx_px)
        .max(((width - 1) as f64 - calibration.cx_px).abs() / calibration.fx_px);
    let max_y = (calibration.cy_px / calibration.fy_px)
        .max(((height - 1) as f64 - calibration.cy_px).abs() / calibration.fy_px);
    let max_radius_squared = max_x * max_x + max_y * max_y;
    if !max_radius_squared.is_finite() {
        return Err("Le rayon maximal du profil de distorsion n'est pas fini.".into());
    }
    let min_x = -calibration.cx_px / calibration.fx_px;
    let max_x = ((width - 1) as f64 - calibration.cx_px) / calibration.fx_px;
    let min_y = -calibration.cy_px / calibration.fy_px;
    let max_y = ((height - 1) as f64 - calibration.cy_px) / calibration.fy_px;
    for grid_y in 0..=16 {
        let y = min_y + (max_y - min_y) * grid_y as f64 / 16.0;
        for grid_x in 0..=16 {
            let x = min_x + (max_x - min_x) * grid_x as f64 / 16.0;
            let radius_squared = x * x + y * y;
            let radius_fourth = radius_squared * radius_squared;
            let radial_factor = 1.0 + calibration.k1 * radius_squared
                + calibration.k2 * radius_fourth + calibration.k3 * radius_fourth * radius_squared;
            let radial_slope = calibration.k1 + 2.0 * calibration.k2 * radius_squared
                + 3.0 * calibration.k3 * radius_fourth;
            let radial_derivative = 1.0 + 3.0 * calibration.k1 * radius_squared
                + 5.0 * calibration.k2 * radius_fourth
                + 7.0 * calibration.k3 * radius_fourth * radius_squared;
            let dr_dx = 2.0 * x * radial_slope;
            let dr_dy = 2.0 * y * radial_slope;
            let j00 = radial_factor + x * dr_dx + 2.0 * calibration.p1 * y + 6.0 * calibration.p2 * x;
            let j01 = x * dr_dy + 2.0 * calibration.p1 * x + 2.0 * calibration.p2 * y;
            let j10 = y * dr_dx + 2.0 * calibration.p1 * x + 2.0 * calibration.p2 * y;
            let j11 = radial_factor + y * dr_dy + 6.0 * calibration.p1 * y + 2.0 * calibration.p2 * x;
            let determinant = j00 * j11 - j01 * j10;
            if !radial_factor.is_finite() || !radial_derivative.is_finite()
                || !determinant.is_finite() || radial_factor <= 0.0
                || radial_derivative <= 0.0 || determinant <= 1e-8
            {
                return Err("Le profil radial/tangentiel n'est pas monotone sur l'image; vérifiez sa calibration.".into());
            }
        }
    }

    let mut output = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let normalized_x = (x as f64 - calibration.cx_px) / calibration.fx_px;
            let normalized_y = (y as f64 - calibration.cy_px) / calibration.fy_px;
            let radius_squared = normalized_x * normalized_x + normalized_y * normalized_y;
            let radius_fourth = radius_squared * radius_squared;
            let radial_factor = 1.0 + calibration.k1 * radius_squared
                + calibration.k2 * radius_fourth + calibration.k3 * radius_fourth * radius_squared;
            let distorted_x = normalized_x * radial_factor
                + 2.0 * calibration.p1 * normalized_x * normalized_y
                + calibration.p2 * (radius_squared + 2.0 * normalized_x * normalized_x);
            let distorted_y = normalized_y * radial_factor
                + calibration.p1 * (radius_squared + 2.0 * normalized_y * normalized_y)
                + 2.0 * calibration.p2 * normalized_x * normalized_y;
            let source_x = calibration.fx_px * distorted_x + calibration.cx_px;
            let source_y = calibration.fy_px * distorted_y + calibration.cy_px;
            let pixel = if source_x.is_finite() && source_y.is_finite()
                && source_x >= 0.0 && source_y >= 0.0
                && source_x <= (width - 1) as f64 && source_y <= (height - 1) as f64
            {
                bilinear_rgb(source, source_x, source_y)
            } else { Rgb([0, 0, 0]) };
            output.put_pixel(x, y, pixel);
        }
    }
    Ok(output)
}

pub fn focus_stack(images: &[RgbImage]) -> Result<RgbImage, String> {
    let Some(first) = images.first() else { return Err("Aucune image dans la pile de mise au point.".into()); };
    if images.len() < 2 { return Err("Sélectionnez au moins deux images pour l'empilement.".into()); }
    if images.len() > 32 { return Err("L'empilement est limité à 32 images par traitement.".into()); }
    for image in &images[1..] { ensure_same_dimensions(first, image)?; }
    let width = first.width() as usize;
    let height = first.height() as usize;
    let pixels = width.checked_mul(height)
        .filter(|count| *count as u64 <= MAX_OUTPUT_PIXELS)
        .ok_or_else(|| "Image trop grande pour l'empilement local.".to_owned())?;
    let input_pixels = (pixels as u64).saturating_mul(images.len() as u64);
    if input_pixels > MAX_STACK_INPUT_PIXELS {
        return Err("La pile dépasse la limite mémoire locale; réduisez le nombre d'images ou leur résolution.".into());
    }
    let mut sums = vec![[0.0_f32; 3]; pixels];
    let mut weights = vec![0.0_f32; pixels];
    let patch = 32_u32;
    let x_starts = patch_starts(first.width(), patch, 16);
    let y_starts = patch_starts(first.height(), patch, 16);

    for &top in &y_starts {
        for &left in &x_starts {
            let right = (left + patch).min(first.width());
            let bottom = (top + patch).min(first.height());
            let best = images.iter().enumerate()
                .max_by(|(_, a), (_, b)| patch_sharpness(a, left, top, right, bottom)
                    .total_cmp(&patch_sharpness(b, left, top, right, bottom)))
                .map(|(index, _)| index).unwrap_or(0);
            let winner = &images[best];
            for y in top..bottom {
                for x in left..right {
                    let edge_distance = (x - left + 1).min(right - x)
                        .min(y - top + 1).min(bottom - y).min(8) as f64;
                    let weight = edge_distance.max(1.0) as f32;
                    let index = y as usize * width + x as usize;
                    let pixel = winner.get_pixel(x, y);
                    for channel in 0..3 { sums[index][channel] += pixel[channel] as f32 * weight; }
                    weights[index] += weight;
                }
            }
        }
    }
    let mut output = RgbImage::new(first.width(), first.height());
    for y in 0..first.height() {
        for x in 0..first.width() {
            let index = y as usize * width + x as usize;
            let mut pixel = [0_u8; 3];
            for channel in 0..3 {
                pixel[channel] = if weights[index] > 0.0 {
                    (sums[index][channel] / weights[index]).round().clamp(0.0, 255.0) as u8
                } else { first.get_pixel(x, y)[channel] };
            }
            output.put_pixel(x, y, Rgb(pixel));
        }
    }
    Ok(output)
}

pub fn manual_panorama(
    first: &RgbImage,
    second: &RgbImage,
    offset_x: i32,
    offset_y: i32,
) -> Result<RgbImage, String> {
    compose_panorama(&[first, second], &[(0, 0), (offset_x, offset_y)])
}

/// Composes 2-32 source images at absolute offsets relative to the first image.
pub fn panorama_mosaic(images: &[RgbImage], offsets: &[(i32, i32)]) -> Result<RgbImage, String> {
    let references = images.iter().collect::<Vec<_>>();
    compose_panorama(&references, offsets)
}

/// Composes 2-32 images using source-to-panorama projective transforms.
/// The returned origin is the integer source-coordinate origin represented by output pixel 0,0.
pub fn panorama_mosaic_projective(
    images: &[RgbImage],
    source_to_panorama: &[[f64; 9]],
) -> Result<(RgbImage, [f64; 2]), String> {
    if !(2..=32).contains(&images.len()) {
        return Err("Un panorama local doit contenir entre 2 et 32 images.".into());
    }
    if source_to_panorama.len() != images.len() {
        return Err("Fournissez une homographie pour chaque image du panorama.".into());
    }
    let total_input_pixels = images.iter().try_fold(0_u64, |total, image| {
        (image.width() as u64).checked_mul(image.height() as u64)
            .and_then(|pixels| total.checked_add(pixels))
    });
    if images.iter().any(|image| image.width() == 0 || image.height() == 0)
        || total_input_pixels.is_none_or(|pixels| pixels > MAX_STACK_INPUT_PIXELS)
    {
        return Err("Les images du panorama sont vides ou dépassent la limite mémoire locale.".into());
    }

    let mut bounds = Vec::with_capacity(images.len());
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (image, transform) in images.iter().zip(source_to_panorama) {
        if transform.iter().any(|value| !value.is_finite()) {
            return Err("Une homographie du panorama contient une valeur non finie.".into());
        }
        let corners = [
            (0.0, 0.0),
            (image.width().saturating_sub(1) as f64, 0.0),
            (0.0, image.height().saturating_sub(1) as f64),
            (image.width().saturating_sub(1) as f64, image.height().saturating_sub(1) as f64),
        ];
        let mut denominator_sign = 0.0_f64;
        let mut projected = Vec::with_capacity(4);
        for corner in corners {
            let denominator = transform[6] * corner.0 + transform[7] * corner.1 + transform[8];
            if !denominator.is_finite() || denominator.abs() < 1e-10 {
                return Err("Une image du panorama traverse l'horizon de son homographie.".into());
            }
            if denominator_sign == 0.0 { denominator_sign = denominator.signum(); }
            else if denominator.signum() != denominator_sign {
                return Err("Une image du panorama traverse l'horizon de son homographie.".into());
            }
            let Some(point) = project_homography(transform, corner) else {
                return Err("Les coordonnées projetées du panorama sont invalides.".into());
            };
            if point.0.abs() > 100_000.0 || point.1.abs() > 100_000.0 {
                return Err("Une image du panorama est projetée trop loin de l'origine.".into());
            }
            min_x = min_x.min(point.0);
            min_y = min_y.min(point.1);
            max_x = max_x.max(point.0);
            max_y = max_y.max(point.1);
            projected.push(point);
        }
        let inverse = invert_homography(transform)
            .ok_or_else(|| "Une homographie du panorama est singulière.".to_owned())?;
        bounds.push((projected, inverse));
    }

    let origin_x = min_x.floor();
    let origin_y = min_y.floor();
    let right = max_x.ceil();
    let bottom = max_y.ceil();
    let width = u32::try_from((right - origin_x + 1.0) as u64)
        .map_err(|_| "Largeur du panorama invalide.".to_owned())?;
    let height = u32::try_from((bottom - origin_y + 1.0) as u64)
        .map_err(|_| "Hauteur du panorama invalide.".to_owned())?;
    let pixels = (width as u64).checked_mul(height as u64)
        .filter(|count| *count > 0 && *count <= MAX_OUTPUT_PIXELS)
        .ok_or_else(|| "Panorama trop grand pour le traitement local.".to_owned())?;
    let work = pixels.checked_mul(images.len() as u64)
        .filter(|count| *count <= 120_000_000)
        .ok_or_else(|| "Le recalage projectif du panorama dépasse la limite de calcul locale.".to_owned())?;
    let _ = work;
    let pixels = pixels as usize;
    let mut sums = vec![[0.0_f32; 3]; pixels];
    let mut weights = vec![0.0_f32; pixels];

    for (image, (projected, inverse)) in images.iter().zip(&bounds) {
        let left = (projected.iter().map(|point| point.0).fold(f64::INFINITY, f64::min)
            .floor() - origin_x).max(0.0) as u32;
        let top = (projected.iter().map(|point| point.1).fold(f64::INFINITY, f64::min)
            .floor() - origin_y).max(0.0) as u32;
        let right = (projected.iter().map(|point| point.0).fold(f64::NEG_INFINITY, f64::max)
            .ceil() - origin_x).max(0.0).min(width.saturating_sub(1) as f64) as u32;
        let bottom = (projected.iter().map(|point| point.1).fold(f64::NEG_INFINITY, f64::max)
            .ceil() - origin_y).max(0.0).min(height.saturating_sub(1) as f64) as u32;
        if left > right || top > bottom { continue; }
        for canvas_y in top..=bottom {
            for canvas_x in left..=right {
                let source_coordinate = (canvas_x as f64 + origin_x, canvas_y as f64 + origin_y);
                let Some((source_x, source_y)) = project_homography(inverse, source_coordinate) else {
                    continue;
                };
                if source_x < 0.0 || source_y < 0.0
                    || source_x > image.width().saturating_sub(1) as f64
                    || source_y > image.height().saturating_sub(1) as f64
                { continue; }
                let edge = (source_x + 1.0).min(image.width() as f64 - source_x)
                    .min(source_y + 1.0).min(image.height() as f64 - source_y)
                    .min(64.0).max(1.0) as f32;
                let pixel = bilinear_rgb(image, source_x, source_y);
                let index = canvas_y as usize * width as usize + canvas_x as usize;
                for channel in 0..3 { sums[index][channel] += pixel[channel] as f32 * edge; }
                weights[index] += edge;
            }
        }
    }

    let mut output = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let index = y as usize * width as usize + x as usize;
            let mut pixel = [0_u8; 3];
            for channel in 0..3 {
                pixel[channel] = (sums[index][channel] / weights[index].max(1.0))
                    .round().clamp(0.0, 255.0) as u8;
            }
            output.put_pixel(x, y, Rgb(pixel));
        }
    }
    Ok((output, [origin_x, origin_y]))
}

pub fn project_homography(transform: &[f64; 9], point: (f64, f64)) -> Option<(f64, f64)> {
    let denominator = transform[6] * point.0 + transform[7] * point.1 + transform[8];
    if !denominator.is_finite() || denominator.abs() < 1e-12 { return None; }
    let x = (transform[0] * point.0 + transform[1] * point.1 + transform[2]) / denominator;
    let y = (transform[3] * point.0 + transform[4] * point.1 + transform[5]) / denominator;
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

pub fn multiply_homographies(left: &[f64; 9], right: &[f64; 9]) -> Option<[f64; 9]> {
    let mut result = [0.0_f64; 9];
    for row in 0..3 {
        for column in 0..3 {
            for inner in 0..3 {
                result[row * 3 + column] += left[row * 3 + inner] * right[inner * 3 + column];
            }
        }
    }
    if result.iter().any(|value| !value.is_finite()) { return None; }
    let divisor = if result[8].abs() > 1e-12 {
        result[8]
    } else {
        result.iter().map(|value| value.abs()).fold(0.0_f64, f64::max)
    };
    if !divisor.is_finite() || divisor.abs() < 1e-15 { return None; }
    for value in &mut result { *value /= divisor; }
    Some(result)
}

fn invert_homography(matrix: &[f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = *matrix;
    let cofactors = [
        e * i - f * h, c * h - b * i, b * f - c * e,
        f * g - d * i, a * i - c * g, c * d - a * f,
        d * h - e * g, b * g - a * h, a * e - b * d,
    ];
    let determinant = a * cofactors[0] + b * cofactors[3] + c * cofactors[6];
    if !determinant.is_finite() || determinant.abs() < 1e-15 { return None; }
    let inverse = cofactors.map(|value| value / determinant);
    inverse.iter().all(|value| value.is_finite()).then_some(inverse)
}

fn compose_panorama(images: &[&RgbImage], offsets: &[(i32, i32)]) -> Result<RgbImage, String> {
    if !(2..=32).contains(&images.len()) {
        return Err("Un panorama local doit contenir entre 2 et 32 images.".into());
    }
    if offsets.len() != images.len() {
        return Err("Fournissez un décalage pour chaque image du panorama.".into());
    }
    let total_input_pixels = images.iter().try_fold(0_u64, |total, image| {
        (image.width() as u64).checked_mul(image.height() as u64)
            .and_then(|pixels| total.checked_add(pixels))
    });
    if images.iter().any(|image| image.width() == 0 || image.height() == 0)
        || total_input_pixels.is_none_or(|pixels| pixels > MAX_STACK_INPUT_PIXELS)
    {
        return Err("Les images du panorama sont vides ou dépassent la limite mémoire locale.".into());
    }
    let left = offsets.iter().map(|(x, _)| *x as i64).min().unwrap_or(0);
    let top = offsets.iter().map(|(_, y)| *y as i64).min().unwrap_or(0);
    let right = images.iter().zip(offsets).map(|(image, (x, _))| *x as i64 + image.width() as i64)
        .max().unwrap_or(0);
    let bottom = images.iter().zip(offsets).map(|(image, (_, y))| *y as i64 + image.height() as i64)
        .max().unwrap_or(0);
    let width = u32::try_from(right - left).map_err(|_| "Largeur du panorama invalide.".to_owned())?;
    let height = u32::try_from(bottom - top).map_err(|_| "Hauteur du panorama invalide.".to_owned())?;
    let pixels = (width as u64).checked_mul(height as u64)
        .filter(|count| *count > 0 && *count <= MAX_OUTPUT_PIXELS)
        .ok_or_else(|| "Panorama trop grand pour le traitement local.".to_owned())? as usize;
    let mut sums = vec![[0.0_f32; 3]; pixels];
    let mut weights = vec![0.0_f32; pixels];
    for (image, (offset_x, offset_y)) in images.iter().zip(offsets) {
        accumulate_panorama(image, *offset_x as i64, *offset_y as i64, left, top, width,
            &mut sums, &mut weights);
    }
    let mut output = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let index = y as usize * width as usize + x as usize;
            let mut pixel = [0_u8; 3];
            for channel in 0..3 {
                pixel[channel] = (sums[index][channel] / weights[index].max(1.0))
                    .round().clamp(0.0, 255.0) as u8;
            }
            output.put_pixel(x, y, Rgb(pixel));
        }
    }
    Ok(output)
}

pub fn rectify_planar(source: &RgbImage, corners: [(f64, f64); 4]) -> Result<(RgbImage, [f64; 8]), String> {
    let (source_width, source_height) = source.dimensions();
    if source_width < 2 || source_height < 2 { return Err("Image source trop petite pour la rectification.".into()); }
    if corners.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()
        || *x < 0.0 || *y < 0.0 || *x > (source_width - 1) as f64 || *y > (source_height - 1) as f64)
    { return Err("Les quatre points doivent rester dans l'image source.".into()); }
    let mut cross_sign = 0.0_f64;
    for index in 0..4 {
        let a = corners[index];
        let b = corners[(index + 1) % 4];
        let c = corners[(index + 2) % 4];
        let cross = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0);
        if cross.abs() < 1.0 { return Err("Les points doivent former un quadrilatère convexe non dégénéré.".into()); }
        if cross_sign == 0.0 { cross_sign = cross.signum(); }
        else if cross.signum() != cross_sign { return Err("Ordre invalide : cliquez les coins dans le sens horaire ou antihoraire, sans croisement.".into()); }
    }
    let distance = |a: (f64, f64), b: (f64, f64)| ((a.0-b.0).powi(2)+(a.1-b.1).powi(2)).sqrt();
    // The selected corners use pixel-center coordinates. A span from x=0 to x=2
    // therefore contains three output pixels, just like an image crop with both
    // endpoints included.
    let width = ((distance(corners[0], corners[1]) + distance(corners[3], corners[2])) / 2.0)
        .round().clamp(1.0, (u32::MAX - 1) as f64) as u32 + 1;
    let height = ((distance(corners[0], corners[3]) + distance(corners[1], corners[2])) / 2.0)
        .round().clamp(1.0, (u32::MAX - 1) as f64) as u32 + 1;
    let output_pixels = (width as u64).checked_mul(height as u64)
        .filter(|count| *count > 0 && *count <= MAX_OUTPUT_PIXELS)
        .ok_or_else(|| "Image rectifiée trop grande pour le traitement local.".to_owned())?;
    let _ = output_pixels;
    let target = [
        (0.0, 0.0), (width.saturating_sub(1) as f64, 0.0),
        (width.saturating_sub(1) as f64, height.saturating_sub(1) as f64),
        (0.0, height.saturating_sub(1) as f64),
    ];
    let homography = solve_homography(target, corners)?;
    let mut output = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let denominator = homography[6] * x as f64 + homography[7] * y as f64 + 1.0;
            if denominator.abs() < 1e-12 { continue; }
            let source_x = (homography[0] * x as f64 + homography[1] * y as f64 + homography[2]) / denominator;
            let source_y = (homography[3] * x as f64 + homography[4] * y as f64 + homography[5]) / denominator;
            if source_x >= 0.0 && source_y >= 0.0
                && source_x <= (source_width - 1) as f64 && source_y <= (source_height - 1) as f64
            { output.put_pixel(x, y, bilinear_rgb(source, source_x, source_y)); }
        }
    }
    Ok((output, homography))
}

fn solve_homography(source_points: [(f64, f64); 4], target_points: [(f64, f64); 4]) -> Result<[f64; 8], String> {
    let mut matrix = [[0.0_f64; 9]; 8];
    for index in 0..4 {
        let (x, y) = source_points[index];
        let (u, v) = target_points[index];
        matrix[index * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -u*x, -u*y, u];
        matrix[index * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v*x, -v*y, v];
    }
    for column in 0..8 {
        let pivot = (column..8).max_by(|a, b| matrix[*a][column].abs().total_cmp(&matrix[*b][column].abs()))
            .ok_or_else(|| "Homographie dégénérée.".to_owned())?;
        if matrix[pivot][column].abs() < 1e-10 { return Err("Homographie dégénérée : points trop proches ou alignés.".into()); }
        matrix.swap(column, pivot);
        let divisor = matrix[column][column];
        for cell in column..=8 { matrix[column][cell] /= divisor; }
        for row in 0..8 {
            if row == column { continue; }
            let factor = matrix[row][column];
            for cell in column..=8 { matrix[row][cell] -= factor * matrix[column][cell]; }
        }
    }
    let mut result = [0.0_f64; 8];
    for index in 0..8 { result[index] = matrix[index][8]; }
    Ok(result)
}

fn bilinear_rgb(image: &RgbImage, x: f64, y: f64) -> Rgb<u8> {
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let tx = x - x0 as f64;
    let ty = y - y0 as f64;
    let p00 = image.get_pixel(x0, y0);
    let p10 = image.get_pixel(x1, y0);
    let p01 = image.get_pixel(x0, y1);
    let p11 = image.get_pixel(x1, y1);
    let mut rgb = [0_u8; 3];
    for channel in 0..3 {
        let top = p00[channel] as f64 * (1.0 - tx) + p10[channel] as f64 * tx;
        let bottom = p01[channel] as f64 * (1.0 - tx) + p11[channel] as f64 * tx;
        rgb[channel] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
    }
    Rgb(rgb)
}

pub fn estimate_translation(first: &RgbImage, second: &RgbImage) -> Option<(i32, i32, f64)> {
    if first.dimensions() != second.dimensions() || first.width() < 24 || first.height() < 24 {
        return None;
    }
    let coarse_first = downsample_gray(first, 96, 72);
    let coarse_second = downsample_gray(second, 96, 72);
    let coarse = search_translation(&coarse_first, &coarse_second, None, 2)?;
    let medium_first = downsample_gray(first, 256, 192);
    let medium_second = downsample_gray(second, 256, 192);
    let center = (
        (coarse.0 as f64 * medium_first.width as f64 / coarse_first.width as f64).round() as i32,
        (coarse.1 as f64 * medium_first.height as f64 / coarse_first.height as f64).round() as i32,
    );
    let refined = search_translation(&medium_first, &medium_second, Some((center, 8)), 2)?;
    let offset_x = (refined.0 as f64 * first.width() as f64 / medium_first.width as f64).round();
    let offset_y = (refined.1 as f64 * first.height() as f64 / medium_first.height as f64).round();
    Some((offset_x as i32, offset_y as i32, refined.2))
}

struct GrayImage { width: u32, height: u32, pixels: Vec<f32> }

fn downsample_gray(image: &RgbImage, max_width: u32, max_height: u32) -> GrayImage {
    let scale = (max_width as f64 / image.width() as f64)
        .min(max_height as f64 / image.height() as f64).min(1.0);
    let width = ((image.width() as f64 * scale).round() as u32).max(1);
    let height = ((image.height() as f64 * scale).round() as u32).max(1);
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        let source_y = ((y as u64 * image.height() as u64) / height as u64) as u32;
        for x in 0..width {
            let source_x = ((x as u64 * image.width() as u64) / width as u64) as u32;
            pixels.push(luminance(image.get_pixel(source_x, source_y)) as f32);
        }
    }
    GrayImage { width, height, pixels }
}

fn search_translation(
    first: &GrayImage,
    second: &GrayImage,
    around: Option<((i32, i32), i32)>,
    sample_step: u32,
) -> Option<(i32, i32, f64)> {
    if first.width != second.width || first.height != second.height { return None; }
    let min_overlap_x = (first.width / 10).max(8).min(first.width - 2) as i32;
    let min_overlap_y = (first.height / 10).max(8).min(first.height - 2) as i32;
    let (x_min, x_max, y_min, y_max) = if let Some(((center_x, center_y), radius)) = around {
        (center_x.saturating_sub(radius).max(-(first.width as i32 - min_overlap_x)),
         center_x.saturating_add(radius).min(first.width as i32 - min_overlap_x),
         center_y.saturating_sub(radius).max(-(first.height as i32 - min_overlap_y)),
         center_y.saturating_add(radius).min(first.height as i32 - min_overlap_y))
    } else {
        (-(first.width as i32 - min_overlap_x), first.width as i32 - min_overlap_x,
         -(first.height as i32 - min_overlap_y), first.height as i32 - min_overlap_y)
    };
    let mut best: Option<(i32, i32, f64)> = None;
    let mut second_best = f64::NEG_INFINITY;
    for dy in y_min..=y_max {
        for dx in x_min..=x_max {
            let Some(score) = gradient_correlation(first, second, dx, dy, sample_step) else { continue; };
            if best.is_some_and(|(best_x, best_y, _)| (best_x - dx).abs() <= 2 && (best_y - dy).abs() <= 2) {
                continue;
            }
            if best.is_none_or(|(_, _, best_score)| score > best_score) {
                if let Some((_, _, old_score)) = best { second_best = second_best.max(old_score); }
                best = Some((dx, dy, score));
            } else {
                second_best = second_best.max(score);
            }
        }
    }
    let (dx, dy, score) = best?;
    if score < 0.25 { return None; }
    let distinctiveness = if second_best.is_finite() { (score - second_best).max(0.0) } else { 0.0 };
    if around.is_none() && distinctiveness < 0.004 { return None; }
    Some((dx, dy, (score * (1.0 - (-distinctiveness * 12.0).exp())).clamp(0.0, 1.0)))
}

fn gradient_correlation(
    first: &GrayImage,
    second: &GrayImage,
    offset_x: i32,
    offset_y: i32,
    step: u32,
) -> Option<f64> {
    let start_x = 1_i32.max(offset_x + 1);
    let end_x = (first.width as i32 - 1).min(offset_x + second.width as i32 - 1);
    let start_y = 1_i32.max(offset_y + 1);
    let end_y = (first.height as i32 - 1).min(offset_y + second.height as i32 - 1);
    if end_x - start_x < 4 || end_y - start_y < 4 { return None; }
    let stride = step.max(1) as i32;
    let mut dot = 0.0_f64;
    let mut norm_a = 0.0_f64;
    let mut norm_b = 0.0_f64;
    let mut count = 0_u64;
    for y in (start_y..end_y).step_by(stride as usize) {
        for x in (start_x..end_x).step_by(stride as usize) {
            let x2 = (x - offset_x) as u32;
            let y2 = (y - offset_y) as u32;
            let gx_a = first.pixels[y as usize * first.width as usize + (x + 1) as usize]
                - first.pixels[y as usize * first.width as usize + (x - 1) as usize];
            let gy_a = first.pixels[(y + 1) as usize * first.width as usize + x as usize]
                - first.pixels[(y - 1) as usize * first.width as usize + x as usize];
            let gx_b = second.pixels[y2 as usize * second.width as usize + (x2 + 1) as usize]
                - second.pixels[y2 as usize * second.width as usize + (x2 - 1) as usize];
            let gy_b = second.pixels[(y2 + 1) as usize * second.width as usize + x2 as usize]
                - second.pixels[(y2 - 1) as usize * second.width as usize + x2 as usize];
            dot += (gx_a * gx_b + gy_a * gy_b) as f64;
            norm_a += (gx_a * gx_a + gy_a * gy_a) as f64;
            norm_b += (gx_b * gx_b + gy_b * gy_b) as f64;
            count += 1;
        }
    }
    if count < 16 || norm_a < 1e-6 || norm_b < 1e-6 { return None; }
    Some(dot / (norm_a * norm_b).sqrt())
}

fn ensure_same_dimensions(first: &RgbImage, second: &RgbImage) -> Result<(), String> {
    if first.dimensions() != second.dimensions() {
        return Err(format!("Dimensions incompatibles : {} × {} et {} × {} pixels.",
            first.width(), first.height(), second.width(), second.height()));
    }
    Ok(())
}

fn patch_starts(length: u32, patch: u32, stride: u32) -> Vec<u32> {
    if length <= patch { return vec![0]; }
    let mut starts = Vec::new();
    let last = length - patch;
    let mut position = 0;
    while position < last {
        starts.push(position);
        position = position.saturating_add(stride);
    }
    if starts.last().copied() != Some(last) { starts.push(last); }
    starts
}

fn patch_sharpness(image: &RgbImage, left: u32, top: u32, right: u32, bottom: u32) -> f64 {
    if right.saturating_sub(left) < 3 || bottom.saturating_sub(top) < 3 { return 0.0; }
    let mut sum = 0.0;
    let mut squared = 0.0;
    let mut count = 0_u64;
    for y in top + 1..bottom - 1 {
        for x in left + 1..right - 1 {
            let center = luminance(image.get_pixel(x, y));
            let laplacian = luminance(image.get_pixel(x - 1, y))
                + luminance(image.get_pixel(x + 1, y))
                + luminance(image.get_pixel(x, y - 1))
                + luminance(image.get_pixel(x, y + 1)) - 4.0 * center;
            sum += laplacian;
            squared += laplacian * laplacian;
            count += 1;
        }
    }
    if count == 0 { return 0.0; }
    let mean = sum / count as f64;
    (squared / count as f64 - mean * mean).max(0.0)
}

fn luminance(pixel: &Rgb<u8>) -> f64 {
    0.2126 * pixel[0] as f64 + 0.7152 * pixel[1] as f64 + 0.0722 * pixel[2] as f64
}

fn accumulate_panorama(
    image: &RgbImage,
    offset_x: i64,
    offset_y: i64,
    left: i64,
    top: i64,
    canvas_width: u32,
    sums: &mut [[f32; 3]],
    weights: &mut [f32],
) {
    for y in 0..image.height() {
        for x in 0..image.width() {
            let canvas_x = (offset_x + x as i64 - left) as usize;
            let canvas_y = (offset_y + y as i64 - top) as usize;
            let index = canvas_y * canvas_width as usize + canvas_x;
            let edge = (x + 1).min(image.width() - x)
                .min(y + 1).min(image.height() - y).min(64) as f32;
            let pixel = image.get_pixel(x, y);
            for channel in 0..3 { sums[index][channel] += pixel[channel] as f32 * edge; }
            weights[index] += edge;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectification_keeps_both_selected_pixel_edges() {
        let mut source = RgbImage::new(3, 3);
        for y in 0..3 {
            for x in 0..3 {
                source.put_pixel(x, y, Rgb([(x + 10 * y) as u8, 0, 0]));
            }
        }

        let (rectified, homography) = rectify_planar(
            &source, [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)],
        ).unwrap();

        assert_eq!(rectified.dimensions(), (3, 3));
        assert_eq!(rectified, source);
        assert_eq!(project_homography(&[
            homography[0], homography[1], homography[2], homography[3],
            homography[4], homography[5], homography[6], homography[7], 1.0,
        ], (2.0, 2.0)), Some((2.0, 2.0)));
    }

    #[test]
    fn flat_field_correction_subtracts_dark_frame_without_mutating_source() {
        let source = RgbImage::from_raw(2, 1, vec![110, 110, 110, 110, 110, 110]).unwrap();
        let flat = RgbImage::from_raw(2, 1, vec![60, 60, 60, 110, 110, 110]).unwrap();
        let dark = RgbImage::from_raw(2, 1, vec![10, 10, 10, 10, 10, 10]).unwrap();
        let original = source.clone();

        let corrected = flat_field_correct(&source, &flat, Some(&dark)).unwrap();

        assert_eq!(corrected.get_pixel(0, 0), &Rgb([150, 150, 150]));
        assert_eq!(corrected.get_pixel(1, 0), &Rgb([75, 75, 75]));
        assert_eq!(source, original);
    }

    #[test]
    fn radial_identity_profile_preserves_pixels_and_stack_prefers_the_sharper_frame() {
        let source = RgbImage::from_fn(3, 3, |x, y| {
            let value = (x + y * 3) as u8 * 17;
            Rgb([value, value, value])
        });
        let profile = RadialDistortionCalibration {
            image_width: 3, image_height: 3, fx_px: 2.0, fy_px: 2.0,
            cx_px: 1.0, cy_px: 1.0, k1: 0.0, k2: 0.0, k3: 0.0, p1: 0.0, p2: 0.0,
        };
        assert_eq!(radial_distortion_correct(&source, &profile).unwrap(), source);

        let sharp = RgbImage::from_fn(32, 32, |x, y| {
            if (x + y) % 2 == 0 { Rgb([0, 0, 0]) } else { Rgb([255, 255, 255]) }
        });
        let blurred = RgbImage::from_pixel(32, 32, Rgb([128, 128, 128]));
        assert_eq!(focus_stack(&[blurred, sharp.clone()]).unwrap(), sharp);
    }

    #[test]
    fn manual_panorama_handles_negative_origins_and_focus_stack_rejects_bad_inputs() {
        let first = RgbImage::from_pixel(2, 2, Rgb([220, 0, 0]));
        let second = RgbImage::from_pixel(2, 2, Rgb([0, 0, 220]));
        let panorama = manual_panorama(&first, &second, 2, -1).unwrap();
        assert_eq!(panorama.dimensions(), (4, 3));
        assert_eq!(panorama.get_pixel(0, 1), &Rgb([220, 0, 0]));
        assert_eq!(panorama.get_pixel(2, 0), &Rgb([0, 0, 220]));

        assert!(focus_stack(&[]).unwrap_err().contains("Aucune image"));
        assert!(focus_stack(std::slice::from_ref(&first)).unwrap_err().contains("au moins deux"));
        assert!(focus_stack(&[first.clone(), RgbImage::new(3, 2)]).unwrap_err()
            .contains("Dimensions incompatibles"));
        let too_many = vec![first; 33];
        assert!(focus_stack(&too_many).unwrap_err().contains("32 images"));
    }

    #[test]
    fn projective_panorama_preserves_translated_image_extent_and_origin() {
        let first = RgbImage::from_pixel(2, 2, Rgb([10, 20, 30]));
        let second = RgbImage::from_pixel(2, 2, Rgb([40, 50, 60]));
        let transforms = [
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 2.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        ];

        let (panorama, origin) = panorama_mosaic_projective(&[first, second], &transforms).unwrap();

        assert_eq!(origin, [0.0, 0.0]);
        assert_eq!(panorama.dimensions(), (4, 2));
        assert_eq!(panorama.get_pixel(0, 0), &Rgb([10, 20, 30]));
        assert_eq!(panorama.get_pixel(3, 0), &Rgb([40, 50, 60]));
    }
}

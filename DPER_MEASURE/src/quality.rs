//! Contrôle de qualité d'image et corrections d'affichage non destructives.
//!
//! Les seuils par défaut servent uniquement au dépistage logiciel. Ils n'ont
//! pas été qualifiés sur un microscope, un capteur ou un procédé particulier.

use serde::{Deserialize, Serialize};

pub(crate) const QUALITY_ALGORITHM_VERSION: u32 = 1;

/// Seuils de dépistage, à qualifier pour chaque configuration optique.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageQualityThresholds {
    /// Luminance moyenne minimale, sur une échelle 0–255.
    pub min_mean_luminance: f64,
    /// Luminance moyenne maximale, sur une échelle 0–255.
    pub max_mean_luminance: f64,
    /// Écart-type minimal de luminance, utilisé comme indicateur de contraste.
    pub min_contrast_std_dev: f64,
    /// Variance minimale du Laplacien, utilisée comme indicateur de netteté.
    pub min_laplacian_variance: f64,
    /// Fraction minimale de pixels révélant un gradient suffisamment marqué.
    pub min_edge_visibility_fraction: f64,
    /// Fraction maximale de pixels proches du noir ou du blanc saturé.
    pub max_clipped_pixel_fraction: f64,
    /// Intensité minimale d'un gradient Sobel, sur une échelle 0–255.
    pub edge_gradient_threshold: f64,
}

impl Default for ImageQualityThresholds {
    fn default() -> Self {
        Self {
            min_mean_luminance: 20.0,
            max_mean_luminance: 235.0,
            min_contrast_std_dev: 18.0,
            min_laplacian_variance: 12.0,
            min_edge_visibility_fraction: 0.02,
            max_clipped_pixel_fraction: 0.35,
            edge_gradient_threshold: 20.0,
        }
    }
}

impl ImageQualityThresholds {
    fn validate(&self) -> Result<(), &'static str> {
        let finite = [
            self.min_mean_luminance,
            self.max_mean_luminance,
            self.min_contrast_std_dev,
            self.min_laplacian_variance,
            self.min_edge_visibility_fraction,
            self.max_clipped_pixel_fraction,
            self.edge_gradient_threshold,
        ].iter().all(|value| value.is_finite());
        if !finite
            || !(0.0..=255.0).contains(&self.min_mean_luminance)
            || !(0.0..=255.0).contains(&self.max_mean_luminance)
            || self.min_mean_luminance >= self.max_mean_luminance
            || self.min_contrast_std_dev < 0.0
            || self.min_laplacian_variance < 0.0
            || !(0.0..=1.0).contains(&self.min_edge_visibility_fraction)
            || !(0.0..=1.0).contains(&self.max_clipped_pixel_fraction)
            || !(0.0..=1020.0).contains(&self.edge_gradient_threshold)
        {
            return Err("Seuils de qualité d'image invalides");
        }
        Ok(())
    }
}

/// Indicateurs calculés depuis les pixels source avec le réglage non destructif mémorisé.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageQualityMetrics {
    pub mean_luminance: f64,
    pub dark_pixel_fraction: f64,
    pub bright_pixel_fraction: f64,
    pub clipped_pixel_fraction: f64,
    pub contrast_std_dev: f64,
    pub laplacian_variance: f64,
    pub edge_visibility_fraction: f64,
}

/// Résultat conservable dans une session et réutilisable comme garde de verdict.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageQualityAssessment {
    pub algorithm_version: u32,
    pub image_dimensions: [u32; 2],
    /// `true` indique seulement que l'image franchit les seuils de dépistage.
    /// Ce champ ne constitue ni une validation métrologique ni une qualification.
    pub is_exploitable: bool,
    pub status: ImageQualityStatus,
    pub issues: Vec<ImageQualityIssue>,
    pub metrics: ImageQualityMetrics,
    /// Copie des seuils utilisés pour conserver la traçabilité du résultat.
    pub thresholds: ImageQualityThresholds,
    /// Correction non destructive appliquée au dépistage lors du calcul.
    #[serde(default)]
    pub display_correction: DisplayCorrection,
}

impl ImageQualityAssessment {
    pub fn matches_context(
        &self,
        thresholds: &ImageQualityThresholds,
        display_correction: DisplayCorrection,
    ) -> bool {
        self.algorithm_version == QUALITY_ALGORITHM_VERSION
            && quality_thresholds_signature(&self.thresholds) == quality_thresholds_signature(thresholds)
            && correction_signature(self.display_correction) == correction_signature(display_correction)
    }
}

fn quality_thresholds_signature(thresholds: &ImageQualityThresholds) -> [u64; 7] {
    [
        thresholds.min_mean_luminance.to_bits(),
        thresholds.max_mean_luminance.to_bits(),
        thresholds.min_contrast_std_dev.to_bits(),
        thresholds.min_laplacian_variance.to_bits(),
        thresholds.min_edge_visibility_fraction.to_bits(),
        thresholds.max_clipped_pixel_fraction.to_bits(),
        thresholds.edge_gradient_threshold.to_bits(),
    ]
}

fn correction_signature(correction: DisplayCorrection) -> (u32, u32, u32) {
    (correction.brightness.to_bits(), correction.contrast.to_bits(), correction.gamma.to_bits())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeSnapResult {
    pub point: crate::geometry::Point,
    pub gradient_strength: f64,
    /// Grayscale samples at -2, -1, 0, +1 and +2 px along the detected edge normal.
    pub contrast_profile: [f64; 5],
}

/// Inspect a local search region, return the strongest edge and its normal gray profile.
pub fn analyze_edge_subpixel(
    rgb: &[u8], width: u32, height: u32, point: crate::geometry::Point,
    search_radius: u32, minimum_gradient: f64,
) -> Option<EdgeSnapResult> {
    use crate::geometry::Point;
    if !point.x.is_finite() || !point.y.is_finite() || width < 3 || height < 3
        || rgb.len() != width as usize * height as usize * 3
        || !minimum_gradient.is_finite() || minimum_gradient < 0.0
    { return None; }
    let gray = |x: i32, y: i32| -> f64 {
        let index = (y as usize * width as usize + x as usize) * 3;
        0.299 * rgb[index] as f64 + 0.587 * rgb[index + 1] as f64 + 0.114 * rgb[index + 2] as f64
    };
    let sample_gray = |point: Point| -> Option<f64> {
        if !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0
            || point.x > width as f64 - 1.0 || point.y > height as f64 - 1.0
        { return None; }
        let x0 = point.x.floor() as i32;
        let y0 = point.y.floor() as i32;
        let x1 = (x0 + 1).min(width as i32 - 1);
        let y1 = (y0 + 1).min(height as i32 - 1);
        let fx = point.x - x0 as f64;
        let fy = point.y - y0 as f64;
        let top = gray(x0, y0) * (1.0 - fx) + gray(x1, y0) * fx;
        let bottom = gray(x0, y1) * (1.0 - fx) + gray(x1, y1) * fx;
        Some(top * (1.0 - fy) + bottom * fy)
    };
    let gradient = |x: i32, y: i32| -> Option<(f64, f64, f64)> {
        if x < 1 || y < 1 || x >= width as i32 - 1 || y >= height as i32 - 1 { return None; }
        let gx = -gray(x - 1, y - 1) + gray(x + 1, y - 1)
            - 2.0 * gray(x - 1, y) + 2.0 * gray(x + 1, y)
            - gray(x - 1, y + 1) + gray(x + 1, y + 1);
        let gy = -gray(x - 1, y - 1) - 2.0 * gray(x, y - 1) - gray(x + 1, y - 1)
            + gray(x - 1, y + 1) + 2.0 * gray(x, y + 1) + gray(x + 1, y + 1);
        Some((gx, gy, gx.hypot(gy)))
    };
    let nearest_x = point.x.round() as i32;
    let nearest_y = point.y.round() as i32;
    let radius = search_radius.min(32) as i32;
    let mut best: Option<(i32, i32, f64)> = None;
    let point_distance_squared = |x: i32, y: i32| {
        (x as f64 - point.x).powi(2) + (y as f64 - point.y).powi(2)
    };
    for y in (nearest_y - radius).max(1)..=(nearest_y + radius).min(height as i32 - 2) {
        for x in (nearest_x - radius).max(1)..=(nearest_x + radius).min(width as i32 - 2) {
            let Some((_, _, strength)) = gradient(x, y) else { continue; };
            let should_replace = best.is_none_or(|(best_x, best_y, best_strength)| {
                strength > best_strength
                    || (strength == best_strength
                        && point_distance_squared(x, y) < point_distance_squared(best_x, best_y))
            });
            if should_replace {
                best = Some((x, y, strength));
            }
        }
    }
    let (x, y, strength) = best?;
    if strength < minimum_gradient { return None; }
    let (gx, gy, _) = gradient(x, y)?;
    let norm = gx.hypot(gy);
    if norm <= f64::EPSILON { return None; }
    let normal = Point { x: gx / norm, y: gy / norm };
    let magnitude_at = |sample: Point| -> f64 {
        let x0 = sample.x.floor() as i32;
        let y0 = sample.y.floor() as i32;
        let fx = sample.x - x0 as f64;
        let fy = sample.y - y0 as f64;
        let at = |px: i32, py: i32| gradient(px, py).map(|value| value.2).unwrap_or(0.0);
        let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
        let bottom = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
        top * (1.0 - fy) + bottom * fy
    };
    let center = Point { x: x as f64, y: y as f64 };
    let before = magnitude_at(Point { x: center.x - normal.x, y: center.y - normal.y });
    let after = magnitude_at(Point { x: center.x + normal.x, y: center.y + normal.y });
    let curvature = before - 2.0 * strength + after;
    let offset = if curvature.abs() > 1e-9 {
        (0.5 * (before - after) / curvature).clamp(-0.75, 0.75)
    } else { 0.0 };
    let snapped = Point { x: center.x + normal.x * offset, y: center.y + normal.y * offset };
    if snapped.dist(point) > search_radius as f64 + 0.75 { return None; }
    let mut contrast_profile = [0.0; 5];
    for (index, sample_offset) in (-2..=2).enumerate() {
        contrast_profile[index] = sample_gray(Point {
            x: snapped.x + normal.x * sample_offset as f64,
            y: snapped.y + normal.y * sample_offset as f64,
        })?;
    }
    Some(EdgeSnapResult { point: snapped, gradient_strength: strength, contrast_profile })
}

/// Convenience wrapper for callers that only need the corrected coordinate.
pub fn snap_to_edge_subpixel(
    rgb: &[u8], width: u32, height: u32, point: crate::geometry::Point,
    search_radius: u32, minimum_gradient: f64,
) -> Option<crate::geometry::Point> {
    analyze_edge_subpixel(rgb, width, height, point, search_radius, minimum_gradient)
        .map(|result| result.point)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageQualityStatus {
    WithinProvisionalLimits,
    NonExploitable,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageQualityIssue {
    Underexposed,
    Overexposed,
    LowContrast,
    InsufficientSharpness,
    PoorEdgeVisibility,
    ExcessiveClipping,
}

impl ImageQualityIssue {
    pub fn label(self) -> &'static str {
        match self {
            Self::Underexposed => "Sous-exposition",
            Self::Overexposed => "Surexposition",
            Self::LowContrast => "Contraste insuffisant",
            Self::InsufficientSharpness => "Netteté insuffisante",
            Self::PoorEdgeVisibility => "Contours peu visibles",
            Self::ExcessiveClipping => "Pixels saturés trop nombreux",
        }
    }
}

/// Évalue la source RGB. Un statut `NonExploitable` doit empêcher toute
/// validation de mesure ou de conformité dans les couches appelantes.
pub fn assess_image_quality(
    rgb: &[u8],
    width: u32,
    height: u32,
    thresholds: &ImageQualityThresholds,
) -> Result<ImageQualityAssessment, &'static str> {
    assess_image_quality_with_correction(
        rgb, width, height, thresholds, DisplayCorrection::default(),
    )
}

/// Évalue la même correction non destructive que l'aperçu sans créer de buffer RGB.
pub fn assess_image_quality_with_correction(
    rgb: &[u8],
    width: u32,
    height: u32,
    thresholds: &ImageQualityThresholds,
    correction: DisplayCorrection,
) -> Result<ImageQualityAssessment, &'static str> {
    thresholds.validate()?;
    validate_display_correction(correction)?;
    if width < 3 || height < 3 {
        return Err("Image trop petite pour évaluer la netteté et les contours");
    }
    let pixel_count = (width as usize).checked_mul(height as usize)
        .ok_or("Dimensions d'image trop grandes")?;
    let expected_bytes = pixel_count.checked_mul(3)
        .ok_or("Dimensions d'image trop grandes")?;
    if rgb.len() != expected_bytes {
        return Err("Taille du buffer RGB incompatible avec les dimensions");
    }

    let mut dark_pixels = 0usize;
    let mut bright_pixels = 0usize;
    let mut clipped_pixels = 0usize;
    let mut luminance_sum = 0.0;
    for pixel in rgb.chunks_exact(3) {
        let value = luminance(pixel, correction);
        luminance_sum += value;
        if value <= 20.0 { dark_pixels += 1; }
        if value >= 235.0 { bright_pixels += 1; }
        if (0..3).any(|channel_index| {
            let channel = corrected_channel(pixel[channel_index], correction);
            channel <= 1 || channel >= 254
        }) {
            clipped_pixels += 1;
        }
    }
    let mean_luminance = luminance_sum / pixel_count as f64;
    let variance_sum = rgb.chunks_exact(3).map(|pixel| {
        (luminance(pixel, correction) - mean_luminance).powi(2)
    }).sum::<f64>();
    let contrast_std_dev = (variance_sum / pixel_count as f64).sqrt();

    let image_width = width as usize;
    let image_height = height as usize;
    let mut laplacian_count = 0usize;
    let mut laplacian_mean = 0.0;
    let mut laplacian_m2 = 0.0;
    let mut visible_edges = 0usize;
    for y in 1..image_height - 1 {
        for x in 1..image_width - 1 {
            let index = y * image_width + x;
            let center = luminance_at(rgb, index, correction);
            let top = luminance_at(rgb, index - image_width, correction);
            let bottom = luminance_at(rgb, index + image_width, correction);
            let left = luminance_at(rgb, index - 1, correction);
            let right = luminance_at(rgb, index + 1, correction);
            let laplacian = top + bottom + left + right - 4.0 * center;
            laplacian_count += 1;
            let delta = laplacian - laplacian_mean;
            laplacian_mean += delta / laplacian_count as f64;
            laplacian_m2 += delta * (laplacian - laplacian_mean);

            let top_left = luminance_at(rgb, index - image_width - 1, correction);
            let top_right = luminance_at(rgb, index - image_width + 1, correction);
            let bottom_left = luminance_at(rgb, index + image_width - 1, correction);
            let bottom_right = luminance_at(rgb, index + image_width + 1, correction);
            let gradient_x = -top_left + top_right - 2.0 * left + 2.0 * right
                - bottom_left + bottom_right;
            let gradient_y = -top_left - 2.0 * top - top_right
                + bottom_left + 2.0 * bottom + bottom_right;
            let gradient_magnitude = gradient_x.hypot(gradient_y) * 0.25;
            if gradient_magnitude >= thresholds.edge_gradient_threshold {
                visible_edges += 1;
            }
        }
    }
    let laplacian_variance = laplacian_m2 / laplacian_count as f64;
    let interior_pixels = laplacian_count as f64;
    let metrics = ImageQualityMetrics {
        mean_luminance,
        dark_pixel_fraction: dark_pixels as f64 / pixel_count as f64,
        bright_pixel_fraction: bright_pixels as f64 / pixel_count as f64,
        clipped_pixel_fraction: clipped_pixels as f64 / pixel_count as f64,
        contrast_std_dev,
        laplacian_variance,
        edge_visibility_fraction: visible_edges as f64 / interior_pixels,
    };

    let mut issues = Vec::new();
    if mean_luminance < thresholds.min_mean_luminance { issues.push(ImageQualityIssue::Underexposed); }
    if mean_luminance > thresholds.max_mean_luminance { issues.push(ImageQualityIssue::Overexposed); }
    if contrast_std_dev < thresholds.min_contrast_std_dev { issues.push(ImageQualityIssue::LowContrast); }
    if laplacian_variance < thresholds.min_laplacian_variance {
        issues.push(ImageQualityIssue::InsufficientSharpness);
    }
    if metrics.edge_visibility_fraction < thresholds.min_edge_visibility_fraction {
        issues.push(ImageQualityIssue::PoorEdgeVisibility);
    }
    if metrics.clipped_pixel_fraction > thresholds.max_clipped_pixel_fraction {
        issues.push(ImageQualityIssue::ExcessiveClipping);
    }
    let is_exploitable = issues.is_empty();
    Ok(ImageQualityAssessment {
        algorithm_version: QUALITY_ALGORITHM_VERSION,
        image_dimensions: [width, height],
        is_exploitable,
        status: if is_exploitable {
            ImageQualityStatus::WithinProvisionalLimits
        } else {
            ImageQualityStatus::NonExploitable
        },
        issues,
        metrics,
        thresholds: thresholds.clone(),
        display_correction: correction,
    })
}

fn luminance(pixel: &[u8], correction: DisplayCorrection) -> f64 {
    0.2126 * corrected_channel(pixel[0], correction) as f64
        + 0.7152 * corrected_channel(pixel[1], correction) as f64
        + 0.0722 * corrected_channel(pixel[2], correction) as f64
}

fn luminance_at(rgb: &[u8], pixel_index: usize, correction: DisplayCorrection) -> f64 {
    let offset = pixel_index * 3;
    luminance(&rgb[offset..offset + 3], correction)
}

/// Réglages d'affichage. La fonction ci-dessous renvoie toujours un buffer
/// distinct et ne modifie jamais les pixels servant aux mesures.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayCorrection {
    /// Décalage de luminance dans l'espace RGB encodé, en unités 0–255.
    pub brightness: f32,
    /// Contraste autour du gris médian (1.0 = inchangé).
    pub contrast: f32,
    /// Exposant gamma (1.0 = inchangé).
    pub gamma: f32,
}

impl Default for DisplayCorrection {
    fn default() -> Self { Self { brightness: 0.0, contrast: 1.0, gamma: 1.0 } }
}

pub fn corrected_display_rgb(
    source_rgb: &[u8],
    correction: DisplayCorrection,
) -> Result<Vec<u8>, &'static str> {
    validate_display_correction(correction)?;
    let mut corrected = Vec::with_capacity(source_rgb.len());
    corrected.extend(source_rgb.iter().map(|channel| corrected_channel(*channel, correction)));
    Ok(corrected)
}

fn validate_display_correction(correction: DisplayCorrection) -> Result<(), &'static str> {
    if !correction.brightness.is_finite()
        || !correction.contrast.is_finite()
        || !correction.gamma.is_finite()
        || !(-100.0..=100.0).contains(&correction.brightness)
        || !(0.0..=4.0).contains(&correction.contrast)
        || !(0.2..=5.0).contains(&correction.gamma)
    {
        return Err("Correction d'affichage invalide");
    }
    Ok(())
}

fn corrected_channel(channel: u8, correction: DisplayCorrection) -> u8 {
    let linear_adjusted = ((channel as f32 - 127.5) * correction.contrast
        + 127.5 + correction.brightness).clamp(0.0, 255.0) / 255.0;
    (linear_adjusted.powf(correction.gamma) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_image_fails_contrast_and_edge_quality_gates() {
        let image = vec![128; 16 * 16 * 3];
        let assessment = assess_image_quality(
            &image, 16, 16, &ImageQualityThresholds::default(),
        ).unwrap();

        assert!(!assessment.is_exploitable);
        assert!(assessment.issues.contains(&ImageQualityIssue::LowContrast));
        assert!(assessment.issues.contains(&ImageQualityIssue::PoorEdgeVisibility));
    }

    #[test]
    fn quality_assessment_preserves_source_and_records_its_context() {
        let mut image = Vec::with_capacity(16 * 16 * 3);
        for y in 0..16 {
            for x in 0..16 {
                let value = if (x + y) % 2 == 0 { 16 } else { 240 };
                image.extend_from_slice(&[value, value, value]);
            }
        }
        let source_before = image.clone();
        let thresholds = ImageQualityThresholds {
            min_mean_luminance: 0.0,
            max_mean_luminance: 255.0,
            min_contrast_std_dev: 0.0,
            min_laplacian_variance: 0.0,
            min_edge_visibility_fraction: 0.0,
            max_clipped_pixel_fraction: 1.0,
            edge_gradient_threshold: 0.0,
        };
        let correction = DisplayCorrection { brightness: 4.0, contrast: 1.1, gamma: 0.9 };
        let assessment = assess_image_quality_with_correction(
            &image, 16, 16, &thresholds, correction,
        ).unwrap();

        assert!(assessment.is_exploitable);
        assert_eq!(image, source_before);
        assert!(assessment.matches_context(&thresholds, correction));
        let mut changed_thresholds = thresholds.clone();
        changed_thresholds.min_contrast_std_dev = 1.0;
        assert!(!assessment.matches_context(&changed_thresholds, correction));
        assert!(!assessment.matches_context(&thresholds, DisplayCorrection::default()));
    }

    #[test]
    fn subpixel_edge_analysis_returns_local_profile_and_rejects_invalid_input() {
        use crate::geometry::Point;

        let mut image = vec![0_u8; 16 * 16 * 3];
        for y in 0..16 {
            for x in 0..16 {
                let value = if x < 8 { 0 } else { 255 };
                let index = (y * 16 + x) * 3;
                image[index..index + 3].fill(value);
            }
        }

        let result = analyze_edge_subpixel(
            &image, 16, 16, Point { x: 6.0, y: 8.0 }, 5, 20.0,
        ).unwrap();

        assert!(result.gradient_strength >= 20.0);
        assert!((result.point.x - 7.5).abs() <= 0.1);
        assert!((result.point.y - 8.0).abs() <= 0.1);
        assert!(result.contrast_profile[0] < result.contrast_profile[4]);
        assert!(analyze_edge_subpixel(
            &image, 16, 16, Point { x: f64::NAN, y: 8.0 }, 5, 20.0,
        ).is_none());
        assert!(analyze_edge_subpixel(&image[..image.len() - 1], 16, 16,
            Point { x: 6.0, y: 8.0 }, 5, 20.0).is_none());
    }
}

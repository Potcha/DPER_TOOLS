//! Bounded local feature matching for panorama alignment.
//!
//! This module has no dependency on an external vision library. It detects
//! FAST corners at three image scales, builds rotation-normalized BRIEF
//! descriptors, matches them with a reciprocal ratio test, then estimates a
//! source-to-target homography with deterministic RANSAC.

use image::{imageops, GrayImage, RgbImage};

const MAX_ANALYSIS_PIXELS: u64 = 1_500_000;
const MAX_ANALYSIS_SIDE: u32 = 1_280;
const MAX_INPUT_IMAGE_PIXELS: u64 = 12_000_000;
const MAX_TOTAL_INPUT_PIXELS: u64 = 30_000_000;
const MAX_FEATURES_PER_LEVEL: usize = 320;
const MAX_FEATURES: usize = 960;
const FAST_THRESHOLD: i16 = 18;
const FAST_MARGIN: u32 = 18;
const FAST_CELL_SIZE: u32 = 32;
const FAST_FEATURES_PER_CELL: usize = 4;
const BRIEF_BITS: usize = 256;
const BRIEF_WORDS: usize = BRIEF_BITS / 64;
const MATCH_MAX_DISTANCE: u32 = 104;
const RATIO_NUMERATOR: u32 = 78;
const RATIO_DENOMINATOR: u32 = 100;
const MIN_MATCHES: usize = 10;
const MIN_INLIERS: usize = 8;
const MIN_INLIER_RATIO: f64 = 0.35;
const RANSAC_ITERATIONS: usize = 1_600;
const INLIER_THRESHOLD_ANALYSIS_PX: f64 = 3.5;

#[derive(Clone, Debug)]
pub struct HomographyEstimate {
    /// Homogeneous 3×3 matrix, row-major, mapping source pixels to target pixels.
    pub source_to_target: [f64; 9],
    /// Number of reciprocal descriptor matches retained as geometric inliers.
    pub inliers: usize,
    /// Number of reciprocal descriptor matches tested by RANSAC.
    pub matches: usize,
    /// Root-mean-square forward reprojection error in target image pixels.
    pub rms_error_px: f64,
}

/// Estimates a homography mapping coordinates in `source` into `target`.
///
/// The analysis image is capped at 1.5 million pixels and 1280 pixels on its
/// longest side. Feature, descriptor, and RANSAC work is bounded independently
/// of the input dimensions. Weak, clustered, or geometrically inconsistent
/// correspondences return an explanatory error instead of a transform.
pub fn estimate_homography(
    source: &RgbImage,
    target: &RgbImage,
) -> Result<HomographyEstimate, String> {
    let source_pixels = (source.width() as u64).checked_mul(source.height() as u64);
    let target_pixels = (target.width() as u64).checked_mul(target.height() as u64);
    let total_input_pixels = source_pixels.and_then(|source_pixels| {
        target_pixels.and_then(|target_pixels| source_pixels.checked_add(target_pixels))
    });
    if source_pixels.is_none_or(|pixels| pixels == 0 || pixels > MAX_INPUT_IMAGE_PIXELS)
        || target_pixels.is_none_or(|pixels| pixels == 0 || pixels > MAX_INPUT_IMAGE_PIXELS)
        || total_input_pixels.is_none_or(|pixels| pixels > MAX_TOTAL_INPUT_PIXELS)
    {
        return Err("Les images sont vides ou dépassent le budget mémoire d'entrée du panorama.".into());
    }
    let source_gray = gray_for_analysis(source)?;
    let target_gray = gray_for_analysis(target)?;
    let source_scale = NativeScale::new(source, &source_gray);
    let target_scale = NativeScale::new(target, &target_gray);

    let source_features = extract_features(&source_gray, source_scale);
    let target_features = extract_features(&target_gray, target_scale);
    if source_features.len() < MIN_MATCHES || target_features.len() < MIN_MATCHES {
        return Err("Pas assez de coins FAST fiables dans les deux images pour aligner le panorama.".into());
    }

    let matches = reciprocal_ratio_matches(&source_features, &target_features);
    if matches.len() < MIN_MATCHES {
        return Err(format!(
            "Appariement panorama insuffisant: {} correspondances réciproques, {} requises.",
            matches.len(), MIN_MATCHES
        ));
    }

    let target_threshold = (INLIER_THRESHOLD_ANALYSIS_PX * target_scale.max_axis_px_per_analysis)
        .max(2.5);
    let mut rng = DeterministicRng::new(
        source.width() as u64
            ^ ((source.height() as u64) << 7)
            ^ ((target.width() as u64) << 19)
            ^ ((target.height() as u64) << 31)
            ^ matches.len() as u64,
    );
    let mut best_model: Option<[f64; 9]> = None;
    let mut best_inliers = Vec::new();
    let mut best_squared_error = f64::INFINITY;

    if matches.len() < 4 {
        return Err("Quatre correspondances sont nécessaires pour estimer une homographie.".into());
    }

    for _ in 0..RANSAC_ITERATIONS {
        let sample = sample_four_distinct(matches.len(), &mut rng);
        let points = sample.map(|index| matched_points(&matches[index], &source_features, &target_features));
        if !sample_is_spatially_stable(&points, source, target) {
            continue;
        }
        let Some(model) = fit_homography(&points, source, target) else { continue; };
        if !homography_is_bounded(&model, source, target) { continue; }

        let mut inliers = Vec::new();
        let mut squared_error = 0.0;
        for (index, matched) in matches.iter().enumerate() {
            let (source_point, target_point) = matched_points(matched, &source_features, &target_features);
            if let Some(error) = reprojection_error(&model, source_point, target_point) {
                if error <= target_threshold {
                    inliers.push(index);
                    squared_error += error * error;
                }
            }
        }
        if inliers.len() > best_inliers.len()
            || (inliers.len() == best_inliers.len() && squared_error < best_squared_error)
        {
            best_squared_error = squared_error;
            best_inliers = inliers;
            best_model = Some(model);
        }
    }

    let Some(mut model) = best_model else {
        return Err("Aucune homographie stable n'a été trouvée par RANSAC.".into());
    };
    if best_inliers.len() < MIN_INLIERS {
        return Err(format!(
            "Alignement panorama trop faible: {} inliers, {} requis.",
            best_inliers.len(), MIN_INLIERS
        ));
    }

    let inlier_ratio = best_inliers.len() as f64 / matches.len() as f64;
    if inlier_ratio < MIN_INLIER_RATIO {
        return Err(format!(
            "Alignement panorama ambigu: seulement {:.0}% des correspondances sont cohérentes.",
            inlier_ratio * 100.0
        ));
    }
    if !inliers_cover_image(&best_inliers, &matches, &source_features, &target_features, source, target) {
        return Err("Les correspondances panorama sont trop concentrées pour une homographie fiable.".into());
    }

    for _ in 0..2 {
        let inlier_points = best_inliers.iter().map(|&index| {
            matched_points(&matches[index], &source_features, &target_features)
        }).collect::<Vec<_>>();
        let Some(refined) = fit_homography(&inlier_points, source, target) else { break; };
        if !homography_is_bounded(&refined, source, target) { break; }
        model = refined;
        best_inliers.clear();
        best_squared_error = 0.0;
        for (index, matched) in matches.iter().enumerate() {
            let (source_point, target_point) = matched_points(matched, &source_features, &target_features);
            if let Some(error) = reprojection_error(&model, source_point, target_point) {
                if error <= target_threshold {
                    best_inliers.push(index);
                    best_squared_error += error * error;
                }
            }
        }
    }

    if best_inliers.len() < MIN_INLIERS {
        return Err("L'affinage de l'homographie a perdu trop de correspondances cohérentes.".into());
    }
    let inlier_ratio = best_inliers.len() as f64 / matches.len() as f64;
    if inlier_ratio < MIN_INLIER_RATIO {
        return Err("L'affinage de l'homographie produit trop peu d'inliers.".into());
    }
    if !inliers_cover_image(&best_inliers, &matches, &source_features, &target_features, source, target) {
        return Err("L'homographie affinée repose sur une zone d'image trop petite.".into());
    }

    let rms_error_px = (best_squared_error / best_inliers.len() as f64).sqrt();
    if !rms_error_px.is_finite() || rms_error_px > target_threshold * 0.8 {
        return Err(format!(
            "Erreur d'alignement trop élevée ({:.2} px RMS). Vérifiez le recouvrement des images.",
            rms_error_px
        ));
    }

    normalize_homography(&mut model);
    Ok(HomographyEstimate {
        source_to_target: model,
        inliers: best_inliers.len(),
        matches: matches.len(),
        rms_error_px,
    })
}

#[derive(Clone, Copy)]
struct NativeScale {
    x_px_per_analysis: f64,
    y_px_per_analysis: f64,
    max_axis_px_per_analysis: f64,
}

impl NativeScale {
    fn new(original: &RgbImage, analysis: &GrayImage) -> Self {
        let x_px_per_analysis = original.width() as f64 / analysis.width().max(1) as f64;
        let y_px_per_analysis = original.height() as f64 / analysis.height().max(1) as f64;
        Self {
            x_px_per_analysis,
            y_px_per_analysis,
            max_axis_px_per_analysis: x_px_per_analysis.max(y_px_per_analysis),
        }
    }
}

#[derive(Clone)]
struct Feature {
    x: f64,
    y: f64,
    descriptor: [u64; BRIEF_WORDS],
}

#[derive(Clone, Copy)]
struct Candidate {
    x: u32,
    y: u32,
    score: u8,
}

fn gray_for_analysis(source: &RgbImage) -> Result<GrayImage, String> {
    let (width, height) = source.dimensions();
    let pixels = (width as u64).checked_mul(height as u64)
        .filter(|count| *count > 0)
        .ok_or_else(|| "Image source vide ou dimensions invalides pour l'alignement.".to_owned())?;
    let max_side = width.max(height).max(1) as f64;
    let scale = (MAX_ANALYSIS_SIDE as f64 / max_side)
        .min((MAX_ANALYSIS_PIXELS as f64 / pixels as f64).sqrt())
        .min(1.0);
    let analysis_width = ((width as f64 * scale).floor() as u32).clamp(1, MAX_ANALYSIS_SIDE);
    let analysis_height = ((height as f64 * scale).floor() as u32).clamp(1, MAX_ANALYSIS_SIDE);
    let count = (analysis_width as u64).checked_mul(analysis_height as u64)
        .filter(|count| *count <= MAX_ANALYSIS_PIXELS)
        .ok_or_else(|| "L'image dépasse le budget mémoire d'analyse du panorama.".to_owned())?;
    let mut gray = GrayImage::new(analysis_width, analysis_height);
    for y in 0..analysis_height {
        let source_y = (((y as f64 + 0.5) * height as f64 / analysis_height as f64).floor() as u32)
            .min(height - 1);
        for x in 0..analysis_width {
            let source_x = (((x as f64 + 0.5) * width as f64 / analysis_width as f64).floor() as u32)
                .min(width - 1);
            let pixel = source.get_pixel(source_x, source_y);
            let luma = (77_u16 * pixel[0] as u16 + 150_u16 * pixel[1] as u16
                + 29_u16 * pixel[2] as u16 + 128) >> 8;
            gray.put_pixel(x, y, image::Luma([luma.min(255) as u8]));
        }
    }
    debug_assert_eq!(count, gray.width() as u64 * gray.height() as u64);
    Ok(gray)
}

fn extract_features(base: &GrayImage, native_scale: NativeScale) -> Vec<Feature> {
    let mut features = Vec::with_capacity(MAX_FEATURES);
    let pattern = brief_pattern();
    for factor in [1.0_f32, 0.72, 0.52] {
        if features.len() >= MAX_FEATURES { break; }
        let level = if factor == 1.0 {
            None
        } else {
            let width = ((base.width() as f32 * factor).round() as u32).max(1);
            let height = ((base.height() as f32 * factor).round() as u32).max(1);
            if width < FAST_MARGIN * 2 + 1 || height < FAST_MARGIN * 2 + 1 { continue; }
            Some(imageops::resize(base, width, height, imageops::FilterType::Triangle))
        };
        let image = level.as_ref().unwrap_or(base);
        let scale_x = image.width() as f64 / base.width().max(1) as f64;
        let scale_y = image.height() as f64 / base.height().max(1) as f64;
        let remaining = MAX_FEATURES.saturating_sub(features.len());
        let level_limit = remaining.min(MAX_FEATURES_PER_LEVEL);
        for candidate in fast_candidates(image, level_limit) {
            let orientation = intensity_centroid_orientation(image, candidate.x, candidate.y);
            let descriptor = brief_descriptor(image, candidate.x, candidate.y, orientation, &pattern);
            let base_x = candidate.x as f64 / scale_x;
            let base_y = candidate.y as f64 / scale_y;
            features.push(Feature {
                x: base_x * native_scale.x_px_per_analysis,
                y: base_y * native_scale.y_px_per_analysis,
                descriptor,
            });
        }
    }
    features
}

fn fast_candidates(image: &GrayImage, limit: usize) -> Vec<Candidate> {
    let (width, height) = image.dimensions();
    if limit == 0 || width <= FAST_MARGIN * 2 || height <= FAST_MARGIN * 2 { return Vec::new(); }
    let Ok(pixel_count) = usize::try_from(width as u64 * height as u64) else { return Vec::new(); };
    let mut scores = vec![0_u8; pixel_count];
    let row_width = width as usize;
    for y in FAST_MARGIN..height - FAST_MARGIN {
        for x in FAST_MARGIN..width - FAST_MARGIN {
            let score = fast_corner_score(image, x, y);
            if score > 0 { scores[y as usize * row_width + x as usize] = score; }
        }
    }

    let columns = width.div_ceil(FAST_CELL_SIZE) as usize;
    let rows = height.div_ceil(FAST_CELL_SIZE) as usize;
    let mut cells = vec![Vec::<Candidate>::new(); columns.saturating_mul(rows)];
    for y in FAST_MARGIN..height - FAST_MARGIN {
        for x in FAST_MARGIN..width - FAST_MARGIN {
            let score = scores[y as usize * row_width + x as usize];
            if score == 0 || !is_local_maximum(&scores, row_width, x, y, score) { continue; }
            let cell_index = (y / FAST_CELL_SIZE) as usize * columns + (x / FAST_CELL_SIZE) as usize;
            let cell = &mut cells[cell_index];
            if cell.iter().any(|point| {
                let dx = point.x.abs_diff(x);
                let dy = point.y.abs_diff(y);
                dx * dx + dy * dy < 64
            }) {
                continue;
            }
            cell.push(Candidate { x, y, score });
            cell.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.y.cmp(&b.y)).then_with(|| a.x.cmp(&b.x)));
            cell.truncate(FAST_FEATURES_PER_CELL);
        }
    }

    let mut selected = Vec::with_capacity(limit);
    let mut rank = 0;
    while selected.len() < limit {
        let mut added = false;
        for cell in &cells {
            if let Some(candidate) = cell.get(rank) {
                selected.push(*candidate);
                added = true;
                if selected.len() == limit { break; }
            }
        }
        if !added { break; }
        rank += 1;
    }
    selected
}

const FAST_CIRCLE: [(i32, i32); 16] = [
    (0, -3), (1, -3), (2, -2), (3, -1), (3, 0), (3, 1), (2, 2), (1, 3),
    (0, 3), (-1, 3), (-2, 2), (-3, 1), (-3, 0), (-3, -1), (-2, -2), (-1, -3),
];

fn fast_corner_score(image: &GrayImage, x: u32, y: u32) -> u8 {
    let center = image.get_pixel(x, y)[0] as i16;
    let ring = FAST_CIRCLE.map(|(dx, dy)| {
        image.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32)[0] as i16 - center
    });
    let bright_count = ring.iter().filter(|&&value| value > FAST_THRESHOLD).count();
    let dark_count = ring.iter().filter(|&&value| value < -FAST_THRESHOLD).count();
    if bright_count < 9 && dark_count < 9 { return 0; }

    let mut best = 0_i16;
    for polarity in [-1_i16, 1_i16] {
        for start in 0..16 {
            let mut minimum = i16::MAX;
            let mut contiguous = true;
            for step in 0..9 {
                let contrast = ring[(start + step) & 15] * polarity;
                if contrast <= FAST_THRESHOLD { contiguous = false; break; }
                minimum = minimum.min(contrast);
            }
            if contiguous { best = best.max(minimum); }
        }
    }
    best.clamp(0, 255) as u8
}

fn is_local_maximum(scores: &[u8], width: usize, x: u32, y: u32, score: u8) -> bool {
    let index = y as usize * width + x as usize;
    for dy in -1_i32..=1 {
        for dx in -1_i32..=1 {
            if dx == 0 && dy == 0 { continue; }
            let neighbor_index = (y as i32 + dy) as usize * width + (x as i32 + dx) as usize;
            let neighbor = scores[neighbor_index];
            if neighbor > score || (neighbor == score && neighbor_index < index) { return false; }
        }
    }
    true
}

fn intensity_centroid_orientation(image: &GrayImage, x: u32, y: u32) -> f64 {
    let mut moment_x = 0.0_f64;
    let mut moment_y = 0.0_f64;
    for dy in -8_i32..=8 {
        for dx in -8_i32..=8 {
            if dx * dx + dy * dy > 64 { continue; }
            let value = image.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32)[0] as f64;
            moment_x += dx as f64 * value;
            moment_y += dy as f64 * value;
        }
    }
    moment_y.atan2(moment_x)
}

#[derive(Clone, Copy)]
struct BriefPair { ax: i8, ay: i8, bx: i8, by: i8 }

fn brief_pattern() -> [BriefPair; BRIEF_BITS] {
    fn next(state: &mut u32) -> u32 {
        *state ^= *state << 13;
        *state ^= *state >> 17;
        *state ^= *state << 5;
        *state
    }
    fn point(state: &mut u32) -> (i8, i8) {
        loop {
            let x = (next(state) % 25) as i32 - 12;
            let y = (next(state) % 25) as i32 - 12;
            if x * x + y * y <= 144 { return (x as i8, y as i8); }
        }
    }
    let mut state = 0x6D2B_79F5_u32;
    let mut pattern = [BriefPair { ax: 0, ay: 0, bx: 1, by: 0 }; BRIEF_BITS];
    for pair in &mut pattern {
        let (ax, ay) = point(&mut state);
        let mut second = point(&mut state);
        while second == (ax, ay) { second = point(&mut state); }
        *pair = BriefPair { ax, ay, bx: second.0, by: second.1 };
    }
    pattern
}

fn brief_descriptor(
    image: &GrayImage,
    x: u32,
    y: u32,
    angle: f64,
    pattern: &[BriefPair; BRIEF_BITS],
) -> [u64; BRIEF_WORDS] {
    let cosine = angle.cos();
    let sine = angle.sin();
    let mut descriptor = [0_u64; BRIEF_WORDS];
    for (bit, pair) in pattern.iter().enumerate() {
        let (ax, ay) = rotate_offset(pair.ax, pair.ay, cosine, sine);
        let (bx, by) = rotate_offset(pair.bx, pair.by, cosine, sine);
        let a = image.get_pixel((x as i32 + ax) as u32, (y as i32 + ay) as u32)[0];
        let b = image.get_pixel((x as i32 + bx) as u32, (y as i32 + by) as u32)[0];
        if a < b { descriptor[bit / 64] |= 1_u64 << (bit % 64); }
    }
    descriptor
}

fn rotate_offset(x: i8, y: i8, cosine: f64, sine: f64) -> (i32, i32) {
    let x = x as f64;
    let y = y as f64;
    ((x * cosine - y * sine).round() as i32, (x * sine + y * cosine).round() as i32)
}

#[derive(Clone, Copy)]
struct Match { source: usize, target: usize }

fn reciprocal_ratio_matches(source: &[Feature], target: &[Feature]) -> Vec<Match> {
    let mut source_best = vec![(usize::MAX, u32::MAX, u32::MAX); source.len()];
    let mut target_best = vec![(usize::MAX, u32::MAX, u32::MAX); target.len()];
    for (source_index, source_feature) in source.iter().enumerate() {
        for (target_index, target_feature) in target.iter().enumerate() {
            let distance = hamming_distance(&source_feature.descriptor, &target_feature.descriptor);
            update_best(&mut source_best[source_index], target_index, distance);
            update_best(&mut target_best[target_index], source_index, distance);
        }
    }
    let mut matches = Vec::new();
    for (source_index, &(target_index, best, second)) in source_best.iter().enumerate() {
        if target_index == usize::MAX || best > MATCH_MAX_DISTANCE
            || best.saturating_mul(RATIO_DENOMINATOR) >= second.saturating_mul(RATIO_NUMERATOR)
        {
            continue;
        }
        let (reverse_source, reverse_best, reverse_second) = target_best[target_index];
        if reverse_source == source_index && reverse_best <= MATCH_MAX_DISTANCE
            && reverse_best.saturating_mul(RATIO_DENOMINATOR) < reverse_second.saturating_mul(RATIO_NUMERATOR)
        {
            matches.push(Match { source: source_index, target: target_index });
        }
    }
    matches
}

fn update_best(best: &mut (usize, u32, u32), candidate_index: usize, distance: u32) {
    if distance < best.1 {
        best.2 = best.1;
        best.1 = distance;
        best.0 = candidate_index;
    } else if distance < best.2 {
        best.2 = distance;
    }
}

fn hamming_distance(a: &[u64; BRIEF_WORDS], b: &[u64; BRIEF_WORDS]) -> u32 {
    a.iter().zip(b).map(|(a, b)| (a ^ b).count_ones()).sum()
}

fn matched_points(matched: &Match, source: &[Feature], target: &[Feature]) -> ((f64, f64), (f64, f64)) {
    ((source[matched.source].x, source[matched.source].y),
        (target[matched.target].x, target[matched.target].y))
}

fn sample_four_distinct(count: usize, rng: &mut DeterministicRng) -> [usize; 4] {
    let mut sample = [0_usize; 4];
    for index in 0..4 {
        loop {
            let candidate = rng.next() as usize % count;
            if !sample[..index].contains(&candidate) {
                sample[index] = candidate;
                break;
            }
        }
    }
    sample
}

fn sample_is_spatially_stable(
    points: &[((f64, f64), (f64, f64)); 4],
    source: &RgbImage,
    target: &RgbImage,
) -> bool {
    let source_norm = Normalizer::new(source.width(), source.height());
    let target_norm = Normalizer::new(target.width(), target.height());
    let source_points = points.map(|(source_point, _)| source_norm.normalize(source_point));
    let target_points = points.map(|(_, target_point)| target_norm.normalize(target_point));
    let source_area = maximum_triangle_area(&source_points);
    let target_area = maximum_triangle_area(&target_points);
    source_area > 0.0025 && target_area > 0.0025
}

#[derive(Clone, Copy)]
struct Normalizer { center_x: f64, center_y: f64, scale: f64 }

impl Normalizer {
    fn new(width: u32, height: u32) -> Self {
        let scale = width.max(height).max(1) as f64 * 0.5;
        Self { center_x: (width as f64 - 1.0) * 0.5, center_y: (height as f64 - 1.0) * 0.5, scale }
    }

    fn normalize(&self, point: (f64, f64)) -> (f64, f64) {
        ((point.0 - self.center_x) / self.scale, (point.1 - self.center_y) / self.scale)
    }
}

fn maximum_triangle_area(points: &[(f64, f64); 4]) -> f64 {
    let mut maximum = 0.0_f64;
    for a in 0..4 {
        for b in a + 1..4 {
            for c in b + 1..4 {
                maximum = maximum.max(cross(points[a], points[b], points[c]).abs() * 0.5);
            }
        }
    }
    maximum
}

fn cross(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn inliers_cover_image(
    inliers: &[usize],
    matches: &[Match],
    source: &[Feature],
    target: &[Feature],
    source_image: &RgbImage,
    target_image: &RgbImage,
) -> bool {
    let mut source_bounds = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut target_bounds = source_bounds;
    for &index in inliers {
        let matched = matches[index];
        let source_point = &source[matched.source];
        let target_point = &target[matched.target];
        source_bounds.0 = source_bounds.0.min(source_point.x);
        source_bounds.1 = source_bounds.1.min(source_point.y);
        source_bounds.2 = source_bounds.2.max(source_point.x);
        source_bounds.3 = source_bounds.3.max(source_point.y);
        target_bounds.0 = target_bounds.0.min(target_point.x);
        target_bounds.1 = target_bounds.1.min(target_point.y);
        target_bounds.2 = target_bounds.2.max(target_point.x);
        target_bounds.3 = target_bounds.3.max(target_point.y);
    }
    let source_width = (source_bounds.2 - source_bounds.0) / source_image.width().max(1) as f64;
    let source_height = (source_bounds.3 - source_bounds.1) / source_image.height().max(1) as f64;
    let target_width = (target_bounds.2 - target_bounds.0) / target_image.width().max(1) as f64;
    let target_height = (target_bounds.3 - target_bounds.1) / target_image.height().max(1) as f64;
    source_width >= 0.06 && source_height >= 0.06 && target_width >= 0.06 && target_height >= 0.06
        && source_width * source_height >= 0.008 && target_width * target_height >= 0.008
}

fn fit_homography(
    points: &[((f64, f64), (f64, f64))],
    source: &RgbImage,
    target: &RgbImage,
) -> Option<[f64; 9]> {
    if points.len() < 4 { return None; }
    let source_norm = Normalizer::new(source.width(), source.height());
    let target_norm = Normalizer::new(target.width(), target.height());
    let mut normal = [[0.0_f64; 9]; 8];
    for &(source_point, target_point) in points {
        let (x, y) = source_norm.normalize(source_point);
        let (u, v) = target_norm.normalize(target_point);
        let rows = [
            [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u],
            [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v],
        ];
        for row in rows {
            for i in 0..8 {
                for j in 0..8 { normal[i][j] += row[i] * row[j]; }
                normal[i][8] += row[i] * row[8];
            }
        }
    }
    let solution = solve_linear_8(normal)?;
    let normalized = [solution[0], solution[1], solution[2], solution[3], solution[4], solution[5], solution[6], solution[7], 1.0];
    let source_to_normalized = [
        1.0 / source_norm.scale, 0.0, -source_norm.center_x / source_norm.scale,
        0.0, 1.0 / source_norm.scale, -source_norm.center_y / source_norm.scale,
        0.0, 0.0, 1.0,
    ];
    let target_from_normalized = [
        target_norm.scale, 0.0, target_norm.center_x,
        0.0, target_norm.scale, target_norm.center_y,
        0.0, 0.0, 1.0,
    ];
    let pixel_homography = multiply_3x3(&multiply_3x3(&target_from_normalized, &normalized), &source_to_normalized);
    if pixel_homography.iter().any(|value| !value.is_finite()) { return None; }
    Some(pixel_homography)
}

fn solve_linear_8(mut matrix: [[f64; 9]; 8]) -> Option<[f64; 8]> {
    for column in 0..8 {
        let mut pivot = column;
        for row in column + 1..8 {
            if matrix[row][column].abs() > matrix[pivot][column].abs() { pivot = row; }
        }
        if matrix[pivot][column].abs() < 1e-10 { return None; }
        matrix.swap(column, pivot);
        let divisor = matrix[column][column];
        for index in column..=8 { matrix[column][index] /= divisor; }
        for row in 0..8 {
            if row == column { continue; }
            let factor = matrix[row][column];
            if factor == 0.0 { continue; }
            for index in column..=8 { matrix[row][index] -= factor * matrix[column][index]; }
        }
    }
    let mut solution = [0.0_f64; 8];
    for index in 0..8 { solution[index] = matrix[index][8]; }
    solution.iter().all(|value| value.is_finite()).then_some(solution)
}

fn multiply_3x3(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut result = [0.0_f64; 9];
    for row in 0..3 {
        for column in 0..3 {
            for inner in 0..3 { result[row * 3 + column] += a[row * 3 + inner] * b[inner * 3 + column]; }
        }
    }
    result
}

fn homography_is_bounded(model: &[f64; 9], source: &RgbImage, target: &RgbImage) -> bool {
    let corners = [
        (0.0, 0.0),
        (source.width().saturating_sub(1) as f64, 0.0),
        (0.0, source.height().saturating_sub(1) as f64),
        (source.width().saturating_sub(1) as f64, source.height().saturating_sub(1) as f64),
    ];
    let mut denominator_sign = 0.0_f64;
    for point in corners {
        let denominator = model[6] * point.0 + model[7] * point.1 + model[8];
        if !denominator.is_finite() || denominator.abs() < 1e-8 { return false; }
        if denominator_sign == 0.0 { denominator_sign = denominator.signum(); }
        else if denominator.signum() != denominator_sign { return false; }
        let Some((x, y)) = project(model, point) else { return false; };
        if x.abs() > target.width() as f64 * 8.0 || y.abs() > target.height() as f64 * 8.0 { return false; }
    }
    true
}

fn project(model: &[f64; 9], point: (f64, f64)) -> Option<(f64, f64)> {
    let denominator = model[6] * point.0 + model[7] * point.1 + model[8];
    if !denominator.is_finite() || denominator.abs() < 1e-10 { return None; }
    let x = (model[0] * point.0 + model[1] * point.1 + model[2]) / denominator;
    let y = (model[3] * point.0 + model[4] * point.1 + model[5]) / denominator;
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

fn reprojection_error(model: &[f64; 9], source: (f64, f64), target: (f64, f64)) -> Option<f64> {
    let projected = project(model, source)?;
    let dx = projected.0 - target.0;
    let dy = projected.1 - target.1;
    let error = (dx * dx + dy * dy).sqrt();
    error.is_finite().then_some(error)
}

fn normalize_homography(model: &mut [f64; 9]) {
    let divisor = if model[8].abs() > 1e-12 {
        model[8]
    } else {
        model.iter().map(|value| value.abs()).fold(0.0_f64, f64::max)
    };
    if divisor.is_finite() && divisor.abs() > 1e-15 {
        for value in model { *value /= divisor; }
    }
}

struct DeterministicRng(u64);

impl DeterministicRng {
    fn new(seed: u64) -> Self { Self(seed ^ 0x9E37_79B9_7F4A_7C15) }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn textured_image(width: u32, height: u32) -> RgbImage {
        let mut image = RgbImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let mut value = x.wrapping_mul(747_796_405)
                    ^ y.wrapping_mul(2_891_336_453)
                    ^ x.wrapping_mul(y).wrapping_mul(277_803_737);
                value ^= value >> 16;
                value = value.wrapping_mul(2_246_822_519);
                value ^= value >> 13;
                let gray = (value >> 24) as u8;
                image.put_pixel(x, y, image::Rgb([gray, gray, gray]));
            }
        }
        image
    }

    #[test]
    fn estimates_translation_from_a_synthetic_overlapping_texture() {
        let source = textured_image(320, 240);
        let mut target = RgbImage::from_pixel(320, 240, image::Rgb([12, 12, 12]));
        let (shift_x, shift_y) = (17_i32, -12_i32);
        for y in 0..source.height() {
            for x in 0..source.width() {
                let target_x = x as i32 + shift_x;
                let target_y = y as i32 + shift_y;
                if target_x >= 0 && target_y >= 0
                    && target_x < target.width() as i32 && target_y < target.height() as i32
                {
                    target.put_pixel(target_x as u32, target_y as u32, *source.get_pixel(x, y));
                }
            }
        }

        let estimate = estimate_homography(&source, &target).unwrap();
        let mapped = project(&estimate.source_to_target, (120.0, 90.0)).unwrap();

        assert!(estimate.inliers >= MIN_INLIERS);
        assert!(estimate.matches >= estimate.inliers);
        assert!(estimate.rms_error_px < 1.0);
        assert!((mapped.0 - (120.0 + shift_x as f64)).abs() < 1.0);
        assert!((mapped.1 - (90.0 + shift_y as f64)).abs() < 1.0);
    }

    #[test]
    fn rejects_an_image_pair_without_stable_features() {
        let first = RgbImage::from_pixel(96, 96, image::Rgb([80, 80, 80]));
        let second = first.clone();
        assert!(estimate_homography(&first, &second)
            .unwrap_err().contains("Pas assez de coins FAST"));
    }
}

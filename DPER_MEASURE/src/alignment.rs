//! Estimation légère de l'orientation des bords visibles pour l'aide au placement.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignmentState {
    Aligned,
    Misaligned,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignmentLine {
    /// Coordinates normalized to the source image, in the range 0..=1.
    pub start: [f32; 2],
    pub end: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignmentAssessment {
    pub state: AlignmentState,
    /// Signed angle relative to the nearest horizontal or vertical axis.
    pub angle_degrees: Option<f32>,
    /// Heuristic visual support index, not a statistical confidence interval.
    pub confidence_percent: u8,
    pub contour: Option<AlignmentLine>,
    /// Fixed central analysis zone as normalized left, top, right, bottom.
    pub analysis_zone: [f32; 4],
    pub threshold_degrees: f32,
    pub supported_contours: u32,
}

const MAX_ANALYSIS_DIMENSION: usize = 480;
const MIN_EDGE_WEIGHT: u16 = 110;
const MIN_LINE_SUPPORT: u32 = 24;
const MAX_TRACE_DISTANCE: i32 = 32;
const MIN_CONFIDENCE_PERCENT: u8 = 15;
const ANALYSIS_ZONE: [f32; 4] = [0.3, 0.3, 0.7, 0.7];

struct LineEvidence {
    support: u32,
    gradient_x_sum: i64,
    gradient_y_sum: i64,
    negative_extent: f32,
    positive_extent: f32,
}

/// Vérifie que deux gradients appartiennent à un même bord, à une tolérance d'environ 20 degrés.
fn same_edge_direction(gx: i32, gy: i32, other_gx: i32, other_gy: i32) -> bool {
    let magnitude_squared = (gx * gx + gy * gy) as u64;
    let other_magnitude_squared = (other_gx * other_gx + other_gy * other_gy) as u64;
    if magnitude_squared == 0 || other_magnitude_squared == 0 {
        return false;
    }

    let dot = (i64::from(gx) * i64::from(other_gx)
        + i64::from(gy) * i64::from(other_gy)).unsigned_abs();
    // |cos(angle)| >= 0.94, sans calculer d'angle pour chaque voisin.
    dot * dot * 10_000 >= magnitude_squared * other_magnitude_squared * 8_836
}

/// Suit un contour assez loin pour que les détails courts de composants pèsent peu.
fn trace_line(
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    gx: i32,
    gy: i32,
    gx_map: &[i16],
    gy_map: &[i16],
    edge_weight: &[u16],
) -> LineEvidence {
    let magnitude = ((gx * gx + gy * gy) as f32).sqrt();
    if magnitude <= f32::EPSILON {
        return LineEvidence {
            support: 0, gradient_x_sum: 0, gradient_y_sum: 0,
            negative_extent: 0.0, positive_extent: 0.0,
        };
    }

    let normal_x = gx as f32 / magnitude;
    let normal_y = gy as f32 / magnitude;
    let tangent_x = -normal_y;
    let tangent_y = normal_x;
    let mut support = 1_u32;
    let mut gradient_x_sum = i64::from(gx);
    let mut gradient_y_sum = i64::from(gy);
    let mut negative_extent = 0.0_f32;
    let mut positive_extent = 0.0_f32;

    for direction in [-1.0_f32, 1.0_f32] {
        let mut gaps = 0;
        for distance in 1..=MAX_TRACE_DISTANCE {
            let target_x = x as f32 + tangent_x * direction * distance as f32;
            let target_y = y as f32 + tangent_y * direction * distance as f32;
            let mut found = false;

            // Search one pixel on either side of the predicted line to handle rasterization
            // and slight curvature without joining unrelated edges.
            for offset in -1..=1 {
                let sample_x = (target_x + normal_x * offset as f32).round() as isize;
                let sample_y = (target_y + normal_y * offset as f32).round() as isize;
                if sample_x < 0 || sample_y < 0
                    || sample_x >= width as isize || sample_y >= height as isize
                {
                    continue;
                }

                let index = sample_y as usize * width + sample_x as usize;
                if edge_weight[index] < MIN_EDGE_WEIGHT {
                    continue;
                }
                let other_gx = i32::from(gx_map[index]);
                let other_gy = i32::from(gy_map[index]);
                if same_edge_direction(gx, gy, other_gx, other_gy) {
                    let dot = i64::from(gx) * i64::from(other_gx)
                        + i64::from(gy) * i64::from(other_gy);
                    let polarity = if dot < 0 { -1 } else { 1 };
                    gradient_x_sum += polarity * i64::from(other_gx);
                    gradient_y_sum += polarity * i64::from(other_gy);
                    found = true;
                    break;
                }
            }

            if found {
                support += 1;
                if direction < 0.0 { negative_extent = distance as f32; }
                else { positive_extent = distance as f32; }
                gaps = 0;
            } else {
                gaps += 1;
                if gaps > 2 {
                    break;
                }
            }
        }
    }

    LineEvidence {
        support, gradient_x_sum, gradient_y_sum, negative_extent, positive_extent,
    }
}

/// Évalue si les longs contours visibles suivent l'horizontale ou la verticale.
/// L'analyse est une aide visuelle; elle ne certifie ni l'identité ni le centrage de la pièce.
pub fn estimate(rgb: &[u8], width: u32, height: u32) -> AlignmentState {
    assess(rgb, width, height, 2.0).state
}

fn empty_assessment(threshold_degrees: f32) -> AlignmentAssessment {
    AlignmentAssessment {
        state: AlignmentState::Unknown,
        angle_degrees: None,
        confidence_percent: 0,
        contour: None,
        analysis_zone: ANALYSIS_ZONE,
        threshold_degrees,
        supported_contours: 0,
    }
}

fn axis_relative_angle(angle_radians: f32) -> f32 {
    (angle_radians + std::f32::consts::FRAC_PI_4)
        .rem_euclid(std::f32::consts::FRAC_PI_2)
        - std::f32::consts::FRAC_PI_4
}

/// Estimates long contours in the central 40% of the image and returns the visible evidence.
/// This is a placement aid; it does not identify the part or certify its centering.
pub fn assess(
    rgb: &[u8],
    width: u32,
    height: u32,
    requested_threshold_degrees: f32,
) -> AlignmentAssessment {
    let threshold_degrees = if requested_threshold_degrees.is_finite() {
        requested_threshold_degrees.clamp(0.5, 15.0)
    } else {
        2.0
    };
    let threshold_radians = threshold_degrees.to_radians();
    let (Some(expected_len), Some(width), Some(height)) = (
        (width as usize).checked_mul(height as usize).and_then(|pixels| pixels.checked_mul(3)),
        usize::try_from(width).ok(),
        usize::try_from(height).ok(),
    ) else {
        return empty_assessment(threshold_degrees);
    };
    if rgb.len() != expected_len || width < 3 || height < 3 {
        return empty_assessment(threshold_degrees);
    }

    // Average source pixels instead of sampling one pixel. This avoids aliasing fine surface
    // textures into strong false edges when reducing a live camera frame.
    let step = width.max(height).saturating_add(MAX_ANALYSIS_DIMENSION - 1)
        / MAX_ANALYSIS_DIMENSION;
    let step = step.max(1);
    let sampled_width = (width - 1) / step + 1;
    let sampled_height = (height - 1) / step + 1;
    if sampled_width < 3 || sampled_height < 3 {
        return empty_assessment(threshold_degrees);
    }

    // Keep the RGB channels separate: converting a blue PCB and a dark mat to luminance can
    // erase an otherwise useful color boundary before edge detection begins.
    let mut channels = [
        vec![0_u8; sampled_width * sampled_height],
        vec![0_u8; sampled_width * sampled_height],
        vec![0_u8; sampled_width * sampled_height],
    ];
    for y in 0..sampled_height {
        let source_top = y * step;
        let source_bottom = source_top.saturating_add(step).min(height);
        for x in 0..sampled_width {
            let source_left = x * step;
            let source_right = source_left.saturating_add(step).min(width);
            let mut channel_sums = [0_u64; 3];
            let mut sample_count = 0_u64;
            for source_y in source_top..source_bottom {
                for source_x in source_left..source_right {
                    let index = (source_y * width + source_x) * 3;
                    for channel in 0..3 {
                        channel_sums[channel] += u64::from(rgb[index + channel]);
                    }
                    sample_count += 1;
                }
            }
            let index = y * sampled_width + x;
            for channel in 0..3 {
                channels[channel][index] = (channel_sums[channel] / sample_count) as u8;
            }
        }
    }

    // A separable 5-tap Gaussian suppresses engraving, knurling and background grain while
    // retaining the broader outlines used to place a part.
    const BLUR_KERNEL: [u32; 5] = [1, 4, 6, 4, 1];
    let mut smoothed_channels = [
        vec![0_u8; sampled_width * sampled_height],
        vec![0_u8; sampled_width * sampled_height],
        vec![0_u8; sampled_width * sampled_height],
    ];
    for channel_index in 0..3 {
        let mut horizontal_blur = vec![0_u8; channels[channel_index].len()];
        for y in 0..sampled_height {
            for x in 0..sampled_width {
                let mut sum = 0_u32;
                for (kernel_index, weight) in BLUR_KERNEL.iter().enumerate() {
                    let offset = kernel_index as isize - 2;
                    let sample_x = (x as isize + offset).clamp(0, sampled_width as isize - 1) as usize;
                    sum += u32::from(channels[channel_index][y * sampled_width + sample_x]) * weight;
                }
                horizontal_blur[y * sampled_width + x] = ((sum + 8) / 16) as u8;
            }
        }
        for y in 0..sampled_height {
            for x in 0..sampled_width {
                let mut sum = 0_u32;
                for (kernel_index, weight) in BLUR_KERNEL.iter().enumerate() {
                    let offset = kernel_index as isize - 2;
                    let sample_y = (y as isize + offset).clamp(0, sampled_height as isize - 1) as usize;
                    sum += u32::from(horizontal_blur[sample_y * sampled_width + x]) * weight;
                }
                smoothed_channels[channel_index][y * sampled_width + x] = ((sum + 8) / 16) as u8;
            }
        }
    }

    let mut gx_map = vec![0_i16; sampled_width * sampled_height];
    let mut gy_map = vec![0_i16; sampled_width * sampled_height];
    let mut edge_weight = vec![0_u16; sampled_width * sampled_height];
    for y in 1..sampled_height - 1 {
        let top = (y - 1) * sampled_width;
        let middle = y * sampled_width;
        let bottom = (y + 1) * sampled_width;
        for x in 1..sampled_width - 1 {
            let left = x - 1;
            let right = x + 1;
            let index = middle + x;
            let mut strongest_gradient = 0_i32;
            let mut strongest_gx = 0_i32;
            let mut strongest_gy = 0_i32;
            for channel in &smoothed_channels {
                let gx = -i32::from(channel[top + left]) + i32::from(channel[top + right])
                    - 2 * i32::from(channel[middle + left]) + 2 * i32::from(channel[middle + right])
                    - i32::from(channel[bottom + left]) + i32::from(channel[bottom + right]);
                let gy = -i32::from(channel[top + left]) - 2 * i32::from(channel[top + x])
                    - i32::from(channel[top + right]) + i32::from(channel[bottom + left])
                    + 2 * i32::from(channel[bottom + x]) + i32::from(channel[bottom + right]);
                let strength = gx.abs() + gy.abs();
                if strength > strongest_gradient {
                    strongest_gradient = strength;
                    strongest_gx = gx;
                    strongest_gy = gy;
                }
            }
            gx_map[index] = strongest_gx as i16;
            gy_map[index] = strongest_gy as i16;
            edge_weight[index] = strongest_gradient as u16;
        }
    }

    // The user aligns the part under the centre reticle. Keep the voting window tight around
    // it: the surrounding textured mat can contain many coherent-looking false contours.
    let roi_left = sampled_width * 3 / 10;
    let roi_right = sampled_width - roi_left;
    let roi_top = sampled_height * 3 / 10;
    let roi_bottom = sampled_height - roi_top;
    for y in 0..sampled_height {
        for x in 0..sampled_width {
            if x < roi_left || x >= roi_right || y < roi_top || y >= roi_bottom {
                edge_weight[y * sampled_width + x] = 0;
            }
        }
    }

    let mut supported_edge_count = 0_u32;
    let mut total_line_weight = 0_u64;
    let mut axis_aligned_weight = 0_u64;
    let mut angle_sine_sum = 0.0_f64;
    let mut angle_cosine_sum = 0.0_f64;
    let mut longest_line: Option<(u32, AlignmentLine)> = None;
    let score_stride = if sampled_width.max(sampled_height) >= 240 { 3 } else { 1 };
    for y in (1..sampled_height - 1).step_by(score_stride) {
        for x in (1..sampled_width - 1).step_by(score_stride) {
            let index = y * sampled_width + x;
            let weight = edge_weight[index];
            if weight < MIN_EDGE_WEIGHT {
                continue;
            }

            let gx = i32::from(gx_map[index]);
            let gy = i32::from(gy_map[index]);
            let line = trace_line(
                sampled_width, sampled_height, x, y, gx, gy,
                &gx_map, &gy_map, &edge_weight,
            );
            if line.support < MIN_LINE_SUPPORT {
                continue;
            }

            // Long, coherent edges count more than isolated texture marks.
            let line_weight = u64::from(weight) * u64::from(line.support);
            let edge_angle = (line.gradient_x_sum as f32).atan2(-(line.gradient_y_sum as f32));
            let relative_angle = axis_relative_angle(edge_angle);
            let normalized_angle = (relative_angle * 4.0) as f64;
            angle_sine_sum += normalized_angle.sin() * line_weight as f64;
            angle_cosine_sum += normalized_angle.cos() * line_weight as f64;
            if longest_line.as_ref().map_or(true, |(support, _)| line.support > *support) {
                let tangent_x = -line.gradient_y_sum as f32;
                let tangent_y = line.gradient_x_sum as f32;
                let tangent_len = (tangent_x * tangent_x + tangent_y * tangent_y).sqrt().max(1.0);
                let direction_x = tangent_x / tangent_len;
                let direction_y = tangent_y / tangent_len;
                let center_x = x as f32;
                let center_y = y as f32;
                let start = [
                    ((center_x - direction_x * line.negative_extent) * step as f32 / width as f32).clamp(0.0, 1.0),
                    ((center_y - direction_y * line.negative_extent) * step as f32 / height as f32).clamp(0.0, 1.0),
                ];
                let end = [
                    ((center_x + direction_x * line.positive_extent) * step as f32 / width as f32).clamp(0.0, 1.0),
                    ((center_y + direction_y * line.positive_extent) * step as f32 / height as f32).clamp(0.0, 1.0),
                ];
                longest_line = Some((line.support, AlignmentLine { start, end }));
            }

            supported_edge_count += 1;
            total_line_weight += line_weight;

            // The averaged gradient along a traced line decides its orientation, not one noisy pixel.
            if relative_angle.abs() <= threshold_radians {
                axis_aligned_weight += line_weight;
            }
        }
    }

    let resultant = angle_sine_sum.hypot(angle_cosine_sum);
    let coherence = if total_line_weight == 0 { 0.0 }
        else { (resultant / total_line_weight as f64).clamp(0.0, 1.0) };
    let support_factor = (supported_edge_count as f32 / 80.0).clamp(0.0, 1.0);
    let confidence_percent = (coherence as f32 * support_factor * 100.0).round() as u8;
    if supported_edge_count < 24 || total_line_weight == 0
        || confidence_percent < MIN_CONFIDENCE_PERCENT
    {
        let mut assessment = empty_assessment(threshold_degrees);
        assessment.confidence_percent = confidence_percent;
        assessment.supported_contours = supported_edge_count;
        assessment.contour = longest_line.map(|(_, contour)| contour);
        return assessment;
    }

    let average_angle = angle_sine_sum.atan2(angle_cosine_sum) as f32 / 4.0;
    let average_angle = axis_relative_angle(average_angle).to_degrees();
    // A long narrow part may show only its two parallel sides in the camera frame, so the
    // estimator does not require both horizontal and vertical edges to be visible.
    let aligned_ratio = axis_aligned_weight as f32 / total_line_weight as f32;
    AlignmentAssessment {
        state: if aligned_ratio >= 0.62 { AlignmentState::Aligned } else { AlignmentState::Misaligned },
        angle_degrees: Some(average_angle),
        confidence_percent,
        contour: longest_line.map(|(_, contour)| contour),
        analysis_zone: ANALYSIS_ZONE,
        threshold_degrees,
        supported_contours: supported_edge_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn rectangular_part(angle_degrees: f64) -> RgbImage {
        let mut image = RgbImage::from_pixel(256, 256, Rgb([40, 40, 40]));
        let angle = angle_degrees.to_radians();
        let (sin, cos) = angle.sin_cos();
        for y in 0..256 {
            for x in 0..256 {
                let dx = x as f64 + 0.5 - 128.0;
                let dy = y as f64 + 0.5 - 128.0;
                let local_x = cos * dx + sin * dy;
                let local_y = -sin * dx + cos * dy;
                if local_x.abs() <= 48.0 && local_y.abs() <= 38.0 {
                    image.put_pixel(x, y, Rgb([220, 220, 220]));
                }
            }
        }
        image
    }

    #[test]
    fn alignment_reports_visible_axis_evidence_and_respects_the_angle_threshold() {
        let aligned = rectangular_part(0.0);
        let assessment = assess(aligned.as_raw(), 256, 256, 2.0);
        assert_eq!(assessment.state, AlignmentState::Aligned);
        assert!(assessment.angle_degrees.unwrap().abs() < 1.0);
        assert!(assessment.confidence_percent >= MIN_CONFIDENCE_PERCENT);
        assert!(assessment.contour.is_some());

        let rotated = rectangular_part(12.0);
        let assessment = assess(rotated.as_raw(), 256, 256, 2.0);
        assert_eq!(assessment.state, AlignmentState::Misaligned);
        assert!((assessment.angle_degrees.unwrap().abs() - 12.0).abs() < 2.0);
    }

    #[test]
    fn weak_or_invalid_alignment_evidence_stays_unknown() {
        let uniform = RgbImage::from_pixel(64, 64, Rgb([90, 90, 90]));
        assert_eq!(estimate(uniform.as_raw(), 64, 64), AlignmentState::Unknown);
        assert_eq!(assess(&[], 64, 64, f32::NAN).state, AlignmentState::Unknown);
    }
}

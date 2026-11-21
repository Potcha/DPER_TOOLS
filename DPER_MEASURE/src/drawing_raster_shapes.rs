use crate::reference::{OcrTextLine, ReferenceOcrBox};
use image::{imageops, GrayImage};
use serde::{Deserialize, Serialize};

const MAX_SIDE: u32 = 1800;
const MIN_SIDE: usize = 18;
const MAX_PROPOSALS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RasterShapeKind {
    Rectangle,
    Circle,
}

impl RasterShapeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Circle => "Cercle",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RasterShapeProposal {
    pub kind: RasterShapeKind,
    pub page: u32,
    /// Bounds of the enclosed region, in source-image pixels.
    pub bbox: ReferenceOcrBox,
}

pub(crate) fn propose(
    image: &GrayImage,
    text: &[OcrTextLine],
) -> (Vec<RasterShapeProposal>, bool) {
    let (source_width, source_height) = image.dimensions();
    let longest_side = source_width.max(source_height);
    let reduced = (longest_side > MAX_SIDE).then(|| {
        let scale = f64::from(MAX_SIDE) / f64::from(longest_side);
        let width = (f64::from(source_width) * scale).round() as u32;
        let height = (f64::from(source_height) * scale).round() as u32;
        imageops::resize(image, width.max(1), height.max(1), imageops::FilterType::Triangle)
    });
    let image = reduced.as_ref().unwrap_or(image);
    let (width, height) = (image.width() as usize, image.height() as usize);
    if width < MIN_SIDE || height < MIN_SIDE {
        return (Vec::new(), false);
    }

    let mut open: Vec<_> = image.as_raw().iter().map(|&value| value > 175).collect();
    let mut region = Vec::new();
    let mut proposals = Vec::new();
    for start in 0..open.len() {
        if !open[start] {
            continue;
        }
        region.clear();
        open[start] = false;
        region.push(start);
        let (mut left, mut right) = (start % width, start % width);
        let (mut top, mut bottom) = (start / width, start / width);
        let mut cursor = 0;
        while cursor < region.len() {
            let index = region[cursor];
            cursor += 1;
            let (x, y) = (index % width, index / width);
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
            for neighbor in [
                (x > 0).then(|| index - 1),
                (x + 1 < width).then_some(index + 1),
                (y > 0).then(|| index - width),
                (y + 1 < height).then_some(index + width),
            ].into_iter().flatten() {
                if open[neighbor] {
                    open[neighbor] = false;
                    region.push(neighbor);
                }
            }
        }

        let (span_x, span_y) = (right - left + 1, bottom - top + 1);
        if left == 0 || top == 0 || right + 1 == width || bottom + 1 == height
            || span_x.min(span_y) < MIN_SIDE
            || span_x > 4 * span_y || span_y > 4 * span_x
            || (span_x * 5 > width * 4 && span_y * 5 > height * 4)
        {
            continue;
        }

        let scale_x = f64::from(source_width) / width as f64;
        let scale_y = f64::from(source_height) / height as f64;
        let x_min = (left as f64 * scale_x).floor() as u32;
        let y_min = (top as f64 * scale_y).floor() as u32;
        let x_max = (((right + 1) as f64 * scale_x).ceil() as u32).min(source_width);
        let y_max = (((bottom + 1) as f64 * scale_y).ceil() as u32).min(source_height);
        if text.iter().any(|line| {
            let bounds = &line.bbox;
            x_min >= bounds.left.saturating_sub(2) && y_min >= bounds.top.saturating_sub(2)
                && x_max <= bounds.left.saturating_add(bounds.width).saturating_add(2)
                && y_max <= bounds.top.saturating_add(bounds.height).saturating_add(2)
        }) {
            continue;
        }

        let area = span_x * span_y;
        let kind = if region.len() * 100 >= area * 96 {
            RasterShapeKind::Rectangle
        } else if span_x.min(span_y) * 100 >= span_x.max(span_y) * 86
            && (71..=85).contains(&(region.len() * 100 / area))
        {
            let inside_circle = region.iter().filter(|&&index| {
                let x = 2.0 * ((index % width - left) as f64 + 0.5) / span_x as f64 - 1.0;
                let y = 2.0 * ((index / width - top) as f64 + 0.5) / span_y as f64 - 1.0;
                x * x + y * y <= 1.0
            }).count();
            let circle_area = std::f64::consts::FRAC_PI_4 * area as f64;
            let union = region.len() as f64 + circle_area - inside_circle as f64;
            if (inside_circle as f64) / union < 0.90 {
                continue;
            }
            RasterShapeKind::Circle
        } else {
            continue;
        };
        if proposals.len() == MAX_PROPOSALS {
            return (proposals, true);
        }
        proposals.push(RasterShapeProposal {
            kind,
            page: 1,
            bbox: ReferenceOcrBox {
                left: x_min,
                top: y_min,
                width: x_max - x_min,
                height: y_max - y_min,
                image_width: source_width,
                image_height: source_height,
            },
        });
    }
    (proposals, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn llt_pln_001_01_localizes_enclosed_raster_shapes_and_ignores_text_and_open_lines() {
        let mut image = GrayImage::from_pixel(240, 160, Luma([255]));
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let rectangle = ((x == 15 || x == 75) && (20..=60).contains(&y))
                || ((y == 20 || y == 60) && (15..=75).contains(&x));
            let distance_squared = (x as i32 - 145).pow(2) + (y as i32 - 45).pow(2);
            let circle = (23 * 23..=25 * 25).contains(&distance_squared);
            let text_box = ((x == 20 || x == 45) && (95..=125).contains(&y))
                || ((y == 95 || y == 125) && (20..=45).contains(&x));
            let open_lines = ((x == 100 || x == 170) && (105..=145).contains(&y))
                || (y == 105 && (100..=170).contains(&x));
            if rectangle || circle || text_box || open_lines {
                *pixel = Luma([0]);
            }
        }
        let text = [OcrTextLine {
            text: "0".into(),
            bbox: ReferenceOcrBox {
                left: 19, top: 94, width: 28, height: 33,
                image_width: 240, image_height: 160,
            },
            confidence: Some(90.0),
        }];
        let (proposals, truncated) = propose(&image, &text);
        assert!(!truncated);
        assert_eq!(proposals.len(), 2);
        let rectangle = &proposals[0];
        assert_eq!(rectangle.kind, RasterShapeKind::Rectangle);
        assert_eq!((rectangle.bbox.left, rectangle.bbox.top, rectangle.bbox.width, rectangle.bbox.height),
            (16, 21, 59, 39));
        let circle = &proposals[1];
        assert_eq!(circle.kind, RasterShapeKind::Circle);
        assert_eq!((circle.bbox.left, circle.bbox.top, circle.bbox.width, circle.bbox.height),
            (123, 23, 45, 45));
    }
}

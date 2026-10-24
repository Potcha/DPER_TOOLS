//! Import des plans de référence, extraction locale de cotes et comparaison indicative.
use crate::geometry::{self, Measurement, Tool};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_PDF_PAGES: usize = 20;
const MAX_REFERENCE_DIMENSIONS: usize = 1_000;
const OCR_MAX_EDGE: u32 = 4000;
const REPORT_MAX_EDGE: u32 = 2200;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferenceDocument {
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub source_name: String,
    #[serde(default)]
    pub extracted_text: String,
    #[serde(default)]
    pub dimensions: Vec<ReferenceDimension>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferenceDimension {
    pub id: String,
    pub label: String,
    pub nominal_mm: f64,
    pub tolerance_minus_mm: f64,
    pub tolerance_plus_mm: f64,
    pub source_text: String,
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub measurement_id: Option<u64>,
    #[serde(default)]
    pub ocr_page: Option<u32>,
    #[serde(default)]
    pub ocr_bbox: Option<ReferenceOcrBox>,
    #[serde(default)]
    pub ocr_confidence: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferenceOcrBox {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub image_width: u32,
    #[serde(default)]
    pub image_height: u32,
}

#[derive(Clone, Debug)]
struct OcrTextLine {
    text: String,
    bbox: ReferenceOcrBox,
    confidence: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct RenderedPage {
    pub rgb: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Comparison {
    pub measured_mm: Option<f64>,
    pub expanded_uncertainty_mm: Option<f64>,
    pub lower_limit_mm: f64,
    pub upper_limit_mm: f64,
    pub status: ComparisonStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonStatus {
    ToConfirm,
    NotAssociated,
    Unavailable,
    InTolerance,
    OutOfTolerance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartStatus {
    InTolerance,
    OutOfTolerance,
    Incomplete,
    NoDimensions,
}

impl PartStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::InTolerance => "Toutes les cotes comparées sont dans la tolérance",
            Self::OutOfTolerance => "Au moins une cote est hors tolérance",
            Self::Incomplete => "Comparaison incomplète",
            Self::NoDimensions => "Aucune cote à comparer",
        }
    }
}

impl ComparisonStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::ToConfirm => "À confirmer",
            Self::NotAssociated => "Non associée",
            Self::Unavailable => "Mesure indisponible",
            Self::InTolerance => "Dans la tolérance",
            Self::OutOfTolerance => "Hors tolérance",
        }
    }
}

impl ReferenceDocument {
    pub fn new(path: &Path, extracted_text: String, dimensions: Vec<ReferenceDimension>) -> Self {
        Self {
            source_file: path.to_string_lossy().into_owned(),
            source_name: path.file_name().and_then(|name| name.to_str())
                .unwrap_or("document de référence").to_owned(),
            extracted_text,
            dimensions,
        }
    }
}

/// Lit le texte vectoriel des PDF et utilise Tesseract hors ligne pour les scans/images.
pub fn analyze_document(path: &Path) -> Result<ReferenceDocument, String> {
    let extension = extension(path)?;
    if !matches!(extension.as_str(), "pdf" | "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff") {
        return Err("Formats acceptés : PDF, PNG, JPG, BMP et TIFF.".into());
    }
    if !path.is_file() {
        return Err(format!("Document introuvable : {}", path.display()));
    }

    let (text, dimensions) = if extension == "pdf" {
        let vector_text = extract_pdf_text(path).unwrap_or_default();
        let mut candidates = extract_pdf_text_dimensions(&vector_text);
        let vector_pages = candidates.iter().filter_map(|dimension| dimension.ocr_page)
            .collect::<Vec<_>>();
        let split_page_count = vector_text.split('\u{000C}').count();
        let page_count = split_page_count
            .saturating_sub(if vector_text.ends_with('\u{000C}') { 1 } else { 0 })
            .max(1);
        let has_pages_without_text_dimensions = candidates.len() < MAX_REFERENCE_DIMENSIONS
            && (1..=page_count.min(MAX_PDF_PAGES))
                .any(|page| !vector_pages.contains(&(page as u32)));
        if has_pages_without_text_dimensions {
            let pages = render_pdf_files(path, OCR_MAX_EDGE, MAX_PDF_PAGES)?;
            let mut recognized = if vector_pages.is_empty() { String::new() } else { vector_text };
            let mut ocr_dimension_index = 0usize;
            for (page_index, page_path) in pages.files.iter().enumerate() {
                let page_number = page_index as u32 + 1;
                if vector_pages.contains(&page_number) { continue; }
                let page = image_page(page_path, OCR_MAX_EDGE)?;
                let (page_text, lines) = ocr_page_with_lines(&page)?;
                if !recognized.is_empty() { recognized.push('\n'); }
                recognized.push_str(&format!("[Page {page_number} · OCR]\n"));
                recognized.push_str(&page_text);
                for line in lines {
                    for mut dimension in extract_dimensions(&line.text) {
                        if candidates.len() >= MAX_REFERENCE_DIMENSIONS { break; }
                        ocr_dimension_index += 1;
                        dimension.id = format!("ocr-{ocr_dimension_index}");
                        dimension.ocr_page = Some(page_number);
                        dimension.ocr_bbox = Some(line.bbox.clone());
                        dimension.ocr_confidence = line.confidence;
                        candidates.push(dimension);
                    }
                    if candidates.len() >= MAX_REFERENCE_DIMENSIONS { break; }
                }
                if candidates.len() >= MAX_REFERENCE_DIMENSIONS { break; }
            }
            (recognized, candidates)
        } else {
            (vector_text, candidates)
        }
    } else {
        let (text, lines) = ocr_image_with_lines(path)?;
        let mut dimensions = Vec::new();
        for line in lines {
            for mut dimension in extract_dimensions(&line.text) {
                if dimensions.len() >= MAX_REFERENCE_DIMENSIONS { break; }
                dimension.id = format!("ocr-{}", dimensions.len() + 1);
                dimension.ocr_page = Some(1);
                dimension.ocr_bbox = Some(line.bbox.clone());
                dimension.ocr_confidence = line.confidence;
                dimensions.push(dimension);
            }
            if dimensions.len() >= MAX_REFERENCE_DIMENSIONS { break; }
        }
        (text, dimensions)
    };

    Ok(ReferenceDocument::new(path, text, dimensions))
}

/// Rend les pages du document pour les inclure visuellement au rapport.
pub fn render_document(path: &Path) -> Result<Vec<RenderedPage>, String> {
    let extension = extension(path)?;
    if extension == "pdf" {
        return render_pdf(path, REPORT_MAX_EDGE, MAX_PDF_PAGES);
    } else if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff") {
        return image_page(path, REPORT_MAX_EDGE).map(|page| vec![page]);
    } else {
        return Err("Le format du document de référence n'est pas pris en charge.".into());
    }
}

pub fn resolve_source(source_file: &str, relative_to: &Path) -> PathBuf {
    let source = PathBuf::from(source_file);
    if source.is_absolute() { source } else { relative_to.join(source) }
}

pub fn compare_dimension(dimension: &ReferenceDimension, measurements: &[Measurement]) -> Comparison {
    let lower_limit_mm = dimension.nominal_mm - dimension.tolerance_minus_mm;
    let upper_limit_mm = dimension.nominal_mm + dimension.tolerance_plus_mm;
    if !dimension.confirmed {
        return Comparison { measured_mm: None, expanded_uncertainty_mm: None,
            lower_limit_mm, upper_limit_mm, status: ComparisonStatus::ToConfirm };
    }
    let Some(measurement_id) = dimension.measurement_id else {
        return Comparison { measured_mm: None, expanded_uncertainty_mm: None,
            lower_limit_mm, upper_limit_mm, status: ComparisonStatus::NotAssociated };
    };
    let Some(measurement) = measurements.iter().find(|measurement| measurement.id == measurement_id) else {
        return Comparison { measured_mm: None, expanded_uncertainty_mm: None,
            lower_limit_mm, upper_limit_mm, status: ComparisonStatus::Unavailable };
    };
    let Some(measured_mm) = measurement_mm(measurement) else {
        return Comparison { measured_mm: None, expanded_uncertainty_mm: None,
            lower_limit_mm, upper_limit_mm, status: ComparisonStatus::Unavailable };
    };
    let expanded_uncertainty_mm = measurement.expanded_uncertainty_mm
        .filter(|uncertainty| uncertainty.is_finite() && *uncertainty > 0.0);
    let status = if let Some(uncertainty) = expanded_uncertainty_mm {
        let interval_lower = measured_mm - uncertainty;
        let interval_upper = measured_mm + uncertainty;
        if interval_lower >= lower_limit_mm && interval_upper <= upper_limit_mm {
            ComparisonStatus::InTolerance
        } else if interval_upper < lower_limit_mm || interval_lower > upper_limit_mm {
            ComparisonStatus::OutOfTolerance
        } else {
            ComparisonStatus::ToConfirm
        }
    } else {
        ComparisonStatus::ToConfirm
    };
    Comparison { measured_mm: Some(measured_mm), expanded_uncertainty_mm, lower_limit_mm, upper_limit_mm, status }
}

pub fn part_status(document: &ReferenceDocument, measurements: &[Measurement]) -> PartStatus {
    if document.dimensions.is_empty() { return PartStatus::NoDimensions; }
    let comparisons = document.dimensions.iter()
        .map(|dimension| compare_dimension(dimension, measurements)).collect::<Vec<_>>();
    if comparisons.iter().any(|comparison| comparison.status == ComparisonStatus::OutOfTolerance) {
        PartStatus::OutOfTolerance
    } else if comparisons.iter().all(|comparison| comparison.status == ComparisonStatus::InTolerance) {
        PartStatus::InTolerance
    } else {
        PartStatus::Incomplete
    }
}

pub fn measurement_mm(measurement: &Measurement) -> Option<f64> {
    let points = geometry::metric_points(measurement)?;
    let pixels = match measurement.kind {
        Tool::Distance if points.len() >= 2 => points[0].dist(points[1]),
        Tool::Polyline if points.len() >= 2 => geometry::polyline_length(&points),
        Tool::Circle if points.len() >= 2 => {
            let scale_x = measurement.mm_per_pixel_x.or(measurement.mm_per_pixel).unwrap_or(1.0);
            let scale_y = measurement.mm_per_pixel_y.or(measurement.mm_per_pixel).unwrap_or(1.0);
            if (scale_x - scale_y).abs() > scale_x.max(scale_y) * 1e-9 { return None; }
            points[0].dist(points[1]) * 2.0
        }
        Tool::PointLineDistance if points.len() >= 3 => geometry::point_line_distance(
            points[0], points[1], points[2],
        )?,
        Tool::ParallelDistance if points.len() >= 4 => geometry::parallel_line_distance(
            points[0], points[1], points[2], points[3],
        )?.0,
        Tool::CircleFit if points.len() >= 3 => geometry::fit_circle(&points)?.radius * 2.0,
        Tool::Arc if points.len() >= 3 => {
            let arc = geometry::arc_geometry(&points)?;
            arc.radius * arc.sweep_radians.abs()
        }
        // A rectangle has two separate sides; the dimension to compare must be specified explicitly.
        Tool::Angle | Tool::Rectangle | Tool::Polygon | Tool::Ellipse | Tool::Coordinate
            | Tool::ObjectCount => return None,
        _ => return None,
    };
    let value = pixels;
    (value.is_finite() && value > 0.0).then_some(value)
}

fn extension(path: &Path) -> Result<String, String> {
    path.extension().and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "Le document doit avoir une extension reconnue.".to_owned())
}

fn extract_pdf_text(path: &Path) -> Result<String, String> {
    let output = Command::new("pdftotext")
        .arg("-layout").arg(path).arg("-")
        .output()
        .map_err(|error| format!("Extraction PDF locale indisponible (pdftotext) : {error}"))?;
    if !output.status.success() {
        return Err("Le texte du PDF n'a pas pu être extrait.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn ocr_image_with_lines(path: &Path) -> Result<(String, Vec<OcrTextLine>), String> {
    let (image_width, image_height) = image::image_dimensions(path)
        .map_err(|error| format!("Dimensions de l'image OCR illisibles : {error}"))?;
    let output = Command::new("tesseract")
        .arg(path).arg("stdout").args(["-l", "fra+eng", "--psm", "11"])
        .arg("tsv")
        .output()
        .map_err(|error| format!("OCR local indisponible (Tesseract) : {error}"))?;
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Échec de l'OCR local : {}", reason.trim()));
    }
    let tsv = String::from_utf8_lossy(&output.stdout);
    let mut grouped: BTreeMap<(u32, u32, u32, u32), (String, u32, u32, u32, u32, f64, u32)> = BTreeMap::new();
    for row in tsv.lines().skip(1) {
        let fields = row.split('\t').collect::<Vec<_>>();
        if fields.len() < 12 || fields[0] != "5" { continue; }
        let (Ok(page), Ok(block), Ok(paragraph), Ok(line), Ok(left), Ok(top), Ok(width), Ok(height)) = (
            fields[1].parse::<u32>(), fields[2].parse::<u32>(), fields[3].parse::<u32>(),
            fields[4].parse::<u32>(), fields[6].parse::<u32>(), fields[7].parse::<u32>(),
            fields[8].parse::<u32>(), fields[9].parse::<u32>(),
        ) else { continue; };
        let text = fields[11].trim();
        if text.is_empty() { continue; }
        let confidence = fields[10].parse::<f64>().ok().filter(|value| *value >= 0.0);
        let entry = grouped.entry((page, block, paragraph, line)).or_insert((
            String::new(), left, top, left.saturating_add(width), top.saturating_add(height), 0.0, 0,
        ));
        if !entry.0.is_empty() { entry.0.push(' '); }
        entry.0.push_str(text);
        entry.1 = entry.1.min(left);
        entry.2 = entry.2.min(top);
        entry.3 = entry.3.max(left.saturating_add(width));
        entry.4 = entry.4.max(top.saturating_add(height));
        if let Some(confidence) = confidence { entry.5 += confidence; entry.6 += 1; }
    }
    let lines = grouped.into_values().map(|(text, left, top, right, bottom, confidence, count)| OcrTextLine {
        text, bbox: ReferenceOcrBox {
            left, top, width: right.saturating_sub(left), height: bottom.saturating_sub(top),
            image_width, image_height,
        },
        confidence: (count > 0).then_some(confidence / count as f64),
    }).collect::<Vec<_>>();
    let text = lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n");
    Ok((text, lines))
}

fn render_pdf(path: &Path, max_edge: u32, max_pages: usize) -> Result<Vec<RenderedPage>, String> {
    let pages = render_pdf_files(path, max_edge, max_pages)?;
    pages.files.iter().map(|page| image_page(page, max_edge)).collect()
}

struct RenderedPdfPages {
    _temporary: TemporaryDirectory,
    files: Vec<PathBuf>,
}

fn render_pdf_files(path: &Path, max_edge: u32, max_pages: usize) -> Result<RenderedPdfPages, String> {
    if !path.is_file() { return Err(format!("Document introuvable : {}", path.display())); }
    let temp = TemporaryDirectory::new()?;
    let prefix = temp.path.join("page");
    let output = Command::new("pdftoppm")
        .args(["-png", "-scale-to"])
        .arg(max_edge.to_string())
        .args(["-f", "1", "-l"])
        .arg(max_pages.to_string())
        .arg(path).arg(&prefix)
        .output()
        .map_err(|error| format!("Rendu PDF local indisponible (pdftoppm) : {error}"))?;
    if !output.status.success() {
        return Err(format!("Impossible de rendre les pages du PDF : {}",
            String::from_utf8_lossy(&output.stderr).trim()));
    }
    let mut pages = fs::read_dir(&temp.path).map_err(|error| error.to_string())?
        .filter_map(Result::ok).map(|entry| entry.path())
        .filter(|page| page.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("png")))
        .collect::<Vec<_>>();
    pages.sort_by_key(|page| {
        page.file_stem().and_then(|stem| stem.to_str())
            .and_then(|stem| stem.rsplit('-').next())
            .and_then(|suffix| suffix.parse::<usize>().ok()).unwrap_or(usize::MAX)
    });
    if pages.is_empty() { return Err("Aucune page n'a pu être rendue depuis le PDF.".into()); }
    Ok(RenderedPdfPages { _temporary: temp, files: pages })
}

fn image_page(path: &Path, max_edge: u32) -> Result<RenderedPage, String> {
    let image = image::open(path).map_err(|error| format!("Document image illisible ({}): {error}", path.display()))?;
    let image = image.thumbnail(max_edge, max_edge).to_rgb8();
    let (width, height) = image.dimensions();
    Ok(RenderedPage { rgb: image.into_raw(), width, height })
}

fn ocr_page_with_lines(page: &RenderedPage) -> Result<(String, Vec<OcrTextLine>), String> {
    let temp = TemporaryDirectory::new()?;
    let image_path = temp.path.join("page.png");
    image::save_buffer_with_format(&image_path, &page.rgb, page.width, page.height,
        image::ColorType::Rgb8, image::ImageFormat::Png)
        .map_err(|error| format!("Préparation de la page PDF pour OCR impossible : {error}"))?;
    ocr_image_with_lines(&image_path)
}

fn extract_dimensions(text: &str) -> Vec<ReferenceDimension> {
    let mut dimensions = Vec::new();
    for line in text.lines() {
        if dimensions.len() >= MAX_REFERENCE_DIMENSIONS { break; }
        let line = collapse_numeric_spaces(line);
        let limit_markers = explicit_limit_markers(&line);
        let signed_pairs = find_signed_pair_markers(&line);
        if !limit_markers.is_empty() && !line.contains('±') && !line.contains("+/-")
            && signed_pairs.is_empty() {
            if let Some((minimum, maximum)) = explicit_limit_bounds(&line, &limit_markers) {
                let nominal = minimum + (maximum - minimum) / 2.0;
                let tolerance_minus = nominal - minimum;
                let tolerance_plus = maximum - nominal;
                if !nominal.is_finite() || !tolerance_minus.is_finite() || !tolerance_plus.is_finite() {
                    continue;
                }
                let source_text = source_text(&line);
                let index = dimensions.len() + 1;
                dimensions.push(ReferenceDimension {
                    id: format!("ocr-{index}"),
                    label: format!("MIN {} / MAX {} mm", decimal_label(minimum), decimal_label(maximum)),
                    nominal_mm: nominal,
                    tolerance_minus_mm: tolerance_minus,
                    tolerance_plus_mm: tolerance_plus,
                    source_text,
                    confirmed: false,
                    measurement_id: None,
                    ocr_page: None,
                    ocr_bbox: None,
                    ocr_confidence: None,
                });
                // Preserve the explicit bounds in the label; the midpoint carries
                // their exact interval through the existing comparison model.
                continue;
            }
        }
        let mut markers = line.match_indices('±').map(|(index, marker)| (index, index + marker.len()))
            .collect::<Vec<_>>();
        markers.extend(line.match_indices("+/-").map(|(index, marker)| (index, index + marker.len())));
        markers.extend(signed_pairs.iter().map(|(index, _)| (*index, *index)));
        markers.sort_unstable();
        for (start, end) in markers {
            // The second sign closes the asymmetric pair; it is not another tolerance marker.
            if signed_pairs.iter().any(|(_, second_sign)| *second_sign == start) {
                continue;
            }
            let before = &line[..start];
            let after = &line[end..];
            let Some((nominal, token_start)) = last_number(before) else { continue; };
            let (tolerance_minus, tolerance_plus, label) = if let Some((minus, plus)) = signed_tolerances(after) {
                (minus, plus, format!("{} +{} / -{} mm", decimal_label(nominal), decimal_label(plus), decimal_label(minus)))
            } else {
                let Some(tolerance) = first_number(after).map(|(value, _)| value) else { continue; };
                (tolerance, tolerance, format!("{} ± {} mm", decimal_label(nominal), decimal_label(tolerance)))
            };
            let preceding = before[..token_start].trim_end().chars().last();
            // Ignore OCR readings such as "4 x 3,5 ± 0,1": the multiplication
            // indicates a repeated pattern, not one unambiguous scalar dimension.
            if matches!(preceding, Some('x' | 'X' | '×')) { continue; }
            if !nominal.is_finite() || nominal <= 0.0
                || !tolerance_minus.is_finite() || !tolerance_plus.is_finite()
                || tolerance_minus < 0.0 || tolerance_plus < 0.0
                || tolerance_minus >= nominal || tolerance_plus >= nominal {
                continue;
            }
            let source_text = source_text(&line);
            let index = dimensions.len() + 1;
            dimensions.push(ReferenceDimension {
                id: format!("ocr-{index}"),
                label,
                nominal_mm: nominal,
                tolerance_minus_mm: tolerance_minus,
                tolerance_plus_mm: tolerance_plus,
                source_text,
                confirmed: false,
                measurement_id: None,
                ocr_page: None,
                ocr_bbox: None,
                ocr_confidence: None,
            });
        }
    }
    dimensions
}

fn extract_pdf_text_dimensions(text: &str) -> Vec<ReferenceDimension> {
    let mut dimensions = Vec::new();
    for (page_index, page_text) in text.split('\u{000C}').enumerate() {
        for mut dimension in extract_dimensions(page_text) {
            if dimensions.len() >= MAX_REFERENCE_DIMENSIONS { return dimensions; }
            dimension.id = format!("pdf-text-{}", dimensions.len() + 1);
            dimension.ocr_page = Some(page_index.saturating_add(1).min(u32::MAX as usize) as u32);
            dimensions.push(dimension);
        }
    }
    dimensions
}

fn find_signed_pair_markers(line: &str) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    let mut cursor = 0;
    while cursor < line.len() {
        let Some((relative_index, character)) = line[cursor..].char_indices()
            .find(|(_, character)| matches!(*character, '+' | '-' | '−')) else { break; };
        let index = cursor + relative_index;
        let preceding = line[..index].chars().rev().find(|value| !value.is_whitespace());
        if preceding == Some('±') || last_number(&line[..index]).is_none() {
            cursor = index + character.len_utf8();
            continue;
        }
        if let Some((_, _, second_sign, second_end)) = signed_tolerance_pair(&line[index..]) {
            pairs.push((index, index + second_sign));
            cursor = index + second_end;
        } else {
            cursor = index + character.len_utf8();
        }
    }
    pairs
}

fn signed_tolerances(text: &str) -> Option<(f64, f64)> {
    let (tolerance_minus, tolerance_plus, _, _) = signed_tolerance_pair(text)?;
    Some((tolerance_minus, tolerance_plus))
}

fn signed_tolerance_pair(text: &str) -> Option<(f64, f64, usize, usize)> {
    let tokens = number_tokens(text);
    if tokens.len() < 2 { return None; }
    let (first_value, first_start) = tokens[0];
    let (second_value, second_start) = tokens[1];
    let sign_before = |start: usize| text[..start].char_indices().rev()
        .find(|(_, value)| !value.is_whitespace());
    let (_, first_sign) = sign_before(first_start)?;
    let (second_sign_index, second_sign) = sign_before(second_start)?;
    if !matches!(first_sign, '+' | '-' | '−') || !matches!(second_sign, '+' | '-' | '−') {
        return None;
    }
    let first_end = number_token_end(text, first_start)?;
    let separators = text.get(first_end..second_sign_index)?;
    if !separators.chars().all(|character| {
        character.is_whitespace() || matches!(character, '/' | '|' | ';' | ':' | '(' | ')')
    }) {
        return None;
    }
    let second_end = number_token_end(text, second_start)?;
    if first_sign == second_sign { return None; }
    if first_value < 0.0 || second_value < 0.0 { return None; }
    match (first_sign, second_sign) {
        ('+', '-' | '−') => Some((second_value, first_value, second_sign_index, second_end)),
        ('-' | '−', '+') => Some((first_value, second_value, second_sign_index, second_end)),
        _ => None,
    }
}

fn explicit_limit_markers(line: &str) -> Vec<(bool, usize, usize)> {
    let upper = line.to_ascii_uppercase();
    let mut markers = Vec::new();
    for (words, is_minimum) in [
        (&["MINIMUM", "MINI", "MIN"][..], true),
        (&["MAXIMUM", "MAXI", "MAX"][..], false),
    ] {
        for &word in words {
            for (start, _) in upper.match_indices(word) {
                let end = start + word.len();
                let before_is_boundary = start == 0 || !upper.as_bytes()[start - 1].is_ascii_alphanumeric();
                let after_is_boundary = end == upper.len() || !upper.as_bytes()[end].is_ascii_alphanumeric();
                if before_is_boundary && after_is_boundary {
                    markers.push((is_minimum, start, end));
                }
            }
        }
    }
    markers.sort_by_key(|marker| marker.1);
    markers
}

fn explicit_limit_bounds(line: &str, markers: &[(bool, usize, usize)]) -> Option<(f64, f64)> {
    if markers.iter().filter(|marker| marker.0).count() != 1
        || markers.iter().filter(|marker| !marker.0).count() != 1 {
        return None;
    }
    let tokens = number_tokens(line);
    let nearest_value = |marker_start: usize, marker_end: usize| -> Option<f64> {
        let mut candidates = tokens.iter().filter_map(|(value, start)| {
            let end = number_token_end(line, *start)?;
            let gap = if end <= marker_start { marker_start - end }
                else if *start >= marker_end { *start - marker_end }
                else { 0 };
            (gap <= 12).then_some((gap, *start >= marker_end, *value))
        }).collect::<Vec<_>>();
        // In the common "MIN 9.9 MAX 10.1" notation, the value after MAX
        // is equally distant from the marker as the preceding value. Resolve
        // that boundary tie in favor of the value following the marker.
        candidates.sort_by_key(|(gap, follows_marker, _)| (*gap, !*follows_marker));
        let (gap, follows_marker, value) = *candidates.first()?;
        if candidates.get(1).is_some_and(|(next_gap, next_follows_marker, _)|
            *next_gap == gap && *next_follows_marker == follows_marker)
        {
            return None;
        }
        Some(value)
    };
    let minimum = markers.iter().find(|marker| marker.0)
        .and_then(|marker| nearest_value(marker.1, marker.2))?;
    let maximum = markers.iter().find(|marker| !marker.0)
        .and_then(|marker| nearest_value(marker.1, marker.2))?;
    (minimum.is_finite() && maximum.is_finite() && minimum > 0.0 && maximum > minimum)
        .then_some((minimum, maximum))
}

fn number_token_end(text: &str, start: usize) -> Option<usize> {
    let suffix = text.get(start..)?;
    suffix.char_indices().take_while(|(_, character)| {
        character.is_ascii_digit() || *character == ',' || *character == '.'
    }).last().map(|(index, character)| start + index + character.len_utf8())
}

fn source_text(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(220).collect()
}

fn collapse_numeric_spaces(line: &str) -> String {
    let chars = line.chars().collect::<Vec<_>>();
    let mut result = String::with_capacity(line.len());
    for (index, character) in chars.iter().copied().enumerate() {
        if character.is_whitespace() {
            let previous = index.checked_sub(1).and_then(|at| chars.get(at)).copied();
            let next = chars.get(index + 1).copied();
            if previous.is_some_and(|value| value.is_ascii_digit() || value == ',' || value == '.')
                && next.is_some_and(|value| value.is_ascii_digit() || value == ',' || value == '.') {
                continue;
            }
        }
        result.push(character);
    }
    result
}

fn number_tokens(text: &str) -> Vec<(f64, usize)> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut token = String::new();
    for (index, character) in text.char_indices() {
        if character.is_ascii_digit() || character == ',' || character == '.' {
            if start.is_none() { start = Some(index); }
            token.push(if character == ',' { '.' } else { character });
        } else if let Some(token_start) = start.take() {
            if token.chars().any(|value| value.is_ascii_digit()) {
                if let Ok(value) = token.parse::<f64>() { tokens.push((value, token_start)); }
            }
            token.clear();
        }
    }
    if let Some(token_start) = start {
        if token.chars().any(|value| value.is_ascii_digit()) {
            if let Ok(value) = token.parse::<f64>() { tokens.push((value, token_start)); }
        }
    }
    tokens
}

fn last_number(text: &str) -> Option<(f64, usize)> { number_tokens(text).into_iter().last() }
fn first_number(text: &str) -> Option<(f64, usize)> { number_tokens(text).into_iter().next() }

fn decimal_label(value: f64) -> String {
    let mut label = format!("{value:.4}");
    while label.contains('.') && label.ends_with('0') { label.pop(); }
    if label.ends_with('.') { label.pop(); }
    label.replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distance_measurement(value_mm: f64, uncertainty_mm: Option<f64>) -> Measurement {
        Measurement {
            id: 7,
            kind: Tool::Distance,
            points: vec![
                geometry::Point { x: 0.0, y: 0.0 },
                geometry::Point { x: value_mm * 100.0, y: 0.0 },
            ],
            label: "Longueur".into(),
            label_position_native_px: None,
            capture_id: "capture-test".into(),
            calibration_id: Some("cal-test".into()),
            mm_per_pixel: Some(0.01),
            mm_per_pixel_x: None,
            mm_per_pixel_y: None,
            expanded_uncertainty_mm: uncertainty_mm,
            origin: "test".into(),
            method: None,
            protocol_id: None,
            protocol_version: None,
        }
    }

    fn dimension(confirmed: bool, measurement_id: Option<u64>) -> ReferenceDimension {
        ReferenceDimension {
            id: "cote-1".into(), label: "Longueur".into(), nominal_mm: 1.0,
            tolerance_minus_mm: 0.1, tolerance_plus_mm: 0.1, source_text: "1 ± 0,1".into(),
            confirmed, measurement_id, ocr_page: None, ocr_bbox: None, ocr_confidence: None,
        }
    }

    #[test]
    fn extracts_symmetric_asymmetric_and_explicit_limit_dimensions() {
        let dimensions = extract_dimensions(
            "Ø 4,2 ± 0,1 mm\n4,2 +0,2/-0,1 mm\nMIN 9,9 MAX 10,1 mm",
        );

        assert_eq!(dimensions.len(), 3);
        assert!((dimensions[0].nominal_mm - 4.2).abs() < 1e-12);
        assert_eq!(dimensions[0].tolerance_minus_mm, 0.1);
        assert_eq!(dimensions[0].tolerance_plus_mm, 0.1);
        assert_eq!(dimensions[1].tolerance_minus_mm, 0.1);
        assert_eq!(dimensions[1].tolerance_plus_mm, 0.2);
        assert!((dimensions[2].nominal_mm - 10.0).abs() < 1e-12);
        assert!((dimensions[2].nominal_mm - dimensions[2].tolerance_minus_mm - 9.9).abs() < 1e-12);
        assert!((dimensions[2].nominal_mm + dimensions[2].tolerance_plus_mm - 10.1).abs() < 1e-12);
    }

    #[test]
    fn associates_pdf_text_dimensions_with_their_page() {
        let dimensions = extract_pdf_text_dimensions("4,2 ± 0,1 mm\u{000c}MIN 9,9 MAX 10,1 mm");

        assert_eq!(dimensions.len(), 2);
        assert_eq!(dimensions[0].ocr_page, Some(1));
        assert_eq!(dimensions[1].ocr_page, Some(2));
        assert_eq!(dimensions[0].id, "pdf-text-1");
    }

    #[test]
    fn conformity_guard_band_requires_the_full_uncertainty_interval_inside_limits() {
        let inside = compare_dimension(&dimension(true, Some(7)), &[distance_measurement(1.0, Some(0.02))]);
        let overlapping_limit = compare_dimension(&dimension(true, Some(7)), &[distance_measurement(1.09, Some(0.02))]);
        let outside = compare_dimension(&dimension(true, Some(7)), &[distance_measurement(1.2, Some(0.02))]);
        let missing_uncertainty = compare_dimension(&dimension(true, Some(7)), &[distance_measurement(1.0, None)]);
        let unassociated = compare_dimension(&dimension(true, None), &[distance_measurement(1.0, Some(0.02))]);
        let unconfirmed = compare_dimension(&dimension(false, Some(7)), &[distance_measurement(1.0, Some(0.02))]);

        assert_eq!(inside.status, ComparisonStatus::InTolerance);
        assert_eq!(overlapping_limit.status, ComparisonStatus::ToConfirm);
        assert_eq!(outside.status, ComparisonStatus::OutOfTolerance);
        assert_eq!(missing_uncertainty.status, ComparisonStatus::ToConfirm);
        assert_eq!(unassociated.status, ComparisonStatus::NotAssociated);
        assert_eq!(unconfirmed.status, ComparisonStatus::ToConfirm);
    }

    #[test]
    fn global_status_never_calls_an_incomplete_document_conforming() {
        let document = ReferenceDocument {
            source_file: String::new(), source_name: "plan".into(), extracted_text: String::new(),
            dimensions: vec![dimension(true, Some(7)), dimension(true, None)],
        };

        assert_eq!(part_status(&document, &[distance_measurement(1.0, Some(0.02))]), PartStatus::Incomplete);
        assert_eq!(part_status(&ReferenceDocument { dimensions: Vec::new(), ..document }, &[]), PartStatus::NoDimensions);
    }
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new() -> Result<Self, String> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let path = std::env::temp_dir().join(format!("dper-measure-reference-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).map_err(|error| format!("Dossier temporaire OCR inaccessible : {error}"))?;
        Ok(Self { path })
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.path); }
}

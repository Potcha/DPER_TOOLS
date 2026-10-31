//! Conservative drawing-to-control-plan interpretation. Drawing values are never calibration data.
//!
//! This first increment interprets explicit text only. It does not reconstruct views,
//! datums, leaders or geometric relations from their positions on a drawing.
use crate::{
    dper_pdf_layout::{self, PdfBox, PdfLayout, PdfPageGeometry, PdfTextGroup},
    reference::{self, ReferenceDocument, ReferenceOcrBox},
};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path, process::Command};

pub const DRAWING_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingControlPlan {
    pub schema_version: u32,
    pub source_name: String,
    pub source_file: Option<String>,
    /// Content binding established only by a stable, current file analysis.
    #[serde(default)]
    pub source_fingerprint: Option<String>,
    pub extraction_method: String,
    pub characteristics: Vec<DrawingCharacteristic>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub page_geometry: Vec<PdfPageGeometry>,
    #[serde(default)]
    pub spatial_text_groups: Vec<PdfTextGroup>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingCharacteristic {
    pub id: String,
    pub label: String,
    pub kind: DrawingDimensionKind,
    /// In `unit`, with no implicit millimetre conversion or assumed drawing scale.
    pub nominal: Option<f64>,
    pub unit: Option<DrawingUnit>,
    pub tolerance: Option<DrawingTolerance>,
    pub repetition: Option<u32>,
    pub source: DrawingSource,
    pub measurability: DrawingMeasurability,
    pub reasons: Vec<String>,
    /// Auditable interpretation proposals; source text and numeric values stay intact.
    #[serde(default)]
    pub interpretation_notes: Vec<String>,
    pub confirmed: bool,
    pub measurement_id: Option<u64>,
    /// Empty unless a later, evidence-backed interpretation supplies explicit relations.
    pub relations: Vec<DrawingRelation>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawingDimensionKind {
    Linear,
    Diameter,
    Radius,
    Angle,
    Depth,
    Thread,
    Geometric,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawingUnit {
    Millimeter,
    Centimeter,
    Meter,
    Inch,
    Degree,
}

impl DrawingUnit {
    pub fn millimeters_per_unit(self) -> Option<f64> {
        match self {
            Self::Millimeter => Some(1.0),
            Self::Centimeter => Some(10.0),
            Self::Meter => Some(1000.0),
            Self::Inch => Some(25.4),
            Self::Degree => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingTolerance {
    /// Absolute limits in the characteristic's unit, never an invented tolerance.
    pub lower_limit: f64,
    pub upper_limit: f64,
    pub kind: DrawingToleranceKind,
    pub source_text: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DrawingToleranceKind {
    Symmetric,
    Deviations,
    ExplicitLimits,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingSource {
    /// The entire original text line, preserving spaces and symbols.
    pub exact_text: String,
    pub page: Option<u32>,
    pub bbox: Option<ReferenceOcrBox>,
    pub confidence: Option<f64>,
    pub method: String,
    /// UTF-8 byte offsets within exact_text, not image or PDF coordinates.
    pub text_start: Option<usize>,
    pub text_end: Option<usize>,
    /// PDF page points; this is independent from the optional OCR pixel box.
    #[serde(default)]
    pub pdf_bbox: Option<PdfBox>,
    #[serde(default)]
    pub word_ids: Vec<String>,
    /// Explicitly proposed reconstruction. `exact_text` and source words stay intact.
    #[serde(default)]
    pub interpretation_text: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DrawingMeasurability {
    PlanarCandidate,
    NeedsConfirmation,
    NotMeasurable2d,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingRelation {
    pub kind: String,
    pub target_id: String,
    pub source_text: String,
}

impl DrawingCharacteristic {
    pub fn nominal_mm(&self) -> Option<f64> {
        let value = self.nominal? * self.unit?.millimeters_per_unit()?;
        value.is_finite().then_some(value)
    }

    pub fn limits_mm(&self) -> Option<(f64, f64)> {
        let scale = self.unit?.millimeters_per_unit()?;
        let tolerance = self.tolerance.as_ref()?;
        let limits = (tolerance.lower_limit * scale, tolerance.upper_limit * scale);
        (limits.0.is_finite() && limits.1.is_finite()).then_some(limits)
    }
}

/// Propose an evidence-backed interpretation without confirming it or altering its source.
/// The supplied note identifies the drawing location or evidence justifying the proposal.
pub fn propose_interpretation(
    plan: &mut DrawingControlPlan,
    feature_id: &str,
    kind: DrawingDimensionKind,
    unit: Option<DrawingUnit>,
    source_note: &str,
) -> Result<(), String> {
    if source_note.trim().is_empty() || source_note.chars().count() > 2_000 {
        return Err(
            "La source de l'interprétation doit être renseignée et limitée à 2 000 caractères."
                .into(),
        );
    }
    if feature_id.trim().is_empty() {
        return Err("L'identifiant de caractéristique est obligatoire.".into());
    }
    let mut matching = plan
        .characteristics
        .iter()
        .enumerate()
        .filter(|(_, feature)| feature.id == feature_id);
    let index = matching
        .next()
        .map(|(index, _)| index)
        .ok_or("Caractéristique absente du plan de contrôle.")?;
    if matching.next().is_some() {
        return Err("Identifiant ambigu : plusieurs caractéristiques portent ce repère.".into());
    }
    let feature = &mut plan.characteristics[index];
    let note = format!(
        "Interprétation proposée : type {:?} -> {:?}; unité {:?} -> {:?}. Justification/source : {}",
        feature.kind, kind, feature.unit, unit, source_note,
    );
    let (measurability, reasons) =
        interpretation_assessment(kind, unit, feature.tolerance.is_some());
    feature.kind = kind;
    feature.unit = unit;
    feature.measurability = measurability;
    feature.reasons = reasons;
    feature.confirmed = false;
    feature.interpretation_notes.push(note);
    Ok(())
}

/// Reuses the application's local PDF/image extraction, including its OCR provenance.
pub fn analyze_path(path: &Path) -> Result<DrawingControlPlan, String> {
    analyze_stable_path(path, analyze_source_path)
}

fn analyze_stable_path(
    path: &Path,
    analyze: impl FnOnce(&Path) -> Result<DrawingControlPlan, String>,
) -> Result<DrawingControlPlan, String> {
    let before = crate::dper_source::fingerprint_file(path)?;
    let mut plan = analyze(path)?;
    let after = crate::dper_source::fingerprint_file(path)?;
    if before != after {
        return Err("Le fichier source a changé pendant l'analyse ; reprenez le plan avant de poursuivre.".into());
    }
    plan.source_file = Some(path.to_string_lossy().into_owned());
    plan.source_fingerprint = Some(after);
    Ok(plan)
}

fn analyze_source_path(path: &Path) -> Result<DrawingControlPlan, String> {
    let mut spatial_warning = None;
    let mut empty_layout = None;
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        match dper_pdf_layout::extract_pdf_layout(path) {
            Ok(layout) if !layout.lines.is_empty() => {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("plan PDF");
                let mut plan = from_pdf_layout(layout, name);
                plan.source_file = Some(path.to_string_lossy().into_owned());
                return Ok(plan);
            }
            Ok(layout) => empty_layout = Some(layout),
            Err(error) => spatial_warning = Some(error),
        }
    }
    let mut plan = analyze_legacy_path(path)?;
    if let Some(warning) = spatial_warning {
        plan.warnings.push(format!(
            "Positions PDF indisponibles ; lecture conservatrice de secours : {warning}"
        ));
    }
    if let Some(layout) = empty_layout {
        plan.page_geometry = layout.pages;
        plan.warnings.extend(layout.warnings);
    }
    Ok(plan)
}

/// Interpret local text groups independently, retaining all original word evidence.
/// No nearest-neighbour association, view identification or CAD relation is inferred.
pub fn from_pdf_layout(layout: PdfLayout, source_name: &str) -> DrawingControlPlan {
    let mut plan = DrawingControlPlan {
        schema_version: DRAWING_SCHEMA_VERSION, source_name: source_name.into(), source_file: None,
        source_fingerprint: None,
        extraction_method: "local_pdf_bbox_layout_conservative".into(), characteristics: Vec::new(),
        warnings: vec!["Les groupes PDF sont des propositions textuelles localisées ; ni vues, ni relations CAD, ni contours photo ne sont reconstruits.".into(),
            "Une cote du plan ne constitue jamais un étalon de calibration ni un contour mesuré.".into()],
        page_geometry: layout.pages, spatial_text_groups: layout.groups,
    };
    plan.warnings.extend(layout.warnings);
    let text = plan
        .spatial_text_groups
        .iter()
        .map(|group| group.source_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let declared = declared_length_unit(&text);
    for group in &plan.spatial_text_groups {
        let proposed = group.proposed_text.as_deref().unwrap_or(&group.source_text);
        let parsed = parse_text_plan(proposed, source_name);
        for (index, mut candidate) in parsed.characteristics.into_iter().enumerate() {
            candidate.id = format!("{}-candidate-{}", group.source_line_id, index + 1);
            candidate.source.exact_text = group.source_text.clone();
            candidate.source.page = Some(group.page);
            candidate.source.pdf_bbox = Some(group.bbox);
            candidate.source.word_ids = group.word_ids.clone();
            candidate.source.method = "pdf_bbox_layout".into();
            if proposed != group.source_text {
                candidate.source.interpretation_text = Some(proposed.into());
                candidate.source.text_start = None;
                candidate.source.text_end = None;
                candidate.interpretation_notes.push(format!("Reconstruction textuelle proposée depuis les mots {} : {:?} -> {:?}. Aucun nominal n'est confirmé.",
                    group.word_ids.join(", "), group.source_text, proposed));
            }
            if candidate.unit.is_none() && candidate.kind != DrawingDimensionKind::Angle {
                if let Some(unit) = declared {
                    candidate.unit = Some(unit);
                    let assessment = interpretation_assessment(
                        candidate.kind,
                        candidate.unit,
                        candidate.tolerance.is_some(),
                    );
                    candidate.measurability = assessment.0;
                    candidate.reasons = assessment.1;
                    candidate.interpretation_notes.push("Unité issue d'une déclaration explicite du document ; vérifier les groupes textuels sources.".into());
                }
            }
            if group.proposed_text.is_none() {
                candidate.nominal = None;
                candidate.tolerance = None;
                candidate.measurability = DrawingMeasurability::NeedsConfirmation;
            }
            candidate
                .reasons
                .extend(group.suspension_reasons.iter().cloned());
            candidate.confirmed = false;
            plan.characteristics.push(candidate);
        }
    }
    for page in &plan.page_geometry {
        if !plan
            .spatial_text_groups
            .iter()
            .any(|group| group.page == page.page)
        {
            plan.warnings.push(format!("Page {} sans texte vectoriel localisable : OCR et lecture visuelle restent nécessaires ; aucune cote n'est supposée extraite.", page.page));
        }
    }
    if plan.characteristics.is_empty() {
        plan.warnings.push("Aucune cote textuelle explicite interprétable ; les groupes spatiaux restent disponibles pour revue.".into());
    }
    plan
}

fn analyze_legacy_path(path: &Path) -> Result<DrawingControlPlan, String> {
    match reference::analyze_document(path) {
        Ok(document) => Ok(from_reference_document(&document)),
        Err(extraction_error) => {
            // Existing extraction falls back to OCR when no toleranced dimension is
            // recognized. A vector PDF with plain dimensions remains usable if OCR
            // is unavailable, without silently losing that original failure.
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
                || !path.is_file()
            {
                return Err(extraction_error);
            }
            let output = Command::new("pdftotext")
                .arg("-layout")
                .arg(path)
                .arg("-")
                .output();
            let Ok(output) = output else {
                return Err(extraction_error);
            };
            let text = String::from_utf8_lossy(&output.stdout);
            if !output.status.success() || text.trim().is_empty() {
                return Err(extraction_error);
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("plan PDF");
            let mut plan = parse_text_plan(&text, name);
            plan.source_file = Some(path.to_string_lossy().into_owned());
            plan.extraction_method = "local_pdf_text_without_ocr".into();
            plan.warnings.push(format!(
                "Extraction de référence incomplète ; texte PDF conservé : {extraction_error}"
            ));
            Ok(plan)
        }
    }
}

/// Builds a plan from raw text and augments candidates with existing extraction evidence.
/// Legacy `_mm` fields alone are deliberately not evidence of a drawing's unit.
pub fn from_reference_document(document: &ReferenceDocument) -> DrawingControlPlan {
    let mut plan = parse_text_plan(&document.extracted_text, &document.source_name);
    plan.source_file = (!document.source_file.is_empty()).then(|| document.source_file.clone());
    plan.extraction_method = "local_pdf_text_or_ocr_conservative".into();
    let mut associated = BTreeSet::new();
    for dimension in &document.dimensions {
        let normalized = normalized_line(&dimension.source_text);
        let matched = plan
            .characteristics
            .iter_mut()
            .enumerate()
            .find(|(index, candidate)| {
                let line = normalized_line(&candidate.source.exact_text);
                let same_source =
                    !normalized.is_empty() && (line == normalized || line.starts_with(&normalized));
                let same_page =
                    dimension.ocr_page.is_none() || candidate.source.page == dimension.ocr_page;
                !associated.contains(index)
                    && same_source
                    && same_page
                    && candidate_matches_reference(candidate, dimension)
            });
        if let Some((index, candidate)) = matched {
            associated.insert(index);
            candidate.id = dimension.id.clone();
            candidate.source.bbox = dimension.ocr_bbox.clone();
            candidate.source.confidence = dimension.ocr_confidence;
            candidate.source.method = if dimension.ocr_bbox.is_some() {
                "ocr"
            } else {
                "pdf_text"
            }
            .into();
            candidate.measurement_id = dimension.measurement_id;
            candidate.confirmed = dimension.confirmed;
            continue;
        }
        let extracted = parse_text_plan(&dimension.source_text, &document.source_name);
        let mut candidate = extracted
            .characteristics
            .into_iter()
            .find(|candidate| candidate_matches_reference(candidate, dimension))
            .unwrap_or_else(|| {
                characteristic(
                    &dimension.source_text,
                    dimension.ocr_page,
                    0,
                    dimension.source_text.len(),
                    DrawingDimensionKind::Unknown,
                    Some(dimension.nominal_mm),
                    None,
                    None,
                    None,
                )
            });
        candidate.id = dimension.id.clone();
        candidate.source.page = dimension.ocr_page;
        candidate.source.bbox = dimension.ocr_bbox.clone();
        candidate.source.confidence = dimension.ocr_confidence;
        candidate.source.method = "legacy_reference_text".into();
        candidate.confirmed = dimension.confirmed;
        candidate.measurement_id = dimension.measurement_id;
        associated.insert(plan.characteristics.len());
        plan.characteristics.push(candidate);
    }
    plan
}

fn candidate_matches_reference(
    candidate: &DrawingCharacteristic,
    dimension: &reference::ReferenceDimension,
) -> bool {
    // Existing ReferenceDimension only stores millimetres. Match its numeric text values
    // without treating that storage convention as a physical unit declaration.
    candidate
        .nominal
        .is_some_and(|value| close(value, dimension.nominal_mm))
        || candidate.tolerance.as_ref().is_some_and(|tolerance| {
            close(
                tolerance.lower_limit,
                dimension.nominal_mm - dimension.tolerance_minus_mm,
            ) && close(
                tolerance.upper_limit,
                dimension.nominal_mm + dimension.tolerance_plus_mm,
            )
        })
}

fn close(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= a.abs().max(b.abs()).max(1.0) * 1e-10
}

fn normalized_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Reads explicit dimension text only. Unsupported notation stays unconfirmed;
/// no implicit relation, drawing scale, general tolerance table or unit is invented.
pub fn parse_text_plan(text: &str, source_name: &str) -> DrawingControlPlan {
    let declared_unit = declared_length_unit(text);
    let mut plan = DrawingControlPlan {
        schema_version: DRAWING_SCHEMA_VERSION,
        source_name: source_name.into(),
        source_file: None,
        source_fingerprint: None,
        extraction_method: "conservative_explicit_text".into(),
        characteristics: Vec::new(),
        page_geometry: Vec::new(),
        spatial_text_groups: Vec::new(),
        warnings: vec![
            "Lecture conservative du texte : vues, contours, références et relations géométriques non reconstruits.".into(),
            "Une cote du plan ne constitue jamais un étalon de calibration ni un contour mesuré.".into(),
        ],
    };
    let mut page = 1u32;
    for page_text in text.split('\u{000c}') {
        for line in page_text.lines() {
            if let Some(ocr_page) = ocr_page_marker(line) {
                page = ocr_page;
                continue;
            }
            let lower = line.to_lowercase();
            if lower.contains("iso 2768")
                || lower.contains("tolérances générales")
                || lower.contains("tolerances generales")
                || lower.contains("general tolerances")
            {
                plan.warnings.push(format!(
                    "Tolérance générale à interpréter explicitement : {line}"
                ));
                continue;
            }
            if metadata_line(&lower) {
                continue;
            }
            let mut candidates = parse_line(line, Some(page), declared_unit);
            for candidate in &mut candidates {
                candidate.id = format!("drawing-{}", plan.characteristics.len() + 1);
                plan.characteristics.push(candidate.clone());
            }
        }
        page = page.saturating_add(1);
    }
    if plan.characteristics.is_empty() {
        plan.warnings.push("Aucune cote textuelle explicite reconnue ; une lecture visuelle du plan reste nécessaire.".into());
    }
    plan
}

fn ocr_page_marker(line: &str) -> Option<u32> {
    let trimmed = line.trim();
    if !trimmed.starts_with("[Page ") || !trimmed.ends_with(']') {
        return None;
    }
    trimmed[6..].split_whitespace().next()?.parse().ok()
}

fn metadata_line(lower: &str) -> bool {
    [
        "échelle",
        "echelle",
        "scale",
        "page ",
        "sheet ",
        "feuille ",
        "révision",
        "revision",
        "date ",
        "dessin ",
        "drawing no",
        "format ",
        "masse",
        "mass ",
        "poids",
        "weight ",
        "unités",
        "unites",
        "units",
    ]
    .iter()
    .any(|prefix| lower.trim_start().starts_with(prefix))
}

fn declared_length_unit(text: &str) -> Option<DrawingUnit> {
    let mut declared = None;
    for line in text.lines() {
        let lower = line.to_lowercase();
        if ![
            "unités",
            "unites",
            "units",
            "toutes les cotes en",
            "all dimensions in",
        ]
        .iter()
        .any(|prefix| lower.trim_start().starts_with(prefix))
        {
            continue;
        }
        let unit = lower
            .split(|c: char| !c.is_alphabetic())
            .find_map(unit_word);
        if let Some(unit) = unit {
            if declared.is_some_and(|previous| previous != unit) {
                return None;
            }
            declared = Some(unit);
        }
    }
    declared
}

fn unit_word(word: &str) -> Option<DrawingUnit> {
    match word {
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" | "millimètre"
        | "millimètres" => Some(DrawingUnit::Millimeter),
        "cm" => Some(DrawingUnit::Centimeter),
        "m" => Some(DrawingUnit::Meter),
        "in" | "inch" | "inches" | "pouce" | "pouces" => Some(DrawingUnit::Inch),
        "deg" | "degree" | "degrees" | "degré" | "degrés" => Some(DrawingUnit::Degree),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
struct Number {
    value: f64,
    start: usize,
    end: usize,
}

fn numbers(text: &str) -> Vec<Number> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        let character = text[cursor..].chars().next().unwrap();
        if !character.is_ascii_digit() {
            cursor += character.len_utf8();
            continue;
        }
        let start = cursor;
        while cursor < text.len() {
            let character = text[cursor..].chars().next().unwrap();
            if !character.is_ascii_digit() && character != '.' && character != ',' {
                break;
            }
            cursor += character.len_utf8();
        }
        if let Ok(value) = text[start..cursor].replace(',', ".").parse::<f64>() {
            if value.is_finite() {
                result.push(Number {
                    value,
                    start,
                    end: cursor,
                });
            }
        }
    }
    result
}

fn following_unit(text: &str) -> (Option<DrawingUnit>, usize) {
    let leading = text.len() - text.trim_start().len();
    let trimmed = text.trim_start();
    if trimmed.starts_with('°') {
        return (Some(DrawingUnit::Degree), leading + '°'.len_utf8());
    }
    if trimmed.starts_with('"') || trimmed.starts_with('″') {
        return (
            Some(DrawingUnit::Inch),
            leading + trimmed.chars().next().unwrap().len_utf8(),
        );
    }
    let end = trimmed
        .char_indices()
        .take_while(|(_, c)| c.is_alphabetic())
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    // Uppercase M is a datum/revision/thread symbol, never the SI metre symbol m.
    let unit = if &trimmed[..end] == "M" {
        None
    } else {
        unit_word(&trimmed[..end].to_lowercase())
    };
    (unit, if unit.is_some() { leading + end } else { 0 })
}

fn parse_line(
    line: &str,
    page: Option<u32>,
    declared: Option<DrawingUnit>,
) -> Vec<DrawingCharacteristic> {
    let tokens = numbers(line);
    if tokens.is_empty() {
        return Vec::new();
    }
    let lower_line = line.to_lowercase();
    let prose_count = (lower_line.contains("depth") || lower_line.contains("profondeur"))
        && [" of ", " des ", " the "]
            .iter()
            .any(|word| lower_line.contains(word))
        && !line.contains('±')
        && !line.contains("+/-")
        && !tokens
            .iter()
            .any(|token| following_unit(&line[token.end..]).0.is_some());
    if prose_count {
        return Vec::new();
    }
    if tokens.windows(2).any(|pair| {
        line[pair[0].end..pair[1].start]
            .chars()
            .all(char::is_whitespace)
    }) && (line.contains('±')
        || line.contains("+/-")
        || tokens
            .iter()
            .any(|token| following_unit(&line[token.end..]).0.is_some()))
    {
        let mut candidate = characteristic(
            line,
            page,
            0,
            line.len(),
            DrawingDimensionKind::Unknown,
            None,
            None,
            None,
            None,
        );
        candidate.reasons.push("Chiffres fragmentés sans preuve spatiale : aucun nominal n'est choisi dans cette ligne.".into());
        return vec![candidate];
    }
    if let Some(candidate) = explicit_limits(line, page, declared) {
        return vec![candidate];
    }
    if line
        .split_whitespace()
        .any(|word| word.trim_matches([':', '=']).eq_ignore_ascii_case("MIN"))
        && line
            .split_whitespace()
            .any(|word| word.trim_matches([':', '=']).eq_ignore_ascii_case("MAX"))
    {
        let mut candidate = characteristic(
            line,
            page,
            0,
            line.len(),
            DrawingDimensionKind::Unknown,
            None,
            None,
            None,
            None,
        );
        candidate
            .reasons
            .push("Limites MIN/MAX illisibles ou inversées ; aucun intervalle exploitable.".into());
        return vec![candidate];
    }
    let mut result = Vec::new();
    let mut index = 0;
    let mut pending_repetition = None;
    let mut context_start = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let before = &line[context_start..token.start];
        let after = &line[token.end..];
        let lower_before = before.to_lowercase();
        let after_trimmed = after.trim_start();
        let multiplicative = after_trimmed.starts_with(['x', 'X', '×']);
        let repeated_named = after_trimmed.to_lowercase().starts_with("trous ")
            || after_trimmed.to_lowercase().starts_with("holes ");
        if token.value >= 1.0
            && token.value.fract() == 0.0
            && token.value <= u32::MAX as f64
            && (multiplicative || repeated_named)
            && index + 1 < tokens.len()
            && !ends_symbol(before, "M")
        {
            let following = if multiplicative {
                after_trimmed[after_trimmed.chars().next().unwrap().len_utf8()..].trim_start()
            } else {
                after_trimmed
            };
            if multiplicative && !following.starts_with(['Ø', 'ø', '⌀', 'φ', 'R', 'M']) {
                let mut ambiguous = characteristic(
                    line,
                    page,
                    token.start,
                    line.len(),
                    DrawingDimensionKind::Unknown,
                    None,
                    None,
                    None,
                    None,
                );
                ambiguous.reasons.push("Notation multiplicative ambiguë : répétition et dimensions de rectangle non distinguées.".into());
                result.push(ambiguous);
                break;
            }
            pending_repetition = Some(token.value as u32);
            context_start = token.end;
            index += 1;
            continue;
        }
        let (direct_unit, unit_end) = following_unit(after);
        let mut kind = classify_kind(before, direct_unit);
        if after.chars().next().is_some_and(|c| c.is_alphanumeric())
            && direct_unit.is_none()
            && kind != DrawingDimensionKind::Thread
        {
            // R2C1, part designations and numeric exponents are not a radius R2.
            index += 1;
            continue;
        }
        let embedded = before.chars().last().is_some_and(|c| c.is_alphanumeric());
        let identifier_decimal = before.ends_with('.')
            && before[..before.len() - 1]
                .chars()
                .last()
                .is_some_and(|c| c.is_alphanumeric());
        if (kind == DrawingDimensionKind::Unknown && embedded) || identifier_decimal {
            // Part numbers, datum identifiers and scientific notation are not dimensions.
            index += 1;
            continue;
        }
        let (tolerance, tolerance_end) = parse_tolerance(line, token, token.end + unit_end);
        let tolerance_rest = &line[token.end + unit_end..];
        let tolerance_trimmed = tolerance_rest.trim_start();
        let explicit_tolerance_marker = tolerance_trimmed.starts_with('±')
            || tolerance_trimmed.starts_with("+/-")
            || (tolerance_rest.starts_with(char::is_whitespace)
                && tolerance_trimmed.starts_with(['+', '-', '−']));
        if explicit_tolerance_marker && tolerance.is_none() {
            let mut candidate = characteristic(
                line,
                page,
                token.start,
                line.len(),
                if kind == DrawingDimensionKind::Unknown {
                    DrawingDimensionKind::Linear
                } else {
                    kind
                },
                Some(token.value),
                direct_unit.or(declared),
                None,
                pending_repetition.take(),
            );
            candidate.measurability = DrawingMeasurability::NeedsConfirmation;
            candidate.reasons.push(
                "Tolérance explicite non interprétable ; conserver le texte et vérifier le plan."
                    .into(),
            );
            result.push(candidate);
            break;
        }
        let has_context = [
            "longueur",
            "largeur",
            "hauteur",
            "épaisseur",
            "epaisseur",
            "distance",
            "entraxe",
            "length",
            "width",
            "height",
            "thickness",
        ]
        .iter()
        .any(|word| lower_before.contains(word));
        if kind == DrawingDimensionKind::Unknown
            && (direct_unit.is_some() || tolerance.is_some() || has_context)
        {
            kind = DrawingDimensionKind::Linear;
        }
        if kind == DrawingDimensionKind::Unknown {
            index += 1;
            continue;
        }
        let end = tolerance_end.max(token.end + unit_end);
        let tail_unit = following_unit(&line[end..]).0;
        let unit = direct_unit.or(tail_unit).or_else(|| {
            (kind != DrawingDimensionKind::Angle && kind != DrawingDimensionKind::Thread)
                .then_some(declared)
                .flatten()
        });
        if kind == DrawingDimensionKind::Linear && unit == Some(DrawingUnit::Degree) {
            kind = DrawingDimensionKind::Angle;
        }
        if token.value > 0.0 || kind == DrawingDimensionKind::Angle {
            let mut candidate = characteristic(
                line,
                page,
                token.start,
                end,
                kind,
                (kind != DrawingDimensionKind::Thread).then_some(token.value),
                unit,
                tolerance,
                pending_repetition.take(),
            );
            if (kind == DrawingDimensionKind::Angle) && unit != Some(DrawingUnit::Degree) {
                candidate.measurability = DrawingMeasurability::NeedsConfirmation;
                candidate
                    .reasons
                    .push("Unité angulaire non documentée.".into());
            }
            result.push(candidate);
        }
        context_start = end;
        index += 1;
        while index < tokens.len() && tokens[index].start < end {
            index += 1;
        }
        // A thread's pitch is part of its designation, not another linear dimension.
        if kind == DrawingDimensionKind::Thread
            && index < tokens.len()
            && line[context_start..tokens[index].start]
                .trim()
                .starts_with(['x', 'X', '×'])
        {
            context_start = tokens[index].end;
            index += 1;
        }
    }
    result
}

fn ends_symbol(before: &str, symbol: &str) -> bool {
    let before = before.trim_end();
    let Some(prefix) = before.strip_suffix(symbol) else {
        return false;
    };
    !prefix.chars().last().is_some_and(|c| c.is_alphanumeric())
}

fn classify_kind(before: &str, unit: Option<DrawingUnit>) -> DrawingDimensionKind {
    let lower = before.to_lowercase();
    if before.contains(['⌖', '⏥', '⌓', '⌭', '⌯', '⊥', '∥', '⌰']) {
        return DrawingDimensionKind::Geometric;
    }
    if lower.contains("profondeur") || lower.contains("depth") || before.contains('↧') {
        return DrawingDimensionKind::Depth;
    }
    if ends_symbol(before, "M") || lower.contains("filetage") || lower.contains("thread") {
        return DrawingDimensionKind::Thread;
    }
    if unit == Some(DrawingUnit::Degree) || lower.contains("angle") {
        return DrawingDimensionKind::Angle;
    }
    if before.contains(['Ø', 'ø', '⌀', 'φ'])
        || lower.contains("diamètre")
        || lower.contains("diametre")
        || lower.contains("diameter")
    {
        return DrawingDimensionKind::Diameter;
    }
    if ends_symbol(before, "R")
        || ends_symbol(before, "r")
        || lower.contains("rayon")
        || lower.contains("radius")
    {
        return DrawingDimensionKind::Radius;
    }
    DrawingDimensionKind::Unknown
}

fn characteristic(
    line: &str,
    page: Option<u32>,
    start: usize,
    end: usize,
    kind: DrawingDimensionKind,
    nominal: Option<f64>,
    unit: Option<DrawingUnit>,
    tolerance: Option<DrawingTolerance>,
    repetition: Option<u32>,
) -> DrawingCharacteristic {
    let (measurability, reasons) = interpretation_assessment(kind, unit, tolerance.is_some());
    DrawingCharacteristic {
        id: String::new(),
        label: line.trim().to_owned(),
        kind,
        nominal,
        unit,
        tolerance,
        repetition,
        source: DrawingSource {
            exact_text: line.into(),
            page,
            bbox: None,
            confidence: None,
            method: "explicit_text".into(),
            text_start: Some(start),
            text_end: Some(end),
            pdf_bbox: None,
            word_ids: Vec::new(),
            interpretation_text: None,
        },
        measurability,
        reasons,
        interpretation_notes: Vec::new(),
        confirmed: false,
        measurement_id: None,
        relations: Vec::new(),
    }
}

fn interpretation_assessment(
    kind: DrawingDimensionKind,
    unit: Option<DrawingUnit>,
    has_tolerance: bool,
) -> (DrawingMeasurability, Vec<String>) {
    let mut reasons = Vec::new();
    let mut measurability = DrawingMeasurability::PlanarCandidate;
    match kind {
        DrawingDimensionKind::Depth => {
            measurability = DrawingMeasurability::NotMeasurable2d;
            reasons.push(
                "La profondeur nécessite une observation ou une mesure hors du plan image.".into(),
            );
        }
        DrawingDimensionKind::Thread => {
            measurability = DrawingMeasurability::NotMeasurable2d;
            reasons.push("Le contrôle complet d'un filetage nécessite une méthode dédiée, non une seule image 2D.".into());
        }
        DrawingDimensionKind::Geometric => {
            measurability = DrawingMeasurability::NotMeasurable2d;
            reasons.push(
                "Tolérance géométrique : cadre, références et méthode de contrôle à interpréter."
                    .into(),
            );
        }
        DrawingDimensionKind::Unknown => {
            measurability = DrawingMeasurability::NeedsConfirmation;
            reasons.push("Type de caractéristique non déterminé par le texte.".into());
        }
        _ => reasons
            .push("Vue, visibilité des bords et recalage à vérifier sur l'image réelle.".into()),
    }
    if unit.is_none() {
        if measurability == DrawingMeasurability::PlanarCandidate {
            measurability = DrawingMeasurability::NeedsConfirmation;
        }
        reasons.push("Unité non spécifiée ; aucune unité mm implicite.".into());
    }
    if kind == DrawingDimensionKind::Angle && unit != Some(DrawingUnit::Degree) {
        measurability = DrawingMeasurability::NeedsConfirmation;
        reasons
            .push("Une cote angulaire exige une unité en degrés explicitement justifiée.".into());
    } else if kind != DrawingDimensionKind::Angle && unit == Some(DrawingUnit::Degree) {
        if measurability == DrawingMeasurability::PlanarCandidate {
            measurability = DrawingMeasurability::NeedsConfirmation;
        }
        reasons
            .push("L'unité angulaire est incompatible avec cette caractéristique métrique.".into());
    }
    if !has_tolerance {
        reasons.push(
            "Tolérance non spécifiée ou non interprétée ; verdict de conformité indisponible."
                .into(),
        );
    }
    (measurability, reasons)
}

fn parse_tolerance(line: &str, nominal: Number, from: usize) -> (Option<DrawingTolerance>, usize) {
    let rest = &line[from..];
    let trimmed = rest.trim_start();
    let offset = from + rest.len() - trimmed.len();
    let symmetric_len = if trimmed.starts_with('±') {
        '±'.len_utf8()
    } else if trimmed.starts_with("+/-") {
        3
    } else {
        0
    };
    if symmetric_len > 0 {
        let suffix = &trimmed[symmetric_len..];
        if let Some(value) = numbers(suffix).first().copied() {
            if suffix[..value.start].trim().is_empty() {
                let end = offset + symmetric_len + value.end;
                let lower = nominal.value - value.value;
                let upper = nominal.value + value.value;
                if lower.is_finite() && upper.is_finite() {
                    return (
                        Some(DrawingTolerance {
                            lower_limit: lower,
                            upper_limit: upper,
                            kind: DrawingToleranceKind::Symmetric,
                            source_text: line[offset..end].into(),
                        }),
                        end,
                    );
                }
            }
        }
        return (None, nominal.end);
    }
    if !trimmed.starts_with(['+', '-', '−']) {
        return (None, nominal.end);
    }
    let values = numbers(trimmed);
    if values.len() < 2 {
        return (None, nominal.end);
    }
    let first = values[0];
    let second = values[1];
    let first_sign = trimmed[..first.start].trim();
    let raw_between = &trimmed[first.end..second.start];
    if !raw_between
        .chars()
        .any(|c| c.is_whitespace() || c == '/' || c == '|')
    {
        // Reject dates and arithmetic such as 2026-10-30 or 10-1-2.
        return (None, nominal.end);
    }
    let between = raw_between.trim();
    let second_sign = between.trim_start_matches(['/', '|']).trim();
    let signed = |sign: &str, value: f64| match sign {
        "+" => Some(value),
        "-" | "−" => Some(-value),
        _ => None,
    };
    let (Some(a), Some(b)) = (
        signed(first_sign, first.value),
        signed(second_sign, second.value),
    ) else {
        return (None, nominal.end);
    };
    let lower = nominal.value + a.min(b);
    let upper = nominal.value + a.max(b);
    let end = offset + second.end;
    if !lower.is_finite() || !upper.is_finite() {
        return (None, nominal.end);
    }
    (
        Some(DrawingTolerance {
            lower_limit: lower,
            upper_limit: upper,
            kind: DrawingToleranceKind::Deviations,
            source_text: line[offset..end].into(),
        }),
        end,
    )
}

fn explicit_limits(
    line: &str,
    page: Option<u32>,
    declared: Option<DrawingUnit>,
) -> Option<DrawingCharacteristic> {
    let words = line.split_whitespace().collect::<Vec<_>>();
    let marker_number = |marker: &str| -> Option<Number> {
        let start = words
            .iter()
            .position(|word| word.trim_matches([':', '=']).eq_ignore_ascii_case(marker))?;
        let word = *words.get(start + 1)?;
        let token = numbers(word).first().copied()?;
        (token.start == 0).then_some(token)
    };
    let minimum = marker_number("MIN")?.value;
    let maximum = marker_number("MAX")?.value;
    if minimum < 0.0 || maximum < minimum {
        return None;
    }
    let final_number = numbers(line).last().copied()?;
    let unit = following_unit(&line[final_number.end..]).0.or(declared);
    let kind = match classify_kind(line, unit) {
        DrawingDimensionKind::Unknown => DrawingDimensionKind::Linear,
        value => value,
    };
    Some(characteristic(
        line,
        page,
        0,
        line.len(),
        kind,
        None,
        unit,
        Some(DrawingTolerance {
            lower_limit: minimum,
            upper_limit: maximum,
            kind: DrawingToleranceKind::ExplicitLimits,
            source_text: line.into(),
        }),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SyntheticSource(std::path::PathBuf);

    impl SyntheticSource {
        fn create() -> Self {
            use std::io::Write;
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("dper-drawing-source-test-{}-{stamp}.bin", std::process::id()));
            let mut source = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
            source.write_all(b"synthetic source bytes").unwrap();
            Self(path)
        }
    }

    impl Drop for SyntheticSource {
        fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
    }

    #[test]
    fn stable_file_analysis_binds_all_extraction_results_to_the_actual_source() {
        let source = SyntheticSource::create();
        let plan = analyze_stable_path(&source.0, |_| {
            let mut parsed = parse_text_plan("Ø4 ±0.1 mm", "synthetic extraction");
            parsed.source_file = Some("untrusted prior path.pdf".into());
            Ok(parsed)
        }).unwrap();
        assert_eq!(plan.source_file.as_deref(), Some(source.0.to_string_lossy().as_ref()));
        assert_eq!(plan.source_fingerprint,
            Some(crate::dper_source::fingerprint_bytes(b"synthetic source bytes")));
    }

    #[test]
    fn changing_source_during_extraction_refuses_the_plan() {
        let source = SyntheticSource::create();
        let result = analyze_stable_path(&source.0, |path| {
            std::fs::write(path, b"different source bytes").unwrap();
            Ok(parse_text_plan("Ø4 ±0.1 mm", "synthetic extraction"))
        });
        assert!(result.unwrap_err().contains("changé pendant l'analyse"));
    }

    #[test]
    fn text_and_reference_reconstruction_do_not_claim_a_current_file_fingerprint() {
        let plan = parse_text_plan("Ø4 ±0.1 mm", "unread.pdf");
        assert!(plan.source_fingerprint.is_none());
        let reference = ReferenceDocument::new(Path::new("unread.pdf"), "Ø4 ±0.1 mm".into(), Vec::new());
        assert!(from_reference_document(&reference).source_fingerprint.is_none());
        let mut legacy = serde_json::to_value(plan).unwrap();
        legacy.as_object_mut().unwrap().remove("source_fingerprint");
        let restored: DrawingControlPlan = serde_json::from_value(legacy).unwrap();
        assert!(restored.source_fingerprint.is_none());
    }

    #[test]
    fn spatial_candidates_preserve_oriented_sources_and_never_use_prose_as_dimensions() {
        let layout =
            crate::dper_pdf_layout::parse_bbox_layout(crate::dper_pdf_layout::tests::fixture())
                .unwrap();
        let plan = from_pdf_layout(layout, "generic-oriented-drawing.pdf");
        assert_eq!(plan.page_geometry.len(), 1);
        assert_eq!(plan.spatial_text_groups.len(), 5);
        assert_eq!(plan.characteristics.len(), 3);
        let horizontal = &plan.characteristics[0];
        assert_eq!(horizontal.nominal, Some(18.0));
        assert_eq!(horizontal.source.exact_text, "1 8 ±0,1");
        assert_eq!(
            horizontal.source.interpretation_text.as_deref(),
            Some("18 ±0,1")
        );
        assert!(horizontal.source.pdf_bbox.is_some());
        assert_eq!(horizontal.source.word_ids.len(), 3);
        assert_eq!(plan.characteristics[1].nominal, Some(50.8));
        assert_eq!(plan.characteristics[2].unit, Some(DrawingUnit::Millimeter));
        assert!(plan
            .characteristics
            .iter()
            .all(|candidate| !candidate.confirmed));
        assert!(plan
            .characteristics
            .iter()
            .all(|candidate| candidate.nominal != Some(8.0)));
    }

    #[test]
    fn raw_fragmented_digits_and_revision_letters_never_manufacture_a_nominal_or_unit() {
        for text in ["1 8 ±0,1", "5 0 , 8 ±0,15"] {
            let plan = parse_text_plan(text, "fragmented");
            assert_eq!(plan.characteristics.len(), 1);
            assert_eq!(plan.characteristics[0].nominal, None);
            assert!(plan.characteristics[0].tolerance.is_none());
        }
        assert!(parse_text_plan("Depth of the 5 vertical slots", "prose")
            .characteristics
            .is_empty());
        assert!(parse_text_plan("REV-001 B.1 M", "metadata")
            .characteristics
            .is_empty());
        let plan = parse_text_plan("12 m", "explicit-unit");
        assert_eq!(plan.characteristics[0].unit, Some(DrawingUnit::Meter));
    }

    #[test]
    fn preserves_varied_dimensions_and_explicit_units_without_tolerance() {
        let plan = parse_text_plan(
            "Ø4 mm\nR2,5 mm\nLongueur 12 cm\n45°\nProfondeur 3 mm\nM6x1",
            "plan",
        );
        assert_eq!(plan.characteristics.len(), 6);
        let kinds = plan
            .characteristics
            .iter()
            .map(|c| c.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                DrawingDimensionKind::Diameter,
                DrawingDimensionKind::Radius,
                DrawingDimensionKind::Linear,
                DrawingDimensionKind::Angle,
                DrawingDimensionKind::Depth,
                DrawingDimensionKind::Thread
            ]
        );
        assert_eq!(plan.characteristics[2].nominal_mm(), Some(120.0));
        assert_eq!(plan.characteristics[3].nominal_mm(), None);
        assert_eq!(
            plan.characteristics[4].measurability,
            DrawingMeasurability::NotMeasurable2d
        );
        assert_eq!(plan.characteristics[5].nominal, None);
        assert!(plan
            .characteristics
            .iter()
            .all(|c| c.tolerance.is_none() && !c.confirmed && c.relations.is_empty()));
    }

    #[test]
    fn preserves_repetitions_source_lines_and_pages() {
        let text = "  4 × Ø3,5 ±0,1 mm  \u{000c}R8 mm\n[Page 4 · OCR]\nØ5 mm";
        let plan = parse_text_plan(text, "plan");
        assert_eq!(plan.characteristics.len(), 3);
        assert_eq!(plan.characteristics[0].repetition, Some(4));
        assert_eq!(plan.characteristics[0].nominal, Some(3.5));
        assert_eq!(
            plan.characteristics[0].source.exact_text,
            "  4 × Ø3,5 ±0,1 mm  "
        );
        assert_eq!(plan.characteristics[0].source.page, Some(1));
        assert_eq!(plan.characteristics[1].source.page, Some(2));
        assert_eq!(plan.characteristics[2].source.page, Some(4));
    }

    #[test]
    fn tolerances_are_optional_and_signed_limits_preserve_the_actual_interval() {
        let plan = parse_text_plan(
            "10 ±0,1 mm\n10 +0,2/-0,1 mm\n10 +0,1/+0,2 mm\nMIN 9,9 MAX 10,1 mm\n10 mm",
            "plan",
        );
        assert_eq!(plan.characteristics.len(), 5);
        assert_eq!(plan.characteristics[0].limits_mm(), Some((9.9, 10.1)));
        assert_eq!(plan.characteristics[1].limits_mm(), Some((9.9, 10.2)));
        assert_eq!(plan.characteristics[2].limits_mm(), Some((10.1, 10.2)));
        assert_eq!(plan.characteristics[3].nominal, None);
        assert_eq!(plan.characteristics[3].limits_mm(), Some((9.9, 10.1)));
        assert!(plan.characteristics[4].tolerance.is_none());
    }

    #[test]
    fn missing_units_and_general_tolerances_never_become_millimetres_or_zero_tolerance() {
        let plan = parse_text_plan(
            "Ø4 ±0,1\nISO 2768-mK\nTolérances générales : à vérifier",
            "plan",
        );
        assert_eq!(plan.characteristics.len(), 1);
        assert_eq!(plan.characteristics[0].unit, None);
        assert_eq!(plan.characteristics[0].nominal_mm(), None);
        assert_eq!(
            plan.characteristics[0].measurability,
            DrawingMeasurability::NeedsConfirmation
        );
        assert_eq!(plan.warnings.len(), 4);
        let declared = parse_text_plan("Toutes les cotes en mm\nØ4", "plan");
        assert_eq!(
            declared.characteristics[0].unit,
            Some(DrawingUnit::Millimeter)
        );
        let conflict = parse_text_plan("Units: mm\nUnits: inches\nØ4", "plan");
        assert_eq!(conflict.characteristics[0].unit, None);
    }

    #[test]
    fn ignores_metadata_part_numbers_and_bare_numbers() {
        let plan = parse_text_plan("Page 2\nScale 1:2\nRévision 4\nFormat A4\nABC123\nR2C1\n2026-10-30\n42\nNote 7 : inspection", "plan");
        assert!(plan.characteristics.is_empty());
    }

    #[test]
    fn independent_inline_dimensions_do_not_inherit_the_previous_symbol_or_unit() {
        let plan = parse_text_plan("Ø4 mm R8 mm\nØ2 mm 5", "plan");
        assert_eq!(plan.characteristics.len(), 3);
        assert_eq!(plan.characteristics[1].kind, DrawingDimensionKind::Radius);
        assert!(plan.characteristics.iter().all(|c| c.nominal != Some(5.0)));
    }

    #[test]
    fn preserves_geometric_controls_as_unresolved_instead_of_inventing_relations() {
        let plan = parse_text_plan("⌖ Ø0,1 mm A B", "plan");
        assert_eq!(plan.characteristics.len(), 1);
        assert_eq!(
            plan.characteristics[0].kind,
            DrawingDimensionKind::Geometric
        );
        assert_eq!(
            plan.characteristics[0].measurability,
            DrawingMeasurability::NotMeasurable2d
        );
        assert!(plan.characteristics[0].relations.is_empty());
    }

    #[test]
    fn reference_adapter_keeps_ocr_provenance_without_trusting_legacy_mm_field_names() {
        let dimension = reference::ReferenceDimension {
            id: "ocr-9".into(),
            label: "Ø4".into(),
            nominal_mm: 4.0,
            tolerance_minus_mm: 0.1,
            tolerance_plus_mm: 0.1,
            source_text: "Ø4 ±0,1".into(),
            confirmed: true,
            measurement_id: Some(8),
            ocr_page: Some(2),
            ocr_bbox: Some(ReferenceOcrBox {
                left: 10,
                top: 20,
                width: 80,
                height: 15,
                image_width: 4000,
                image_height: 3000,
            }),
            ocr_confidence: Some(92.0),
        };
        let document = ReferenceDocument {
            source_file: "part.pdf".into(),
            source_name: "part.pdf".into(),
            extracted_text: "\u{000c}Ø4 ±0,1".into(),
            dimensions: vec![dimension],
        };
        let plan = from_reference_document(&document);
        assert_eq!(plan.characteristics.len(), 1);
        let candidate = &plan.characteristics[0];
        assert_eq!(candidate.id, "ocr-9");
        assert_eq!(candidate.unit, None);
        assert_eq!(candidate.source.page, Some(2));
        assert_eq!(candidate.source.bbox.as_ref().unwrap().left, 10);
        assert_eq!(candidate.source.confidence, Some(92.0));
        assert_eq!(candidate.measurement_id, Some(8));
        assert!(candidate.confirmed);
    }

    #[test]
    fn drawing_plan_serialization_roundtrips_optional_data_and_stable_enum_names() {
        let plan = parse_text_plan("Ø4\nMIN 9.9 MAX 10.1 mm", "part.pdf");
        let serialized = serde_json::to_string(&plan).unwrap();
        assert!(serialized.contains("\"kind\":\"diameter\""));
        let read: DrawingControlPlan = serde_json::from_str(&serialized).unwrap();
        assert_eq!(read.schema_version, 1);
        assert_eq!(read.characteristics[0].unit, None);
        assert!(read.characteristics[0].tolerance.is_none());
        assert_eq!(read.characteristics[1].limits_mm(), Some((9.9, 10.1)));
    }

    #[test]
    fn ambiguous_multiplication_and_invalid_tolerance_stay_unresolved() {
        let plan = parse_text_plan(
            "10 × 20 mm\nMIN 11 MAX 9 mm\nØ4 ±abc 0,1 mm\n10 +0,2 mm",
            "plan",
        );
        assert_eq!(plan.characteristics.len(), 4);
        assert!(plan
            .characteristics
            .iter()
            .all(|c| c.measurability == DrawingMeasurability::NeedsConfirmation));
        assert!(plan.characteristics.iter().all(|c| c.tolerance.is_none()));
        assert!(plan.characteristics.iter().all(|c| c.repetition.is_none()));
        assert_eq!(plan.characteristics[0].nominal, None);
        assert_eq!(plan.characteristics[1].nominal, None);
        assert_eq!(plan.characteristics[2].nominal, Some(4.0));
        assert_eq!(plan.characteristics[3].nominal, Some(10.0));
    }

    #[test]
    fn angle_unit_after_tolerance_and_named_hole_repetition_are_preserved() {
        let plan = parse_text_plan("45 ±0,5°\n4 trous Ø3 mm", "plan");
        assert_eq!(plan.characteristics.len(), 2);
        assert_eq!(plan.characteristics[0].kind, DrawingDimensionKind::Angle);
        assert_eq!(plan.characteristics[0].unit, Some(DrawingUnit::Degree));
        assert_eq!(plan.characteristics[0].nominal_mm(), None);
        assert_eq!(plan.characteristics[1].repetition, Some(4));
        assert_eq!(plan.characteristics[1].nominal_mm(), Some(3.0));
    }

    #[test]
    fn repeated_identical_reference_dimensions_keep_distinct_ids_and_associations() {
        let dimension = reference::ReferenceDimension {
            id: "first".into(),
            label: "diamètre".into(),
            nominal_mm: 4.0,
            tolerance_minus_mm: 0.1,
            tolerance_plus_mm: 0.1,
            source_text: "Ø4 ±0,1 mm".into(),
            confirmed: false,
            measurement_id: Some(1),
            ocr_page: Some(1),
            ocr_bbox: None,
            ocr_confidence: None,
        };
        let mut second = dimension.clone();
        second.id = "second".into();
        second.measurement_id = Some(2);
        let document = ReferenceDocument {
            source_file: "drawing.pdf".into(),
            source_name: "drawing.pdf".into(),
            extracted_text: "Ø4 ±0,1 mm\nØ4 ±0,1 mm".into(),
            dimensions: vec![dimension, second],
        };
        let plan = from_reference_document(&document);
        assert_eq!(plan.characteristics.len(), 2);
        assert_eq!(plan.characteristics[0].id, "first");
        assert_eq!(plan.characteristics[1].id, "second");
        assert_eq!(plan.characteristics[0].measurement_id, Some(1));
        assert_eq!(plan.characteristics[1].measurement_id, Some(2));
    }

    #[test]
    fn interpretation_proposals_keep_original_evidence_and_values_and_require_confirmation() {
        let mut plan = parse_text_plan("  4 × Ø3,5 ±0,1  ", "drawing.pdf");
        plan.source_fingerprint = Some("fnv1a64:a430d84680aabd0b".into());
        plan.characteristics[0].confirmed = true;
        plan.characteristics[0].measurement_id = Some(7);
        let original = plan.characteristics[0].clone();
        let source = serde_json::to_string(&original.source).unwrap();
        let tolerance = serde_json::to_string(&original.tolerance).unwrap();
        let note = "Page 1, vue de dessus : cartouche « dimensions en mm » ; rayon selon repère A.";
        propose_interpretation(
            &mut plan,
            "drawing-1",
            DrawingDimensionKind::Radius,
            Some(DrawingUnit::Millimeter),
            note,
        )
        .unwrap();
        assert_eq!(plan.source_fingerprint.as_deref(), Some("fnv1a64:a430d84680aabd0b"));
        let feature = &plan.characteristics[0];
        assert_eq!(feature.kind, DrawingDimensionKind::Radius);
        assert_eq!(feature.unit, Some(DrawingUnit::Millimeter));
        assert_eq!(feature.nominal, original.nominal);
        assert_eq!(feature.repetition, original.repetition);
        assert_eq!(feature.measurement_id, original.measurement_id);
        assert_eq!(serde_json::to_string(&feature.source).unwrap(), source);
        assert_eq!(
            serde_json::to_string(&feature.tolerance).unwrap(),
            tolerance
        );
        assert!(!feature.confirmed);
        assert_eq!(feature.interpretation_notes.len(), 1);
        assert!(feature.interpretation_notes[0].contains("Diameter -> Radius"));
        assert!(feature.interpretation_notes[0].contains("None -> Some(Millimeter)"));
        assert!(feature.interpretation_notes[0].contains(note));
        assert_eq!(feature.measurability, DrawingMeasurability::PlanarCandidate);
        let mut legacy = serde_json::to_value(feature).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("interpretation_notes");
        let restored: DrawingCharacteristic = serde_json::from_value(legacy).unwrap();
        assert!(restored.interpretation_notes.is_empty());
    }

    #[test]
    fn interpretation_rejects_ambiguous_requests_and_keeps_unsupported_2d_types_blocked() {
        let mut plan = parse_text_plan("Ø4", "drawing");
        let original = serde_json::to_string(&plan).unwrap();
        for (id, note) in [("missing", "Page 1"), ("drawing-1", "   "), ("", "Page 1")] {
            assert!(propose_interpretation(
                &mut plan,
                id,
                DrawingDimensionKind::Diameter,
                Some(DrawingUnit::Millimeter),
                note
            )
            .is_err());
            assert_eq!(serde_json::to_string(&plan).unwrap(), original);
        }
        assert!(propose_interpretation(
            &mut plan,
            "drawing-1",
            DrawingDimensionKind::Diameter,
            None,
            &"a".repeat(2_001)
        )
        .is_err());
        plan.characteristics.push(plan.characteristics[0].clone());
        let duplicate = serde_json::to_string(&plan).unwrap();
        assert!(propose_interpretation(
            &mut plan,
            "drawing-1",
            DrawingDimensionKind::Diameter,
            Some(DrawingUnit::Millimeter),
            "Page 1"
        )
        .is_err());
        assert_eq!(serde_json::to_string(&plan).unwrap(), duplicate);
        plan.characteristics.pop();
        for kind in [
            DrawingDimensionKind::Depth,
            DrawingDimensionKind::Thread,
            DrawingDimensionKind::Geometric,
        ] {
            propose_interpretation(
                &mut plan,
                "drawing-1",
                kind,
                Some(DrawingUnit::Millimeter),
                "Page 1, détail A",
            )
            .unwrap();
            assert_eq!(
                plan.characteristics[0].measurability,
                DrawingMeasurability::NotMeasurable2d
            );
            assert!(!plan.characteristics[0].confirmed);
        }
        propose_interpretation(
            &mut plan,
            "drawing-1",
            DrawingDimensionKind::Angle,
            Some(DrawingUnit::Millimeter),
            "Page 1 : unité encore à confirmer",
        )
        .unwrap();
        assert_eq!(
            plan.characteristics[0].measurability,
            DrawingMeasurability::NeedsConfirmation
        );
        propose_interpretation(
            &mut plan,
            "drawing-1",
            DrawingDimensionKind::Angle,
            Some(DrawingUnit::Degree),
            "Page 1 : symbole degré",
        )
        .unwrap();
        assert_eq!(
            plan.characteristics[0].measurability,
            DrawingMeasurability::PlanarCandidate
        );
    }
}


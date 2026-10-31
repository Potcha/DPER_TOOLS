//! Spatial text proposals from Poppler's bbox-layout XHTML. Coordinates are PDF
//! page points with top-left origin, never native photo pixels or physical millimetres.
//! Word strings remain intact; any numeric joining is a separate, reviewable proposal.
use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command};

const MAX_PAGES: usize = 20;
const MAX_WORDS: usize = 10_000;
const MAX_GROUPS: usize = 2_000;
const MAX_XML_BYTES: usize = 16_000_000;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct PdfBox {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}

impl PdfBox {
    fn union(self, other: Self) -> Self {
        Self {
            x_min: self.x_min.min(other.x_min),
            y_min: self.y_min.min(other.y_min),
            x_max: self.x_max.max(other.x_max),
            y_max: self.y_max.max(other.y_max),
        }
    }
    fn center(self) -> (f64, f64) {
        (
            (self.x_min + self.x_max) / 2.0,
            (self.y_min + self.y_max) / 2.0,
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PdfPageGeometry {
    pub page: u32,
    pub width_pt: f64,
    pub height_pt: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextOrientation {
    Horizontal,
    Vertical,
    Ambiguous,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PdfTextWord {
    pub id: String,
    pub text: String,
    pub bbox: PdfBox,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PdfTextLine {
    pub id: String,
    pub page: u32,
    pub bbox: PdfBox,
    pub words: Vec<PdfTextWord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PdfTextGroup {
    pub source_line_id: String,
    pub page: u32,
    pub bbox: PdfBox,
    pub orientation: TextOrientation,
    /// Original decoded words, separated by spaces; exact word strings and their
    /// positions are retained below because PDF does not define literal line spaces.
    pub source_text: String,
    pub proposed_text: Option<String>,
    pub word_ids: Vec<String>,
    pub words: Vec<PdfTextWord>,
    pub suspension_reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PdfLayout {
    pub pages: Vec<PdfPageGeometry>,
    pub lines: Vec<PdfTextLine>,
    pub groups: Vec<PdfTextGroup>,
    pub warnings: Vec<String>,
}

pub fn extract_pdf_layout(path: &Path) -> Result<PdfLayout, String> {
    if !path.is_file() {
        return Err("PDF source introuvable pour l'extraction spatiale.".into());
    }
    let output = Command::new("pdftotext")
        .args(["-bbox-layout", "-enc", "UTF-8", "-f", "1", "-l"])
        .arg(MAX_PAGES.to_string())
        .arg(path)
        .arg("-")
        .output()
        .map_err(|error| format!("Extraction spatiale PDF indisponible : {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Échec de l'extraction spatiale PDF : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.len() > MAX_XML_BYTES {
        return Err("Extraction spatiale PDF trop volumineuse.".into());
    }
    let xml = String::from_utf8(output.stdout).map_err(|_| "Texte spatial PDF non UTF-8.")?;
    let mut layout = parse_bbox_layout(&xml)?;
    layout.warnings.push(format!("Extraction spatiale limitée aux {MAX_PAGES} premières pages ; aucune géométrie vectorielle, vue ou relation de cotation n'est reconstruite."));
    Ok(layout)
}

fn attribute(start: &BytesStart<'_>, name: &[u8]) -> Result<Option<String>, String> {
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|error| format!("Attribut XML invalide : {error}"))?;
        if attribute.key.as_ref() == name {
            let raw = std::str::from_utf8(attribute.value.as_ref())
                .map_err(|_| "Attribut XML non UTF-8.")?;
            return quick_xml::escape::unescape(raw)
                .map(|value| Some(value.into_owned()))
                .map_err(|error| format!("Entité XML invalide : {error}"));
        }
    }
    Ok(None)
}

fn number(start: &BytesStart<'_>, name: &[u8]) -> Result<f64, String> {
    let raw = attribute(start, name)?.ok_or("Coordonnée absente du texte PDF spatial.")?;
    let value: f64 = raw.parse().map_err(|_| "Coordonnée PDF non numérique.")?;
    if !value.is_finite() || value.abs() > 1e7 {
        return Err("Coordonnée PDF non finie ou excessive.".into());
    }
    Ok(value)
}

fn bbox(start: &BytesStart<'_>) -> Result<PdfBox, String> {
    let bbox = PdfBox {
        x_min: number(start, b"xMin")?,
        y_min: number(start, b"yMin")?,
        x_max: number(start, b"xMax")?,
        y_max: number(start, b"yMax")?,
    };
    if bbox.x_min > bbox.x_max || bbox.y_min > bbox.y_max {
        return Err("Boîte PDF inversée.".into());
    }
    Ok(bbox)
}

/// Parse standard Poppler XHTML without resolving its external DOCTYPE or resources.
pub fn parse_bbox_layout(xml: &str) -> Result<PdfLayout, String> {
    if xml.len() > MAX_XML_BYTES {
        return Err("XML spatial PDF trop volumineux.".into());
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut layout = PdfLayout {
        pages: Vec::new(),
        lines: Vec::new(),
        groups: Vec::new(),
        warnings: Vec::new(),
    };
    let mut page = None;
    let mut line: Option<PdfTextLine> = None;
    let mut word: Option<PdfTextWord> = None;
    let mut word_count = 0;
    loop {
        match reader
            .read_event()
            .map_err(|error| format!("XML spatial PDF illisible : {error}"))?
        {
            Event::Start(start) | Event::Empty(start) => {
                let is_empty = xml
                    .as_bytes()
                    .get(reader.buffer_position().saturating_sub(2) as usize)
                    == Some(&b'/');
                match start.local_name().as_ref() {
                    b"page" => {
                        if page.is_some() || layout.pages.len() >= MAX_PAGES {
                            return Err("Pages PDF imbriquées ou limite de pages dépassée.".into());
                        }
                        let width = number(&start, b"width")?;
                        let height = number(&start, b"height")?;
                        if width <= 0.0 || height <= 0.0 {
                            return Err("Dimensions de page PDF invalides.".into());
                        }
                        let current = layout.pages.len() as u32 + 1;
                        layout.pages.push(PdfPageGeometry {
                            page: current,
                            width_pt: width,
                            height_pt: height,
                        });
                        if !is_empty {
                            page = Some(current);
                        }
                    }
                    b"line" => {
                        if line.is_some() || layout.lines.len() >= MAX_GROUPS {
                            return Err(
                                "Lignes PDF imbriquées ou limite de groupes dépassée.".into()
                            );
                        }
                        let current = page.ok_or("Ligne PDF hors d'une page.")?;
                        if !is_empty {
                            line = Some(PdfTextLine {
                                id: format!("pdf-p{current}-line-{}", layout.lines.len() + 1),
                                page: current,
                                bbox: bbox(&start)?,
                                words: Vec::new(),
                            });
                        }
                    }
                    b"word" => {
                        if word.is_some() || line.is_none() || word_count >= MAX_WORDS {
                            return Err("Mot PDF mal placé ou limite de mots dépassée.".into());
                        }
                        word_count += 1;
                        if !is_empty {
                            word = Some(PdfTextWord {
                                id: format!("pdf-word-{word_count}"),
                                text: String::new(),
                                bbox: bbox(&start)?,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(text) => {
                if let Some(word) = &mut word {
                    let raw =
                        std::str::from_utf8(text.as_ref()).map_err(|_| "Mot PDF non UTF-8.")?;
                    word.text.push_str(
                        &quick_xml::escape::unescape(raw)
                            .map_err(|error| format!("Entité de mot PDF invalide : {error}"))?,
                    );
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(word) = &mut word {
                    let raw = std::str::from_utf8(reference.as_ref())
                        .map_err(|_| "Entité PDF non UTF-8.")?;
                    let entity = format!("&{raw};");
                    word.text.push_str(
                        &quick_xml::escape::unescape(&entity)
                            .map_err(|error| format!("Entité de mot PDF non résolue : {error}"))?,
                    );
                }
            }
            Event::CData(text) => {
                if let Some(word) = &mut word {
                    word.text.push_str(
                        std::str::from_utf8(text.as_ref()).map_err(|_| "Mot PDF non UTF-8.")?,
                    );
                }
            }
            Event::End(end) => match end.local_name().as_ref() {
                b"word" => {
                    let word = word.take().ok_or("Fin de mot PDF inattendue.")?;
                    line.as_mut().ok_or("Mot PDF hors ligne.")?.words.push(word);
                }
                b"line" => {
                    if word.is_some() {
                        return Err("Mot PDF non terminé.".into());
                    }
                    let line = line.take().ok_or("Fin de ligne PDF inattendue.")?;
                    if !line.words.is_empty() {
                        layout.lines.push(line);
                    }
                }
                b"page" => {
                    if line.is_some() || word.is_some() || page.take().is_none() {
                        return Err("Page PDF non terminée ou incohérente.".into());
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            // The standard XHTML DOCTYPE is metadata, never an instruction to load it.
            _ => {}
        }
    }
    if layout.pages.is_empty() || page.is_some() || line.is_some() || word.is_some() {
        return Err("Structure spatiale PDF incomplète.".into());
    }
    layout.groups = layout.lines.iter().map(propose_group).collect();
    if layout.groups.is_empty() {
        layout
            .warnings
            .push("Aucun texte vectoriel localisable ; OCR ou lecture visuelle nécessaire.".into());
    }
    Ok(layout)
}

fn orientation(words: &[PdfTextWord]) -> TextOrientation {
    if words.len() < 2 {
        return TextOrientation::Ambiguous;
    }
    let centers = words
        .iter()
        .map(|word| word.bbox.center())
        .collect::<Vec<_>>();
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for &(x, y) in &centers {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let width = max_x - min_x;
    let height = max_y - min_y;
    let direction = if width > height * 3.0 && width > 1e-6 {
        TextOrientation::Horizontal
    } else if height > width * 3.0 && height > 1e-6 {
        TextOrientation::Vertical
    } else {
        return TextOrientation::Ambiguous;
    };
    let mut sign = 0.0f64;
    for pair in centers.windows(2) {
        let delta = if direction == TextOrientation::Horizontal {
            pair[1].0 - pair[0].0
        } else {
            pair[1].1 - pair[0].1
        };
        if delta.abs() < 1e-6 {
            return TextOrientation::Ambiguous;
        }
        if sign != 0.0 && delta.signum() != sign {
            return TextOrientation::Ambiguous;
        }
        sign = delta.signum();
    }
    direction
}

fn numeric_fragment(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_digit() || character == ',' || character == '.')
}

fn adjacent(first: &PdfTextWord, second: &PdfTextWord, orientation: TextOrientation) -> bool {
    let a = first.bbox;
    let b = second.bbox;
    let (axis_a, axis_b, thickness_a, thickness_b, perpendicular) = match orientation {
        TextOrientation::Horizontal => (
            (a.x_min, a.x_max),
            (b.x_min, b.x_max),
            a.y_max - a.y_min,
            b.y_max - b.y_min,
            (a.center().1 - b.center().1).abs(),
        ),
        TextOrientation::Vertical => (
            (a.y_min, a.y_max),
            (b.y_min, b.y_max),
            a.x_max - a.x_min,
            b.x_max - b.x_min,
            (a.center().0 - b.center().0).abs(),
        ),
        TextOrientation::Ambiguous => return false,
    };
    let thickness = thickness_a.max(thickness_b);
    let gap = if axis_b.0 >= axis_a.0 {
        axis_b.0 - axis_a.1
    } else {
        axis_a.0 - axis_b.1
    };
    thickness > 0.0
        && thickness_a.min(thickness_b) >= thickness * 0.65
        && perpendicular <= thickness * 0.2
        && gap >= -thickness * 0.15
        && gap <= thickness * 0.45
}

fn propose_group(line: &PdfTextLine) -> PdfTextGroup {
    let source_text = line
        .words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let orientation = orientation(&line.words);
    let mut reasons = Vec::new();
    let mut output: Vec<String> = Vec::new();
    let cue = source_text.contains(['±', 'Ø', 'ø', '⌀', '°'])
        || source_text.contains("+/-")
        || source_text.split_whitespace().any(|word| {
            matches!(
                word.to_lowercase().as_str(),
                "mm" | "cm" | "inch" | "diametre" | "diamètre" | "diameter" | "radius" | "rayon"
            )
        });
    let mut fragmented = false;
    let mut suspended = false;
    for (index, word) in line.words.iter().enumerate() {
        if cue
            && index > 0
            && numeric_fragment(&line.words[index - 1].text)
            && numeric_fragment(&word.text)
        {
            fragmented = true;
            if adjacent(&line.words[index - 1], word, orientation) {
                output.last_mut().unwrap().push_str(&word.text);
                continue;
            }
            suspended = true;
        }
        output.push(word.text.clone());
    }
    if fragmented && suspended {
        reasons.push(
            "Fragments numériques sans alignement/espacement univoque ; interprétation suspendue."
                .into(),
        );
    }
    if fragmented && !suspended {
        reasons.push("Jonction numérique proposée depuis des mots adjacents et alignés ; vérifier les boîtes sources.".into());
    }
    if orientation == TextOrientation::Ambiguous && line.words.len() > 1 {
        reasons.push(
            "Orientation ou ordre des mots ambigu ; aucun réordonnancement automatique.".into(),
        );
    }
    let bbox = line
        .words
        .iter()
        .map(|word| word.bbox)
        .reduce(PdfBox::union)
        .unwrap_or(line.bbox);
    PdfTextGroup {
        source_line_id: line.id.clone(),
        page: line.page,
        bbox,
        orientation,
        source_text,
        proposed_text: (!suspended).then(|| output.join(" ")),
        word_ids: line.words.iter().map(|word| word.id.clone()).collect(),
        words: line.words.clone(),
        suspension_reasons: reasons,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // Generic fixture: horizontal and vertical glyph fragmentation plus prose and metadata.
    const FIXTURE: &str = r#"<?xml version="1.0"?><!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd"><html xmlns="http://www.w3.org/1999/xhtml"><body><doc><page width="400" height="300">
    <flow><block><line xMin="10" yMin="10" xMax="90" yMax="30"><word xMin="10" yMin="10" xMax="18" yMax="28">1</word><word xMin="20" yMin="10" xMax="28" yMax="28">8</word><word xMin="34" yMin="11" xMax="65" yMax="27">±0,1</word></line>
    <line xMin="100" yMin="50" xMax="118" yMax="110"><word xMin="100" yMin="100" xMax="118" yMax="108">5</word><word xMin="100" yMin="90" xMax="118" yMax="98">0</word><word xMin="100" yMin="80" xMax="118" yMax="88">,</word><word xMin="100" yMin="70" xMax="118" yMax="78">8</word><word xMin="101" yMin="35" xMax="117" yMax="64">±0,15</word></line>
    <line xMin="10" yMin="150" xMax="160" yMax="168"><word xMin="10" yMin="150" xMax="80" yMax="168">Diametre</word><word xMin="90" yMin="150" xMax="130" yMax="168">4.000</word><word xMin="140" yMin="150" xMax="210" yMax="168">+/-0.100</word><word xMin="220" yMin="150" xMax="240" yMax="168">mm</word></line>
    <line xMin="10" yMin="190" xMax="250" yMax="208"><word xMin="10" yMin="190" xMax="230" yMax="208">Depth of the 5 vertical slots</word></line>
    <line xMin="10" yMin="230" xMax="250" yMax="248"><word xMin="10" yMin="230" xMax="130" yMax="248">REV-001 B.1 M</word></line>
    </block></flow></page></doc></body></html>"#;

    #[test]
    fn parses_multiple_orientations_and_preserves_words_before_proposing_joining() {
        let layout = parse_bbox_layout(FIXTURE).unwrap();
        assert_eq!(layout.pages[0].width_pt, 400.0);
        assert_eq!(layout.groups.len(), 5);
        assert_eq!(layout.groups[0].orientation, TextOrientation::Horizontal);
        assert_eq!(layout.groups[0].source_text, "1 8 ±0,1");
        assert_eq!(layout.groups[0].proposed_text.as_deref(), Some("18 ±0,1"));
        assert_eq!(layout.groups[0].words[0].text, "1");
        assert_eq!(layout.groups[1].orientation, TextOrientation::Vertical);
        assert_eq!(layout.groups[1].source_text, "5 0 , 8 ±0,15");
        assert_eq!(
            layout.groups[1].proposed_text.as_deref(),
            Some("50,8 ±0,15")
        );
    }

    #[test]
    fn large_numeric_gaps_suspend_instead_of_manufacturing_a_dimension() {
        let fixture = FIXTURE.replace(
            "xMin=\"20\" yMin=\"10\" xMax=\"28\"",
            "xMin=\"50\" yMin=\"10\" xMax=\"58\"",
        );
        let layout = parse_bbox_layout(&fixture).unwrap();
        assert!(layout.groups[0].proposed_text.is_none());
        assert!(!layout.groups[0].suspension_reasons.is_empty());
    }

    #[test]
    fn namespaces_entities_multiple_pages_and_invalid_coordinates_are_explicit() {
        let fixture = r#"<html><doc><page width="100" height="200"><line xMin="1" yMin="1" xMax="50" yMax="20"><word xMin="1" yMin="1" xMax="50" yMax="20">45&#176; &amp; A</word></line></page><page width="200" height="100"/></doc></html>"#;
        let layout = parse_bbox_layout(fixture).unwrap();
        assert_eq!(layout.pages.len(), 2);
        assert_eq!(layout.groups[0].words[0].text, "45° & A");
        assert!(parse_bbox_layout(&fixture.replace("width=\"100\"", "width=\"NaN\"")).is_err());
        assert!(parse_bbox_layout(&fixture.replace("xMax=\"50\"", "xMax=\"0\"")).is_err());
        assert!(parse_bbox_layout("<html><page width=\"100\" height=\"100\">").is_err());
    }

    pub(crate) fn fixture() -> &'static str {
        FIXTURE
    }
}


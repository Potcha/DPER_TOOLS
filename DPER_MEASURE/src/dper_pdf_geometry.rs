//! Bounded SVG vector localization proposals, never observations or physical measurements.
//! All output points are PDF page points. Curves are flattened within 0.05 page point;
//! unsupported path commands cause abstention for that entire source path.
use crate::{
    dper_pdf_layout::PdfBox,
    geometry::{self, Point},
};
use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use serde::{Deserialize, Serialize};
use std::{f64::consts::TAU, path::Path, process::Command};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_PATHS: usize = 20_000;
const MAX_SEGMENTS: usize = 100_000;
const FLATNESS_PT: f64 = 0.05;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingGeometryCatalog {
    pub document_fingerprint: String,
    pub geometry: SvgGeometryDocument,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SvgGeometryDocument {
    pub page: u32,
    pub width_pt: f64,
    pub height_pt: f64,
    pub view_box: PdfBox,
    pub flattening_tolerance_pt: f64,
    pub primitives: Vec<VectorPrimitive>,
    pub warnings: Vec<String>,
    pub ignored_text_or_image_count: usize,
    pub unsupported_path_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VectorPrimitive {
    pub id: String,
    pub page: u32,
    pub bbox: PdfBox,
    pub kind: VectorPrimitiveKind,
    pub points: Vec<Point>,
    pub closed: bool,
    pub source: VectorSource,
    pub style: VectorStyle,
    pub circle: Option<VectorCircleCandidate>,
    pub confirmed: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VectorPrimitiveKind {
    Line,
    Polyline,
    ClosedPolygon,
    CircleCandidate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VectorSource {
    pub path_id: Option<String>,
    pub element_index: usize,
    pub subpath_index: usize,
    pub element_name: String,
    /// Original opening-element bytes, before XML attribute decoding.
    pub element_attributes: String,
    /// Original d attribute for paths; explicit conversion for basic SVG shapes.
    pub path_data: String,
    pub transform: [f64; 6],
    pub transform_chain: Vec<String>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct VectorStyle {
    pub stroke: Option<String>,
    pub fill: Option<String>,
    pub stroke_width: Option<String>,
    pub stroke_dasharray: Option<String>,
    pub opacity: Option<String>,
    #[serde(default)]
    pub fill_opacity: Option<String>,
    #[serde(default)]
    pub stroke_opacity: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VectorCircleCandidate {
    pub center: Point,
    pub radius_pt: f64,
    pub rms_residual_pt: f64,
    pub max_residual_pt: f64,
    pub angular_coverage_rad: f64,
    pub sample_count: usize,
}

#[derive(Clone, Copy, Debug)]
struct Affine([f64; 6]);
impl Affine {
    const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    fn apply(self, p: Point) -> Result<Point, String> {
        let [a, b, c, d, e, f] = self.0;
        let point = Point {
            x: a * p.x + c * p.y + e,
            y: b * p.x + d * p.y + f,
        };
        if !finite(point) {
            return Err("Coordonnée vectorielle transformée invalide ou excessive.".into());
        }
        Ok(point)
    }
    fn then(self, rhs: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = rhs.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }
}

fn finite(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.x.abs() <= 1e9 && point.y.abs() <= 1e9
}

fn attr(start: &BytesStart<'_>, key: &[u8]) -> Result<Option<String>, String> {
    for item in start.attributes() {
        let item = item.map_err(|error| format!("Attribut SVG invalide : {error}"))?;
        if item.key.as_ref() == key {
            let raw =
                std::str::from_utf8(item.value.as_ref()).map_err(|_| "Attribut SVG non UTF-8.")?;
            return quick_xml::escape::unescape(raw)
                .map(|value| Some(value.into_owned()))
                .map_err(|error| format!("Entité SVG invalide : {error}"));
        }
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug)]
enum Token {
    Command(char),
    Number(f64),
}

fn tokens(data: &str) -> Result<Vec<Token>, String> {
    let bytes = data.as_bytes();
    let mut position = 0;
    let mut output = Vec::new();
    while position < bytes.len() {
        let byte = bytes[position];
        if byte.is_ascii_whitespace() || byte == b',' {
            position += 1;
            continue;
        }
        if byte.is_ascii_alphabetic() {
            output.push(Token::Command(byte as char));
            position += 1;
            continue;
        }
        let start = position;
        if matches!(byte, b'+' | b'-') {
            position += 1;
        }
        let mut digits = 0;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            digits += 1;
            position += 1;
        }
        if bytes.get(position) == Some(&b'.') {
            position += 1;
            while position < bytes.len() && bytes[position].is_ascii_digit() {
                digits += 1;
                position += 1;
            }
        }
        if digits == 0 {
            return Err("Nombre SVG non reconnu.".into());
        }
        if matches!(bytes.get(position), Some(b'e' | b'E')) {
            position += 1;
            if matches!(bytes.get(position), Some(b'+' | b'-')) {
                position += 1;
            }
            let exponent = position;
            while position < bytes.len() && bytes[position].is_ascii_digit() {
                position += 1;
            }
            if exponent == position {
                return Err("Exposant SVG incomplet.".into());
            }
        }
        let number: f64 = data[start..position]
            .parse()
            .map_err(|_| "Nombre SVG invalide.")?;
        if !number.is_finite() || number.abs() > 1e9 {
            return Err("Nombre SVG non fini ou excessif.".into());
        }
        output.push(Token::Number(number));
        if output.len() > MAX_SEGMENTS * 8 {
            return Err("Limite de paramètres vectoriels dépassée.".into());
        }
    }
    Ok(output)
}

fn number_list(value: &str) -> Result<Vec<f64>, String> {
    tokens(value)?
        .into_iter()
        .map(|token| match token {
            Token::Number(value) => Ok(value),
            _ => Err("Liste numérique SVG invalide.".into()),
        })
        .collect()
}

fn transform(value: &str) -> Result<Affine, String> {
    let mut remaining = value.trim();
    let mut output = Affine::IDENTITY;
    while !remaining.is_empty() {
        let open = remaining
            .find('(')
            .ok_or("Transformation SVG sans parenthèse.")?;
        let name = remaining[..open].trim();
        let close = remaining[open + 1..]
            .find(')')
            .map(|index| open + 1 + index)
            .ok_or("Transformation SVG incomplète.")?;
        let values = number_list(&remaining[open + 1..close])?;
        let current = match (name, values.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Affine([*a, *b, *c, *d, *e, *f]),
            ("translate", [x]) => Affine([1.0, 0.0, 0.0, 1.0, *x, 0.0]),
            ("translate", [x, y]) => Affine([1.0, 0.0, 0.0, 1.0, *x, *y]),
            ("scale", [s]) => Affine([*s, 0.0, 0.0, *s, 0.0, 0.0]),
            ("scale", [x, y]) => Affine([*x, 0.0, 0.0, *y, 0.0, 0.0]),
            ("rotate", [angle]) | ("rotate", [angle, _, _]) => {
                let (sin, cos) = angle.to_radians().sin_cos();
                let rotation = Affine([cos, sin, -sin, cos, 0.0, 0.0]);
                if values.len() == 3 {
                    let (x, y) = (values[1], values[2]);
                    Affine([1.0, 0.0, 0.0, 1.0, x, y])
                        .then(rotation)
                        .then(Affine([1.0, 0.0, 0.0, 1.0, -x, -y]))
                } else {
                    rotation
                }
            }
            _ => return Err(format!("Transformation SVG non prise en charge : {name}.")),
        };
        output = output.then(current);
        remaining = remaining[close + 1..]
            .trim_start_matches(|character: char| character.is_whitespace() || character == ',');
    }
    if output
        .0
        .iter()
        .any(|value| !value.is_finite() || value.abs() > 1e12)
    {
        return Err("Matrice SVG excessive.".into());
    }
    Ok(output)
}

fn physical_length(value: &str) -> Result<f64, String> {
    let trimmed = value.trim();
    let split = trimmed
        .find(|character: char| character.is_alphabetic() || character == '%')
        .unwrap_or(trimmed.len());
    let number: f64 = trimmed[..split]
        .parse()
        .map_err(|_| "Dimension SVG illisible.")?;
    let factor = match trimmed[split..].trim() {
        "pt" => 1.0,
        "" | "px" => 0.75,
        "in" => 72.0,
        "mm" => 72.0 / 25.4,
        "cm" => 720.0 / 25.4,
        _ => return Err("Unité de page SVG relative ou non prise en charge.".into()),
    };
    let value = number * factor;
    if !value.is_finite() || value <= 0.0 || value > 1e7 {
        return Err("Dimension SVG invalide.".into());
    }
    Ok(value)
}

fn root_geometry(start: &BytesStart<'_>) -> Result<(f64, f64, PdfBox, Affine), String> {
    let view = attr(start, b"viewBox")?
        .map(|value| number_list(&value))
        .transpose()?;
    let width = attr(start, b"width")?
        .map(|value| physical_length(&value))
        .transpose()?;
    let height = attr(start, b"height")?
        .map(|value| physical_length(&value))
        .transpose()?;
    let view = match view {
        Some(values) if values.len() == 4 && values[2] > 0.0 && values[3] > 0.0 => values,
        Some(_) => return Err("viewBox SVG invalide.".into()),
        None => vec![
            0.0,
            0.0,
            width.ok_or("Largeur ou viewBox SVG absent.")? / 0.75,
            height.ok_or("Hauteur ou viewBox SVG absent.")? / 0.75,
        ],
    };
    let width = width.unwrap_or(view[2]);
    let height = height.unwrap_or(view[3]);
    let bounds = PdfBox {
        x_min: view[0],
        y_min: view[1],
        x_max: view[0] + view[2],
        y_max: view[1] + view[3],
    };
    let aspect = attr(start, b"preserveAspectRatio")?.unwrap_or_else(|| "xMidYMid meet".into());
    let fields = aspect.split_whitespace().collect::<Vec<_>>();
    let mut sx = width / view[2];
    let mut sy = height / view[3];
    let (mut extra_x, mut extra_y) = (0.0, 0.0);
    if fields.first().copied() != Some("none") {
        let alignment = fields.first().copied().unwrap_or("xMidYMid");
        if !matches!(
            alignment,
            "xMinYMin"
                | "xMidYMin"
                | "xMaxYMin"
                | "xMinYMid"
                | "xMidYMid"
                | "xMaxYMid"
                | "xMinYMax"
                | "xMidYMax"
                | "xMaxYMax"
        ) {
            return Err("Alignement viewBox SVG non pris en charge.".into());
        }
        let scale = match fields.get(1).copied().unwrap_or("meet") {
            "meet" => sx.min(sy),
            "slice" => sx.max(sy),
            _ => return Err("Ajustement viewBox SVG non pris en charge.".into()),
        };
        sx = scale;
        sy = scale;
        extra_x = (width - view[2] * scale)
            * if alignment.starts_with("xMid") {
                0.5
            } else if alignment.starts_with("xMax") {
                1.0
            } else {
                0.0
            };
        extra_y = (height - view[3] * scale)
            * if alignment.ends_with("YMid") {
                0.5
            } else if alignment.ends_with("YMax") {
                1.0
            } else {
                0.0
            };
    }
    Ok((
        width,
        height,
        bounds,
        Affine([
            sx,
            0.0,
            0.0,
            sy,
            extra_x - view[0] * sx,
            extra_y - view[1] * sy,
        ]),
    ))
}

fn styled(parent: &VectorStyle, start: &BytesStart<'_>) -> Result<(VectorStyle, bool), String> {
    let mut style = parent.clone();
    let mut hidden = false;
    for (key, target) in [
        (b"stroke".as_slice(), &mut style.stroke),
        (b"fill", &mut style.fill),
        (b"stroke-width", &mut style.stroke_width),
        (b"stroke-dasharray", &mut style.stroke_dasharray),
        (b"opacity", &mut style.opacity),
        (b"fill-opacity", &mut style.fill_opacity),
        (b"stroke-opacity", &mut style.stroke_opacity),
    ] {
        if let Some(value) = attr(start, key)? {
            *target = Some(value);
        }
    }
    for key in [b"display".as_slice(), b"visibility"] {
        if attr(start, key)?
            .is_some_and(|value| matches!(value.as_str(), "none" | "hidden" | "collapse"))
        {
            hidden = true;
        }
    }
    if let Some(value) = attr(start, b"style")? {
        for declaration in value.split(';') {
            let Some((key, value)) = declaration.split_once(':') else {
                continue;
            };
            let value = value.trim().to_owned();
            match key.trim() {
                "stroke" => style.stroke = Some(value),
                "fill" => style.fill = Some(value),
                "stroke-width" => style.stroke_width = Some(value),
                "stroke-dasharray" => style.stroke_dasharray = Some(value),
                "opacity" => style.opacity = Some(value),
                "fill-opacity" => style.fill_opacity = Some(value),
                "stroke-opacity" => style.stroke_opacity = Some(value),
                "display" | "visibility"
                    if matches!(value.as_str(), "none" | "hidden" | "collapse") =>
                {
                    hidden = true
                }
                _ => {}
            }
        }
    }
    if !positive_opacity(style.opacity.as_deref()) {
        hidden = true;
    }
    Ok((style, hidden))
}

fn positive_opacity(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        value
            .trim()
            .trim_end_matches('%')
            .parse::<f64>()
            .ok()
            .is_some_and(|value| value.is_finite() && value > 0.0)
    })
}

fn has_paint(style: &VectorStyle, name: &str) -> bool {
    let fill = !matches!(
        style.fill.as_deref().unwrap_or("black").trim(),
        "none" | "transparent"
    ) && positive_opacity(style.fill_opacity.as_deref())
        && name != "line";
    let stroke_width = style.stroke_width.as_deref().is_none_or(|value| {
        value
            .trim()
            .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%')
            .parse::<f64>()
            .ok()
            .is_some_and(|width| width.is_finite() && width > 0.0)
    });
    let stroke = !matches!(
        style.stroke.as_deref().unwrap_or("none").trim(),
        "none" | "transparent"
    ) && positive_opacity(style.stroke_opacity.as_deref())
        && stroke_width;
    fill || stroke
}

fn unresolved_clip(start: &BytesStart<'_>) -> Result<bool, String> {
    let mut clip = attr(start, b"clip-path")?;
    let mut mask = attr(start, b"mask")?;
    if let Some(style) = attr(start, b"style")? {
        for declaration in style.split(';') {
            if let Some((name, value)) = declaration.split_once(':') {
                match name.trim() {
                    "clip-path" => clip = Some(value.trim().into()),
                    "mask" => mask = Some(value.trim().into()),
                    _ => {}
                }
            }
        }
    }
    Ok(clip
        .iter()
        .chain(mask.iter())
        .any(|value| value.trim() != "none"))
}

#[derive(Clone)]
struct Frame {
    matrix: Affine,
    chain: Vec<String>,
    style: VectorStyle,
    skip: bool,
}

fn warning(document: &mut SvgGeometryDocument, message: String) {
    if document.warnings.len() < 32 {
        document.warnings.push(message);
    }
}

/// Render exactly one PDF page through local Poppler; stdout is kept in memory.
/// Bind the proposed vectors to unchanged source contents, not merely a file name.
pub fn analyze_pdf_geometry(path: &Path, page: u32) -> Result<DrawingGeometryCatalog, String> {
    analyze_with_source_check(path, page, extract_pdf_geometry)
}

fn analyze_with_source_check(
    path: &Path,
    page: u32,
    extract: impl FnOnce(&Path, u32) -> Result<SvgGeometryDocument, String>,
) -> Result<DrawingGeometryCatalog, String> {
    if page == 0 || page > 20 {
        return Err("Page hors du domaine 1 à 20.".into());
    }
    let before = crate::dper_source::fingerprint_file(path)?;
    let geometry = extract(path, page)?;
    let after = crate::dper_source::fingerprint_file(path)?;
    if before != after {
        return Err(
            "La source PDF a changé pendant l'extraction vectorielle ; catalogue refusé.".into(),
        );
    }
    Ok(DrawingGeometryCatalog {
        document_fingerprint: before,
        geometry,
    })
}

pub fn extract_pdf_geometry(path: &Path, page: u32) -> Result<SvgGeometryDocument, String> {
    if !path.is_file() || page == 0 || page > 20 {
        return Err("PDF absent ou page hors du domaine 1 à 20.".into());
    }
    let output = Command::new("pdftocairo")
        .args(["-svg", "-f"])
        .arg(page.to_string())
        .arg("-l")
        .arg(page.to_string())
        .arg(path)
        .arg("-")
        .output()
        .map_err(|error| format!("Extraction vectorielle PDF indisponible : {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Extraction SVG impossible : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.len() > MAX_BYTES {
        return Err("SVG au-delà du budget de 16 Mio.".into());
    }
    parse_svg_geometry(
        &String::from_utf8(output.stdout).map_err(|_| "SVG non UTF-8.")?,
        page,
    )
}

pub fn parse_svg_geometry(svg: &str, page: u32) -> Result<SvgGeometryDocument, String> {
    if svg.len() > MAX_BYTES || page == 0 {
        return Err("SVG trop volumineux ou page invalide.".into());
    }
    let mut reader = Reader::from_str(svg);
    reader.config_mut().expand_empty_elements = true;
    let mut document: Option<SvgGeometryDocument> = None;
    let mut stack: Vec<Frame> = Vec::new();
    let mut elements = 0;
    let mut segments = 0;
    let mut source_bytes = 0usize;
    loop {
        match reader
            .read_event()
            .map_err(|error| format!("XML SVG invalide : {error}"))?
        {
            Event::Start(start) => {
                if stack.len() >= 64 {
                    return Err("Imbrication SVG excessive.".into());
                }
                let name = std::str::from_utf8(start.local_name().as_ref())
                    .map_err(|_| "Élément SVG non UTF-8.")?
                    .to_owned();
                if stack.is_empty() {
                    if name != "svg" || document.is_some() {
                        return Err("Racine SVG absente ou multiple.".into());
                    }
                    let (width, height, view, mut matrix) = root_geometry(&start)?;
                    let mut chain = vec![format!("viewBox -> page points: {:?}", matrix.0)];
                    if let Some(value) = attr(&start, b"transform")? {
                        matrix = matrix.then(transform(&value)?);
                        chain.push(value);
                    }
                    document=Some(SvgGeometryDocument{page,width_pt:width,height_pt:height,view_box:view,flattening_tolerance_pt:FLATNESS_PT,
                        primitives:Vec::new(),warnings:vec!["Géométries nominales candidates en points PDF : aucune observation photo, unité physique, vue ou relation de cotation n'est validée.".into()],ignored_text_or_image_count:0,unsupported_path_count:0});
                    let (style, hidden) = styled(&VectorStyle::default(), &start)?;
                    let clipped = unresolved_clip(&start)?;
                    if clipped {
                        warning(document.as_mut().unwrap(), "Découpage/masque SVG non résolu à la racine : toutes les géométries concernées sont suspendues.".into());
                    }
                    stack.push(Frame {
                        matrix,
                        chain,
                        style,
                        skip: hidden || clipped,
                    });
                    continue;
                }
                let parent = stack.last().unwrap();
                let (style, hidden) = styled(&parent.style, &start)?;
                let mut frame = Frame {
                    matrix: parent.matrix,
                    chain: parent.chain.clone(),
                    style,
                    skip: parent.skip || hidden,
                };
                let doc = document.as_mut().unwrap();
                if !frame.skip && unresolved_clip(&start)? {
                    frame.skip = true;
                    warning(doc, "Découpage/masque SVG non résolu : élément et descendants suspendus, aucun contour complet supposé.".into());
                }
                if let Some(value) = attr(&start, b"transform")? {
                    frame.chain.push(value.clone());
                    match transform(&value) {
                        Ok(local) => frame.matrix = frame.matrix.then(local),
                        Err(error) => {
                            frame.skip = true;
                            warning(doc, error);
                        }
                    }
                }
                if matches!(
                    name.as_str(),
                    "defs"
                        | "symbol"
                        | "clipPath"
                        | "mask"
                        | "pattern"
                        | "marker"
                        | "linearGradient"
                        | "radialGradient"
                        | "style"
                        | "text"
                        | "tspan"
                        | "image"
                        | "use"
                        | "svg"
                ) {
                    frame.skip = true;
                    if matches!(name.as_str(), "text" | "image" | "use") {
                        doc.ignored_text_or_image_count += 1;
                    }
                    if name == "use"
                        && !attr(&start, b"href")?
                            .or(attr(&start, b"xlink:href")?)
                            .is_some_and(|value| value.starts_with("#glyph"))
                    {
                        warning(doc,"Référence SVG use non textuelle ignorée : son contenu n'est pas supposé être un contour.".into());
                    }
                    if name == "svg" {
                        warning(
                            doc,
                            "Viewport SVG imbriqué ignoré ; aucun repère de page n'est supposé."
                                .into(),
                        );
                    }
                }
                if !frame.skip
                    && matches!(
                        name.as_str(),
                        "path" | "line" | "polyline" | "polygon" | "rect" | "circle"
                    )
                {
                    if !has_paint(&frame.style, &name) {
                        stack.push(frame);
                        continue;
                    }
                    elements += 1;
                    if elements > MAX_PATHS {
                        return Err("Limite de chemins vectoriels dépassée.".into());
                    }
                    if attr(&start, b"class")?.is_some() {
                        warning(
                            doc,
                            "Classe CSS SVG présente : styles externes non interprétés.".into(),
                        );
                    }
                    let data = shape_data(&start, &name);
                    let result = data
                        .as_ref()
                        .map_err(Clone::clone)
                        .and_then(|data| parse_path(data, frame.matrix, &mut segments));
                    match result {
                        Ok(subpaths) => {
                            for (subpath_index, subpath) in subpaths.into_iter().enumerate() {
                                if subpath.points.len() < 2 {
                                    continue;
                                }
                                let cost = start.as_ref().len()
                                    + data.as_ref().unwrap().len()
                                    + frame.chain.iter().map(String::len).sum::<usize>();
                                source_bytes = source_bytes
                                    .checked_add(cost)
                                    .ok_or("Limite de provenance vectorielle dépassée.")?;
                                if source_bytes > MAX_BYTES || doc.primitives.len() >= MAX_PATHS {
                                    return Err("Limite de primitives ou de provenance vectorielle dépassée.".into());
                                }
                                let source = VectorSource {
                                    path_id: attr(&start, b"id")?,
                                    element_index: elements,
                                    subpath_index: subpath_index + 1,
                                    element_name: name.clone(),
                                    element_attributes: String::from_utf8_lossy(start.as_ref())
                                        .into_owned(),
                                    path_data: data.as_ref().unwrap().clone(),
                                    transform: frame.matrix.0,
                                    transform_chain: frame.chain.clone(),
                                };
                                doc.primitives.push(primitive(
                                    page,
                                    subpath,
                                    source,
                                    frame.style.clone(),
                                ));
                            }
                        }
                        Err(error) => {
                            if error.starts_with("Limite") {
                                return Err(error);
                            }
                            doc.unsupported_path_count += 1;
                            warning(doc,format!("Élément vectoriel {elements} ignoré sans géométrie partielle : {error}"));
                        }
                    }
                }
                stack.push(frame);
            }
            Event::End(_) => {
                if stack.pop().is_none() {
                    return Err("Structure SVG incohérente.".into());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("SVG non terminé.".into());
    }
    document.ok_or_else(|| "Aucun document SVG.".into())
}

fn shape_number(start: &BytesStart<'_>, key: &[u8], default: Option<f64>) -> Result<f64, String> {
    match attr(start, key)? {
        Some(value) => {
            let numbers = number_list(&value)?;
            if numbers.len() != 1 {
                return Err("Attribut de forme SVG non scalaire.".into());
            }
            Ok(numbers[0])
        }
        None => default.ok_or_else(|| "Attribut de forme SVG absent.".into()),
    }
}

fn shape_data(start: &BytesStart<'_>, name: &str) -> Result<String, String> {
    match name {
        "path" => attr(start, b"d")?.ok_or_else(|| "Chemin SVG sans d.".into()),
        "line" => Ok(format!(
            "M {} {} L {} {}",
            shape_number(start, b"x1", Some(0.0))?,
            shape_number(start, b"y1", Some(0.0))?,
            shape_number(start, b"x2", Some(0.0))?,
            shape_number(start, b"y2", Some(0.0))?
        )),
        "polyline" | "polygon" => {
            let values = number_list(&attr(start, b"points")?.ok_or("Liste points absente.")?)?;
            if values.len() < 4 || values.len() % 2 != 0 {
                return Err("Liste points SVG incomplète.".into());
            }
            let mut data = format!("M {} {}", values[0], values[1]);
            for pair in values[2..].chunks_exact(2) {
                data.push_str(&format!(" L {} {}", pair[0], pair[1]));
            }
            if name == "polygon" {
                data.push_str(" Z");
            }
            Ok(data)
        }
        "rect" => {
            if shape_number(start, b"rx", Some(0.0))? != 0.0
                || shape_number(start, b"ry", Some(0.0))? != 0.0
            {
                return Err("Rectangle arrondi non pris en charge.".into());
            }
            let (x, y) = (
                shape_number(start, b"x", Some(0.0))?,
                shape_number(start, b"y", Some(0.0))?,
            );
            let (w, h) = (
                shape_number(start, b"width", None)?,
                shape_number(start, b"height", None)?,
            );
            if w <= 0.0 || h <= 0.0 {
                return Err("Rectangle SVG vide.".into());
            }
            Ok(format!("M {x} {y} H {} V {} H {x} Z", x + w, y + h))
        }
        "circle" => {
            let center = Point {
                x: shape_number(start, b"cx", Some(0.0))?,
                y: shape_number(start, b"cy", Some(0.0))?,
            };
            let radius = shape_number(start, b"r", None)?;
            if radius <= 0.0 {
                return Err("Cercle SVG vide.".into());
            }
            let mut data = String::new();
            for index in 0..128 {
                let angle = TAU * index as f64 / 128.0;
                data.push_str(&format!(
                    "{} {} {} ",
                    if index == 0 { "M" } else { "L" },
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin()
                ));
            }
            data.push('Z');
            Ok(data)
        }
        _ => Err("Forme SVG non prise en charge.".into()),
    }
}

struct Subpath {
    points: Vec<Point>,
    closed: bool,
}
fn subpath(mut points: Vec<Point>, explicitly_closed: bool) -> Subpath {
    let mut closed = explicitly_closed;
    if points.len() >= 4 {
        let first = points[0];
        let extent = points
            .iter()
            .map(|point| point.dist(first))
            .fold(0.0, f64::max);
        if points.last().unwrap().dist(first) <= (extent * 1e-8).max(1e-7) {
            points.pop();
            closed = true;
        }
    }
    Subpath { points, closed }
}
fn push(points: &mut Vec<Point>, point: Point, segments: &mut usize) -> Result<(), String> {
    if !finite(point) {
        return Err("Point de chemin invalide.".into());
    }
    if points
        .last()
        .is_none_or(|previous| previous.dist(point) > 1e-10)
    {
        *segments += 1;
        if *segments > MAX_SEGMENTS {
            return Err("Limite de segments vectoriels dépassée.".into());
        }
        points.push(point);
    }
    Ok(())
}
fn line_distance(point: Point, a: Point, b: Point) -> f64 {
    let length = a.dist(b);
    if length < 1e-12 {
        return point.dist(a);
    }
    ((point.x - a.x) * (b.y - a.y) - (point.y - a.y) * (b.x - a.x)).abs() / length
}
fn midpoint(a: Point, b: Point) -> Point {
    Point {
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
    }
}
fn flatten_cubic(
    points: &mut Vec<Point>,
    p: [Point; 4],
    depth: usize,
    segments: &mut usize,
) -> Result<(), String> {
    let excess = p[0].dist(p[1]) + p[1].dist(p[2]) + p[2].dist(p[3]) - p[0].dist(p[3]);
    if line_distance(p[1], p[0], p[3]).max(line_distance(p[2], p[0], p[3])) <= FLATNESS_PT
        && excess <= FLATNESS_PT
    {
        return push(points, p[3], segments);
    }
    if depth >= 12 {
        return Err("Courbe cubique hors budget de précision.".into());
    }
    let (a, b, c) = (
        midpoint(p[0], p[1]),
        midpoint(p[1], p[2]),
        midpoint(p[2], p[3]),
    );
    let (d, e) = (midpoint(a, b), midpoint(b, c));
    let f = midpoint(d, e);
    flatten_cubic(points, [p[0], a, d, f], depth + 1, segments)?;
    flatten_cubic(points, [f, e, c, p[3]], depth + 1, segments)
}

fn parse_path(data: &str, matrix: Affine, segments: &mut usize) -> Result<Vec<Subpath>, String> {
    let tokens = tokens(data)?;
    let mut index = 0;
    let mut command = None;
    let mut current = Point::default();
    let mut start = current;
    let mut points: Vec<Point> = Vec::new();
    let mut paths = Vec::new();
    let mut started = false;
    while index < tokens.len() {
        if let Token::Command(value) = tokens[index] {
            command = Some(value);
            index += 1;
            if value.eq_ignore_ascii_case(&'z') {
                if !started {
                    return Err("Fermeture SVG sans sous-chemin.".into());
                }
                if points.len() > 1 && points.last().unwrap().dist(points[0]) < 1e-9 {
                    points.pop();
                }
                paths.push(subpath(std::mem::take(&mut points), true));
                current = start;
                started = false;
                command = None;
                continue;
            }
        }
        let cmd = command.ok_or("Paramètres SVG sans commande.")?;
        let count=match cmd.to_ascii_uppercase(){'M'|'L'=>2,'H'|'V'=>1,'C'=>6,'Q'=>4,_=>return Err(format!("Commande {cmd} non prise en charge (arcs SVG et raccourcis demandent une interprétation dédiée)."))};
        if index + count > tokens.len() {
            return Err("Commande de chemin SVG incomplète.".into());
        }
        let mut values = Vec::with_capacity(count);
        for token in &tokens[index..index + count] {
            match token {
                Token::Number(value) => values.push(*value),
                _ => return Err("Commande SVG interrompue.".into()),
            }
        }
        index += count;
        let relative = cmd.is_ascii_lowercase();
        let point = |x: f64, y: f64| Point {
            x: x + if relative { current.x } else { 0.0 },
            y: y + if relative { current.y } else { 0.0 },
        };
        match cmd.to_ascii_uppercase() {
            'M' => {
                if !points.is_empty() {
                    paths.push(subpath(std::mem::take(&mut points), false));
                }
                current = point(values[0], values[1]);
                start = current;
                started = true;
                push(&mut points, matrix.apply(current)?, segments)?;
                command = Some(if relative { 'l' } else { 'L' });
            }
            'L' | 'H' | 'V' => {
                if !started {
                    return Err("Chemin SVG sans commande M initiale.".into());
                }
                current = match cmd.to_ascii_uppercase() {
                    'H' => Point {
                        x: values[0] + if relative { current.x } else { 0.0 },
                        y: current.y,
                    },
                    'V' => Point {
                        x: current.x,
                        y: values[0] + if relative { current.y } else { 0.0 },
                    },
                    _ => point(values[0], values[1]),
                };
                push(&mut points, matrix.apply(current)?, segments)?;
            }
            'C' => {
                if !started {
                    return Err("Courbe SVG sans commande M initiale.".into());
                }
                let end = point(values[4], values[5]);
                flatten_cubic(
                    &mut points,
                    [
                        matrix.apply(current)?,
                        matrix.apply(point(values[0], values[1]))?,
                        matrix.apply(point(values[2], values[3]))?,
                        matrix.apply(end)?,
                    ],
                    0,
                    segments,
                )?;
                current = end;
            }
            'Q' => {
                if !started {
                    return Err("Courbe SVG sans commande M initiale.".into());
                }
                let control = point(values[0], values[1]);
                let end = point(values[2], values[3]);
                let c1 = Point {
                    x: current.x + (control.x - current.x) * 2.0 / 3.0,
                    y: current.y + (control.y - current.y) * 2.0 / 3.0,
                };
                let c2 = Point {
                    x: end.x + (control.x - end.x) * 2.0 / 3.0,
                    y: end.y + (control.y - end.y) * 2.0 / 3.0,
                };
                flatten_cubic(
                    &mut points,
                    [
                        matrix.apply(current)?,
                        matrix.apply(c1)?,
                        matrix.apply(c2)?,
                        matrix.apply(end)?,
                    ],
                    0,
                    segments,
                )?;
                current = end;
            }
            _ => unreachable!(),
        }
    }
    if !points.is_empty() {
        paths.push(subpath(points, false));
    }
    Ok(paths)
}

fn primitive(
    page: u32,
    path: Subpath,
    source: VectorSource,
    style: VectorStyle,
) -> VectorPrimitive {
    let mut bounds = PdfBox {
        x_min: f64::INFINITY,
        y_min: f64::INFINITY,
        x_max: f64::NEG_INFINITY,
        y_max: f64::NEG_INFINITY,
    };
    for point in &path.points {
        bounds.x_min = bounds.x_min.min(point.x);
        bounds.y_min = bounds.y_min.min(point.y);
        bounds.x_max = bounds.x_max.max(point.x);
        bounds.y_max = bounds.y_max.max(point.y);
    }
    let circle = circle_candidate(&path);
    let kind = if circle.is_some() {
        VectorPrimitiveKind::CircleCandidate
    } else if path.points.len() == 2 {
        VectorPrimitiveKind::Line
    } else if path.closed {
        VectorPrimitiveKind::ClosedPolygon
    } else {
        VectorPrimitiveKind::Polyline
    };
    VectorPrimitive {
        id: format!(
            "pdf-p{page}-vector-{}-subpath-{}",
            source.element_index, source.subpath_index
        ),
        page,
        bbox: bounds,
        kind,
        points: path.points,
        closed: path.closed,
        source,
        style,
        circle,
        confirmed: false,
    }
}

fn circle_candidate(path: &Subpath) -> Option<VectorCircleCandidate> {
    if !path.closed || path.points.len() < 24 {
        return None;
    }
    let fit = geometry::fit_circle(&path.points)?;
    if fit.radius <= 0.0 || !finite(fit.center) {
        return None;
    }
    let mut samples = Vec::with_capacity(path.points.len() * 2);
    let mut angles = Vec::with_capacity(path.points.len());
    for index in 0..path.points.len() {
        let point = path.points[index];
        samples.push(point);
        samples.push(midpoint(
            point,
            path.points[(index + 1) % path.points.len()],
        ));
        angles.push(
            (point.y - fit.center.y)
                .atan2(point.x - fit.center.x)
                .rem_euclid(TAU),
        );
    }
    angles.sort_by(f64::total_cmp);
    let mut max_gap = angles[0] + TAU - angles[angles.len() - 1];
    for pair in angles.windows(2) {
        max_gap = max_gap.max(pair[1] - pair[0]);
    }
    let coverage = TAU - max_gap;
    let residuals = samples
        .iter()
        .map(|point| (point.dist(fit.center) - fit.radius).abs())
        .collect::<Vec<_>>();
    let rms =
        (residuals.iter().map(|value| value * value).sum::<f64>() / residuals.len() as f64).sqrt();
    let max = residuals.iter().copied().fold(0.0, f64::max);
    if !rms.is_finite()
        || coverage < TAU * 0.90
        || max_gap > TAU / 12.0
        || rms > fit.radius * 0.0025
        || max > fit.radius * 0.005
    {
        return None;
    }
    Some(VectorCircleCandidate {
        center: fit.center,
        radius_pt: fit.radius,
        rms_residual_pt: rms,
        max_residual_pt: max,
        angular_coverage_rad: coverage,
        sample_count: samples.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn svg(body: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200pt" height="200pt" viewBox="0 0 200 200">{body}</svg>"#
        )
    }

    #[test]
    fn catalogs_bind_unchanged_contents_and_refuse_a_source_changed_during_extraction() {
        use std::{fs, io::Write};
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "dper-vector-source-{}-{stamp}.bin",
            std::process::id()
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(b"original source").unwrap();
        drop(file);
        struct TemporaryFile(std::path::PathBuf);
        impl Drop for TemporaryFile {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let source = TemporaryFile(path);
        let text = svg(r#"<path d="M 1 1 L 10 10"/>"#);
        let catalog =
            analyze_with_source_check(&source.0, 1, |_, page| parse_svg_geometry(&text, page))
                .unwrap();
        assert_eq!(
            catalog.document_fingerprint,
            crate::dper_source::fingerprint_file(&source.0).unwrap()
        );
        assert_eq!(catalog.geometry.page, 1);
        let changed = analyze_with_source_check(&source.0, 1, |path, page| {
            fs::write(path, b"modified source").unwrap();
            parse_svg_geometry(&text, page)
        });
        assert!(changed.unwrap_err().contains("a changé"));
    }

    #[test]
    fn nested_affine_transforms_keep_source_and_split_independent_subpaths() {
        let text = svg(
            r#"<g transform="translate(10,20)" stroke="black"><g transform="rotate(90)"><path id="two" d="M 0 0 h 10 M 20 0 v 5" transform="scale(2)"/></g></g>"#,
        );
        let document = parse_svg_geometry(&text, 3).unwrap();
        assert_eq!(document.primitives.len(), 2);
        let first = &document.primitives[0];
        assert_eq!(first.kind, VectorPrimitiveKind::Line);
        assert!(first.points[0].dist(Point { x: 10.0, y: 20.0 }) < 1e-10);
        assert!(first.points[1].dist(Point { x: 10.0, y: 40.0 }) < 1e-10);
        assert_eq!(first.source.path_id.as_deref(), Some("two"));
        assert_eq!(first.source.path_data, "M 0 0 h 10 M 20 0 v 5");
        assert_eq!(first.style.stroke.as_deref(), Some("black"));
        assert_eq!(first.page, 3);
        assert!(!first.confirmed);
        assert_ne!(first.id, document.primitives[1].id);
    }

    #[test]
    fn definitions_text_images_and_hidden_paths_never_become_contours() {
        let text = svg(
            r##"<defs><path id="glyph-1" d="M 0 0 L 4 4"/></defs><use href="#glyph-1"/><text x="1" y="1">18</text><image href="data:image/png;base64,AA=="/><g style="display:none"><path d="M 2 2 L 6 6"/></g><path id="visible" style="stroke:blue;fill:none;stroke-width:0.3" d="M 10 10 L 20 20"/>"##,
        );
        let document = parse_svg_geometry(&text, 1).unwrap();
        assert_eq!(document.primitives.len(), 1);
        assert_eq!(document.ignored_text_or_image_count, 3);
        assert_eq!(document.primitives[0].style.stroke.as_deref(), Some("blue"));
        assert_eq!(document.primitives[0].style.fill.as_deref(), Some("none"));
    }

    #[test]
    fn unresolved_clips_masks_and_transparent_paint_abstain_including_inherited_styles() {
        let clipped = svg(
            r##"<g clip-path="url(#crop)"><circle cx="40" cy="40" r="10"/></g><circle cx="80" cy="80" r="10" clip-path="url(#crop)"/><g style="mask:url(#mask)"><circle cx="20" cy="20" r="5"/></g>"##,
        );
        let document = parse_svg_geometry(&clipped, 1).unwrap();
        assert!(document.primitives.is_empty());
        assert!(document
            .warnings
            .iter()
            .any(|message| message.contains("suspendus")));
        let transparent = svg(
            r#"<circle cx="40" cy="40" r="10" fill="none" stroke="none"/><g fill-opacity="0"><circle cx="20" cy="20" r="5"/></g><circle cx="20" cy="20" r="5" fill="none" stroke="black" stroke-opacity="0"/>"#,
        );
        assert!(parse_svg_geometry(&transparent, 1)
            .unwrap()
            .primitives
            .is_empty());
        let root_hidden = r#"<svg width="100pt" height="100pt" opacity="0"><circle cx="20" cy="20" r="5"/></svg>"#;
        assert!(parse_svg_geometry(root_hidden, 1)
            .unwrap()
            .primitives
            .is_empty());
        let root_style = r#"<svg width="100pt" height="100pt" stroke="green" fill="none"><line x1="0" y1="0" x2="20" y2="20"/></svg>"#;
        assert_eq!(
            parse_svg_geometry(root_style, 1).unwrap().primitives[0]
                .style
                .stroke
                .as_deref(),
            Some("green")
        );
    }

    #[test]
    fn circles_need_closed_distributed_support_and_rectangles_remain_polygons() {
        let mut poly = String::new();
        for index in 0..64 {
            let angle = TAU * index as f64 / 64.0;
            poly.push_str(&format!(
                "{} {} {} ",
                if index == 0 { "M" } else { "L" },
                50.0 + 20.0 * angle.cos(),
                50.0 + 20.0 * angle.sin()
            ));
        }
        poly.push('Z');
        let text = svg(&format!(
            r#"<path d="{poly}"/><rect x="100" y="100" width="30" height="20"/><path d="M 0 0 L 5 5 L 10 0"/>"#
        ));
        let document = parse_svg_geometry(&text, 1).unwrap();
        assert_eq!(document.primitives.len(), 3);
        assert_eq!(
            document.primitives[0].kind,
            VectorPrimitiveKind::CircleCandidate
        );
        assert!((document.primitives[0].circle.as_ref().unwrap().radius_pt - 20.0).abs() < 1e-9);
        assert_eq!(
            document.primitives[1].kind,
            VectorPrimitiveKind::ClosedPolygon
        );
        assert!(document.primitives[1].circle.is_none());
        assert_eq!(document.primitives[2].kind, VectorPrimitiveKind::Polyline);
        let coincident = svg(&format!(
            r#"<path d="{} L 70 50"/>"#,
            poly.trim_end_matches('Z')
        ));
        assert_eq!(
            parse_svg_geometry(&coincident, 1).unwrap().primitives[0].kind,
            VectorPrimitiveKind::CircleCandidate
        );
        let ellipse = parse_svg_geometry(
            &svg(r#"<circle cx="30" cy="30" r="10" transform="scale(2,1)"/>"#),
            1,
        )
        .unwrap();
        assert!(ellipse.primitives[0].circle.is_none());
    }

    #[test]
    fn curves_flatten_with_bounds_and_unsupported_arcs_abstain_entirely() {
        let text = svg(
            r#"<path d="M 0 0 C 0 20 20 20 20 0 Q 30 -10 40 0"/><path d="M 60 60 L 70 70 A 10 10 0 0 1 80 80"/><path d="M 1 1 L NaN 2"/>"#,
        );
        let document = parse_svg_geometry(&text, 1).unwrap();
        assert_eq!(document.primitives.len(), 1);
        assert!(document.primitives[0].points.len() > 8);
        assert_eq!(document.unsupported_path_count, 2);
        let folded =
            parse_svg_geometry(&svg(r#"<path d="M 0 0 C 100 0 -100 0 10 0"/>"#), 1).unwrap();
        assert!(folded.primitives[0].points.len() > 2);
        assert!(folded.primitives[0].bbox.x_min < 0.0 && folded.primitives[0].bbox.x_max > 10.0);
        assert!(parse_svg_geometry("<svg width=\"NaNpt\" height=\"1pt\"/>", 1).is_err());
        assert!(parse_svg_geometry("<svg width=\"100pt\" height=\"100pt\">", 1).is_err());
        let scaled=parse_svg_geometry(r#"<svg width="200pt" height="100pt" viewBox="10 20 100 100" preserveAspectRatio="none"><line stroke="black" x1="10" y1="20" x2="110" y2="120"/></svg>"#,1).unwrap();
        assert_eq!(
            scaled.primitives[0].points,
            vec![Point { x: 0.0, y: 0.0 }, Point { x: 200.0, y: 100.0 }]
        );
    }
}

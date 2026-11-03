//! Small, explicit filled triangles provide geometric hypotheses only.
//! Every threshold is in PDF page points, independent from nominals and scale.
use super::*;
use attachment_chains::{ChainJointTrace, ChainSupportTrace};

pub(super) const TIP_PREFIX: &str = "geometric_triangular_tip_attachment_hypothesis:";
const TIP_SEMANTICS: &str = "explicit_filled_triangle_geometry_only_no_general_arrow_recognition";
const MAX_TIPS: usize = 32;
const MIN_HEIGHT_PT: f64 = 3.0;
const MAX_HEIGHT_PT: f64 = 12.0;
const MIN_BASE_PT: f64 = 1.0;
const MAX_BASE_PT: f64 = 6.0;
const MIN_ASPECT_RATIO: f64 = 2.0;
const BASE_TOLERANCE_PT: f64 = 0.5;
const MIN_DIRECTION_COSINE: f64 = 0.98;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TriangleSourceTrace {
    pub triangle_id: String,
    pub triangle_fingerprint: String,
    pub source: crate::dper_pdf_geometry::VectorSource,
    pub style: crate::dper_pdf_geometry::VectorStyle,
    pub vertices: [Point; 3],
    pub apex_index: usize,
    pub apex: Point,
    pub base: [Point; 2],
    pub base_midpoint: Point,
    pub height_pt: f64,
    pub base_width_pt: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TipTrace {
    pub schema_version: u32,
    pub scope_fingerprint: String,
    pub triangle: TriangleSourceTrace,
    pub ordered_supports: Vec<ChainSupportTrace>,
    pub joints: Vec<ChainJointTrace>,
    pub text_endpoint: Point,
    pub support_endpoint: Point,
    pub text_gap_pt: f64,
    pub base_gap_pt: f64,
    pub base_axial_offset_pt: f64,
    pub direction_cosine: f64,
    pub circle_gap_pt: f64,
    pub tolerance_pt: f64,
    pub joint_tolerance_pt: f64,
    pub base_tolerance_pt: f64,
    pub min_direction_cosine: f64,
    pub min_height_pt: f64,
    pub max_height_pt: f64,
    pub min_base_pt: f64,
    pub max_base_pt: f64,
    pub min_aspect_ratio: f64,
    pub matching_feature_ids: Vec<String>,
    pub matching_geometry_ids: Vec<String>,
    pub matching_triangle_ids: Vec<String>,
    pub matching_support_chains: Vec<Vec<String>>,
    pub unique_within_selected_view: bool,
    pub requires_review: bool,
    pub semantics: String,
}

fn solid_paint(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 6) && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
    }
    if let Some(rgb) = value.strip_prefix("rgb(").and_then(|text| text.strip_suffix(')')) {
        let channels: Vec<_> = rgb.split(',').map(str::trim).collect();
        return channels.len() == 3 && channels.iter().all(|channel| {
            let percent = channel.ends_with('%');
            channel.trim_end_matches('%').parse::<f64>().is_ok_and(|value|
                value.is_finite() && value >= 0.0 && value <= if percent { 100.0 } else { 255.0 })
        });
    }
    matches!(value.as_str(), "black" | "silver" | "gray" | "grey" | "white" | "maroon" | "red"
        | "purple" | "fuchsia" | "green" | "lime" | "olive" | "yellow" | "navy" | "blue" | "teal" | "aqua" | "orange")
}

fn move_pair(text: &str) -> Option<([f64; 2], &str)> {
    let mut rest = text;
    let mut pair = [0.0_f64; 2];
    for value in &mut pair {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        let bytes = rest.as_bytes();
        let mut end = 0;
        if bytes.first().is_some_and(|byte| matches!(byte, b'+' | b'-')) { end += 1; }
        let mut digits = 0;
        while end < bytes.len() && bytes[end].is_ascii_digit() { end += 1; digits += 1; }
        if end < bytes.len() && bytes[end] == b'.' {
            end += 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() { end += 1; digits += 1; }
        }
        if digits == 0 { return None; }
        if end < bytes.len() && matches!(bytes[end], b'e' | b'E') {
            end += 1;
            if end < bytes.len() && matches!(bytes[end], b'+' | b'-') { end += 1; }
            let exponent_start = end;
            while end < bytes.len() && bytes[end].is_ascii_digit() { end += 1; }
            if exponent_start == end { return None; }
        }
        *value = rest[..end].parse().ok()?;
        if !value.is_finite() { return None; }
        rest = &rest[end..];
    }
    Some((pair, rest))
}

fn closed_straight_source(data: &str) -> bool {
    let data = data.trim();
    let Some(close) = data.rfind(['Z', 'z']) else { return false; };
    let open = &data[..close];
    if !straight_single_source(open) { return false; }
    let suffix = data[close + 1..].trim();
    if suffix.is_empty() { return true; }
    // Poppler emits exactly one inert absolute moveto to the original source
    // start after closing a filled triangle. Preserve it in provenance, but do
    // not permit another location, a relative move, drawing, or another subpath.
    let Some(suffix) = suffix.strip_prefix('M') else { return false; };
    let Some((terminal, remaining)) = move_pair(suffix) else { return false; };
    let Some(start) = open.trim_start().strip_prefix('M') else { return false; };
    let Some((initial, _)) = move_pair(start) else { return false; };
    remaining.trim().is_empty() && terminal == initial
}

pub(super) fn triangle_shape(primitive: &VectorPrimitive) -> Option<TriangleSourceTrace> {
    if !primitive.closed || primitive.kind != VectorPrimitiveKind::ClosedPolygon || primitive.points.len() != 3
        || primitive.source.subpath_index != 1 || !matches!(primitive.source.element_name.as_str(), "path" | "polygon")
        || primitive.source.path_data.len() > MAX_SUPPORT_SOURCE_BYTES
        || primitive.source.element_attributes.len() > MAX_SUPPORT_SOURCE_BYTES
        || primitive.source.path_id.as_ref().is_some_and(|id| id.len() > 256) { return None; }
    if !closed_straight_source(&primitive.source.path_data) { return None; }
    let style = &primitive.style;
    if !style.fill.as_deref().is_some_and(solid_paint)
        || !opaque(style.opacity.as_deref()) || !opaque(style.fill_opacity.as_deref())
        || !opaque(style.stroke_opacity.as_deref())
        || !style.stroke_dasharray.as_deref().is_none_or(|value| value.trim() == "none")
        || !style.stroke.as_deref().is_none_or(|value| value.trim() == "none" || solid_paint(value))
        || !style.stroke_width.as_deref().is_none_or(|value| value.trim().strip_suffix("pt").unwrap_or(value.trim())
            .parse::<f64>().is_ok_and(|width| width.is_finite() && width > 0.0)) { return None; }
    let attributes = primitive.source.element_attributes.to_ascii_lowercase();
    if ["marker-", "clip-path", "mask", "filter", "class=", "class ="].iter().any(|name| attributes.contains(name)) { return None; }
    if primitive.source.transform.iter().any(|value| !value.is_finite()) { return None; }
    let [a, b, c, d, _, _] = primitive.source.transform;
    if !(a*d - b*c).is_finite() || (a*d - b*c).abs() < 1e-12 { return None; }
    let vertices: [Point; 3] = primitive.points.as_slice().try_into().ok()?;
    if vertices.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()) { return None; }
    // Re-read the original straight commands with the recorded transform. A
    // polygon classification and three client-supplied points alone cannot
    // establish that those points actually came from this source triangle.
    // The source lexer above permits only numeric straight-path syntax, so it
    // is safe to embed this bounded path in a minimal SVG document.
    let [a, b, c, d, e, f] = primitive.source.transform;
    let source = crate::dper_pdf_geometry::parse_svg_geometry(&format!(
        r#"<svg width="1000pt" height="1000pt" viewBox="0 0 1000 1000"><path fill="black" stroke="none" transform="matrix({a} {b} {c} {d} {e} {f})" d="{}"/></svg>"#,
        primitive.source.path_data), primitive.page).ok()?;
    if source.unsupported_path_count != 0 || source.primitives.len() != 1
        || !source.primitives[0].closed || source.primitives[0].points.len() != 3
        || source.primitives[0].points.iter().zip(vertices).any(|(actual, recorded)| actual.dist(recorded) > 1e-6) {
        return None;
    }
    let mut candidates = Vec::new();
    for index in 0..3 {
        let apex = vertices[index];
        let base = [vertices[(index + 1) % 3], vertices[(index + 2) % 3]];
        let width = base[0].dist(base[1]);
        if !(MIN_BASE_PT..=MAX_BASE_PT).contains(&width) { continue; }
        let height = cross(base[0], base[1], apex).abs() / width;
        let projection = ((apex.x - base[0].x)*(base[1].x - base[0].x)
            + (apex.y - base[0].y)*(base[1].y - base[0].y)) / (width*width);
        if !(MIN_HEIGHT_PT..=MAX_HEIGHT_PT).contains(&height) || height / width < MIN_ASPECT_RATIO
            || !(1.0/3.0..=2.0/3.0).contains(&projection) { continue; }
        candidates.push((index, apex, base, width, height));
    }
    if candidates.len() != 1 { return None; }
    let (apex_index, apex, base, base_width_pt, height_pt) = candidates[0];
    Some(TriangleSourceTrace {
        triangle_id: primitive.id.clone(), triangle_fingerprint: primitive_fingerprint(primitive).ok()?,
        source: primitive.source.clone(), style: primitive.style.clone(), vertices, apex_index, apex, base,
        base_midpoint: Point { x: (base[0].x + base[1].x)/2.0, y: (base[0].y + base[1].y)/2.0 },
        height_pt, base_width_pt,
    })
}

fn eligible_triangle(primitive: &VectorPrimitive, catalog: &SvgGeometryDocument, view: &DrawingView) -> Option<TriangleSourceTrace> {
    if primitive.page != catalog.page || !on_page(primitive.bbox, catalog)
        || primitive.bbox.x_min <= 0.0 || primitive.bbox.y_min <= 0.0
        || primitive.bbox.x_max >= catalog.width_pt || primitive.bbox.y_max >= catalog.height_pt { return None; }
    for point in &primitive.points {
        if point.x < view.bounds.x_min || point.x > view.bounds.x_max || point.y < view.bounds.y_min || point.y > view.bounds.y_max { return None; }
    }
    let actual = PdfBox {
        x_min: primitive.points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        y_min: primitive.points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        x_max: primitive.points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
        y_max: primitive.points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
    };
    if [(actual.x_min, primitive.bbox.x_min), (actual.x_max, primitive.bbox.x_max),
        (actual.y_min, primitive.bbox.y_min), (actual.y_max, primitive.bbox.y_max)]
        .iter().any(|(left, right)| !right.is_finite() || (left-right).abs() > 1e-6) { return None; }
    triangle_shape(primitive)
}

fn alignment(last: &ChainSupportTrace, triangle: &TriangleSourceTrace) -> Option<(f64, f64, f64)> {
    let points = &last.directed_points;
    let end = *points.last()?;
    let previous = *points.get(points.len().checked_sub(2)?)?;
    let (ax, ay) = (triangle.apex.x - triangle.base_midpoint.x, triangle.apex.y - triangle.base_midpoint.y);
    let length = ax.hypot(ay);
    let base_gap = attachment_chains::point_segment_distance(end, triangle.base[0], triangle.base[1]);
    let axial = ((end.x - triangle.base_midpoint.x)*ax + (end.y - triangle.base_midpoint.y)*ay) / length;
    let direction = ((end.x - previous.x)*ax + (end.y - previous.y)*ay) / (end.dist(previous)*length);
    (base_gap <= BASE_TOLERANCE_PT && axial.abs() <= BASE_TOLERANCE_PT && direction >= MIN_DIRECTION_COSINE)
        .then_some((base_gap, axial, direction))
}

fn charge(remaining: &mut usize, amount: usize) -> bool {
    if *remaining < amount { return false; }
    *remaining -= amount;
    true
}

/// None is a whole-stage budget abstention; never return a usable partial set.
pub(super) fn hypotheses(
    catalog: &SvgGeometryDocument, view: &DrawingView,
    features: &BTreeMap<&str, &DrawingCharacteristic>, primitives: &BTreeMap<&str, &VectorPrimitive>,
    supports: &[&VectorPrimitive], topology: &attachment_chains::ChainTopology,
    circles: &[(&String, Point, f64)], feature_id: &str, scope: &str,
    remaining: &mut usize, total_traces: &mut usize, relationships: &mut usize, bytes: &mut usize,
) -> Result<Option<BTreeMap<String, Vec<TipTrace>>>, String> {
    let triangles: Vec<_> = view.geometry_ids.iter().filter_map(|id| eligible_triangle(primitives[id.as_str()], catalog, view)).collect();
    if triangles.is_empty() { return Ok(Some(BTreeMap::new())); }
    if triangles.len() > MAX_TIPS { return Ok(None); }
    let mut oriented = Vec::new();
    for component in &topology.components {
        for reverse in [false, true] {
            let order: Vec<_> = if reverse { component.iter().rev().map(|(index, forward)| (*index, !*forward)).collect() }
                else { component.clone() };
            oriented.push(attachment_chains::directed_sources(supports, &order)?);
        }
    }
    let mut contacts = Vec::new();
    let mut tips_by_support = vec![Vec::new(); oriented.len()];
    let mut supports_by_tip = vec![Vec::new(); triangles.len()];
    for (support, (directed, _)) in oriented.iter().enumerate() {
        for (tip, triangle) in triangles.iter().enumerate() {
            if !charge(remaining, 1) { return Ok(None); }
            if let Some(gaps) = alignment(directed.last().unwrap(), triangle) {
                contacts.push((support, tip, gaps));
                tips_by_support[support].push(tip);
                supports_by_tip[tip].push(support);
            }
        }
    }
    let scope_fingerprint = fingerprint(&(scope, "explicit_triangular_tips_v1", MIN_HEIGHT_PT, MAX_HEIGHT_PT,
        MIN_BASE_PT, MAX_BASE_PT, MIN_ASPECT_RATIO, BASE_TOLERANCE_PT, MIN_DIRECTION_COSINE))?;
    let mut result: BTreeMap<String, Vec<TipTrace>> = BTreeMap::new();
    for (support, tip, (base_gap_pt, base_axial_offset_pt, direction_cosine)) in &contacts {
        let (directed, joints) = &oriented[*support];
        let triangle = &triangles[*tip];
        if !charge(remaining, view.feature_ids.len() + circles.len()) { return Ok(None); }
        let text_endpoint = directed[0].start_endpoint;
        let matching_feature_ids: Vec<_> = view.feature_ids.iter().filter(|id|
            features[id.as_str()].source.pdf_bbox.is_some_and(|bounds| point_box_distance(text_endpoint, bounds) <= ATTACHMENT_TOLERANCE_PT)).cloned().collect();
        let matching_circles: Vec<_> = circles.iter().filter(|(_, center, radius)|
            (triangle.apex.dist(*center) - radius).abs() <= ATTACHMENT_TOLERANCE_PT)
            .map(|(id, center, radius)| ((*id).clone(), *center, *radius)).collect();
        if !matching_feature_ids.iter().any(|id| id == feature_id) { continue; }
        let matching_triangle_ids: Vec<_> = tips_by_support[*support].iter().map(|tip| triangles[*tip].triangle_id.clone()).collect();
        let matching_support_chains: Vec<Vec<String>> = supports_by_tip[*tip].iter()
            .map(|support| oriented[*support].0.iter().map(|source| source.support_id.clone()).collect()).collect();
        *total_traces += matching_circles.len();
        *relationships += matching_circles.len()*(matching_circles.len() + matching_feature_ids.len()
            + matching_triangle_ids.len() + matching_support_chains.iter().map(Vec::len).sum::<usize>());
        if *total_traces > MAX_TRACES || *relationships > MAX_TRACE_RELATIONSHIPS { return Ok(None); }
        for (id, center, radius) in &matching_circles {
            let trace = TipTrace {
                schema_version: 1, scope_fingerprint: scope_fingerprint.clone(), triangle: triangle.clone(),
                ordered_supports: directed.clone(), joints: joints.clone(), text_endpoint,
                support_endpoint: directed.last().unwrap().end_endpoint,
                text_gap_pt: point_box_distance(text_endpoint, features[feature_id].source.pdf_bbox.unwrap()),
                base_gap_pt: *base_gap_pt, base_axial_offset_pt: *base_axial_offset_pt, direction_cosine: *direction_cosine,
                circle_gap_pt: (triangle.apex.dist(*center) - *radius).abs(), tolerance_pt: ATTACHMENT_TOLERANCE_PT,
                joint_tolerance_pt: attachment_chains::JOINT_TOLERANCE_PT, base_tolerance_pt: BASE_TOLERANCE_PT,
                min_direction_cosine: MIN_DIRECTION_COSINE, min_height_pt: MIN_HEIGHT_PT, max_height_pt: MAX_HEIGHT_PT,
                min_base_pt: MIN_BASE_PT, max_base_pt: MAX_BASE_PT, min_aspect_ratio: MIN_ASPECT_RATIO,
                matching_feature_ids: matching_feature_ids.clone(),
                matching_geometry_ids: matching_circles.iter().map(|(id, _, _)| id.clone()).collect(),
                matching_triangle_ids: matching_triangle_ids.clone(), matching_support_chains: matching_support_chains.clone(),
                unique_within_selected_view: false, requires_review: true, semantics: TIP_SEMANTICS.into(),
            };
            *bytes += serde_json::to_vec(&trace).map_err(|error| format!("Trace de pointe impossible : {error}"))?.len();
            if *bytes > MAX_CHAIN_TRACE_BYTES { return Ok(None); }
            result.entry(id.clone()).or_default().push(trace);
        }
    }
    Ok(Some(result))
}

pub(super) fn validate_evidence(encoded: &str, primitives: &BTreeMap<&str, &VectorPrimitive>,
    geometry_id: &str, feature_id: &str) -> Result<(), String> {
    let trace: TipTrace = serde_json::from_str(encoded).map_err(|_| "Trace de pointe triangulaire invalide.")?;
    if trace.schema_version != 1 || !trace.requires_review || trace.semantics != TIP_SEMANTICS
        || trace.tolerance_pt != ATTACHMENT_TOLERANCE_PT || trace.joint_tolerance_pt != attachment_chains::JOINT_TOLERANCE_PT
        || trace.base_tolerance_pt != BASE_TOLERANCE_PT || trace.min_direction_cosine != MIN_DIRECTION_COSINE
        || trace.min_height_pt != MIN_HEIGHT_PT || trace.max_height_pt != MAX_HEIGHT_PT
        || trace.min_base_pt != MIN_BASE_PT || trace.max_base_pt != MAX_BASE_PT || trace.min_aspect_ratio != MIN_ASPECT_RATIO
        || !(1..=attachment_chains::MAX_CHAIN_PATHS).contains(&trace.ordered_supports.len()) {
        return Err("Trace de pointe triangulaire incohérente.".into());
    }
    let source = primitives.get(trace.triangle.triangle_id.as_str()).ok_or("Pointe source absente.")?;
    let current = triangle_shape(source).ok_or("Pointe source devenue incompatible.")?;
    if fingerprint(&current)? != fingerprint(&trace.triangle)? { return Err("La source de la pointe a changé ; reproposez l'association et sa revue.".into()); }
    attachment_chains::validate_source_traces(&trace.ordered_supports, &trace.joints, primitives)?;
    let circle = primitives.get(geometry_id).ok_or("Cercle de la pointe absent.")?;
    let circle_gap = match circle_geometry(circle)? {
        InspectionGeometry::Circle { center, radius } => (trace.triangle.apex.dist(center) - radius).abs(),
        _ => return Err("Géométrie de pointe incompatible.".into()),
    };
    let gaps = alignment(trace.ordered_supports.last().unwrap(), &trace.triangle).ok_or("Attachement de pointe incohérent.")?;
    let unique_ids = |ids: &[String]| !ids.is_empty() && ids.iter().all(|id| !id.is_empty())
        && ids.iter().collect::<BTreeSet<_>>().len() == ids.len();
    let support_ids: Vec<_> = trace.ordered_supports.iter().map(|source| source.support_id.clone()).collect();
    if trace.text_endpoint != trace.ordered_supports[0].start_endpoint
        || trace.support_endpoint != trace.ordered_supports.last().unwrap().end_endpoint
        || (trace.base_gap_pt, trace.base_axial_offset_pt, trace.direction_cosine) != gaps
        || trace.circle_gap_pt != circle_gap
        || !trace.matching_geometry_ids.iter().any(|id| id == geometry_id)
        || !trace.matching_feature_ids.iter().any(|id| id == feature_id)
        || !unique_ids(&trace.matching_geometry_ids) || !unique_ids(&trace.matching_feature_ids)
        || !unique_ids(&trace.matching_triangle_ids)
        || !trace.matching_triangle_ids.contains(&trace.triangle.triangle_id)
        || !trace.matching_support_chains.contains(&support_ids)
        || trace.matching_support_chains.iter().any(|ids| !unique_ids(ids)
            || ids.len() > attachment_chains::MAX_CHAIN_PATHS)
        || trace.matching_support_chains.iter().collect::<BTreeSet<_>>().len() != trace.matching_support_chains.len()
        || (trace.unique_within_selected_view && (trace.matching_geometry_ids.len() != 1
            || trace.matching_feature_ids.len() != 1 || trace.matching_triangle_ids.len() != 1
            || trace.matching_support_chains.len() != 1))
        || !trace.text_gap_pt.is_finite() || trace.text_gap_pt < 0.0 || trace.text_gap_pt > ATTACHMENT_TOLERANCE_PT
        || !trace.circle_gap_pt.is_finite() || trace.circle_gap_pt < 0.0 || trace.circle_gap_pt > ATTACHMENT_TOLERANCE_PT {
        return Err("Écarts de pointe triangulaire incohérents.".into());
    }
    Ok(())
}

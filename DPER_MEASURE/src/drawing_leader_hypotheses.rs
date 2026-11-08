//! Bounded geometric endpoint hints, without leader, arrow or symbol recognition.
//! Only explicitly scoped primitives participate. Ambiguities never disappear.
use super::*;

const ATTACHMENT_TOLERANCE_PT: f64 = 2.0;
const MAX_SUPPORT_POINTS: usize = 32;
const MAX_SUPPORTS: usize = 32;
const MAX_ATTACHMENT_COMPARISONS: usize = 65_536;
const MAX_TRACE_RELATIONSHIPS: usize = 16_384;
const MAX_TRACES: usize = 256;
const MAX_SUPPORT_SOURCE_BYTES: usize = 16_384;
const TRACE_PREFIX: &str = "endpoint_attachment_hypothesis:";
const NO_SEMANTICS: &str = "endpoint_attachment_only_no_leader_or_arrow_recognition";
const MAX_CHAIN_TRACE_BYTES: usize = 262_144;

#[path = "drawing_attachment_chains.rs"]
mod attachment_chains;
pub(super) use attachment_chains::ChainTrace;

#[path = "drawing_triangular_tips.rs"]
mod triangular_tips;
pub(super) use triangular_tips::TipTrace;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct AttachmentTrace {
    schema_version: u32,
    support_id: String,
    support_fingerprint: String,
    scope_fingerprint: String,
    source_element_index: usize,
    source_subpath_index: usize,
    source_path_id: Option<String>,
    text_endpoint: Point,
    circle_endpoint: Point,
    text_endpoint_index: usize,
    text_gap_pt: f64,
    circle_gap_pt: f64,
    tolerance_pt: f64,
    matching_feature_ids: Vec<String>,
    matching_geometry_ids: Vec<String>,
    unique_within_selected_view: bool,
    requires_review: bool,
    semantics: String,
}

#[derive(Default)]
pub(super) struct AttachmentHints {
    pub by_geometry: BTreeMap<String, Vec<AttachmentTrace>>,
    pub chain_by_geometry: BTreeMap<String, Vec<ChainTrace>>,
    pub tip_by_geometry: BTreeMap<String, Vec<TipTrace>>,
    pub preferred_geometry: Option<String>,
    pub abstention_reason: Option<&'static str>,
}

fn abstain(reason: &'static str) -> AttachmentHints {
    AttachmentHints { abstention_reason: Some(reason), ..Default::default() }
}

// A polyline classification alone is insufficient: curves are flattened to
// polylines by extraction. Check original source commands, with a numeric lexer
// that treats e/E inside scientific notation as part of a number.
fn straight_single_source(data: &str) -> bool {
    let bytes = data.as_bytes();
    let mut index = 0;
    let mut command = None;
    let mut values = 0usize;
    let mut moves = 0usize;
    let valid_arguments = |command: Option<u8>, count: usize| match command {
        Some(b'M' | b'L') => count >= 2 && count % 2 == 0,
        Some(b'H' | b'V') => count >= 1,
        _ => false,
    };
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() || bytes[index] == b',' { index += 1; continue; }
        if bytes[index].is_ascii_alphabetic() {
            let next = bytes[index].to_ascii_uppercase();
            if !matches!(next, b'M' | b'L' | b'H' | b'V') { return false; }
            if command.is_some() && !valid_arguments(command, values) { return false; }
            if next == b'M' { moves += 1; }
            if moves != 1 || (command.is_none() && next != b'M') { return false; }
            command = Some(next); values = 0; index += 1; continue;
        }
        if command.is_none() { return false; }
        let start = index;
        if matches!(bytes[index], b'+' | b'-') { index += 1; }
        let mut digits = 0;
        while index < bytes.len() && bytes[index].is_ascii_digit() { index += 1; digits += 1; }
        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_digit() { index += 1; digits += 1; }
        }
        if digits == 0 { return false; }
        if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
            index += 1;
            if index < bytes.len() && matches!(bytes[index], b'+' | b'-') { index += 1; }
            let exponent = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() { index += 1; }
            if index == exponent { return false; }
        }
        if !data[start..index].parse::<f64>().is_ok_and(|value| value.is_finite() && value.abs() <= 1e9) {
            return false;
        }
        values += 1;
    }
    moves == 1 && valid_arguments(command, values)
}

fn opaque(value: Option<&str>) -> bool {
    value.is_none_or(|value| match value.trim() {
        "100%" => true,
        value => value.parse::<f64>().is_ok_and(|value| value == 1.0),
    })
}

fn visible_solid_unfilled(primitive: &VectorPrimitive) -> bool {
    let style = &primitive.style;
    let stroke = style.stroke.as_deref().unwrap_or("").trim();
    let solid = style.stroke_dasharray.as_deref().is_none_or(|value| value.trim() == "none");
    let width = style.stroke_width.as_deref().is_none_or(|value| {
        let value = value.trim().strip_suffix("pt").unwrap_or(value.trim());
        value.parse::<f64>().is_ok_and(|value| value.is_finite() && value > 0.0)
    });
    let attributes = primitive.source.element_attributes.to_ascii_lowercase();
    let unsupported_attribute = ["marker-", "clip-path", "mask", "filter", "class=", "class ="]
        .iter().any(|name| attributes.contains(name));
    style.fill.as_deref().is_some_and(|value| value.trim() == "none")
        && !stroke.is_empty() && !matches!(stroke, "none" | "transparent" | "inherit" | "unset" | "currentColor")
        && !stroke.contains("url(") && !stroke.contains("var(")
        && solid && width && opaque(style.opacity.as_deref()) && opaque(style.stroke_opacity.as_deref())
        && !unsupported_attribute
}

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn touches_segment(a: Point, b: Point, point: Point) -> bool {
    cross(a, b, point).abs() <= 1e-9
        && point.x >= a.x.min(b.x) - 1e-9 && point.x <= a.x.max(b.x) + 1e-9
        && point.y >= a.y.min(b.y) - 1e-9 && point.y <= a.y.max(b.y) + 1e-9
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let (ac, ad, ca, cb) = (cross(a, b, c), cross(a, b, d), cross(c, d, a), cross(c, d, b));
    (ac.is_sign_positive() != ad.is_sign_positive() && ca.is_sign_positive() != cb.is_sign_positive())
        || touches_segment(a, b, c) || touches_segment(a, b, d)
        || touches_segment(c, d, a) || touches_segment(c, d, b)
}

fn eligible_support(primitive: &VectorPrimitive, catalog: &SvgGeometryDocument, view: &DrawingView) -> bool {
    if !matches!(primitive.kind, VectorPrimitiveKind::Line | VectorPrimitiveKind::Polyline)
        || primitive.closed || primitive.circle.is_some()
        || !(2..=MAX_SUPPORT_POINTS).contains(&primitive.points.len())
        || primitive.source.path_data.len() > MAX_SUPPORT_SOURCE_BYTES
        || primitive.source.element_attributes.len() > MAX_SUPPORT_SOURCE_BYTES
        || primitive.source.path_id.as_ref().is_some_and(|id| id.len() > 256)
        || primitive.source.subpath_index != 1
        || !matches!(primitive.source.element_name.as_str(), "path" | "line" | "polyline")
        || !straight_single_source(&primitive.source.path_data)
        || !visible_solid_unfilled(primitive)
        || primitive.page != catalog.page || !on_page(primitive.bbox, catalog)
        // Endpoints on a page edge might be clipped; never infer a full support.
        || primitive.bbox.x_min <= 0.0 || primitive.bbox.y_min <= 0.0
        || primitive.bbox.x_max >= catalog.width_pt || primitive.bbox.y_max >= catalog.height_pt
    { return false; }
    let [a, b, c, d, _, _] = primitive.source.transform;
    let determinant = a * d - b * c;
    if !a.is_finite() || !b.is_finite() || !c.is_finite() || !d.is_finite()
        || !determinant.is_finite() || determinant.abs() < 1e-12 { return false; }
    let mut bounds = PdfBox { x_min: f64::INFINITY, y_min: f64::INFINITY,
        x_max: f64::NEG_INFINITY, y_max: f64::NEG_INFINITY };
    for point in &primitive.points {
        if !point.x.is_finite() || !point.y.is_finite()
            || point.x < view.bounds.x_min - 1e-6 || point.x > view.bounds.x_max + 1e-6
            || point.y < view.bounds.y_min - 1e-6 || point.y > view.bounds.y_max + 1e-6 { return false; }
        bounds.x_min = bounds.x_min.min(point.x); bounds.y_min = bounds.y_min.min(point.y);
        bounds.x_max = bounds.x_max.max(point.x); bounds.y_max = bounds.y_max.max(point.y);
    }
    if [(bounds.x_min, primitive.bbox.x_min), (bounds.y_min, primitive.bbox.y_min),
        (bounds.x_max, primitive.bbox.x_max), (bounds.y_max, primitive.bbox.y_max)]
        .iter().any(|(actual, stored)| (actual - stored).abs() > 1e-6) { return false; }
    for (index, left) in primitive.points.iter().enumerate() {
        for right in &primitive.points[index + 1..] {
            if (left.x - right.x).hypot(left.y - right.y) <= 1e-6 { return false; }
        }
    }
    for turn in primitive.points.windows(3) {
        let (a, b, c) = (turn[0], turn[1], turn[2]);
        if cross(a, b, c).abs() <= 1e-9
            && (b.x - a.x) * (c.x - b.x) + (b.y - a.y) * (c.y - b.y) <= 0.0 { return false; }
    }
    for first in 0..primitive.points.len() - 1 {
        for second in first + 2..primitive.points.len() - 1 {
            if segments_intersect(primitive.points[first], primitive.points[first + 1],
                primitive.points[second], primitive.points[second + 1]) { return false; }
        }
    }
    true
}

pub(super) fn attachment_hypotheses(
    catalog: &SvgGeometryDocument, view: &DrawingView,
    features: &BTreeMap<&str, &DrawingCharacteristic>,
    primitives: &BTreeMap<&str, &VectorPrimitive>, feature_id: &str,
) -> Result<AttachmentHints, String> {
    // This initial advisory is always emitted by the extractor. Any other
    // warning can hide alternatives or paint/style semantics; abstain entirely.
    if catalog.unsupported_path_count > 0 || catalog.warnings.iter().any(|warning|
        !warning.starts_with("Géométries nominales candidates en points PDF")) {
        return Ok(abstain("endpoint_attachment_abstained_due_to_extraction_limitations"));
    }
    let feature = features[feature_id];
    let unresolved_document_conflict = feature.unit_context.as_ref().is_some_and(|context|
        context.document.status == DrawingUnitStatus::Conflict
            && feature.has_unresolved_unit_context());
    let conflicting_explicit_tolerance_units = feature.reasons.iter().any(|reason|
        reason.starts_with("explicit_tolerance_unit_conflict:"));
    if unresolved_document_conflict || conflicting_explicit_tolerance_units {
        return Ok(abstain("endpoint_attachment_abstained_due_to_unit_conflict"));
    }
    // Choosing a scope cannot disambiguate two interpretations of the same
    // PDF words. Include sibling callouts outside the view and older plans
    // that retained only the full line's box.
    let word_ids = feature.source.word_ids.iter().collect::<std::collections::BTreeSet<_>>();
    let has_hidden_competitor = features.iter().any(|(&id, other)| {
        if id == feature_id || other.source.page != feature.source.page {
            return false;
        }
        let same_box = feature.source.pdf_bbox.is_some() && feature.source.pdf_bbox == other.source.pdf_bbox;
        let shared_words = other.source.word_ids.iter().any(|word| word_ids.contains(word));
        // Identical boxes inside this view are already represented explicitly
        // as competing matches in the attachment trace.
        (same_box || shared_words) && !(same_box && view.feature_ids.iter().any(|member| member == id))
    });
    if has_hidden_competitor {
        return Ok(abstain(super::SHARED_CALLOUT_SOURCE));
    }
    let supports: Vec<_> = view.geometry_ids.iter().map(|id| primitives[id.as_str()])
        .filter(|primitive| eligible_support(primitive, catalog, view)).collect();
    if supports.is_empty() { return Ok(AttachmentHints::default()); }
    let circles: Vec<_> = view.geometry_ids.iter().filter_map(|id| {
        let primitive = primitives[id.as_str()];
        match circle_geometry(primitive).ok()? {
            InspectionGeometry::Circle { center, radius } => Some((id, center, radius)), _ => None,
        }
    }).collect();
    let comparisons = supports.len().checked_mul(2)
        .and_then(|count| count.checked_mul(view.feature_ids.len() + circles.len()));
    if supports.len() > MAX_SUPPORTS || comparisons.is_none_or(|count| count > MAX_ATTACHMENT_COMPARISONS) {
        return Ok(abstain("endpoint_attachment_budget_exceeded_no_partial_priority"));
    }
    let feature_sources: Vec<_> = view.feature_ids.iter().map(|id|
        Ok((id, feature_fingerprint(features[id.as_str()])?))).collect::<Result<_, String>>()?;
    let geometry_sources: Vec<_> = view.geometry_ids.iter().map(|id|
        Ok((id, primitive_fingerprint(primitives[id.as_str()])?))).collect::<Result<_, String>>()?;
    let scope_fingerprint = fingerprint(&(view, &feature_sources, &geometry_sources,
        catalog.flattening_tolerance_pt, ATTACHMENT_TOLERANCE_PT))?;
    let topology = attachment_chains::topology(&supports, MAX_ATTACHMENT_COMPARISONS - comparisons.unwrap());
    if matches!(&topology, Err(attachment_chains::TopologyError::Budget)) {
        return Ok(abstain("endpoint_attachment_budget_exceeded_no_partial_priority"));
    }
    let mut hints = AttachmentHints::default();
    let mut trace_relationships = 0usize;
    let mut total_traces = 0usize;
    let mut chain_bytes = 0usize;
    for support in &supports {
        for text_endpoint_index in [0, support.points.len() - 1] {
            let text_endpoint = support.points[text_endpoint_index];
            let circle_endpoint = support.points[if text_endpoint_index == 0 { support.points.len() - 1 } else { 0 }];
            let matching_features: Vec<_> = view.feature_ids.iter().filter(|id|
                features[id.as_str()].source.pdf_bbox.is_some_and(|bounds|
                    point_box_distance(text_endpoint, bounds) <= ATTACHMENT_TOLERANCE_PT)).collect();
            if !matching_features.iter().any(|id| id.as_str() == feature_id) { continue; }
            let matching_circles: Vec<_> = circles.iter().filter(|(_, center, radius)|
                ((circle_endpoint.x - center.x).hypot(circle_endpoint.y - center.y) - radius).abs()
                    <= ATTACHMENT_TOLERANCE_PT)
                .map(|(id, center, radius)| ((*id).clone(), *center, *radius)).collect();
            total_traces += matching_circles.len();
            trace_relationships += matching_circles.len() * (matching_circles.len() + matching_features.len() + 1);
            if total_traces > MAX_TRACES || trace_relationships > MAX_TRACE_RELATIONSHIPS {
                return Ok(abstain("endpoint_attachment_budget_exceeded_no_partial_priority"));
            }
            for (id, center, radius) in &matching_circles {
                let trace = AttachmentTrace {
                    schema_version: 1, support_id: support.id.clone(),
                    support_fingerprint: primitive_fingerprint(support)?, scope_fingerprint: scope_fingerprint.clone(),
                    source_element_index: support.source.element_index,
                    source_subpath_index: support.source.subpath_index,
                    source_path_id: support.source.path_id.clone(),
                    text_endpoint, circle_endpoint, text_endpoint_index,
                    text_gap_pt: point_box_distance(text_endpoint, features[feature_id].source.pdf_bbox.unwrap()),
                    circle_gap_pt: ((circle_endpoint.x - center.x).hypot(circle_endpoint.y - center.y) - radius).abs(),
                    tolerance_pt: ATTACHMENT_TOLERANCE_PT,
                    matching_feature_ids: matching_features.iter().map(|id| (*id).clone()).collect(),
                    matching_geometry_ids: matching_circles.iter().map(|(id, _, _)| id.clone()).collect(),
                    unique_within_selected_view: false, requires_review: true, semantics: NO_SEMANTICS.into(),
                };
                chain_bytes += serde_json::to_vec(&trace).map_err(|error| format!("Trace d'attachement impossible : {error}"))?.len();
                if chain_bytes > MAX_CHAIN_TRACE_BYTES {
                    return Ok(abstain("endpoint_attachment_budget_exceeded_no_partial_priority"));
                }
                hints.by_geometry.entry(id.clone()).or_default().push(trace);
            }
        }
    }
    let topology = match topology {
        Ok(topology) => topology,
        Err(error) => {
            // Legacy complete single-path hypotheses remain auditable, but a
            // bad component cannot keep a usable prefix or unique priority.
            hints.abstention_reason = Some(match error {
                attachment_chains::TopologyError::ComponentTooLong => "endpoint_attachment_chain_component_exceeds_eight_paths_no_priority",
                _ => "endpoint_attachment_chain_topology_ambiguous_no_priority",
            });
            return Ok(hints);
        },
    };
    let chain_scope = fingerprint(&(&scope_fingerprint, attachment_chains::JOINT_TOLERANCE_PT,
        attachment_chains::MAX_CHAIN_PATHS, "distinct_source_endpoint_chains_v1"))?;
    let mut remaining_work = topology.remaining_work;
    for component in topology.components.iter().filter(|component| component.len() > 1) {
        for reverse in [false, true] {
            let matching_work = view.feature_ids.len() + circles.len();
            if remaining_work < matching_work {
                return Ok(abstain("endpoint_attachment_budget_exceeded_no_partial_priority"));
            }
            remaining_work -= matching_work;
            let order: Vec<_> = if reverse { component.iter().rev().map(|(index, forward)| (*index, !*forward)).collect() }
                else { component.clone() };
            let (directed, joints) = attachment_chains::directed_sources(&supports, &order)?;
            let text_endpoint = directed[0].start_endpoint;
            let circle_endpoint = directed.last().unwrap().end_endpoint;
            let matching_features: Vec<_> = view.feature_ids.iter().filter(|id|
                features[id.as_str()].source.pdf_bbox.is_some_and(|bounds|
                    point_box_distance(text_endpoint, bounds) <= ATTACHMENT_TOLERANCE_PT)).collect();
            if !matching_features.iter().any(|id| id.as_str() == feature_id) { continue; }
            let matching_circles: Vec<_> = circles.iter().filter(|(_, center, radius)|
                ((circle_endpoint.x - center.x).hypot(circle_endpoint.y - center.y) - radius).abs() <= ATTACHMENT_TOLERANCE_PT)
                .map(|(id, center, radius)| ((*id).clone(), *center, *radius)).collect();
            total_traces += matching_circles.len();
            trace_relationships += matching_circles.len() * (matching_circles.len() + matching_features.len() + component.len());
            if total_traces > MAX_TRACES || trace_relationships > MAX_TRACE_RELATIONSHIPS {
                return Ok(abstain("endpoint_attachment_budget_exceeded_no_partial_priority"));
            }
            for (id, center, radius) in &matching_circles {
                let trace = ChainTrace {
                    schema_version: 1, scope_fingerprint: chain_scope.clone(), ordered_supports: directed.clone(), joints: joints.clone(),
                    joint_tolerance_pt: attachment_chains::JOINT_TOLERANCE_PT, tolerance_pt: ATTACHMENT_TOLERANCE_PT,
                    text_endpoint, circle_endpoint,
                    text_gap_pt: point_box_distance(text_endpoint, features[feature_id].source.pdf_bbox.unwrap()),
                    circle_gap_pt: ((circle_endpoint.x - center.x).hypot(circle_endpoint.y - center.y) - radius).abs(),
                    matching_feature_ids: matching_features.iter().map(|id| (*id).clone()).collect(),
                    matching_geometry_ids: matching_circles.iter().map(|(id, _, _)| id.clone()).collect(),
                    unique_within_selected_view: false, requires_review: true, semantics: NO_SEMANTICS.into(),
                };
                chain_bytes += serde_json::to_vec(&trace).map_err(|error| format!("Trace de chaîne impossible : {error}"))?.len();
                if chain_bytes > MAX_CHAIN_TRACE_BYTES {
                    return Ok(abstain("endpoint_attachment_budget_exceeded_no_partial_priority"));
                }
                hints.chain_by_geometry.entry(id.clone()).or_default().push(trace);
            }
        }
    }
    hints.tip_by_geometry = match triangular_tips::hypotheses(catalog, view, features, primitives,
        &supports, &topology, &circles, feature_id, &scope_fingerprint,
        &mut remaining_work, &mut total_traces, &mut trace_relationships, &mut chain_bytes)? {
        Some(traces) => traces,
        None => return Ok(abstain("endpoint_attachment_budget_exceeded_no_partial_priority")),
    };
    // Several oriented supports, text boxes or circles remain ambiguous, even
    // if proximity would otherwise choose one. Never truncate these alternatives.
    let trace_count: usize = hints.by_geometry.values().map(Vec::len).sum::<usize>()
        + hints.chain_by_geometry.values().map(Vec::len).sum::<usize>()
        + hints.tip_by_geometry.values().map(Vec::len).sum::<usize>();
    if trace_count == 1 {
        if let Some((id, traces)) = hints.by_geometry.iter_mut().next() {
            let trace = &mut traces[0];
            if trace.matching_feature_ids.len() == 1 && trace.matching_geometry_ids.len() == 1
                && !topology.multi_path_supports.contains(&trace.support_id) {
                trace.unique_within_selected_view = true;
                hints.preferred_geometry = Some(id.clone());
            }
        }
        if let Some((id, traces)) = hints.chain_by_geometry.iter_mut().next() {
            let trace = &mut traces[0];
            if trace.matching_feature_ids.len() == 1 && trace.matching_geometry_ids.len() == 1 {
                trace.unique_within_selected_view = true;
                hints.preferred_geometry = Some(id.clone());
            }
        }
        if let Some((id, traces)) = hints.tip_by_geometry.iter_mut().next() {
            let trace = &mut traces[0];
            if trace.matching_feature_ids.len() == 1 && trace.matching_geometry_ids.len() == 1
                && trace.matching_triangle_ids.len() == 1 && trace.matching_support_chains.len() == 1 {
                trace.unique_within_selected_view = true;
                hints.preferred_geometry = Some(id.clone());
            }
        }
    }
    if hints.preferred_geometry.is_none() {
        hints.abstention_reason = topology.rejected_component.map(|error| match error {
            attachment_chains::TopologyError::ComponentTooLong => "endpoint_attachment_chain_component_exceeds_eight_paths_no_priority",
            _ => "endpoint_attachment_chain_topology_ambiguous_no_priority",
        });
    }
    Ok(hints)
}

pub(super) fn validate_support_evidence(candidate: &BindingCandidate,
    primitives: &BTreeMap<&str, &VectorPrimitive>) -> Result<(), String> {
    for evidence in &candidate.evidence {
        if let Some(encoded) = evidence.strip_prefix(triangular_tips::TIP_PREFIX) {
            triangular_tips::validate_evidence(encoded, primitives, &candidate.geometry_id, &candidate.feature_id)?;
            continue;
        }
        if let Some(encoded) = evidence.strip_prefix(attachment_chains::CHAIN_PREFIX) {
            attachment_chains::validate_evidence(encoded, primitives)?;
            continue;
        }
        let Some(encoded) = evidence.strip_prefix(TRACE_PREFIX) else { continue; };
        let trace: AttachmentTrace = serde_json::from_str(encoded)
            .map_err(|_| "Trace d'attachement invalide ; reproposez l'association.".to_string())?;
        let support = primitives.get(trace.support_id.as_str())
            .ok_or("Support de l'hypothèse d'attachement absent ; reproposez l'association.")?;
        if trace.schema_version != 1 || !trace.requires_review || trace.semantics != NO_SEMANTICS
            || trace.tolerance_pt != ATTACHMENT_TOLERANCE_PT
            || primitive_fingerprint(support)? != trace.support_fingerprint {
            return Err("La source du support d'attachement a changé ; reproposez l'association et sa revue.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "drawing_leader_hypotheses_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "drawing_attachment_chain_tests.rs"]
mod chain_tests;

#[cfg(test)]
#[path = "drawing_triangular_tip_tests.rs"]
mod tip_tests;

//! Bounded documentary hypotheses from localized PDF text and vector presentation.
use crate::{
    dper_drawing::DrawingControlPlan,
    dper_pdf_geometry::{SvgGeometryDocument, VectorPrimitive, VectorPrimitiveKind},
    dper_pdf_layout::{PdfBox, PdfTextWord, TextOrientation},
};
use serde::Serialize;
use std::collections::BTreeSet;

const MAX_ANCHORS: usize = 16;
const MAX_REGION_ALTERNATIVES: usize = 32;
const MAX_LINE_ROLES: usize = 256;
const MAX_LABEL_GAP_PT: f64 = 48.0;
const EPSILON_PT: f64 = 1e-5;

#[derive(Serialize)]
pub(crate) struct ScopeHypotheses {
    pub(crate) page: u32,
    pub(crate) status: ScopeStatus,
    pub(crate) anchors: Vec<ViewAnchor>,
    pub(crate) line_roles: Vec<LineRoleHypothesis>,
    pub(crate) line_role_limit_reached: bool,
    pub(crate) selected_view_id: Option<String>,
    pub(crate) requires_confirmation: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScopeStatus {
    Unresolved,
    Ambiguous,
    RequiresConfirmation,
}

#[derive(Serialize)]
pub(crate) struct ViewAnchor {
    label: String,
    source_text: String,
    source_line_id: String,
    bbox: PdfBox,
    words: Vec<PdfTextWord>,
    alternatives: Vec<RegionHypothesis>,
}

#[derive(Serialize)]
struct RegionHypothesis {
    geometry_id: String,
    bbox: PdfBox,
    source_element_index: usize,
    source_subpath_index: usize,
}

#[derive(Serialize)]
pub(crate) struct LineRoleHypothesis {
    geometry_id: String,
    bbox: PdfBox,
    source_element_index: usize,
    source_subpath_index: usize,
    stroke_dasharray: Option<String>,
    role_hint: LineRoleHint,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LineRoleHint {
    PossibleAuxiliaryLine,
    Unknown,
}

fn explicit_label(text: &str) -> Option<String> {
    // Limit recognition to standalone labels with an identifier. General
    // prose and text reconstructed from PDF fragments cannot anchor a view.
    if text.len() > 32 {
        return None;
    }
    let mut words = text.split_whitespace();
    let prefix = words.next()?.to_ascii_uppercase();
    let id = words.next()?.to_ascii_uppercase();
    if words.next().is_some() {
        return None;
    }
    let short_id = |value: &str| {
        !value.is_empty()
            && value.len() <= 4
            && value.as_bytes()[0].is_ascii_uppercase()
            && value.as_bytes()[1..].iter().all(u8::is_ascii_digit)
    };
    let valid = match prefix.as_str() {
        "VUE" | "VIEW" => short_id(&id),
        "COUPE" | "SECTION" => id.split_once('-').is_some_and(|(left, right)| {
            left == right && short_id(left)
        }),
        _ => false,
    };
    valid.then(|| format!("{prefix} {id}"))
}

fn rectangular_region(primitive: &VectorPrimitive) -> bool {
    let bounds = primitive.bbox;
    primitive.kind == VectorPrimitiveKind::ClosedPolygon
        && primitive.closed
        && primitive.points.len() == 4
        && bounds.x_max - bounds.x_min > EPSILON_PT
        && bounds.y_max - bounds.y_min > EPSILON_PT
        && primitive.points.iter().zip(primitive.points.iter().cycle().skip(1)).all(|(from, to)| {
            let dx = (from.x - to.x).abs();
            let dy = (from.y - to.y).abs();
            let width = bounds.x_max - bounds.x_min;
            let height = bounds.y_max - bounds.y_min;
            (dx < EPSILON_PT && (dy - height).abs() < EPSILON_PT)
                || (dy < EPSILON_PT && (dx - width).abs() < EPSILON_PT)
        })
}

fn aligned_with_label(region: PdfBox, label: PdfBox) -> bool {
    let gap = (region.y_min - label.y_max).max(label.y_min - region.y_max);
    let max_gap = MAX_LABEL_GAP_PT.min((region.y_max - region.y_min) / 2.0);
    label.x_min >= region.x_min && label.x_max <= region.x_max && (0.0..=max_gap).contains(&gap)
}

fn role_hint(dasharray: Option<&str>) -> LineRoleHint {
    let Some(dasharray) = dasharray else { return LineRoleHint::Unknown; };
    let mut any_positive = false;
    for component in dasharray.split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|component| !component.is_empty())
    {
        let Ok(length) = component.parse::<f64>() else { return LineRoleHint::Unknown; };
        if !length.is_finite() || length < 0.0 {
            return LineRoleHint::Unknown;
        }
        any_positive |= length > 0.0;
    }
    if any_positive { LineRoleHint::PossibleAuxiliaryLine } else { LineRoleHint::Unknown }
}

fn overlaps(left: PdfBox, right: PdfBox) -> bool {
    left.x_min < right.x_max && right.x_min < left.x_max
        && left.y_min < right.y_max && right.y_min < left.y_max
}

pub(crate) fn inspect(
    plan: &DrawingControlPlan,
    catalog: &SvgGeometryDocument,
) -> Result<ScopeHypotheses, String> {
    let mut pages = plan.page_geometry.iter().filter(|page| page.page == catalog.page);
    let page = pages.next().ok_or("Repère textuel de cette page PDF absent.")?;
    if pages.next().is_some()
        || !(page.width_pt.is_finite() && page.height_pt.is_finite())
        || !(catalog.width_pt.is_finite() && catalog.height_pt.is_finite())
        || (page.width_pt - catalog.width_pt).abs() > EPSILON_PT
        || (page.height_pt - catalog.height_pt).abs() > EPSILON_PT
    {
        return Err("Le texte et les tracés doivent partager un même repère de page PDF.".into());
    }

    let mut anchors = Vec::new();
    let mut labels = BTreeSet::new();
    let mut ambiguous = false;
    for group in plan.spatial_text_groups.iter().filter(|group| {
        group.page == catalog.page && group.orientation == TextOrientation::Horizontal
    }) {
        let Some(label) = explicit_label(&group.source_text) else { continue; };
        if anchors.len() == MAX_ANCHORS {
            return Err("Trop de libellés de vue sur cette page pour proposer un périmètre.".into());
        }
        let alternatives = catalog.primitives.iter().filter(|primitive| {
            primitive.page == catalog.page
                && aligned_with_label(primitive.bbox, group.bbox)
                && rectangular_region(primitive)
        }).take(MAX_REGION_ALTERNATIVES + 1).map(|primitive| RegionHypothesis {
            geometry_id: primitive.id.clone(),
            bbox: primitive.bbox,
            source_element_index: primitive.source.element_index,
            source_subpath_index: primitive.source.subpath_index,
        }).collect::<Vec<_>>();
        if alternatives.len() > MAX_REGION_ALTERNATIVES {
            return Err("Trop de périmètres compatibles avec un libellé de vue sur cette page.".into());
        }
        ambiguous |= !labels.insert(label.clone()) || alternatives.len() > 1;
        anchors.push(ViewAnchor {
            label,
            source_text: group.source_text.clone(),
            source_line_id: group.source_line_id.clone(),
            bbox: group.bbox,
            words: group.words.clone(),
            alternatives,
        });
    }
    let regions = anchors.iter().filter_map(|anchor| anchor.alternatives.first()).collect::<Vec<_>>();
    ambiguous |= regions.iter().enumerate().any(|(index, region)| {
        regions[index + 1..].iter().any(|other| overlaps(region.bbox, other.bbox))
    });
    let status = if ambiguous {
        ScopeStatus::Ambiguous
    } else if anchors.is_empty() || anchors.iter().any(|anchor| anchor.alternatives.is_empty()) {
        ScopeStatus::Unresolved
    } else {
        ScopeStatus::RequiresConfirmation
    };

    let mut lines = catalog.primitives.iter().filter(|primitive| {
        primitive.page == catalog.page && primitive.kind == VectorPrimitiveKind::Line && !primitive.closed
    }).take(MAX_LINE_ROLES + 1).collect::<Vec<_>>();
    let line_role_limit_reached = lines.len() > MAX_LINE_ROLES;
    if line_role_limit_reached {
        lines.clear();
    }
    let line_roles = lines.into_iter().map(|primitive| {
        let dasharray = primitive.style.stroke_dasharray.as_deref();
        LineRoleHypothesis {
            geometry_id: primitive.id.clone(),
            bbox: primitive.bbox,
            source_element_index: primitive.source.element_index,
            source_subpath_index: primitive.source.subpath_index,
            stroke_dasharray: primitive.style.stroke_dasharray.clone(),
            role_hint: role_hint(dasharray),
        }
    }).collect();
    Ok(ScopeHypotheses {
        page: catalog.page,
        status,
        anchors,
        line_roles,
        line_role_limit_reached,
        selected_view_id: None,
        requires_confirmation: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dper_drawing::parse_text_plan,
        dper_pdf_geometry::parse_svg_geometry,
        dper_pdf_layout::parse_bbox_layout,
    };

    fn fixture(extra_svg: &str, second_label: &str) -> (DrawingControlPlan, SvgGeometryDocument) {
        let layout = parse_bbox_layout(&format!(r#"<html><doc><page width="400" height="300">
            <line xMin="50" yMin="120" xMax="80" yMax="130">
                <word xMin="50" yMin="120" xMax="70" yMax="130">VUE</word>
                <word xMin="74" yMin="120" xMax="80" yMax="130">A</word>
            </line>
            <line xMin="250" yMin="120" xMax="280" yMax="130">
                <word xMin="250" yMin="120" xMax="270" yMax="130">VUE</word>
                <word xMin="274" yMin="120" xMax="280" yMax="130">{second_label}</word>
            </line>
            <line xMin="150" yMin="240" xMax="250" yMax="250">
                <word xMin="150" yMin="240" xMax="250" yMax="250">Vues symétriques</word>
            </line>
            </page></doc></html>"#)).unwrap();
        let mut plan = parse_text_plan("Ø12 mm", "synthetic.pdf");
        plan.page_geometry = layout.pages;
        plan.spatial_text_groups = layout.groups;
        let catalog = parse_svg_geometry(&format!(r#"<svg width="400pt" height="300pt" viewBox="0 0 400 300">
            <rect x="25" y="20" width="100" height="90" stroke="black" fill="none" />
            <rect x="225" y="20" width="100" height="90" stroke="black" fill="none" />
            <line x1="30" y1="55" x2="115" y2="55" stroke="black" stroke-dasharray="4 3" />
            <line x1="230" y1="55" x2="315" y2="55" stroke="black" />
            {extra_svg}
            </svg>"#), 1).unwrap();
        (plan, catalog)
    }

    #[test]
    fn localized_labels_preserve_distinct_hypotheses_without_selecting_a_view_or_role() {
        let (plan, catalog) = fixture("", "B");
        let result = inspect(&plan, &catalog).unwrap();
        assert_eq!(result.status, ScopeStatus::RequiresConfirmation);
        assert!(result.requires_confirmation && result.selected_view_id.is_none());
        assert_eq!(result.anchors.len(), 2);
        assert_eq!(result.anchors[0].source_text, "VUE A");
        assert_eq!(result.anchors[0].words[1].text, "A");
        assert_eq!(result.anchors[0].words[1].bbox.x_min, 74.0);
        assert_eq!(result.anchors[0].alternatives.len(), 1);
        assert_eq!(result.anchors[1].alternatives.len(), 1);
        assert_ne!(result.anchors[0].alternatives[0].geometry_id, result.anchors[1].alternatives[0].geometry_id);
        assert_eq!(result.line_roles[0].role_hint, LineRoleHint::PossibleAuxiliaryLine);
        assert_eq!(result.line_roles[1].role_hint, LineRoleHint::Unknown);
    }

    #[test]
    fn competing_regions_and_repeated_labels_are_ambiguous() {
        let (plan, catalog) = fixture(r#"<rect x="30" y="30" width="90" height="75" stroke="black" fill="none" />"#, "B");
        let result = inspect(&plan, &catalog).unwrap();
        assert_eq!(result.status, ScopeStatus::Ambiguous);
        assert_eq!(result.anchors[0].alternatives.len(), 2);
        assert!(result.selected_view_id.is_none());

        let (plan, catalog) = fixture("", "A");
        let result = inspect(&plan, &catalog).unwrap();
        assert_eq!(result.status, ScopeStatus::Ambiguous);
        assert!(result.selected_view_id.is_none());
    }

    #[test]
    fn unresolved_anchor_and_role_budget_preserve_abstention() {
        let line = r#"<line x1="30" y1="60" x2="115" y2="60" stroke="black" />"#;
        let (mut plan, catalog) = fixture(&line.repeat(MAX_LINE_ROLES), "B");
        plan.spatial_text_groups[1].bbox.x_max = 360.0;
        let result = inspect(&plan, &catalog).unwrap();
        assert_eq!(result.status, ScopeStatus::Unresolved);
        assert!(result.anchors[1].alternatives.is_empty());
        assert!(result.line_role_limit_reached && result.line_roles.is_empty());
        assert!(result.selected_view_id.is_none());
        assert_eq!(role_hint(Some("0, 0")), LineRoleHint::Unknown);
        assert_eq!(role_hint(Some("none")), LineRoleHint::Unknown);
    }
}

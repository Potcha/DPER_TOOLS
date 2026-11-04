use super::*;
use crate::{dper_drawing::parse_text_plan, dper_pdf_geometry::parse_svg_geometry,
    dper_pdf_layout::PdfPageGeometry};

const SUPPORT: &str = r#"<path id="support" d="M198 35 L150 35 L150 150 L100 150"/>"#;

fn fixture(support: &str, extra: &str) -> (DrawingControlPlan, SvgGeometryDocument, DrawingView) {
    transformed_fixture(support, extra, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
}

fn transformed_fixture(support: &str, extra: &str, matrix: [f64; 6])
    -> (DrawingControlPlan, SvgGeometryDocument, DrawingView) {
    let mut plan = parse_text_plan("Ø999 mm", "synthetic-endpoint-hypotheses.pdf");
    plan.page_geometry = vec![PdfPageGeometry { page: 1, width_pt: 400.0, height_pt: 320.0 }];
    let [a, b, c, d, e, f] = matrix;
    let corners = [(200.0, 30.0), (230.0, 30.0), (230.0, 40.0), (200.0, 40.0)]
        .map(|(x, y)| Point { x: a * x + c * y + e, y: b * x + d * y + f });
    let feature = &mut plan.characteristics[0];
    feature.source.page = Some(1);
    feature.source.word_ids = vec!["synthetic-word-1".into()];
    feature.source.pdf_bbox = Some(PdfBox {
        x_min: corners.iter().map(|point| point.x).fold(f64::INFINITY, f64::min),
        y_min: corners.iter().map(|point| point.y).fold(f64::INFINITY, f64::min),
        x_max: corners.iter().map(|point| point.x).fold(f64::NEG_INFINITY, f64::max),
        y_max: corners.iter().map(|point| point.y).fold(f64::NEG_INFINITY, f64::max),
    });
    let svg = format!(r#"<svg width="400pt" height="320pt" viewBox="0 0 400 320"><g transform="matrix({a} {b} {c} {d} {e} {f})" fill="none" stroke="black"><circle cx="80" cy="150" r="20"/><circle cx="200" cy="100" r="15"/>{support}{extra}</g></svg>"#);
    let catalog = parse_svg_geometry(&svg, 1).unwrap();
    let view = all_view(&plan, &catalog);
    (plan, catalog, view)
}

fn all_view(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument) -> DrawingView {
    define_view(plan, "synthetic-document-current", catalog, "Vue explicite synthétique",
        &plan.characteristics.iter().map(|feature| feature.id.clone()).collect::<Vec<_>>(),
        &catalog.primitives.iter().map(|primitive| primitive.id.clone()).collect::<Vec<_>>()).unwrap()
}

fn candidates(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument, view: &DrawingView) -> Vec<BindingCandidate> {
    binding_candidates(plan, "synthetic-document-current", catalog, view, &plan.characteristics[0].id).unwrap()
}

fn traces(candidate: &BindingCandidate) -> Vec<AttachmentTrace> {
    candidate.evidence.iter().filter_map(|line| line.strip_prefix(TRACE_PREFIX))
        .map(|encoded| serde_json::from_str(encoded).unwrap()).collect()
}

fn support_index(catalog: &SvgGeometryDocument) -> usize {
    catalog.primitives.iter().position(|primitive| primitive.source.path_id.as_deref() == Some("support")).unwrap()
}

#[test]
fn unique_endpoint_hypothesis_precedes_nearer_distractor_without_selecting_or_reviewing() {
    let (plan, catalog, view) = fixture(SUPPORT, "");
    let candidates = candidates(&plan, &catalog, &view);
    assert_eq!(candidates.len(), 2, "Every compatible circle remains available");
    assert_eq!(candidates[0].geometry_id, catalog.primitives[0].id);
    assert!(candidates[0].distance_pt > candidates[1].distance_pt, "Priority is independent of nearest text-circle proximity");
    assert_eq!(plan.characteristics[0].nominal, Some(999.0), "Nominal never filters shape or establishes scale");
    let trace = traces(&candidates[0]).remove(0);
    assert_eq!(trace.support_id, catalog.primitives[support_index(&catalog)].id);
    assert_eq!(trace.schema_version, 1);
    assert_eq!(trace.tolerance_pt, 2.0);
    assert!((trace.text_gap_pt - 2.0).abs() < 1e-6);
    assert!(trace.circle_gap_pt < 1e-6);
    assert!(trace.unique_within_selected_view && trace.requires_review);
    assert_eq!(trace.matching_feature_ids, vec![plan.characteristics[0].id.clone()]);
    assert_eq!(trace.matching_geometry_ids, vec![catalog.primitives[0].id.clone()]);
    assert_eq!(trace.semantics, NO_SEMANTICS);
    assert!(!trace.support_fingerprint.is_empty() && !trace.scope_fingerprint.is_empty());
    assert!(traces(&candidates[1]).is_empty());
    assert!(candidates[0].suspension_reasons.contains(&"binding_requires_explicit_review".into()));
    let selected = select_binding(&candidates, &candidates[0].id, &catalog,
        "Hypothèse de deux extrémités à revoir sur le document, sans reconnaissance de symbole.").unwrap();
    assert!(!selected.reviewed);
    assert!(resolve_binding(&plan, "synthetic-document-current", &catalog, &view, &selected).is_ok());
    assert!(!selected.reviewed, "Resolution never grants review");
}

#[test]
fn endpoint_hypotheses_are_rotation_translation_and_direction_independent() {
    for matrix in [[1.0, 0.0, 0.0, 1.0, 15.0, 20.0], [0.0, 1.0, -1.0, 0.0, 340.0, -20.0]] {
        for source in [SUPPORT, r#"<path id="support" d="M100 150 L150 150 L150 35 L198 35"/>"#] {
            let (plan, catalog, view) = transformed_fixture(source, "", matrix);
            let result = candidates(&plan, &catalog, &view);
            assert_eq!(result[0].geometry_id, catalog.primitives[0].id);
            let trace = traces(&result[0]).remove(0);
            assert!(trace.unique_within_selected_view);
            assert!((trace.text_gap_pt - 2.0).abs() < 1e-6);
            assert!(trace.circle_gap_pt < 1e-6);
            assert_eq!(trace.text_endpoint_index, if source == SUPPORT { 0 } else { 3 });
        }
    }
    let (plan, catalog, view) = fixture(r#"<path id="support" d="M1.98e2 35 H150 V150 H1e2"/>"#, "");
    assert!(traces(&candidates(&plan, &catalog, &view)[0])[0].unique_within_selected_view,
        "Scientific-notation e/E is numeric, not a curve command");
}

#[test]
fn concentric_tangent_and_conflicting_support_ambiguities_keep_all_circles_without_priority() {
    for extra in [r#"<circle cx="80" cy="150" r="19"/>"#,
        r#"<circle cx="120" cy="150" r="20"/>"#,
        r#"<path id="conflict" d="M198 35 L240 35 L240 85 L200 85"/>"#] {
        let (plan, catalog, view) = fixture(SUPPORT, extra);
        let result = candidates(&plan, &catalog, &view);
        assert_eq!(result.len(), if extra.contains("circle") { 3 } else { 2 });
        assert!(result.windows(2).all(|pair| pair[0].distance_pt <= pair[1].distance_pt),
            "Ambiguous endpoints must retain the original proximity ordering");
        let hypotheses: Vec<_> = result.iter().flat_map(traces).collect();
        assert!(hypotheses.len() >= 2);
        assert!(hypotheses.iter().all(|trace| !trace.unique_within_selected_view));
        assert!(result.iter().filter(|candidate| !traces(candidate).is_empty()).all(|candidate|
            candidate.suspension_reasons.contains(&"endpoint_attachment_ambiguity_is_retained".into())));
    }
}

#[test]
fn multiple_scoped_text_boxes_do_not_grant_unique_attachment_priority() {
    let (mut plan, catalog, _) = fixture(SUPPORT, "");
    let mut second = plan.characteristics[0].clone();
    second.id = "synthetic-other-text".into();
    plan.characteristics.push(second);
    let view = all_view(&plan, &catalog);
    let result = candidates(&plan, &catalog, &view);
    assert!(result[0].distance_pt <= result[1].distance_pt);
    let hypotheses: Vec<_> = result.iter().flat_map(traces).collect();
    assert_eq!(hypotheses.len(), 1);
    assert_eq!(hypotheses[0].matching_feature_ids.len(), 2);
    assert!(!hypotheses[0].unique_within_selected_view);
}

#[test]
fn unrelated_out_of_scope_and_disconnected_supports_leave_proximity_behavior() {
    for source in [r#"<path id="support" d="M198 35 L150 35 L150 150 L110 150"/>"#,
        r#"<path id="support" d="M190 35 L150 35 L150 150 L100 150"/>"#,
        r#"<path id="support" d="M10 20 L30 20"/>"#] {
        let (plan, catalog, view) = fixture(source, "");
        let result = candidates(&plan, &catalog, &view);
        assert!(result.iter().all(|candidate| traces(candidate).is_empty()));
        assert_eq!(result[0].geometry_id, catalog.primitives[1].id);
    }
    let (plan, catalog, _) = fixture(SUPPORT, "");
    let view = define_view(&plan, "synthetic-document-current", &catalog, "Cercles seuls",
        &[plan.characteristics[0].id.clone()],
        &[catalog.primitives[0].id.clone(), catalog.primitives[1].id.clone()]).unwrap();
    let result = candidates(&plan, &catalog, &view);
    assert!(result.iter().all(|candidate| traces(candidate).is_empty()));
    assert_eq!(result[0].geometry_id, catalog.primitives[1].id, "Unselected lines do not join the scope");
    let (other_plan, _, _) = fixture("", "");
    let clean = parse_svg_geometry(r#"<svg width="400pt" height="320pt"><g fill="none" stroke="black"><circle cx="80" cy="150" r="20"/><circle cx="200" cy="100" r="15"/></g></svg>"#, 1).unwrap();
    let clean_view = all_view(&other_plan, &clean);
    let clean_candidates = candidates(&other_plan, &clean, &clean_view);
    assert_eq!(result.iter().map(|candidate| &candidate.evidence).collect::<Vec<_>>(),
        clean_candidates.iter().map(|candidate| &candidate.evidence).collect::<Vec<_>>(),
        "No hints retain the existing evidence contract");
}

#[test]
fn dashed_filled_closed_curved_compound_degenerate_clipped_and_marker_paths_never_supply_hints() {
    for source in [
        r#"<path id="support" stroke-dasharray="4 2" d="M198 35 L150 35 L150 150 L100 150"/>"#,
        r#"<path id="support" fill="black" d="M198 35 L150 35 L150 150 L100 150"/>"#,
        r#"<path id="support" d="M198 35 L150 35 L150 150 L100 150 Z"/>"#,
        r#"<path id="support" d="M198 35 Q150 35 150 150 L100 150"/>"#,
        r#"<path id="support" d="M198 35 L150 35 M150 150 L100 150"/>"#,
        r#"<path id="support" d="M198 35 L198 35"/>"#,
        r#"<path id="support" d="M198 35 L150 35 L175 35 L100 150"/>"#,
        r#"<path id="support" marker-end="url(#arrow)" d="M198 35 L150 35 L150 150 L100 150"/>"#,
        r#"<path id="support" opacity="0.5" d="M198 35 L150 35 L150 150 L100 150"/>"#,
        r#"<path id="support" clip-path="url(#unresolved)" d="M198 35 L150 35 L150 150 L100 150"/>"#,
    ] {
        let (plan, catalog, view) = fixture(source, "");
        let result = candidates(&plan, &catalog, &view);
        assert!(result.iter().all(|candidate| traces(candidate).is_empty()), "Source must abstain: {source}");
        assert_eq!(result[0].geometry_id, catalog.primitives[1].id);
    }
}

#[test]
fn support_changes_and_context_changes_invalidate_proposal_without_granting_review() {
    let (plan, catalog, view) = fixture(SUPPORT, "");
    let original = candidates(&plan, &catalog, &view);
    let selected = select_binding(&original, &original[0].id, &catalog, "Hypothèse locale à revoir.").unwrap();
    assert!(!selected.reviewed);
    for change in 0..5 {
        let mut changed = catalog.clone();
        let index = support_index(&changed);
        match change {
            0 => changed.primitives[index].source.path_data.push(' '),
            1 => changed.primitives[index].style.stroke_dasharray = Some("1 1".into()),
            2 => changed.primitives[index].points[1].x += 0.25,
            3 => changed.primitives[index].source.transform[4] += 1.0,
            _ => changed.primitives[1].source.element_attributes.push_str(" data-source-revised='yes'"),
        }
        assert!(resolve_binding(&plan, "synthetic-document-current", &changed, &view, &selected).is_err(), "change={change}");
        if change != 4 {
            assert!(select_binding(&original, &original[0].id, &changed, "Ancienne hypothèse.").is_err());
        }
        assert!(!selected.reviewed);
    }
    let mut imported = serde_json::to_value(&selected).unwrap();
    imported["reviewed"] = true.into();
    let imported: BindingSelection = serde_json::from_value(imported).unwrap();
    assert!(!imported.reviewed);
}

#[test]
fn excessive_supports_and_extraction_uncertainty_abstain_as_a_whole() {
    let extra = (0..MAX_SUPPORTS).map(|index| format!(r#"<path id="unrelated-{index}" d="M10 20 L30 20"/>"#)).collect::<String>();
    let (plan, catalog, view) = fixture(SUPPORT, &extra);
    let result = candidates(&plan, &catalog, &view);
    assert!(result.iter().all(|candidate| traces(candidate).is_empty()));
    assert!(result.iter().all(|candidate| candidate.suspension_reasons.contains(
        &"endpoint_attachment_budget_exceeded_no_partial_priority".into())));
    let (plan, mut catalog, view) = fixture(SUPPORT, "");
    catalog.warnings.push("Classe CSS SVG présente : styles externes non interprétés.".into());
    assert!(binding_candidates(&plan, "synthetic-document-current", &catalog, &view, &plan.characteristics[0].id).is_err());
    let result = candidates(&plan, &catalog, &all_view(&plan, &catalog));
    assert!(result.iter().all(|candidate| traces(candidate).is_empty()));
    assert!(result.iter().all(|candidate| candidate.suspension_reasons.contains(
        &"endpoint_attachment_abstained_due_to_extraction_limitations".into())));
}

#[test]
fn unresolved_ancestor_styles_and_group_compositing_abstain_without_losing_circle_alternatives() {
    for attributes in ["class='external-css'", "filter='url(#unresolved)'", "marker-end='url(#arrow)'",
        "style='filter:url(#unresolved)'", "style='marker-start:url(#arrow)'", "opacity='0.5'",
        "style='OPACITY:0.5'", "style='STROKE:none'", "style='stroke:none !important'",
        "style='CLIP-PATH:url(#unresolved)'", "style='MASK:url(#unresolved)'"] {
        let support = format!("<g {attributes}><g opacity='1'>{SUPPORT}</g></g>");
        let (plan, catalog, view) = fixture(&support, "");
        let result = candidates(&plan, &catalog, &view);
        assert_eq!(result.len(), 2, "Retain alternatives under unresolved presentation");
        assert!(result.iter().all(|candidate| traces(candidate).is_empty()), "{attributes}");
        assert_eq!(result[0].geometry_id, catalog.primitives[1].id);
        assert!(result.iter().all(|candidate| candidate.suspension_reasons.contains(
            &"endpoint_attachment_abstained_due_to_extraction_limitations".into())), "{attributes}");
    }
}

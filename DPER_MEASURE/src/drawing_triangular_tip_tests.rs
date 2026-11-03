use super::*;
use triangular_tips::{TipTrace, TIP_PREFIX};
use crate::{dper_drawing::parse_text_plan, dper_pdf_geometry::parse_svg_geometry,
    dper_pdf_layout::PdfPageGeometry};

const SUPPORT: &str = r#"<path id="support" d="M198 35 L150 35 L150 150 L106 150"/>"#;
const CHAIN: &str = r#"<path id="first" d="M198 35 L150 35"/><path id="middle" d="M150 35 L150 150"/><path id="last" d="M150 150 L106 150"/>"#;
const TRIANGLE: &str = r#"<path id="tip" fill="black" stroke="none" d="M100 150 L106 148.5 L106 151.5 Z"/>"#;
const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn fixture(support: &str, triangle: &str, extra: &str, matrix: [f64; 6])
    -> (DrawingControlPlan, SvgGeometryDocument, DrawingView) {
    let mut plan = parse_text_plan("Ø999 mm", "triangular.pdf");
    plan.page_geometry = vec![PdfPageGeometry { page: 1, width_pt: 400.0, height_pt: 320.0 }];
    let [a, b, c, d, e, f] = matrix;
    let corners = [(200.0, 30.0), (230.0, 30.0), (230.0, 40.0), (200.0, 40.0)]
        .map(|(x, y)| Point { x: a*x + c*y + e, y: b*x + d*y + f });
    plan.characteristics[0].source.page = Some(1);
    plan.characteristics[0].source.word_ids = vec!["tip-word".into()];
    plan.characteristics[0].source.pdf_bbox = Some(PdfBox {
        x_min: corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        y_min: corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        x_max: corners.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
        y_max: corners.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
    });
    let catalog = parse_svg_geometry(&format!(r#"<svg width="400pt" height="320pt" viewBox="0 0 400 320"><g fill="none" stroke="black" transform="matrix({a} {b} {c} {d} {e} {f})"><circle cx="80" cy="150" r="20"/><circle cx="200" cy="100" r="15"/>{support}{triangle}{extra}</g></svg>"#), 1).unwrap();
    assert_eq!(catalog.unsupported_path_count, 0);
    assert!(catalog.warnings.iter().all(|warning| warning.starts_with("Géométries nominales candidates en points PDF")), "{:?}", catalog.warnings);
    let view = all_view(&plan, &catalog);
    (plan, catalog, view)
}

fn all_view(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument) -> DrawingView {
    define_view(plan, "tip-document", catalog, "Pointe explicite",
        &plan.characteristics.iter().map(|f| f.id.clone()).collect::<Vec<_>>(),
        &catalog.primitives.iter().map(|p| p.id.clone()).collect::<Vec<_>>()).unwrap()
}

fn candidates(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument, view: &DrawingView) -> Vec<BindingCandidate> {
    binding_candidates(plan, "tip-document", catalog, view, &plan.characteristics[0].id).unwrap()
}

fn traces(candidate: &BindingCandidate) -> Vec<TipTrace> {
    candidate.evidence.iter().filter_map(|line| line.strip_prefix(TIP_PREFIX))
        .map(|encoded| serde_json::from_str(encoded).unwrap()).collect()
}

#[test]
fn explicit_filled_tip_ranks_single_and_chain_hypotheses_without_authority_or_snapping() {
    for support in [SUPPORT, CHAIN, r#"<path d="M106 150 L150 150 L150 35 L198 35"/>"#] {
        for matrix in [IDENTITY, [0.0, 1.0, -1.0, 0.0, 340.0, -20.0], [-1.0, 0.0, 0.0, 1.0, 350.0, 0.0]] {
            let (plan, catalog, view) = fixture(support, TRIANGLE, "", matrix);
            let result = candidates(&plan, &catalog, &view);
            assert_eq!(result.len(), 2);
            assert_eq!(result[0].geometry_id, catalog.primitives[0].id);
            assert!(result[0].distance_pt > result[1].distance_pt);
            let trace = traces(&result[0]).remove(0);
            assert!(trace.unique_within_selected_view && trace.requires_review);
            assert_eq!(trace.triangle.height_pt, 6.0);
            assert_eq!(trace.triangle.base_width_pt, 3.0);
            assert_eq!(trace.base_gap_pt, 0.0);
            assert_eq!(trace.direction_cosine, 1.0);
            // Test-only PDF-point rounding allowance for the transformed circle fit; not a physical or production tolerance.
            assert!(trace.circle_gap_pt.abs() <= 1e-10);
            assert_eq!(trace.text_gap_pt, 2.0);
            assert_eq!(trace.ordered_supports.len(), if support == CHAIN { 3 } else { 1 });
            assert_eq!(plan.characteristics[0].nominal, Some(999.0));
            let primitive = catalog.primitives.iter().find(|p| p.id == trace.triangle.triangle_id).unwrap();
            assert_eq!(trace.triangle.vertices.as_slice(), primitive.points.as_slice());
            assert_eq!(trace.triangle.triangle_fingerprint, primitive_fingerprint(primitive).unwrap());
            let selected = select_binding(&result, &result[0].id, &catalog, "Géométrie triangulaire explicite à revoir.").unwrap();
            assert!(!selected.reviewed);
            assert!(resolve_binding(&plan, "tip-document", &catalog, &view, &selected).is_ok());
            assert!(!selected.reviewed);
        }
    }
}

#[test]
fn vertex_order_polygon_and_exact_poppler_inert_terminal_move_are_supported() {
    for triangle in [
        r#"<path fill="black" stroke="none" d="M106 151.5 L106 148.5 L100 150 Z"/>"#,
        r##"<polygon fill="#000" stroke="none" points="100,150 106,148.5 106,151.5"/>"##,
        r#"<path fill="rgb(0%,0%,0%)" stroke="none" d="M100 150 L106 148.5 L106 151.5 Z M100 150"/>"#,
    ] {
        let (plan, catalog, view) = fixture(SUPPORT, triangle, "", IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        assert!(traces(&result[0])[0].unique_within_selected_view, "{triangle}");
    }
}

#[test]
fn actual_poppler_triangle_source_survives_source_reparse() {
    // Exact path observed in the real synthetic PDF/MCP probe, including
    // Poppler's inactive final absolute moveto.
    let data = "M190 160 L196 158.5 L196 161.5 Z M190 160 ";
    let catalog = parse_svg_geometry(&format!(
        r#"<svg width="400pt" height="320pt" viewBox="0 0 400 320"><path fill="black" stroke="none" d="{data}"/></svg>"#), 1).unwrap();
    assert_eq!(catalog.primitives.len(), 1);
    let triangle = triangular_tips::triangle_shape(&catalog.primitives[0])
        .expect("An extracted real Poppler triangle must survive provenance reparse");
    assert_eq!(triangle.source.path_data, data);
    assert_eq!(triangle.apex, Point { x: 190.0, y: 160.0 });
    assert_eq!(triangle.height_pt, 6.0);
    assert_eq!(triangle.base_width_pt, 3.0);
}

#[test]
fn hollow_translucent_curved_compound_squat_and_wrong_direction_triangles_abstain() {
    for triangle in [
        TRIANGLE.replace("fill=\"black\"", "fill=\"none\"").replace("stroke=\"none\"", "stroke=\"black\""),
        TRIANGLE.replace("fill=\"black\"", "fill=\"black\" fill-opacity=\"0.5\""),
        TRIANGLE.replace("fill=\"black\"", "fill=\"rgba(0,0,0,0.5)\""),
        TRIANGLE.replace("L106 148.5", "Q103 149.25 106 148.5"),
        TRIANGLE.replace(" Z", " Z M20 20 L21 20 L21 21 Z"),
        TRIANGLE.replace(" Z", " Z M100.1 150"),
        TRIANGLE.replace(" Z", " Z m100 150"),
        TRIANGLE.replace("100 150", "104 150"),
        TRIANGLE.replace("100 150", "112 150"),
        TRIANGLE.replace("148.5", "145").replace("151.5", "155"),
    ] {
        let (plan, catalog, view) = fixture(SUPPORT, &triangle, "", IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        assert!(result.iter().all(|candidate| traces(candidate).is_empty()), "{triangle}");
        assert!(result[0].distance_pt <= result[1].distance_pt);
    }
}

#[test]
fn base_distance_and_direction_are_bounded_without_extending_the_support() {
    for (end, accepted) in [("106.4 150", true), ("106.6 150", false), ("103 150", false)] {
        let source = SUPPORT.replace("106 150", end);
        let (plan, catalog, view) = fixture(&source, TRIANGLE, "", IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        let tips: Vec<_> = result.iter().flat_map(traces).collect();
        assert_eq!(!tips.is_empty(), accepted, "{end}");
        if accepted {
            assert_eq!(tips[0].support_endpoint, Point { x: 106.4, y: 150.0 });
            assert_eq!(tips[0].triangle.base_midpoint, Point { x: 106.0, y: 150.0 });
            assert!((tips[0].base_gap_pt - 0.4).abs() < 1e-9);
        }
    }
    let support = r#"<path d="M198 35 L106 35 L106 150"/>"#;
    let (plan, catalog, view) = fixture(support, TRIANGLE, "", IDENTITY);
    assert!(candidates(&plan, &catalog, &view).iter().all(|candidate| traces(candidate).is_empty()));
}

#[test]
fn triangle_circle_text_and_support_ambiguities_keep_all_candidates_without_priority() {
    for extra in [
        r#"<path fill="black" stroke="none" d="M100 150 L106 148.5 L106 151.5 Z"/>"#,
        r#"<circle cx="80" cy="150" r="19"/>"#,
        r#"<path d="M250 200 L120 200 L120 150.4 L106 150.4"/>"#,
    ] {
        let (plan, catalog, view) = fixture(SUPPORT, TRIANGLE, extra, IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        assert!(result.iter().flat_map(traces).all(|trace| !trace.unique_within_selected_view));
        assert!(result.windows(2).all(|pair| pair[0].distance_pt <= pair[1].distance_pt));
        assert_eq!(result.len(), if extra.contains("circle") { 3 } else { 2 });
    }
    let (mut plan, catalog, _) = fixture(SUPPORT, TRIANGLE, "", IDENTITY);
    let mut second = plan.characteristics[0].clone();
    second.id = "second-text".into();
    plan.characteristics.push(second);
    let result = candidates(&plan, &catalog, &all_view(&plan, &catalog));
    assert_eq!(result.iter().flat_map(traces).next().unwrap().matching_feature_ids.len(), 2);
    assert!(result.iter().flat_map(traces).all(|trace| !trace.unique_within_selected_view));
}

#[test]
fn rejected_chain_components_cannot_regain_priority_through_a_tip() {
    for extra in [r#"<path d="M150 35 L170 20"/>"#,
        r#"<path d="M130 80 L170 80"/>"#,
        r#"<path d="M106 150 L106 250 L198 250 L198 35"/>"#] {
        let (plan, catalog, view) = fixture(CHAIN, TRIANGLE, extra, IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        assert!(result.iter().all(|candidate| traces(candidate).is_empty()));
        assert!(result[0].distance_pt <= result[1].distance_pt);
    }
}

#[test]
fn changed_triangle_or_support_sources_styles_scope_and_trace_invalidate_selection() {
    let (plan, catalog, view) = fixture(CHAIN, TRIANGLE, "", IDENTITY);
    let result = candidates(&plan, &catalog, &view);
    let selected = select_binding(&result, &result[0].id, &catalog, "Hypothèse de pointe à revoir.").unwrap();
    let tip = catalog.primitives.iter().position(|p| p.source.path_id.as_deref() == Some("tip")).unwrap();
    let middle = catalog.primitives.iter().position(|p| p.source.path_id.as_deref() == Some("middle")).unwrap();
    for change in 0..6 {
        let mut changed = catalog.clone();
        match change {
            0 => changed.primitives[tip].source.path_data.push(' '),
            1 => changed.primitives[tip].style.fill = Some("red".into()),
            2 => changed.primitives[tip].source.transform[4] += 0.1,
            3 => changed.primitives[tip].points[0].x += 0.1,
            4 => changed.primitives[middle].source.path_data.push(' '),
            _ => { changed.primitives.remove(tip); },
        }
        assert!(select_binding(&result, &result[0].id, &changed, "Ancienne pointe.").is_err());
        assert!(resolve_binding(&plan, "tip-document", &changed, &all_view(&plan, &changed), &selected).is_err());
    }
    let ids: Vec<_> = view.geometry_ids.iter().filter(|id| id.as_str() != catalog.primitives[tip].id).cloned().collect();
    let scoped = define_view(&plan, "tip-document", &catalog, &view.label, &view.feature_ids, &ids).unwrap();
    assert!(resolve_binding(&plan, "tip-document", &catalog, &scoped, &selected).is_err());
    let mut altered = result.clone();
    let evidence = altered[0].evidence.iter_mut().find(|line| line.starts_with(TIP_PREFIX)).unwrap();
    let mut trace: TipTrace = serde_json::from_str(evidence.strip_prefix(TIP_PREFIX).unwrap()).unwrap();
    trace.triangle.apex.x += 0.1;
    *evidence = format!("{TIP_PREFIX}{}", serde_json::to_string(&trace).unwrap());
    assert!(select_binding(&altered, &altered[0].id, &catalog, "Trace modifiée.").is_err());
    assert!(!selected.reviewed);
}

#[test]
fn excessive_tip_count_and_trace_bytes_abstain_as_a_whole() {
    let extras = TRIANGLE.repeat(33).replace("id=\"tip\"", "");
    let (plan, catalog, view) = fixture(SUPPORT, &extras, "", IDENTITY);
    let result = candidates(&plan, &catalog, &view);
    assert!(result.iter().all(|candidate| traces(candidate).is_empty()));
    assert!(result.iter().all(|candidate| candidate.suspension_reasons.contains(&"endpoint_attachment_budget_exceeded_no_partial_priority".into())));
    let (plan, mut catalog, view) = fixture(SUPPORT, TRIANGLE, "", IDENTITY);
    for support in catalog.primitives.iter_mut().filter(|p| p.source.path_id.as_deref() == Some("support")) {
        support.style.stroke = Some("b".repeat(MAX_CHAIN_TRACE_BYTES));
    }
    assert!(binding_candidates(&plan, "tip-document", &catalog, &view, &plan.characteristics[0].id).is_err());
    let result = candidates(&plan, &catalog, &all_view(&plan, &catalog));
    assert!(result.iter().all(|candidate| traces(candidate).is_empty()));
    assert!(result.iter().all(|candidate| candidate.suspension_reasons.contains(&"endpoint_attachment_budget_exceeded_no_partial_priority".into())));
}

#[test]
fn source_geometry_and_transform_must_describe_the_recorded_triangle() {
    let (plan, catalog, view) = fixture(SUPPORT, TRIANGLE, "", IDENTITY);
    let tip = catalog.primitives.iter().position(|p| p.source.path_id.as_deref() == Some("tip")).unwrap();
    for change in 0..4 {
        let mut changed = catalog.clone();
        match change {
            0 => changed.primitives[tip].source.path_data = "M10 10 L16 8.5 L16 11.5 Z".into(),
            1 => changed.primitives[tip].source.transform[4] += 1.0,
            2 => changed.primitives[tip].bbox.x_min -= 1.0,
            _ => changed.primitives[tip].style.fill = Some("url(#paint)".into()),
        }
        assert!(binding_candidates(&plan, "tip-document", &changed, &view, &plan.characteristics[0].id).is_err());
        let result = candidates(&plan, &changed, &all_view(&plan, &changed));
        assert!(result.iter().flat_map(traces).next().is_none(), "source mutation {change}");
    }
}

#[test]
fn every_tip_trace_field_and_removal_are_bound_to_the_proposal_at_selection() {
    let (plan, catalog, view) = fixture(CHAIN, TRIANGLE, "", IDENTITY);
    let result = candidates(&plan, &catalog, &view);
    for field in ["scope_fingerprint", "matching_feature_ids", "matching_geometry_ids",
        "matching_triangle_ids", "matching_support_chains", "unique_within_selected_view",
        "base_gap_pt", "requires_review", "semantics"] {
        let mut changed = result.clone();
        let line = changed[0].evidence.iter_mut().find(|line| line.starts_with(TIP_PREFIX)).unwrap();
        let mut trace: serde_json::Value = serde_json::from_str(line.strip_prefix(TIP_PREFIX).unwrap()).unwrap();
        trace[field] = serde_json::Value::Null;
        *line = format!("{TIP_PREFIX}{}", serde_json::to_string(&trace).unwrap());
        assert!(select_binding(&changed, &changed[0].id, &catalog, "Trace altérée.").is_err(), "{field}");
    }
    let mut removed = result.clone();
    removed[0].evidence.retain(|line| !line.starts_with(TIP_PREFIX));
    assert!(select_binding(&removed, &removed[0].id, &catalog, "Trace supprimée.").is_err());
}

#[test]
fn legacy_competition_retains_both_traces_and_disconnected_invalid_graph_does_not_hide_tip() {
    let legacy = r#"<path d="M230 35 L215 100"/>"#;
    let (plan, catalog, view) = fixture(SUPPORT, TRIANGLE, legacy, IDENTITY);
    let result = candidates(&plan, &catalog, &view);
    assert!(result.iter().any(|candidate| candidate.evidence.iter().any(|line| line.starts_with(TRACE_PREFIX))));
    assert!(result.iter().flat_map(traces).all(|trace| !trace.unique_within_selected_view));
    assert!(result.windows(2).all(|pair| pair[0].distance_pt <= pair[1].distance_pt));

    let unrelated = r#"<path d="M260 210 L280 230"/><path d="M260 230 L280 210"/>"#;
    let (plan, catalog, view) = fixture(SUPPORT, TRIANGLE, unrelated, IDENTITY);
    let result = candidates(&plan, &catalog, &view);
    assert!(traces(&result[0])[0].unique_within_selected_view);
}

#[test]
fn nine_path_component_does_not_leave_a_usable_eight_path_prefix() {
    let points = [(198.,35.), (180.,35.), (150.,35.), (150.,70.), (150.,110.),
        (150.,150.), (140.,150.), (130.,150.), (120.,150.), (106.,150.)];
    let paths: String = points.windows(2).map(|pair| format!(
        r#"<path d="M{} {} L{} {}"/>"#, pair[0].0, pair[0].1, pair[1].0, pair[1].1)).collect();
    let (plan, catalog, view) = fixture(&paths, TRIANGLE, "", IDENTITY);
    assert!(candidates(&plan, &catalog, &view).iter().flat_map(traces).next().is_none());
}

use super::*;
use attachment_chains::{ChainTrace, CHAIN_PREFIX};
use crate::{dper_drawing::parse_text_plan, dper_pdf_geometry::parse_svg_geometry,
    dper_pdf_layout::PdfPageGeometry};

const TWO: &str = r#"<path id="first" d="M198 35 L150 35 L150 150"/><path id="last" d="M150 150 L100 150"/>"#;
const THREE: &str = r#"<path id="first" d="M198 35 L150 35"/><path id="middle" d="M150 35 L150 150"/><path id="last" d="M150 150 L100 150"/>"#;
const REVERSED: &str = r#"<path id="last" d="M100 150 L150 150"/><path id="middle" d="M150 150 L150 35"/><path id="first" d="M150 35 L198 35"/>"#;

fn fixture(paths: &str, extra: &str, matrix: [f64; 6])
    -> (DrawingControlPlan, SvgGeometryDocument, DrawingView) {
    let mut plan = parse_text_plan("Ø999 mm", "synthetic-chain.pdf");
    plan.page_geometry = vec![PdfPageGeometry { page: 1, width_pt: 400.0, height_pt: 320.0 }];
    let [a, b, c, d, e, f] = matrix;
    let corners = [(200.0, 30.0), (230.0, 30.0), (230.0, 40.0), (200.0, 40.0)]
        .map(|(x, y)| Point { x: a*x + c*y + e, y: b*x + d*y + f });
    plan.characteristics[0].source.page = Some(1);
    plan.characteristics[0].source.word_ids = vec!["chain-word".into()];
    plan.characteristics[0].source.pdf_bbox = Some(PdfBox {
        x_min: corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        y_min: corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        x_max: corners.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
        y_max: corners.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
    });
    let catalog = parse_svg_geometry(&format!(r#"<svg width="400pt" height="320pt" viewBox="0 0 400 320"><g fill="none" stroke="black" transform="matrix({a} {b} {c} {d} {e} {f})"><circle cx="80" cy="150" r="20"/><circle cx="200" cy="100" r="15"/>{paths}{extra}</g></svg>"#), 1).unwrap();
    assert_eq!(catalog.unsupported_path_count, 0, "The topology fixture must actually reach the attachment stage");
    assert!(catalog.warnings.iter().all(|warning| warning.starts_with("Géométries nominales candidates en points PDF")),
        "Do not mask a topology rejection with an unrelated extractor abstention: {:?}", catalog.warnings);
    let view = all_view(&plan, &catalog);
    (plan, catalog, view)
}

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn all_view(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument) -> DrawingView {
    define_view(plan, "chain-document", catalog, "Chaîne explicitement délimitée",
        &plan.characteristics.iter().map(|feature| feature.id.clone()).collect::<Vec<_>>(),
        &catalog.primitives.iter().map(|primitive| primitive.id.clone()).collect::<Vec<_>>()).unwrap()
}

fn candidates(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument, view: &DrawingView) -> Vec<BindingCandidate> {
    binding_candidates(plan, "chain-document", catalog, view, &plan.characteristics[0].id).unwrap()
}

fn traces(candidate: &BindingCandidate) -> Vec<ChainTrace> {
    candidate.evidence.iter().filter_map(|line| line.strip_prefix(CHAIN_PREFIX))
        .map(|encoded| serde_json::from_str(encoded).unwrap()).collect()
}

fn assert_no_chain_priority(result: &[BindingCandidate]) {
    assert!(result.windows(2).all(|pair| pair[0].distance_pt <= pair[1].distance_pt));
    assert!(result.iter().flat_map(traces).all(|trace| !trace.unique_within_selected_view));
}

#[test]
fn two_and_three_path_chains_preserve_order_sources_and_explicit_review() {
    for paths in [TWO, THREE, REVERSED] {
        for matrix in [IDENTITY, [0.0, 1.0, -1.0, 0.0, 340.0, -20.0]] {
            let (plan, catalog, view) = fixture(paths, "", matrix);
            let result = candidates(&plan, &catalog, &view);
            assert_eq!(result.len(), 2, "Every circle remains available");
            assert_eq!(result[0].geometry_id, catalog.primitives[0].id);
            assert!(result[0].distance_pt > result[1].distance_pt);
            let trace = traces(&result[0]).remove(0);
            assert_eq!(trace.ordered_supports.len(), if paths == TWO { 2 } else { 3 });
            assert_eq!(trace.schema_version, 1);
            assert_eq!(trace.joint_tolerance_pt, 0.25);
            assert_eq!(trace.tolerance_pt, 2.0);
            assert!(trace.unique_within_selected_view && trace.requires_review);
            assert_eq!(trace.semantics, NO_SEMANTICS);
            assert!((trace.text_gap_pt - 2.0).abs() < 1e-9);
            assert!(trace.circle_gap_pt < 1e-9);
            assert_eq!(trace.ordered_supports[0].source.path_id.as_deref(), Some("first"));
            assert_eq!(trace.ordered_supports.last().unwrap().source.path_id.as_deref(), Some("last"));
            for support in &trace.ordered_supports {
                let original = catalog.primitives.iter().find(|p| p.id == support.support_id).unwrap();
                assert_eq!(support.support_fingerprint, primitive_fingerprint(original).unwrap());
                assert_eq!(fingerprint(&support.source).unwrap(), fingerprint(&original.source).unwrap());
                assert_eq!(fingerprint(&support.style).unwrap(), fingerprint(&original.style).unwrap());
                assert_eq!(support.start_endpoint, original.points[support.start_endpoint_index]);
                assert_eq!(support.end_endpoint, original.points[support.end_endpoint_index]);
            }
            assert!(trace.joints.iter().all(|joint| joint.gap_pt == 0.0));
            assert_eq!(plan.characteristics[0].nominal, Some(999.0));
            let chosen = select_binding(&result, &result[0].id, &catalog, "Hypothèse à revoir.").unwrap();
            assert!(!chosen.reviewed);
            assert!(resolve_binding(&plan, "chain-document", &catalog, &view, &chosen).is_ok());
            assert!(!chosen.reviewed);
            let mut imported = serde_json::to_value(&chosen).unwrap();
            imported["reviewed"] = true.into();
            assert!(!serde_json::from_value::<BindingSelection>(imported).unwrap().reviewed);
        }
    }
}

#[test]
fn small_nonintersecting_gaps_are_recorded_without_snapping_and_larger_gaps_disconnect() {
    for gap in [0.2, 0.4] {
        let paths = format!(r#"<path id="first" d="M198 35 L150 35"/><path id="middle" d="M150 {} L150 150"/><path id="last" d="M150 150 L100 150"/>"#, 35.0 + gap);
        let (plan, catalog, view) = fixture(&paths, "", IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        let chains: Vec<_> = result.iter().flat_map(traces).collect();
        if gap < 0.25 {
            assert_eq!(chains.len(), 1);
            let joint = &chains[0].joints[0];
            assert_eq!(joint.from_endpoint, Point { x: 150.0, y: 35.0 });
            assert_eq!(joint.to_endpoint, Point { x: 150.0, y: 35.2 });
            assert!((joint.gap_pt - gap).abs() < 1e-9);
            assert_ne!(joint.from_endpoint, joint.to_endpoint);
        } else {
            assert!(chains.is_empty());
            assert_no_chain_priority(&result);
        }
    }
}

#[test]
fn interior_intersections_are_rejected_even_inside_the_endpoint_joint_radius() {
    // Near-joint T and X contacts formerly passed because both original ends
    // were within 0.25 pt of the actual interior intersection.
    for second in ["M150.2 35 L150.2 150", "M150.2 34.9 L150.2 150"] {
        let paths = format!(r#"<path id="first" d="M198 35 L150 35"/><path id="middle" d="{second}"/><path id="last" d="M150.2 150 L100 150"/>"#);
        let (plan, catalog, view) = fixture(&paths, "", IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        assert!(result.iter().all(|candidate| traces(candidate).is_empty()), "{second}");
        assert_no_chain_priority(&result);
        assert!(result.iter().all(|candidate| candidate.suspension_reasons.contains(
            &"endpoint_attachment_chain_topology_ambiguous_no_priority".into())));
    }
    // Exact reported regression, independent of text/circle placement.
    for second in ["M9.8 9.9 L9.8 20", "M9.8 10 L9.8 20"] {
        let (_, catalog, _) = fixture(&format!(r#"<path d="M1 10 L10 10"/><path d="{second}"/>"#), "", IDENTITY);
        let paths: Vec<_> = catalog.primitives.iter().filter(|p| p.circle.is_none()).collect();
        assert!(matches!(attachment_chains::topology(&paths, MAX_ATTACHMENT_COMPARISONS),
            Err(attachment_chains::TopologyError::Ambiguous)));
    }
}

#[test]
fn branches_interior_t_crossings_cycles_and_ambiguous_partners_never_keep_a_chain_prefix() {
    for extra in [
        r#"<path d="M150 35 L160 20"/>"#,
        r#"<path d="M150 80 L170 80"/>"#,
        r#"<path d="M130 80 L170 80"/>"#,
        r#"<path d="M100 150 L100 250 L198 250 L198 35"/>"#,
        r#"<path d="M150.1 35.1 L180 20"/>"#,
    ] {
        let (plan, catalog, view) = fixture(THREE, extra, IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|candidate| traces(candidate).is_empty()), "{extra}");
        assert_no_chain_priority(&result);
    }
}

#[test]
fn complete_legacy_traces_survive_conflicting_topology_without_priority() {
    let legacy = r#"<path id="complete" d="M198 35 L150 35 L150 150 L100 150"/>"#;
    for extra in [r#"<path d="M198 35 L250 35"/>"#,
        r#"<path d="M130 80 L170 80"/>"#] {
        let (plan, catalog, view) = fixture(legacy, extra, IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        assert_no_chain_priority(&result);
        let legacy: Vec<AttachmentTrace> = result.iter().flat_map(|candidate|
            candidate.evidence.iter().filter_map(|line| line.strip_prefix(TRACE_PREFIX))
                .map(|encoded| serde_json::from_str(encoded).unwrap())).collect();
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].schema_version, 1);
        assert!(!legacy[0].unique_within_selected_view);
    }
}

#[test]
fn disconnected_bad_contact_components_preserve_isolated_legacy_and_chain_priority() {
    let too_long = (0..9).map(|i| format!(r#"<path d="M{} 240 L{} 240"/>"#, 20 + i*10, 30 + i*10)).collect::<String>();
    for extra in [
        r#"<path d="M20 230 L40 230"/><path d="M40 230 L60 230"/><path d="M40 230 L40 260"/>"#,
        r#"<path d="M20 230 L60 230"/><path d="M40 230 L40 260"/>"#,
        r#"<path d="M20 230 L60 270"/><path d="M20 270 L60 230"/>"#,
        r#"<path d="M20 230 L60 230"/><path d="M60 230 L60 270"/><path d="M60 270 L20 270"/><path d="M20 270 L20 230"/>"#,
        too_long.as_str(),
    ] {
        for base in [r#"<path d="M198 35 L150 35 L150 150 L100 150"/>"#, THREE] {
            let (plan, catalog, view) = fixture(base, extra, IDENTITY);
            let result = candidates(&plan, &catalog, &view);
            assert_eq!(result.len(), 2);
            assert_eq!(result[0].geometry_id, catalog.primitives[0].id, "{extra}");
            if base == THREE {
                let trace = traces(&result[0]).remove(0);
                assert!(trace.unique_within_selected_view);
                assert_eq!(trace.ordered_supports.len(), 3);
            } else {
                let encoded = result[0].evidence.iter().find_map(|line| line.strip_prefix(TRACE_PREFIX)).unwrap();
                let trace: AttachmentTrace = serde_json::from_str(encoded).unwrap();
                assert!(trace.unique_within_selected_view);
                assert_eq!(trace.schema_version, 1);
            }
        }
    }
}

#[test]
fn chain_ambiguities_retain_circle_and_text_alternatives() {
    for extra in [r#"<circle cx="80" cy="150" r="19"/>"#,
        r#"<circle cx="120" cy="150" r="20"/>"#] {
        let (plan, catalog, view) = fixture(THREE, extra, IDENTITY);
        let result = candidates(&plan, &catalog, &view);
        assert_eq!(result.len(), 3);
        assert_eq!(result.iter().flat_map(traces).count(), 2);
        assert_no_chain_priority(&result);
    }
    let (mut plan, catalog, _) = fixture(THREE, "", IDENTITY);
    let mut second = plan.characteristics[0].clone();
    second.id = "other-chain-text".into();
    plan.characteristics.push(second);
    let result = candidates(&plan, &catalog, &all_view(&plan, &catalog));
    assert_no_chain_priority(&result);
    let trace = result.iter().flat_map(traces).next().unwrap();
    assert_eq!(trace.matching_feature_ids.len(), 2);
}

#[test]
fn changed_chain_sources_styles_scope_and_missing_middle_invalidate_selection() {
    let (plan, catalog, view) = fixture(THREE, "", IDENTITY);
    let original = candidates(&plan, &catalog, &view);
    let selected = select_binding(&original, &original[0].id, &catalog, "Sources à revoir.").unwrap();
    let middle = catalog.primitives.iter().position(|p| p.source.path_id.as_deref() == Some("middle")).unwrap();
    for change in 0..6 {
        let mut changed = catalog.clone();
        match change {
            0 => changed.primitives[middle].source.path_data.push(' '),
            1 => changed.primitives[middle].style.stroke = Some("red".into()),
            2 => changed.primitives[middle].source.transform[4] += 1.0,
            3 => changed.primitives[middle].source.element_attributes.push_str(" data-revised='yes'"),
            4 => changed.primitives[1].style.stroke = Some("blue".into()),
            _ => { changed.primitives.remove(middle); },
        }
        let changed_view = all_view(&plan, &changed);
        assert!(resolve_binding(&plan, "chain-document", &changed, &changed_view, &selected).is_err(), "change={change}");
        if change != 4 {
            assert!(select_binding(&original, &original[0].id, &changed, "Ancienne chaîne.").is_err());
        }
    }
    let ids: Vec<_> = view.geometry_ids.iter().filter(|id| **id != catalog.primitives[middle].id).cloned().collect();
    let scoped = define_view(&plan, "chain-document", &catalog, &view.label, &view.feature_ids, &ids).unwrap();
    assert!(resolve_binding(&plan, "chain-document", &catalog, &scoped, &selected).is_err());
    assert!(candidates(&plan, &catalog, &scoped).iter().all(|candidate| traces(candidate).is_empty()));
    assert!(!selected.reviewed);
}

#[test]
fn altered_chain_trace_coordinates_cannot_be_selected_as_original_evidence() {
    let (plan, catalog, view) = fixture(THREE, "", IDENTITY);
    let result = candidates(&plan, &catalog, &view);
    for field in ["point", "source", "joint"] {
        let mut altered = result.clone();
        let evidence = altered[0].evidence.iter_mut().find(|line| line.starts_with(CHAIN_PREFIX)).unwrap();
        let mut trace: ChainTrace = serde_json::from_str(evidence.strip_prefix(CHAIN_PREFIX).unwrap()).unwrap();
        match field {
            "point" => trace.ordered_supports[0].directed_points[0].x += 0.1,
            "source" => trace.ordered_supports[0].source.path_data.push(' '),
            _ => trace.joints[0].to_endpoint.x += 0.1,
        }
        *evidence = format!("{CHAIN_PREFIX}{}", serde_json::to_string(&trace).unwrap());
        assert!(select_binding(&altered, &altered[0].id, &catalog, "Trace modifiée.").is_err(), "{field}");
    }
}

#[test]
fn work_trace_and_component_budgets_abstain_as_a_whole() {
    let (plan, catalog, view) = fixture(THREE, "", IDENTITY);
    let supports: Vec<_> = catalog.primitives.iter().filter(|p| p.circle.is_none()).collect();
    assert!(matches!(attachment_chains::topology(&supports, 0), Err(attachment_chains::TopologyError::Budget)));
    let too_long = (0..9).map(|i| format!(r#"<path d="M{} 240 L{} 240"/>"#, 20 + i*10, 30 + i*10)).collect::<String>();
    let (_, long_catalog, _) = fixture(&too_long, "", IDENTITY);
    let supports: Vec<_> = long_catalog.primitives.iter().filter(|p| p.circle.is_none()).collect();
    assert!(matches!(attachment_chains::topology(&supports, MAX_ATTACHMENT_COMPARISONS),
        Err(attachment_chains::TopologyError::ComponentTooLong)));
    let mut eight = (0..6).map(|i| format!(r#"<path d="M{} 35 L{} 35"/>"#, 198 - i*8, 190 - i*8)).collect::<String>();
    eight.push_str(r#"<path d="M150 35 L150 150"/><path d="M150 150 L100 150"/>"#);
    let (eight_plan, eight_catalog, eight_view) = fixture(&eight, "", IDENTITY);
    let eight_result = candidates(&eight_plan, &eight_catalog, &eight_view);
    let trace = traces(&eight_result[0]).remove(0);
    assert_eq!(trace.ordered_supports.len(), 8);
    assert!(trace.unique_within_selected_view);
    // A valid chain encountered first must lose priority when completing the
    // selected view's unrelated topology would exceed the shared work budget.
    let dense = (0..29).map(|row| {
        let y = 200 + row*3;
        let points = (0..32).map(|column| format!("{}{} {y} ", if column == 0 { "M" } else { "L" }, 10 + column)).collect::<String>();
        format!(r#"<path d="{points}"/>"#)
    }).collect::<String>();
    let (dense_plan, dense_catalog, dense_view) = fixture(THREE, &dense, IDENTITY);
    let dense_result = candidates(&dense_plan, &dense_catalog, &dense_view);
    assert!(dense_result.iter().all(|candidate| traces(candidate).is_empty()));
    assert_no_chain_priority(&dense_result);
    assert!(dense_result.iter().all(|candidate| candidate.suspension_reasons.contains(
        &"endpoint_attachment_budget_exceeded_no_partial_priority".into())));
    // Full style/source snapshots count against the encoded chain trace budget.
    let mut large = catalog.clone();
    for support in large.primitives.iter_mut().filter(|p| p.circle.is_none()) {
        support.style.stroke = Some("b".repeat(MAX_CHAIN_TRACE_BYTES));
    }
    assert!(binding_candidates(&plan, "chain-document", &large, &view, &plan.characteristics[0].id).is_err());
    let result = candidates(&plan, &large, &all_view(&plan, &large));
    assert_no_chain_priority(&result);
    assert!(result.iter().all(|candidate| traces(candidate).is_empty()));
    assert!(result.iter().all(|candidate| candidate.suspension_reasons.contains(
        &"endpoint_attachment_budget_exceeded_no_partial_priority".into())));
}

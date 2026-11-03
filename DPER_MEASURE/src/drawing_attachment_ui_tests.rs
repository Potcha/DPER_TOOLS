//! Native presentation check with supplied SVG/text; real PDF extraction is tested separately.
use super::*;

fn assert_native_attachment_choice(support: &str, prefix: &str) {
    let mut fixture = Fixture::new();
    let fingerprint = fixture.app.inspection.plan.as_ref().unwrap().source_fingerprint.clone().unwrap();
    let svg = format!(r#"<svg width="240pt" height="180pt" viewBox="0 0 240 180"><circle cx="60" cy="90" r="20"/><circle cx="180" cy="90" r="20"/>{support}</svg>"#);
    let geometry = crate::dper_pdf_geometry::parse_svg_geometry(&svg, 1).unwrap();
    fixture.app.handle_drawing_request(DrawingRequest::InstallCatalog {
        catalog: DrawingGeometryCatalog { document_fingerprint: fingerprint, geometry },
        expected_revision: fixture.app.document_revision,
    }).unwrap();
    fixture.settle();
    fixture.define_view_by_pointer();
    fixture.prepare_binding_by_pointer();
    let draft = fixture.app.inspection_plan_ui.binding.as_ref().unwrap();
    assert!(draft.candidate_id.is_none(), "An attachment must not select a candidate");
    assert_eq!(draft.candidates.len(), 2);
    let preferred = draft.candidates[0].clone();
    let target = fixture.app.drawing_geometry_cache[0].geometry.primitives.iter()
        .find(|primitive| primitive.circle.as_ref().is_some_and(|circle| (circle.center.x - 180.0).abs() < 0.1)).unwrap();
    assert_eq!(preferred.geometry_id, target.id);
    let trace = preferred.evidence.iter().find(|line| line.starts_with(prefix)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(trace.split_once(':').unwrap().1).unwrap();
    assert_eq!(parsed["requires_review"], true);
    assert_eq!(parsed["unique_within_selected_view"], true);
    if prefix == "geometric_triangular_tip_attachment_hypothesis:" {
        fixture.text_rect("Pointe triangulaire : hypothèse géométrique à vérifier sur le plan.");
        assert!(fixture.text_rects(trace).is_empty(), "Detailed sources are shown on demand");
        fixture.click("Sources de la pointe triangulaire");
        assert_eq!(parsed["triangle"]["vertices"].as_array().unwrap().len(), 3);
        assert!(parsed["triangle"]["source"]["path_data"].as_str().unwrap().contains('Z'));
    }
    if prefix == "endpoint_attachment_chain_hypothesis:" {
        assert_eq!(parsed["ordered_supports"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["joints"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["joints"][0]["gap_pt"], 0.0);
        assert!(parsed["ordered_supports"].as_array().unwrap().iter().all(|support|
            support["source"]["path_data"].as_str().is_some_and(|data| !data.is_empty())));
    }
    assert!(!fixture.text_rects(trace).is_empty(), "The actual egui output must expose the source trace");
    assert!(!fixture.text_rects("Le rang compare les indices d'attache puis la proximité. Une attache reste une hypothèse à vérifier sur le plan.").is_empty());
    fixture.click("Enregistrer l'association choisie");
    assert!(fixture.app.inspection.bindings.is_empty(), "Submit still requires choice and justification");
    fixture.click(&format!("{} · rang {} · distance {:.2} pt", preferred.geometry_id, preferred.rank, preferred.distance_pt));
    fixture.type_into_empty("Décrire le rappel de cote et le cercle choisi", "Attache synthétique inspectée ; hypothèse explicitement choisie.");
    fixture.click("Enregistrer l'association choisie");
    assert_eq!(fixture.app.inspection.bindings.len(), 1);
    let selected = &fixture.app.inspection.bindings[0];
    assert_eq!(selected.candidate.id, preferred.id);
    assert!(!selected.reviewed);
    assert!(selected.candidate.evidence.contains(trace));
    assert!(fixture.app.measurements.is_empty());
    assert!(fixture.app.calibration.is_none());
}

#[test]
fn pointer_choice_keeps_attachment_trace_visible_and_never_auto_reviews() {
    assert_native_attachment_choice(
        r#"<path fill="none" stroke="black" d="M 128 22 L 200 22 L 200 90"/>"#,
        "endpoint_attachment_hypothesis:");
}

#[test]
fn pointer_choice_keeps_ordered_split_supports_visible_and_never_auto_reviews() {
    assert_native_attachment_choice(
        r#"<path fill="none" stroke="black" d="M128 22 L200 22"/><path fill="none" stroke="black" d="M200 22 L200 90"/>"#,
        "endpoint_attachment_chain_hypothesis:");
}

#[test]
fn pointer_choice_exposes_triangular_tip_sources_without_auto_selection_or_review() {
    assert_native_attachment_choice(
        r#"<path fill="none" stroke="black" d="M128 22 L220 22 L220 90 L206 90"/><path fill="black" d="M200 90 L206 88.5 L206 91.5 Z"/>"#,
        "geometric_triangular_tip_attachment_hypothesis:");
}

#[test]
fn pointer_choice_exposes_triangular_tip_and_split_supports_without_auto_review() {
    assert_native_attachment_choice(
        r#"<path fill="none" stroke="black" d="M128 22 L220 22"/><path fill="none" stroke="black" d="M220 22 L220 90 L206 90"/><path fill="black" d="M200 90 L206 88.5 L206 91.5 Z"/>"#,
        "geometric_triangular_tip_attachment_hypothesis:");
}

//! Explicit documentary datum/support selections. These never establish a
//! physical datum, authorize a geometric control, or restore review on import.

use crate::{
    dper_binding::DrawingView,
    dper_drawing::DrawingControlPlan,
    dper_pdf_geometry::SvgGeometryDocument,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct DatumSupportAssociation {
    pub id: String,
    pub feature_id: String,
    pub label: String,
    pub view_id: String,
    pub page: u32,
    pub document_fingerprint: String,
    pub support_id: String,
    pub justification: String,
    pub evidence: serde_json::Value,
    /// Only an explicit confirmation against current source evidence grants this.
    #[serde(skip_deserializing, default)]
    pub reviewed: bool,
}

/// Create a reviewable, persistable documentary proposal without confirmation.
pub(crate) fn propose(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    feature_id: &str,
    label: &str,
    support_id: &str,
    justification: &str,
) -> Result<DatumSupportAssociation, String> {
    let evidence = crate::drawing_structure::inspect_datum_support(
        plan, docfp, catalog, view, feature_id, label, support_id, justification,
    )?;
    let bytes = serde_json::to_vec(&evidence).map_err(|error| error.to_string())?;
    Ok(DatumSupportAssociation {
        id: format!("datum-support-{}", crate::dper_binding::fingerprint_bytes(&bytes)),
        feature_id: feature_id.into(),
        label: label.into(),
        view_id: view.id.clone(),
        page: view.page,
        document_fingerprint: docfp.into(),
        support_id: support_id.into(),
        justification: justification.trim().into(),
        evidence,
        reviewed: false,
    })
}

/// Recreate all documentary evidence before granting current-session review.
/// Call only after the user explicitly confirms this proposal.
pub(crate) fn confirm(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    proposal: &DatumSupportAssociation,
) -> Result<DatumSupportAssociation, String> {
    validate_evidence(plan, docfp, catalog, view, proposal)?;
    let mut confirmed = proposal.clone();
    confirmed.reviewed = true;
    Ok(confirmed)
}

/// Source freshness is checked even for historical or unconfirmed associations.
pub(crate) fn validate_evidence(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    association: &DatumSupportAssociation,
) -> Result<(), String> {
    let current = propose(plan, docfp, catalog, view, &association.feature_id,
        &association.label, &association.support_id, &association.justification)?;
    let mut expected = association.clone();
    // Review is an independent session-local decision, never documentary evidence.
    expected.reviewed = false;
    if current != expected {
        return Err("Les sources ou le périmètre du lien repère/support ont changé ; recréez la proposition.".into());
    }
    Ok(())
}

pub(crate) fn validate_current(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    association: &DatumSupportAssociation,
) -> Result<(), String> {
    validate_evidence(plan, docfp, catalog, view, association)?;
    if !association.reviewed {
        return Err("Lien repère/support à confirmer explicitement dans cette session.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (DrawingControlPlan, SvgGeometryDocument, DrawingView) {
        let xml = r#"<html><body><doc><page width="400" height="320"><flow><block xMin="10" yMin="10" xMax="300" yMax="70">
<line xMin="10" yMin="10" xMax="290" yMax="20"><word xMin="10" yMin="10" xMax="25" yMax="20">⌖</word><word xMin="30" yMin="10" xMax="35" yMax="20">|</word><word xMin="40" yMin="10" xMax="70" yMax="20">Ø0,1</word><word xMin="75" yMin="10" xMax="95" yMax="20">mm</word><word xMin="100" yMin="10" xMax="105" yMax="20">|</word><word xMin="110" yMin="10" xMax="125" yMax="20">A</word></line>
<line xMin="10" yMin="40" xMax="80" yMax="50"><word xMin="10" yMin="40" xMax="55" yMax="50">DATUM</word><word xMin="60" yMin="40" xMax="75" yMax="50">A</word></line>
</block></flow></page></doc></body></html>"#;
        let plan = crate::dper_drawing::from_pdf_layout(
            crate::dper_pdf_layout::parse_bbox_layout(xml).unwrap(), "datum.pdf");
        let catalog = crate::dper_pdf_geometry::parse_svg_geometry(
            r#"<svg width="400pt" height="320pt" viewBox="0 0 400 320"><path stroke="black" fill="none" d="M 10 80 L 300 80"/></svg>"#, 1).unwrap();
        let view = crate::dper_binding::define_view(&plan, "source", &catalog, "Vue",
            &[plan.characteristics[0].id.clone()], &[catalog.primitives[0].id.clone()]).unwrap();
        (plan, catalog, view)
    }

    fn proposal(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument, view: &DrawingView) -> DatumSupportAssociation {
        propose(plan, "source", catalog, view, &plan.characteristics[0].id, "A",
            &catalog.primitives[0].id, " Support vérifié sur le plan ").unwrap()
    }

    #[test]
    fn dpm4_confirmation_is_documentary_and_import_requires_review() {
        let (plan, catalog, view) = fixture();
        let proposal = proposal(&plan, &catalog, &view);
        assert!(!proposal.reviewed);
        assert!(validate_current(&plan, "source", &catalog, &view, &proposal).is_err());
        let selected = confirm(&plan, "source", &catalog, &view, &proposal).unwrap();
        assert!(validate_current(&plan, "source", &catalog, &view, &selected).is_ok());
        assert_eq!(selected.justification, "Support vérifié sur le plan");
        assert_eq!(selected.evidence["declaration"]["source_text"], "DATUM A");
        for key in ["reference_resolved", "measured_on_photo", "reviewed"] {
            assert_eq!(selected.evidence[key], false);
        }
        assert_eq!(crate::drawing_structure::suspension_reason(&plan.characteristics[0]),
            Some("drawing_relation_unresolved"));
        let serialized = serde_json::to_string(&selected).unwrap();
        let restored: DatumSupportAssociation = serde_json::from_str(&serialized).unwrap();
        assert!(!restored.reviewed);
        assert!(validate_evidence(&plan, "source", &catalog, &view, &restored).is_ok());
        assert!(validate_current(&plan, "source", &catalog, &view, &restored).is_err());
        assert!(confirm(&plan, "source", &catalog, &view, &restored).is_ok());
    }

    #[test]
    fn dpm4_modified_identity_evidence_or_sources_cannot_be_confirmed() {
        let (plan, catalog, view) = fixture();
        let proposed = proposal(&plan, &catalog, &view);
        for field in ["id", "feature_id", "label", "view_id", "document_fingerprint", "support_id", "justification"] {
            let mut json = serde_json::to_value(&proposed).unwrap();
            json[field] = serde_json::json!("changed");
            let changed: DatumSupportAssociation = serde_json::from_value(json).unwrap();
            assert!(confirm(&plan, "source", &catalog, &view, &changed).is_err(), "{field}");
        }
        let mut changed = proposed.clone();
        changed.page += 1;
        assert!(confirm(&plan, "source", &catalog, &view, &changed).is_err());
        changed = proposed.clone();
        changed.evidence["declaration"]["source_text"] = serde_json::json!("REPERE A");
        assert!(confirm(&plan, "source", &catalog, &view, &changed).is_err());
        let mut changed_plan = plan.clone();
        changed_plan.spatial_text_groups[1].source_text = "REPERE A".into();
        assert!(confirm(&changed_plan, "source", &catalog, &view, &proposed).is_err());
        let mut changed_catalog = catalog.clone();
        changed_catalog.primitives[0].points[1].x -= 1.0;
        assert!(confirm(&plan, "source", &changed_catalog, &view, &proposed).is_err());
        assert!(confirm(&plan, "new-source", &catalog, &view, &proposed).is_err());
    }
}

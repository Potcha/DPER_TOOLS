//! Read-only progress derived from the same reassessed workspace used by MCP and UI.
use crate::dper_drawing::{declared_repetition, DrawingMeasurability};
use crate::dper_inspection::InspectionWorkspace;
use crate::dper_metrology::{ConformityStatus, GeometryState};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct CharacteristicProgress {
    pub feature_id: String,
    pub label: String,
    pub expected_count: u32,
    pub current_observations: usize,
    pub measurement_ids: Vec<u64>,
    pub groups: Vec<String>,
    pub source_page: Option<u32>,
    pub source_text: String,
    pub historical_observations: usize,
    pub conformity: ConformityStatus,
    pub status: &'static str,
    pub blockers: Vec<String>,
    pub next_action: ControlAction,
}

/// Guidance only: executing an action still goes through the existing guarded
/// operations. In particular, a suggested observation is not a validated one.
#[derive(Debug, Serialize)]
pub(crate) struct ControlAction {
    pub code: &'static str,
    pub instruction: &'static str,
    pub occurrence_id: Option<String>,
}

impl ControlAction {
    fn new(code: &'static str, instruction: &'static str) -> Self {
        Self { code, instruction, occurrence_id: None }
    }
}

fn next_action(workspace: &InspectionWorkspace, feature: &crate::dper_drawing::DrawingCharacteristic,
    count: usize, repeated: Option<&crate::drawing_structure::OccurrenceObservations>,
    conformity: ConformityStatus) -> ControlAction {
    if feature.measurability == DrawingMeasurability::NotMeasurable2d {
        return ControlAction::new("outside_2d", "Documenter une autre méthode de contrôle : cette cote n'est pas vérifiable sur cette image 2D.");
    }
    if feature.has_unresolved_unit_context() || feature.unit.is_none() {
        return ControlAction::new("resolve_units", "Résoudre l'unité et son contexte dans le plan ; ne pas déduire une échelle du nominal.");
    }
    if !feature.confirmed || feature.measurability != DrawingMeasurability::PlanarCandidate {
        return ControlAction::new("review_interpretation", "Vérifier le texte source, le type de cote et sa visibilité, puis confirmer l'interprétation dans le panneau du plan.");
    }
    // Documentary relations and missing tolerances can suspend the decision
    // without preventing acquisition of real contours. Their assessed blockers
    // remain available when reviewing the resulting observation.
    if let Some(group) = repeated {
        match group.decision_blocker {
            Some("assignment_unavailable") => return ControlAction::new("assign_occurrences", "Affecter les occurrences distinctes aux objets de la vue PDF courante avant d'observer leurs contours."),
            Some("ambiguous_observations") => return ControlAction::new("review_observations", "Résoudre les observations multiples d'une occurrence avant de poursuivre la décision du groupe."),
            Some("mixed_observation_groups") => return ControlAction::new("review_groups", "Réunir les observations dans une même capture et un même groupe de contrôle, puis réexaminer les preuves."),
            Some("incomplete_observations") => {
                let missing = workspace.occurrence_assignments.iter().find(|a| a.feature_id == feature.id)
                    .and_then(|a| a.members.iter().find(|m| !group.observed_occurrence_ids.contains(&m.id)));
                return ControlAction { occurrence_id: missing.map(|m| m.id.clone()),
                    ..ControlAction::new("observe_contours", "Préparer l'occurrence manquante, vérifier le recalage et les contours sur les pixels, puis enregistrer explicitement l'aperçu courant.") };
            }
            _ => {}
        }
    } else if count > 1 {
        return ControlAction::new("review_observations", "Choisir quelle observation représente cette cote ; les observations concurrentes ne constituent pas une décision unique.");
    } else if count == 0 {
        if matches!(feature.kind, crate::dper_drawing::DrawingDimensionKind::Diameter | crate::dper_drawing::DrawingDimensionKind::Radius) {
            let bindings: Vec<_> = workspace.bindings.iter().filter(|b| b.candidate.feature_id == feature.id).collect();
            if bindings.len() != 1 || !bindings[0].reviewed {
                return ControlAction::new("review_association", "Choisir et vérifier l'association de la cote avec le cercle de la vue PDF ; conserver les alternatives ambiguës pour examen.");
            }
        }
        return ControlAction::new("observe_contours", "Préparer une observation, vérifier l'association et le recalage, prévisualiser les pixels puis enregistrer explicitement les contours.");
    }
    if feature.tolerance.is_none() {
        return ControlAction::new("review_tolerance", "Rechercher la tolérance dans le document source ; sans tolérance établie, conserver une conformité indéterminée.");
    }
    if matches!(conformity, ConformityStatus::Indeterminate | ConformityStatus::NotApplicable) {
        return ControlAction::new("review_decision", "Examiner les preuves et blocages de chaque observation, la correspondance plan/photo et la base d'incertitude avant toute décision physique.");
    }
    ControlAction::new("report_result", "Conserver les preuves et exporter le résultat et ses limites dans le rapport ; cette étape ne valide pas les autres cotes du plan.")
}

pub(crate) fn summarize(workspace: &InspectionWorkspace) -> Vec<CharacteristicProgress> {
    let Some(plan) = &workspace.plan else { return Vec::new(); };
    let occurrences = crate::drawing_structure::occurrence_observations(workspace);
    plan.characteristics.iter().map(|feature| {
        let records: Vec<_> = workspace.records.iter().filter(|r| r.feature_id == feature.id).collect();
        let current: Vec<_> = records.iter().filter(|r| !r.imported && r.measurement_id.is_some()
            && r.outcome.geometry_state == GeometryState::Validated).collect();
        let expected = declared_repetition(feature).unwrap_or(1);
        let repeated = occurrences.iter().find(|item| item.feature_id == feature.id);
        let count = repeated.map_or(current.len(), |item| item.observed_occurrence_ids.len());
        let mut blockers = Vec::new();
        if !feature.confirmed { blockers.push("Interprétation du plan à confirmer.".into()); }
        if feature.has_unresolved_unit_context() { blockers.push("Unité du plan à résoudre.".into()); }
        if feature.tolerance.is_none() { blockers.push("Tolérance non disponible ou non interprétée.".into()); }
        if let Some(reason) = crate::drawing_structure::suspension_reason(feature).filter(|_| repeated.is_none()) {
            blockers.push(crate::dper_metrology::reason_label(reason).to_owned());
        }
        if let Some(group) = repeated {
            if group.decision_blocker.is_some() { blockers.push(group.decision_explanation().into()); }
        }
        for record in records.iter().filter(|record| !record.imported) {
            for reason in &record.outcome.blocking_reasons {
                let text = crate::dper_metrology::reason_label(reason).to_owned();
                if !blockers.contains(&text) { blockers.push(text); }
            }
        }
        let status = if feature.measurability == DrawingMeasurability::NotMeasurable2d {
            "Non mesurable en 2D"
        } else if count == 0 { "Aucune observation courante vérifiée" }
        else if count < expected as usize { "Observations incomplètes" }
        else if repeated.is_none() && count > 1 { "Observations multiples à examiner" }
        else { "Contours vérifiés par logiciel ; conformité à examiner" };
        let mut groups: Vec<_> = current.iter().map(|r| r.group.clone()).collect();
        groups.sort(); groups.dedup();
        let conformity = if !feature.confirmed || feature.measurability != DrawingMeasurability::PlanarCandidate {
            ConformityStatus::Indeterminate
        } else if let Some(group) = repeated {
            group.conformity
        } else if let [record] = current.as_slice() {
            record.outcome.conformity_status
        } else { ConformityStatus::Indeterminate };
        CharacteristicProgress { feature_id: feature.id.clone(), label: feature.label.clone(),
            expected_count: expected, current_observations: count,
            measurement_ids: current.iter().filter_map(|r| r.measurement_id).collect(), groups,
            source_page: feature.source.page, source_text: feature.source.exact_text.clone(),
            historical_observations: records.iter().filter(|r| r.imported).count(),
            next_action: next_action(workspace, feature, count, repeated, conformity),
            conformity, status, blockers }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_requirement_is_visible_without_inventing_observations() {
        let mut workspace = InspectionWorkspace::default();
        workspace.plan = Some(crate::dper_drawing::parse_text_plan("Ø4 ±0,1 mm\n18 mm\n45°", "plan.pdf"));
        let summary = summarize(&workspace);
        assert_eq!(summary.len(), 3);
        assert!(summary.iter().all(|item| item.current_observations == 0 && item.measurement_ids.is_empty()));
        assert!(summary.iter().all(|item| item.status == "Aucune observation courante vérifiée"));
        assert!(summary.iter().any(|item| item.blockers.iter().any(|b| b.contains("Tolérance"))));
    }
    #[test]
    fn absence_of_a_plan_has_no_synthetic_requirement() {
        assert!(summarize(&InspectionWorkspace::default()).is_empty());
    }

    #[test]
    fn guidance_resumes_after_review_without_inventing_a_tolerance_or_observation() {
        let mut workspace = InspectionWorkspace::default();
        workspace.plan = Some(crate::dper_drawing::parse_text_plan("18 mm", "plan.pdf"));
        assert_eq!(summarize(&workspace)[0].next_action.code, "review_interpretation");
        workspace.plan.as_mut().unwrap().characteristics[0].confirmed = true;
        let before = serde_json::to_value(&workspace).unwrap();
        let summary = summarize(&workspace);
        assert_eq!(summary[0].next_action.code, "observe_contours");
        assert_eq!(summary[0].current_observations, 0);
        assert_eq!(serde_json::to_value(&workspace).unwrap(), before);
        let feature = &workspace.plan.as_ref().unwrap().characteristics[0];
        assert_eq!(next_action(&workspace, feature, 1, None, ConformityStatus::Indeterminate).code, "review_tolerance");
    }

    #[test]
    fn guidance_preserves_local_ambiguity_and_non_2d_limits() {
        let mut workspace = InspectionWorkspace::default();
        workspace.plan = Some(crate::dper_drawing::parse_text_plan("18 ±0,1 mm\n20 mm", "plan.pdf"));
        for feature in &mut workspace.plan.as_mut().unwrap().characteristics { feature.confirmed = true; }
        workspace.plan.as_mut().unwrap().characteristics[0].measurability = DrawingMeasurability::NotMeasurable2d;
        let summary = summarize(&workspace);
        assert_eq!(summary[0].next_action.code, "outside_2d");
        assert_eq!(summary[1].next_action.code, "observe_contours", "one blocked feature must not block another");
        let feature = &workspace.plan.as_ref().unwrap().characteristics[1];
        assert_eq!(next_action(&workspace, feature, 2, None, ConformityStatus::Indeterminate).code, "review_observations");
    }

    #[test]
    fn guidance_requires_repeated_assignment_before_missing_contours() {
        let mut workspace = InspectionWorkspace::default();
        workspace.plan = Some(crate::dper_drawing::parse_text_plan("4xØ4 ±0,1 mm", "plan.pdf"));
        workspace.plan.as_mut().unwrap().characteristics[0].confirmed = true;
        assert_eq!(summarize(&workspace)[0].next_action.code, "assign_occurrences");
    }
}

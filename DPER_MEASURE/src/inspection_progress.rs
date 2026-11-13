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
}

//! Documentary structure derived from the control plan. No occurrence is inferred
//! from a nominal count and no relation is authorized by deserialization.

use crate::dper_drawing::{declared_repetition, has_geometric_control_source, DrawingCharacteristic, DrawingControlPlan, DrawingSource};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub(crate) struct DrawingStructure<'a> {
    groups: Vec<RepetitionGroup<'a>>,
    relations: Vec<RelationEvidence<'a>>,
}

#[derive(Debug, Serialize)]
struct RepetitionGroup<'a> {
    id: String,
    declaration_id: &'a str,
    expected_count: u32,
    identified_occurrence_ids: Vec<String>,
    source: &'a DrawingSource,
    state: &'static str,
}

#[derive(Debug, Serialize)]
struct RelationEvidence<'a> {
    dependent_id: &'a str,
    kind: &'a str,
    target_id: Option<&'a str>,
    state: &'static str,
    justification: &'a str,
    dependent_source: &'a DrawingSource,
    target_source: Option<&'a DrawingSource>,
}

pub(crate) fn suspension_reason(feature: &DrawingCharacteristic) -> Option<&'static str> {
    if !feature.relations.is_empty() || has_geometric_control_source(feature) {
        Some("drawing_relation_unresolved")
    } else if declared_repetition(feature).is_some_and(|count| count != 1) {
        Some("drawing_occurrences_unresolved")
    } else {
        None
    }
}

pub(crate) fn structure(plan: &DrawingControlPlan) -> DrawingStructure<'_> {
    let mut features_by_id = BTreeMap::new();
    for feature in &plan.characteristics {
        features_by_id.entry(feature.id.as_str()).and_modify(|entry| *entry = None).or_insert(Some(feature));
    }
    let mut groups = Vec::new();
    let mut relations = Vec::new();
    for feature in &plan.characteristics {
        if let Some(expected_count) = declared_repetition(feature).filter(|count| *count != 1) {
            groups.push(RepetitionGroup {
                id: format!("repetition:{}", feature.id),
                declaration_id: &feature.id,
                expected_count,
                identified_occurrence_ids: Vec::new(),
                source: &feature.source,
                state: if expected_count == 0 || feature.id.trim().is_empty()
                    || matches!(features_by_id.get(feature.id.as_str()), Some(None)) {
                    "invalid_declaration"
                } else {
                    "occurrences_unresolved"
                },
            });
        }
        if feature.relations.is_empty() && has_geometric_control_source(feature) {
            relations.push(RelationEvidence {
                dependent_id: &feature.id, kind: "geometric_control", target_id: None,
                state: "reference_unresolved", justification: "Cadre, références et méthode de contrôle à identifier sur le plan.",
                dependent_source: &feature.source, target_source: None,
            });
        }
        for relation in &feature.relations {
            let target_id = relation.target_id.as_str();
            let (state, target_source) = if target_id.trim().is_empty() || target_id == feature.id
                || relation.kind.trim().is_empty() || relation.source_text.trim().is_empty() {
                ("invalid_declaration", None)
            } else {
                match features_by_id.get(target_id) {
                    None => ("target_missing", None),
                    Some(None) => ("target_ambiguous", None),
                    Some(Some(target)) => ("relation_unreviewed", Some(&target.source)),
                }
            };
            relations.push(RelationEvidence {
                dependent_id: &feature.id, kind: &relation.kind, target_id: Some(target_id),
                state, justification: &relation.source_text, dependent_source: &feature.source, target_source,
            });
        }
    }
    DrawingStructure { groups, relations }
}

pub(crate) fn summary_lines(plan: &DrawingControlPlan) -> Vec<String> {
    let structure = structure(plan);
    let mut lines = Vec::new();
    for group in structure.groups {
        lines.push(format!("{} : {} occurrence(s) déclarée(s), aucune individualisée. Source : {}",
            group.declaration_id, group.expected_count, group.source.exact_text));
    }
    for relation in structure.relations {
        let state = match relation.state {
            "target_missing" => "cible absente",
            "target_ambiguous" => "cible ambiguë",
            "invalid_declaration" => "déclaration invalide",
            "relation_unreviewed" => "relation à vérifier",
            _ => "référence non résolue",
        };
        lines.push(format!("{} → {} · {} · {state}. Justification : {}",
            relation.dependent_id, relation.target_id.unwrap_or("référence à identifier"), relation.kind, relation.justification));
        if let Some(target) = relation.target_source {
            lines.push(format!("Source de la cible : {}", target.exact_text));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dper_drawing::{parse_text_plan, DrawingRelation};

    #[test]
    fn declared_repetition_remains_one_documentary_requirement_after_review_and_round_trip() {
        let mut plan = parse_text_plan("4× Ø4 ±0.1 mm\nØ6 ±0.2 mm", "plan.pdf");
        assert_eq!(plan.characteristics.len(), 2);
        let before = serde_json::to_value(structure(&plan)).unwrap();
        assert_eq!(before["groups"][0]["expected_count"], 4);
        assert_eq!(before["groups"][0]["identified_occurrence_ids"], serde_json::json!([]));
        assert_eq!(before["groups"][0]["source"]["exact_text"], "4× Ø4 ±0.1 mm");
        plan.characteristics[0].confirmed = true;
        plan.characteristics[0].measurement_id = Some(91);
        let reloaded = serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
        assert_eq!(before, serde_json::to_value(structure(&reloaded)).unwrap());
        assert_eq!(suspension_reason(&reloaded.characteristics[0]), Some("drawing_occurrences_unresolved"));
        assert_eq!(suspension_reason(&reloaded.characteristics[1]), None);
        let mut limits = parse_text_plan("4× Ø MIN 3.9 MAX 4.1 mm", "limites.pdf");
        assert_eq!(limits.characteristics[0].repetition, Some(4));
        assert!(limits.characteristics[0].tolerance.is_some());
        limits.characteristics[0].repetition = None;
        assert_eq!(suspension_reason(&limits.characteristics[0]), Some("drawing_occurrences_unresolved"));
        assert_eq!(serde_json::to_value(structure(&limits)).unwrap()["groups"][0]["expected_count"], 4);
    }

    #[test]
    fn relations_keep_both_sources_and_abstain_on_missing_duplicate_or_self_targets() {
        let mut plan = parse_text_plan("Ø4 ±0.1 mm\nØ6 ±0.2 mm", "plan.pdf");
        let source = plan.characteristics[0].id.clone();
        let target = plan.characteristics[1].id.clone();
        plan.characteristics[0].relations = [target, "missing".into(), source].into_iter().map(|target_id|
            DrawingRelation { kind: "axis".into(), target_id, source_text: "Renvoi textuel A".into() }).collect();
        let result = serde_json::to_value(structure(&plan)).unwrap();
        assert_eq!(result["relations"][0]["state"], "relation_unreviewed");
        assert_eq!(result["relations"][0]["target_source"]["exact_text"], "Ø6 ±0.2 mm");
        assert_eq!(result["relations"][1]["state"], "target_missing");
        assert_eq!(result["relations"][2]["state"], "invalid_declaration");
        plan.characteristics.push(plan.characteristics[1].clone());
        assert_eq!(serde_json::to_value(structure(&plan)).unwrap()["relations"][0]["state"], "target_ambiguous");
    }

    #[test]
    fn historical_reference_comparison_does_not_accept_one_measurement_for_four_occurrences() {
        use crate::reference::{compare_dimension, ComparisonStatus, ReferenceDimension};
        let mut dimension: ReferenceDimension = serde_json::from_value(serde_json::json!({
            "id": "ocr-1", "label": "Ø4", "nominal_mm": 4.0, "tolerance_minus_mm": 0.1,
            "tolerance_plus_mm": 0.1, "source_text": "4× Ø4 ±0.1 mm", "confirmed": true, "measurement_id": 1,
        })).unwrap();
        let circle = serde_json::from_value(serde_json::json!({
            "id": 1, "kind": "Circle", "label": "Mesure manuelle", "mm_per_pixel": 0.1,
            "expanded_uncertainty_mm": 0.02, "points": [{"x": 50.0, "y": 50.0}, {"x": 70.0, "y": 50.0}],
        })).unwrap();
        for source in ["4× Ø4 ±0.1 mm", "4× Ø MIN 3.9 MAX 4.1 mm"] {
            dimension.source_text = source.into();
            assert_eq!(compare_dimension(&dimension, std::slice::from_ref(&circle)).status, ComparisonStatus::ToConfirm);
        }
        dimension.source_text = "Ø4 ±0.1 mm".into();
        assert_eq!(compare_dimension(&dimension, &[circle]).status, ComparisonStatus::InTolerance);
    }

    #[test]
    fn reclassifying_a_geometric_control_does_not_remove_its_source_dependency() {
        use crate::dper_drawing::{propose_interpretation, DrawingDimensionKind, DrawingUnit};
        let mut plan = parse_text_plan("⌖ Ø0,1 mm A B", "plan.pdf");
        let id = plan.characteristics[0].id.clone();
        propose_interpretation(&mut plan, &id, DrawingDimensionKind::Diameter,
            Some(DrawingUnit::Millimeter), "Interprétation de test").unwrap();
        plan.characteristics[0].confirmed = true;
        assert_eq!(suspension_reason(&plan.characteristics[0]), Some("drawing_relation_unresolved"));
        assert_eq!(serde_json::to_value(structure(&plan)).unwrap()["relations"][0]["state"], "reference_unresolved");
    }
}

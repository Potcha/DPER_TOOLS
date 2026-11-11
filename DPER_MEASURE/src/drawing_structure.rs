//! Documentary structure derived from the control plan. No occurrence is inferred
//! from a nominal count and no relation is authorized by deserialization.

use crate::dper_drawing::{declared_repetition, has_geometric_control_source, DrawingCharacteristic, DrawingControlPlan, DrawingSource};
use crate::dper_metrology::{ConformityStatus, GeometryState};
use crate::dper_pdf_layout::{PdfTextGroup, TextOrientation};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub(crate) struct DrawingStructure<'a> {
    groups: Vec<RepetitionGroup<'a>>,
    relations: Vec<RelationEvidence<'a>>,
    datum_declarations: Vec<DatumDeclaration<'a>>,
    datum_references: Vec<DatumReference<'a>>,
}

#[derive(Debug, Serialize)]
struct DatumDeclaration<'a> {
    label: &'a str,
    source: &'a PdfTextGroup,
}

#[derive(Debug, Serialize)]
struct DatumReference<'a> {
    dependent_id: &'a str,
    dependent_source: &'a DrawingSource,
    label: &'a str,
    state: &'static str,
    justification: &'static str,
    candidate_source_line_id: Option<&'a str>,
}

fn datum_label(text: &str) -> bool {
    text.len() == 1 && text.as_bytes()[0].is_ascii_uppercase()
}

fn datum_declaration(group: &PdfTextGroup) -> Option<DatumDeclaration<'_>> {
    if !exact_text_group(group) { return None; }
    let mut tokens = group.source_text.split_whitespace();
    let prefix = tokens.next()?;
    let label = tokens.next()?;
    (matches!(prefix, "DATUM" | "REPÈRE" | "REPERE") && datum_label(label)
        && tokens.next().is_none()).then_some(DatumDeclaration { label, source: group })
}

fn exact_text_group(group: &PdfTextGroup) -> bool {
    group.proposed_text.as_deref() == Some(group.source_text.as_str())
        && group.orientation != TextOrientation::Ambiguous
        && group.suspension_reasons.is_empty()
        && !group.word_ids.is_empty()
}

fn framed_datum_labels<'a>(feature: &'a DrawingCharacteristic, group: &PdfTextGroup) -> Option<Vec<&'a str>> {
    let source = &feature.source;
    if !exact_text_group(group) || source.exact_text != group.source_text
        || source.pdf_bbox != Some(group.bbox) || source.word_ids != group.word_ids
        || source.interpretation_text.is_some() || feature.nominal.is_none() {
        return None;
    }
    let mut cells = source.exact_text.split('|').map(str::trim);
    if !matches!(cells.next()?, "⌖" | "⊥" | "∥" | "⌯" | "⌰") { return None; }
    let tolerance_cell = cells.next()?;
    let tolerance_text = tolerance_cell.strip_prefix(['Ø', 'ø', '⌀', 'φ']).unwrap_or(tolerance_cell).trim();
    let observed = source.exact_text.get(source.text_start?..source.text_end?)?.trim();
    if observed.is_empty() || tolerance_text != observed {
        return None;
    }
    let labels: Vec<_> = cells.take(4).collect();
    let unique: BTreeSet<_> = labels.iter().copied().collect();
    (!labels.is_empty() && labels.len() <= 3 && unique.len() == labels.len()
        && labels.iter().all(|label| datum_label(label))).then_some(labels)
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
    let mut text_groups = BTreeMap::new();
    let mut datum_declarations = Vec::new();
    let mut datums_by_label = BTreeMap::new();
    for group in &plan.spatial_text_groups {
        if let Some(first_word_id) = group.word_ids.first() {
            text_groups.entry((group.page, first_word_id.as_str()))
                .and_modify(|entry| *entry = None).or_insert(Some(group));
        }
        if let Some(declaration) = datum_declaration(group) {
            datums_by_label.entry(declaration.label)
                .and_modify(|entry| *entry = None).or_insert(Some(group));
            datum_declarations.push(declaration);
        }
    }
    let mut datum_references = Vec::new();
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
            if let Some(group) = feature.source.page.zip(feature.source.word_ids.first().map(String::as_str))
                .and_then(|key| text_groups.get(&key)).and_then(|group| *group) {
                if let Some(labels) = framed_datum_labels(feature, group) {
                    for label in labels {
                        let (state, justification, candidate_source_line_id) = match datums_by_label.get(label) {
                            None => ("target_missing", "Aucune déclaration explicite de ce repère dans le texte extrait.", None),
                            Some(None) => ("target_ambiguous", "Plusieurs déclarations portent le même repère.", None),
                            Some(Some(target)) if target.page != group.page => ("reference_unresolved", "La déclaration est sur une autre page ; le lien de vue reste à établir.", None),
                            Some(Some(target)) => ("candidate_view_unresolved", "Même lettre dans le cadre et dans une déclaration unique sur la même page. Vérifier la vue et le support du repère.", Some(target.source_line_id.as_str())),
                        };
                        datum_references.push(DatumReference {
                            dependent_id: &feature.id, dependent_source: &feature.source,
                            label, state, justification, candidate_source_line_id,
                        });
                    }
                }
            }
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
    DrawingStructure { groups, relations, datum_declarations, datum_references }
}

pub(crate) fn structure_with_occurrences<'a>(
    plan: &'a DrawingControlPlan,
    current_assignments: &[&crate::dper_binding::OccurrenceAssignment],
) -> DrawingStructure<'a> {
    let mut structure = structure(plan);
    for group in &mut structure.groups {
        if group.state != "occurrences_unresolved" { continue; }
        if let Some(assignment) = current_assignments.iter().find(|item| item.feature_id == group.declaration_id) {
            group.identified_occurrence_ids = assignment.members.iter().map(|member| member.id.clone()).collect();
            group.state = "documentary_proposal";
        }
    }
    structure
}

#[derive(Debug, Serialize)]
pub(crate) struct OccurrenceObservations {
    pub feature_id: String,
    pub observed_occurrence_ids: Vec<String>,
    pub expected_count: u32,
    pub conformity: ConformityStatus,
    pub decision_blocker: Option<&'static str>,
    pub ambiguous_occurrence_ids: Vec<String>,
}

/// Consume the live, reassessed inspection snapshot.
pub(crate) fn occurrence_observations(
    workspace: &crate::dper_inspection::InspectionWorkspace,
) -> Vec<OccurrenceObservations> {
    let Some(plan) = &workspace.plan else { return Vec::new(); };
    plan.characteristics.iter().filter_map(|feature| {
        let count = declared_repetition(feature).filter(|count| *count != 1)?;
        let mut summary = OccurrenceObservations {
            feature_id: feature.id.clone(),
            expected_count: count,
            observed_occurrence_ids: Vec::new(),
            ambiguous_occurrence_ids: Vec::new(),
            conformity: ConformityStatus::Indeterminate,
            decision_blocker: Some("assignment_unavailable"),
        };
        let mut assignments = workspace.occurrence_assignments.iter()
            .filter(|assignment| assignment.feature_id == feature.id);
        let Some(assignment) = assignments.next().filter(|assignment|
            assignment.selected_in_session && assignment.members.len() == count as usize
                && assignment.members.iter().map(|member| &member.id).collect::<BTreeSet<_>>().len() == count as usize
                && assignments.next().is_none()) else { return Some(summary); };
        let mut records_by_member = BTreeMap::<_, Vec<_>>::new();
        for record in &workspace.records {
            if record.feature_id == feature.id && !record.imported
                && record.outcome.geometry_state == GeometryState::Validated {
                if let (Some(proof), Some(id)) = (&record.occurrence, record.measurement_id) {
                    if proof.view_id == assignment.view_id {
                        records_by_member.entry(&proof.member.id).or_default().push((record, id));
                    }
                }
            }
        }
        let mut measurement_ids = BTreeSet::new();
        let mut control_groups = BTreeSet::new();
        let mut all_decided = true;
        let mut any_nonconforming = false;
        for member in &assignment.members {
            let Some(records) = records_by_member.get(&member.id) else { continue; };
            summary.observed_occurrence_ids.push(member.id.clone());
            match records.as_slice() {
                [(record, id)] if measurement_ids.insert(*id) => {
                    control_groups.insert((&record.capture_id, &record.group));
                    match record.outcome.conformity_status {
                        ConformityStatus::Conforming => {}
                        ConformityStatus::Nonconforming => any_nonconforming = true,
                        _ => all_decided = false,
                    }
                }
                _ => summary.ambiguous_occurrence_ids.push(member.id.clone()),
            }
        }
        summary.decision_blocker = if !summary.ambiguous_occurrence_ids.is_empty() {
            Some("ambiguous_observations")
        } else if summary.observed_occurrence_ids.len() != count as usize {
            Some("incomplete_observations")
        } else if control_groups.len() != 1 {
            Some("mixed_observation_groups")
        } else if !all_decided {
            Some("member_decision_indeterminate")
        } else {
            summary.conformity = if any_nonconforming { ConformityStatus::Nonconforming }
                else { ConformityStatus::Conforming };
            None
        };
        Some(summary)
    }).collect()
}

impl OccurrenceObservations {
    pub(crate) fn decision_explanation(&self) -> &'static str {
        match self.decision_blocker {
            Some("assignment_unavailable") => "affectation courante indisponible",
            Some("ambiguous_observations") => "plusieurs observations courantes pour un même emplacement ou identité de mesure réutilisée",
            Some("incomplete_observations") => "contours courants manquants",
            Some("mixed_observation_groups") => "les observations appartiennent à des groupes de contrôle différents",
            Some("member_decision_indeterminate") => "décision indéterminée pour au moins une occurrence",
            _ => "décision calculée à partir des occurrences individuelles",
        }
    }
}

pub(crate) fn summary_lines(
    plan: &DrawingControlPlan,
    assignments: &[crate::dper_binding::OccurrenceAssignment],
) -> Vec<String> {
    let structure = structure(plan);
    let mut lines = Vec::new();
    for group in structure.groups {
        lines.push(format!("{} : {} occurrence(s) déclarée(s). Source : {}",
            group.declaration_id, group.expected_count, group.source.exact_text));
        for assignment in assignments.iter().filter(|item| item.feature_id == group.declaration_id) {
            lines.push(format!("Affectation documentaire enregistrée · page {} · vue {} · justification : {}",
                assignment.page, assignment.view_id, assignment.justification));
            for member in &assignment.members {
                lines.push(format!("Occurrence {} · objet PDF {}", member.number, member.geometry_id));
            }
        }
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
    for declaration in structure.datum_declarations {
        let source = declaration.source;
        lines.push(format!("Repère déclaré {} · page {} · {} · {} · ({:.2}, {:.2})–({:.2}, {:.2}) pt PDF · mots {}",
            declaration.label, source.page, source.source_line_id, source.source_text,
            source.bbox.x_min, source.bbox.y_min, source.bbox.x_max, source.bbox.y_max, source.word_ids.join(", ")));
    }
    for reference in structure.datum_references {
        let target = reference.candidate_source_line_id.map_or_else(String::new, |id| format!(" · source {id}"));
        lines.push(format!("{} → repère {}{target} · {}",
            reference.dependent_id, reference.label, reference.justification));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dper_drawing::{parse_text_plan, DrawingRelation};

    fn datum_plan(pages: &[&[&str]]) -> DrawingControlPlan {
        let mut xml = String::from("<html><body><doc>");
        for lines in pages {
            xml.push_str("<page width=\"400\" height=\"320\"><flow><block xMin=\"10\" yMin=\"10\" xMax=\"350\" yMax=\"300\">");
            for (index, line) in lines.iter().enumerate() {
                let y = 10 + 22 * index;
                xml.push_str(&format!("<line xMin=\"10\" yMin=\"{y}\" xMax=\"350\" yMax=\"{}\">", y + 10));
                for (word_index, word) in line.split_whitespace().enumerate() {
                    let x = 10 + 35 * word_index;
                    xml.push_str(&format!("<word xMin=\"{x}\" yMin=\"{y}\" xMax=\"{}\" yMax=\"{}\">{word}</word>", x + 25, y + 10));
                }
                xml.push_str("</line>");
            }
            xml.push_str("</block></flow></page>");
        }
        xml.push_str("</doc></body></html>");
        crate::dper_drawing::from_pdf_layout(crate::dper_pdf_layout::parse_bbox_layout(&xml).unwrap(), "reperes.pdf")
    }

    #[test]
    fn dpm4_datums_trace_a_unique_candidate_and_keep_the_control_suspended() {
        let mut plan = datum_plan(&[&["⌖ | Ø0,1 mm | A | B", "DATUM A", "B"]]);
        let feature = &mut plan.characteristics[0];
        feature.confirmed = true;
        assert_eq!(suspension_reason(feature), Some("drawing_relation_unresolved"));
        let before = serde_json::to_value(structure(&plan)).unwrap();
        assert_eq!(before["datum_references"][0]["state"], "candidate_view_unresolved");
        assert_eq!(before["datum_references"][1]["state"], "target_missing");
        assert_eq!(before["datum_references"][0]["candidate_source_line_id"], before["datum_declarations"][0]["source"]["source_line_id"]);
        assert_eq!(before["datum_references"][0]["dependent_source"]["page"], 1);
        assert_eq!(before["datum_declarations"][0]["source"]["word_ids"].as_array().unwrap().len(), 2);
        assert!(summary_lines(&plan, &[]).iter().any(|line| line.contains("Repère déclaré A · page 1") && line.contains("pt PDF")));
        let reopened = serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
        assert_eq!(before, serde_json::to_value(structure(&reopened)).unwrap());
    }

    #[test]
    fn dpm4_datums_abstain_on_duplicates_and_declarations_in_another_page() {
        let plan = datum_plan(&[&["⌖ | Ø0,1 mm | A | B", "DATUM A"], &["REPÈRE A", "DATUM B"]]);
        let result = structure(&plan);
        assert_eq!(result.datum_declarations.len(), 3);
        assert_eq!(result.datum_references.iter().map(|item| item.state).collect::<Vec<_>>(),
            ["target_ambiguous", "reference_unresolved"]);
        assert!(result.datum_references.iter().all(|item| item.candidate_source_line_id.is_none()));
    }

    #[test]
    fn dpm4_datums_leave_ambiguous_or_unsupported_frame_text_unresolved() {
        for line in ["⌖ Ø0,1 mm A", "⌖ | Ø0,1 mm | A | A", "⌖ | Ø0,1 mm | A-B", "⌖ | Ø0,1 mm | A | B | C | D",
            "⏥ | 0,1 mm | A", "⌖ | Ø0,1 mm 2 mm | A"] {
            let plan = datum_plan(&[&[line, "DATUM A"]]);
            assert!(structure(&plan).datum_references.is_empty(), "{line}");
            assert_eq!(suspension_reason(&plan.characteristics[0]), Some("drawing_relation_unresolved"), "{line}");
        }
    }

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

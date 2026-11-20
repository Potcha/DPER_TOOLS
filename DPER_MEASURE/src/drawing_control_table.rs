use crate::dper_drawing::{DrawingDimensionKind, DrawingMeasurability, DrawingUnit};
use crate::dper_inspection::InspectionWorkspace;

#[derive(Debug)]
pub(crate) struct ControlRow<'a> {
    pub id: String,
    pub label: &'a str,
    pub reference: String,
    pub kind: &'static str,
    pub nominal: Option<f64>,
    pub unit: &'static str,
    pub tolerance: Option<&'a str>,
    pub occurrence: Option<(u32, u32)>,
    pub status: &'static str,
    pub reason: String,
    pub target: Option<crate::manual_control::ManualControlTarget>,
}

pub(crate) fn rows(workspace: &InspectionWorkspace) -> Vec<ControlRow<'_>> {
    let Some(plan) = &workspace.plan else { return Vec::new() };
    let progress = crate::inspection_progress::summarize(workspace);
    let observations = crate::drawing_structure::occurrence_observations(workspace);
    let references = crate::drawing_structure::feature_reference_labels(plan);
    let mut rows = Vec::new();

    for (feature, item) in plan.characteristics.iter().zip(&progress) {
        let excluded = feature.measurability == DrawingMeasurability::NotMeasurable2d;
        let repeated = observations.iter().find(|group| group.feature_id == feature.id);
        let count = if (1..=crate::dper_binding::MAX_OCCURRENCES).contains(&item.expected_count) {
            item.expected_count
        } else {
            1
        };
        for number in 1..=count {
            let occurrence = (count > 1).then_some((number, count));
            let id = if occurrence.is_some() {
                format!("repetition:{}:{number}", feature.id)
            } else {
                feature.id.clone()
            };
            let inaccessible = workspace.records.iter().filter(|record| {
                !record.imported && record.feature_id == feature.id
                    && record.occurrence.as_ref().map(|proof| proof.member.id.as_str())
                        == occurrence.map(|_| id.as_str())
            }).flat_map(|record| &record.outcome.blocking_reasons).find(|reason| {
                matches!(reason.as_str(), "not_observable_in_2d" | "contour_coverage_failed"
                    | "insufficient_observed_samples" | "contour_fit_failed")
            });
            let target = crate::manual_control::ManualControlTarget::identify(plan, feature, occurrence.map(|(number, _)| number));
            let manually_measured = target.as_ref().is_some_and(|target| {
                workspace.manual_control_links.iter().any(|link| &link.target == target)
            });
            let (status, reason) = if excluded {
                ("Exclue", feature.reasons.first().map(String::as_str).unwrap_or(item.next_action.instruction))
            } else if manually_measured {
                ("Mesurée", "")
            } else if !feature.confirmed || feature.has_unresolved_unit_context()
                || feature.measurability != DrawingMeasurability::PlanarCandidate
                || repeated.is_some_and(|group| group.decision_blocker == Some("assignment_unavailable")
                    || group.ambiguous_occurrence_ids.contains(&id))
            {
                ("À confirmer", item.next_action.instruction)
            } else if repeated.map_or(item.current_observations == 1, |group| group.observed_occurrence_ids.contains(&id)) {
                ("Mesurée", "")
            } else if item.current_observations > 1 && repeated.is_none() {
                ("À confirmer", item.next_action.instruction)
            } else if let Some(reason) = inaccessible {
                ("Inaccessible", crate::dper_metrology::reason_label(reason))
            } else {
                ("À mesurer", item.next_action.instruction)
            };
            rows.push(ControlRow {
                id,
                label: &feature.label,
                reference: references.get(feature.id.as_str()).map_or_else(String::new, |labels| labels.join(", ")),
                kind: match feature.kind {
                    DrawingDimensionKind::Linear => "Longueur",
                    DrawingDimensionKind::Diameter => "Diamètre",
                    DrawingDimensionKind::Radius => "Rayon",
                    DrawingDimensionKind::Angle => "Angle",
                    DrawingDimensionKind::Depth => "Profondeur",
                    DrawingDimensionKind::Thread => "Filetage",
                    DrawingDimensionKind::Geometric => "Géométrique",
                    DrawingDimensionKind::Unknown => "À identifier",
                },
                nominal: feature.nominal,
                unit: match feature.unit {
                    Some(DrawingUnit::Millimeter) => "mm",
                    Some(DrawingUnit::Centimeter) => "cm",
                    Some(DrawingUnit::Meter) => "m",
                    Some(DrawingUnit::Inch) => "in",
                    Some(DrawingUnit::Degree) => "°",
                    None => "",
                },
                tolerance: feature.tolerance.as_ref().map(|tolerance| tolerance.source_text.as_str()),
                occurrence,
                status,
                reason: reason.to_owned(),
                target,
            });
        }
    }
    rows
}

pub(crate) fn show(ui: &mut egui::Ui, workspace: &InspectionWorkspace, active: Option<&str>)
    -> Option<crate::manual_control::ManualControlTarget>
{
    workspace.plan.as_ref()?;
    let mut selected = None;
    let rows = rows(workspace);
    let controls = rows.iter().filter(|row| row.status != "Exclue").count();
    egui::CollapsingHeader::new(format!("Tableau de contrôle · {controls} à contrôler · {} lignes", rows.len()))
        .id_salt("drawing-control-table-header")
        .default_open(true)
        .show(ui, |ui| {
        egui::ScrollArea::horizontal()
            .id_salt("drawing-control-table-scroll")
            .show(ui, |ui| {
                egui::Grid::new("drawing-control-table")
                    .striped(true)
                    .show(ui, |ui| {
                        for label in ["Libellé", "Statut", "Repère", "Type", "Nominal", "Unité", "Tolérance", "Occurrence"] {
                            ui.strong(label);
                        }
                        ui.end_row();
                        for row in rows {
                            ui.push_id(&row.id, |ui| {
                                if let Some(target) = row.target {
                                    if ui.selectable_label(active == Some(row.id.as_str()), row.label).clicked() {
                                        selected = Some(target);
                                    }
                                } else {
                                    ui.label(row.label);
                                }
                                let status = ui.label(row.status);
                                if !row.reason.is_empty() {
                                    status.on_hover_text(&row.reason);
                                }
                                ui.label(&row.reference);
                                ui.label(row.kind);
                                ui.label(row.nominal.map_or_else(String::new, |value| value.to_string().replace('.', ",")));
                                ui.label(row.unit);
                                ui.label(row.tolerance.unwrap_or_default());
                                ui.label(row.occurrence.map_or_else(String::new, |(number, total)| format!("{number}/{total}")));
                            });
                            ui.end_row();
                        }
                    });
            });
    });
    selected
}

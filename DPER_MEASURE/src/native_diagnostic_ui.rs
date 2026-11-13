//! Native review of diagnostic previews and observation uncertainty budgets.

use crate::{
    dper_inspection::InspectionRecord,
    geometry::Tool,
    mcp_server::{ApplyObservationUncertaintyArgs, InspectNativeDiagnosticArgs},
    native_diagnostic::NativeDiagnosticRequest,
    observation_uncertainty::ObservationUncertaintyInput,
    DperMeasureApp,
};
use eframe::egui;
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct PanelState {
    measurement_id: Option<u64>,
    draft_measurement_id: Option<u64>,
    budget_json: String,
    status: String,
}

fn diagnostic_title(request: &NativeDiagnosticRequest) -> &'static str {
    match request {
        NativeDiagnosticRequest::StraightEdgeEndpoints { .. } => "Extrémités visibles d'un bord droit",
        NativeDiagnosticRequest::ArcEndpoints { .. } => "Extrémités visibles d'un arc",
        NativeDiagnosticRequest::LineIntersection { .. } => "Intersection construite de supports",
    }
}

/// Unknown components stay null: opening the editor supplies no measurement
/// uncertainty or assumed qualification. The operator must document them.
fn budget_template(tool: Tool) -> Option<Value> {
    let point = || json!({"standard_uncertainty_x_px": null, "standard_uncertainty_y_px": null, "source": ""});
    let scale = || json!({"relative_standard_uncertainty": null, "source": ""});
    let (mut value, count) = match tool {
        Tool::Distance => (json!({"model": "distance", "points": [point(), point()], "scale": scale()}), 5),
        Tool::Angle => (json!({"model": "angle", "points": [point(), point(), point()]}), 6),
        Tool::Circle | Tool::CircleFit => (json!({"model": "diameter", "standard_uncertainty_radius_px": null,
            "radius_source": "", "scale": scale()}), 2),
        Tool::Arc => (json!({"model": "arc_length", "standard_uncertainty_radius_px": null, "radius_source": "",
            "standard_uncertainty_sweep_deg": null, "sweep_source": "", "scale": scale()}), 3),
        _ => return None,
    };
    let matrix: Vec<Vec<Value>> = (0..count).map(|row| (0..count).map(|column|
        if row == column { json!(1.0) } else { Value::Null }).collect()).collect();
    value["propagation"] = json!({"coverage_factor": null, "correlation_matrix": matrix, "correlation_justification": ""});
    Some(value)
}

fn json_details(ui: &mut egui::Ui, title: &str, value: &Value) {
    ui.collapsing(title, |ui| {
        let mut text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "Preuve illisible".into());
        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY).desired_rows(8).interactive(false));
        });
    });
}

impl DperMeasureApp {
    fn diagnostic_preview_status(&self, record: &InspectionRecord) -> Result<(), String> {
        let proof = record.diagnostic.as_ref().ok_or("Preuve diagnostique absente.")?;
        if record.imported || record.measurement_id.is_some() {
            return Err("Une nouvelle observation est requise pour cet historique.".into());
        }
        if proof.preview_revision != self.document_revision {
            return Err("Le document a changé depuis l'aperçu ; recalculez-le.".into());
        }
        self.inspection_guard(&record.capture_id, self.document_revision)?;
        let image = self.inspection_image()?;
        let frame = self.inspection_frame();
        if !frame.quality_pass { return Err("La qualité courante de l'image est insuffisante.".into()); }
        let current = crate::clearance_inspection::input_context(&self.inspection, &image, &frame, &self.measurements, &proof.sources)?;
        if proof.context.get("inputs") != Some(&current) {
            return Err("Pixels, étalonnage ou sources modifiés ; recalculez l'aperçu.".into());
        }
        Ok(())
    }

    pub(crate) fn integrated_observation_panel(&mut self, ui: &mut egui::Ui) {
        let state_id = ui.id().with("integrated_native_observation_state");
        let mut state = ui.ctx().data_mut(|data| data.get_temp::<PanelState>(state_id)).unwrap_or_default();
        let pending: Vec<_> = self.inspection.records.iter().rev().filter(|record|
            record.diagnostic.is_some() && record.measurement_id.is_none()).take(8).cloned().collect();
        let mut diagnostic_action = None;
        ui.collapsing("Aperçus des extrémités et intersections", |ui| {
            if pending.is_empty() { ui.label("Aucun aperçu diagnostique en attente d'enregistrement."); }
            for (index, record) in pending.iter().enumerate() {
                ui.push_id(("diagnostic_preview", index), |ui| {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        let proof = record.diagnostic.as_ref().unwrap();
                        ui.strong(diagnostic_title(&proof.request));
                        ui.label(format!("{} · {} · capture {}", record.group, record.feature_id, record.capture_id));
                        let sources = proof.sources.iter().map(|id| format!("M{id}")).collect::<Vec<_>>().join(", ");
                        ui.label(format!("Sources : {}", if sources.is_empty() { "aucune" } else { &sources }));
                        let status = self.diagnostic_preview_status(record);
                        match &status {
                            Ok(()) => { ui.label("Contexte et sources courants ; aucune mesure créée par cet aperçu."); }
                            Err(reason) => { ui.label(reason); }
                        }
                        ui.label("Portée locale visible ; occultation, incertitude physique et conformité non établies.");
                        json_details(ui, "Supports, intersections et profils conservés", &proof.result);
                        ui.horizontal(|ui| {
                            if ui.add_enabled(status.is_ok(), egui::Button::new("Enregistrer")).clicked() {
                                diagnostic_action = Some((record.clone(), true));
                            }
                            if ui.button("Recalculer l'aperçu").clicked() {
                                diagnostic_action = Some((record.clone(), false));
                            }
                        });
                    });
                });
            }
        });
        if let Some((record, commit)) = diagnostic_action {
            let proof = record.diagnostic.as_ref().unwrap();
            state.status = match self.perform_native_diagnostic(InspectNativeDiagnosticArgs {
                capture_id: record.capture_id.clone(), expected_revision: self.document_revision,
                feature_id: record.feature_id.clone(), group: record.group.clone(), request: proof.request.clone(),
                source_measurement_ids: proof.sources.clone(), commit,
            }) {
                Ok(_) => if commit { "Observation native enregistrée ; conformité physique indéterminée.".into() }
                    else { "Nouvel aperçu calculé dans le contexte courant.".into() },
                Err(error) => error,
            };
        }

        let mut current = self.current_inspection();
        // validate_current reads the same live source-backed catalogue context
        // as the application operation, never a persisted catalogue claim.
        current.current_drawing_catalogs = if self.current_drawing_source().is_ok() {
            self.drawing_geometry_cache.clone()
        } else { Vec::new() };
        let recorded: Vec<_> = current.records.iter().filter(|record| record.measurement_id.is_some()).collect();
        if state.measurement_id.is_none_or(|id| !recorded.iter().any(|record| record.measurement_id == Some(id))) {
            state.measurement_id = self.selected_measurement.filter(|id|
                recorded.iter().any(|record| record.measurement_id == Some(*id)))
                .or_else(|| recorded.last().and_then(|record| record.measurement_id));
        }
        let mut apply = None;
        ui.collapsing("Budget d'incertitude de l'observation", |ui| {
            if recorded.is_empty() { ui.label("Enregistrez une observation native pour documenter son budget."); return; }
            egui::ComboBox::from_id_salt("native_budget_measurement")
                .selected_text(state.measurement_id.map(|id| format!("M{id}")).unwrap_or_else(|| "Observation".into()))
                .show_ui(ui, |ui| {
                    for record in &recorded {
                        let id = record.measurement_id.unwrap();
                        ui.selectable_value(&mut state.measurement_id, Some(id), format!("M{id} · {} · {}", record.group, record.feature_id));
                    }
                });
            let Some(record) = recorded.iter().find(|record| record.measurement_id == state.measurement_id).copied() else { return; };
            let Some(measurement) = self.measurements.iter().find(|measurement| Some(measurement.id) == state.measurement_id) else {
                ui.label("La mesure native a été supprimée."); return;
            };
            ui.label(format!("Observation : {} · conformité : {}", record.outcome.geometry_state.label(), record.outcome.conformity_status.label()));
            if let Some(proof) = &record.observation_uncertainty {
                let status = self.inspection_image().and_then(|image|
                    crate::observation_uncertainty_inspection::validate_current(record, &current, &image,
                        &self.inspection_frame(), &self.measurements));
                match status {
                    Ok(()) => { ui.label("Budget courant : calcul rejoué sur l'observation et les sources actuelles."); }
                    Err(error) => { ui.label(format!("Budget suspendu : {error}")); }
                }
                if let Some(value) = proof.calculation.get("expanded_uncertainty") {
                    ui.label(format!("Incertitude élargie calculée : {value} · unité {}", proof.calculation["output_unit"]));
                }
                json_details(ui, "Calcul conservé et limites", &proof.calculation);
            } else { ui.label("Aucun budget attaché à cette observation."); }
            if state.draft_measurement_id != Some(measurement.id) {
                state.draft_measurement_id = Some(measurement.id);
                state.budget_json = record.observation_uncertainty.as_ref()
                    .and_then(|proof| serde_json::to_string_pretty(&proof.input).ok())
                    .or_else(|| budget_template(measurement.kind).and_then(|value| serde_json::to_string_pretty(&value).ok()))
                    .unwrap_or_default();
            }
            if measurement.is_construction || record.clearance.is_some() || budget_template(measurement.kind).is_none() {
                ui.label("Cette construction ou géométrie dérivée ne reçoit pas de budget d'observation numérique."); return;
            }
            ui.label("Renseignez chaque null, les sources, le facteur de couverture et la justification des corrélations. Les coordonnées et l'échelle proviennent de l'observation ; aucun résidu ne devient automatiquement une incertitude.");
            ui.label("Ordre des corrélations : distance x1,y1,x2,y2,échelle relative ; angle x1,y1,xSommet,ySommet,x2,y2 ; diamètre rayon,échelle ; arc rayon,ouverture,échelle.");
            egui::ScrollArea::vertical().id_salt("native_budget_editor_scroll").max_height(300.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut state.budget_json).font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY).desired_rows(12));
            });
            if ui.button("Calculer et attacher le budget").clicked() {
                apply = Some((measurement.id, serde_json::from_str::<ObservationUncertaintyInput>(&state.budget_json)));
            }
            ui.label("Calcul déclaré par l'opérateur : qualification des sources non vérifiée, incertitude totale non attestée, conformité non attribuée.");
        });
        if let Some((measurement_id, input)) = apply {
            state.status = match input {
                Ok(input) => match self.apply_observation_uncertainty(ApplyObservationUncertaintyArgs {
                    capture_id: self.capture_id.clone(), expected_revision: self.document_revision, measurement_id, input,
                }) {
                    Ok(_) => "Budget calculé et attaché ; qualification physique et conformité restent à établir.".into(),
                    Err(error) => error,
                },
                Err(error) => format!("Paramètres incomplets ou invalides : {error}"),
            };
        }
        if !state.status.is_empty() { ui.label(&state.status); }
        ui.ctx().data_mut(|data| data.insert_temp(state_id, state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_budget_editor_never_supplies_unknown_uncertainties_or_correlations() {
        for tool in [Tool::Distance, Tool::Angle, Tool::Circle, Tool::CircleFit, Tool::Arc] {
            let template = budget_template(tool).unwrap();
            assert!(serde_json::from_value::<ObservationUncertaintyInput>(template.clone()).is_err());
            assert!(template["propagation"]["coverage_factor"].is_null());
            let matrix = template["propagation"]["correlation_matrix"].as_array().unwrap();
            for (row, values) in matrix.iter().enumerate() {
                for (column, value) in values.as_array().unwrap().iter().enumerate() {
                    if row == column { assert_eq!(value, &json!(1.0)); }
                    else { assert!(value.is_null()); }
                }
            }
        }
        assert!(budget_template(Tool::Coordinate).is_none());
    }
}

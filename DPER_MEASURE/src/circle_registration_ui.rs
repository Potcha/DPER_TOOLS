//! Review the same source-backed correspondence alternatives used by MCP.
use crate::{
    mcp_server::{ProposeImageCircleLandmarkMatchesArgs, RegisterImageCircleLandmarksArgs},
    DperMeasureApp,
};
use eframe::egui;
use serde_json::Value;

pub(crate) struct CircleRegistrationUi {
    view_id: String,
    circle_ids: Vec<String>,
    threshold_px: f64,
    status: String,
    minimum_supports: usize,
    minimum_coverage_percent: f64,
    view_comparison: Option<Result<Value, String>>,
    pub proposal: Option<CircleRegistrationProposal>,
}

impl Default for CircleRegistrationUi {
    fn default() -> Self {
        Self { view_id: String::new(), circle_ids: Vec::new(), threshold_px: 2.0,
            status: String::new(), proposal: None, minimum_supports: 3,
            minimum_coverage_percent: 5.0, view_comparison: None }
    }
}

pub(crate) struct CircleRegistrationProposal {
    pub search: ProposeImageCircleLandmarkMatchesArgs,
    pub result: Value,
    pub selected_candidate: Option<String>,
    pub justification: String,
}

impl CircleRegistrationUi {
    pub(crate) fn set_proposal(&mut self, search: ProposeImageCircleLandmarkMatchesArgs, result: Value) {
        self.view_id = search.view_id.clone();
        self.circle_ids = search.plan_circle_ids.clone();
        self.threshold_px = search.threshold_px;
        self.proposal = Some(CircleRegistrationProposal {
            search, result, selected_candidate: None, justification: String::new(),
        });
    }
}

impl DperMeasureApp {
    pub(crate) fn apply_circle_registration_choice(&mut self) -> Result<(), String> {
        let state = &self.registration_ui.circle_matches;
        let proposal = state.proposal.as_ref()
            .ok_or("Recherchez les hypothèses de recalage.")?;
        if state.view_id != proposal.search.view_id || state.circle_ids != proposal.search.plan_circle_ids
            || state.threshold_px != proposal.search.threshold_px
        {
            return Err("Les paramètres ont changé. Relancez la recherche avant de choisir.".into());
        }
        let candidate = proposal.selected_candidate.as_ref()
            .ok_or("Choisissez explicitement une hypothèse après examen des repères.")?;
        let fingerprint = proposal.result["candidate_fingerprints"].as_array()
            .and_then(|items| items.iter().find(|item| item["candidate_id"] == *candidate))
            .and_then(|item| item["proposal_fingerprint"].as_str())
            .ok_or("Empreinte du candidat indisponible. Relancez la recherche.")?;
        let args = RegisterImageCircleLandmarksArgs {
            search: proposal.search.clone(), proposal_fingerprint: fingerprint.to_owned(),
            candidate_id: Some(candidate.clone()), justification: Some(proposal.justification.clone()),
        };
        self.register_observed_circle_landmarks(args)?;
        Ok(())
    }

    pub(crate) fn circle_registration_panel(&mut self, ui: &mut egui::Ui) {
        let mut state = std::mem::take(&mut self.registration_ui.circle_matches);
        let mut compare = false;
        let mut manual = false;
        let mut another_image = false;
        let mut search = false;
        let mut apply = false;
        egui::CollapsingHeader::new("Hypothèses des repères circulaires").show(ui, |ui| {

            ui.collapsing("Comparer les vues par repères circulaires", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Appuis minimum");
                    if ui.add(egui::DragValue::new(&mut state.minimum_supports).range(3..=6)).changed() {
                        state.view_comparison = None;
                    }
                    ui.label("Couverture minimale des centres (%)");
                    if ui.add(egui::DragValue::new(&mut state.minimum_coverage_percent).range(0.0..=100.0)).changed() {
                        state.view_comparison = None;
                    }
                });
                compare = ui.button("Comparer les vues").clicked();
                match &state.view_comparison {
                    Some(Ok(report)) if report["capture_id"] == self.capture_id
                        && report["document_revision"] == self.document_revision
                        && report["threshold_px"] == state.threshold_px =>
                    {
                        let proposed_id = report["proposed_view_id"].as_str();
                        if let Some(proposed) = self.inspection.views.iter().find(|view| Some(view.id.as_str()) == proposed_id) {
                            ui.label(format!("Vue proposée : {}", proposed.label));
                        } else if let Some(reason) = report["refusal"]["message"].as_str() {
                            ui.label(reason);
                            (manual, another_image) = view_recovery_actions(ui);
                        }
                        ui.label(format!("{} appui(s), couverture {:.1} %",
                            report["support_count"], report["coverage"].as_f64().unwrap_or_default() * 100.0));
                        for entry in report["views"].as_array().into_iter().flatten() {
                            let id = entry["view_id"].as_str().unwrap_or_default();
                            let label = self.inspection.views.iter().find(|view| view.id == id)
                                .map_or(id, |view| view.label.as_str());
                            let hypotheses = entry["hypotheses"].as_array().map(Vec::as_slice).unwrap_or_default();
                            ui.collapsing(format!("{label} · {} hypothèse(s)", hypotheses.len()), |ui| {
                                for hypothesis in hypotheses {
                                    ui.label(format!("{} · résidu RMS {:.3} px",
                                        hypothesis["candidate_id"].as_str().unwrap_or_default(),
                                        hypothesis["registration"]["rms_error_px"].as_f64().unwrap_or(f64::NAN)));
                                }
                            });
                        }
                    }
                    Some(Ok(_)) => { ui.label("Comparaison périmée. Relancez la recherche."); }
                    Some(Err(error)) => {
                        ui.label(error);
                        (manual, another_image) = view_recovery_actions(ui);
                    }
                    None => {}
                }
            });
            let selected = self.inspection.views.iter().find(|view| view.id == state.view_id)
                .map(|view| view.label.as_str()).unwrap_or("Choisir une vue PDF");
            egui::ComboBox::from_id_salt("circle_registration_view").selected_text(selected).show_ui(ui, |ui| {
                for view in &self.inspection.views {
                    if ui.selectable_value(&mut state.view_id, view.id.clone(), &view.label).changed() {
                        state.circle_ids.clear();
                    }
                }
            });
            if let Some(view) = self.inspection.views.iter().find(|view| view.id == state.view_id) {
                if let Some(catalog) = self.drawing_geometry_cache.iter().find(|catalog| catalog.geometry.page == view.page) {
                    for primitive in &catalog.geometry.primitives {
                        if view.geometry_ids.contains(&primitive.id)
                            && !view.auxiliary_roles.contains_key(&primitive.id)
                            && crate::dper_binding::circle_geometry(primitive).is_ok()
                        {
                            let mut selected = state.circle_ids.contains(&primitive.id);
                            if ui.checkbox(&mut selected, &primitive.id).changed() {
                                state.circle_ids.retain(|id| id != &primitive.id);
                                if selected { state.circle_ids.push(primitive.id.clone()); }
                            }
                        }
                    }
                }
            }
            ui.horizontal(|ui| {
                ui.label("Seuil de résidu (px)");
                ui.add(egui::DragValue::new(&mut state.threshold_px).range(0.01..=32.0));
            });
            search = ui.add_enabled((3..=6).contains(&state.circle_ids.len()),
                egui::Button::new("Rechercher les correspondances")).clicked();
            if let Some(proposal) = &mut state.proposal {
                let stale = proposal.search.capture_id != self.capture_id
                    || proposal.search.expected_revision != self.document_revision
                    || state.view_id != proposal.search.view_id
                    || state.circle_ids != proposal.search.plan_circle_ids
                    || state.threshold_px != proposal.search.threshold_px;
                ui.label(format!("Vue {} · seuil {} px · révision {}",
                    proposal.search.view_id, proposal.search.threshold_px, proposal.search.expected_revision));
                if stale { ui.label("Proposition périmée. Relancez la recherche avant application."); }
                if let Some(hypotheses) = proposal.result["proposal"]["matching"]["hypotheses"].as_array() {
                    ui.label(format!("{} hypothèse(s) à examiner", hypotheses.len()));
                    for hypothesis in hypotheses {
                        let id = hypothesis["candidate_id"].as_str().unwrap_or_default();
                        ui.radio_value(&mut proposal.selected_candidate, Some(id.to_owned()), id);
                        if let Some(mappings) = hypothesis["mappings"].as_array() {
                            for mapping in mappings {
                                ui.label(format!("{} → {} · résidu {:.3} px",
                                    mapping["plan_id"].as_str().unwrap_or_default(),
                                    mapping["image_id"].as_str().unwrap_or_default(),
                                    mapping["residual_px"].as_f64().unwrap_or(f64::NAN)));
                            }
                        }
                    }
                }
                ui.collapsing("Coordonnées et preuves des contours", |ui| {
                    ui.label(serde_json::to_string_pretty(&proposal.result).unwrap_or_default());
                });
                ui.label("Justification du choix");
                ui.add(egui::TextEdit::multiline(&mut proposal.justification).char_limit(1000)
                    .hint_text("Décrire le repère qui départage les correspondances"));
                apply = ui.add_enabled(!stale && proposal.selected_candidate.is_some(),
                    egui::Button::new("Appliquer l’hypothèse choisie")).clicked();
            }
            if !state.status.is_empty() { ui.label(&state.status); }
            if !self.inspection.registration_choices.is_empty() {
                ui.collapsing("Historique des choix de recalage", |ui| {
                    for choice in &self.inspection.registration_choices {
                        ui.label(format!("{} · {}", choice.candidate_id, choice.justification));
                        ui.weak(format!("Empreinte {}", choice.proposal_fingerprint));
                    }
                });
            }
        });
        self.registration_ui.circle_matches = state;
        if manual {
            self.registration_ui.circle_matches.view_comparison = None;
            self.active_ribbon_tab = crate::RibbonTab::Measurements;
            self.right_panel_annotations = false;
            self.workflow_open_reference_panel = false;
            self.choose_measurement_tool(crate::Tool::Distance);
        }
        if another_image {
            self.open_image(ui.ctx());
        }
        if compare {
            let state = &self.registration_ui.circle_matches;
            let comparison = self.recognize_active_circle_view(
                state.minimum_supports, state.minimum_coverage_percent / 100.0, state.threshold_px,
            );
            self.registration_ui.circle_matches.view_comparison = Some(comparison);
        }
        if search {
            let state = &self.registration_ui.circle_matches;
            let args = ProposeImageCircleLandmarkMatchesArgs {
                capture_id: self.capture_id.clone(), expected_revision: self.document_revision,
                view_id: state.view_id.clone(), plan_circle_ids: state.circle_ids.clone(),
                threshold_px: state.threshold_px,
            };
            let result = self.propose_observed_circle_matches(args.into(), true);
            self.registration_ui.circle_matches.status = match result {
                Ok(_) => "Examinez les repères et choisissez une hypothèse.".into(),
                Err(error) => error,
            };
        }
        if apply {
            self.registration_ui.circle_matches.status = match self.apply_circle_registration_choice() {
                Ok(()) => "Choix appliqué. Les alternatives sont conservées dans la session.".into(),
                Err(error) => error,
            };
        }
    }
}


fn view_recovery_actions(ui: &mut egui::Ui) -> (bool, bool) {
    ui.horizontal_wrapped(|ui| {
        let manual = ui.button("Revenir aux mesures manuelles").clicked();
        let another_image = ui.button("Ouvrir une autre image").clicked();
        (manual, another_image)
    }).inner
}

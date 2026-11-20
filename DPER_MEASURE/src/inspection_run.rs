use crate::{mcp_server::PreviewControlPlanArgs, ui_widgets, DperMeasureApp};

#[derive(Default)]
pub(super) enum InspectionRun {
    #[default]
    Idle,
    Blocked(String),
    Running(PreviewControlPlanArgs, bool),
    Completed(String),
    Cancelled,
}

impl InspectionRun {
    pub(super) fn status(&self) -> serde_json::Value {
        let (state, message) = match self {
            Self::Idle => ("idle", None),
            Self::Running(..) => ("running", None),
            Self::Blocked(message) => ("blocked", Some(message)),
            Self::Completed(message) => ("completed", Some(message)),
            Self::Cancelled => ("cancelled", None),
        };
        serde_json::json!({"state": state, "message": message})
    }
}

impl DperMeasureApp {
    pub(super) fn start_inspection_run(&mut self) -> Result<(), String> {
        if matches!(self.inspection_run, InspectionRun::Running(..)) {
            return Err("Une inspection est déjà en cours.".into());
        }
        let result = self.inspection_guard(&self.capture_id, self.document_revision).and_then(|_| {
            if self.inspection.plan.as_ref().is_none_or(|plan| plan.characteristics.is_empty()) {
                return Err("Importez un plan qui contient des cotes à contrôler.".into());
            }
            if self.inspection_observation_ui.has_draft() {
                return Err("Terminez le brouillon d’observation avant de lancer l’inspection.".into());
            }
            if !self.quality_assessment.as_ref().is_some_and(|quality| quality.is_exploitable) {
                return Err("Chargez une image exploitable ou corrigez sa qualité.".into());
            }
            Ok(())
        });
        if let Err(reason) = result {
            self.inspection_run = InspectionRun::Blocked(reason.clone());
            return Err(reason);
        }
        self.inspection_run = InspectionRun::Running(PreviewControlPlanArgs {
            capture_id: self.capture_id.clone(),
            expected_revision: self.document_revision,
            limit: 8,
            cursor: None,
        }, false);
        Ok(())
    }

    pub(super) fn advance_inspection_run(&mut self) {
        let InspectionRun::Running(args, ready) = &mut self.inspection_run else { return; };
        if !*ready {
            *ready = true;
            return;
        }
        let args = args.clone();
        self.inspection_run = match self.preview_control_plan_page(args) {
            Ok(page) => {
                let attempted = page["attempted"].as_u64().unwrap_or_default();
                if attempted == 0 {
                    InspectionRun::Blocked("Aucune cote prête à observer. Consultez les motifs dans le plan de contrôle.".into())
                } else if page["next_cursor"].is_string() {
                    InspectionRun::Completed(format!("{attempted} recherche(s) exécutée(s) · suite disponible dans le plan de contrôle."))
                } else {
                    InspectionRun::Completed(format!("{attempted} recherche(s) exécutée(s) · aperçus à examiner dans le plan de contrôle."))
                }
            }
            Err(reason) => InspectionRun::Blocked(reason),
        };
    }

    pub(super) fn cancel_inspection_run(&mut self) -> bool {
        if !matches!(self.inspection_run, InspectionRun::Running(..))
            && self.control_plan_previews.is_none() && self.control_plan_continuation.is_none()
        {
            return false;
        }
        self.inspection_run = InspectionRun::Cancelled;
        self.control_plan_previews = None;
        self.control_plan_continuation = None;
        self.inspection_observation_ui.cancel_draft();
        self.document_revision = self.document_revision.wrapping_add(1);
        self.inspection_panel_cache = None;
        self.message = "Inspection interrompue. Les mesures enregistrées restent disponibles.".into();
        true
    }

    pub(super) fn inspection_launch_button(&mut self, ui: &mut egui::Ui) {
        let running = matches!(self.inspection_run, InspectionRun::Running(..));
        if ui.add_enabled_ui(!running, |ui| {
            ui_widgets::button(ui, "Lancer l’automate", Some(ui_widgets::Icon::Play),
                ui_widgets::Tone::Primary, egui::vec2(ui.available_width(), 42.0))
        }).inner.clicked() {
            let _ = self.start_inspection_run();
        }
        if (running || self.control_plan_previews.is_some())
            && ui.button("Interrompre et revenir au mode manuel").clicked()
        {
            self.cancel_inspection_run();
        }
        match &self.inspection_run {
            InspectionRun::Idle => {}
            InspectionRun::Cancelled => { ui.label("Mode manuel · inspection interrompue"); }
            InspectionRun::Blocked(reason) | InspectionRun::Completed(reason) => { ui.label(reason); }
            InspectionRun::Running(..) => {
                ui.label("Inspection en cours…");
                ui.ctx().request_repaint();
            }
        }
    }
}

//! Native, context-bound review of an independently established decision basis.
use crate::{dper_inspection::{self, DecisionBasis, InspectionRecord}, DperMeasureApp};
use eframe::egui;

#[derive(Clone, PartialEq, Eq)]
struct RecordKey { measurement_id: u64, capture_id: String, feature_id: String, group: String }

impl RecordKey {
    fn of(record: &InspectionRecord) -> Option<Self> {
        Some(Self { measurement_id: record.measurement_id?, capture_id: record.capture_id.clone(),
            feature_id: record.feature_id.clone(), group: record.group.clone() })
    }
    fn label(&self) -> String {
        format!("M{} · {} · {} · capture {}", self.measurement_id, self.feature_id, self.group, self.capture_id)
    }
}

#[derive(PartialEq, Eq)]
struct DecisionContext {
    capture_id: String, revision: u64, inputs: serde_json::Value,
    source_bytes: Vec<u8>, image_fingerprint: u64,
}

struct DecisionDraft {
    key: RecordKey, context: DecisionContext, qualification_reference: String,
    qualification_scope: String, uncertainty_mm: String, uncertainty_source: String, reviewer: String,
}

impl DecisionDraft {
    fn basis(&self) -> Result<DecisionBasis, String> {
        let uncertainty = self.uncertainty_mm.trim().replace(',', ".").parse::<f64>()
            .ok().filter(|value| value.is_finite() && *value > 0.0)
            .ok_or("Entrez une incertitude élargie TOTALE finie et strictement positive, en mm.")?;
        Ok(DecisionBasis {
            qualification_reference: self.qualification_reference.clone(),
            qualification_scope: self.qualification_scope.clone(), expanded_uncertainty_mm: uncertainty,
            uncertainty_source: self.uncertainty_source.clone(), reviewer: self.reviewer.clone(),
        })
    }
}

#[derive(Default)]
pub(crate) struct InspectionDecisionUi {
    selected: Option<RecordKey>, draft: Option<DecisionDraft>, status: String,
    #[cfg(test)]
    widgets: std::collections::BTreeMap<&'static str, egui::Rect>,
}

impl InspectionDecisionUi {
    fn remember(&mut self, _key: &'static str, _response: &egui::Response) {
        #[cfg(test)]
        self.widgets.insert(_key, _response.rect);
    }
}

impl DperMeasureApp {
    fn decision_context(&self) -> Result<DecisionContext, String> {
        if !self.image_is_editable() || self.mcp_mutation_blocked() {
            return Err("Figez l'image et terminez l'action en cours avant la revue de décision.".into());
        }
        let (path, fingerprint) = self.current_drawing_source()?;
        let source_bytes = std::fs::read(path).map_err(|error| format!("Lecture du plan source : {error}"))?;
        self.current_drawing_source()?;
        if crate::dper_source::fingerprint_bytes(&source_bytes) != fingerprint {
            return Err("Le PDF a changé pendant la lecture ; préparez une nouvelle revue.".into());
        }
        let image = self.inspection_image()?;
        Ok(DecisionContext {
            capture_id: self.capture_id.clone(), revision: self.document_revision,
            inputs: serde_json::json!({
                "workspace": self.inspection, "catalogs": self.drawing_geometry_cache,
                "catalog_generation": self.drawing_geometry_generation,
                "measurements": self.measurements, "calibration": self.calibration,
                "quality": self.quality_assessment, "mm_per_pixel": self.mm_per_pixel,
            }), source_bytes, image_fingerprint: dper_inspection::fingerprint(&image),
        })
    }

    fn decision_record_index(&self, key: &RecordKey) -> Result<usize, String> {
        let matches: Vec<_> = self.inspection.records.iter().enumerate()
            .filter(|(_, record)| RecordKey::of(record).as_ref() == Some(key)).map(|(index, _)| index).collect();
        if matches.len() != 1 { return Err("Observation enregistrée absente ou ambiguë ; sélectionnez une observation unique.".into()); }
        Ok(matches[0])
    }

    fn current_decision_record(&self, key: &RecordKey) -> Result<usize, String> {
        let index = self.decision_record_index(key)?;
        let current = self.current_inspection();
        let record = current.records.get(index).ok_or("Observation absente.")?;
        if record.outcome.geometry_state != crate::dper_metrology::GeometryState::Validated || record.imported {
            return Err("Une observation actuelle, enregistrée et vérifiée par logiciel est requise. Réobservez les preuves importées ou périmées.".into());
        }
        if let Some(binding_id) = &record.drawing_binding_id { self.validate_drawing_binding(binding_id)?; }
        Ok(index)
    }

    fn prepare_native_decision(&self, state: &mut InspectionDecisionUi) -> Result<(), String> {
        let key = state.selected.clone().ok_or("Sélectionnez une observation enregistrée.")?;
        let context = self.decision_context()?;
        self.current_decision_record(&key)?;
        // Never prefill from calibration U, residuals or an imported review.
        state.draft = Some(DecisionDraft { key, context, qualification_reference: String::new(),
            qualification_scope: String::new(), uncertainty_mm: String::new(),
            uncertainty_source: String::new(), reviewer: String::new() });
        Ok(())
    }

    fn apply_native_decision(&mut self, state: &mut InspectionDecisionUi) -> Result<(), String> {
        let draft = state.draft.as_ref().ok_or("Préparez d'abord la revue de décision.")?;
        if state.selected.as_ref() != Some(&draft.key) || self.decision_context()? != draft.context {
            return Err("Revue périmée : capture, révision, pixels, PDF ou contexte modifié. Préparez une nouvelle revue explicitement.".into());
        }
        let index = self.current_decision_record(&draft.key)?;
        let mut record = self.inspection.records[index].clone();
        let image = self.inspection_image()?;
        dper_inspection::attach_decision_evidence_with_catalogs(&mut record, &self.inspection, &image,
            &self.inspection_frame(), &self.measurements, draft.basis()?, &self.drawing_geometry_cache)?;
        // Check the actual source bytes again immediately before committing the
        // local clone. A failed/stale review cannot mutate even its old evidence.
        if self.decision_context()? != draft.context || self.decision_record_index(&draft.key)? != index {
            return Err("Contexte modifié pendant la revue ; préparez une nouvelle revue.".into());
        }
        self.inspection.records[index] = record;
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        self.inspection_panel_cache = None;
        state.draft = None;
        Ok(())
    }

    fn withdraw_native_decision(&mut self, key: &RecordKey, context: &DecisionContext) -> Result<(), String> {
        if self.decision_context()? != *context {
            return Err("Contexte modifié : relisez l'observation avant de retirer sa base de décision.".into());
        }
        let index = self.decision_record_index(key)?;
        let mut record = self.inspection.records[index].clone();
        if record.decision_evidence.is_none() { return Err("Cette observation n'a plus de base de décision à retirer.".into()); }
        record.decision_evidence = None;
        let image = self.inspection_image()?;
        record.outcome = dper_inspection::assess_record_with_catalogs(&record, &self.inspection,
            &image, &self.inspection_frame(), &self.measurements,
            dper_inspection::fingerprint(&image), &self.drawing_geometry_cache);
        if self.decision_context()? != *context || self.decision_record_index(key)? != index {
            return Err("Contexte modifié pendant le retrait ; relisez l'observation.".into());
        }
        self.inspection.records[index] = record;
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        self.inspection_panel_cache = None;
        Ok(())
    }

    pub(crate) fn inspection_decision_panel(&mut self, ui: &mut egui::Ui) {
        if self.inspection.plan.is_none() { return; }
        let keys: Vec<_> = self.inspection.records.iter().filter_map(RecordKey::of).collect();
        let mut state = std::mem::take(&mut self.inspection_decision_ui);
        let mut prepare = false;
        let mut apply = false;
        let mut withdraw = None;
        egui::CollapsingHeader::new("Base documentaire de décision de conformité")
            .default_open(true).show(ui, |ui| {
                ui.label("La qualification indépendante et l'incertitude élargie totale de cette caractéristique doivent être établies et revues par l'opérateur. Le document externe n'est pas vérifié automatiquement ; aucune nouvelle incertitude n'est calculée ici.");
                let response = egui::ComboBox::from_id_salt("decision_record")
                    .selected_text(state.selected.as_ref().map(RecordKey::label).unwrap_or("Choisir une observation enregistrée".into()))
                    .show_ui(ui, |ui| { for key in &keys {
                        let unique = keys.iter().filter(|other| *other == key).count() == 1;
                        let response = ui.add_enabled(unique, egui::Button::selectable(state.selected.as_ref() == Some(key), key.label()));
                        if response.clicked() { state.selected = Some(key.clone()); state.draft = None; }
                    }}).response;
                state.remember("record", &response);
                let response = ui.button("Préparer la revue de cette observation");
                prepare = response.clicked(); state.remember("prepare", &response);
                if let Some(draft) = state.draft.as_mut() {
                    ui.label(format!("{} · révision {}", draft.key.label(), draft.context.revision));
                    if draft.context.revision != self.document_revision || draft.context.capture_id != self.capture_id {
                        ui.colored_label(egui::Color32::YELLOW, "Revue périmée : préparez une nouvelle revue.");
                    }
                    for (key, label, value) in [
                        ("qualification", "Référence du document de qualification", &mut draft.qualification_reference),
                        ("scope", "Domaine qualifié pour cette caractéristique", &mut draft.qualification_scope),
                        ("uncertainty", "Incertitude élargie TOTALE U (mm)", &mut draft.uncertainty_mm),
                        ("source", "Source documentaire de U totale (méthode et portée)", &mut draft.uncertainty_source),
                        ("reviewer", "Responsable de la revue", &mut draft.reviewer),
                    ] {
                        ui.label(label);
                        let response = ui.add(egui::TextEdit::singleline(value).id_salt(("decision", key)).desired_width(430.0));
                        #[cfg(test)]
                        state.widgets.insert(key, response.rect);
                        let _ = response;
                    }
                    ui.label("U totale est celle du résultat mesuré, en mm ; l'incertitude du profil d'étalonnage seul ne suffit pas. La décision applique l'intervalle [mesure − U ; mesure + U] aux limites confirmées.");
                    let response = ui.button("J'atteste avoir revu la qualification indépendante et U totale ; appliquer cette base");
                    apply = response.clicked();
                    #[cfg(test)]
                    state.widgets.insert("apply", response.rect);
                }
                if let Some(key) = &state.selected {
                    if let Ok(index) = self.decision_record_index(key) {
                        let current = self.current_inspection();
                        let record = &current.records[index];
                        ui.label(format!("Décision courante : {}", record.outcome.conformity_status.label()));
                        for reason in &record.outcome.blocking_reasons { ui.label(crate::dper_metrology::reason_label(reason)); }
                        if let Some(evidence) = &record.decision_evidence {
                            ui.label(format!("Base : {} · {} · U totale {:.6} mm · revue par {}",
                                evidence.basis.qualification_reference, evidence.basis.qualification_scope,
                                evidence.basis.expanded_uncertainty_mm, evidence.basis.reviewer));
                            let response = ui.button("Retirer la base de décision");
                            if response.clicked() {
                                // Carry this selected observation and its rendered
                                // context into the guarded operation below.
                                withdraw = Some((key.clone(), self.decision_context()));
                            }
                            state.remember("withdraw", &response);
                        }
                    }
                }
                if !state.status.is_empty() { ui.label(&state.status); }
            });
        if prepare { state.status = match self.prepare_native_decision(&mut state) {
            Ok(()) => "Revue préparée. Renseignez la base indépendante avant l'attestation explicite.".into(), Err(error) => error,
        }; }
        if apply { state.status = match self.apply_native_decision(&mut state) {
            Ok(()) => "Base revue appliquée à cette observation ; consultez la décision et ses motifs.".into(), Err(error) => error,
        }; }
        if let Some((key, context)) = withdraw {
            state.status = match context.and_then(|context| self.withdraw_native_decision(&key, &context)) {
                Ok(()) => {
                    state.draft = None;
                    "Base de décision retirée. L'observation est conservée ; la conformité exige une nouvelle base explicitement revue.".into()
                },
                Err(error) => error,
            };
        }
        self.inspection_decision_ui = state;
    }
}

#[cfg(test)]
#[path = "inspection_decision_ui_tests.rs"]
mod tests;

//! Native observation drafts use the same guarded operations as MCP. Drawing
//! coordinates only locate searches; all recorded geometry comes from pixels.
use crate::{
    dper_inspection::{self, InspectionRecord},
    dper_vision::{EdgePolarity, InspectionGeometry, VisionOptions},
    geometry::Point,
    mcp_server::{DrawingRequest, InspectBoundFeatureArgs, InspectFeatureArgs, InspectionRequest},
    DperMeasureApp,
};
use eframe::egui;

pub(crate) struct InspectionObservationUi {
    feature_id: String,
    occurrence_id: Option<String>,
    group: String,
    draft: Option<ObservationDraft>,
    status: String,
    result: Option<InspectionRecord>,
    #[cfg(test)]
    widgets: std::collections::BTreeMap<&'static str, egui::Rect>,
}

impl Default for InspectionObservationUi {
    fn default() -> Self {
        Self {
            feature_id: String::new(), occurrence_id: None, group: "Groupe 1".into(), draft: None,
            status: String::new(), result: None,
            #[cfg(test)]
            widgets: Default::default(),
        }
    }
}

#[derive(PartialEq, Eq)]
struct ObservationContext {
    capture_id: String,
    revision: u64,
    inputs: serde_json::Value,
    source_bytes: Vec<u8>,
    image_fingerprint: u64,
}

struct ObservationDraft {
    context: ObservationContext,
    feature_id: String,
    occurrence_id: Option<String>,
    group: String,
    bound: bool,
    kind: usize,
    // Centre/start X,Y; end X,Y or radius/start angle/sweep (degrees).
    values: [String; 5],
    options: VisionOptions,
    preview_signature: Option<serde_json::Value>,
}

fn number(text: &str, label: &str) -> Result<f64, String> {
    text.trim().replace(',', ".").parse::<f64>().ok()
        .filter(|value| value.is_finite() && value.abs() <= 1e9)
        .ok_or_else(|| format!("{label} : entrez un nombre fini de module au plus 10⁹."))
}

impl ObservationDraft {
    fn geometry(&self) -> Result<InspectionGeometry, String> {
        let point = Point {
            x: number(&self.values[0], "X plan")?,
            y: number(&self.values[1], "Y plan")?,
        };
        Ok(match self.kind {
            0 => InspectionGeometry::Line {
                start: point,
                end: Point { x: number(&self.values[2], "Fin X plan")?, y: number(&self.values[3], "Fin Y plan")? },
            },
            1 => InspectionGeometry::Circle { center: point, radius: number(&self.values[2], "Rayon plan")? },
            _ => InspectionGeometry::Arc {
                center: point,
                radius: number(&self.values[2], "Rayon plan")?,
                start_rad: number(&self.values[3], "Départ en degrés")?.to_radians(),
                sweep_rad: number(&self.values[4], "Balayage en degrés")?.to_radians(),
            },
        })
    }

    fn signature(&self) -> serde_json::Value {
        serde_json::json!([self.feature_id, self.occurrence_id, self.group, self.bound, self.kind, self.values, self.options])
    }

    fn request(&self, commit: bool) -> Result<InspectionRequest, String> {
        if self.group.trim().is_empty() || self.group.chars().count() > 200 {
            return Err("Renseignez un groupe de contrôle (200 caractères au maximum).".into());
        }
        if self.bound {
            Ok(InspectionRequest::Drawing(DrawingRequest::InspectBound(InspectBoundFeatureArgs {
                source_measurement_ids: Vec::new(),
                capture_id: self.context.capture_id.clone(),
                expected_revision: self.context.revision,
                feature_id: self.feature_id.clone(), group: self.group.clone(),
                occurrence_id: self.occurrence_id.clone(),
                options: Some(self.options.clone()), commit,
            })))
        } else {
            Ok(InspectionRequest::Inspect(InspectFeatureArgs {
                source_measurement_ids: Vec::new(),
                capture_id: self.context.capture_id.clone(),
                expected_revision: self.context.revision,
                feature_id: self.feature_id.clone(), group: self.group.clone(),
                geometry_in_drawing: self.geometry()?, options: Some(self.options.clone()), commit,
            }))
        }
    }
}

impl InspectionObservationUi {
    fn remember(&mut self, _key: &'static str, _response: &egui::Response) {
        #[cfg(test)]
        self.widgets.insert(_key, _response.rect);
    }
}

fn field(ui: &mut egui::Ui, label: &'static str, value: &mut String) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).id_salt(("observation", label)).desired_width(105.0))
    }).inner
}

impl DperMeasureApp {
    fn current_native_observation_result(&self, original: &InspectionRecord) -> InspectionRecord {
        // Preserve the last operation as evidence; only this display copy is
        // reassessed. In particular, rendering never refreshes a prepared draft.
        let evidence = serde_json::to_value(&original.result).ok();
        let current = self.current_inspection().records.into_iter().rev().find(|record|
            record.feature_id == original.feature_id && record.group == original.group
                && record.occurrence == original.occurrence
                && record.capture_id == original.capture_id
                && record.source_fingerprint == original.source_fingerprint
                && record.measurement_id == original.measurement_id
                && record.measurement_points == original.measurement_points
                && serde_json::to_value(&record.result).ok() == evidence)
            .unwrap_or_else(|| {
                let mut historical = original.clone();
                historical.outcome.geometry_state = crate::dper_metrology::GeometryState::Proposed;
                historical.outcome.conformity_status = crate::dper_metrology::ConformityStatus::Indeterminate;
                historical.outcome.blocking_reasons.push("imported_evidence_requires_reassessment".into());
                historical
            });
        current
    }

    fn observation_context(&self) -> Result<ObservationContext, String> {
        if !self.image_is_editable() || self.mcp_mutation_blocked() {
            return Err("Figez l'image et terminez l'action en cours avant de préparer l'observation.".into());
        }
        let (path, fingerprint) = self.current_drawing_source()?;
        let source_bytes = std::fs::read(path).map_err(|error| format!("Lecture du plan source : {error}"))?;
        // Check the fingerprint again after the read to reject a source changed
        // during preparation. Exact bytes are retained until this draft expires.
        self.current_drawing_source()?;
        if crate::dper_source::fingerprint_bytes(&source_bytes) != fingerprint {
            return Err("Le plan source a changé pendant la lecture ; préparez de nouveau.".into());
        }
        let image = self.image.as_ref().ok_or("Aucune image active.")?;
        let pixels = image::RgbImage::from_raw(image.width, image.height, image.rgb.clone())
            .ok_or("Tampon image invalide.")?;
        let mut workspace = self.inspection.clone();
        // The shared preview appends audit history. It must not invalidate its
        // own preparation; all interpretation/registration/dependency inputs do.
        workspace.records.clear();
        Ok(ObservationContext {
            capture_id: self.capture_id.clone(), revision: self.document_revision,
            inputs: serde_json::json!({
                "workspace": workspace, "catalogs": self.drawing_geometry_cache,
                "catalog_generation": self.drawing_geometry_generation,
                "measurements": self.measurements, "calibration": self.calibration,
                "quality": self.quality_assessment, "mm_per_pixel": self.mm_per_pixel,
            }),
            source_bytes, image_fingerprint: dper_inspection::fingerprint(&pixels),
        })
    }

    fn prepare_native_observation(&self, state: &mut InspectionObservationUi) -> Result<(), String> {
        let context = self.observation_context()?;
        let plan = self.inspection.plan.as_ref().ok_or("Aucun plan de contrôle actif.")?;
        let mut features = plan.characteristics.iter().filter(|feature| feature.id == state.feature_id);
        let feature = features.next().filter(|_| features.next().is_none())
            .ok_or("Sélectionnez une caractéristique unique du plan.")?;
        if state.group.trim().is_empty() { return Err("Renseignez un groupe de contrôle.".into()); }
        let count = self.inspection.bindings.iter().filter(|binding|
            binding.candidate.feature_id == state.feature_id).count();
        let repeated = crate::dper_drawing::declared_repetition(feature).is_some_and(|count| count != 1);
        if repeated {
            let occurrence_id = state.occurrence_id.as_deref().ok_or("Sélectionnez une occurrence affectée dans la vue.")?;
            dper_inspection::resolve_occurrence(&self.inspection, &state.feature_id, occurrence_id,
                &self.drawing_geometry_cache)?;
        } else if count > 1 {
            return Err("Association de dessin ambiguë ; corrigez-la avant l'observation.".into());
        } else if count == 1 {
            let binding = self.inspection.bindings.iter().find(|binding|
                binding.candidate.feature_id == state.feature_id).unwrap();
            self.validate_drawing_binding(&binding.candidate.id)?;
        }
        state.draft = Some(ObservationDraft {
            context, feature_id: state.feature_id.clone(), occurrence_id: state.occurrence_id.clone(),
            group: state.group.clone(), bound: repeated || count == 1,
            kind: 1, values: ["50", "50", "20", "0", "180"].map(str::to_owned),
            options: VisionOptions::default(), preview_signature: None,
        });
        state.result = None;
        Ok(())
    }

    fn run_native_observation(&mut self, state: &mut InspectionObservationUi, commit: bool) -> Result<(), String> {
        let draft = state.draft.as_ref().ok_or("Préparez d'abord une observation.")?;
        if self.observation_context()? != draft.context {
            return Err("Brouillon périmé : capture, révision, pixels, plan, source ou contexte modifié. Préparez une nouvelle observation explicitement.".into());
        }
        if state.feature_id != draft.feature_id || state.occurrence_id != draft.occurrence_id || state.group != draft.group {
            return Err("Caractéristique ou groupe modifié : préparez une nouvelle observation.".into());
        }
        if commit && draft.preview_signature.as_ref() != Some(&draft.signature()) {
            return Err("Prévisualisez les réglages actuels avant d'enregistrer l'observation.".into());
        }
        let request = draft.request(commit)?;
        self.handle_inspection_request(request)?;
        state.result = self.inspection.records.last().cloned();
        self.inspection_panel_cache = None;
        if commit {
            // Consuming a successful recording prevents an accidental double click
            // from duplicating a measurement under a new revision.
            state.draft = None;
        } else if let Some(draft) = state.draft.as_mut() {
            draft.context.revision = self.document_revision;
            draft.preview_signature = Some(draft.signature());
        }
        Ok(())
    }

    pub(crate) fn inspection_observation_panel(&mut self, ui: &mut egui::Ui) {
        let Some(plan) = self.inspection.plan.as_ref() else { return; };
        let features: Vec<_> = plan.characteristics.iter().map(|feature|
            (feature.id.clone(), feature.label.clone())).collect();
        let mut state = std::mem::take(&mut self.inspection_observation_ui);
        let display_result = state.result.as_ref().map(|result| self.current_native_observation_result(result));
        let mut prepare = false;
        let mut preview = false;
        let mut record = false;
        let mut reevaluate = false;
        egui::CollapsingHeader::new("Observer les contours de la photo")
            .default_open(true).show(ui, |ui| {
                ui.label("La géométrie de recherche est une proposition dans le repère du dessin ; seule la photo fournit les contours observés.");
                let response = egui::ComboBox::from_id_salt("observation_feature")
                    .selected_text(if state.feature_id.is_empty() { "Choisir une caractéristique" } else { &state.feature_id })
                    .show_ui(ui, |ui| {
                        for (id, label) in &features {
                            let response = ui.selectable_value(&mut state.feature_id, id.clone(), format!("{id} · {label}"));
                            if response.changed() { state.draft = None; state.result = None; state.occurrence_id = None; }
                        }
                    }).response;
                state.remember("feature", &response);
                if let Some(assignment) = self.inspection.occurrence_assignments.iter()
                    .find(|assignment| assignment.feature_id == state.feature_id) {
                    let selected = assignment.members.iter().find(|member| Some(&member.id) == state.occurrence_id.as_ref());
                    let response = egui::ComboBox::from_id_salt("observation_occurrence")
                        .selected_text(selected.map(|member| format!("Occurrence {}", member.number))
                            .unwrap_or_else(|| "Choisir une occurrence".into()))
                        .show_ui(ui, |ui| {
                            for member in &assignment.members {
                                let label = format!("Occurrence {} · {}", member.number, member.geometry_id);
                                if ui.selectable_value(&mut state.occurrence_id, Some(member.id.clone()), label).changed() {
                                    state.draft = None;
                                    state.result = None;
                                }
                            }
                        }).response;
                    state.remember("occurrence", &response);
                }
                let response = field(ui, "Groupe de contrôle", &mut state.group);
                if response.changed() { state.draft = None; state.result = None; }
                state.remember("group", &response);
                let response = ui.button("Préparer une nouvelle observation");
                prepare = response.clicked(); state.remember("prepare", &response);
                if let Some(draft) = state.draft.as_mut() {
                    ui.label(format!("{} · {} · capture {} · révision {}", draft.feature_id, draft.group,
                        draft.context.capture_id, draft.context.revision));
                    if draft.context.revision != self.document_revision || draft.context.capture_id != self.capture_id {
                        ui.colored_label(egui::Color32::YELLOW, "Brouillon périmé : préparez une nouvelle observation.");
                    }
                    let before = draft.signature();
                    if draft.bound {
                        ui.label("Recherche issue de l'association PDF. Le moteur résout la géométrie liée ; aucune substitution manuelle.");
                    } else {
                        ui.label("Localisation manuelle dans le DESSIN (unités du dessin, pt pour un PDF). Les valeurs initiales sont des exemples à remplacer, jamais des mesures.");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut draft.kind, 0, "Droite");
                            ui.selectable_value(&mut draft.kind, 1, "Cercle");
                            ui.selectable_value(&mut draft.kind, 2, "Arc");
                        });
                        let labels = if draft.kind == 0 {
                            vec!["Départ X (plan)", "Départ Y (plan)", "Fin X (plan)", "Fin Y (plan)"]
                        } else if draft.kind == 1 {
                            vec!["Centre X (plan)", "Centre Y (plan)", "Rayon (plan)"]
                        } else {
                            vec!["Centre X (plan)", "Centre Y (plan)", "Rayon (plan)", "Départ (°)", "Balayage signé (°)"]
                        };
                        for (index, label) in labels.into_iter().enumerate() {
                            let response = field(ui, label, &mut draft.values[index]);
                            #[cfg(test)]
                            state.widgets.insert(label, response.rect);
                            let _ = response;
                        }
                    }
                    ui.collapsing("Réglages bornés de recherche des contours", |ui| {
                        ui.horizontal(|ui| { ui.label("Profils (16–256)"); ui.add(egui::DragValue::new(&mut draft.options.sample_count).range(16..=256)); });
                        ui.horizontal(|ui| { ui.label("Recherche (px, 1–32)"); ui.add(egui::DragValue::new(&mut draft.options.search_radius_px).range(1.0..=32.0).speed(0.1)); });
                        ui.horizontal(|ui| { ui.label("Gradient minimum"); ui.add(egui::DragValue::new(&mut draft.options.minimum_gradient).range(0.1..=255.0).speed(0.1)); });
                        egui::ComboBox::from_id_salt("observation_polarity").selected_text(format!("Polarité : {:?}", draft.options.polarity)).show_ui(ui, |ui| {
                            ui.selectable_value(&mut draft.options.polarity, EdgePolarity::Any, "Toutes");
                            ui.selectable_value(&mut draft.options.polarity, EdgePolarity::Rising, "Montante");
                            ui.selectable_value(&mut draft.options.polarity, EdgePolarity::Falling, "Descendante");
                        });
                        ui.horizontal(|ui| { ui.label("Couverture minimum (0,5–1)"); ui.add(egui::DragValue::new(&mut draft.options.minimum_coverage).range(0.5..=1.0).speed(0.01)); });
                        ui.horizontal(|ui| { ui.label("Rejet des écarts (px)"); ui.add(egui::DragValue::new(&mut draft.options.outlier_threshold_px).range(0.05..=5.0).speed(0.05)); });
                        ui.horizontal(|ui| { ui.label("RMS maximum (px, ≤ seuil de rejet)"); ui.add(egui::DragValue::new(&mut draft.options.maximum_rms_px).range(0.01..=draft.options.outlier_threshold_px).speed(0.01)); });
                        let mut degrees = draft.options.minimum_arc_sweep_rad.to_degrees();
                        ui.horizontal(|ui| { ui.label("Arc minimum (60–180°)"); if ui.add(egui::DragValue::new(&mut degrees).range(60.0..=180.0)).changed() { draft.options.minimum_arc_sweep_rad = degrees.to_radians(); } });
                    });
                    if before != draft.signature() { draft.preview_signature = None; state.result = None; }
                    let can_record = draft.preview_signature.as_ref() == Some(&draft.signature());
                    let response = ui.button("Prévisualiser les contours (sans mesure)");
                    preview = response.clicked();
                    #[cfg(test)]
                    state.widgets.insert("preview", response.rect);
                    let response = ui.add_enabled(can_record, egui::Button::new("Enregistrer explicitement l'observation"));
                    record = response.clicked();
                    #[cfg(test)]
                    state.widgets.insert("record", response.rect);
                }
                let response = ui.button("Réévaluer les observations existantes");
                reevaluate = response.clicked(); state.remember("reevaluate", &response);
                ui.label("Les extrémités de droites et d'arcs ne sont pas mesurées. Une droite reste un aperçu : l'enregistrement d'une longueur est suspendu. Le recalage ne remplace pas l'étalonnage indépendant. Validation logicielle et conformité physique sont distinctes.");
                if let Some(result) = &display_result {
                    ui.separator();
                    ui.label("Dernier résultat de l'opération ; statut réévalué contre le contexte courant.");
                    if result.occurrence.is_some() { ui.label(result.target_label()); }
                    ui.label(if result.measurement_id.is_some() { "Observation enregistrée" } else { "Aperçu observé, aucune mesure native créée" });
                    ui.label(format!("Géométrie observée en pixels : {:?}", result.result.geometry));
                    ui.label(format!("{} points acceptés / {} · couverture {:.0} % · RMS {:.3} px",
                        result.result.evidence.accepted_points.len(), result.result.evidence.sample_count,
                        result.result.evidence.coverage_fraction * 100.0, result.result.evidence.rms_residual_px));
                    ui.label(format!("Contours : {} · conformité : {}", result.outcome.geometry_state.label(), result.outcome.conformity_status.label()));
                    for reason in &result.outcome.blocking_reasons { ui.label(crate::dper_metrology::reason_label(reason)); }
                }
                if !state.status.is_empty() { ui.label(&state.status); }
            });
        if prepare {
            state.status = match self.prepare_native_observation(&mut state) {
                Ok(()) => "Brouillon préparé. Vérifiez la localisation puis prévisualisez les contours réels.".into(),
                Err(error) => error,
            };
        }
        if preview || record {
            state.status = match self.run_native_observation(&mut state, record) {
                Ok(()) if record => "Observation enregistrée à partir des contours de la photo.".into(),
                Ok(()) => "Aperçu calculé ; trace conservée dans l'historique, aucune mesure native créée. L'enregistrement demande une action explicite.".into(),
                Err(error) => error,
            };
        }
        if reevaluate {
            state.status = match self.handle_inspection_request(InspectionRequest::GetState) {
                Ok(_) => {
                    self.inspection_panel_cache = None;
                    if let Some(result) = &state.result {
                        state.result = self.current_inspection().records.into_iter().rev().find(|record|
                            record.feature_id == result.feature_id && record.occurrence == result.occurrence
                                && record.measurement_id == result.measurement_id && record.group == result.group);
                    }
                    "Observations réévaluées contre les pixels et le contexte courants ; voir les résultats et motifs dans le plan de contrôle.".into()
                },
                Err(error) => error,
            };
        }
        self.inspection_observation_ui = state;
    }
}

#[cfg(test)]
#[path = "inspection_observation_ui_tests.rs"]
mod tests;

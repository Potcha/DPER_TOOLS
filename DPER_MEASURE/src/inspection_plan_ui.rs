//! Native, explicit PDF view and binding drafts. All writes use the shared adapter.
use crate::{
    dper_binding::{BindingCandidate, DrawingView},
    dper_drawing::{DrawingCharacteristic, DrawingDimensionKind},
    dper_pdf_geometry::{VectorPrimitive, VectorPrimitiveKind},
    mcp_server::{
        AssignDrawingOccurrencesArgs, BindingCandidatesArgs, DefineDrawingViewArgs, DrawingRequest,
        InspectDrawingAngularSupportsArgs, InspectDrawingLinearSupportsArgs, SelectBindingArgs,
    },
    DperMeasureApp,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct DraftContext {
    capture_id: String,
    revision: u64,
    plan_fingerprint: String,
    source_path: String,
    source_fingerprint: String,
    catalog_generation: u64,
    catalog_fingerprint: String,
    page: u32,
}

#[derive(Clone)]
struct ViewDraft {
    context: DraftContext,
    replace_view_id: Option<String>,
    label: String,
    feature_ids: BTreeSet<String>,
    geometry_ids: BTreeSet<String>,
    features: Vec<DrawingCharacteristic>,
    primitives: Vec<VectorPrimitive>,
}

#[derive(Clone)]
struct BindingDraft {
    context: DraftContext,
    view_id: String,
    feature_id: String,
    candidates: Vec<BindingCandidate>,
    candidate_id: Option<String>,
    occurrence_count: Option<u32>,
    occurrence_candidates: Vec<String>,
    occurrence_centers: BTreeMap<String, crate::geometry::Point>,
    justification: String,
}

struct SupportDraft {
    context: DraftContext,
    view_id: String,
    feature_id: String,
    lines: Vec<VectorPrimitive>,
    support_ids: [String; 2],
    justification: String,
    result: Option<Result<serde_json::Value, String>>,
}

#[derive(Default)]
pub(crate) struct InspectionPlanUi {
    page: u32,
    replace_view_id: Option<String>,
    view: Option<ViewDraft>,
    binding_view_id: String,
    binding_feature_id: String,
    binding: Option<BindingDraft>,
    supports: Option<SupportDraft>,
    status: String,
}

fn serialized_fingerprint(value: &impl serde::Serialize) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| crate::dper_source::fingerprint_bytes(&bytes))
        .map_err(|error| format!("Impossible de préparer les sources : {error}"))
}

fn primitive_label(primitive: &VectorPrimitive) -> String {
    let kind = match primitive.kind {
        VectorPrimitiveKind::Line => "Ligne",
        VectorPrimitiveKind::Polyline => "Polyligne",
        VectorPrimitiveKind::ClosedPolygon => "Contour fermé",
        VectorPrimitiveKind::CircleCandidate => "Cercle proposé",
    };
    format!("{} · {} · ({:.1}, {:.1})–({:.1}, {:.1}) pt", primitive.id, kind,
        primitive.bbox.x_min, primitive.bbox.y_min, primitive.bbox.x_max, primitive.bbox.y_max)
}

fn feature_label(feature: &DrawingCharacteristic) -> String {
    format!("{} · {} · {}", feature.id, feature.label, feature.source.exact_text)
}

impl DperMeasureApp {
    fn plan_editor_guard(&self) -> Result<(), String> {
        if self.mcp_mutation_blocked() {
            return Err("Terminez ou annulez l'action en cours avant de préparer ou modifier le plan.".into());
        }
        Ok(())
    }

    fn plan_editor_context(&self, page: u32) -> Result<DraftContext, String> {
        // Always read current bytes. A metadata cache cannot authorize a draft.
        let (source_path, source_fingerprint) = self.current_drawing_source()?;
        let plan = self.inspection.plan.as_ref().ok_or("Importez un plan PDF.")?;
        let catalog = self.drawing_geometry_cache.iter().find(|catalog| {
            catalog.geometry.page == page && catalog.document_fingerprint == source_fingerprint
        }).ok_or("Analysez les géométries de cette page PDF avant de préparer une vue.")?;
        Ok(DraftContext {
            capture_id: self.capture_id.clone(),
            revision: self.document_revision,
            plan_fingerprint: serialized_fingerprint(plan)?,
            source_path,
            source_fingerprint,
            catalog_generation: self.drawing_geometry_generation,
            catalog_fingerprint: serialized_fingerprint(catalog)?,
            page,
        })
    }

    fn validate_plan_editor_context(&self, context: &DraftContext) -> Result<(), String> {
        self.plan_editor_guard()?;
        self.ensure_mcp_revision(Some(context.revision))?;
        let current = self.plan_editor_context(context.page)?;
        if &current != context {
            return Err("La capture, le plan ou son catalogue a changé. Préparez / rechargez explicitement le brouillon.".into());
        }
        Ok(())
    }

    fn prepare_native_view(&mut self) -> Result<(), String> {
        self.plan_editor_guard()?;
        let page = self.inspection_plan_ui.page;
        let context = self.plan_editor_context(page)?;
        let plan = self.inspection.plan.as_ref().unwrap();
        let catalog = self.drawing_geometry_cache.iter().find(|catalog| {
            catalog.geometry.page == page && catalog.document_fingerprint == context.source_fingerprint
        }).unwrap();
        let replace_view_id = self.inspection_plan_ui.replace_view_id.clone();
        let existing = match &replace_view_id {
            Some(id) => Some(self.inspection.views.iter().find(|view| &view.id == id && view.page == page)
                .ok_or("La vue à remplacer n'appartient pas à cette page." )?),
            None => None,
        };
        self.inspection_plan_ui.view = Some(ViewDraft {
            context,
            replace_view_id,
            label: existing.map(|view| view.label.clone()).unwrap_or_default(),
            feature_ids: existing.map(|view| view.feature_ids.iter().cloned().collect()).unwrap_or_default(),
            geometry_ids: existing.map(|view| view.geometry_ids.iter().cloned().collect()).unwrap_or_default(),
            features: plan.characteristics.iter().filter(|feature| {
                feature.source.page == Some(page) && feature.source.pdf_bbox.is_some()
            }).cloned().collect(),
            primitives: catalog.geometry.primitives.clone(),
        });
        Ok(())
    }

    fn submit_native_view(&mut self) -> Result<(), String> {
        let draft = self.inspection_plan_ui.view.as_ref().ok_or("Préparez la vue.")?.clone();
        self.validate_plan_editor_context(&draft.context)?;
        if self.inspection_plan_ui.page != draft.context.page || self.inspection_plan_ui.replace_view_id != draft.replace_view_id {
            return Err("La page ou la vue choisie a changé. Préparez / rechargez la vue.".into());
        }
        self.handle_drawing_request(DrawingRequest::DefineView(DefineDrawingViewArgs {
            expected_revision: draft.context.revision,
            page: draft.context.page,
            label: draft.label,
            feature_ids: draft.feature_ids.into_iter().collect(),
            geometry_ids: draft.geometry_ids.into_iter().collect(),
            replace_view_id: draft.replace_view_id,
        }))?;
        self.inspection_plan_ui.view = None;
        Ok(())
    }

    fn prepare_native_binding(&mut self) -> Result<(), String> {
        self.plan_editor_guard()?;
        let view_id = self.inspection_plan_ui.binding_view_id.clone();
        let feature_id = self.inspection_plan_ui.binding_feature_id.clone();
        let page = self.inspection.views.iter().find(|view| view.id == view_id)
            .ok_or("Choisissez une vue explicite.")?.page;
        let context = self.plan_editor_context(page)?;
        let occurrence_count = self.inspection.plan.as_ref().unwrap().characteristics.iter()
            .find(|feature| feature.id == feature_id)
            .and_then(crate::dper_drawing::declared_repetition)
            .filter(|count| *count != 1);
        let value = self.handle_drawing_request(DrawingRequest::Candidates(BindingCandidatesArgs {
            view_id: view_id.clone(), feature_id: feature_id.clone(),
        }))?;
        let candidates = serde_json::from_value(value["candidates"].clone())
            .map_err(|error| format!("Propositions d'association invalides : {error}"))?;
        let mut occurrence_centers = BTreeMap::new();
        if occurrence_count.is_some() {
            let catalog = self.drawing_geometry_cache.iter().find(|catalog|
                catalog.geometry.page == page && catalog.document_fingerprint == context.source_fingerprint
            ).ok_or("Catalogue de la vue absent.")?;
            occurrence_centers.extend(catalog.geometry.primitives.iter().filter_map(|primitive|
                primitive.circle.as_ref().map(|circle| (primitive.id.clone(), circle.center))
            ));
        }
        self.inspection_plan_ui.binding = Some(BindingDraft {
            context, view_id, feature_id, candidates,
            candidate_id: None, occurrence_count, occurrence_candidates: Vec::new(), occurrence_centers,
            justification: String::new(),
        });
        Ok(())
    }

    fn submit_native_binding(&mut self) -> Result<(), String> {
        let draft = self.inspection_plan_ui.binding.as_ref().ok_or("Préparez les associations.")?.clone();
        self.validate_plan_editor_context(&draft.context)?;
        if draft.view_id != self.inspection_plan_ui.binding_view_id || draft.feature_id != self.inspection_plan_ui.binding_feature_id {
            return Err("La vue ou la caractéristique choisie a changé. Préparez / rechargez les associations.".into());
        }
        if draft.occurrence_count.is_some() {
            self.handle_drawing_request(DrawingRequest::AssignOccurrences(AssignDrawingOccurrencesArgs {
                expected_revision: draft.context.revision,
                view_id: draft.view_id, feature_id: draft.feature_id,
                candidate_ids: draft.occurrence_candidates, justification: draft.justification,
            }))?;
            self.inspection_plan_ui.binding = None;
            return Ok(());
        }
        let candidate_id = draft.candidate_id.ok_or("Choisissez explicitement une géométrie proposée.")?;
        if !draft.candidates.iter().any(|candidate| candidate.id == candidate_id) {
            return Err("La proposition choisie n'appartient pas à ce brouillon.".into());
        }
        self.handle_drawing_request(DrawingRequest::Select(SelectBindingArgs {
            expected_revision: draft.context.revision,
            view_id: draft.view_id, feature_id: draft.feature_id,
            candidate_id, justification: draft.justification,
        }))?;
        self.inspection_plan_ui.binding = None;
        Ok(())
    }

    pub(crate) fn inspection_plan_editor_panel(&mut self, ui: &mut egui::Ui) {
        if self.inspection.plan.is_none() {
            self.inspection_plan_ui.supports = None;
            return;
        }
        egui::CollapsingHeader::new("Vues et associations du plan PDF")
            .default_open(true).show(ui, |ui| {
                ui.weak("Définissez une vue, puis choisissez une association. Aucun choix ne confirme une mesure ni une conformité.");
                let source = self.current_drawing_source();
                let pages = match &source {
                    Ok((_, fingerprint)) => self.drawing_geometry_cache.iter()
                        .filter(|catalog| &catalog.document_fingerprint == fingerprint)
                        .map(|catalog| catalog.geometry.page).collect::<Vec<_>>(),
                    Err(error) => { ui.label(error); Vec::new() }
                };
                if pages.is_empty() {
                    self.inspection_plan_ui.supports = None;
                    ui.label("Analysez une page PDF dans « Géométries proposées du PDF ».");
                    if self.inspection.views.iter().any(|view| view.source_proof.is_none()) {
                        ui.colored_label(egui::Color32::YELLOW,
                            "Vues historiques sans preuve de sources : redéfinissez les vues et leurs associations après analyse.");
                    }
                    if self.inspection_plan_ui.view.is_some() || self.inspection_plan_ui.binding.is_some() {
                        ui.label("Brouillons conservés, utilisation refusée : rechargez après analyse de la source.");
                    }
                    return;
                }
                if self.inspection_plan_ui.page == 0 { self.inspection_plan_ui.page = pages[0]; }
                let editable = !self.mcp_mutation_blocked();
                if !editable { ui.label("Terminez ou annulez l'action en cours pour modifier le plan."); }
                let views = self.inspection.views.clone();
                for view in &views {
                    if let Err(error) = self.drawing_registration_scope(&view.id) {
                        ui.colored_label(egui::Color32::YELLOW, format!("Vue « {} » suspendue : {}", view.label, error));
                    }
                }
                ui.heading("1. Définir une vue");
                egui::ComboBox::from_id_salt("native_view_page")
                    .selected_text(format!("Page {} analysée", self.inspection_plan_ui.page))
                    .show_ui(ui, |ui| {
                        for page in &pages { ui.selectable_value(&mut self.inspection_plan_ui.page, *page, format!("Page {page}")); }
                    });
                let replacement = self.inspection_plan_ui.replace_view_id.as_ref()
                    .and_then(|id| views.iter().find(|view| &view.id == id))
                    .map(|view| format!("Remplacer : {}", view.label)).unwrap_or_else(|| "Nouvelle vue".into());
                egui::ComboBox::from_id_salt("native_view_replacement").selected_text(replacement)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.inspection_plan_ui.replace_view_id, None, "Nouvelle vue");
                        for view in views.iter().filter(|view| view.page == self.inspection_plan_ui.page) {
                            ui.selectable_value(&mut self.inspection_plan_ui.replace_view_id, Some(view.id.clone()), format!("Remplacer : {} · {}", view.label, view.id));
                        }
                    });
                if ui.add_enabled(editable, egui::Button::new("Préparer / recharger la vue")).clicked() {
                    self.inspection_plan_ui.status = match self.prepare_native_view() {
                        Ok(()) => "Brouillon préparé. Choisissez son libellé et ses sources.".into(), Err(error) => error,
                    };
                }
                if let Some(draft) = self.inspection_plan_ui.view.clone() {
                    let validity = self.validate_plan_editor_context(&draft.context).and_then(|()| {
                        if self.inspection_plan_ui.page != draft.context.page || self.inspection_plan_ui.replace_view_id != draft.replace_view_id {
                            Err("La page ou la vue choisie a changé. Préparez / rechargez la vue.".into())
                        } else { Ok(()) }
                    });
                    if let Err(error) = &validity { ui.colored_label(egui::Color32::YELLOW, error); }
                    ui.add_enabled_ui(validity.is_ok(), |ui| {
                        let state = self.inspection_plan_ui.view.as_mut().unwrap();
                        ui.label("Libellé explicite de la vue");
                        ui.add(egui::TextEdit::singleline(&mut state.label).hint_text("Ex. : vue de dessus"));
                        ui.label("Caractéristiques localisées de cette page");
                        egui::ScrollArea::vertical().id_salt("native_view_features").max_height(160.0).show(ui, |ui| {
                            for feature in &state.features {
                                let mut selected = state.feature_ids.contains(&feature.id);
                                if ui.checkbox(&mut selected, feature_label(feature)).changed() {
                                    if selected { state.feature_ids.insert(feature.id.clone()); }
                                    else { state.feature_ids.remove(&feature.id); }
                                }
                            }
                            if state.features.is_empty() { ui.label("Aucune caractéristique PDF localisée sur cette page."); }
                        });
                        ui.label("Géométries réellement incluses dans cette vue");
                        egui::ScrollArea::vertical().id_salt("native_view_primitives").max_height(180.0).show(ui, |ui| {
                            for primitive in &state.primitives {
                                let mut selected = state.geometry_ids.contains(&primitive.id);
                                if ui.checkbox(&mut selected, primitive_label(primitive)).changed() {
                                    if selected { state.geometry_ids.insert(primitive.id.clone()); }
                                    else { state.geometry_ids.remove(&primitive.id); }
                                }
                            }
                        });
                        if state.replace_view_id.is_some() { ui.weak("Le remplacement supprime les associations et le recalage liés à cette vue."); }
                        if ui.button("Enregistrer la vue explicite").clicked() {
                            self.inspection_plan_ui.status = match self.submit_native_view() {
                                Ok(()) => "Vue enregistrée. Préparez les associations pour choisir une géométrie.".into(), Err(error) => error,
                            };
                        }
                    });
                }
                ui.separator();
                self.native_binding_editor(ui, &views, editable);
                if !self.inspection_plan_ui.status.is_empty() { ui.label(&self.inspection_plan_ui.status); }
            });
    }

    fn native_binding_editor(&mut self, ui: &mut egui::Ui, views: &[DrawingView], editable: bool) {
        ui.heading("2. Choisir une association");
        let old_view = self.inspection_plan_ui.binding_view_id.clone();
        let selected_view = views.iter().find(|view| view.id == old_view);
        egui::ComboBox::from_id_salt("native_binding_view")
            .selected_text(selected_view.map(|view| format!("{} · page {}", view.label, view.page))
                .unwrap_or_else(|| "Choisir une vue".into()))
            .show_ui(ui, |ui| {
                for view in views {
                    ui.selectable_value(&mut self.inspection_plan_ui.binding_view_id, view.id.clone(), format!("{} · page {} · {}", view.label, view.page, view.id));
                }
            });
        if old_view != self.inspection_plan_ui.binding_view_id {
            self.inspection_plan_ui.binding_feature_id.clear();
            self.inspection_plan_ui.supports = None;
        }
        let selected_view = views.iter().find(|view| view.id == self.inspection_plan_ui.binding_view_id);
        let plan = self.inspection.plan.as_ref().unwrap();
        let features = plan.characteristics.iter().filter(|feature| selected_view.is_some_and(|view| view.feature_ids.contains(&feature.id))).collect::<Vec<_>>();
        let old_feature = self.inspection_plan_ui.binding_feature_id.clone();
        egui::ComboBox::from_id_salt("native_binding_feature")
            .selected_text(features.iter().find(|feature| feature.id == self.inspection_plan_ui.binding_feature_id)
                .map(|feature| feature_label(feature)).unwrap_or_else(|| "Choisir une caractéristique de la vue".into()))
            .show_ui(ui, |ui| {
                for feature in &features {
                    ui.selectable_value(&mut self.inspection_plan_ui.binding_feature_id, feature.id.clone(), feature_label(feature));
                }
            });
        if old_feature != self.inspection_plan_ui.binding_feature_id {
            self.inspection_plan_ui.supports = None;
        }
        if let (Some(view), Some(feature)) = (selected_view, features.iter().find(|feature|
            feature.id == self.inspection_plan_ui.binding_feature_id))
        {
            let angular = match feature.kind {
                DrawingDimensionKind::Angle => Some(true),
                DrawingDimensionKind::Linear => Some(false),
                _ => None,
            };
            if let Some(angular) = angular {
                self.native_support_editor(ui, view, angular, editable);
                return;
            }
        }
        if ui.add_enabled(editable, egui::Button::new("Préparer / recharger les associations")).clicked() {
            self.inspection_plan_ui.status = match self.prepare_native_binding() {
                Ok(()) => "Propositions chargées. Aucun candidat n'est sélectionné automatiquement.".into(), Err(error) => error,
            };
        }
        let Some(draft) = self.inspection_plan_ui.binding.clone() else { return; };
        let validity = self.validate_plan_editor_context(&draft.context).and_then(|()| {
            if draft.view_id != self.inspection_plan_ui.binding_view_id || draft.feature_id != self.inspection_plan_ui.binding_feature_id {
                Err("La vue ou la caractéristique choisie a changé. Préparez / rechargez les associations.".into())
            } else { Ok(()) }
        });
        if let Err(error) = &validity { ui.colored_label(egui::Color32::YELLOW, error); }
        ui.add_enabled_ui(validity.is_ok(), |ui| {
            let state = self.inspection_plan_ui.binding.as_mut().unwrap();
            if let Some(count) = state.occurrence_count {
                if !(2..=crate::dper_binding::MAX_OCCURRENCES).contains(&count) {
                    ui.label("L’affectation prend en charge les groupes de 2 à 64 occurrences.");
                    return;
                }
                ui.label(format!("Choisissez {count} cercles dans l’ordre des occurrences."));
            }
            if state.candidates.is_empty() { ui.label("Aucun cercle complet compatible dans cette vue. Aucune association créée."); }
            if state.candidates.len() > 1 { ui.label("Plusieurs cercles possibles : comparez toutes les alternatives et justifiez votre choix."); }
            if state.candidates.iter().any(|candidate| candidate.suspension_reasons.iter()
                .any(|reason| reason == crate::dper_binding::SHARED_CALLOUT_SOURCE)) {
                ui.label("Plusieurs cotes partagent une source de texte PDF. Aucun renvoi n’est prioritaire.");
            }
            ui.weak("Le rang compare les indices d'attache puis la proximité. Une attache reste une hypothèse à vérifier sur le plan.");
            egui::ScrollArea::vertical().id_salt("native_binding_candidates").max_height(220.0).show(ui, |ui| {
                for candidate in &state.candidates {
                    let label = format!("{} · rang {} · distance {:.2} pt", candidate.geometry_id, candidate.rank, candidate.distance_pt);
                    if let Some(count) = state.occurrence_count {
                        let index = state.occurrence_candidates.iter().position(|id| id == &candidate.id);
                        let label = index.map_or(label.clone(), |index| format!("Occurrence {} · {label}", index + 1));
                        let mut chosen = index.is_some();
                        if ui.add_enabled(chosen || state.occurrence_candidates.len() < count as usize,
                            egui::Checkbox::new(&mut chosen, label)).changed() {
                            if let Some(index) = index {
                                state.occurrence_candidates.remove(index);
                            } else {
                                state.occurrence_candidates.push(candidate.id.clone());
                            }
                        }
                        if let Some(center) = state.occurrence_centers.get(&candidate.geometry_id) {
                            ui.weak(format!("Centre sur le plan ({:.2}, {:.2}) pt", center.x, center.y));
                        }
                    } else {
                        ui.radio_value(&mut state.candidate_id, Some(candidate.id.clone()), label);
                    }
                    for (index, evidence) in candidate.evidence.iter().enumerate() {
                        if evidence.starts_with("geometric_triangular_tip_attachment_hypothesis:") {
                            ui.label("Pointe triangulaire : hypothèse géométrique à vérifier sur le plan.");
                            ui.push_id((&candidate.id, index), |ui| {
                                ui.collapsing("Sources de la pointe triangulaire", |ui| { ui.weak(evidence); });
                            });
                        } else {
                            ui.weak(evidence);
                        }
                    }
                    ui.weak("Proposition à vérifier : vue physique et visibilité du cercle à confirmer.");
                }
            });
            ui.label("Justification du choix d'après le plan");
            ui.add(egui::TextEdit::multiline(&mut state.justification).hint_text("Décrire le rappel de cote et le cercle choisi"));
            let repeated = state.occurrence_count.is_some();
            let ready = !state.justification.trim().is_empty() && state.occurrence_count.map_or(
                state.candidate_id.is_some(), |count| state.occurrence_candidates.len() == count as usize);
            let label = if repeated { "Enregistrer les occurrences choisies" } else { "Enregistrer l'association choisie" };
            if ui.add_enabled(ready, egui::Button::new(label)).clicked() {
                self.inspection_plan_ui.status = match self.submit_native_binding() {
                    Ok(()) if repeated => "Occurrences proposées enregistrées. Vérifiez chaque emplacement sur le plan.".into(),
                    Ok(()) => "Association proposée enregistrée. Vérifiez-la ensuite sur le plan dans le panneau de contrôle.".into(), Err(error) => error,
                };
            }
        });
    }

    fn prepare_native_supports(&self, view: &DrawingView) -> Result<SupportDraft, String> {
        self.plan_editor_guard()?;
        self.drawing_registration_scope(&view.id)?;
        let context = self.plan_editor_context(view.page)?;
        let catalog = self.drawing_geometry_cache.iter().find(|catalog|
            catalog.geometry.page == view.page && catalog.document_fingerprint == context.source_fingerprint
        ).ok_or("Catalogue de la vue absent.")?;
        let lines = catalog.geometry.primitives.iter().filter(|primitive| {
            primitive.kind == VectorPrimitiveKind::Line && !primitive.closed
                && primitive.points.len() == 2 && view.geometry_ids.contains(&primitive.id)
        }).cloned().collect();
        Ok(SupportDraft {
            context,
            view_id: view.id.clone(),
            feature_id: self.inspection_plan_ui.binding_feature_id.clone(),
            lines,
            support_ids: Default::default(),
            justification: String::new(),
            result: None,
        })
    }

    fn native_support_editor(&mut self, ui: &mut egui::Ui, view: &DrawingView, angular: bool, editable: bool) {
        ui.label("Diagnostic documentaire des supports");
        ui.weak("Choisissez deux traits de la vue. Le diagnostic aide à lire le plan et reste à vérifier.");
        if ui.add_enabled(editable, egui::Button::new("Préparer le diagnostic des supports")).clicked() {
            match self.prepare_native_supports(view) {
                Ok(draft) => {
                    self.inspection_plan_ui.supports = Some(draft);
                    self.inspection_plan_ui.status.clear();
                }
                Err(error) => {
                    self.inspection_plan_ui.supports = None;
                    self.inspection_plan_ui.status = error;
                }
            }
        }
        let Some(draft) = self.inspection_plan_ui.supports.as_ref() else { return; };
        if let Err(error) = self.validate_plan_editor_context(&draft.context) {
            self.inspection_plan_ui.supports = None;
            ui.colored_label(egui::Color32::YELLOW, error);
            return;
        }
        let draft = self.inspection_plan_ui.supports.as_mut().unwrap();
        if draft.lines.len() < 2 {
            ui.label("La vue doit contenir au moins deux traits droits.");
            return;
        }
        for (index, hint) in ["Choisir le premier support", "Choisir le second support"].into_iter().enumerate() {
            let selected = &mut draft.support_ids[index];
            let old = selected.clone();
            egui::ComboBox::from_id_salt(("native_support", index))
                .selected_text(draft.lines.iter().find(|line| line.id == *selected)
                    .map(primitive_label).unwrap_or_else(|| hint.into()))
                .show_ui(ui, |ui| {
                    for line in &draft.lines {
                        ui.selectable_value(selected, line.id.clone(), primitive_label(line));
                    }
                });
            if *selected != old {
                draft.result = None;
            }
        }
        if ui.add(egui::TextEdit::multiline(&mut draft.justification)
            .id_salt("native_support_justification")
            .hint_text("Indiquer les deux traits repérés sur le plan")).changed()
        {
            draft.result = None;
        }
        let ready = draft.support_ids.iter().all(|id| !id.is_empty())
            && draft.support_ids[0] != draft.support_ids[1] && !draft.justification.trim().is_empty();
        if ui.add_enabled(ready, egui::Button::new("Diagnostiquer les supports")).clicked() {
            let expected_revision = draft.context.revision;
            let feature_id = draft.feature_id.clone();
            let view_id = draft.view_id.clone();
            let support_ids = draft.support_ids.clone();
            let justification = draft.justification.clone();
            let request = if angular {
                DrawingRequest::InspectAngularSupports(InspectDrawingAngularSupportsArgs {
                    expected_revision, feature_id, view_id, support_ids, justification,
                })
            } else {
                DrawingRequest::InspectLinearSupports(InspectDrawingLinearSupportsArgs {
                    expected_revision, feature_id, view_id, support_ids, justification,
                })
            };
            let result = self.handle_drawing_request(request).map(|mut response| response["diagnostic"].take());
            self.inspection_plan_ui.supports.as_mut().unwrap().result = Some(result);
        }
        match self.inspection_plan_ui.supports.as_ref().and_then(|draft| draft.result.as_ref()) {
            Some(Ok(result)) => show_support_diagnostic(ui, result, angular),
            Some(Err(error)) => { ui.colored_label(egui::Color32::YELLOW, error); }
            None => {}
        }
    }
}

fn show_support_diagnostic(ui: &mut egui::Ui, result: &serde_json::Value, angular: bool) {
    let number = |value: &serde_json::Value| value.as_f64()
        .map(|number| format!("{number:.2}")).unwrap_or_else(|| "—".into());
    if angular {
        let angles = &result["candidate_angles_deg"];
        ui.label(format!("Angles possibles : {}° et {}°", number(&angles[0]), number(&angles[1])));
        let intersection = &result["intersection_pt"];
        ui.label(format!("Intersection : ({}, {}) pt PDF", number(&intersection["x"]), number(&intersection["y"])));
        ui.weak("Secteur à déterminer sur le plan.");
    } else {
        ui.label(format!("Séparation : {} pt PDF · Recouvrement : {} pt PDF",
            number(&result["normal_separation_pt"]), number(&result["overlap_length_pt"])));
    }
    ui.weak("Proposition documentaire à vérifier. Aucune mesure photo ni conformité.");
    ui.collapsing("Sources et limites du diagnostic", |ui| {
        egui::ScrollArea::vertical().id_salt("native_support_evidence").max_height(220.0).show(ui, |ui| {
            ui.monospace(serde_json::to_string_pretty(result).unwrap_or_default());
        });
    });
}

#[cfg(test)]
#[path = "inspection_plan_ui_tests.rs"]
mod tests;

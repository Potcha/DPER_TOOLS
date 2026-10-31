//! Drawing sources stay proposals; only the vision service observes photo pixels.
use crate::{
    dper_binding,
    dper_inspection::DrawingRegistrationScope,
    dper_pdf_geometry::DrawingGeometryCatalog,
    mcp_server::{DrawingRequest, InspectFeatureArgs},
    DperMeasureApp,
};
use serde_json::{json, Value};
use std::path::Path;

impl DperMeasureApp {
    /// Verify the actual file, rather than trusting a path or imported JSON hash.
    pub(crate) fn current_drawing_source(&self) -> Result<(String, String), String> {
        let plan = self
            .inspection
            .plan
            .as_ref()
            .ok_or("Aucun plan de contrôle actif.")?;
        let path = plan
            .source_file
            .as_ref()
            .ok_or("Réanalysez le fichier PDF source.")?;
        let expected = plan
            .source_fingerprint
            .as_ref()
            .ok_or("Le plan ne possède pas d'empreinte de fichier ; réanalysez le PDF.")?;
        let actual = crate::dper_source::fingerprint_file(Path::new(path))?;
        if actual != *expected {
            return Err(
                "Le document source a changé ; réanalysez le PDF puis ses associations.".into(),
            );
        }
        Ok((path.clone(), actual))
    }

    pub(crate) fn drawing_source_cache_key(&self) -> String {
        self.inspection
            .plan
            .as_ref()
            .and_then(|plan| plan.source_file.as_deref())
            .map(|path| match std::fs::metadata(path) {
                Ok(meta) => format!("{path}:{}:{:?}", meta.len(), meta.modified()),
                Err(error) => format!("{path}:{error}"),
            })
            .unwrap_or_default()
    }

    fn current_catalog(&self, page: u32) -> Result<&DrawingGeometryCatalog, String> {
        let (_, fingerprint) = self.current_drawing_source()?;
        self.drawing_geometry_cache
            .iter()
            .find(|catalog| {
                catalog.geometry.page == page && catalog.document_fingerprint == fingerprint
            })
            .ok_or_else(|| {
                "Analysez les géométries de cette page PDF dans la session courante.".into()
            })
    }

    pub(crate) fn drawing_registration_scope(
        &self,
        view_id: &str,
    ) -> Result<DrawingRegistrationScope, String> {
        let matches: Vec<_> = self
            .inspection
            .views
            .iter()
            .filter(|view| view.id == view_id)
            .collect();
        if matches.len() != 1 {
            return Err("Vue de recalage absente ou ambiguë.".into());
        }
        let view = matches[0];
        let catalog = self.current_catalog(view.page)?;
        let plan = self.inspection.plan.as_ref().unwrap();
        let checked = dper_binding::define_view(
            plan,
            &catalog.document_fingerprint,
            &catalog.geometry,
            &view.label,
            &view.feature_ids,
            &view.geometry_ids,
        )?;
        if checked.id != view.id || checked.bounds != view.bounds {
            return Err("Les sources de la vue ont changé ; redéfinissez-la.".into());
        }
        Ok(DrawingRegistrationScope {
            document_fingerprint: catalog.document_fingerprint.clone(),
            page: view.page,
            view_id: view.id.clone(),
        })
    }

    fn binding_mutation_guard(&self, revision: u64) -> Result<(), String> {
        self.ensure_mcp_revision(Some(revision))?;
        if self.mcp_mutation_blocked() {
            return Err("Terminez l'action en cours.".into());
        }
        Ok(())
    }

    pub(crate) fn validate_drawing_binding(&self, id: &str) -> Result<(), String> {
        let bindings: Vec<_> = self
            .inspection
            .bindings
            .iter()
            .filter(|binding| binding.candidate.id == id)
            .collect();
        if bindings.len() != 1 {
            return Err("Association absente ou ambiguë.".into());
        }
        let binding = bindings[0];
        let view = self
            .inspection
            .views
            .iter()
            .find(|view| view.id == binding.candidate.view_id)
            .ok_or("Vue absente.")?;
        let catalog = self.current_catalog(view.page)?;
        dper_binding::resolve_binding(
            self.inspection.plan.as_ref().unwrap(),
            &catalog.document_fingerprint,
            &catalog.geometry,
            view,
            binding,
        )
        .map(|_| ())
    }

    pub(crate) fn handle_drawing_request(
        &mut self,
        request: DrawingRequest,
    ) -> Result<Value, String> {
        match request {
            DrawingRequest::Source(args) => {
                self.binding_mutation_guard(args.expected_revision)?;
                let (path, fingerprint) = self.current_drawing_source()?;
                if !path.to_lowercase().ends_with(".pdf") {
                    return Err("Une source PDF est requise.".into());
                }
                let plan = self.inspection.plan.as_ref().unwrap();
                if args.page == 0 || !plan.page_geometry.iter().any(|page| page.page == args.page) {
                    return Err("Page absente de l'extraction textuelle PDF.".into());
                }
                Ok(json!({"path":path,"document_fingerprint":fingerprint,"page":args.page}))
            }
            DrawingRequest::InstallCatalog {
                catalog,
                expected_revision,
            } => {
                self.binding_mutation_guard(expected_revision)?;
                let (_, fingerprint) = self.current_drawing_source()?;
                if fingerprint != catalog.document_fingerprint {
                    return Err("Le PDF a changé pendant l'extraction.".into());
                }
                let page = self
                    .inspection
                    .plan
                    .as_ref()
                    .unwrap()
                    .page_geometry
                    .iter()
                    .find(|page| page.page == catalog.geometry.page)
                    .ok_or("Page PDF absente du plan.")?;
                if (page.width_pt - catalog.geometry.width_pt).abs() > 1e-5
                    || (page.height_pt - catalog.geometry.height_pt).abs() > 1e-5
                {
                    return Err("Repères PDF texte/vectoriel incompatibles.".into());
                }
                let summary = json!({"document_revision":self.document_revision,
                    "document_fingerprint":fingerprint,"page":catalog.geometry.page,
                    "primitive_count":catalog.geometry.primitives.len(),"warnings":catalog.geometry.warnings,
                    "note":"Primitives de dessin proposées, aucune mesure physique. Catalogue de session uniquement."});
                self.drawing_geometry_cache
                    .retain(|item| item.geometry.page != catalog.geometry.page);
                self.drawing_geometry_cache.push(catalog);
                if self.drawing_geometry_cache.len() > 3 {
                    self.drawing_geometry_cache.remove(0);
                }
                Ok(summary)
            }
            DrawingRequest::List(args) => {
                let catalog = self.current_catalog(args.page)?;
                let entries: Vec<_> = catalog
                    .geometry
                    .primitives
                    .iter()
                    .filter(|primitive| {
                        args.geometry_id
                            .as_ref()
                            .is_none_or(|id| id == &primitive.id)
                            && args.kind.as_ref().is_none_or(|kind| {
                                serde_json::to_value(primitive.kind)
                                    .ok()
                                    .as_ref()
                                    .and_then(Value::as_str)
                                    == Some(kind.as_str())
                            })
                    })
                    .collect();
                if args.geometry_id.is_some() && entries.len() != 1 {
                    return Err("Géométrie absente ou ambiguë.".into());
                }
                let offset = args.offset.unwrap_or(0);
                let limit = args.limit.unwrap_or(50).clamp(1, 100);
                let page_entries: Vec<_> = entries
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|primitive| {
                        if args.geometry_id.is_some() {
                            serde_json::to_value(primitive).unwrap()
                        } else {
                            json!({"id":primitive.id,"page":primitive.page,"kind":primitive.kind,
                        "bbox":primitive.bbox,"closed":primitive.closed,"circle":primitive.circle,
                        "source_element":primitive.source.element_name,"confirmed":false})
                        }
                    })
                    .collect();
                Ok(
                    json!({"document_revision":self.document_revision,"document_fingerprint":catalog.document_fingerprint,
                    "page":args.page,"total":entries.len(),"offset":offset,"items":page_entries,
                    "next_offset":(offset.saturating_add(limit)<entries.len()).then_some(offset.saturating_add(limit))}),
                )
            }
            DrawingRequest::DefineView(args) => {
                self.binding_mutation_guard(args.expected_revision)?;
                if self.inspection.views.len() >= 100 && args.replace_view_id.is_none() {
                    return Err("100 vues au maximum.".into());
                }
                if args
                    .replace_view_id
                    .as_ref()
                    .is_some_and(|id| !self.inspection.views.iter().any(|view| &view.id == id))
                {
                    return Err("Vue à remplacer absente.".into());
                }
                let catalog = self.current_catalog(args.page)?;
                let view = dper_binding::define_view(
                    self.inspection.plan.as_ref().unwrap(),
                    &catalog.document_fingerprint,
                    &catalog.geometry,
                    &args.label,
                    &args.feature_ids,
                    &args.geometry_ids,
                )?;
                if self.inspection.views.iter().any(|old| {
                    Some(&old.id) != args.replace_view_id.as_ref()
                        && old
                            .feature_ids
                            .iter()
                            .any(|id| view.feature_ids.contains(id))
                }) {
                    return Err("Une caractéristique appartient déjà à une vue ; remplacez explicitement cette vue.".into());
                }
                if let Some(id) = &args.replace_view_id {
                    self.invalidate_inspection_inputs();
                    self.inspection.views.retain(|old| &old.id != id);
                    self.inspection
                        .bindings
                        .retain(|binding| &binding.candidate.view_id != id);
                    if self
                        .inspection
                        .registration_scope
                        .as_ref()
                        .is_some_and(|scope| &scope.view_id == id)
                    {
                        self.inspection.registration = None;
                        self.inspection.registration_scope = None;
                    }
                }
                self.inspection.views.push(view.clone());
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
                Ok(json!({"document_revision":self.document_revision,"view":view}))
            }
            DrawingRequest::Candidates(args) => {
                let view = self
                    .inspection
                    .views
                    .iter()
                    .find(|view| view.id == args.view_id)
                    .ok_or("Vue absente.")?;
                let catalog = self.current_catalog(view.page)?;
                let candidates = dper_binding::binding_candidates(
                    self.inspection.plan.as_ref().unwrap(),
                    &catalog.document_fingerprint,
                    &catalog.geometry,
                    view,
                    &args.feature_id,
                )?;
                Ok(
                    json!({"document_revision":self.document_revision,"candidates":candidates,
                    "note":"Le classement de proximité ne confirme aucune association ; toutes les alternatives sont conservées."}),
                )
            }
            DrawingRequest::Select(args) => {
                self.binding_mutation_guard(args.expected_revision)?;
                let view = self
                    .inspection
                    .views
                    .iter()
                    .find(|view| view.id == args.view_id)
                    .ok_or("Vue absente.")?;
                let catalog = self.current_catalog(view.page)?;
                let candidates = dper_binding::binding_candidates(
                    self.inspection.plan.as_ref().unwrap(),
                    &catalog.document_fingerprint,
                    &catalog.geometry,
                    view,
                    &args.feature_id,
                )?;
                let selection = dper_binding::select_binding(
                    &candidates,
                    &args.candidate_id,
                    &catalog.geometry,
                    &args.justification,
                )?;
                self.inspection
                    .bindings
                    .retain(|old| old.candidate.feature_id != args.feature_id);
                self.invalidate_inspection_inputs();
                self.inspection.bindings.push(selection.clone());
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
                Ok(json!({"document_revision":self.document_revision,"binding":selection}))
            }
            DrawingRequest::InspectBound(args) => {
                let bindings: Vec<_> = self
                    .inspection
                    .bindings
                    .iter()
                    .filter(|binding| binding.candidate.feature_id == args.feature_id)
                    .collect();
                if bindings.len() != 1 {
                    return Err(
                        "Sélectionnez une association unique pour cette caractéristique.".into(),
                    );
                }
                let binding = bindings[0];
                let view = self
                    .inspection
                    .views
                    .iter()
                    .find(|view| view.id == binding.candidate.view_id)
                    .ok_or("Vue absente.")?;
                let expected_scope = self.drawing_registration_scope(&view.id)?;
                if self.inspection.registration_scope.as_ref() != Some(&expected_scope) {
                    return Err("Recalage absent ou établi pour une autre page/vue ; utilisez drawing_view_id.".into());
                }
                let catalog = self.current_catalog(view.page)?;
                let geometry = dper_binding::resolve_binding(
                    self.inspection.plan.as_ref().unwrap(),
                    &catalog.document_fingerprint,
                    &catalog.geometry,
                    view,
                    binding,
                )?;
                let binding_id = binding.candidate.id.clone();
                self.perform_inspection(
                    InspectFeatureArgs {
                        capture_id: args.capture_id,
                        expected_revision: args.expected_revision,
                        feature_id: args.feature_id,
                        group: args.group,
                        source_measurement_ids: args.source_measurement_ids,
            geometry_in_drawing: geometry,
                        options: args.options,
                        commit: args.commit,
                    },
                    Some(binding_id),
                )
            }
        }
    }
}

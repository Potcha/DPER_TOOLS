//! Drawing sources stay proposals; only the vision service observes photo pixels.
use crate::{
    dper_binding,
    dper_inspection::DrawingRegistrationScope,
    dper_pdf_geometry::DrawingGeometryCatalog,
    mcp_server::{DrawingRequest, InspectFeatureArgs, SelectDrawingSupportsArgs},
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
        let mut catalogs = self.drawing_geometry_cache.iter().filter(|catalog| {
                catalog.geometry.page == page && catalog.document_fingerprint == fingerprint
            });
        let catalog = catalogs.next().ok_or("Analysez les géométries de cette page PDF dans la session courante.")?;
        if catalogs.next().is_some() { return Err("Catalogue de page ambigu.".into()); }
        Ok(catalog)
    }

    pub(crate) fn drawing_registration_scope(
        &self,
        view_id: &str,
    ) -> Result<DrawingRegistrationScope, String> {
        let view = dper_binding::unique_view(&self.inspection.views, view_id)?;
        let catalog = self.current_catalog(view.page)?;
        let plan = self.inspection.plan.as_ref().unwrap();
        dper_binding::validate_view(
            plan,
            &catalog.document_fingerprint,
            &catalog.geometry,
            view,
        )?;
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

    fn require_unambiguous_drawing_scope(
        &self, view: &dper_binding::DrawingView, feature_id: &str, geometry_ids: &[String],
    ) -> Result<&crate::dper_drawing::DrawingCharacteristic, String> {
        let plan = self.inspection.plan.as_ref().ok_or("Aucun plan de contrôle actif.")?;
        let feature = plan.characteristics.iter().find(|feature| feature.id == feature_id)
            .ok_or("Caractéristique absente.")?;
        if feature.has_unresolved_unit_context() && feature.unit_context.as_ref().is_some_and(|context|
            context.document.status == crate::dper_drawing::DrawingUnitStatus::Conflict)
        {
            return Err("Le contexte documentaire d'unités reste contradictoire.".into());
        }
        if feature.reasons.iter().any(|reason| reason.starts_with("explicit_tolerance_unit_conflict:")) {
            return Err("Les unités de la cote et de sa tolérance sont incompatibles.".into());
        }
        if self.inspection.views.iter().any(|other| other.id != view.id
            && other.page == view.page && other.document_fingerprint == view.document_fingerprint
            && (other.feature_ids.iter().any(|id| id == feature_id)
                || geometry_ids.iter().any(|id| other.geometry_ids.contains(id))))
        {
            return Err("La cote ou un de ses traits est revendiqué par plusieurs vues. Clarifiez leur périmètre.".into());
        }
        Ok(feature)
    }

    fn build_support_association(
        &self, args: &SelectDrawingSupportsArgs,
    ) -> Result<dper_binding::SupportAssociation, String> {
        use crate::dper_drawing::DrawingDimensionKind;
        let view = dper_binding::unique_view(&self.inspection.views, &args.view_id)?;
        let catalog = self.current_catalog(view.page)?;
        let feature = self.require_unambiguous_drawing_scope(view, &args.feature_id, &args.support_ids)?;
        let plan = self.inspection.plan.as_ref().unwrap();
        let (evidence, angular_sector) = match (feature.kind, args.angular_endpoint_indices) {
            (DrawingDimensionKind::Linear, None) => {
                let diagnostic = dper_binding::drawing_linear_supports::inspect(
                    plan, &catalog.document_fingerprint, &catalog.geometry, view,
                    &args.feature_id, &args.support_ids, &args.justification,
                )?;
                (serde_json::to_value(&diagnostic), None)
            }
            (DrawingDimensionKind::Angle, Some(indices)) => {
                let diagnostic = dper_binding::drawing_angular_supports::inspect(
                    plan, &catalog.document_fingerprint, &catalog.geometry, view,
                    &args.feature_id, &args.support_ids, &args.justification,
                )?;
                let sector = diagnostic.available_sectors.iter()
                    .find(|sector| sector.endpoint_indices == indices)
                    .ok_or("Le secteur doit joindre deux extrémités distinctes du sommet.")?;
                (serde_json::to_value(&diagnostic), Some(sector.clone()))
            }
            (DrawingDimensionKind::Angle, None) => {
                return Err("Choisissez explicitement le secteur angulaire sur le plan.".into());
            }
            _ => return Err("La cote ou le choix de secteur est incompatible avec ces supports.".into()),
        };
        self.current_drawing_source()?;
        Ok(dper_binding::SupportAssociation {
            feature_id: args.feature_id.clone(),
            view_id: args.view_id.clone(),
            page: view.page,
            support_ids: args.support_ids.clone(),
            angular_sector,
            evidence: evidence.map_err(|error| error.to_string())?,
            selected_in_session: true,
        })
    }

    pub(crate) fn validate_support_association(
        &self, association: &dper_binding::SupportAssociation,
    ) -> Result<(), String> {
        if !association.selected_in_session {
            return Err("Association historique : réanalysez la page et confirmez de nouveau les supports.".into());
        }
        let current = self.build_support_association(&SelectDrawingSupportsArgs {
            expected_revision: self.document_revision,
            feature_id: association.feature_id.clone(),
            view_id: association.view_id.clone(),
            support_ids: association.support_ids.clone(),
            justification: association.evidence["justification"].as_str().unwrap_or_default().into(),
            angular_endpoint_indices: association.angular_sector.as_ref().map(|sector| sector.endpoint_indices),
        })?;
        if current.page != association.page || current.evidence != association.evidence
            || current.angular_sector != association.angular_sector
        {
            return Err("Les sources de l'association ont changé.".into());
        }
        Ok(())
    }

    pub(crate) fn propose_observed_circle_matches(
        &self,
        mut args: crate::mcp_server::ProposeCircleLandmarkMatchesArgs,
        whole_image: bool,
    ) -> Result<Value, String> {
        self.inspection_guard(&args.capture_id, args.expected_revision)?;
        if !self.inspection_frame().quality_pass {
            return Err("La qualité de la photo active doit être évaluée et exploitable.".into());
        }
        let view = dper_binding::unique_view(&self.inspection.views, &args.view_id)?;
        let catalog = self.current_catalog(view.page)?;
        let plan = self.inspection.plan.as_ref().ok_or("Aucun plan de contrôle actif.")?;
        dper_binding::validate_view(plan, &catalog.document_fingerprint, &catalog.geometry, view)?;
        let image = self.inspection_image()?;
        let segmentation_threshold = if whole_image {
            let (threshold, searches) = crate::circle_landmark_matching::find_circle_landmark_searches(&image)?;
            if searches.len() < args.plan_circle_ids.len() {
                return Err(format!("Recherche globale : {} composante(s) compatible(s) pour {} repères PDF.",
                    searches.len(), args.plan_circle_ids.len()));
            }
            args.image_circle_searches = searches;
            Some(threshold)
        } else {
            None
        };
        let mut proposal = crate::circle_landmark_matching::propose_circle_landmark_matches(
            &catalog.geometry,
            view,
            &image,
            &args.plan_circle_ids,
            &args.image_circle_searches,
            args.threshold_px,
        )?;
        // The PDF is external to the application. Recheck it after the pixel work.
        self.current_drawing_source()?;
        if whole_image {
            proposal.limitations[1] = "Recherche globale par seuillage de luminance. Les amorces proviennent de composantes isolées et le contour est ensuite ajusté sur les pixels. Les repères peu contrastés, accolés, tronqués ou non assimilables à un disque peuvent échapper à la recherche.".into();
            proposal.matching.limitations[0] = "Centres PDF issus du catalogue vectoriel de la vue. Centres photo ajustés à partir des composantes détectées dans l'image entière.".into();
        }
        let mut response = json!({
            "capture_id": args.capture_id,
            "document_revision": args.expected_revision,
            "image_fingerprint": format!("{:016x}", crate::dper_inspection::fingerprint(&image)),
            "view_id": view.id,
            "page": view.page,
            "document_fingerprint": catalog.document_fingerprint,
            "view_source_proof": view.source_proof,
            "proposal": proposal,
        });
        if let Some(threshold) = segmentation_threshold {
            response["search_mode"] = json!("whole_image");
            response["segmentation_threshold"] = json!(threshold);
        }
        Ok(response)
    }

    pub(crate) fn validate_occurrence_assignment(
        &self,
        assignment: &dper_binding::OccurrenceAssignment,
    ) -> Result<(), String> {
        if !assignment.selected_in_session {
            return Err("Affectation historique : rechargez les candidats et renouvelez le choix.".into());
        }
        let view = dper_binding::unique_view(&self.inspection.views, &assignment.view_id)?;
        let catalog = self.current_catalog(view.page)?;
        let candidate_ids = assignment.members.iter().map(|member| member.candidate_id.clone()).collect::<Vec<_>>();
        let current = dper_binding::assign_occurrences(
            self.inspection.plan.as_ref().unwrap(),
            &catalog.document_fingerprint, &catalog.geometry, view,
            &assignment.feature_id, &candidate_ids, &assignment.justification,
        )?;
        if &current != assignment {
            return Err("Les sources de l'affectation ont changé.".into());
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
        let view = dper_binding::unique_view(&self.inspection.views, &binding.candidate.view_id)?;
        self.require_unambiguous_drawing_scope(view, &binding.candidate.feature_id,
            std::slice::from_ref(&binding.candidate.geometry_id))?;
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
                self.drawing_geometry_generation = self.drawing_geometry_generation.wrapping_add(1);
                self.inspection_panel_cache = None;
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
                    for assignment in &mut self.inspection.occurrence_assignments {
                        if &assignment.view_id == id {
                            assignment.selected_in_session = false;
                        }
                    }
                    for association in &mut self.inspection.support_associations {
                        if &association.view_id == id {
                            association.selected_in_session = false;
                        }
                    }
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
                let view = dper_binding::unique_view(&self.inspection.views, &args.view_id)?;
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
            DrawingRequest::InspectLinearSupports(args) => {
                self.ensure_mcp_revision(Some(args.expected_revision))?;
                let view = dper_binding::unique_view(&self.inspection.views, &args.view_id)?;
                self.require_unambiguous_drawing_scope(view, &args.feature_id, &args.support_ids)?;
                let catalog = self.current_catalog(view.page)?;
                let diagnostic = dper_binding::drawing_linear_supports::inspect(
                    self.inspection.plan.as_ref().unwrap(),
                    &catalog.document_fingerprint, &catalog.geometry, view,
                    &args.feature_id, &args.support_ids, &args.justification,
                )?;
                Ok(json!({"document_revision":self.document_revision,"diagnostic":diagnostic}))
            }
            DrawingRequest::InspectAngularSupports(args) => {
                self.ensure_mcp_revision(Some(args.expected_revision))?;
                let view = dper_binding::unique_view(&self.inspection.views, &args.view_id)?;
                self.require_unambiguous_drawing_scope(view, &args.feature_id, &args.support_ids)?;
                let catalog = self.current_catalog(view.page)?;
                let diagnostic = dper_binding::drawing_angular_supports::inspect(
                    self.inspection.plan.as_ref().unwrap(),
                    &catalog.document_fingerprint, &catalog.geometry, view,
                    &args.feature_id, &args.support_ids, &args.justification,
                )?;
                Ok(json!({"document_revision":self.document_revision,"diagnostic":diagnostic}))
            }
            DrawingRequest::SelectSupports(args) => {
                self.binding_mutation_guard(args.expected_revision)?;
                let association = self.build_support_association(&args)?;
                self.inspection.support_associations.retain(|old| old.feature_id != args.feature_id);
                self.inspection.support_associations.push(association.clone());
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
                Ok(json!({"document_revision":self.document_revision,"association":association}))
            }
            DrawingRequest::ScopeHypotheses(args) => {
                self.ensure_mcp_revision(Some(args.expected_revision))?;
                let catalog = self.current_catalog(args.page)?;
                let plan = self.inspection.plan.as_ref().ok_or("Aucun plan de contrôle actif.")?;
                let hypotheses = crate::drawing_scope_hypotheses::inspect(plan, &catalog.geometry)?;
                self.current_drawing_source()?;
                Ok(json!({"document_revision":self.document_revision,"hypotheses":hypotheses}))
            }
            DrawingRequest::Select(args) => {
                self.binding_mutation_guard(args.expected_revision)?;
                let view = dper_binding::unique_view(&self.inspection.views, &args.view_id)?;
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
                self.require_unambiguous_drawing_scope(view, &args.feature_id,
                    std::slice::from_ref(&selection.candidate.geometry_id))?;
                self.inspection
                    .bindings
                    .retain(|old| old.candidate.feature_id != args.feature_id);
                self.invalidate_inspection_inputs();
                self.inspection.bindings.push(selection.clone());
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
                Ok(json!({"document_revision":self.document_revision,"binding":selection}))
            }
            DrawingRequest::AssignOccurrences(args) => {
                self.binding_mutation_guard(args.expected_revision)?;
                let view = dper_binding::unique_view(&self.inspection.views, &args.view_id)?;
                let catalog = self.current_catalog(view.page)?;
                let assignment = dper_binding::assign_occurrences(
                    self.inspection.plan.as_ref().unwrap(),
                    &catalog.document_fingerprint, &catalog.geometry, view,
                    &args.feature_id, &args.candidate_ids, &args.justification,
                )?;
                let previous = self.inspection.records.iter()
                    .filter(|record| record.feature_id == args.feature_id && record.occurrence.is_some())
                    .filter_map(|record| record.measurement_id).collect::<Vec<_>>();
                self.inspection.dependencies.invalidate(&self.measurements, &previous);
                self.refresh_dependency_flags();
                self.inspection.occurrence_assignments.retain(|old| old.feature_id != args.feature_id);
                self.inspection.occurrence_assignments.push(assignment.clone());
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
                Ok(json!({"document_revision":self.document_revision,"assignment":assignment,
                    "state":"documentary_proposal"}))
            }
            DrawingRequest::InspectBound(args) => {
                if let Some(occurrence_id) = &args.occurrence_id {
                    self.inspection_guard(&args.capture_id, args.expected_revision)?;
                    self.current_drawing_source()?;
                    let occurrence = crate::dper_inspection::resolve_occurrence(
                        &self.inspection, &args.feature_id, occurrence_id, &self.drawing_geometry_cache)?;
                    let geometry = occurrence.1.get(occurrence.0.member.number as usize - 1)
                        .ok_or("Position de l'occurrence invalide.")?.clone();
                    return self.perform_inspection(InspectFeatureArgs {
                        capture_id: args.capture_id, expected_revision: args.expected_revision,
                        feature_id: args.feature_id, group: args.group,
                        source_measurement_ids: args.source_measurement_ids,
                        geometry_in_drawing: geometry, options: args.options, commit: args.commit,
                    }, None, Some(occurrence));
                }
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
                let view = dper_binding::unique_view(&self.inspection.views, &binding.candidate.view_id)?;
                self.require_unambiguous_drawing_scope(view, &args.feature_id,
                    std::slice::from_ref(&binding.candidate.geometry_id))?;
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
                    None,
                )
            }
        }
    }
}

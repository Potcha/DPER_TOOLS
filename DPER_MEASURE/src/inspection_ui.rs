//! Application adapter. The application owns source pixels and native measurements.
use crate::{
    dper_inspection::{self, FrameContext},
    dper_metrology::GeometryState,
    dper_registration,
    dper_vision::InspectionGeometry,
    mcp_server::InspectionRequest,
    DperMeasureApp, Tool,
};
use eframe::egui;
use image::RgbImage;

impl DperMeasureApp {
    pub(crate) fn validate_dependency_sources(&self, capture_id: &str, ids: &[u64]) -> Result<(), String> {
        crate::measurement_graph::validate_sources(&self.measurements, capture_id, ids)?;
        if ids.iter().any(|id| self.measurements.iter().any(|measurement|
            measurement.id == *id && measurement.origin.eq_ignore_ascii_case("vision"))) {
            let current = self.current_inspection();
            for id in ids.iter().filter(|id| self.measurements.iter().any(|measurement|
                measurement.id == **id && measurement.origin.eq_ignore_ascii_case("vision"))) {
                let observed = current.records.iter().any(|record| record.measurement_id == Some(*id)
                    && !record.imported && record.outcome.blocking_reasons.iter().all(|reason|
                        matches!(reason.as_str(), "calibration_missing" | "calibration_unqualified"
                            | "uncertainty_unknown" | "tolerance_missing" | "drawing_feature_unconfirmed"
                            | "drawing_binding_unreviewed")));
                if !observed { return Err(format!("La source vision M{id} doit être observée de nouveau avant de servir de parent.")); }
            }
        }
        Ok(())
    }

    pub(crate) fn invalidate_changed_drawing_dependencies(&mut self) {
        let ids = self.inspection.records.iter().filter(|record|
            record.drawing_binding_id.is_some()
                && record.measurement_id.is_some_and(|id|
                    record.measurement_revision == Some(self.inspection.dependencies.revision(id))))
            .filter_map(|record| record.measurement_id).collect::<Vec<_>>();
        if !ids.is_empty() && self.current_drawing_source().is_err() {
            self.inspection.dependencies.invalidate(&self.measurements, &ids);
            self.refresh_dependency_flags();
            self.inspection_panel_cache = None;
            self.session_saved = false;
            self.document_revision = self.document_revision.wrapping_add(1);
        }
    }

    pub(crate) fn refresh_dependency_flags(&mut self) {
        for measurement in &mut self.measurements {
            measurement.dependency_stale = !measurement.source_measurement_ids.is_empty()
                && self.inspection.dependencies.is_stale(measurement.id);
        }
    }

    pub(crate) fn invalidate_inspection_inputs(&mut self) {
        let ids = self.measurements.iter().map(|measurement| measurement.id).collect::<Vec<_>>();
        self.inspection.dependencies.invalidate(&self.measurements, &ids);
        self.refresh_dependency_flags();
        self.inspection_panel_cache = None;
    }

    pub(crate) fn invalidate_measurement_action(&mut self, action: &crate::MeasurementHistoryAction) {
        use crate::MeasurementHistoryAction as Action;
        let ids = match action {
            Action::Added { measurement, .. } | Action::Removed { measurement, .. } => vec![measurement.id],
            Action::Cleared { measurements, .. } => measurements.iter().map(|measurement| measurement.id).collect(),
            Action::Edited { id, before, after } => {
                let material_change = before.points != after.points || before.kind != after.kind
                    || before.is_construction != after.is_construction
                    || before.capture_id != after.capture_id || before.calibration_id != after.calibration_id
                    || before.mm_per_pixel != after.mm_per_pixel || before.mm_per_pixel_x != after.mm_per_pixel_x
                    || before.mm_per_pixel_y != after.mm_per_pixel_y
                    || before.expanded_uncertainty_mm != after.expanded_uncertainty_mm
                    || before.protocol_id != after.protocol_id || before.protocol_version != after.protocol_version
                    || before.source_measurement_ids != after.source_measurement_ids;
                if material_change { vec![*id] } else { Vec::new() }
            }
        };
        self.inspection.dependencies.invalidate(&self.measurements, &ids);
        self.refresh_dependency_flags();
        self.inspection_panel_cache = None;
    }

    pub(crate) fn ensure_dependency_deletion(&self, ids: &[u64]) -> Result<(), String> {
        if let Some(child) = self.measurements.iter().find(|measurement|
            !ids.contains(&measurement.id) && measurement.source_measurement_ids.iter().any(|id| ids.contains(id))) {
            return Err(format!("La mesure M{} dépend de cette construction/mesure ; supprimez d'abord ses descendants.", child.id));
        }
        Ok(())
    }

    pub(crate) fn install_inspection_plan(
        &mut self,
        plan: crate::dper_drawing::DrawingControlPlan,
    ) {
        self.invalidate_inspection_inputs();
        let dependencies = std::mem::take(&mut self.inspection.dependencies);
        self.inspection = dper_inspection::InspectionWorkspace {
            dependencies,
            plan: Some(plan),
            ..Default::default()
        };
        self.inspection_panel_cache = None;
        self.drawing_geometry_cache.clear();
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
    }
    fn inspection_image(&self) -> Result<RgbImage, String> {
        let source = self.image.as_ref().ok_or("Aucune image active.")?;
        RgbImage::from_raw(source.width, source.height, source.rgb.clone())
            .ok_or_else(|| "Tampon image invalide.".into())
    }

    fn inspection_frame(&self) -> FrameContext {
        FrameContext {
            capture_id: self.capture_id.clone(),
            calibration: self.calibration.clone(),
            quality_pass: self
                .quality_assessment
                .as_ref()
                .is_some_and(|quality| quality.is_exploitable),
        }
    }

    fn inspection_guard(&self, capture_id: &str, revision: u64) -> Result<(), String> {
        self.ensure_mcp_revision(Some(revision))?;
        if self.capture_id != capture_id {
            return Err("La capture a changé.".into());
        }
        if !self.image_is_editable() || self.mcp_mutation_blocked() {
            return Err("Figez l'image et terminez le tracé en cours avant l'inspection.".into());
        }
        Ok(())
    }

    pub(crate) fn current_inspection(&self) -> crate::dper_inspection::InspectionWorkspace {
        let mut state = self.inspection.clone();
        if let Ok(image) = self.inspection_image() {
            let frame = self.inspection_frame();
            let source_fingerprint = dper_inspection::fingerprint(&image);
            for record in &mut state.records {
                record.outcome = dper_inspection::assess_record_with_fingerprint(
                    record,
                    &self.inspection,
                    &image,
                    &frame,
                    &self.measurements,
                    source_fingerprint,
                );
            }
        } else {
            for record in &mut state.records {
                record.outcome.geometry_state = GeometryState::Proposed;
                record.outcome.conformity_status =
                    crate::dper_metrology::ConformityStatus::Indeterminate;
                record
                    .outcome
                    .blocking_reasons
                    .push("source_image_missing".into());
            }
        }
        if state
            .records
            .iter()
            .any(|record| record.drawing_binding_id.is_some())
            && self.current_drawing_source().is_err()
        {
            for record in state
                .records
                .iter_mut()
                .filter(|record| record.drawing_binding_id.is_some())
            {
                if record.outcome.geometry_state == GeometryState::Validated {
                    record.outcome.geometry_state = GeometryState::Measured;
                }
                record.outcome.conformity_status =
                    crate::dper_metrology::ConformityStatus::Indeterminate;
                record
                    .outcome
                    .blocking_reasons
                    .push("drawing_source_changed".into());
            }
        }
        state
    }

    pub(crate) fn handle_inspection_request(
        &mut self,
        request: InspectionRequest,
    ) -> Result<serde_json::Value, String> {
        match request {
            InspectionRequest::Drawing(request) => self.handle_drawing_request(request),
            InspectionRequest::ProposeInterpretation(args) => {
                self.ensure_mcp_revision(Some(args.expected_revision))?;
                if self.mcp_mutation_blocked() {
                    return Err("Terminez l'action en cours.".into());
                }
                let plan = self
                    .inspection
                    .plan
                    .as_mut()
                    .ok_or("Aucun plan de contrôle actif.")?;
                crate::dper_drawing::propose_interpretation(
                    plan,
                    &args.feature_id,
                    args.kind,
                    args.unit,
                    &args.source_note,
                )?;
                self.invalidate_inspection_inputs();
                self.inspection.registration = None;
                self.inspection.registration_scope = None;
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
                self.handle_inspection_request(InspectionRequest::GetState)
            }
            InspectionRequest::GetState => Ok(serde_json::json!({
                "capture_id": self.capture_id, "document_revision": self.document_revision,
                "inspection": self.current_inspection(),
                "limitations": ["Assisted landmark registration; automatic drawing-to-photo matching is not implemented.",
                    "Line and arc endpoints are not measured extents.",
                    "Software contour validation is distinct from physical qualification and conformity."]
            })),
            InspectionRequest::InstallPlan {
                plan,
                expected_revision,
            } => {
                self.ensure_mcp_revision(Some(expected_revision))?;
                if self.mcp_mutation_blocked() {
                    return Err("Terminez l'action en cours.".into());
                }
                self.install_inspection_plan(plan);
                // A new plan must not leave associations to the previous drawing active.
                self.reference_document = None;
                self.handle_inspection_request(InspectionRequest::GetState)
            }
            InspectionRequest::Register(args) => {
                self.inspection_guard(&args.capture_id, args.expected_revision)?;
                if self.inspection.plan.is_none() {
                    return Err("Importez d'abord un plan de contrôle.".into());
                }
                if args.correspondences.len() > 64 {
                    return Err("64 repères au maximum.".into());
                }
                let image = self.inspection_image()?;
                for pair in &args.correspondences {
                    if pair.image.x < 0.0
                        || pair.image.y < 0.0
                        || pair.image.x >= image.width() as f64
                        || pair.image.y >= image.height() as f64
                    {
                        return Err("Un repère sort de l'image source.".into());
                    }
                }
                let report = dper_registration::register_similarity(
                    &args.correspondences,
                    args.maximum_error_px,
                )?;
                let scope = args
                    .drawing_view_id
                    .as_deref()
                    .map(|id| self.drawing_registration_scope(id))
                    .transpose()?;
                if let Some(scope) = &scope {
                    let bounds = self
                        .inspection
                        .views
                        .iter()
                        .find(|view| view.id == scope.view_id)
                        .unwrap()
                        .bounds;
                    if args.correspondences.iter().any(|pair| {
                        pair.plan.x < bounds.x_min
                            || pair.plan.x > bounds.x_max
                            || pair.plan.y < bounds.y_min
                            || pair.plan.y > bounds.y_max
                    }) {
                        return Err("Un repère plan sort de la vue PDF sélectionnée.".into());
                    }
                }
                self.invalidate_inspection_inputs();
                self.inspection.registration = Some(report);
                self.inspection.registration_scope = scope;
                self.inspection.registration_capture_id = Some(self.capture_id.clone());
                self.inspection.registration_fingerprint =
                    Some(dper_inspection::fingerprint(&image));
                self.inspection.records.clear();
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
                self.handle_inspection_request(InspectionRequest::GetState)
            }
            InspectionRequest::Inspect(args) => self.perform_inspection(args, None),
        }
    }

    pub(crate) fn perform_inspection(
        &mut self,
        args: crate::mcp_server::InspectFeatureArgs,
        drawing_binding_id: Option<String>,
    ) -> Result<serde_json::Value, String> {
        self.inspection_guard(&args.capture_id, args.expected_revision)?;
        if drawing_binding_id.is_none()
            && self
                .inspection
                .bindings
                .iter()
                .any(|binding| binding.candidate.feature_id == args.feature_id)
        {
            return Err("Cette caractéristique possède une association au plan ; utilisez inspect_bound_feature.".into());
        }
        self.validate_dependency_sources(&args.capture_id, &args.source_measurement_ids)?;
        let image = self.inspection_image()?;
        let frame = self.inspection_frame();
        let mut record = dper_inspection::inspect_feature(
            &self.inspection,
            &image,
            &frame,
            &args.feature_id,
            &args.group,
            &args.geometry_in_drawing,
            &args.options.unwrap_or_default(),
        )?;
        record.drawing_binding_id = drawing_binding_id;
        if args.commit {
            if matches!(record.result.geometry, InspectionGeometry::Line { .. }) {
                return Err("Cette méthode ajuste le support de la droite, sans mesurer ses extrémités. L'engagement d'une longueur est suspendu.".into());
            }
            // Retain the robust observed fit exactly; contour samples stay in evidence.
            let label = format!("{} · {}", record.group, record.feature_id);
            let measurement = self.add_measurement_with_construction(Tool::Circle, record.measurement_points.clone(), Some(label),
            self.capture_id.clone(), frame.calibration.as_ref().map(|profile| profile.id.clone()), self.mm_per_pixel,
            "vision".into(), Some("Directional subpixel contour; robust fit; software evidence in inspection state; physical qualification pending.".into()), false, args.source_measurement_ids);
            record.measurement_revision = Some(self.inspection.dependencies.revision(measurement.id));
            record.measurement_id = Some(measurement.id);
            record.outcome = dper_inspection::assess_record(
                &record,
                &self.inspection,
                &image,
                &frame,
                &self.measurements,
            );
        }
        record.outcome = dper_inspection::assess_record(
            &record,
            &self.inspection,
            &image,
            &frame,
            &self.measurements,
        );
        // Retain bounded history without silently deleting native measurements.
        self.inspection.records.push(record.clone());
        if self.inspection.records.len() > 1000 {
            self.inspection.records.remove(0);
        }
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        Ok(serde_json::json!({"document_revision": self.document_revision, "record": record}))
    }

    pub(crate) fn inspection_audit_notes(&self) -> String {
        let state = self.current_inspection();
        if state.records.is_empty() {
            return String::new();
        }
        let mut lines = vec!["INSPECTION AUTOMATISEE - validation logicielle des contours; qualification physique et U de mesure requises pour la conformite.".to_owned()];
        for record in state.records {
            let object = record
                .measurement_id
                .map(|id| format!("M{id}"))
                .unwrap_or_else(|| "Aperçu".into());
            let value = record
                .value_mm
                .map(|value| format!("{value:.4} mm"))
                .unwrap_or_else(|| "valeur non établie".into());
            lines.push(format!(
                "{} / {} / {} : {}; géométrie {}; conformité {}; {}",
                record.group,
                record.feature_id,
                object,
                value,
                record.outcome.geometry_state.label(),
                record.outcome.conformity_status.label(),
                record
                    .outcome
                    .blocking_reasons
                    .iter()
                    .map(|reason| crate::dper_metrology::reason_label(reason))
                    .collect::<Vec<_>>()
                    .join(" ; ")
            ));
        }
        lines.join("\n")
    }

    pub(crate) fn automated_inspection_panel(&mut self, ui: &mut egui::Ui) {
        self.drawing_geometry_panel(ui);
        self.registration_editor_panel(ui);
        if self.inspection.plan.is_none() {
            return;
        }
        if self
            .inspection_panel_cache
            .as_ref()
            .is_none_or(|(revision, source_key, _)| {
                *revision != self.document_revision
                    || source_key != &self.drawing_source_cache_key()
            })
        {
            self.invalidate_changed_drawing_dependencies();
            self.inspection_panel_cache = Some((
                self.document_revision,
                self.drawing_source_cache_key(),
                self.current_inspection(),
            ));
        }
        let state = self.inspection_panel_cache.as_ref().unwrap().2.clone();
        egui::CollapsingHeader::new("Plan de contrôle et inspection IA")
            .default_open(true)
            .show(ui, |ui| {
                if let Some(plan) = &state.plan {
                    ui.label(format!(
                        "{} · {} caractéristiques",
                        plan.source_name,
                        plan.characteristics.len()
                    ));
                    ui.label(match state.registration.as_ref() {
                        Some(report) if report.correspondences.is_empty() => "Recalage historique : repères non conservés",
                        Some(_) => "Recalage assisté établi",
                        None => "Repères de recalage à définir dans l'éditeur ou via MCP",
                    });
                    if let Some(registration) = &state.registration {
                        ui.collapsing("Repères et résidus du recalage", |ui| {
                            if registration.correspondences.is_empty() {
                                ui.label("Recalage historique : coordonnées des repères non conservées. Refaites le recalage pour en conserver la trace.");
                            } else {
                                ui.label(format!("{} repères retenus sur {} · RMS {:.3} px · maximum retenu {:.3} px",
                                    registration.inlier_indices.len(), registration.correspondences.len(),
                                    registration.rms_error_px, registration.max_error_px));
                                if let Some(threshold) = registration.threshold_px {
                                    ui.label(format!("Seuil de sélection : {threshold:.3} px"));
                                }
                                ui.label("Plan : repère du dessin. Image et résidus : pixels natifs. L'échelle du recalage ne remplace pas l'étalonnage.");
                                for (index, pair) in registration.correspondences.iter().enumerate() {
                                    let status = if registration.inlier_indices.contains(&index) { "Retenu" } else { "Rejeté" };
                                    let residual = registration.residuals_px.get(index)
                                        .map(|value| format!("{value:.3} px")).unwrap_or_else(|| "inconnu".into());
                                    ui.label(format!("{} · {status} · plan ({:.3}, {:.3}) → image ({:.3}, {:.3}) · résidu {residual}",
                                        index + 1, pair.plan.x, pair.plan.y, pair.image.x, pair.image.y));
                                }
                            }
                        });
                    }
                    for binding in &state.bindings {
                        ui.collapsing(format!("Association · {}", binding.candidate.feature_id), |ui| {
                            ui.label(format!("Page {} · géométrie {}", binding.candidate.page, binding.candidate.geometry_id));
                            ui.label(&binding.justification);
                            ui.label("Le contour du plan localise la recherche. La mesure provient uniquement de la photo.");
                            let mut reviewed = binding.reviewed;
                            if ui.checkbox(&mut reviewed, "Association cote / géométrie vérifiée sur le plan").changed() {
                                let current = self.validate_drawing_binding(&binding.candidate.id);
                                match current {
                                    Ok(()) => {
                                        if let Some(selection) = self.inspection.bindings.iter_mut().find(|item| item.candidate.id == binding.candidate.id) {
                                            selection.reviewed = reviewed;
                                        }
                                        self.session_saved = false;
                                        self.document_revision = self.document_revision.wrapping_add(1);
                                    }
                                    Err(error) => self.message = error,
                                }
                            }
                        });
                    }
                    for candidate in &plan.characteristics {
                        ui.collapsing(format!("{} · {}", candidate.id, candidate.label), |ui| {
                            ui.label(format!(
                                "Nominal : {:?} {:?} · {:?}",
                                candidate.nominal, candidate.unit, candidate.measurability
                            ));
                            ui.label(&candidate.source.exact_text);
                            use crate::dper_drawing::{DrawingDimensionKind as K, DrawingUnit as U};
                            let mut kind = candidate.kind;
                            let mut unit = candidate.unit;
                            egui::ComboBox::from_id_salt(("inspection_kind", &candidate.id)).selected_text(format!("Type : {kind:?}")).show_ui(ui, |ui| {
                                for (label, value) in [("Distance", K::Linear), ("Diamètre", K::Diameter), ("Rayon", K::Radius), ("Angle", K::Angle), ("Profondeur", K::Depth), ("Filetage", K::Thread), ("Tolérance géométrique", K::Geometric), ("À identifier", K::Unknown)] { ui.selectable_value(&mut kind, value, label); }
                            });
                            egui::ComboBox::from_id_salt(("inspection_unit", &candidate.id)).selected_text(format!("Unité : {unit:?}")).show_ui(ui, |ui| {
                                for (label, value) in [("Inconnue", None), ("mm", Some(U::Millimeter)), ("cm", Some(U::Centimeter)), ("m", Some(U::Meter)), ("pouce", Some(U::Inch)), ("degré", Some(U::Degree))] { ui.selectable_value(&mut unit, value, label); }
                            });
                            if kind != candidate.kind || unit != candidate.unit {
                                if let Some(plan) = self.inspection.plan.as_mut() {
                                    match crate::dper_drawing::propose_interpretation(plan, &candidate.id, kind, unit, "Correction manuelle du type ou de l'unité dans le panneau du plan de contrôle.") {
                                        Ok(()) => { self.invalidate_inspection_inputs(); self.inspection.registration = None; self.session_saved = false; self.document_revision = self.document_revision.wrapping_add(1); },
                                        Err(error) => self.message = error,
                                    }
                                }
                            }
                            for note in &candidate.interpretation_notes { ui.label(note); }
                            for reason in &candidate.reasons {
                                ui.label(reason);
                            }
                        let mut confirmed = candidate.confirmed;
                        if ui.checkbox(&mut confirmed, "Interprétation du plan vérifiée").changed() {
                            self.invalidate_inspection_inputs();
                            if let Some(plan) = self.inspection.plan.as_mut() {
                                if let Some(item) = plan.characteristics.iter_mut().find(|item| item.id == candidate.id) { item.confirmed = confirmed; }
                            }
                            self.session_saved = false;
                            self.document_revision = self.document_revision.wrapping_add(1);
                        }
                        });
                    }
                }
                for record in &state.records {
                    ui.separator();
                    ui.label(format!("{} · {}", record.group, record.feature_id));
                    ui.label(match record.value_mm {
                        Some(value) => format!("Mesuré : {value:.4} mm"),
                        None => "Valeur dimensionnelle non établie".into(),
                    });
                    let geometry_label = match record.outcome.geometry_state { GeometryState::Proposed => "proposés", GeometryState::Measured => "mesurés, à vérifier", GeometryState::Validated => "vérifiés par logiciel" };
                    let conformity_label = match record.outcome.conformity_status { crate::dper_metrology::ConformityStatus::Conforming => "conforme", crate::dper_metrology::ConformityStatus::Nonconforming => "hors tolérance", crate::dper_metrology::ConformityStatus::Indeterminate => "indéterminée", crate::dper_metrology::ConformityStatus::NotApplicable => "non évaluée" };
                    ui.label(format!("Contours : {geometry_label} · Conformité : {conformity_label}"));
                    if let Some(id) = record.measurement_id {
                        if ui
                            .button(format!("Sélectionner M{id} pour correction"))
                            .clicked()
                        {
                            self.selected_measurement = Some(id);
                        }
                    }
                    ui.collapsing("Preuves et limites", |ui| {
                        ui.label(format!(
                            "{} points · couverture {:.0} % · RMS {:.3} px",
                            record.result.evidence.accepted_points.len(),
                            record.result.evidence.coverage_fraction * 100.0,
                            record.result.evidence.rms_residual_px
                        ));
                        for reason in &record.outcome.blocking_reasons {
                            ui.label(crate::dper_metrology::reason_label(reason));
                        }
                    });
                }
            });
    }

    pub(crate) fn paint_inspection_evidence(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        width: f32,
        height: f32,
    ) {
        let Some((revision, source_key, state)) = &self.inspection_panel_cache else {
            return;
        };
        if *revision != self.document_revision || source_key != &self.drawing_source_cache_key() {
            return;
        }
        if let Some(record) = state.records.last() {
            if record.capture_id != self.capture_id || record.imported {
                return;
            }
            if record
                .outcome
                .blocking_reasons
                .iter()
                .any(|reason| reason.contains("changed") || reason.contains("stale"))
            {
                return;
            }
            for (points, color) in [
                (
                    &record.result.evidence.accepted_points,
                    egui::Color32::LIGHT_GREEN,
                ),
                (
                    &record.result.evidence.rejected_points,
                    egui::Color32::LIGHT_RED,
                ),
            ] {
                for point in points {
                    painter.circle_filled(
                        Self::point_to_screen(*point, rect, width, height),
                        2.0,
                        color,
                    );
                }
            }
        }
    }
}

//! Original workspace actions recovered from the pre-sync backup.
use super::*;
impl DperMeasureApp {
    pub(super) fn record_display_correction_change(&mut self, before: DisplayCorrection, dragging: bool) {
        let after = self.display_correction;
        if before == after || self.image.is_none() { return; }
        let merged = if self.display_correction_drag_active {
            if let Some(MeasurementHistoryAction::DisplayAdjusted { after: previous_after, .. }) = self.undo_history.last_mut() {
                *previous_after = after;
                true
            } else { false }
        } else { false };
        if !merged { self.undo_history.push(MeasurementHistoryAction::DisplayAdjusted { before, after }); }
        if matches!(self.undo_history.last(), Some(MeasurementHistoryAction::DisplayAdjusted { before, after }) if before == after) {
            self.undo_history.pop();
        }
        self.redo_history.clear();
        self.display_correction_drag_active = dragging;
        self.display_history_priority = true;
    }

    pub(super) fn refresh_changed_display(&mut self, ctx: &egui::Context) {
        if self.display_texture_dirty {
            self.refresh_display_texture(ctx);
            self.assess_current_quality();
            self.display_texture_dirty = false;
        }
    }

    pub(super) fn close_loaded_image(&mut self) {
        if self.image.is_none() {
            self.message = "Aucune image chargée à fermer.".into();
            return;
        }
        if self.capture_timer.is_some() || self.timelapse_capture.is_some() || self.video_recording.is_some()
            || self.export_dialog.is_some()
        {
            self.message = "Terminez la capture, l’enregistrement ou l’export avant de fermer l’image.".into();
            return;
        }
        if !self.session_saved || self.has_unfinished_image_action() {
            self.confirm_close_image = true;
            return;
        }
        self.unload_current_image();
    }

    pub(super) fn has_unfinished_image_action(&self) -> bool {
        !self.pending.is_empty() || self.calibration_mode || self.measurement_gesture_active()
            || self.annotation_mode || !self.annotation_points.is_empty()
    }

    pub(super) fn show_close_image_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_close_image { return; }
        if self.image.is_none() {
            self.confirm_close_image = false;
            return;
        }
        let unfinished = self.has_unfinished_image_action();
        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::new("close-image-confirmation"))
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                ui.heading("Fermer l’image ?");
                ui.label("Les modifications de cette image ne sont pas enregistrées.");
                if unfinished {
                    ui.label("Un tracé, un étalonnage ou une annotation est en cours. Vous pouvez revenir le terminer, ou fermer sans enregistrer.");
                }
                ui.label("Le fichier image d’origine restera sur le disque.");
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    save = ui.add_enabled(!unfinished, egui::Button::new("Enregistrer puis fermer…"))
                        .on_disabled_hover_text("Terminez ou annulez le tracé en cours avant d’enregistrer le projet.")
                        .clicked();
                    discard = ui.button(egui::RichText::new("Fermer sans enregistrer")
                        .color(Color32::from_rgb(187, 42, 57))).clicked();
                    let cancel_button = ui.button("Annuler");
                    cancel = cancel_button.clicked();
                });
            });
        if cancel || response.should_close() {
            self.confirm_close_image = false;
        } else if discard {
            self.unload_current_image();
        } else if save && self.save_project_with_result() {
            self.unload_current_image();
        }
    }

    pub(super) fn unload_current_image(&mut self) {
        self.pattern_dialog = None;
        if let Some(document) = self.reference_document.as_mut() {
            for dimension in &mut document.dimensions {
                dimension.measurement_id = None;
            }
        }

        self.image = None;
        self.texture = None;
        self.image_is_imported = false;
        self.active_device = "Aucune image".into();
        self.active_device_id = "no-image".into();
        self.running = false;
        self.paused = true;
        self.camera_task = None;
        self.measurements.clear();
        self.annotations.clear();
        self.measurement_audit_history.clear();
        self.undo_history.clear();
        self.redo_history.clear();
        self.point_edit = None;
        self.display_texture_dirty = false;
        self.display_correction_drag_active = false;
        self.display_history_priority = false;
        self.clear_pending();
        self.set_measurements_selection(Vec::new());
        self.selection_mode = false;
        self.calibration_mode = false;
        self.annotation_mode = false;
        self.annotation_points.clear();
        self.calibration = None;
        self.calibration_history.clear();
        self.mm_per_pixel = None;
        self.expanded_uncertainty_mm.clear();
        self.quality_assessment = None;
        self.right_panel_annotations = false;
        self.zoom = 1.0;
        self.zoom_percent = 100.0;
        self.fit_view = true;
        self.pan = Vec2::ZERO;
        self.capture_id = new_capture_id();
        self.document_revision = self.document_revision.wrapping_add(1);
        // The image session has been saved or explicitly discarded. Its former dirty
        // flag must not block opening another project after the image is closed.
        self.session_saved = true;
        self.confirm_close_image = false;
        self.message = "Image fermée dans DPER_MEASURE. Le fichier source sur le disque n'a pas été supprimé.".into();
    }

    pub(super) fn add_measurement_with_style(
        &mut self, kind: Tool, points: Vec<Point>, label: Option<String>, capture_id: String,
        calibration_id: Option<String>, mm_per_pixel: Option<f64>, origin: String,
        method: Option<String>, is_construction: bool, drawing_variant: Option<String>,
    ) -> Measurement {
        self.add_measurement_with_style_sources(kind, points, label, capture_id, calibration_id,
            mm_per_pixel, origin, method, is_construction, drawing_variant, Vec::new())
    }

    pub(super) fn add_measurement_with_style_sources(
        &mut self, kind: Tool, points: Vec<Point>, label: Option<String>, capture_id: String,
        calibration_id: Option<String>, mm_per_pixel: Option<f64>, origin: String,
        method: Option<String>, is_construction: bool, drawing_variant: Option<String>,
        source_measurement_ids: Vec<u64>,
    ) -> Measurement {
        let id = self.next_id;
        let expanded_uncertainty_mm = (origin != "vision").then_some(calibration_id.as_deref()).flatten()
            .and_then(|id| self.calibration_history.iter().find(|profile| profile.id == id)
                .and_then(|profile| profile.expanded_uncertainty_mm));
        let calibration_profile = calibration_id.as_deref().and_then(|calibration_id| self.calibration_history.iter()
            .find(|profile| profile.id == calibration_id));
        let mm_per_pixel_x = calibration_profile.and_then(|profile| profile.scale_x_mm_per_pixel).or(mm_per_pixel);
        let mm_per_pixel_y = calibration_profile.and_then(|profile| profile.scale_y_mm_per_pixel).or(mm_per_pixel);
        let protocol_id = self.active_protocol.as_ref().map(|protocol| protocol.id.clone());
        let protocol_version = self.active_protocol.as_ref().map(|protocol| protocol.version);
        let measurement = Measurement {
            id,
            kind,
            is_construction,
            drawing_variant,
            points,
            label: label.unwrap_or_else(|| format!("M{id}")),
            label_position_native_px: None,
            capture_id: capture_id.clone(),
            calibration_id,
            mm_per_pixel,
            mm_per_pixel_x,
            mm_per_pixel_y,
            expanded_uncertainty_mm,
            origin,
            method,
            protocol_id,
            protocol_version,
            source_measurement_ids, dependency_stale: false,
        };
        let exceeds_image = self.image.as_ref().is_some_and(|image| {
            geometry::exceeds_image_bounds(measurement.kind, &measurement.points, image.width, image.height)
        });
        self.next_id += 1;
        let previous_selection = self.selected_measurement;
        let index = self.measurements.len();
        self.measurements.push(measurement.clone());
        self.set_measurement_selection(Some(id));
        self.record_measurement_action(MeasurementHistoryAction::Added {
            measurement: measurement.clone(),
            index,
            previous_selection,
        });
        self.inspection.dependencies.mark_created(id);
        self.refresh_dependency_flags();
        if exceeds_image {
            self.message = "Attention : la géométrie dépasse le bord de l'image source.".into();
        } else {
            self.message = format!("Mesure {} ajoutée à la capture {}", measurement.label, capture_id);
        }
        measurement
    }

    pub(super) fn save_project_with_result(&mut self) -> bool {
        if !self.pending.is_empty() {
            self.message = "Terminez ou annulez le tracé en cours avant d'enregistrer le projet.".into();
            return false;
        }
        let Some(mut project_path) = rfd::FileDialog::new()
            .set_file_name("inspection.dperproj")
            .add_filter("Projet DPER_MEASURE", &["dperproj"])
            .save_file() else { return false; };
        if project_path.extension().and_then(|ext| ext.to_str())
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("dperproj"))
        {
            project_path.set_extension("dperproj");
        }
        match self.save_project_to_path(&project_path, true) {
            Ok(path) => {
                self.message = format!("Projet enregistré avec sa capture source : {path}");
                true
            }
            Err(error) => {
                self.message = format!("Erreur d'enregistrement du projet : {error}");
                false
            }
        }
    }

    pub(super) fn annotation_target_anchor(&self, annotation: &Annotation) -> Option<Point> {
        match &annotation.target {
            AnnotationTarget::Measurement { measurement_id } => self.measurements.iter()
                .find(|measurement| measurement.id == *measurement_id)
                .and_then(|measurement| {
                    if measurement.points.is_empty() { return None; }
                    let count = measurement.points.len() as f64;
                    Some(Point {
                        x: measurement.points.iter().map(|point| point.x).sum::<f64>() / count,
                        y: measurement.points.iter().map(|point| point.y).sum::<f64>() / count,
                    })
                }),
            AnnotationTarget::Point { point } => Some(*point),
            AnnotationTarget::Zone { first, second } => Some(Point {
                x: (first.x + second.x) * 0.5,
                y: (first.y + second.y) * 0.5,
            }),
        }
    }

    pub(super) fn annotation_visuals(&self, painter: &egui::Painter, image_rect: Rect, w: f32, h: f32)
        -> Vec<AnnotationVisual> {
        let mut occupied = Vec::<Rect>::new();
        for measurement in &self.measurements {
            if let Some(label_rect) = self.measurement_label_card_rect(painter, image_rect, w, h, measurement) {
                occupied.push(label_rect.expand(3.0));
            }
            let source_points = geometry::sampled_curve(measurement.kind, &measurement.points, 96)
                .unwrap_or_else(|| measurement.points.clone());
            let points = source_points.iter()
                .map(|point| Self::point_to_screen(*point, image_rect, w, h))
                .collect::<Vec<_>>();
            for segment in points.windows(2) {
                occupied.push(Rect::from_two_pos(segment[0], segment[1]).expand(5.0));
            }
        }

        let mut visuals = Vec::new();
        for (index, annotation) in self.annotations.iter().enumerate() {
            let anchor = self.annotation_target_anchor(annotation);
            let Some(anchor) = anchor else { continue; };
            let target = Self::point_to_screen(anchor, image_rect, w, h);
            if !image_rect.contains(target) { continue; }

            let badge_size = Vec2::new(30.0, 20.0);
            let mut badge = None;
            'rings: for radius in [16.0_f32, 26.0, 38.0, 52.0, 68.0, 88.0] {
                for direction in 0..16 {
                    let angle = direction as f32 * std::f32::consts::TAU / 16.0;
                    let center = target + Vec2::new(angle.cos(), angle.sin()) * radius;
                    let candidate = Rect::from_center_size(center, badge_size);
                    if candidate.left() < image_rect.left() || candidate.right() > image_rect.right()
                        || candidate.top() < image_rect.top() || candidate.bottom() > image_rect.bottom()
                        || occupied.iter().any(|rect| rect.intersects(candidate))
                    {
                        continue;
                    }
                    badge = Some(candidate);
                    break 'rings;
                }
            }

            if let Some(badge_rect) = badge { occupied.push(badge_rect.expand(3.0)); }
            visuals.push(AnnotationVisual { id: annotation.id.clone(), index, target, badge });
        }
        visuals
    }

    pub(super) fn annotation_at(&self, pointer: Pos2, rect: Rect, image_w: f32, image_h: f32,
                     painter: &egui::Painter) -> Option<String> {
        self.annotation_visuals(painter, rect, image_w, image_h).into_iter().rev()
            .find(|visual| {
                visual.badge.is_some_and(|badge|badge.contains(pointer))
                    || visual.target.distance(pointer)<=10.0
                    || visual.badge.is_some_and(|badge|
                        Self::distance_to_segment(pointer,visual.target,badge.center())<=6.0)
            })
            .map(|visual|visual.id)
    }

    pub(super) fn distance_to_segment(point: Pos2, first: Pos2, second: Pos2) -> f32 {
        let segment=second-first;
        let length_sq=segment.length_sq();
        let along=if length_sq>f32::EPSILON {((point-first).dot(segment)/length_sq).clamp(0.0,1.0)} else {0.0};
        point.distance(first+segment*along)
    }

    pub(super) fn annotation_label_bounds(&self, painter: &egui::Painter, rect: Rect, image_w: f32, image_h: f32)
        -> Vec<(String, [f64; 4])> {
        self.annotation_visuals(painter, rect, image_w, image_h).into_iter().filter_map(|visual| {
            let badge=visual.badge?;
            let a=Self::point_from_screen(badge.min,rect,image_w,image_h);
            let b=Self::point_from_screen(badge.max,rect,image_w,image_h);
            Some((visual.id,[a.x,a.y,b.x,b.y]))
        }).collect()
    }

    pub(super) fn annotation_connector_segments(&self, painter: &egui::Painter, rect: Rect, image_w: f32, image_h: f32)
        -> Vec<(String, Point, Point)> {
        self.annotation_visuals(painter, rect, image_w, image_h).into_iter().filter_map(|visual| {
            let badge=visual.badge?;
            Some((visual.id,Self::point_from_screen(visual.target,rect,image_w,image_h),
                Self::point_from_screen(badge.center(),rect,image_w,image_h)))
        }).collect()
    }

    pub(super) fn choose_measurement_tool(&mut self, tool: Tool) {
        if self.registration_photo_pick_owns_frame() || self.video_recording.is_some()
            || self.timelapse_capture.is_some() {
            self.message = "Terminez la sélection ou l’acquisition avant de choisir un outil.".into();
            return;
        }
        self.cancel_selection_gesture();
        self.pan_mode = false;
        if self.calibration_mode {
            self.message = "Terminez ou annulez l’étalonnage dans la barre de commandes avant de choisir un outil de mesure.".into();
        } else if self.pending.is_empty() {
            self.annotation_mode = false;
            self.annotation_points.clear();
            self.calibration_mode = false;
            self.selection_mode = false;
            self.mode = tool;
            self.drawing_variant = drawing_variants::DrawingVariant::for_tool(tool);
        } else {
            self.message = "Terminez ou annulez le tracé en cours avant de changer d'outil.".into();
        }
    }

    pub(super) fn start_calibration_mode(&mut self) {
        if self.registration_photo_pick_owns_frame() || self.video_recording.is_some()
            || self.timelapse_capture.is_some() {
            self.message = "Terminez la sélection ou l’acquisition avant l’étalonnage.".into();
            return;
        }
        self.pan_mode = false;
        if !self.pending.is_empty() || self.measurement_gesture_active() {
            self.message = "Terminez ou annulez l’action en cours avant l’étalonnage.".into();
            return;
        }
        if self.image.is_none() {
            self.message = "Ouvrez une image ou démarrez un flux avant l’étalonnage.".into();
            return;
        }
        self.freeze_capture();
        self.selection_mode = false;
        self.annotation_mode = false;
        self.calibration_mode = true;
        self.clear_pending();
        self.message = format!("Étalonnage · {}. {}", self.calibration_type.label(), self.calibration_type.instruction());
    }

    pub(super) fn cancel_calibration_mode(&mut self) {
        self.calibration_mode = false;
        self.clear_pending();
        self.pending_uncertainty_budget = None;
        self.selection_mode = true;
        self.message = "Étalonnage annulé ; mode sélection actif.".into();
    }
}

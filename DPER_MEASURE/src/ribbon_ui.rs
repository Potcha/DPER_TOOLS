//! DPER-specific ribbon composition and dispatch to existing application actions.
use super::*;
use crate::ribbon::{Command as C, Group as G};

#[derive(Clone, Copy)]
enum Action {
    RefreshCamera, Stream, CameraSettings, ApplyCameraMode,
    Photo, Video, TimeLapse, OpenImage, OpenProject, SearchProjects, Save, CloseImage,
    StartCalibration, CalibrationMethod(CalibrationType), CancelCalibration, ValidateCalibration,
    Select, Measure(Tool), Finish, Cancel, Undo, Redo, Delete,
    Draw(drawing_variants::DrawingVariant), Construction(bool), Pattern(patterns::PatternKind),
    ImportPlan, Guided, Compare, Comments, Enhance, Image(AdvancedImageAction),
    Report, Export(DataExportFormat), Server,
}

impl DperMeasureApp {
    fn ribbon_groups(&self) -> Vec<G<Action>> {
        use Action::*;
        let image = self.image.is_some();
        let acquisition_idle = self.video_recording.is_none() && self.timelapse_capture.is_none();
        let idle = self.pending.is_empty() && !self.measurement_gesture_active() && !self.calibration_mode && acquisition_idle;
        let editable = image && idle;
        let measure = image && idle;
        let server = self.mcp_server.as_ref().is_some_and(|server|
            matches!(server.status(), mcp_server::ServerStatus::Running));
        match self.active_ribbon_tab {
            RibbonTab::Camera => vec![
                G::new("camera-device", "Caméra", 2, vec![
                    C::new("camera.refresh", "Actualiser", UiIcon::Refresh, RefreshCamera),
                    C::new("camera.stream", if !self.running { "Démarrer le flux" } else if self.paused { "Reprendre" } else { "Figer" },
                        UiIcon::Play, Stream).enabled(!self.cameras.is_empty() || self.running),
                ]),
                G::new("camera-settings", "Optique et format", 1, vec![
                    C::new("camera.settings", "Réglages caméra", UiIcon::Settings, CameraSettings)
                        .enabled(self.running && self.camera_controls_camera.as_deref() == Some(self.active_device_id.as_str())),
                    C::new("camera.mode.apply", "Appliquer le format", UiIcon::Check, ApplyCameraMode)
                        .enabled(self.running && !self.camera_modes.is_empty()),
                ]),
            ],
            RibbonTab::Capture => vec![
                G::new("capture-photo", "Photo", 1, vec![C::new("capture.photo", "Photo PNG", UiIcon::Camera, Photo)
                    .enabled(image || self.running)]),
                G::new("capture-sequence", "Vidéo et série", 2, vec![
                    C::new("capture.video", if self.video_recording.is_some() { "Arrêter la vidéo" } else { "Vidéo MP4" },
                        UiIcon::Video, Video).enabled(self.running || self.video_recording.is_some()),
                    C::new("capture.timelapse", if self.timelapse_capture.is_some() { "Arrêter la série" } else { "Time-lapse" },
                        UiIcon::Clock, TimeLapse).enabled(self.running || self.timelapse_capture.is_some()),
                ]),
            ],
            RibbonTab::Files => vec![
                G::new("files-open", "Ouvrir", 2, vec![
                    C::new("files.image", "Image…", UiIcon::Image, OpenImage).enabled(idle),
                    C::new("files.project", "Projet…", UiIcon::Folder, OpenProject).enabled(idle),
                    C::new("files.search", "Rechercher un projet…", UiIcon::ZoomIn, SearchProjects),
                ]),
                G::new("files-project", "Projet", 2, vec![
                    C::new("files.save", "Enregistrer sous…", UiIcon::Save, Save).enabled(editable),
                    C::new("files.close", "Fermer l’image", UiIcon::Close, CloseImage).enabled(image).destructive(),
                ]),
            ],
            RibbonTab::Calibration => {
                if !self.calibration_mode {
                    return vec![G::new("calibration-start", "Étalonnage", 1, vec![
                        C::new("calibration.start", "Étalonner", UiIcon::Crosshair, StartCalibration).enabled(image && idle),
                    ])];
                }
                let methods = [
                    ("calibration.segment", "Segment", UiIcon::Distance, CalibrationType::Segment),
                    ("calibration.circle", "Cercle Ø", UiIcon::Point, CalibrationType::CircleDiameter),
                    ("calibration.rectangle", "Rectangle", UiIcon::Rectangle, CalibrationType::RectangleDiagonal),
                    ("calibration.xy", "Multipoint X/Y", UiIcon::Grid, CalibrationType::MultiPointXY),
                ].into_iter().map(|(id, label, icon, method)|
                    C::new(id, label, icon, CalibrationMethod(method)).selected(self.calibration_type == method)).collect();
                let required = if self.calibration_type == CalibrationType::MultiPointXY { 12 } else { 2 };
                vec![G::new("calibration-methods", "Méthode", 3, methods),
                    G::new("calibration-actions", "Actions", 2, vec![
                        C::new("calibration.validate", "Valider", UiIcon::Check, ValidateCalibration).enabled(self.pending.len() == required),
                        C::new("calibration.cancel", "Annuler", UiIcon::Undo, CancelCalibration),
                    ])]
            }
            RibbonTab::Measurements => {
                use drawing_variants::DrawingVariant as V;
                let tools = |items: &[(&'static str, &'static str, UiIcon, Tool)]| -> Vec<C<Action>> { items.iter().map(|&(id, label, icon, tool)|
                    C::new(id, label, icon, Measure(tool)).enabled(measure)
                        .selected(!self.selection_mode && !self.calibration_mode && self.mode == tool)).collect() };
                vec![
                    G::new("drawing-purpose", "Nature du tracé", 2, vec![
                        C::new("drawing.measurement", "Mesure", UiIcon::Ruler, Construction(false)).enabled(measure).selected(!self.construction_mode),
                        C::new("drawing.construction", "Construction", UiIcon::Construction, Construction(true)).enabled(measure).selected(self.construction_mode),
                    ]),
                    G::new("measure-selection", "Sélection", 1, vec![
                        C::new("measure.select", "Sélection", UiIcon::Select, Select).enabled(measure).selected(self.selection_mode),
                        C::new("measure.delete", "Supprimer la sélection", UiIcon::Close, Delete)
                            .enabled(editable && self.selected_measurement.is_some()).destructive().shortcut("Suppr"),
                    ]),
                    G::new("measure-distances", "Distances et angles", 2, [
                        ("measure.distance", "Ligne", UiIcon::Distance, V::Line, Some("Lignes")),
                        ("measure.angle", "Angle", UiIcon::Angle, V::Standard(Tool::Angle), None),
                        ("measure.midpoint", "Ligne par milieu", UiIcon::Distance, V::MidpointLine, Some("Lignes")),
                        ("measure.polyline", "Ligne brisée", UiIcon::Polyline, V::Standard(Tool::Polyline), None),
                        ("measure.pointline", "Point-ligne", UiIcon::Distance, V::Standard(Tool::PointLineDistance), None),
                        ("measure.parallel", "Parallèles", UiIcon::Parallel, V::Standard(Tool::ParallelDistance), None),
                    ].into_iter().map(|(id, label, icon, variant, category)| {
                        let mut command = C::new(id, label, icon, Draw(variant)).enabled(measure)
                            .selected(!self.selection_mode && self.drawing_variant == variant);
                        if let Some(category) = category { command = command.category(category); }
                        command
                    }).collect()),
                    G::new("measure-shapes", "Formes et variantes", 2, [
                        ("measure.circle", "Cercle", UiIcon::Point, V::CircleCenter, "Cercles"),
                        ("measure.rectangle", "Rectangle", UiIcon::Rectangle, V::Rectangle2, "Rectangles"),
                        ("measure.circle.diameter", "Cercle — 2 points du diamètre", UiIcon::Point, V::CircleDiameter, "Cercles"),
                        ("measure.circle.three", "Cercle — 3 points", UiIcon::Point, V::Circle3, "Cercles"),
                        ("measure.circle.tangent2", "Cercle — 2 tangentes et rayon", UiIcon::Point, V::Circle2Tangent, "Cercles"),
                        ("measure.circle.tangent3", "Cercle — 3 tangentes", UiIcon::Point, V::Circle3Tangent, "Cercles"),
                        ("measure.circlefit", "Cercle ajusté", UiIcon::Point, V::Standard(Tool::CircleFit), "Cercles"),
                        ("measure.rectangle.three", "Rectangle orienté — 3 points", UiIcon::Rectangle, V::Rectangle3, "Rectangles"),
                        ("measure.rectangle.center", "Rectangle — centre et coin", UiIcon::Rectangle, V::CenterRectangle, "Rectangles"),
                        ("measure.rectangle.rounded", "Rectangle arrondi", UiIcon::Rectangle, V::RoundedRectangle, "Rectangles"),
                        ("measure.arc.three", "Arc — 3 points", UiIcon::Angle, V::Arc3, "Arcs"),
                        ("measure.arc.center", "Arc — centre et extrémités", UiIcon::Angle, V::ArcCenter, "Arcs"),
                        ("measure.arc.tangent", "Arc tangent", UiIcon::Angle, V::ArcTangent, "Arcs"),
                        ("measure.polygon", "Polygone", UiIcon::Shapes, V::Standard(Tool::Polygon), "Polygones"),
                        ("measure.ellipse", "Ellipse", UiIcon::Ellipse, V::Standard(Tool::Ellipse), "Ellipses"),
                    ].into_iter().map(|(id, label, icon, variant, category)|
                        C::new(id, label, icon, Draw(variant)).enabled(measure)
                            .selected(!self.selection_mode && self.drawing_variant == variant).category(category)).collect()),
                    G::new("measure-data", "Coordonnées et comptage", 1, tools(&[
                        ("measure.coordinate", "Coordonnées", UiIcon::Coordinates, Tool::Coordinate),
                        ("measure.count", "Comptage", UiIcon::Crosshair, Tool::ObjectCount),
                    ])),
                    G::new("measure-patterns", "Patterns", 3, vec![
                        C::new("pattern.mirror", "Miroir", UiIcon::Mirror, Pattern(patterns::PatternKind::Mirror))
                            .enabled(editable && self.selected_measurement.is_some()),
                        C::new("pattern.circular", "Circulaire", UiIcon::CircularPattern, Pattern(patterns::PatternKind::Circular))
                            .enabled(editable && self.selected_measurement.is_some()),
                        C::new("pattern.rectangular", "Rectangulaire", UiIcon::RectangularPattern, Pattern(patterns::PatternKind::Rectangular))
                            .enabled(editable && self.selected_measurement.is_some()),
                    ]),
                    G::new("measure-edit", "Tracé et historique", 2, vec![
                        C::new("measure.undo", "Annuler", UiIcon::Undo, Undo).shortcut("Ctrl+Z")
                            .enabled(!self.pending.is_empty() || !self.undo_history.is_empty()),
                        C::new("measure.redo", "Rétablir", UiIcon::Redo, Redo).shortcut("Ctrl+Y")
                            .enabled(!self.redo_history.is_empty() && !self.measurement_gesture_active()
                                && (idle || (self.display_history_priority && matches!(self.redo_history.last(),
                                    Some(MeasurementHistoryAction::DisplayAdjusted { .. }))))),
                        C::new("measure.finish", "Terminer le tracé", UiIcon::Check, Finish).shortcut("Entrée")
                            .enabled(!self.pending.is_empty() && !self.calibration_mode),
                        C::new("measure.cancel", "Quitter le tracé", UiIcon::Close, Cancel).shortcut("Échap"),
                    ]),
                ]
            }
            RibbonTab::Inspection => vec![
                G::new("inspection-reference", "Référence", 1, vec![
                    C::new("inspection.import", "Importer un plan…", UiIcon::Layers, ImportPlan).enabled(idle),
                ]),
                G::new("inspection-control", "Contrôle", 2, vec![
                    C::new("inspection.guided", "Parcours guidé", UiIcon::Document, Guided),
                    C::new("inspection.compare", "Comparer A/B…", UiIcon::Database, Compare).enabled(editable),
                    C::new("inspection.comments", "Commentaires", UiIcon::Comment, Comments),
                ]),
            ],
            RibbonTab::Image => vec![
                G::new("image-quality", "Qualité", 1, vec![C::new("image.enhance", "Améliorer", UiIcon::Sun, Enhance)
                    .enabled(image && (!self.running || self.paused))]),
                G::new("image-geometry", "Corrections optiques", 1, vec![
                    C::new("image.perspective", "Perspective…", UiIcon::Crop, Image(AdvancedImageAction::RectifyPerspective)).enabled(editable),
                    C::new("image.flatfield", "Flat-field…", UiIcon::Image, Image(AdvancedImageAction::FlatField)).enabled(editable),
                    C::new("image.radial", "Distorsion radiale…", UiIcon::Shapes, Image(AdvancedImageAction::RadialDistortion)).enabled(editable),
                    C::new("image.calibrate", "Mire de distorsion…", UiIcon::Crosshair, Image(AdvancedImageAction::CalibrateDistortion)).enabled(idle),
                ]),
                G::new("image-assembly", "Assemblage", 2, vec![
                    C::new("image.focus", "Empilement focus…", UiIcon::Layers, Image(AdvancedImageAction::FocusStack)).enabled(idle),
                    C::new("image.panorama", "Panorama…", UiIcon::Image, Image(AdvancedImageAction::Panorama)).enabled(idle),
                ]),
            ],
            RibbonTab::Exports => vec![
                G::new("exports-report", "Rapport", 1, vec![C::new("exports.report", "Rapport PDF / PPTX…", UiIcon::Document, Report).enabled(idle)]),
                G::new("exports-data", "Mesures", 2, vec![
                    C::new("exports.csv", "CSV", UiIcon::Grid, Export(DataExportFormat::Csv)).enabled(editable),
                    C::new("exports.xlsx", "Excel", UiIcon::Grid, Export(DataExportFormat::Xlsx)).enabled(editable),
                    C::new("exports.json", "Session JSON", UiIcon::Document, Export(DataExportFormat::SessionJson)).enabled(editable),
                    C::new("exports.profiles", "Profils JSON", UiIcon::Crosshair, Export(DataExportFormat::CalibrationJson)).enabled(editable),
                ]),
                G::new("exports-images", "Images et CAO", 2, vec![
                    C::new("exports.png", "Image annotée", UiIcon::Image, Export(DataExportFormat::AnnotatedPng)).enabled(editable),
                    C::new("exports.dxf", "DXF", UiIcon::Export, Export(DataExportFormat::Dxf)).enabled(editable),
                    C::new("exports.svg", "SVG", UiIcon::Shapes, Export(DataExportFormat::Svg)).enabled(editable),
                    C::new("exports.tiff", "OME-TIFF", UiIcon::Image, Export(DataExportFormat::OmeTiff)).enabled(editable),
                ]),
            ],
            RibbonTab::Automation => vec![G::new("automation-server", "Serveur MCP", 1, vec![
                C::new("automation.server", if server { "Arrêter le serveur" } else { "Démarrer le serveur" }, UiIcon::Database, Server).selected(server),
            ])],
        }
    }

    pub(super) fn command_ribbon_ui(&mut self, ctx: &egui::Context) -> bool {
        let groups = self.ribbon_groups();
        let mut chosen = None;
        egui::TopBottomPanel::top("command-ribbon")
            .frame(egui::Frame::new().fill(Color32::from_rgb(17, 35, 54))
                .stroke(Stroke::new(1.0, Color32::from_rgb(49, 76, 102)))
                .inner_margin(egui::Margin::symmetric(12, 5)))
            .show(ctx, |ui| {
                if self.registration_photo_pick_owns_frame() || self.numeric_resize_dialog.is_some() { ui.disable(); }
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(219, 234, 246));
                ui.visuals_mut().extreme_bg_color = Color32::from_rgb(12, 27, 43);
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::from_rgb(28, 53, 77);
                ui.visuals_mut().widgets.hovered.weak_bg_fill = Color32::from_rgb(39, 78, 111);
                ui.visuals_mut().widgets.active.weak_bg_fill = Color32::from_rgb(0, 112, 171);
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                if self.active_ribbon_tab == RibbonTab::Calibration && self.calibration_mode {
                    let card_width = if self.calibration_type == CalibrationType::MultiPointXY { 400.0 } else { 240.0 };
                    chosen = crate::ribbon::show_with_inserted_control(ui, &groups, 1, card_width + 7.0, |ui| {
                        ui.add_space(7.0);
                        let width = card_width.min(ui.available_width());
                        self.calibration_reference_card(ui, width);
                    });
                } else { chosen = crate::ribbon::show(ui, &groups); }
                ui.separator();
                self.ribbon_options(ui);
            });
        chosen.is_some_and(|action| self.execute_ribbon_action(ctx, action))
    }

    fn execute_ribbon_action(&mut self, ctx: &egui::Context, action: Action) -> bool {
        use Action::*;
        match action {
            RefreshCamera => self.refresh_cameras(),
            Stream => if self.video_recording.is_some() || self.timelapse_capture.is_some() {
                self.message = "Arrêtez l’acquisition avant de modifier le flux.".into();
            } else if self.running { self.toggle_stream_pause(); } else { self.start_camera(); },
            CameraSettings => self.show_camera_controls = true,
            ApplyCameraMode => self.start_camera(),
            Photo => self.capture_photo(),
            Video => if self.video_recording.is_some() {
                self.message = self.stop_video_recording("terminé").unwrap_or_else(|| "Enregistrement MP4 arrêté.".into());
            } else { self.start_video_recording(); },
            TimeLapse => if self.timelapse_capture.is_some() {
                self.message = self.stop_time_lapse("interrompu", true).unwrap_or_else(|| "Time-lapse arrêté.".into());
            } else { self.start_time_lapse(); },
            OpenImage => self.open_image(ctx), OpenProject => self.open_project(ctx),
            SearchProjects => self.start_project_search(), Save => self.save_project(), CloseImage => self.close_loaded_image(),
            StartCalibration => self.start_calibration_mode(),
            CalibrationMethod(method) => if self.calibration_type != method { self.calibration_type = method; self.clear_pending(); },
            CancelCalibration => self.cancel_calibration_mode(), ValidateCalibration => self.apply_calibration(),
            Select => { self.freeze_capture(); self.selection_mode = true; self.pan_mode = false; },
            Measure(tool) => { self.choose_measurement_tool(tool); self.right_panel_annotations = false; },
            Draw(variant) => self.choose_drawing_variant(variant),
            Construction(enabled) => self.set_construction_mode(enabled),
            Pattern(kind) => self.open_pattern(kind),
            Finish => self.finish_measurement(), Cancel => self.escape_active_mode(),
            Undo => self.undo_current_action(), Redo => self.redo_current_action(),
            Delete => { self.delete_selected_measurement(); },
            ImportPlan => { self.right_panel_annotations = false; self.workflow_open_reference_panel = true; self.open_reference_document(); },
            Guided => self.show_guided_examples = true, Compare => self.compare_with_session(),
            Comments => self.right_panel_annotations = true,
            Enhance => {
                match self.image.as_ref().map(|image| quality::automatic_display_correction(&image.rgb)) {
                    Some(Ok(correction)) if correction != self.display_correction => {
                        self.display_correction = correction;
                        self.message = "Amélioration appliquée à l’aperçu ; l’image source reste inchangée.".into();
                        return true;
                    }
                    Some(Ok(_)) => self.message = "L’aperçu est déjà proche du réglage automatique.".into(),
                    Some(Err(error)) => self.message = format!("Amélioration automatique impossible : {error}."),
                    None => self.message = "Ouvrez une image avant l’amélioration automatique.".into(),
                }
            }
            Image(action) => match action {
                AdvancedImageAction::CalibrateDistortion => self.calibrate_optical_distortion(),
                AdvancedImageAction::FlatField => self.apply_flat_field(ctx),
                AdvancedImageAction::RadialDistortion => self.apply_optical_distortion_correction(ctx),
                AdvancedImageAction::FocusStack => self.create_focus_stack(ctx),
                AdvancedImageAction::Panorama => self.start_assisted_panorama(),
                AdvancedImageAction::RectifyPerspective => self.start_rectification(ctx),
            },
            Report => self.open_folder_export(ctx), Export(format) => self.export_current_data(format),
            Server => {
                let running = self.mcp_server.as_ref().is_some_and(|server| matches!(server.status(), mcp_server::ServerStatus::Running));
                self.mcp_server = None;
                if running { self.message = "Serveur MCP local arrêté.".into(); }
                else { match mcp_server::start(self.mcp_bridge.clone()) {
                    Ok(server) => { self.mcp_server = Some(server); self.message = format!("Serveur MCP local : {}", mcp_server::MCP_ENDPOINT); }
                    Err(error) => self.message = error,
                } }
            }
        }
        false
    }

    fn ribbon_options(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            match self.active_ribbon_tab {
            RibbonTab::Camera => {
                ui.label("Caméra");
                let previous = self.selected_camera;
                let name = self.cameras.get(previous).map(|(_, name, _)| name.as_str()).unwrap_or("Aucune caméra");
                egui::ComboBox::from_id_salt("ribbon-camera-select").selected_text(name).width(190.0).show_ui(ui, |ui| {
                    *ui.visuals_mut() = dper_visuals();
                    for (index, (_, name, _)) in self.cameras.iter().enumerate() { ui.selectable_value(&mut self.selected_camera, index, name); }
                });
                if previous != self.selected_camera { self.camera_modes.clear(); self.camera_modes_camera = None; self.selected_camera_mode = None; }
                if !self.camera_modes.is_empty() {
                    let label = self.selected_camera_mode.and_then(|index| self.camera_modes.get(index))
                        .map(|mode| mode.label.as_str()).unwrap_or("Résolution auto");
                    egui::ComboBox::from_id_salt("ribbon-camera-mode").selected_text(label).width(170.0).show_ui(ui, |ui| {
                        *ui.visuals_mut() = dper_visuals();
                        ui.selectable_value(&mut self.selected_camera_mode, None, "Résolution auto");
                        for (index, mode) in self.camera_modes.iter().enumerate() { ui.selectable_value(&mut self.selected_camera_mode, Some(index), &mode.label); }
                    });
                }
                ui.label("Optique");
                if ui.add(egui::TextEdit::singleline(&mut self.optical_setup).desired_width(190.0)
                    .hint_text("objectif / distance / zoom")).lost_focus() { self.activate_saved_calibration(); }
            }
            RibbonTab::Capture => {
                ui.label("Retardateur");
                egui::ComboBox::from_id_salt("ribbon-capture-delay").selected_text(format!("{} s", self.capture_delay_seconds))
                    .show_ui(ui, |ui| { *ui.visuals_mut() = dper_visuals(); for seconds in [0, 3, 5, 10] { ui.selectable_value(&mut self.capture_delay_seconds, seconds, format!("{seconds} s")); } });
                ui.label("Série");
                ui.add_enabled_ui(self.timelapse_capture.is_none(), |ui| {
                    egui::ComboBox::from_id_salt("ribbon-timelapse-interval").selected_text(format!("{} s", self.timelapse_interval_seconds))
                        .show_ui(ui, |ui| { *ui.visuals_mut() = dper_visuals(); for seconds in [1_u16, 2, 5, 10, 30, 60, 300] {
                            ui.selectable_value(&mut self.timelapse_interval_seconds, seconds, format!("{seconds} s"));
                        } });
                    ui.add(egui::DragValue::new(&mut self.timelapse_frame_count).range(2..=500).suffix(" images"));
                });
            }
            RibbonTab::Calibration if self.calibration_mode => {
                ui.label(egui::RichText::new(self.calibration_type.instruction()).size(13.0));
                let required = if self.calibration_type == CalibrationType::MultiPointXY { 12 } else { 2 };
                if self.pending.len() == required {
                    ui.separator();
                    ui.label(egui::RichText::new("Glissez un point de référence pour ajuster la mesure.")
                        .size(12.0).color(Color32::from_rgb(133, 211, 232)));
                }
            }
            RibbonTab::Calibration => { ui.label("Ouvrez une image ou figez le flux, puis choisissez Étalonner."); }
            RibbonTab::Measurements => { ui.label("Choisissez un outil dans le ruban pour commencer un tracé."); }
            RibbonTab::Automation => {
                if let Some(server) = &self.mcp_server {
                    match server.status() {
                        mcp_server::ServerStatus::Running => { ui.label(mcp_server::MCP_ENDPOINT); }
                        mcp_server::ServerStatus::Starting => { ui.label("Serveur en cours de démarrage…"); }
                        mcp_server::ServerStatus::Failed(error) => { ui.colored_label(Color32::LIGHT_RED, error); }
                        mcp_server::ServerStatus::Stopped => { ui.label("Serveur arrêté"); }
                    }
                } else { ui.label("Serveur MCP arrêté"); }
            }
            _ => { ui.label("Ouvrez le menu d’un groupe pour afficher toutes ses commandes."); }
            }
        });
    }

    pub(super) fn manual_tool_options(&mut self, ui: &mut egui::Ui) {
        if self.selection_mode {
            ui.label(egui::RichText::new("Sélection").strong().size(14.0));
            ui.label("Cliquez ou encadrez les tracés.")
                .on_hover_text("Maj : ajouter à la sélection. Ctrl : basculer. Les annotations peuvent aussi être sélectionnées.");
            return;
        }
        let color = if self.construction_mode { Color32::from_rgb(213, 160, 255) } else { Color32::from_rgb(40, 213, 236) };
        ui.label(egui::RichText::new(format!("{} · {}", if self.construction_mode { "Construction" } else { "Mesure" },
            self.drawing_variant.label())).strong().size(14.0).color(color))
            .on_hover_text(self.drawing_variant.instruction());
        ui.separator();
        if matches!(self.drawing_variant, drawing_variants::DrawingVariant::RoundedRectangle | drawing_variants::DrawingVariant::Circle2Tangent) {
            ui.label("Rayon");
            ui.add(egui::DragValue::new(&mut self.drawing_radius_px).range(0.1..=100000.0).speed(0.5).suffix(" px"));
        }
        let snap_available = self.pending.is_empty() && self.mode != Tool::ObjectCount;
        ui.add_enabled(snap_available, egui::Checkbox::new(&mut self.edge_snap_enabled, "Bords"))
            .on_hover_text("Accrochage subpixel : ajuste chaque point vers le bord local le plus net.");
        if self.edge_snap_enabled && self.mode != Tool::ObjectCount {
            ui.add_enabled(self.pending.is_empty(), egui::DragValue::new(&mut self.edge_snap_radius).range(2..=24).suffix(" px"))
                .on_hover_text("Rayon de recherche du bord, en pixels natifs.");
        }
        ui.checkbox(&mut self.construction_snap_enabled, "Constructions")
            .on_hover_text("Accrocher aux sommets, centres et intersections des constructions.");
        let expected = self.drawing_variant.required_points().map(|count| count.to_string()).unwrap_or_else(|| match self.mode {
            Tool::Polyline => "≥ 2".into(), Tool::ObjectCount => "≥ 1".into(), _ => "≥ 3".into(),
        });
        ui.label(egui::RichText::new(format!("{} / {} points", self.pending.len(), expected)).strong());
        if self.drawing_variant.required_points().is_none_or(|required| self.pending.len() >= required)
            && ui.add_enabled(!self.pending.is_empty(), egui::Button::new("Terminer")).clicked() {
            self.finish_measurement();
        }
        if !self.pending.is_empty() && ui.button("Annuler le tracé").clicked() {
            self.clear_pending();
            self.message = "Tracé en cours annulé.".into();
        }
    }

    fn calibration_reference_card(&mut self, ui: &mut egui::Ui, width: f32) {
        egui::Frame::new().fill(Color32::from_rgb(230, 246, 255))
            .stroke(Stroke::new(1.5, Color32::from_rgb(0, 165, 214)))
            .corner_radius(10).inner_margin(12).show(ui, |ui| {
                *ui.visuals_mut() = dper_visuals();
                ui.set_width((width - 27.0).max(100.0));
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                let required = if self.calibration_type == CalibrationType::MultiPointXY { 12 } else { 2 };
                let ready = self.pending.len() == required;
                let points_color = if ready { Color32::from_rgb(23, 124, 73) } else { Color32::from_rgb(45, 87, 124) };
                ui.vertical(|ui| {
                    if self.calibration_type == CalibrationType::MultiPointXY {
                        ui.label(egui::RichText::new("Références d’étalonnage").strong().size(17.0)
                                .color(Color32::from_rgb(0, 72, 120)));
                        ui.horizontal_wrapped(|ui| {
                            for (label, value) in [("X (mm)", &mut self.reference_mm), ("Y (mm)", &mut self.reference_mm_y),
                                ("Contrôle (mm)", &mut self.verification_reference_mm)] {
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    ui.label(egui::RichText::new(label).strong().size(12.5));
                                    ui.add(egui::TextEdit::singleline(value).desired_width(90.0)
                                        .font(FontId::proportional(22.0)).margin(egui::vec2(8.0, 5.0)));
                                });
                            }
                        });
                        ui.label(egui::RichText::new(format!("Points : {} / {}", self.pending.len(), required))
                            .strong().color(points_color))
                            .on_hover_text(if ready { "Prêt à valider" } else { "Placez les références sur l’image" });
                    } else {
                        ui.label(egui::RichText::new(self.calibration_type.reference_label()).strong().size(15.0)
                            .color(Color32::from_rgb(0, 72, 120)));
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.reference_mm).desired_width(90.0)
                                .font(FontId::proportional(22.0)).margin(egui::vec2(8.0, 5.0)));
                            ui.label(egui::RichText::new("mm").strong().size(14.0));
                        });
                        ui.label(egui::RichText::new(format!("Points : {} / {}", self.pending.len(), required))
                            .strong().color(points_color))
                            .on_hover_text(if ready { "Prêt à valider" } else { "Placez les références sur l’image" });
                    }
                });
            });
    }
}

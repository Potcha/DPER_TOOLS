use super::*;
use crate::ui_widgets::Icon as UiIcon;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RibbonTab { Camera, Capture, Files, Calibration, Measurements, Inspection, Image, Exports, Automation }
impl RibbonTab {
    const ALL: [(Self, &'static str); 9] = [
        (Self::Camera, "Caméra"), (Self::Capture, "Capture"), (Self::Files, "Fichiers"),
        (Self::Calibration, "Calibration"), (Self::Measurements, "Mesures"),
        (Self::Inspection, "Inspection"), (Self::Image, "Image"),
        (Self::Exports, "Exports"), (Self::Automation, "Automatisation"),
    ];
    fn uv(self) -> egui::Rect {
        let index = self as usize;
        egui::Rect::from_min_max(egui::pos2((index % 3) as f32 / 3.0, (index / 3) as f32 / 3.0),
            egui::pos2((index % 3 + 1) as f32 / 3.0, (index / 3 + 1) as f32 / 3.0))
    }
}


impl DperMeasureApp {
    fn ribbon_texture(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        self.tab_icon_sheet.get_or_insert_with(|| {
            let image = image::load_from_memory(include_bytes!("../assets/ui/ribbon_tabs/atlas_onglets.png"))
                .expect("embedded ribbon atlas").to_rgba8();
            ctx.load_texture("dper-ribbon-atlas", egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize], image.as_raw()), egui::TextureOptions::LINEAR)
        }).clone()
    }
    pub(super) fn ribbon_tabs(&mut self, ctx: &egui::Context) {
        let texture = self.ribbon_texture(ctx);
        egui::TopBottomPanel::top("ribbon-tabs").frame(egui::Frame::new()
            .fill(Color32::from_rgb(24, 57, 101)).inner_margin(egui::Margin::symmetric(12, 5)))
            .show(ctx, |ui| {
                if self.registration_photo_pick_owns_frame() { ui.disable(); }
                egui::ScrollArea::horizontal().id_salt("ribbon-tabs-scroll").auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("DPER_MEASURE").strong().size(18.0).color(Color32::WHITE));
                        ui.separator();
                        for (tab, label) in RibbonTab::ALL {
                            if ui_widgets::navigation_tab_button(ui, label, &texture, tab.uv(), self.active_ribbon_tab == tab).clicked() {
                                self.active_ribbon_tab = tab;
                            }
                        }
                    });
                });
            });
    }
    pub(super) fn select_ribbon_tool(&mut self, tool: Tool) {
        if self.registration_photo_pick_owns_frame() || self.point_edit.is_some() { return; }
        if self.calibration_mode {
            self.message = "Choisissez le type de référence dans le panneau d’étalonnage.".into();
        } else if self.pending.is_empty() {
            self.selection_mode = false;
            self.mode = tool;
        } else {
            self.message = "Terminez ou annulez le tracé en cours avant de changer d'outil.".into();
        }
    }
    pub(super) fn start_ribbon_calibration(&mut self) {
        if self.registration_photo_pick_owns_frame() || self.point_edit.is_some()
            || !self.pending.is_empty() || self.calibration_mode || self.annotation_mode
            || self.export_dialog.is_some() {
            self.message = "Terminez ou annulez l’action en cours avant l’étalonnage.".into();
            return;
        }
        if self.capture_timer.is_some() || self.timelapse_capture.is_some() || self.video_recording.is_some() {
            self.message = "Arrêtez l’acquisition en cours avant l’étalonnage.".into();
            return;
        }
        if self.image.is_none() {
            self.message = "Ouvrez une image ou démarrez le flux avant l’étalonnage.".into();
            return;
        }
        self.freeze_capture();
        self.selection_mode = false;
        self.calibration_mode = true;
        self.clear_pending();
        self.message = format!("Image figée : {}", self.calibration_type.instruction());
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
                            ui.label(egui::RichText::new("mm").strong().size(14.0).color(Color32::from_rgb(0, 72, 120)));
                        });
                        ui.label(egui::RichText::new(format!("Points : {} / {}", self.pending.len(), required))
                            .strong().color(points_color))
                            .on_hover_text(if ready { "Prêt à valider" } else { "Placez les références sur l’image" });
                    }
                });
            });
    }

    pub(super) fn ribbon_commands(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        use crate::ribbon::{Command as C, Group as G};
        #[derive(Clone, Copy)] enum Action {
            Refresh, Stream, CameraSettings, ApplyMode, Photo, Video, TimeLapse, OpenImage, OpenProject, Search, Save,
            StartCalibration, CalibrationMethod(CalibrationType), ValidateCalibration, CancelCalibration,
            Tool(Tool), Select, Finish, Cancel, Undo, Redo, Delete, ImportPlan, Guided, Compare,
            Perspective, FlatField, Radial, CalibrateDistortion, Focus, Panorama,
            Report, Export(DataExportFormat), Server,
        }
        let image = self.image.is_some();
        let idle = self.pending.is_empty() && self.point_edit.is_none() && !self.calibration_mode;
        let editable = image && idle;
        let c = |id, label, icon, action| C::new(id, label, icon, action);
        let tools = |items: &[(&'static str, &'static str, UiIcon, Tool)]| items.iter().map(|&(id, label, icon, tool)| {
            let command = c(id, label, icon, Action::Tool(tool)).enabled(idle)
                .selected(!self.selection_mode && !self.calibration_mode && self.mode == tool);
            if matches!(tool, Tool::Circle | Tool::CircleFit) { command.category("Cercles") } else { command }
        }).collect();
        let groups = match self.active_ribbon_tab {
            RibbonTab::Camera => vec![
                G::new("camera-device", "Caméra", 2, vec![c("refresh", "Actualiser", UiIcon::Refresh, Action::Refresh),
                    c("stream", if !self.running { "Démarrer le flux" } else if self.paused { "Reprendre" } else { "Figer" }, UiIcon::Play, Action::Stream)]),
                G::new("camera-settings", "Optique et format", 1, vec![c("settings", "Réglages caméra", UiIcon::Settings, Action::CameraSettings)
                    .enabled(self.running && self.camera_controls_camera.as_deref() == Some(self.active_device_id.as_str())),
                    c("apply-mode", "Appliquer le format", UiIcon::Check, Action::ApplyMode).enabled(self.running && !self.camera_modes.is_empty())])],
            RibbonTab::Capture => vec![
                G::new("photo", "Photo", 1, vec![c("photo", "Photo PNG", UiIcon::Camera, Action::Photo)]),
                G::new("sequence", "Vidéo et série", 2, vec![c("video", if self.video_recording.is_some() { "Arrêter la vidéo" } else { "Vidéo MP4" }, UiIcon::Video, Action::Video),
                    c("timelapse", if self.timelapse_capture.is_some() { "Arrêter la série" } else { "Time-lapse" }, UiIcon::Clock, Action::TimeLapse)])],
            RibbonTab::Files => vec![
                G::new("open", "Ouvrir", 2, vec![c("open-image", "Image…", UiIcon::Image, Action::OpenImage),
                    c("open-project", "Projet…", UiIcon::Folder, Action::OpenProject), c("search", "Rechercher un projet…", UiIcon::ZoomIn, Action::Search)]),
                G::new("project", "Projet", 1, vec![c("save", "Enregistrer le projet", UiIcon::Save, Action::Save).enabled(editable)])],
            RibbonTab::Calibration => if self.calibration_mode { vec![
                G::new("calibration-method", "Méthode", 3, [("segment", "Segment", UiIcon::Distance, CalibrationType::Segment),
                    ("circle-cal", "Cercle Ø", UiIcon::Point, CalibrationType::CircleDiameter), ("rectangle-cal", "Rectangle", UiIcon::Rectangle, CalibrationType::RectangleDiagonal),
                    ("xy-cal", "Multipoint X/Y", UiIcon::Grid, CalibrationType::MultiPointXY)].into_iter()
                    .map(|(id, label, icon, method)| c(id, label, icon, Action::CalibrationMethod(method)).selected(self.calibration_type == method)).collect()),
                G::new("calibration-actions", "Actions", 2, vec![c("validate-cal", "Valider", UiIcon::Check, Action::ValidateCalibration)
                    .enabled(self.pending.len() == if self.calibration_type == CalibrationType::MultiPointXY { 12 } else { 2 }),
                    c("cancel-cal", "Annuler", UiIcon::Undo, Action::CancelCalibration)])]
                } else { vec![G::new("calibration", "Étalonnage", 1, vec![c("start-cal", "Étalonner", UiIcon::Crosshair, Action::StartCalibration).enabled(image && idle)])] },
            RibbonTab::Measurements => vec![
                G::new("selection", "Sélection", 1, vec![c("select", "Sélection", UiIcon::Select, Action::Select).enabled(idle).selected(self.selection_mode),
                    c("delete", "Supprimer", UiIcon::Close, Action::Delete).enabled(editable && self.selected_measurement.is_some()).destructive()]),
                G::new("distances", "Distances et angles", 2, tools(&[("distance", "Distance", UiIcon::Distance, Tool::Distance),
                    ("angle", "Angle", UiIcon::Angle, Tool::Angle), ("polyline", "Ligne brisée", UiIcon::Polyline, Tool::Polyline),
                    ("pointline", "Point-ligne", UiIcon::Distance, Tool::PointLineDistance), ("parallel", "Parallèles", UiIcon::Parallel, Tool::ParallelDistance)])),
                G::new("shapes", "Formes et contours", 2, tools(&[("circle", "Cercle", UiIcon::Point, Tool::Circle),
                    ("rectangle", "Rectangle", UiIcon::Rectangle, Tool::Rectangle), ("polygon", "Polygone", UiIcon::Shapes, Tool::Polygon),
                    ("fit", "Cercle ajusté", UiIcon::Point, Tool::CircleFit), ("ellipse", "Ellipse", UiIcon::Ellipse, Tool::Ellipse), ("arc", "Arc", UiIcon::Angle, Tool::Arc)])),
                G::new("data", "Coordonnées et comptage", 1, tools(&[("coordinate", "Coordonnées", UiIcon::Coordinates, Tool::Coordinate),
                    ("count", "Comptage", UiIcon::Crosshair, Tool::ObjectCount)])),
                G::new("history", "Tracé et historique", 2, vec![c("undo", "Annuler", UiIcon::Undo, Action::Undo),
                    c("redo", "Rétablir", UiIcon::Redo, Action::Redo), c("finish", "Terminer le tracé", UiIcon::Check, Action::Finish)
                        .enabled(!self.calibration_mode && !self.pending.is_empty()), c("cancel", "Quitter le tracé", UiIcon::Close, Action::Cancel)])],
            RibbonTab::Inspection => vec![G::new("reference", "Référence", 1, vec![c("import-plan", "Importer un plan…", UiIcon::Layers, Action::ImportPlan).enabled(idle)]),
                G::new("control", "Contrôle", 2, vec![c("guided", "Parcours guidé", UiIcon::Document, Action::Guided), c("compare", "Comparer A/B…", UiIcon::Database, Action::Compare).enabled(editable)])],
            RibbonTab::Image => vec![G::new("optics", "Corrections optiques", 1, vec![c("perspective", "Perspective…", UiIcon::Crop, Action::Perspective).enabled(editable),
                c("flatfield", "Flat-field…", UiIcon::Image, Action::FlatField).enabled(editable), c("radial", "Distorsion radiale…", UiIcon::Shapes, Action::Radial).enabled(editable),
                c("cal-distortion", "Mire de distorsion…", UiIcon::Crosshair, Action::CalibrateDistortion).enabled(idle)]),
                G::new("assembly", "Assemblage", 2, vec![c("focus", "Empilement focus…", UiIcon::Layers, Action::Focus).enabled(idle), c("panorama", "Panorama…", UiIcon::Image, Action::Panorama).enabled(idle)])],
            RibbonTab::Exports => vec![G::new("report", "Rapport", 1, vec![c("report", "Rapport PDF / PPTX…", UiIcon::Document, Action::Report).enabled(idle)]),
                G::new("data", "Mesures", 2, [("csv", "CSV", UiIcon::Grid, DataExportFormat::Csv), ("xlsx", "Excel", UiIcon::Grid, DataExportFormat::Xlsx),
                    ("json", "Session JSON", UiIcon::Document, DataExportFormat::SessionJson), ("profiles", "Profils JSON", UiIcon::Crosshair, DataExportFormat::CalibrationJson)]
                    .into_iter().map(|(id, label, icon, format)| c(id, label, icon, Action::Export(format)).enabled(editable)).collect()),
                G::new("images", "Images et CAO", 2, [("png", "Image annotée", UiIcon::Image, DataExportFormat::AnnotatedPng),
                    ("dxf", "DXF", UiIcon::Export, DataExportFormat::Dxf), ("svg", "SVG", UiIcon::Shapes, DataExportFormat::Svg),
                    ("tiff", "OME-TIFF", UiIcon::Image, DataExportFormat::OmeTiff)].into_iter()
                    .map(|(id, label, icon, format)| c(id, label, icon, Action::Export(format)).enabled(editable)).collect())],
            RibbonTab::Automation => vec![G::new("server", "Serveur MCP", 1, vec![c("server", "Serveur local", UiIcon::Database, Action::Server)])],
        };
        let action = if self.active_ribbon_tab == RibbonTab::Calibration && self.calibration_mode {
            let width = if self.calibration_type == CalibrationType::MultiPointXY { 400.0 } else { 240.0 };
            crate::ribbon::show_with_inserted_control(ui, &groups, 1, width + 7.0, |ui| {
                ui.add_space(7.0);
                self.calibration_reference_card(ui, width.min(ui.available_width()));
            })
        } else { crate::ribbon::show(ui, &groups) };
        if let Some(action) = action { match action {
            Action::Refresh => self.refresh_cameras(), Action::Stream => if self.running {
                if self.timelapse_capture.is_some() || self.video_recording.is_some() { self.message = "Arrêtez le time-lapse ou la vidéo avant de figer ou reprendre le flux.".into(); }
                else { self.toggle_stream_pause(); }
            } else { self.start_camera(); },
            Action::CameraSettings => self.show_camera_controls = true, Action::ApplyMode => self.start_camera(), Action::Photo => self.capture_photo(),
            Action::Video => if self.video_recording.is_some() { self.message = self.stop_video_recording("terminé").unwrap_or_else(|| "Enregistrement MP4 arrêté.".into()); } else { self.start_video_recording(); },
            Action::TimeLapse => if self.timelapse_capture.is_some() { self.message = self.stop_time_lapse("interrompu", true).unwrap_or_else(|| "Time-lapse arrêté.".into()); } else { self.start_time_lapse(); },
            Action::OpenImage => self.open_image(ctx), Action::OpenProject => self.open_project(ctx), Action::Search => self.start_project_search(), Action::Save => self.save_project(),
            Action::StartCalibration => self.start_ribbon_calibration(),
            Action::CalibrationMethod(method) => if self.calibration_type != method { self.calibration_type = method; self.clear_pending(); },
            Action::ValidateCalibration => self.apply_calibration(), Action::CancelCalibration => self.escape_active_mode(),
            Action::Tool(tool) => self.select_ribbon_tool(tool), Action::Select => if idle { self.freeze_capture(); self.selection_mode = true; },
            Action::Finish => self.finish_measurement(), Action::Cancel => self.escape_active_mode(), Action::Undo => self.undo_current_action(),
            Action::Redo => self.redo_current_action(), Action::Delete => { self.delete_selected_measurement(); },
            Action::ImportPlan => self.open_reference_document(), Action::Guided => self.show_guided_examples = true, Action::Compare => self.compare_with_session(),
            Action::Perspective => self.start_rectification(ctx), Action::FlatField => self.apply_flat_field(ctx),
            Action::Radial => self.apply_optical_distortion_correction(ctx), Action::CalibrateDistortion => self.calibrate_optical_distortion(),
            Action::Focus => self.create_focus_stack(ctx), Action::Panorama => self.start_assisted_panorama(),
            Action::Report => self.open_folder_export(ctx), Action::Export(format) => self.export_current_data(format),
            Action::Server => { let running = self.mcp_server.as_ref().is_some_and(|server| matches!(server.status(), mcp_server::ServerStatus::Running));
                self.mcp_server = None; if running { self.message = "Serveur MCP local arrêté.".into(); } else { match mcp_server::start(self.mcp_bridge.clone()) {
                    Ok(server) => { self.mcp_server = Some(server); self.message = format!("Serveur MCP local : {}", mcp_server::MCP_ENDPOINT); }, Err(error) => self.message = error,
                } } },
        } }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_cover_each_distinct_atlas_cell() {
        assert_eq!(RibbonTab::ALL.len(), 9);
        for (index, (tab, _)) in RibbonTab::ALL.iter().enumerate() {
            let uv = tab.uv();
            assert!(uv.min.x >= 0.0 && uv.min.y >= 0.0 && uv.max.x <= 1.0 && uv.max.y <= 1.0);
            for (other, _) in RibbonTab::ALL.iter().skip(index + 1) { assert_ne!(uv, other.uv()); }
        }
    }

    #[test]
    fn changing_tool_respects_unfinished_trace_and_calibration() {
        let mut app = DperMeasureApp::new_with_services(egui::Context::default(), false);
        app.mode = Tool::Distance;
        app.pending.push(crate::geometry::Point { x: 1.0, y: 2.0 });
        app.select_ribbon_tool(Tool::Circle);
        assert_eq!(app.mode, Tool::Distance);
        assert_eq!(app.pending.len(), 1);
        app.pending.clear();
        app.calibration_mode = true;
        app.select_ribbon_tool(Tool::Circle);
        assert_eq!(app.mode, Tool::Distance);
        app.calibration_mode = false;
        app.select_ribbon_tool(Tool::Circle);
        assert_eq!(app.mode, Tool::Circle);
    }
}
#[cfg(test)]
#[path = "ribbon_ui_tests.rs"]
mod interaction_tests;

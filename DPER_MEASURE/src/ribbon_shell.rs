//! Restored workspace shell: workflow, large viewer and optional configuration.
use super::*;
use crate::ribbon_ui::RibbonTab;

impl DperMeasureApp {
    pub(crate) fn workflow_guide(&mut self, ctx: &egui::Context) {
        let has_image = self.image.is_some();
        let has_plan = self.reference_document.is_some() || self.inspection.plan.is_some();
        let has_calibration = self.calibration.is_some();
        let has_measurements = self.measurements.iter().any(|m| !m.is_construction);
        let steps = [
            ("Capture / image / projet", has_image, RibbonTab::Files),
            ("Plan PDF / image", has_plan, RibbonTab::Inspection),
            ("Étalonnage", has_calibration, RibbonTab::Calibration),
            ("Mesures", has_measurements, RibbonTab::Measurements),
            ("Contrôle", false, RibbonTab::Inspection),
            ("Export", false, RibbonTab::Exports),
        ];
        let current = match self.active_ribbon_tab {
            RibbonTab::Camera | RibbonTab::Capture | RibbonTab::Files => 0,
            RibbonTab::Calibration => 2,
            RibbonTab::Measurements => 3,
            RibbonTab::Inspection => if has_measurements { 4 } else { 1 },
            RibbonTab::Exports => 5,
            _ => if has_measurements { 4 } else if has_image { 2 } else { 0 },
        };
        egui::TopBottomPanel::top("workflow-guide").exact_height(76.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(225, 234, 243))
                .stroke(Stroke::new(1.0, Color32::from_rgb(190, 207, 223)))
                .inner_margin(egui::Margin::symmetric(14, 3)))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter_at(rect);
                let inset = 105.0_f32.min(rect.width() * 0.10);
                let centers: Vec<_> = (0..6).map(|i| Pos2::new(
                    rect.left() + inset + (rect.width() - 2.0 * inset) * i as f32 / 5.0,
                    rect.top() + 20.0)).collect();
                painter.line_segment([centers[0], centers[5]], Stroke::new(3.0, Color32::from_rgb(177, 198, 219)));
                if current > 0 { painter.line_segment([centers[0], centers[current]], Stroke::new(3.0, Color32::from_rgb(0, 145, 196))); }
                for (i, (label, complete, tab)) in steps.iter().enumerate() {
                    let left = if i == 0 { rect.left() } else { (centers[i-1].x + centers[i].x) * 0.5 };
                    let right = if i == 5 { rect.right() } else { (centers[i+1].x + centers[i].x) * 0.5 };
                    let area = Rect::from_min_max(Pos2::new(left, rect.top()), Pos2::new(right, rect.bottom()));
                    let response = ui.interact(area, ui.id().with(("workflow-step", i)), Sense::click());
                    if response.clicked() { self.active_ribbon_tab = *tab; }
                    let color = if i == current { Color32::from_rgb(0, 137, 190) }
                        else if *complete { Color32::from_rgb(40, 161, 97) }
                        else { Color32::from_rgb(174, 192, 209) };
                    painter.circle_filled(centers[i], 9.5, color);
                    if *complete && i != current {
                        let c = centers[i];
                        painter.line_segment([c + Vec2::new(-4.0, 0.0), c + Vec2::new(-1.0, 3.0)], Stroke::new(1.5, Color32::WHITE));
                        painter.line_segment([c + Vec2::new(-1.0, 3.0), c + Vec2::new(4.0, -3.0)], Stroke::new(1.5, Color32::WHITE));
                    } else {
                        painter.text(centers[i], Align2::CENTER_CENTER, (i+1).to_string(),
                            FontId::proportional(11.0), Color32::WHITE);
                    }
                    painter.with_clip_rect(area).text(centers[i] + Vec2::new(0.0, 23.0), Align2::CENTER_CENTER,
                        label, FontId::proportional(11.0), Color32::from_rgb(48, 73, 98));
                    if i == 1 || i == 2 { painter.with_clip_rect(area).text(centers[i] + Vec2::new(0.0, 39.0),
                        Align2::CENTER_CENTER, "facultatif", FontId::proportional(9.0), Color32::from_rgb(105, 124, 146)); }
                }
            });
    }

    pub(crate) fn viewer_toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().fill(Color32::from_rgb(29, 47, 65)).corner_radius(7)
            .inner_margin(egui::Margin::symmetric(9, 6)).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui_widgets::dark_chrome(ui);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("−").on_hover_text("Réduire le zoom").clicked() { self.viewer_zoom(0.8); }
                    ui.label(format!("Zoom : {:.0} %", self.zoom_percent));
                    if ui.button("+").on_hover_text("Agrandir le zoom").clicked() { self.viewer_zoom(1.25); }
                    if ui.button("1:1").clicked() { self.zoom = 1.0; self.fit_view = false; self.pan = Vec2::ZERO; }
                    if ui.button("Ajuster").clicked() { self.fit_view = true; self.pan = Vec2::ZERO; }
                    ui.separator();
                    ui.toggle_value(&mut self.show_alignment_grid, "▦ Grille");
                    ui.toggle_value(&mut self.show_center_guides, "⌖ Axes");
                    ui.separator();
                    ui.add_enabled_ui(!self.registration_photo_pick_owns_frame(), |ui| {
                        if ui.button("Annuler").clicked() { self.undo_current_action(); }
                        if ui.button("Rétablir").clicked() { self.redo_current_action(); }
                        if ui.button("Configuration").clicked() { self.show_configuration_panel = !self.show_configuration_panel; }
                    });
                    ui.weak("Molette : zoom · clic molette : déplacement");
                });
                if self.calibration_mode { ui.label(self.calibration_type.instruction()); }
                else if self.selection_mode { ui.label("Sélection · cliquez une mesure ou encadrez les tracés"); }
                else { ui.label(format!("{} · {} point(s)", self.mode.label(), self.pending.len())); }
            });
        ui.add_space(5.0);
    }

    fn viewer_zoom(&mut self, factor: f32) {
        self.zoom = ((if self.fit_view { self.zoom_percent / 100.0 } else { self.zoom }) * factor).clamp(0.05, 32.0);
        self.fit_view = false;
    }
}

//! Transactional numeric geometry editor: preview is never an observation.
use super::*;
use crate::numeric_resize::{self, Anchor};

pub(super) struct NumericResizeDialog {
    before: Measurement,
    revision: u64,
    capture: String,
    primary: String,
    secondary: Option<String>,
    anchor: Anchor,
    radius: bool,
    focus: bool,
    error: String,
    position: Option<Pos2>,
    initial_primary: String,
    initial_secondary: Option<String>,
}

fn input_value(value: f64) -> String {
    let text = format!("{value:.6}").trim_end_matches('0').trim_end_matches('.').to_owned();
    if text == "0" { value.to_string() } else { text }
}

pub(super) fn measurement_name(m: &Measurement) -> String {
    if m.label.trim().is_empty() { format!("M{}", m.id) } else { m.label.trim().to_owned() }
}

impl NumericResizeDialog {
    pub(super) fn measurement_id(&self) -> u64 { self.before.id }
    fn preview(&self, width: u32, height: u32) -> Result<Measurement, String> {
        let primary = numeric_resize::parse_value(&self.primary)?;
        let secondary = self.secondary.as_deref().map(numeric_resize::parse_value).transpose()?;
        let mut result = self.before.clone();
        if self.primary != self.initial_primary || self.secondary != self.initial_secondary {
            result.points = numeric_resize::resize(&self.before, primary, secondary, self.anchor, self.radius)?;
        }
        if geometry::exceeds_image_bounds(result.kind, &result.points, width, height) {
            return Err("La géométrie redimensionnée sortirait de l’image.".into());
        }
        result.origin = "numeric_resize".into();
        if !self.before.is_numeric_resize() {
            result.method = Some(format!("[dper:numeric_resize:v1] Géométrie imposée par saisie ; aucune observation de contour. {}",
                self.before.method.as_deref().unwrap_or_default()));
        }
        result.expanded_uncertainty_mm = None;
        Ok(result)
    }
}

impl DperMeasureApp {
    pub(super) fn open_numeric_resize(&mut self, id: u64) {
        if self.numeric_resize_dialog.is_some() || self.other_action_blocked()
            || self.registration_photo_pick_owns_frame() || !self.image_is_editable()
            || self.pattern_dialog.is_some() || self.confirm_close_image {
            self.message = "Terminez l’action en cours avant de redimensionner.".into();
            return;
        }
        let Some(before) = self.measurements.iter().find(|m| m.id == id && m.capture_id == self.capture_id).cloned() else { return; };
        if before.dependency_stale {
            self.message = "Cette géométrie dépend de sources modifiées ; recréez-la avant de la redimensionner.".into();
            return;
        }
        let dimensions = match numeric_resize::dimensions(&before, false) {
            Ok(value) => value,
            Err(error) => { self.message = error; return; }
        };
        self.cancel_selection_gesture();
        self.set_measurement_selection(Some(id));
        self.numeric_resize_dialog = Some(NumericResizeDialog {
            before, revision: self.document_revision, capture: self.capture_id.clone(),
            primary: input_value(dimensions.primary), secondary: dimensions.secondary.map(input_value),
            initial_primary: input_value(dimensions.primary), initial_secondary: dimensions.secondary.map(input_value), position: None,
            anchor: Anchor::First, radius: false, focus: true, error: String::new(),
        });
    }

    pub(super) fn commit_numeric_resize(&mut self) -> Result<(), String> {
        let dialog = self.numeric_resize_dialog.as_ref().ok_or("Aucune saisie en cours.")?;
        let before = dialog.before.clone();
        let current = self.measurements.iter().find(|m| m.id == before.id).ok_or("Géométrie absente.")?;
        if dialog.revision != self.document_revision || dialog.capture != self.capture_id
            || serde_json::to_value(current).ok() != serde_json::to_value(&before).ok() || !self.image_is_editable() {
            return Err("Le contexte a changé. Annulez puis rouvrez la saisie.".into());
        }
        let image = self.image.as_ref().ok_or("Image absente.")?;
        if dialog.primary == dialog.initial_primary && dialog.secondary == dialog.initial_secondary {
            self.numeric_resize_dialog = None;
            return Ok(());
        }
        let after = dialog.preview(image.width, image.height)?;
        if after.points == before.points {
            self.numeric_resize_dialog = None;
            return Ok(());
        }
        let target = self.measurements.iter_mut().find(|m| m.id == before.id).unwrap();
        *target = after.clone();
        self.numeric_resize_dialog = None;
        self.record_measurement_action(MeasurementHistoryAction::Edited { id: before.id, before, after });
        self.message = "Dimension modifiée. Ctrl+Z pour annuler.".into();
        Ok(())
    }

    pub(super) fn paint_numeric_resize_preview(&mut self, painter: &egui::Painter, rect: Rect, w: f32, h: f32) {
        let Some(dialog) = self.numeric_resize_dialog.as_ref() else { return; };
        let position = self.measurement_label_card_rect(painter, rect, w, h, &dialog.before)
            .map(|card| card.min).unwrap_or_else(|| {
                let points = dialog.before.points.iter().map(|p| Self::point_to_screen(*p, rect, w, h)).collect::<Vec<_>>();
                Self::label_card_min(&dialog.before, &points, rect, w, h, painter.clip_rect(), Vec2::new(240.0, 65.0))
            });
        let preview = dialog.preview(w as u32, h as u32).unwrap_or_else(|_| dialog.before.clone());
        self.render_measurement(painter, rect, w, h, &preview, Color32::from_rgb(245, 177, 42), true);
        self.numeric_resize_dialog.as_mut().unwrap().position = Some(position);
    }

    pub(super) fn numeric_resize_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.numeric_resize_dialog.as_mut() else { return; };
        let Some(position) = dialog.position else { return; };
        let mut apply = false;
        let mut cancel = false;
        let response = egui::Area::new(egui::Id::new("numeric-resize-inline"))
            .order(egui::Order::Foreground).fixed_pos(position).movable(false)
            .show(ctx, |ui| {
                // This cartouche is dark even when the application uses light visuals.
                ui.visuals_mut().text_edit_bg_color = Some(Color32::from_rgb(18, 38, 58));
                ui.visuals_mut().selection.bg_fill = Color32::from_rgb(0, 90, 140);
                ui.visuals_mut().selection.stroke = Stroke::new(1.0, Color32::WHITE);
                egui::Frame::new().fill(Color32::from_rgb(3, 10, 20))
                    .stroke(Stroke::new(2.0, Color32::from_rgb(239, 177, 42)))
                    .inner_margin(egui::Margin::symmetric(9, 5)).show(ui, |ui| {
                        ui.set_max_width(320.0);
                        ui.label(egui::RichText::new(measurement_name(&dialog.before)).size(11.5)
                            .color(Color32::from_rgb(168, 204, 226)));
                        let unit = if numeric_resize::dimensions(&dialog.before, false).unwrap().calibrated { "mm" } else { "px" };
                        ui.horizontal(|ui| {
                            if dialog.before.kind == Tool::Circle { ui.label(egui::RichText::new("Ø").color(Color32::WHITE)); }
                            let edit = ui.add(egui::TextEdit::singleline(&mut dialog.primary)
                                .id(egui::Id::new("numeric-resize-primary")).desired_width(92.0)
                                .background_color(Color32::from_rgb(18, 38, 58))
                                .font(egui::TextStyle::Body).text_color(Color32::WHITE));
                            if dialog.focus {
                                edit.request_focus();
                                if let Some(mut state) = egui::TextEdit::load_state(ctx, edit.id) {
                                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                                        egui::text::CCursor::new(0), egui::text::CCursor::new(dialog.primary.chars().count()))));
                                    state.store(ctx, edit.id);
                                }
                                dialog.focus = false;
                            }
                            if let Some(height) = dialog.secondary.as_mut() {
                                ui.label(egui::RichText::new("×").color(Color32::WHITE));
                                ui.add(egui::TextEdit::singleline(height).desired_width(92.0)
                                    .background_color(Color32::from_rgb(18, 38, 58)).text_color(Color32::WHITE));
                            }
                            ui.label(egui::RichText::new(unit).color(Color32::WHITE));
                        });
                        let preview = self.image.as_ref().ok_or_else(|| "Image absente.".to_owned())
                            .and_then(|image| dialog.preview(image.width, image.height));
                        if let Err(error) = &preview { ui.colored_label(Color32::LIGHT_RED, error); }
                        if !dialog.error.is_empty() { ui.colored_label(Color32::LIGHT_RED, &dialog.error); }
                        apply = preview.is_ok() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    });
            });
        // An outside click cancels the draft; it never moves the underlying measurement.
        cancel |= ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos()
            .is_some_and(|point| !response.response.rect.contains(point)));
        if cancel { self.numeric_resize_dialog = None; }
        else if apply {
            if let Err(error) = self.commit_numeric_resize() {
                if let Some(dialog) = self.numeric_resize_dialog.as_mut() { dialog.error = error; }
            }
        }
    }

}

#[cfg(test)]
#[path = "numeric_resize_ui_tests.rs"]
mod tests;

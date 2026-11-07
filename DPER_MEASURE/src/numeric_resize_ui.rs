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
}

impl NumericResizeDialog {
    fn preview(&self, width: u32, height: u32) -> Result<Measurement, String> {
        let primary = numeric_resize::parse_value(&self.primary)?;
        let secondary = self.secondary.as_deref().map(numeric_resize::parse_value).transpose()?;
        let mut result = self.before.clone();
        result.points = numeric_resize::resize(&self.before, primary, secondary, self.anchor, self.radius)?;
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
            primary: dimensions.primary.to_string(), secondary: dimensions.secondary.map(|v| v.to_string()),
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
        let after = dialog.preview(image.width, image.height)?;
        if after.points == before.points {
            self.numeric_resize_dialog = None;
            return Ok(());
        }
        let target = self.measurements.iter_mut().find(|m| m.id == before.id).unwrap();
        *target = after.clone();
        self.numeric_resize_dialog = None;
        self.record_measurement_action(MeasurementHistoryAction::Edited { id: before.id, before, after });
        self.message = "Géométrie redimensionnée · valeur imposée, conformité suspendue. Ctrl+Z pour annuler.".into();
        Ok(())
    }

    pub(super) fn paint_numeric_resize_preview(&self, painter: &egui::Painter, rect: Rect, w: f32, h: f32) {
        if let Some(dialog) = &self.numeric_resize_dialog {
            if let Ok(preview) = dialog.preview(w as u32, h as u32) {
                self.render_measurement(painter, rect, w, h, &preview, Color32::from_rgb(245, 177, 42), false);
            }
        }
    }

    pub(super) fn numeric_resize_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.numeric_resize_dialog.as_mut() else { return; };
        let mut apply = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("numeric-resize")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading("Redimensionner la géométrie");
            ui.label(format!("M{} · {}", dialog.before.id, dialog.before.type_label()));
            ui.label("Valeur imposée : ce changement ne constitue pas une mesure des contours et suspend la conformité.");
            let dimensions = numeric_resize::dimensions(&dialog.before, dialog.radius).unwrap();
            let unit = if dimensions.calibrated { "mm" } else { "px" };
            if dialog.before.kind == Tool::Circle {
                let old = dialog.radius;
                ui.horizontal(|ui| {
                    ui.radio_value(&mut dialog.radius, false, "Diamètre");
                    ui.radio_value(&mut dialog.radius, true, "Rayon");
                });
                if old != dialog.radius {
                    if let Ok(value) = numeric_resize::parse_value(&dialog.primary) {
                        dialog.primary = (value * if dialog.radius { 0.5 } else { 2.0 }).to_string();
                    }
                }
            }
            ui.horizontal(|ui| {
                ui.label(if dialog.secondary.is_some() { "Longueur" } else { "Valeur" });
                let response = ui.text_edit_singleline(&mut dialog.primary);
                if dialog.focus { response.request_focus(); dialog.focus = false; }
                ui.label(unit);
            });
            if let Some(height) = dialog.secondary.as_mut() {
                ui.horizontal(|ui| { ui.label("Largeur"); ui.text_edit_singleline(height); ui.label(unit); });
            }
            if dialog.before.kind != Tool::Circle {
                ui.label("Point conservé");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut dialog.anchor, Anchor::First, "Premier point");
                    ui.radio_value(&mut dialog.anchor, Anchor::Last, "Point opposé");
                    ui.radio_value(&mut dialog.anchor, Anchor::Center, "Centre");
                });
            } else { ui.label("Centre conservé"); }
            if dialog.before.kind == Tool::Circle && dimensions.calibrated {
                ui.small("La dimension utilise l’échelle de cette géométrie dans la direction centre–bord ; elle ne qualifie pas un diamètre physique sous échelles X/Y différentes.");
            }
            let preview = self.image.as_ref().ok_or_else(|| "Image absente.".to_owned())
                .and_then(|image| dialog.preview(image.width, image.height));
            if let Err(error) = &preview { ui.colored_label(Color32::RED, error); }
            if !dialog.error.is_empty() { ui.colored_label(Color32::RED, &dialog.error); }
            ui.horizontal(|ui| {
                apply = ui.add_enabled(preview.is_ok(), egui::Button::new("Valider")).clicked()
                    || (preview.is_ok() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                cancel = ui.button("Annuler").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        });
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

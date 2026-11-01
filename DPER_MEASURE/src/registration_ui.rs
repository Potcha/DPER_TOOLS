//! Explicit native landmark drafts. Only the shared Register operation installs
//! a similarity; draft preparation never derives landmarks or changes calibration.
use crate::{
    dper_registration::Correspondence,
    geometry::Point,
    mcp_server::{InspectionRequest, RegisterInspectionArgs},
    DperMeasureApp,
};
use eframe::egui;

const MAX_ROWS: usize = 64;

#[derive(Default)]
pub(crate) struct RegistrationUi {
    draft: Option<RegistrationDraft>,
    status: String,
}

#[derive(Clone, PartialEq, Eq)]
struct PreparedContext {
    capture_id: String,
    revision: u64,
    plan_identity: String,
    source_path: String,
    source_fingerprint: String,
    source_metadata: String,
    image_fingerprint: u64,
}

struct RegistrationDraft {
    prepared: PreparedContext,
    rows: Vec<[String; 4]>,
    threshold_px: String,
    view_id: Option<String>,
}

fn number(text: &str, label: &str) -> Result<f64, String> {
    text.trim().replace(',', ".").parse::<f64>().ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{label} : entrez un nombre fini."))
}

impl RegistrationDraft {
    fn request(&self) -> Result<RegisterInspectionArgs, String> {
        if !(3..=MAX_ROWS).contains(&self.rows.len()) {
            return Err("Renseignez entre 3 et 64 repères explicites distincts.".into());
        }
        let mut correspondences = Vec::with_capacity(self.rows.len());
        for (index, row) in self.rows.iter().enumerate() {
            let mut values = [0.0; 4];
            for (column, label) in ["plan X", "plan Y", "image X", "image Y"].iter().enumerate() {
                values[column] = number(&row[column], &format!("Repère {} · {label}", index + 1))?;
            }
            correspondences.push(Correspondence {
                plan: Point { x: values[0], y: values[1] },
                image: Point { x: values[2], y: values[3] },
            });
        }
        Ok(RegisterInspectionArgs {
            capture_id: self.prepared.capture_id.clone(),
            expected_revision: self.prepared.revision,
            correspondences,
            maximum_error_px: number(&self.threshold_px, "Seuil de résidu en pixels")?,
            drawing_view_id: self.view_id.clone(),
        })
    }
}

impl DperMeasureApp {
    fn registration_unavailable_reason(&self) -> Option<String> {
        let Some(plan) = &self.inspection.plan else {
            return Some("Importez d'abord un plan PDF.".into());
        };
        if !plan.source_file.as_ref().is_some_and(|path| path.to_lowercase().ends_with(".pdf")) {
            return Some("Cet éditeur utilise les coordonnées en points d'un PDF source. Importez un plan PDF.".into());
        }
        if self.image.is_none() { return Some("Ouvrez une image de pièce avant de préparer les repères.".into()); }
        if !self.image_is_editable() { return Some("Figez l'image de la caméra avant de préparer ou appliquer le recalage.".into()); }
        if self.mcp_mutation_blocked() { return Some("Terminez ou annulez le tracé, l'édition, l'étalonnage ou l'export en cours.".into()); }
        None
    }

    fn registration_prepared_context(&self) -> Result<PreparedContext, String> {
        if let Some(reason) = self.registration_unavailable_reason() { return Err(reason); }
        let (source_path, source_fingerprint) = self.current_drawing_source()?;
        let source = self.image.as_ref().ok_or("Aucune image active.")?;
        let image = image::RgbImage::from_raw(source.width, source.height, source.rgb.clone())
            .ok_or("Tampon image invalide.")?;
        Ok(PreparedContext {
            capture_id: self.capture_id.clone(),
            revision: self.document_revision,
            plan_identity: serde_json::to_string(self.inspection.plan.as_ref().unwrap())
                .map_err(|error| format!("Plan invalide : {error}"))?,
            source_path,
            source_fingerprint,
            source_metadata: self.drawing_source_cache_key(),
            image_fingerprint: crate::dper_inspection::fingerprint(&image),
        })
    }

    /// No implicit reload: every preparation snapshots the current context, and
    /// starting again deliberately discards the old editable draft.
    fn prepare_native_registration(&mut self, reload: bool) -> Result<(), String> {
        let prepared = self.registration_prepared_context()?;
        let (rows, threshold_px, view_id) = if reload {
            let report = self.inspection.registration.as_ref().ok_or("Aucun recalage à reprendre.")?;
            if self.inspection.registration_capture_id.as_deref() != Some(prepared.capture_id.as_str())
                || self.inspection.registration_fingerprint != Some(prepared.image_fingerprint)
                || !(3..=MAX_ROWS).contains(&report.correspondences.len())
            {
                return Err("Les repères enregistrés ne correspondent pas à cette capture ou sont historiques/incomplets. Préparez un nouveau brouillon.".into());
            }
            let threshold = report.threshold_px.ok_or("Le recalage historique n'a pas de seuil conservé. Préparez un nouveau brouillon.")?;
            crate::dper_registration::register_similarity(&report.correspondences, threshold)?;
            let view_id = self.inspection.registration_scope.as_ref().map(|scope| scope.view_id.clone());
            if let Some(scope) = &self.inspection.registration_scope {
                if self.drawing_registration_scope(&scope.view_id)? != *scope {
                    return Err("La vue du recalage a changé. Préparez un nouveau brouillon.".into());
                }
            }
            let rows = report.correspondences.iter().map(|pair| [
                pair.plan.x.to_string(), pair.plan.y.to_string(), pair.image.x.to_string(), pair.image.y.to_string(),
            ]).collect();
            (rows, threshold.to_string(), view_id)
        } else {
            (vec![Default::default(); 3], "2".into(), None)
        };
        self.registration_ui.draft = Some(RegistrationDraft { prepared, rows, threshold_px, view_id });
        Ok(())
    }

    fn native_registration_stale(&self) -> bool {
        self.registration_ui.draft.as_ref().is_some_and(|draft| {
            draft.prepared.revision != self.document_revision
                || draft.prepared.capture_id != self.capture_id
                || draft.prepared.source_metadata != self.drawing_source_cache_key()
        })
    }

    fn apply_native_registration(&mut self) -> Result<(), String> {
        let draft = self.registration_ui.draft.as_ref().ok_or("Préparez d'abord un brouillon de repères.")?;
        if self.native_registration_stale() {
            return Err("Le contexte a changé depuis la préparation. Préparez un nouveau brouillon ou reprenez explicitement les repères du recalage courant.".into());
        }
        if draft.prepared != self.registration_prepared_context()? {
            return Err("La capture ou le plan source a changé depuis la préparation. Préparez de nouveau les repères.".into());
        }
        let request = draft.request()?;
        // The saved revision/capture are intentional: refreshing them here would
        // hide changes that occurred while the operator edited the draft.
        self.handle_inspection_request(InspectionRequest::Register(request))?;
        Ok(())
    }

    pub(crate) fn registration_editor_panel(&mut self, ui: &mut egui::Ui) {
        if self.inspection.plan.is_none() { return; }
        egui::CollapsingHeader::new("Recalage par repères explicites")
            .default_open(true).show(ui, |ui| {
                ui.label("Saisie experte manuelle : associez au moins trois repères distincts réellement identifiés sur le plan et la photo.");
                ui.weak("Plan : points PDF, origine en haut à gauche. Image : pixels natifs, origine en haut à gauche ; ni pixels d'écran ni millimètres.");
                ui.weak("Une similitude ajuste translation, rotation et échelle pour localiser la recherche. Cette échelle ne remplace jamais l'étalonnage physique. Aucun repère n'est déduit du nominal.");
                let unavailable = self.registration_unavailable_reason();
                if let Some(reason) = &unavailable { ui.label(reason); }
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(unavailable.is_none(), egui::Button::new("Nouveau brouillon (effacer la saisie)")).clicked() {
                        self.registration_ui.status = match self.prepare_native_registration(false) {
                            Ok(()) => "Brouillon préparé pour la capture et la révision actuelles. Renseignez les coordonnées.".into(),
                            Err(error) => error,
                        };
                    }
                    if ui.add_enabled(unavailable.is_none() && self.inspection.registration.is_some(),
                        egui::Button::new("Reprendre les repères du recalage courant")).clicked() {
                        self.registration_ui.status = match self.prepare_native_registration(true) {
                            Ok(()) => "Repères repris comme brouillon modifiable. Vérifiez chaque correspondance avant d'appliquer.".into(),
                            Err(error) => error,
                        };
                    }
                });
                let stale = self.native_registration_stale();
                if stale { ui.label("Brouillon périmé : le contexte a changé. La saisie est conservée pour consultation ; préparez ou reprenez explicitement les repères avant application."); }
                let views = self.inspection.views.iter().map(|view| (view.id.clone(), format!("{} · page {}", view.label, view.page))).collect::<Vec<_>>();
                let Some(draft) = &mut self.registration_ui.draft else {
                    if !self.registration_ui.status.is_empty() { ui.label(&self.registration_ui.status); }
                    return;
                };
                ui.label(format!("Capture préparée : {} · révision {}", draft.prepared.capture_id, draft.prepared.revision));
                if let Some(image) = &self.image { ui.label(format!("Image native : {} × {} pixels", image.width, image.height)); }
                let selected = draft.view_id.as_ref().and_then(|id| views.iter().find(|(view_id, _)| view_id == id))
                    .map(|(_, label)| label.as_str()).unwrap_or("Sans vue — repère plan explicite");
                egui::ComboBox::from_id_salt("native_registration_view").selected_text(selected).show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.view_id, None, "Sans vue — repère plan explicite");
                    for (id, label) in &views { ui.selectable_value(&mut draft.view_id, Some(id.clone()), label); }
                });
                if draft.view_id.is_none() { ui.weak("Sans vue, la page et les limites du repère plan sont à vérifier manuellement. Une inspection liée au PDF exige une vue explicite existante."); }
                let mut remove = None;
                egui::ScrollArea::horizontal().id_salt("native_registration_rows_scroll").show(ui, |ui| {
                    egui::Grid::new("native_registration_rows").striped(true).show(ui, |ui| {
                        for heading in ["Repère", "Plan X (pt)", "Plan Y (pt)", "Image X (px)", "Image Y (px)", ""] { ui.label(heading); }
                        ui.end_row();
                        for (index, row) in draft.rows.iter_mut().enumerate() {
                            ui.label((index + 1).to_string());
                            for (column, coordinate) in row.iter_mut().enumerate() {
                                ui.add(egui::TextEdit::singleline(coordinate).desired_width(65.0)
                                    .id_salt(("native_registration_coordinate", index, column)));
                            }
                            if ui.button("Retirer").clicked() { remove = Some(index); }
                            ui.end_row();
                        }
                    });
                });
                if let Some(index) = remove { draft.rows.remove(index); }
                if ui.add_enabled(draft.rows.len() < MAX_ROWS, egui::Button::new("Ajouter un repère (64 maximum)")).clicked() {
                    draft.rows.push(Default::default());
                }
                ui.horizontal(|ui| {
                    ui.label("Seuil de résidu (pixels natifs)");
                    ui.add(egui::TextEdit::singleline(&mut draft.threshold_px).desired_width(65.0));
                });
                let apply = ui.add_enabled(unavailable.is_none() && !stale,
                    egui::Button::new("Calculer et appliquer la similitude")).clicked();
                if apply {
                    self.registration_ui.status = match self.apply_native_registration() {
                        Ok(()) => "Recalage appliqué. Consultez les repères retenus/rejetés et les résidus dans le plan de contrôle. Les observations antérieures ont été invalidées ; l'étalonnage reste indépendant.".into(),
                        Err(error) => error,
                    };
                }
                if !self.registration_ui.status.is_empty() { ui.label(&self.registration_ui.status); }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        app: DperMeasureApp,
        path: std::path::PathBuf,
        ctx: egui::Context,
    }

    impl Fixture {
        fn new() -> Self {
            let ctx = egui::Context::default();
            let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
            let path = std::env::temp_dir().join(format!("{}.pdf", crate::new_id("dper-native-registration")));
            std::fs::write(&path, b"source fingerprint fixture, not parsed by these tests").unwrap();
            let mut plan = crate::dper_drawing::parse_text_plan("Diametre 20 mm", "fixture.pdf");
            plan.source_file = Some(path.display().to_string());
            plan.source_fingerprint = Some(crate::dper_source::fingerprint_file(&path).unwrap());
            plan.page_geometry = vec![crate::dper_pdf_layout::PdfPageGeometry {
                page: 1, width_pt: 100.0, height_pt: 100.0,
            }];
            app.install_inspection_plan(plan);
            app.image = Some(crate::ImageData {
                width: 100, height: 100, rgb: vec![128; 100 * 100 * 3], source: "native-test.png".into(),
            });
            app.mm_per_pixel = Some(0.125);
            app.calibration = Some(serde_json::from_value(serde_json::json!({
                "device": "independent reference", "resolution": [100,100], "optical_setup": "test",
                "mm_per_pixel": 0.125, "reference_mm": 10.0, "reference_px": 80.0,
            })).unwrap());
            Self { app, path, ctx }
        }

        fn prepare_valid(&mut self) {
            self.app.prepare_native_registration(false).unwrap();
            self.app.registration_ui.draft.as_mut().unwrap().rows = vec![
                ["5", "5", "20", "30"].map(str::to_owned),
                ["25", "5", "60", "30"].map(str::to_owned),
                ["5", "25", "20", "70"].map(str::to_owned),
            ];
            self.app.registration_ui.draft.as_mut().unwrap().threshold_px = "0.1".into();
        }

        fn render(&mut self) {
            let output = self.ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| self.app.registration_editor_panel(ui));
            });
            assert!(!output.shapes.is_empty());
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) { let _ = std::fs::remove_file(&self.path); }
    }

    #[test]
    fn native_registration_matches_shared_operation_and_preserves_independent_calibration() {
        let mut native = Fixture::new();
        native.prepare_valid();
        let calibration = serde_json::to_value(&native.app.calibration).unwrap();
        let mut direct = Fixture::new();
        direct.prepare_valid();
        let request = direct.app.registration_ui.draft.as_ref().unwrap().request().unwrap();
        direct.app.handle_inspection_request(InspectionRequest::Register(request)).unwrap();
        native.app.apply_native_registration().unwrap();
        let report = native.app.inspection.registration.as_ref().unwrap();
        assert!((report.transform.scale - 2.0).abs() < 1e-10);
        assert!(report.transform.rotation_rad.abs() < 1e-10);
        assert!((report.transform.translation.x - 10.0).abs() < 1e-10);
        assert!((report.transform.translation.y - 20.0).abs() < 1e-10);
        assert_eq!(report.inlier_indices.len(), 3);
        assert_eq!(serde_json::to_value(report).unwrap(), serde_json::to_value(&direct.app.inspection.registration).unwrap());
        assert_eq!(serde_json::to_value(&native.app.calibration).unwrap(), calibration);
        assert_eq!(native.app.mm_per_pixel, Some(0.125));
        assert!(native.app.measurements.is_empty());
        assert!(native.app.native_registration_stale());
        assert!(native.app.apply_native_registration().is_err());
        native.app.prepare_native_registration(true).unwrap();
        assert!(!native.app.native_registration_stale());
        assert_eq!(native.app.registration_ui.draft.as_ref().unwrap().rows.len(), 3);
    }

    #[test]
    fn prepared_draft_refuses_revision_capture_plan_source_or_pixel_change() {
        for change in 0..5 {
            let mut fixture = Fixture::new();
            fixture.prepare_valid();
            let prepared_revision = fixture.app.registration_ui.draft.as_ref().unwrap().prepared.revision;
            match change {
                0 => fixture.app.document_revision += 1,
                1 => fixture.app.capture_id = "different-capture".into(),
                2 => fixture.app.inspection.plan.as_mut().unwrap().source_name = "different-plan".into(),
                3 => {
                    let length = std::fs::metadata(&fixture.path).unwrap().len() as usize;
                    std::fs::write(&fixture.path, vec![b'x'; length]).unwrap();
                }
                _ => fixture.app.image.as_mut().unwrap().rgb[0] = 42,
            }
            let before = serde_json::to_value(&fixture.app.inspection).unwrap();
            assert!(fixture.app.apply_native_registration().is_err(), "change {change}");
            assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), before);
            assert_eq!(fixture.app.registration_ui.draft.as_ref().unwrap().prepared.revision, prepared_revision);
        }
    }

    #[test]
    fn invalid_draft_leaves_existing_registration_and_revision_unchanged() {
        for invalid in 0..8 {
            let mut fixture = Fixture::new();
            fixture.prepare_valid();
            fixture.app.apply_native_registration().unwrap();
            fixture.app.prepare_native_registration(true).unwrap();
            let draft = fixture.app.registration_ui.draft.as_mut().unwrap();
            match invalid {
                0 => draft.rows[0][0].clear(),
                1 => draft.rows[0][1] = "NaN".into(),
                2 => draft.rows[0][2] = "100".into(), // Native image edge is exclusive.
                3 => draft.rows[1] = draft.rows[0].clone(),
                4 => draft.threshold_px = "-1".into(),
                5 => { draft.rows.pop(); },
                6 => draft.rows.resize(65, ["1", "2", "3", "4"].map(str::to_owned)),
                _ => draft.rows[0][0] = "1e100".into(),
            }
            let revision = fixture.app.document_revision;
            let before = serde_json::to_value(&fixture.app.inspection).unwrap();
            assert!(fixture.app.apply_native_registration().is_err(), "invalid case {invalid}");
            assert_eq!(fixture.app.document_revision, revision);
            assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), before);
        }
    }

    #[test]
    fn native_editor_requires_frozen_image_and_no_action_in_progress() {
        let mut fixture = Fixture::new();
        fixture.prepare_valid();
        fixture.app.running = true;
        assert!(fixture.app.apply_native_registration().unwrap_err().contains("Figez"));
        fixture.app.paused = true;
        fixture.app.pending.push(Point { x: 5.0, y: 5.0 });
        assert!(fixture.app.apply_native_registration().unwrap_err().contains("Terminez"));
        fixture.app.pending.clear();
        fixture.app.apply_native_registration().unwrap();
    }

    #[test]
    fn existing_landmarks_are_never_implicitly_loaded_on_different_capture() {
        let mut fixture = Fixture::new();
        fixture.prepare_valid();
        fixture.app.apply_native_registration().unwrap();
        fixture.app.capture_id = "new-image".into();
        assert!(fixture.app.prepare_native_registration(true).is_err());
        fixture.app.prepare_native_registration(false).unwrap();
        assert!(fixture.app.registration_ui.draft.as_ref().unwrap().rows.iter()
            .all(|row| row.iter().all(String::is_empty)));
    }

    #[test]
    fn headless_editor_renders_empty_editable_and_stale_drafts_without_mutation() {
        let mut fixture = Fixture::new();
        fixture.render();
        fixture.prepare_valid();
        fixture.render();
        let rows = fixture.app.registration_ui.draft.as_ref().unwrap().rows.clone();
        fixture.app.document_revision += 1;
        fixture.render();
        assert!(fixture.app.native_registration_stale());
        assert_eq!(fixture.app.registration_ui.draft.as_ref().unwrap().rows, rows);
        assert!(fixture.app.inspection.registration.is_none());
        assert!(fixture.app.measurements.is_empty());
    }
}

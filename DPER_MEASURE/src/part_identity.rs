//! Stable identifiers shared by the saved captures of one part.
use serde::{Deserialize, Serialize};
use std::{fmt::Write as _, fs, io::Write as _, path::{Path, PathBuf}};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PartIdentity {
    pub lot_id: String,
    pub part_id: String,
    pub requirements_revision: String,
    pub drawing_revision: Option<String>,
}

impl PartIdentity {
    pub(crate) fn new(lot: &str, part: &str, requirements: &str, drawing: &str) -> Result<Self, String> {
        let value = Self {
            lot_id: lot.trim().into(),
            part_id: part.trim().into(),
            requirements_revision: requirements.trim().into(),
            drawing_revision: (!drawing.trim().is_empty()).then(|| drawing.trim().to_owned()),
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        for value in [&self.lot_id, &self.part_id] {
            if value.is_empty() || value.trim() != value || value.len() > 96 || value.chars().any(char::is_control) {
                return Err("Renseignez des identifiants de lot et de pièce valides (96 octets maximum).".into());
            }
        }
        if self.requirements_revision.trim().is_empty() {
            return Err("Renseignez la révision des exigences applicables à la pièce.".into());
        }
        Ok(())
    }

    pub(crate) fn create(&self, root: &Path) -> Result<(), String> {
        self.validate()?;
        let parent = catalog_lot_path(root, &self.lot_id);
        let path = parent.join(format!("{}.json", encode_key(&self.part_id)));
        fs::create_dir_all(&parent).map_err(|error| format!("Création du registre des pièces : {error}"))?;
        let mut staged = tempfile::NamedTempFile::new_in(&parent)
            .map_err(|error| format!("Préparation de la fiche pièce : {error}"))?;
        let content = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        staged.write_all(&content).and_then(|_| staged.as_file().sync_all())
            .map_err(|error| format!("Écriture de la fiche pièce : {error}"))?;
        staged.persist_noclobber(&path).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                "Cette pièce est déjà déclarée dans ce lot. Associez la capture à la pièce existante.".into()
            } else {
                format!("Publication de la fiche pièce : {}", error.error)
            }
        })?;
        Ok(())
    }

    pub(crate) fn load(root: &Path, lot: &str, part: &str) -> Result<Self, String> {
        let lot = lot.trim();
        let part = part.trim();
        if lot.is_empty() || part.is_empty() || lot.len() > 96 || part.len() > 96 {
            return Err("Renseignez le lot et la pièce à retrouver.".into());
        }
        let path = catalog_lot_path(root, lot).join(format!("{}.json", encode_key(part)));
        let bytes = fs::read(path).map_err(|error| format!("Lecture de la fiche pièce : {error}"))?;
        let record: Self = serde_json::from_slice(&bytes).map_err(|error| format!("Fiche pièce illisible : {error}"))?;
        record.validate()?;
        if record.lot_id != lot || record.part_id != part {
            return Err("La fiche trouvée porte une autre identité de lot ou de pièce.".into());
        }
        Ok(record)
    }

    pub(crate) fn matches(&self, lot: &str, part: &str) -> bool {
        (lot.is_empty() || self.lot_id == lot) && (part.is_empty() || self.part_id == part)
    }
}

fn catalog_lot_path(root: &Path, lot: &str) -> PathBuf {
    root.join(".dper-measure-parts").join(encode_key(lot))
}

fn encode_key(value: &str) -> String {
    // Encode bytes so separators and filesystem case folding cannot alias identifiers.
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        write!(encoded, "{byte:02x}").unwrap();
    }
    encoded
}

#[derive(Default)]
pub(super) struct PartEditor {
    lot: String,
    part: String,
    requirements: String,
    drawing: String,
    message: String,
}

impl crate::DperMeasureApp {
    pub(super) fn start_part_editor(&mut self) {
        if !self.image_is_editable() || self.mcp_mutation_blocked() {
            self.message = "Figez l’image et terminez l’action en cours avant d’associer une pièce.".into();
            return;
        }
        self.part_editor = Some(self.part_identity.as_ref().map_or_else(PartEditor::default, |identity| {
            PartEditor {
                lot: identity.lot_id.clone(),
                part: identity.part_id.clone(),
                requirements: identity.requirements_revision.clone(),
                drawing: identity.drawing_revision.clone().unwrap_or_default(),
                message: String::new(),
            }
        }));
    }

    pub(super) fn part_editor_window(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.part_editor.take() else { return; };
        let mut open = true;
        let mut create = false;
        let mut attach = false;
        let editable = self.image_is_editable() && !self.mcp_mutation_blocked();
        egui::Window::new("Lot et pièce")
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.weak(self.directory.to_string_lossy());
                if let Some(identity) = &self.part_identity {
                    ui.label(format!("Capture associée au lot {} · pièce {}", identity.lot_id, identity.part_id));
                }
                egui::Grid::new("part-identity-fields").num_columns(2).show(ui, |ui| {
                    ui.label("Lot");
                    ui.text_edit_singleline(&mut editor.lot);
                    ui.end_row();
                    ui.label("Pièce");
                    ui.text_edit_singleline(&mut editor.part);
                    ui.end_row();
                    ui.label("Révision des exigences client");
                    ui.text_edit_singleline(&mut editor.requirements);
                    ui.end_row();
                    ui.label("Révision du plan de contrôle");
                    ui.text_edit_singleline(&mut editor.drawing);
                    ui.end_row();
                });
                ui.add_enabled_ui(editable, |ui| {
                    create = ui.button("Créer la pièce et associer la capture").clicked();
                    attach = ui.button("Associer à une pièce existante").clicked();
                });
                if !editor.message.is_empty() { ui.label(&editor.message); }
            });
        if create || attach {
            let result = if create {
                PartIdentity::new(&editor.lot, &editor.part, &editor.requirements, &editor.drawing)
                    .and_then(|identity| { identity.create(&self.directory)?; Ok(identity) })
            } else {
                PartIdentity::load(&self.directory, &editor.lot, &editor.part)
            };
            match result {
                Ok(identity) => {
                    self.message = format!("Capture associée au lot {} · pièce {}.", identity.lot_id, identity.part_id);
                    self.set_part_identity(identity);
                    open = false;
                }
                Err(error) => editor.message = error,
            }
        }
        if open { self.part_editor = Some(editor); }
    }

    fn set_part_identity(&mut self, identity: PartIdentity) {
        if self.part_identity.as_ref() != Some(&identity) {
            self.part_identity = Some(identity);
            self.session_saved = false;
            self.document_revision = self.document_revision.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llt_dat_004_01_duplicate_part_is_refused_without_replacing_its_revisions() {
        let workspace = tempfile::tempdir().unwrap();
        let first = PartIdentity::new("Série A", "P/001", "Client R1", "Contrôle C").unwrap();
        let second = PartIdentity::new("Série B", "P/001", "Client R2", "Contrôle D").unwrap();
        first.create(workspace.path()).unwrap();
        second.create(workspace.path()).unwrap();
        let duplicate = PartIdentity::new("Série A", "P/001", "Client R3", "Contrôle E").unwrap();
        assert!(duplicate.create(workspace.path()).unwrap_err().contains("déjà déclarée"));
        assert_eq!(PartIdentity::load(workspace.path(), "Série A", "P/001").unwrap(), first);
        assert_eq!(PartIdentity::load(workspace.path(), "Série B", "P/001").unwrap(), second);
    }

    #[test]
    fn hlt_dat_004_reopens_and_searches_captures_by_lot_and_part() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let source = root.join("source.png");
        image::RgbImage::from_pixel(32, 24, image::Rgb([120; 3])).save(&source).unwrap();
        let first = PartIdentity::new("A", "P01", "Exigences-A-v1", "Plan-A-v1").unwrap();
        let second = PartIdentity::new("B", "P01", "Exigences-B-v2", "Plan-B-v2").unwrap();
        first.create(root).unwrap();
        second.create(root).unwrap();

        let ctx = egui::Context::default();
        let mut app = crate::DperMeasureApp::new_with_services(ctx.clone(), false);
        let mut captures = Vec::new();
        for (file, label, identity) in [
            ("first.dperproj", "Ensemble", &first),
            ("detail.dperproj", "Détail", &first),
            ("other.dperproj", "Autre lot", &second),
        ] {
            app.open_image_path(&ctx, &source).unwrap();
            assert!(app.part_identity.is_none());
            app.set_part_identity(PartIdentity::load(root, &identity.lot_id, &identity.part_id).unwrap());
            let capture_id = app.capture_id.clone();
            let measurement = app.add_measurement(crate::Tool::Distance,
                vec![crate::Point { x: 2.0, y: 3.0 }, crate::Point { x: 12.0, y: 3.0 }],
                Some(label.into()), capture_id.clone(), None, Some(0.1), "manual".into(), None);
            app.measurements.last_mut().unwrap().expanded_uncertainty_mm = Some(0.02);
            let nominal_mm = if identity.lot_id == "A" { 1.0 } else { 2.0 };
            app.reference_document = Some(crate::reference::ReferenceDocument::new(&source,
                identity.requirements_revision.clone(), vec![crate::reference::ReferenceDimension {
                    id: "CPP-1".into(), label: "Distance".into(), nominal_mm,
                    tolerance_minus_mm: Some(0.1), tolerance_plus_mm: Some(0.1),
                    source_text: format!("{nominal_mm} ±0.1 mm"), confirmed: true,
                    measurement_id: Some(measurement.id), ocr_page: None, ocr_bbox: None, ocr_confidence: None,
                }]));
            let path = root.join(file);
            app.save_project_to_path(&path, false).unwrap();
            captures.push((path, capture_id, label));
        }

        let (lot_a, _, _) = crate::scan_project_search(root, "", "A", "P01").unwrap();
        let (lot_b, _, _) = crate::scan_project_search(root, "", "B", "P01").unwrap();
        assert_eq!(lot_a.len(), 2);
        assert_eq!(lot_b.len(), 1);
        let absent = crate::scan_project_search(root, "", "A", "P").unwrap().0;
        assert!(absent.is_empty(), "Une recherche de pièce doit utiliser son identifiant exact.");
        let mut reopened = crate::DperMeasureApp::new_with_services(ctx.clone(), false);
        for (result, expected) in lot_a.iter().map(|result| (result, &first))
            .chain(lot_b.iter().map(|result| (result, &second)))
        {
            let (_, capture_id, label) = captures.iter().find(|(path, _, _)| path == &result.path).unwrap();
            assert!(result.summary.contains(&expected.requirements_revision));
            reopened.open_project_path(&ctx, &result.path).unwrap();
            assert_eq!(reopened.part_identity.as_ref(), Some(expected));
            assert_eq!(&reopened.capture_id, capture_id);
            assert_eq!(reopened.measurements.len(), 1);
            assert_eq!(&reopened.measurements[0].label, label);
            assert_eq!(&reopened.measurements[0].capture_id, capture_id);
            let reference = reopened.reference_document.as_ref().unwrap();
            assert_eq!(reference.extracted_text, expected.requirements_revision);
            let status = crate::reference::part_status(reference, &reopened.measurements);
            assert_eq!(status, if expected.lot_id == "A" {
                crate::reference::PartStatus::InTolerance
            } else {
                crate::reference::PartStatus::OutOfTolerance
            });
        }
    }
}

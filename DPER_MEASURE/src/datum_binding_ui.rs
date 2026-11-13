//! Native review of saved documentary datum/support proposals.
//! Confirmation never resolves the physical datum or authorizes conformity.

use crate::{
    datum_binding::{self, DatumSupportAssociation},
    dper_binding,
    mcp_server::InspectDrawingDatumSupportArgs,
    DperMeasureApp,
};
use eframe::egui;
use serde_json::{json, Value};

impl DperMeasureApp {
    /// MCP can save a documentary proposal; it cannot confirm operator review.
    pub(crate) fn save_datum_association(
        &mut self,
        args: InspectDrawingDatumSupportArgs,
    ) -> Result<Value, String> {
        self.binding_mutation_guard(args.expected_revision)?;
        let view = dper_binding::unique_view(&self.inspection.views, &args.view_id)?;
        self.require_unambiguous_drawing_scope(view, &args.feature_id,
            std::slice::from_ref(&args.support_id))?;
        let catalog = self.current_catalog(view.page)?;
        let plan = self.inspection.plan.as_ref().ok_or("Aucun plan de contrôle actif.")?;
        let association = datum_binding::propose(plan, &catalog.document_fingerprint,
            &catalog.geometry, view, &args.feature_id, &args.label, &args.support_id,
            &args.justification)?;
        self.current_drawing_source()?;
        self.inspection.datum_associations.retain(|old|
            old.feature_id != association.feature_id || old.label != association.label);
        self.inspection.datum_associations.push(association.clone());
        self.datum_association_changed();
        Ok(json!({"document_revision": self.document_revision,
            "association": association,
            "requires_native_confirmation": true,
            "reference_resolved": false,
            "suspension_reason": "drawing_relation_unresolved"}))
    }

    fn datum_association_index(&self, id: &str) -> Result<usize, String> {
        let mut matches = self.inspection.datum_associations.iter().enumerate()
            .filter(|(_, association)| association.id == id);
        let (index, association) = matches.next().ok_or("Lien repère/support absent.")?;
        if matches.next().is_some() || self.inspection.datum_associations.iter().filter(|other|
            other.feature_id == association.feature_id && other.label == association.label).count() != 1
        {
            return Err("Lien repère/support ambigu ; recréez une proposition unique.".into());
        }
        Ok(index)
    }

    fn validate_datum_association_evidence(&self, association: &DatumSupportAssociation) -> Result<(), String> {
        let (_, source) = self.current_drawing_source()?;
        self.validate_datum_association_evidence_for_source(association, &source)
    }

    fn validate_datum_association_evidence_for_source(
        &self, association: &DatumSupportAssociation, source: &str,
    ) -> Result<(), String> {
        let index = self.datum_association_index(&association.id)?;
        if &self.inspection.datum_associations[index] != association {
            return Err("Le lien repère/support a changé depuis son affichage.".into());
        }
        let view = dper_binding::unique_view(&self.inspection.views, &association.view_id)?;
        self.require_unambiguous_drawing_scope(view, &association.feature_id,
            std::slice::from_ref(&association.support_id))?;
        let mut catalogs = self.drawing_geometry_cache.iter().filter(|catalog|
            catalog.geometry.page == view.page && catalog.document_fingerprint == source);
        let catalog = catalogs.next().ok_or("Analysez les géométries de cette page PDF dans la session courante.")?;
        if catalogs.next().is_some() { return Err("Catalogue de page ambigu.".into()); }
        let plan = self.inspection.plan.as_ref().ok_or("Aucun plan de contrôle actif.")?;
        datum_binding::validate_evidence(plan, &catalog.document_fingerprint,
            &catalog.geometry, view, association)
    }

    pub(crate) fn validate_datum_association(&self, association: &DatumSupportAssociation) -> Result<(), String> {
        self.validate_datum_association_evidence(association)?;
        if !association.reviewed {
            return Err("Proposition documentaire à confirmer explicitement dans cette session.".into());
        }
        Ok(())
    }

    pub(crate) fn confirm_datum_association(&mut self, id: &str, expected_revision: u64) -> Result<(), String> {
        self.binding_mutation_guard(expected_revision)?;
        let index = self.datum_association_index(id)?;
        let association = &self.inspection.datum_associations[index];
        self.validate_datum_association_evidence(association)?;
        let view = dper_binding::unique_view(&self.inspection.views, &association.view_id)?;
        let catalog = self.current_catalog(view.page)?;
        let confirmed = datum_binding::confirm(
            self.inspection.plan.as_ref().ok_or("Aucun plan de contrôle actif.")?,
            &catalog.document_fingerprint, &catalog.geometry, view, association,
        )?;
        self.current_drawing_source()?;
        self.inspection.datum_associations[index] = confirmed;
        self.datum_association_changed();
        Ok(())
    }

    /// Withdrawal remains available when the original file or catalogue is gone.
    pub(crate) fn withdraw_datum_association(&mut self, id: &str, expected_revision: u64) -> Result<(), String> {
        self.binding_mutation_guard(expected_revision)?;
        // Duplicate entries can also be removed; they never grant confirmation.
        if !self.inspection.datum_associations.iter().any(|association| association.id == id) {
            return Err("Lien repère/support absent.".into());
        }
        self.inspection.datum_associations.retain(|association| association.id != id);
        self.datum_association_changed();
        Ok(())
    }

    fn datum_association_changed(&mut self) {
        self.inspection_panel_cache = None;
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
    }

    pub(crate) fn datum_association_panel(&mut self, ui: &mut egui::Ui) {
        if self.inspection.datum_associations.is_empty() { return; }
        let associations = self.inspection.datum_associations.clone();
        let revision = self.document_revision;
        let mutation_allowed = self.binding_mutation_guard(revision).is_ok();
        // Hash the actual PDF once for the whole panel, then recheck on action.
        let source = self.current_drawing_source().map(|(_, fingerprint)| fingerprint);
        let mut confirm = None;
        let mut withdraw = None;
        ui.collapsing("Repères et supports du plan", |ui| {
            ui.label("Vérifiez sur le PDF la déclaration du repère et le trait choisi avant de confirmer le lien documentaire.");
            for (index, association) in associations.iter().enumerate() {
                ui.push_id(("datum_association", index, &association.id), |ui| {
                    ui.group(|ui| {
                        ui.label(format!("Repère {} · {} · page {}",
                            association.label, association.feature_id, association.page));
                        ui.label(format!("Vue : {} · support : {}", association.view_id, association.support_id));
                        ui.label(format!("Justification : {}", association.justification));
                        let validity = source.as_ref().map_err(|error| error.clone()).and_then(|source|
                            self.validate_datum_association_evidence_for_source(association, source));
                        match &validity {
                            Ok(()) if association.reviewed => { ui.label("Lien documentaire confirmé dans cette session."); }
                            Ok(()) => { ui.label("Proposition enregistrée, à confirmer dans cette session."); }
                            Err(error) => { ui.colored_label(egui::Color32::YELLOW, format!("Lien suspendu : {error}")); }
                        }
                        ui.collapsing("Sources PDF et support retenu", |ui| {
                            if let Some(text) = association.evidence["declaration"]["source_text"].as_str() {
                                ui.label(format!("Déclaration : {text}"));
                            }
                            if let Some(text) = association.evidence["dependent_source"]["exact_text"].as_str() {
                                ui.label(format!("Cadre : {text}"));
                            }
                            ui.label(format!("Document : {}", association.document_fingerprint));
                            ui.monospace(serde_json::to_string_pretty(&association.evidence).unwrap_or_default());
                        });
                        ui.horizontal(|ui| {
                            if ui.add_enabled(mutation_allowed && validity.is_ok() && !association.reviewed,
                                egui::Button::new("Je confirme ce lien documentaire")).clicked() {
                                confirm = Some(association.id.clone());
                            }
                            if ui.add_enabled(mutation_allowed, egui::Button::new("Retirer le lien")).clicked() {
                                withdraw = Some(association.id.clone());
                            }
                        });
                        ui.weak("Référence physique non établie ; le contrôle géométrique et la conformité restent suspendus.");
                    });
                });
            }
        });
        if let Some(id) = confirm {
            self.message = match self.confirm_datum_association(&id, revision) {
                Ok(()) => "Lien documentaire repère/support confirmé ; contrôle géométrique toujours suspendu.".into(),
                Err(error) => error,
            };
        }
        if let Some(id) = withdraw {
            self.message = match self.withdraw_datum_association(&id, revision) {
                Ok(()) => "Lien documentaire repère/support retiré.".into(),
                Err(error) => error,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::OpenOptions, io::Write, path::PathBuf, sync::atomic::{AtomicU64, Ordering}};

    static NEXT_SOURCE: AtomicU64 = AtomicU64::new(0);

    struct TemporaryPdf(PathBuf);

    impl TemporaryPdf {
        fn create() -> Self {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("dper-datum-native-{}-{stamp}-{}.pdf",
                std::process::id(), NEXT_SOURCE.fetch_add(1, Ordering::Relaxed)));
            let mut file = OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
            // The adapter hashes a real source file. Text/vector extraction is
            // supplied synthetically below; these tests do not qualify a PDF parser.
            file.write_all(b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n%%EOF\n").unwrap();
            Self(path)
        }
    }

    impl Drop for TemporaryPdf {
        fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
    }

    fn fixture() -> (DperMeasureApp, InspectDrawingDatumSupportArgs, TemporaryPdf) {
        let source = TemporaryPdf::create();
        let fingerprint = crate::dper_source::fingerprint_file(&source.0).unwrap();
        let xml = r#"<html><body><doc><page width="400" height="320"><flow><block xMin="10" yMin="10" xMax="300" yMax="70">
<line xMin="10" yMin="10" xMax="290" yMax="20"><word xMin="10" yMin="10" xMax="25" yMax="20">⌖</word><word xMin="30" yMin="10" xMax="35" yMax="20">|</word><word xMin="40" yMin="10" xMax="70" yMax="20">Ø0,1</word><word xMin="75" yMin="10" xMax="95" yMax="20">mm</word><word xMin="100" yMin="10" xMax="105" yMax="20">|</word><word xMin="110" yMin="10" xMax="125" yMax="20">A</word></line>
<line xMin="10" yMin="40" xMax="80" yMax="50"><word xMin="10" yMin="40" xMax="55" yMax="50">DATUM</word><word xMin="60" yMin="40" xMax="75" yMax="50">A</word></line>
</block></flow></page></doc></body></html>"#;
        let mut plan = crate::dper_drawing::from_pdf_layout(
            crate::dper_pdf_layout::parse_bbox_layout(xml).unwrap(), "datum.pdf");
        plan.source_file = Some(source.0.to_string_lossy().into_owned());
        plan.source_fingerprint = Some(fingerprint.clone());
        let geometry = crate::dper_pdf_geometry::parse_svg_geometry(
            r#"<svg width="400pt" height="320pt" viewBox="0 0 400 320"><path stroke="black" fill="none" d="M 10 80 L 300 80"/></svg>"#, 1).unwrap();
        let view = dper_binding::define_view(&plan, &fingerprint, &geometry, "Vue datum",
            &[plan.characteristics[0].id.clone()], &[geometry.primitives[0].id.clone()]).unwrap();
        let mut app = DperMeasureApp::new_with_services(egui::Context::default(), false);
        let args = InspectDrawingDatumSupportArgs {
            expected_revision: app.document_revision,
            feature_id: plan.characteristics[0].id.clone(), view_id: view.id.clone(),
            label: "A".into(), support_id: geometry.primitives[0].id.clone(),
            justification: "Déclaration A et trait vérifiés sur cette vue du plan".into(),
        };
        app.inspection.plan = Some(plan);
        app.inspection.views.push(view);
        app.drawing_geometry_cache.push(crate::dper_pdf_geometry::DrawingGeometryCatalog {
            document_fingerprint: fingerprint, geometry,
        });
        app.measurements.push(serde_json::from_value(json!({
            "id": 77, "kind": "Distance", "points": [{"x": 1.0, "y": 2.0}, {"x": 8.0, "y": 9.0}],
            "label": "Mesure indépendante", "capture_id": app.capture_id,
            "calibration_id": "cal-native", "mm_per_pixel": 0.01,
        })).unwrap());
        app.calibration = Some(serde_json::from_value(json!({
            "id": "cal-native", "device": "test", "resolution": [400,320],
            "optical_setup": "test", "mm_per_pixel": 0.01,
            "reference_mm": 1.0, "reference_px": 100.0,
        })).unwrap());
        (app, args, source)
    }

    fn physical_state(app: &DperMeasureApp) -> Value {
        json!({"measurements": app.measurements, "calibration": app.calibration,
            "scale": app.mm_per_pixel, "calibration_history": app.calibration_history,
            "records": app.inspection.records})
    }

    #[test]
    fn dpm4_native_review_saves_confirms_replaces_and_withdraws_without_physical_authority() {
        let (mut app, mut args, _source) = fixture();
        let physical = physical_state(&app);
        let saved = app.save_datum_association(args.clone()).unwrap();
        assert_eq!(saved["requires_native_confirmation"], true);
        assert_eq!(saved["association"]["reviewed"], false);
        let id = app.inspection.datum_associations[0].id.clone();
        assert!(app.validate_datum_association(&app.inspection.datum_associations[0]).is_err());
        // A revision from before proposal storage cannot authorize native review.
        let revision = app.document_revision;
        assert!(app.confirm_datum_association(&id, args.expected_revision).is_err());
        assert_eq!(app.document_revision, revision);
        app.confirm_datum_association(&id, revision).unwrap();
        let selected = &app.inspection.datum_associations[0];
        assert!(selected.reviewed);
        assert!(app.validate_datum_association(selected).is_ok());
        assert_eq!(selected.evidence["reference_resolved"], false);
        assert_eq!(selected.evidence["measured_on_photo"], false);
        assert_eq!(crate::drawing_structure::suspension_reason(
            &app.inspection.plan.as_ref().unwrap().characteristics[0]), Some("drawing_relation_unresolved"));
        // Replacing a confirmed mapping through MCP always revokes native review.
        args.expected_revision = app.document_revision;
        app.save_datum_association(args).unwrap();
        assert_eq!(app.inspection.datum_associations.len(), 1);
        assert!(!app.inspection.datum_associations[0].reviewed);
        app.confirm_datum_association(&id, app.document_revision).unwrap();
        app.withdraw_datum_association(&id, app.document_revision).unwrap();
        assert!(app.inspection.datum_associations.is_empty());
        assert_eq!(physical_state(&app), physical);
    }

    #[test]
    fn dpm4_native_confirmation_rechecks_real_file_and_withdraws_missing_sources() {
        let (mut app, args, source) = fixture();
        let physical = physical_state(&app);
        app.save_datum_association(args).unwrap();
        let id = app.inspection.datum_associations[0].id.clone();
        app.confirm_datum_association(&id, app.document_revision).unwrap();
        let mut changed = std::fs::read(&source.0).unwrap();
        // Same path and length: actual content, rather than metadata, must win.
        changed[7] = b'7';
        std::fs::write(&source.0, changed).unwrap();
        assert!(app.validate_datum_association(&app.inspection.datum_associations[0]).is_err());
        let revision = app.document_revision;
        assert!(app.confirm_datum_association(&id, revision).is_err());
        assert_eq!(app.document_revision, revision);
        std::fs::remove_file(&source.0).unwrap();
        app.withdraw_datum_association(&id, revision).unwrap();
        assert!(app.inspection.datum_associations.is_empty());
        assert_eq!(physical_state(&app), physical);
    }

    #[test]
    fn dpm4_import_requires_native_review_and_current_session_catalog() {
        let (mut app, args, _source) = fixture();
        app.save_datum_association(args).unwrap();
        let id = app.inspection.datum_associations[0].id.clone();
        app.confirm_datum_association(&id, app.document_revision).unwrap();
        let physical = physical_state(&app);
        let saved = serde_json::to_value(&app.inspection).unwrap();
        assert_eq!(saved["datum_associations"][0]["reviewed"], true);
        app.inspection = serde_json::from_value(saved).unwrap();
        assert!(!app.inspection.datum_associations[0].reviewed);
        assert!(app.validate_datum_association(&app.inspection.datum_associations[0]).is_err());
        let catalog = std::mem::take(&mut app.drawing_geometry_cache);
        assert!(app.confirm_datum_association(&id, app.document_revision).is_err());
        // Simulate a fresh extraction in the reopened session, followed by the
        // actual native confirmation path; deserialization alone grants nothing.
        app.drawing_geometry_cache = catalog;
        app.confirm_datum_association(&id, app.document_revision).unwrap();
        assert!(app.validate_datum_association(&app.inspection.datum_associations[0]).is_ok());
        assert_eq!(physical_state(&app), physical);
    }
}

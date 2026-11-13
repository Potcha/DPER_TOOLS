use super::*;

// Shared real-widget workflow; baseline refusal assertions remain in the original test.
pub(super) fn composed_native_observation(fixture: &mut Fixture, mut checkpoint: impl FnMut(&mut Fixture, &str)) -> serde_json::Value {
    let original_nominal = fixture.app.inspection.plan.as_ref().unwrap().characteristics[0].nominal;
    fixture.define_view_by_pointer();
    checkpoint(fixture, "explicit_view");
    fixture.prepare_binding_by_pointer();
    let candidate = fixture.app.inspection_plan_ui.binding.as_ref().unwrap().candidates[1].clone();
    fixture.click(&format!("{} · rang {} · distance {:.2} pt", candidate.geometry_id, candidate.rank, candidate.distance_pt));
    fixture.type_into_empty("Décrire le rappel de cote et le cercle choisi", "Rappel de cote explicitement rattaché au cercle droit de la vue de dessus.");
    fixture.click("Enregistrer l'association choisie");
    assert_eq!(fixture.app.inspection.bindings.len(), 1);
    assert!(!fixture.app.inspection.bindings[0].reviewed);
    checkpoint(fixture, "selected_unreviewed_binding");

    let source_circle = fixture.app.inspection.bindings[0].snapshot.circle.as_ref().unwrap();
    let center = source_circle.center;
    let proposed_radius = source_circle.radius_pt;
    // Actual pixels deliberately describe a different radius than the drawing
    // proposal. Independent calibration deliberately differs from the nominal.
    let observed_radius = proposed_radius + 2.5;
    let image = image::RgbImage::from_fn(240, 180, |x, y| {
        let distance = (f64::from(x) - center.x).hypot(f64::from(y) - center.y);
        image::Rgb([(20.0 + 200.0 * (distance - (observed_radius - 0.5)).clamp(0.0, 1.0)) as u8; 3])
    });
    fixture.app.image = Some(crate::ImageData {
        width: 240, height: 180, rgb: image.as_raw().clone(), source: "independent-synthetic-disk.png".into(),
    });
    let mut quality = crate::quality::assess_image_quality(
        image.as_raw(), 240, 180, &crate::quality::ImageQualityThresholds::default()).unwrap();
    // This acceptance fixture tests the complete UI flow, not qualification of
    // physical lighting/optics or the real image-quality thresholds.
    quality.is_exploitable = true;
    fixture.app.quality_assessment = Some(quality);
    fixture.app.calibration = Some(serde_json::from_value(serde_json::json!({
        "id": "independent-native-acceptance-reference", "device": "independent reference",
        "resolution": [240,180], "optical_setup": "synthetic acceptance",
        "mm_per_pixel": 0.1, "reference_mm": 10.0, "reference_px": 100.0,
        "expanded_uncertainty_mm": 0.002,
    })).unwrap());
    fixture.app.mm_per_pixel = Some(0.1);
    let independent_calibration = serde_json::to_value(&fixture.app.calibration).unwrap();

    // Reviews are separate, real operator checkbox actions after selection.
    fixture.show_panel(TestPanel::Control);
    let feature = fixture.app.inspection.plan.as_ref().unwrap().characteristics[0].clone();
    fixture.click(&format!("Association · {}", feature.id));
    fixture.click("Association cote / géométrie vérifiée sur le plan");
    fixture.click(&format!("{} · {}", feature.id, feature.label));
    fixture.click("Interprétation du plan vérifiée");
    assert!(fixture.app.inspection.bindings[0].reviewed);
    assert!(fixture.app.inspection.plan.as_ref().unwrap().characteristics[0].confirmed);

    checkpoint(fixture, "human_reviews");
    fixture.show_panel(TestPanel::Registration);
    fixture.click("Nouveau brouillon (effacer la saisie)");
    fixture.click("Sans vue — repère plan explicite");
    fixture.click("Vue de dessus · page 1");
    let bounds = fixture.app.inspection.views[0].bounds;
    let rows = [[50.0, 30.0, 50.0, 30.0], [190.0, 30.0, 190.0, 30.0], [50.0, 100.0, 50.0, 100.0]];
    for (row_index, row) in rows.iter().enumerate() {
        assert!(row[0] >= bounds.x_min && row[0] <= bounds.x_max && row[1] >= bounds.y_min && row[1] <= bounds.y_max);
        for (column, heading) in ["Plan X (pt)", "Plan Y (pt)", "Image X (px)", "Image Y (px)"].iter().enumerate() {
            let heading_rect = fixture.text_rect(heading);
            let plan_x = fixture.text_rect("Plan X (pt)");
            let ordinal = fixture.text_rects(&(row_index + 1).to_string()).into_iter()
                .find(|rect| rect.left() < plan_x.left())
                .expect("Rendered landmark row ordinal missing");
            fixture.replace_at(egui::pos2(heading_rect.left() + 8.0, ordinal.center().y), &row[column].to_string());
        }
    }
    fixture.click("Calculer et appliquer la similitude");
    let registration = fixture.app.inspection.registration.as_ref().expect("Native registration UI must install the shared similarity");
    assert!((registration.transform.scale - 1.0).abs() < 1e-10);
    assert_eq!(registration.inlier_indices.len(), 3);
    assert_eq!(fixture.app.inspection.registration_scope.as_ref().unwrap().view_id, fixture.app.inspection.views[0].id);
    assert_eq!(serde_json::to_value(&fixture.app.calibration).unwrap(), independent_calibration);

    checkpoint(fixture, "similarity_registration");
    fixture.show_panel(TestPanel::Observation);
    fixture.click("Choisir une caractéristique");
    fixture.click(&format!("{} · {}", feature.id, feature.label));
    let group_field = fixture.text_rect("Groupe 1").center();
    fixture.replace_at(group_field, "Pièce témoin intégrée");
    fixture.click("Préparer une nouvelle observation");
    // The bound path must resolve the selected drawing source, not substitute a
    // hand-entered search circle or the nominal characteristic value.
    fixture.text_rect("Recherche issue de l'association PDF. Le moteur résout la géométrie liée ; aucune substitution manuelle.");
    fixture.click("Enregistrer explicitement l'observation");
    assert!(fixture.app.measurements.is_empty(), "Recording requires an explicit preview first");
    fixture.click("Prévisualiser les contours (sans mesure)");
    assert!(fixture.app.measurements.is_empty());
    let preview = fixture.app.inspection.records.last().expect("Pixel contour preview must leave audit evidence");
    assert_eq!(preview.group, "Pièce témoin intégrée");
    assert_eq!(preview.drawing_binding_id.as_deref(), Some(candidate.id.as_str()));
    assert!(preview.result.evidence.accepted_points.len() > 60);
    match preview.result.geometry {
        crate::dper_vision::InspectionGeometry::Circle { radius, .. } => {
            assert!((radius - observed_radius).abs() < 0.5);
            assert!((radius - proposed_radius).abs() > 1.5);
        }
        _ => panic!("The selected circular binding must produce an observed circle"),
    }
    assert!((preview.value_mm.unwrap() - observed_radius * 0.2).abs() < 0.1);
    assert_eq!(fixture.app.inspection.plan.as_ref().unwrap().characteristics[0].nominal, original_nominal);
    checkpoint(fixture, "grouped_contour_preview");
    fixture.click("Enregistrer explicitement l'observation");
    assert_eq!(fixture.app.measurements.len(), 1);
    checkpoint(fixture, "native_measurement_recorded");
    independent_calibration
}

fn parity_checkpoint(fixture: &mut Fixture, stage: &str) {
    let panel = fixture.panel;
    let revision = fixture.app.document_revision;
    let saved = fixture.app.session_saved;
    let evidence = serde_json::to_value(&fixture.app.inspection).unwrap();
    fixture.show_panel(TestPanel::Control);
    let current = fixture.app.current_inspection();
    let mcp = fixture.app.handle_inspection_request(crate::mcp_server::InspectionRequest::GetState).unwrap();
    assert_eq!(mcp["characteristic_progress"], serde_json::to_value(crate::inspection_progress::summarize(&current)).unwrap(), "progress UI/MCP mismatch at {stage}");
    let progress = mcp["characteristic_progress"].as_array().unwrap();
    assert_eq!(progress.len(), current.plan.as_ref().unwrap().characteristics.len());
    if stage == "grouped_contour_preview" {
        assert_eq!(progress[0]["current_observations"], 0, "a preview is not a registered observation");
    }
    assert_eq!(mcp["inspection"]["records"], serde_json::to_value(&current.records).unwrap(), "{stage}");
    for (native, remote) in current.records.iter().zip(mcp["inspection"]["records"].as_array().unwrap()) {
        let rendered = format!("Conformité physique : {}", native.outcome.conformity_status.label());
        assert!(!fixture.text_rects(&rendered).is_empty(), "Missing native renderer outcome at {stage}: {rendered}");
        let expected: std::collections::BTreeSet<_> = native.outcome.blocking_reasons.iter().map(String::as_str).collect();
        let actual: std::collections::BTreeSet<_> = remote["outcome"]["blocking_reasons"].as_array().unwrap()
            .iter().map(|reason| reason.as_str().unwrap()).collect();
        assert_eq!(actual, expected, "{stage}");
    }
    fixture.show_panel(panel);
    assert_eq!(fixture.app.document_revision, revision, "read-only stage {stage}");
    assert_eq!(fixture.app.session_saved, saved, "read-only stage {stage}");
    assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), evidence, "read-only stage {stage}");
}

fn requirement_fixture(text: &str) -> Fixture {
    let mut fixture = Fixture::new();
    // This supplied text interpretation is a synthetic test prerequisite. The
    // fixture exercises native association/review, not PDF/OCR extraction or a
    // physical qualification of the source artwork.
    let parsed = crate::dper_drawing::parse_text_plan(text, "synthetic supplied control requirement");
    let feature = &mut fixture.app.inspection.plan.as_mut().unwrap().characteristics[0];
    feature.nominal = parsed.characteristics[0].nominal;
    feature.unit = parsed.characteristics[0].unit;
    feature.tolerance = parsed.characteristics[0].tolerance.clone();
    feature.label = format!("Exigence synthétique fournie : {text}");
    fixture.app.inspection.plan.as_mut().unwrap().warnings.push(
        "DPM28 : interprétation textuelle et propositions SVG fournies au test ; l'extraction PDF/OCR et la lecture du rappel de cote de l'artwork ne sont pas évaluées. Aucune qualification physique.".into());
    fixture.settle();
    fixture
}

fn decision_by_pointer(fixture: &mut Fixture, uncertainty: &str) {
    fixture.show_panel(TestPanel::Decision);
    let record = fixture.app.inspection.records.last().unwrap();
    let label = format!("M{} · {} · {} · capture {}", record.measurement_id.unwrap(),
        record.feature_id, record.group, record.capture_id);
    fixture.click("Choisir une observation enregistrée");
    fixture.click(&label);
    fixture.click("Préparer la revue de cette observation");
    for (label, value) in [
        ("Référence du document de qualification", "SYNTH-QUAL-DPM28-R1"),
        ("Domaine qualifié pour cette caractéristique", "Déclaration synthétique de test : diamètre dans un plan, montage indépendant"),
        ("Incertitude élargie TOTALE U (mm)", uncertainty),
        ("Source documentaire de U totale (méthode et portée)", "SYNTH-U-DPM28-R1 : U totale du diamètre, k=2, fournie sans calcul logiciel"),
        ("Responsable de la revue", "Opérateur synthétique DPM28"),
    ] {
        let label = fixture.text_rect(label);
        // TextEdit is the next layout row after its visible label. Use real
        // pointer/keyboard events rather than editing the decision draft.
        fixture.replace_at(egui::pos2(label.left() + 18.0, label.bottom() + 12.0), value);
    }
    fixture.click("J'atteste avoir revu la qualification indépendante et U totale ; appliquer cette base");
}

fn independently_replay_bound_observation(fixture: &Fixture) {
    let ctx = egui::Context::default();
    let mut replay = DperMeasureApp::new_with_services(ctx, false);
    let image = fixture.app.image.as_ref().unwrap();
    replay.image = Some(crate::ImageData { width: image.width, height: image.height,
        rgb: image.rgb.clone(), source: image.source.clone() });
    replay.capture_id = fixture.app.capture_id.clone();
    replay.calibration = fixture.app.calibration.clone();
    replay.mm_per_pixel = fixture.app.mm_per_pixel;
    replay.quality_assessment = fixture.app.quality_assessment.clone();
    // Copy established native operator reviews as explicit prerequisites. This
    // is not an MCP grant: InspectBound has no human-review authority.
    replay.inspection = fixture.app.inspection.clone();
    replay.inspection.records.clear();
    replay.inspection.dependencies = Default::default();
    replay.measurements.clear();
    replay.drawing_geometry_cache = fixture.app.drawing_geometry_cache.clone();
    replay.drawing_geometry_generation = fixture.app.drawing_geometry_generation;
    assert!(replay.inspection.bindings[0].reviewed);
    assert!(replay.inspection.plan.as_ref().unwrap().characteristics[0].confirmed);
    let native_records = &fixture.app.inspection.records;
    for (commit, native) in [(false, &native_records[native_records.len() - 2]), (true, native_records.last().unwrap())] {
        let result = replay.handle_inspection_request(crate::mcp_server::InspectionRequest::Drawing(
            DrawingRequest::InspectBound(crate::mcp_server::InspectBoundFeatureArgs {
                source_measurement_ids: Vec::new(), capture_id: replay.capture_id.clone(),
                expected_revision: replay.document_revision, feature_id: native.feature_id.clone(),
                occurrence_id: None,
                group: native.group.clone(), options: native.vision_options.clone(), commit,
            }))).unwrap();
        let replayed = &result["record"];
        let native = serde_json::to_value(native).unwrap();
        for key in ["feature_id", "drawing_binding_id", "group", "capture_id", "source_fingerprint",
            "calibration_id", "calibration_fingerprint", "result", "vision_options", "value_mm",
            "measurement_points", "outcome"] {
            assert_eq!(replayed[key], native[key], "independently executed MCP operation, commit={commit}, field={key}");
        }
        assert_eq!(replayed["measurement_id"].is_null(), !commit);
        if commit {
            let id = replayed["measurement_id"].as_u64().unwrap();
            assert!(replay.measurements.iter().any(|measurement| measurement.id == id));
            assert_eq!(replayed["measurement_revision"].as_u64(), Some(replay.inspection.dependencies.revision(id)));
        }
    }
    assert_eq!(replay.measurements.len(), 1);
    assert_eq!(replay.inspection.records.len(), 2);
    assert!((replay.inspection.records[1].value_mm.unwrap() - 4.5).abs() < 0.1,
        "Analytic synthetic truth: 22.5 px radius × 2 × independent 0.1 mm/px");
}

struct AcceptanceFiles(std::path::PathBuf, bool);
impl AcceptanceFiles {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let index = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let retained = std::env::var_os("DPER_TEST_ACCEPTANCE_OUTPUT");
        let root = retained.as_ref().map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!("dper-dpm28-acceptance-{}-{index}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path, retained.is_some())
    }
}
impl Drop for AcceptanceFiles {
    fn drop(&mut self) { if !self.1 { let _ = std::fs::remove_dir_all(&self.0); } }
}

fn export_project_reports(app: &mut DperMeasureApp, ctx: &egui::Context,
    folder: &std::path::Path, prefix: &str, expected_current: &str) {
    let preparation = app.prepare_mcp_report(ctx, Some(folder)).unwrap();
    assert_eq!(preparation.selected_count, 1);
    let selected: Vec<_> = preparation.items.iter().filter(|item| item.selected).map(|item| item.key.clone()).collect();
    for format in ["pdf", "pptx"] {
        let destination = folder.join(format!("{prefix}.{format}"));
        let args = serde_json::from_value(serde_json::json!({
            "capture_id": app.capture_id, "expected_revision": app.document_revision,
            "format": format, "destination": destination, "folder": folder,
            "selected_image_keys": selected, "title": "DPM28 — acceptation synthétique",
            "include_measurements": true, "include_audit_page": true,
            "operator_name": "Opérateur synthétique DPM28", "persist_preferences": false,
            "audit_notes": "DPM28 : scénario logiciel synthétique. Interprétation textuelle et propositions SVG fournies, sans évaluer l'extraction PDF/OCR ni la lecture du rappel de cote de l'artwork. U et qualification sont des déclarations fictives de test ; aucune qualification physique.",
            "overwrite": false,
        })).unwrap();
        let result = app.export_mcp_report(ctx, args).unwrap();
        assert_eq!(result.image_count, 1);
        assert!(result.manifest_written);
        let report = std::fs::read(&result.report_path).unwrap();
        assert!(report.len() > 1000);
        assert!(report.starts_with(if format == "pdf" { b"%PDF" } else { b"PK\x03\x04" }));
        let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&result.manifest_path).unwrap()).unwrap();
        let audit = manifest["audit"]["audit_notes"].as_str().unwrap();
        assert!(audit.contains(expected_current), "{prefix}/{format}: {audit}");
        assert!(audit.contains("SYNTH-QUAL-DPM28-R1"));
        assert!(audit.contains("SYNTH-U-DPM28-R1"));
        assert_eq!(manifest["pictures"].as_array().unwrap().len(), 1);
        assert_eq!(manifest["pictures"][0]["session"]["capture_id"], app.capture_id);
    }
}

#[test]
fn composed_native_workflow_decision_independent_mcp_replay_save_reopen_and_exports() {
    let files = AcceptanceFiles::new();
    let mut fixture = requirement_fixture("Ø4.5 ±0.5 mm");
    let project_source = files.0.join("synthetic-source-artwork.pdf");
    std::fs::copy(&fixture.path, &project_source).unwrap();
    fixture.app.inspection.plan.as_mut().unwrap().source_file = Some(project_source.display().to_string());
    let mut stages = Vec::new();
    let mut checkpoint_states = Vec::new();
    let calibration = composed_native_observation(&mut fixture, |fixture, stage| {
        parity_checkpoint(fixture, stage); stages.push(stage.to_owned());
        checkpoint_states.push(serde_json::json!({"stage": stage, "revision": fixture.app.document_revision,
            "records": fixture.app.current_inspection().records, "renderer_get_state_parity": true,
            "read_only": true}));
    });
    assert_eq!(stages, ["explicit_view", "selected_unreviewed_binding", "human_reviews",
        "similarity_registration", "grouped_contour_preview", "native_measurement_recorded"]);
    independently_replay_bound_observation(&fixture);
    let observed = fixture.app.current_inspection().records.last().unwrap().clone();
    assert_eq!(observed.outcome.geometry_state, crate::dper_metrology::GeometryState::Validated);
    assert_eq!(observed.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Indeterminate);
    assert!((observed.value_mm.unwrap() - 4.5).abs() < 0.1);
    let generation = fixture.app.inspection.dependencies.revision(observed.measurement_id.unwrap());
    decision_by_pointer(&mut fixture, "0.1");
    parity_checkpoint(&mut fixture, "native_explicit_documentary_attestation");
    let reviewed = fixture.app.current_inspection().records.last().unwrap().clone();
    checkpoint_states.push(serde_json::json!({"stage": "native_explicit_documentary_attestation",
        "revision": fixture.app.document_revision, "records": fixture.app.current_inspection().records,
        "renderer_get_state_parity": true, "read_only": true}));
    assert_eq!(reviewed.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Conforming);
    assert_eq!(reviewed.decision_evidence.as_ref().unwrap().basis.expanded_uncertainty_mm, 0.1);
    assert_eq!(fixture.app.inspection.dependencies.revision(observed.measurement_id.unwrap()), generation);
    assert_eq!(serde_json::to_value(&fixture.app.calibration).unwrap(), calibration);
    let project = files.0.join("synthetic-acceptance.dperproj");
    fixture.app.save_project_to_path(&project, false).unwrap();
    parity_checkpoint(&mut fixture, "project_saved_current_session");
    export_project_reports(&mut fixture.app, &fixture.ctx, &files.0, "current", "conformité conforme");
    let session: crate::storage::Session = serde_json::from_slice(&std::fs::read(&project).unwrap()).unwrap();
    let csv = crate::exports::measurements_csv(&session).unwrap();
    let csv_text = String::from_utf8(csv.clone()).unwrap();
    assert!(csv_text.contains("SYNTH-QUAL-DPM28-R1"));
    assert!(csv_text.contains("historique non réévalué ; conformité suspendue"));
    std::fs::write(files.0.join("saved.csv"), csv).unwrap();
    let xlsx = crate::exports::measurements_xlsx(&session).unwrap();
    assert!(xlsx.starts_with(b"PK\x03\x04"));
    std::fs::write(files.0.join("saved.xlsx"), xlsx).unwrap();
    let fresh_ctx = egui::Context::default();
    let mut reopened = DperMeasureApp::new_with_services(fresh_ctx.clone(), false);
    reopened.open_project_path(&fresh_ctx, &project).unwrap();
    assert_eq!(reopened.inspection.views[0].source_proof,
        fixture.app.inspection.views[0].source_proof,
        "The project preserves source provenance without granting current-session authority");
    assert!(reopened.drawing_geometry_cache.is_empty());
    assert!(reopened.inspection.current_drawing_catalogs.is_empty());
    assert_eq!(reopened.measurements.len(), 1);
    assert_eq!(reopened.capture_id, fixture.app.capture_id);
    assert_eq!(reopened.measurements[0].kind, fixture.app.measurements[0].kind);
    assert_eq!(reopened.measurements[0].points.len(), fixture.app.measurements[0].points.len());
    // Decimal JSON parsing can differ by one floating-point ULP. This numerical
    // tolerance is far below pixel resolution; proof/review authority still resets.
    for (restored, original) in reopened.measurements[0].points.iter().zip(&fixture.app.measurements[0].points) {
        assert!((restored.x - original.x).abs() <= 1e-12);
        assert!((restored.y - original.y).abs() <= 1e-12);
    }
    assert!(reopened.inspection.records.iter().all(|record| record.imported));
    assert!(!reopened.inspection.bindings[0].reviewed);
    // Interpretation confirmation is retained as historical plan content;
    // current-session binding and decision reviews are independently revoked.
    assert!(reopened.inspection.plan.as_ref().unwrap().characteristics[0].confirmed);
    let historical = reopened.inspection.records.last().unwrap().decision_evidence.as_ref().unwrap();
    assert!(!historical.reviewed);
    assert_eq!(historical.basis.qualification_reference, "SYNTH-QUAL-DPM28-R1");
    let state = reopened.current_inspection();
    for record in &state.records {
        assert!(!record.outcome.blocking_reasons.iter().any(|reason|
            matches!(reason.as_str(), "inspection_points_changed" | "physical_value_changed")),
            "A decimal JSON roundtrip must not invent a geometry or value edit");
    }
    for record in &state.records {
        assert!(record.imported);
        assert_ne!(record.outcome.geometry_state, crate::dper_metrology::GeometryState::Validated,
            "Imported pixels never retain software-validation authority");
        assert_ne!(record.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Conforming);
        for reason in ["imported_evidence_requires_reassessment", "drawing_binding_unreviewed"] {
            assert!(record.outcome.blocking_reasons.iter().any(|actual| actual == reason),
                "Missing reload suspension reason {reason}");
        }
    }
    assert!(state.records.last().unwrap().outcome.blocking_reasons.iter()
        .any(|reason| reason == "decision_evidence_unreviewed"));
    let revision = reopened.document_revision;
    let before = serde_json::to_value(&reopened.inspection).unwrap();
    let mut rendered_labels = Vec::new();
    for frame in 0..3 {
        let output = fresh_ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1800.0, 3200.0))),
            time: Some(frame as f64 / 60.0), focused: true, ..Default::default()
        }, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| reopened.automated_inspection_panel(ui));
        });
        fn collect_labels(shape: &egui::epaint::Shape, labels: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => labels.push(text.galley.text().to_owned()),
                egui::epaint::Shape::Vec(shapes) => for shape in shapes { collect_labels(shape, labels); },
                _ => {}
            }
        }
        rendered_labels.clear();
        for clipped in &output.shapes { collect_labels(&clipped.shape, &mut rendered_labels); }
    }
    let feature_id = &state.records.last().unwrap().feature_id;
    assert!(rendered_labels.contains(&format!("{feature_id} · Mesurée · à vérifier")));
    assert!(!rendered_labels.contains(&format!("{feature_id} · Vérifiée par logiciel")));
    for required in ["Valeur suspendue · nouvelle observation requise",
        "Suspendu : preuves historiques à réévaluer sur l'image"] {
        assert!(rendered_labels.iter().any(|label| label == required), "Missing reload renderer warning: {required}");
    }
    for record in &state.records {
        assert!(rendered_labels.contains(&format!("Conformité physique : {}", record.outcome.conformity_status.label())));
    }
    assert_eq!(reopened.document_revision, revision, "Reopened renderer must be read-only");
    assert_eq!(serde_json::to_value(&reopened.inspection).unwrap(), before, "Reopened renderer must preserve historical evidence");
    let mcp = reopened.handle_inspection_request(crate::mcp_server::InspectionRequest::GetState).unwrap();
    assert_eq!(mcp["drawing_view_status"][0]["current"], false,
        "A restored view requires a fresh session catalog before use");
    assert_eq!(mcp["inspection"]["records"], serde_json::to_value(&state.records).unwrap());
    assert_eq!(reopened.document_revision, revision);
    assert_eq!(serde_json::to_value(&reopened.inspection).unwrap(), before);
    export_project_reports(&mut reopened, &fresh_ctx, &files.0, "reopened", "conformité indéterminée");
    checkpoint_states.push(serde_json::json!({"stage": "fresh_application_project_reopened",
        "revision": reopened.document_revision, "records": state.records,
        "renderer_get_state_parity": true, "read_only": true,
        "binding_review_reset": true, "decision_review_reset": true, "pixel_evidence_imported": true,
        "imported_geometry_not_validated": true, "imported_review_reasons_asserted": true,
        "actual_reopened_renderer_labels": rendered_labels}));
    let required = ["synthetic-acceptance.dperproj", "synthetic-source-artwork.pdf", "saved.csv", "saved.xlsx",
        "current.pdf", "current.pdf.dper_measure.json", "current.pptx", "current.pptx.dper_measure.json",
        "reopened.pdf", "reopened.pdf.dper_measure.json", "reopened.pptx", "reopened.pptx.dper_measure.json"];
    for name in required { assert!(std::fs::metadata(files.0.join(name)).unwrap().len() > 0, "{name}"); }
    assert!(files.0.join(&session.image_file).is_file());
    let summary = serde_json::json!({
        "scenario": "DPM28 composed synthetic native workflow acceptance", "completed": true,
        "boundary": "Supplied text interpretation and SVG proposals; this flow does not evaluate PDF/OCR extraction, artwork annotation reading, or physical metrology qualification. The independent MCP replay starts with native-reviewed prerequisites and does not grant human review through MCP.",
        "analytic_truth": {"image_circle_radius_px": 22.5, "independent_mm_per_pixel": 0.1,
            "diameter_mm": 4.5, "assertion_tolerance_mm": 0.1,
            "fixed_tolerance_limits_mm": [4.0, 5.0], "supplied_total_expanded_uncertainty_mm": 0.1},
        "independent_mcp_preview_and_commit_geometry_evidence_value_outcome_parity": true,
        "checkpoint_states": checkpoint_states,
        "current_decision": reviewed.outcome,
        "reopened_decision": reopened.current_inspection().records.last().unwrap().outcome,
        "artifacts": required,
        "source_image": session.image_file,
        "project": "synthetic-acceptance.dperproj",
        "source_pdf": "synthetic-source-artwork.pdf",
        "static_reports": "Historical state is distinguished from the live current audit; no saved report grants current-session conformity.",
    });
    let mut summary_file = std::fs::OpenOptions::new().write(true).create_new(true)
        .open(files.0.join("acceptance_summary.json")).unwrap();
    std::io::Write::write_all(&mut summary_file, &serde_json::to_vec_pretty(&summary).unwrap()).unwrap();
}

#[test]
fn composed_native_decision_fixed_limit_matrix_and_explicit_withdrawal() {
    use crate::dper_metrology::{ConformityStatus, GeometryState};
    for (requirement, uncertainty, expected) in [
        ("Ø4.5 ±0.5 mm", "0.1", ConformityStatus::Conforming),
        ("Ø5.75 ±0.25 mm", "0.1", ConformityStatus::Nonconforming),
        ("Ø4.6 ±0.2 mm", "0.25", ConformityStatus::Indeterminate),
        ("Ø4.5 mm", "0.1", ConformityStatus::NotApplicable),
    ] {
        let mut fixture = requirement_fixture(requirement);
        composed_native_observation(&mut fixture, |_, _| {});
        let measurement = serde_json::to_value(&fixture.app.measurements).unwrap();
        decision_by_pointer(&mut fixture, uncertainty);
        parity_checkpoint(&mut fixture, requirement);
        let record = fixture.app.current_inspection().records.last().unwrap().clone();
        assert_eq!(record.outcome.geometry_state, GeometryState::Validated);
        assert_eq!(record.outcome.conformity_status, expected, "fixed synthetic requirement {requirement}");
        if expected == ConformityStatus::NotApplicable {
            assert!(record.decision_evidence.is_none(), "Missing tolerance must refuse documentary attestation");
            continue;
        }
        assert!(record.decision_evidence.as_ref().unwrap().reviewed);
        let revision = fixture.app.document_revision;
        fixture.click("Retirer la base de décision");
        parity_checkpoint(&mut fixture, "explicit_documentary_withdrawal");
        let record = fixture.app.current_inspection().records.last().unwrap().clone();
        assert!(record.decision_evidence.is_none());
        assert_eq!(record.outcome.geometry_state, GeometryState::Validated);
        assert_eq!(record.outcome.conformity_status, ConformityStatus::Indeterminate);
        assert_eq!(fixture.app.document_revision, revision + 1);
        assert_eq!(serde_json::to_value(&fixture.app.measurements).unwrap(), measurement);
    }
}

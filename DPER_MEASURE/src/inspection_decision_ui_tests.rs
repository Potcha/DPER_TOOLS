use super::*;
use crate::geometry::Point;

static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[test]
fn unscoped_pdf_change_invalidates_descendants_and_csv_once_without_touching_manual_or_imported() {
    for imported in [false, true] {
        let mut fixture = Fixture::new();
        fixture.app.inspection.records[0].drawing_binding_id = None;
        fixture.app.inspection.bindings.clear();
        fixture.app.inspection.registration_scope = None;
        let root_id = fixture.app.inspection.records[0].measurement_id.unwrap();
        let parent = fixture.app.measurements.iter().find(|item| item.id == root_id).unwrap().clone();
        let mut child = parent.clone();
        child.id = root_id + 1;
        child.label = "DPM40_DERIVED_VALUE".into();
        child.origin = "derived".into();
        child.source_measurement_ids = vec![root_id];
        let child_id = child.id;
        let mut manual = parent;
        manual.id = root_id + 2;
        manual.label = "DPM40_MANUAL_VALUE".into();
        manual.origin = "manual".into();
        manual.source_measurement_ids.clear();
        let manual_id = manual.id;
        let manual_before = serde_json::to_value(&manual).unwrap();
        fixture.app.measurements.extend([child, manual]);
        fixture.app.inspection.dependencies.mark_created(child_id);
        fixture.app.inspection.dependencies.mark_created(manual_id);
        if imported {
            fixture.app.inspection = serde_json::from_value(serde_json::to_value(&fixture.app.inspection).unwrap()).unwrap();
            fixture.app.inspection.dependencies = crate::measurement_graph::DependencyGraph::restore_import(
                &fixture.app.measurements).unwrap();
            fixture.app.refresh_dependency_flags();
        } else {
            assert_eq!(fixture.app.current_inspection().records[0].outcome.geometry_state,
                crate::dper_metrology::GeometryState::Validated);
            let csv = String::from_utf8(crate::exports::measurements_csv(
                &fixture.app.current_export_image().unwrap().session()).unwrap()).unwrap();
            assert!(csv.contains("DPM40_DERIVED_VALUE"));
        }
        fixture.app.session_saved = true;
        let before = serde_json::to_value(&fixture.app.inspection).unwrap();
        let measurements_before = serde_json::to_value(&fixture.app.measurements).unwrap();
        let root_generation = fixture.app.inspection.dependencies.revision(root_id);
        let child_generation = fixture.app.inspection.dependencies.revision(child_id);
        let manual_generation = fixture.app.inspection.dependencies.revision(manual_id);
        let revision = fixture.app.document_revision;
        fixture.app.invalidate_changed_drawing_dependencies();
        assert_eq!(fixture.app.document_revision, revision);
        std::fs::write(&fixture.path, b"changed PDF source for DPM-40").unwrap();
        assert!(fixture.app.current_drawing_source().is_err());
        // Export must protect its own snapshot before any panel or MCP refresh.
        for _ in 0..2 {
            let snapshot = fixture.app.current_export_image().unwrap();
            let session = snapshot.session();
            let csv = String::from_utf8(crate::exports::measurements_csv(&session).unwrap()).unwrap();
            assert!(!csv.contains("DPM40_DERIVED_VALUE"), "Direct export excludes stale descendants");
            assert!(csv.contains("DPM40_MANUAL_VALUE"));
            if !imported {
                let workspace = session.automated_inspection.as_ref().unwrap();
                assert_eq!(workspace.dependencies.revision(root_id), root_generation + 1);
                assert_eq!(workspace.dependencies.revision(child_id), child_generation + 1);
            }
            assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), before);
            assert_eq!(serde_json::to_value(&fixture.app.measurements).unwrap(), measurements_before);
            assert_eq!(fixture.app.document_revision, revision);
            assert!(fixture.app.session_saved);
        }
        fixture.app.invalidate_changed_drawing_dependencies();
        if imported {
            assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), before);
            assert_eq!(serde_json::to_value(&fixture.app.measurements).unwrap(), measurements_before);
            assert_eq!(fixture.app.document_revision, revision);
            assert!(fixture.app.session_saved);
        } else {
            assert_eq!(fixture.app.inspection.dependencies.revision(root_id), root_generation + 1);
            assert_eq!(fixture.app.inspection.dependencies.revision(child_id), child_generation + 1);
            assert!(fixture.app.measurements.iter().find(|item| item.id == child_id).unwrap().dependency_stale);
            assert_eq!(fixture.app.document_revision, revision + 1);
            assert!(!fixture.app.session_saved);
            let csv = String::from_utf8(crate::exports::measurements_csv(
                &fixture.app.current_export_image().unwrap().session()).unwrap()).unwrap();
            assert!(!csv.contains("DPM40_DERIVED_VALUE"));
            assert!(csv.contains("DPM40_MANUAL_VALUE"));
        }
        assert_eq!(fixture.app.inspection.dependencies.revision(manual_id), manual_generation);
        assert_eq!(serde_json::to_value(fixture.app.measurements.iter().find(|item| item.id == manual_id).unwrap()).unwrap(), manual_before);
        let after = serde_json::to_value(&fixture.app.inspection).unwrap();
        let measurements_after = serde_json::to_value(&fixture.app.measurements).unwrap();
        let revision_after = fixture.app.document_revision;
        let saved_after = fixture.app.session_saved;
        for _ in 0..3 { fixture.app.invalidate_changed_drawing_dependencies(); }
        assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), after);
        assert_eq!(serde_json::to_value(&fixture.app.measurements).unwrap(), measurements_after);
        assert_eq!(fixture.app.document_revision, revision_after);
        assert_eq!(fixture.app.session_saved, saved_after);
    }
}

#[test]
fn unscoped_text_plan_without_pdf_does_not_invalidate_current_measurements() {
    let mut fixture = Fixture::new();
    fixture.app.inspection.records[0].drawing_binding_id = None;
    fixture.app.inspection.bindings.clear();
    fixture.app.inspection.registration_scope = None;
    fixture.app.inspection.plan.as_mut().unwrap().source_file = None;
    fixture.app.inspection.plan.as_mut().unwrap().source_fingerprint = None;
    assert!(fixture.app.current_drawing_source().is_err());
    let before = serde_json::to_value(&fixture.app.inspection).unwrap();
    let measurements = serde_json::to_value(&fixture.app.measurements).unwrap();
    let revision = fixture.app.document_revision;
    fixture.app.session_saved = true;
    fixture.app.invalidate_changed_drawing_dependencies();
    assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), before);
    assert_eq!(serde_json::to_value(&fixture.app.measurements).unwrap(), measurements);
    assert_eq!(fixture.app.document_revision, revision);
    assert!(fixture.app.session_saved);
}

#[test]
fn view_scope_unbound_scoped_vision_invalidates_descendants_and_preserves_manual_measurements() {
    let mut fixture = Fixture::new();
    fixture.app.inspection.records[0].drawing_binding_id = None;
    fixture.app.inspection.bindings.clear();
    let root_id = fixture.app.inspection.records[0].measurement_id.unwrap();
    let parent = fixture.app.measurements.iter().find(|measurement| measurement.id == root_id).unwrap().clone();
    let mut child = parent.clone();
    child.id = root_id + 1;
    child.origin = "derived".into();
    child.source_measurement_ids = vec![root_id];
    let child_id = child.id;
    let mut manual = parent;
    manual.id = root_id + 2;
    manual.origin = "manual".into();
    manual.source_measurement_ids.clear();
    let manual_id = manual.id;
    fixture.app.measurements.extend([child, manual]);
    fixture.app.inspection.dependencies.mark_created(child_id);
    fixture.app.inspection.dependencies.mark_created(manual_id);
    let root_generation = fixture.app.inspection.dependencies.revision(root_id);
    let child_generation = fixture.app.inspection.dependencies.revision(child_id);
    let manual_generation = fixture.app.inspection.dependencies.revision(manual_id);
    let revision = fixture.app.document_revision;
    fixture.app.invalidate_changed_drawing_dependencies();
    assert_eq!(fixture.app.document_revision, revision, "Current scoped registration remains usable without a binding");
    fixture.app.inspection.views[0].source_proof = None;
    fixture.app.invalidate_changed_drawing_dependencies();
    assert_eq!(fixture.app.inspection.dependencies.revision(root_id), root_generation + 1);
    assert_eq!(fixture.app.inspection.dependencies.revision(child_id), child_generation + 1);
    assert!(fixture.app.inspection.dependencies.is_stale(child_id));
    assert!(fixture.app.measurements.iter().find(|measurement| measurement.id == child_id).unwrap().dependency_stale);
    assert_eq!(fixture.app.inspection.dependencies.revision(manual_id), manual_generation);
    assert!(!fixture.app.measurements.iter().find(|measurement| measurement.id == manual_id).unwrap().dependency_stale);
    assert_eq!(fixture.app.document_revision, revision + 1);
    fixture.app.invalidate_changed_drawing_dependencies();
    assert_eq!(fixture.app.document_revision, revision + 1);
    assert_eq!(fixture.app.inspection.dependencies.revision(child_id), child_generation + 1);
}

#[test]
fn view_scope_imported_generation_collision_does_not_mutate_a_reopened_project() {
    let mut fixture = Fixture::new();
    fixture.app.inspection = serde_json::from_value(serde_json::to_value(&fixture.app.inspection).unwrap()).unwrap();
    fixture.app.inspection.dependencies = crate::measurement_graph::DependencyGraph::restore_import(
        &fixture.app.measurements).unwrap();
    fixture.app.drawing_geometry_cache.clear();
    fixture.app.refresh_dependency_flags();
    fixture.app.session_saved = true;
    let record = &fixture.app.inspection.records[0];
    assert!(record.imported);
    assert_eq!(record.measurement_revision, Some(fixture.app.inspection.dependencies.revision(record.measurement_id.unwrap())),
        "The historical stamp must collide with the reset generation to reproduce the rendering bug");
    let revision = fixture.app.document_revision;
    let before = serde_json::to_value(&fixture.app.inspection).unwrap();
    for _ in 0..3 { fixture.app.invalidate_changed_drawing_dependencies(); }
    assert_eq!(fixture.app.document_revision, revision);
    assert!(fixture.app.session_saved);
    assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), before);
    let current = fixture.app.current_inspection();
    assert_ne!(current.records[0].outcome.geometry_state, crate::dper_metrology::GeometryState::Validated);
    assert!(current.records[0].outcome.blocking_reasons.contains(&"imported_evidence_requires_reassessment".into()));
}

#[test]
fn view_scope_live_source_change_invalidates_current_dependencies_once() {
    let mut fixture = Fixture::new();
    let measurement_id = fixture.app.inspection.records[0].measurement_id.unwrap();
    assert!(!fixture.app.inspection.records[0].imported);
    let generation = fixture.app.inspection.dependencies.revision(measurement_id);
    let revision = fixture.app.document_revision;
    fixture.app.drawing_geometry_cache[0].geometry.flattening_tolerance_pt *= 2.0;
    fixture.app.invalidate_changed_drawing_dependencies();
    assert_eq!(fixture.app.inspection.dependencies.revision(measurement_id), generation + 1);
    assert_eq!(fixture.app.document_revision, revision + 1);
    let changed = serde_json::to_value(&fixture.app.inspection).unwrap();
    fixture.app.invalidate_changed_drawing_dependencies();
    assert_eq!(fixture.app.document_revision, revision + 1);
    assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), changed);
}

#[test]
fn view_scope_bound_tool_response_uses_current_catalog_without_spurious_staleness() {
    let mut fixture = Fixture::new();
    let feature_id = fixture.app.inspection.bindings[0].candidate.feature_id.clone();
    let response = fixture.app.handle_drawing_request(crate::mcp_server::DrawingRequest::InspectBound(
        crate::mcp_server::InspectBoundFeatureArgs {
            source_measurement_ids: Vec::new(), capture_id: fixture.app.capture_id.clone(),
            expected_revision: fixture.app.document_revision, feature_id,
            group: "Réponse outil liée".into(), options: None, commit: true,
        })).unwrap();
    assert_eq!(response["record"]["outcome"]["geometry_state"],
        serde_json::to_value(crate::dper_metrology::GeometryState::Validated).unwrap());
    let reasons = response["record"]["outcome"]["blocking_reasons"].as_array().unwrap();
    assert!(!reasons.iter().any(|reason| reason == "drawing_binding_changed" || reason == "registration_missing_or_stale"));
}

#[test]
fn view_scope_linked_native_decision_can_attest_a_current_observation() {
    let mut fixture = Fixture::new();
    fixture.prepare();
    fixture.fill();
    fixture.click("apply");
    let record = &fixture.app.inspection.records[0];
    assert!(record.decision_evidence.as_ref().is_some_and(|evidence| evidence.reviewed));
    assert_eq!(record.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Conforming);
}

#[test]
fn view_scope_unbound_inspect_rejects_stale_scoped_registration_but_preserves_unscoped_mode() {
    let mut fixture = Fixture::new();
    let feature_id = fixture.app.inspection.bindings[0].candidate.feature_id.clone();
    fixture.app.inspection.bindings.clear();
    let request = |app: &DperMeasureApp| crate::mcp_server::InspectFeatureArgs {
        source_measurement_ids: Vec::new(), capture_id: app.capture_id.clone(),
        expected_revision: app.document_revision, feature_id: feature_id.clone(),
        group: "Inspection explicite sans association".into(), geometry_in_drawing:
            crate::dper_vision::InspectionGeometry::Circle { center: Point { x: 64.0, y: 64.0 }, radius: 25.0 },
        options: None, commit: false,
    };
    fixture.app.perform_inspection(request(&fixture.app), None).unwrap();
    fixture.app.inspection.views[0].source_proof = None;
    let before = fixture.app.inspection.records.len();
    assert!(fixture.app.perform_inspection(request(&fixture.app), None).is_err());
    assert_eq!(fixture.app.inspection.records.len(), before);
    fixture.app.inspection.registration_scope = None;
    fixture.app.perform_inspection(request(&fixture.app), None).unwrap();
}

#[test]
fn view_scope_catalog_refresh_suspends_existing_observation_registration_and_mcp_state() {
    let mut fixture = Fixture::new();
    let view_id = fixture.app.inspection.views[0].id.clone();
    assert!(fixture.app.drawing_registration_scope(&view_id).is_ok());
    let record_count = fixture.app.inspection.records.len();
    let measurement_count = fixture.app.measurements.len();
    let mut catalog = fixture.app.drawing_geometry_cache[0].clone();
    catalog.geometry.flattening_tolerance_pt *= 2.0;
    fixture.app.handle_drawing_request(crate::mcp_server::DrawingRequest::InstallCatalog {
        catalog, expected_revision: fixture.app.document_revision,
    }).unwrap();
    assert!(fixture.app.inspection_panel_cache.is_none());
    assert!(fixture.app.drawing_registration_scope(&view_id).is_err());
    let current = fixture.app.current_inspection();
    assert_ne!(current.records[0].outcome.geometry_state, crate::dper_metrology::GeometryState::Validated);
    assert!(current.records[0].outcome.blocking_reasons.contains(&"drawing_binding_changed".into()));
    assert_eq!(fixture.app.inspection.records.len(), record_count);
    assert_eq!(fixture.app.measurements.len(), measurement_count);
    let state = fixture.app.handle_inspection_request(crate::mcp_server::InspectionRequest::GetState).unwrap();
    assert_eq!(state["drawing_view_status"][0]["current"], false);
    assert!(state["drawing_view_status"][0]["reason"].is_string());
}

#[test]
fn view_scope_historical_and_duplicate_views_are_suspended_in_application() {
    let mut fixture = Fixture::new();
    let view_id = fixture.app.inspection.views[0].id.clone();
    fixture.app.inspection.views[0].source_proof = None;
    assert!(fixture.app.drawing_registration_scope(&view_id).unwrap_err().contains("historique"));
    let state = fixture.app.handle_inspection_request(crate::mcp_server::InspectionRequest::GetState).unwrap();
    assert_eq!(state["drawing_view_status"][0]["current"], false);
    fixture.app.inspection.views.push(fixture.app.inspection.views[0].clone());
    let binding_id = fixture.app.inspection.bindings[0].candidate.id.clone();
    assert!(fixture.app.validate_drawing_binding(&binding_id).is_err());
}

struct Fixture {
    app: DperMeasureApp, ctx: egui::Context, path: std::path::PathBuf,
    shapes: Vec<egui::epaint::ClippedShape>, time: f64,
}

impl Fixture {
    fn new() -> Self {
        let ctx = egui::Context::default();
        let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
        let sequence = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("{}-{}-{sequence}.pdf", crate::new_id("decision-ui-test"), std::process::id()));
        std::fs::write(&path, b"independent synthetic PDF source, no parser invoked").unwrap();
        let fingerprint = crate::dper_source::fingerprint_file(&path).unwrap();
        let mut plan = crate::dper_drawing::parse_text_plan("Ø5 ±0.2 mm", "fixture.pdf");
        plan.source_file = Some(path.display().to_string());
        plan.source_fingerprint = Some(fingerprint.clone());
        plan.page_geometry = vec![crate::dper_pdf_layout::PdfPageGeometry { page: 1, width_pt: 128.0, height_pt: 128.0 }];
        plan.characteristics[0].confirmed = true;
        plan.characteristics[0].source.page = Some(1);
        plan.characteristics[0].source.pdf_bbox = Some(crate::dper_pdf_layout::PdfBox {
            x_min: 5.0, y_min: 108.0, x_max: 80.0, y_max: 120.0,
        });
        let catalog = crate::dper_pdf_geometry::parse_svg_geometry(
            r#"<svg width="128pt" height="128pt" viewBox="0 0 128 128"><circle cx="64" cy="64" r="25" fill="none" stroke="black"/></svg>"#, 1).unwrap();
        let feature_id = plan.characteristics[0].id.clone();
        let view = crate::dper_binding::define_view(&plan, &fingerprint, &catalog, "Vue synthétique",
            &[feature_id.clone()], &[catalog.primitives[0].id.clone()]).unwrap();
        let candidates = crate::dper_binding::binding_candidates(&plan, &fingerprint, &catalog, &view, &feature_id).unwrap();
        let mut binding = crate::dper_binding::select_binding(&candidates, &candidates[0].id, &catalog,
            "Texte et cercle synthétiques de la page 1 revus.").unwrap();
        // Fixture setup models a prior native review, not a JSON/MCP grant.
        binding.reviewed = true;
        let binding_id = binding.candidate.id.clone();
        app.install_inspection_plan(plan);
        app.inspection.registration_scope = Some(dper_inspection::DrawingRegistrationScope {
            document_fingerprint: fingerprint.clone(), page: 1, view_id: view.id.clone(),
        });
        app.inspection.views.push(view);
        app.inspection.bindings.push(binding);
        app.drawing_geometry_cache.push(crate::dper_pdf_geometry::DrawingGeometryCatalog {
            document_fingerprint: fingerprint, geometry: catalog,
        });
        let image = image::RgbImage::from_fn(128, 128, |x, y| {
            let distance = (x as f64 - 64.0).hypot(y as f64 - 64.0);
            image::Rgb([(20.0 + 200.0 * (distance - 24.5).clamp(0.0, 1.0)) as u8; 3])
        });
        app.image = Some(crate::ImageData { width: 128, height: 128,
            rgb: image.as_raw().clone(), source: "synthetic-observed.png".into() });
        let mut quality = crate::quality::assess_image_quality(image.as_raw(), 128, 128,
            &crate::quality::ImageQualityThresholds::default()).unwrap();
        quality.is_exploitable = true; // isolate the native review gates
        app.quality_assessment = Some(quality);
        app.calibration = Some(serde_json::from_value(serde_json::json!({
            "id": "independent-calibration", "device": "reference", "resolution": [128,128],
            "optical_setup": "fixture", "mm_per_pixel": 0.1,
            "reference_mm": 10.0, "reference_px": 100.0, "expanded_uncertainty_mm": 0.002,
        })).unwrap());
        app.mm_per_pixel = Some(0.1);
        let correspondences = [Point { x: 10.0, y: 10.0 }, Point { x: 30.0, y: 10.0 }, Point { x: 10.0, y: 30.0 }]
            .map(|plan| crate::dper_registration::Correspondence { plan, image: plan });
        app.inspection.registration = Some(crate::dper_registration::register_similarity(&correspondences, 0.1).unwrap());
        app.inspection.registration_capture_id = Some(app.capture_id.clone());
        app.inspection.registration_fingerprint = Some(dper_inspection::fingerprint(&image));
        app.perform_inspection(crate::mcp_server::InspectFeatureArgs {
            source_measurement_ids: Vec::new(), capture_id: app.capture_id.clone(),
            expected_revision: app.document_revision, feature_id, group: "Pièce témoin".into(),
            geometry_in_drawing: crate::dper_vision::InspectionGeometry::Circle {
                center: Point { x: 64.0, y: 64.0 }, radius: 25.0,
            }, options: None, commit: true,
        }, Some(binding_id)).unwrap();
        assert_eq!(app.current_inspection().records[0].outcome.geometry_state,
            crate::dper_metrology::GeometryState::Validated);
        let mut fixture = Self { app, ctx, path, shapes: Vec::new(), time: 0.0 };
        fixture.settle(); fixture
    }

    fn frame(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        self.time += 1.0 / 60.0;
        self.shapes = self.ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1800.0))),
            time: Some(self.time), focused: true, events, modifiers, ..Default::default()
        }, |ctx| { egui::CentralPanel::default().show(ctx, |ui| self.app.inspection_decision_panel(ui)); }).shapes;
    }
    fn settle(&mut self) { for _ in 0..3 { self.frame(Vec::new(), Default::default()); } }
    fn click_at(&mut self, pos: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default(),
        }], Default::default());
        self.frame(vec![egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default(),
        }], Default::default()); self.settle();
    }
    fn click(&mut self, key: &'static str) { self.click_at(self.app.inspection_decision_ui.widgets[key].center()); }
    fn text_rect(&self, needle: &str) -> egui::Rect {
        fn collect(shape: &egui::epaint::Shape, needle: &str, output: &mut Vec<egui::Rect>) {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == needle => output.push(text.galley.rect.translate(text.pos.to_vec2())),
                egui::epaint::Shape::Vec(shapes) => for shape in shapes { collect(shape, needle, output); }, _ => {}
            }
        }
        let mut rectangles = Vec::new();
        for clipped in &self.shapes { collect(&clipped.shape, needle, &mut rectangles); }
        assert_eq!(rectangles.len(), 1, "Rendered label {needle}"); rectangles[0]
    }
    fn replace(&mut self, key: &'static str, value: &str) {
        self.click(key);
        let modifiers = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        self.frame(vec![egui::Event::Key { key: egui::Key::A, physical_key: Some(egui::Key::A),
            pressed: true, repeat: false, modifiers }], modifiers);
        let replacement = if value.is_empty() {
            egui::Event::Key { key: egui::Key::Backspace, physical_key: Some(egui::Key::Backspace),
                pressed: true, repeat: false, modifiers: Default::default() }
        } else { egui::Event::Text(value.into()) };
        self.frame(vec![egui::Event::Key { key: egui::Key::A, physical_key: Some(egui::Key::A),
            pressed: false, repeat: false, modifiers: Default::default() }, replacement], Default::default());
        self.settle();
    }
    fn prepare(&mut self) {
        let label = RecordKey::of(&self.app.inspection.records[0]).unwrap().label();
        self.click("record"); self.click_at(self.text_rect(&label).center());
        self.click("prepare"); assert!(self.app.inspection_decision_ui.draft.is_some());
    }
    fn fill(&mut self) {
        for (key, value) in [("qualification", "QUAL-2026-17 rev B"),
            ("scope", "Diamètres 4 à 6 mm dans le plan de mesure, montage fixture"),
            ("uncertainty", "0,05"), ("source", "Bilan U-M17 rev C, U élargie k=2, diamètre complet"),
            ("reviewer", "Opérateur témoin")] { self.replace(key, value); }
    }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_file(&self.path); } }

#[test]
fn native_decision_pointer_keyboard_review_applies_total_u_without_changing_calibration_or_geometry() {
    let mut f = Fixture::new();
    let before_calibration = serde_json::to_value(&f.app.calibration).unwrap();
    let generation = f.app.inspection.dependencies.revision(f.app.measurements[0].id);
    let revision = f.app.document_revision;
    f.prepare(); f.fill();
    assert_eq!(f.app.document_revision, revision);
    assert!(f.app.inspection.records[0].decision_evidence.is_none());
    assert_eq!(f.app.current_inspection().records[0].outcome.conformity_status,
        crate::dper_metrology::ConformityStatus::Indeterminate);
    f.click("apply");
    let record = &f.app.inspection.records[0];
    let evidence = record.decision_evidence.as_ref().expect("explicit pointer attestation");
    assert!(evidence.reviewed);
    assert_eq!(evidence.basis.expanded_uncertainty_mm, 0.05);
    assert_eq!(evidence.basis.qualification_reference, "QUAL-2026-17 rev B");
    assert_eq!(record.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Conforming);
    assert_eq!(f.app.document_revision, revision + 1);
    assert!(!f.app.session_saved);
    assert!(f.app.inspection_decision_ui.draft.is_none());
    assert_eq!(f.app.inspection.dependencies.revision(f.app.measurements[0].id), generation);
    assert_eq!(serde_json::to_value(&f.app.calibration).unwrap(), before_calibration);
    assert_eq!(f.app.measurements.len(), 1);
    let current = f.app.current_inspection();
    let via_mcp = f.app.handle_inspection_request(crate::mcp_server::InspectionRequest::GetState).unwrap();
    assert_eq!(via_mcp["inspection"]["records"][0], serde_json::to_value(&current.records[0]).unwrap());
    let historical: crate::dper_inspection::InspectionWorkspace = serde_json::from_value(
        serde_json::to_value(&f.app.inspection).unwrap()).unwrap();
    assert!(historical.records[0].imported);
    assert!(!historical.records[0].decision_evidence.as_ref().unwrap().reviewed);
    f.app.inspection = historical;
    assert_eq!(f.app.current_inspection().records[0].outcome.conformity_status,
        crate::dper_metrology::ConformityStatus::Indeterminate);
}

#[test]
fn native_decision_stale_review_is_noop_and_never_refreshes_its_context() {
    for change in 0..9 {
        let mut f = Fixture::new(); f.prepare(); f.fill();
        let prepared_revision = f.app.inspection_decision_ui.draft.as_ref().unwrap().context.revision;
        match change {
            0 => f.app.document_revision += 1,
            1 => f.app.capture_id = "other-capture".into(),
            2 => f.app.inspection.plan.as_mut().unwrap().characteristics[0].nominal = Some(99.0),
            3 => { let bytes = std::fs::read(&f.path).unwrap(); std::fs::write(&f.path, vec![b'x'; bytes.len()]).unwrap(); },
            4 => f.app.image.as_mut().unwrap().rgb[0] ^= 1,
            5 => f.app.calibration.as_mut().unwrap().mm_per_pixel = 0.2,
            6 => f.app.inspection.bindings[0].reviewed = false,
            7 => { f.app.running = true; f.app.paused = false; },
            _ => f.app.calibration_mode = true,
        }
        let revision = f.app.document_revision;
        let workspace = serde_json::to_value(&f.app.inspection).unwrap();
        f.click("apply");
        assert_eq!(f.app.document_revision, revision, "change={change}");
        assert_eq!(serde_json::to_value(&f.app.inspection).unwrap(), workspace, "change={change}");
        assert_eq!(f.app.inspection_decision_ui.draft.as_ref().unwrap().context.revision, prepared_revision);
        assert!(f.app.inspection.records[0].decision_evidence.is_none());
    }
}

#[test]
fn native_decision_invalid_or_missing_basis_fields_cannot_grant_review() {
    for (key, value) in [("qualification", ""), ("scope", ""), ("source", ""), ("reviewer", ""),
        ("uncertainty", "NaN"), ("uncertainty", "0"), ("uncertainty", "-0.1")] {
        let mut f = Fixture::new(); f.prepare(); f.fill(); f.replace(key, value);
        let revision = f.app.document_revision;
        let workspace = serde_json::to_value(&f.app.inspection).unwrap();
        f.click("apply");
        assert_eq!(f.app.document_revision, revision, "{key}={value}");
        assert_eq!(serde_json::to_value(&f.app.inspection).unwrap(), workspace);
        assert!(f.app.inspection_decision_ui.draft.is_some());
    }
}

#[test]
fn native_decision_imported_duplicate_and_unreviewed_binding_observations_cannot_prepare() {
    for case in 0..3 {
        let mut f = Fixture::new();
        let key = RecordKey::of(&f.app.inspection.records[0]).unwrap();
        match case {
            0 => f.app.inspection.records[0].imported = true,
            1 => f.app.inspection.records.push(f.app.inspection.records[0].clone()),
            _ => f.app.inspection.bindings[0].reviewed = false,
        }
        f.app.inspection_decision_ui.selected = Some(key);
        f.settle(); f.click("prepare");
        assert!(f.app.inspection_decision_ui.draft.is_none());
        assert!(f.app.inspection.records.iter().all(|record| record.decision_evidence.is_none()));
    }
}

#[test]
fn native_decision_pointer_withdrawal_preserves_observation_and_removes_physical_authority() {
    let mut f = Fixture::new(); f.prepare(); f.fill(); f.click("apply");
    let points = f.app.inspection.records[0].measurement_points.clone();
    let measurements = serde_json::to_value(&f.app.measurements).unwrap();
    let calibration = serde_json::to_value(&f.app.calibration).unwrap();
    let generation = f.app.inspection.dependencies.revision(f.app.measurements[0].id);
    let drawing_generation = f.app.drawing_geometry_generation;
    let revision = f.app.document_revision;
    // Preparing replacement documentation must never retract the current basis.
    f.click("prepare");
    assert!(f.app.inspection.records[0].decision_evidence.is_some());
    assert_eq!(f.app.document_revision, revision);
    f.click("withdraw");
    let record = &f.app.inspection.records[0];
    assert!(record.decision_evidence.is_none(), "former U and documentary declaration are removed together");
    assert_eq!(record.outcome.geometry_state, crate::dper_metrology::GeometryState::Validated);
    assert_eq!(record.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Indeterminate);
    assert!(record.outcome.blocking_reasons.contains(&"uncertainty_unknown".into()));
    assert_eq!(record.measurement_points, points);
    assert_eq!(serde_json::to_value(&f.app.measurements).unwrap(), measurements);
    assert_eq!(serde_json::to_value(&f.app.calibration).unwrap(), calibration);
    assert_eq!(f.app.inspection.dependencies.revision(f.app.measurements[0].id), generation);
    assert_eq!(f.app.drawing_geometry_generation, drawing_generation);
    assert_eq!(f.app.document_revision, revision + 1);
    assert!(!f.app.session_saved);
    assert!(f.app.inspection_panel_cache.is_none());
    assert!(f.app.inspection_decision_ui.draft.is_none());
}

#[test]
fn native_decision_pointer_withdrawal_refuses_active_actions_live_image_and_changed_source() {
    for change in 0..3 {
        let mut f = Fixture::new(); f.prepare(); f.fill(); f.click("apply");
        match change {
            0 => f.app.calibration_mode = true,
            1 => { f.app.running = true; f.app.paused = false; },
            _ => { let bytes = std::fs::read(&f.path).unwrap(); std::fs::write(&f.path, vec![b'x'; bytes.len()]).unwrap(); },
        }
        let revision = f.app.document_revision;
        let workspace = serde_json::to_value(&f.app.inspection).unwrap();
        f.click("withdraw");
        assert_eq!(f.app.document_revision, revision, "change={change}");
        assert_eq!(serde_json::to_value(&f.app.inspection).unwrap(), workspace, "change={change}");
        assert!(f.app.inspection.records[0].decision_evidence.is_some());
    }
}

#[test]
fn native_decision_withdrawal_rejects_stale_rendered_context_before_mutation() {
    let mut f = Fixture::new(); f.prepare(); f.fill(); f.click("apply");
    let key = RecordKey::of(&f.app.inspection.records[0]).unwrap();
    let context = f.app.decision_context().unwrap();
    f.app.document_revision += 1;
    let workspace = serde_json::to_value(&f.app.inspection).unwrap();
    let revision = f.app.document_revision;
    assert!(f.app.withdraw_native_decision(&key, &context).is_err());
    assert_eq!(f.app.document_revision, revision);
    assert_eq!(serde_json::to_value(&f.app.inspection).unwrap(), workspace);
}

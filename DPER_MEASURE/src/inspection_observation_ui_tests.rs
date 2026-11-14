use super::*;

static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Fixture {
    app: DperMeasureApp,
    ctx: egui::Context,
    path: std::path::PathBuf,
    shapes: Vec<egui::epaint::ClippedShape>,
    time: f64,
}

impl Fixture {
    fn new() -> Self {
        let ctx = egui::Context::default();
        let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
        let sequence = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("{}-{}-{sequence}.pdf", crate::new_id("observation-ui-test"), std::process::id()));
        std::fs::write(&path, b"independent drawing source, no parser invoked").unwrap();
        let mut plan = crate::dper_drawing::parse_text_plan("Ø50 ±1 mm", "fixture.pdf");
        plan.source_file = Some(path.display().to_string());
        plan.source_fingerprint = Some(crate::dper_source::fingerprint_file(&path).unwrap());
        plan.characteristics[0].confirmed = true;
        app.install_inspection_plan(plan);
        let image = image::RgbImage::from_fn(128, 128, |x, y| {
            let distance = (x as f64 - 64.0).hypot(y as f64 - 64.0);
            image::Rgb([(20.0 + 200.0 * (distance - 24.5).clamp(0.0, 1.0)) as u8; 3])
        });
        app.image = Some(crate::ImageData {
            width: 128, height: 128, rgb: image.as_raw().clone(), source: "synthetic-observed.png".into(),
        });
        let mut quality = crate::quality::assess_image_quality(
            image.as_raw(), 128, 128, &crate::quality::ImageQualityThresholds::default()).unwrap();
        // This fixture isolates contour/interaction gates. Qualification of the
        // real quality thresholds belongs to the physical acquisition protocol.
        quality.is_exploitable = true;
        app.quality_assessment = Some(quality);
        app.calibration = Some(serde_json::from_value(serde_json::json!({
            "id": "independent-calibration", "device": "reference", "resolution": [128,128],
            "optical_setup": "fixture", "mm_per_pixel": 0.1,
            "reference_mm": 10.0, "reference_px": 100.0, "expanded_uncertainty_mm": 0.002,
        })).unwrap());
        app.mm_per_pixel = Some(0.1);
        let correspondences = [Point { x: 5.0, y: 5.0 }, Point { x: 25.0, y: 5.0 }, Point { x: 5.0, y: 25.0 }]
            .map(|plan| crate::dper_registration::Correspondence {
                plan, image: Point { x: plan.x * 2.0, y: plan.y * 2.0 },
            });
        app.inspection.registration = Some(crate::dper_registration::register_similarity(&correspondences, 0.1).unwrap());
        app.inspection.registration_capture_id = Some(app.capture_id.clone());
        app.inspection.registration_fingerprint = Some(dper_inspection::fingerprint(&image));
        let mut fixture = Self { app, ctx, path, shapes: Vec::new(), time: 0.0 };
        fixture.settle();
        fixture
    }

    fn frame(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        self.time += 1.0 / 60.0;
        self.shapes = self.ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 1800.0))),
            time: Some(self.time), focused: true, events, modifiers, ..Default::default()
        }, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| self.app.inspection_observation_panel(ui));
        }).shapes;
    }

    fn settle(&mut self) { for _ in 0..3 { self.frame(Vec::new(), egui::Modifiers::default()); } }

    fn click_at(&mut self, pos: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default(),
        }], Default::default());
        self.frame(vec![egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default(),
        }], Default::default());
        self.settle();
    }

    fn click(&mut self, key: &'static str) {
        self.click_at(self.app.inspection_observation_ui.widgets[key].center());
    }

    fn text_rect(&self, needle: &str) -> egui::Rect {
        fn collect(shape: &egui::epaint::Shape, needle: &str, output: &mut Vec<egui::Rect>) {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == needle =>
                    output.push(text.galley.rect.translate(text.pos.to_vec2())),
                egui::epaint::Shape::Vec(shapes) => for shape in shapes { collect(shape, needle, output); },
                _ => {}
            }
        }
        let mut rectangles = Vec::new();
        for clipped in &self.shapes { collect(&clipped.shape, needle, &mut rectangles); }
        assert_eq!(rectangles.len(), 1, "Rendered label {needle}");
        rectangles[0]
    }

    fn replace(&mut self, key: &'static str, value: &str) {
        self.click(key);
        let modifiers = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        self.frame(vec![egui::Event::Key {
            key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: true, repeat: false, modifiers,
        }], modifiers);
        self.frame(vec![egui::Event::Key {
            key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: false, repeat: false,
            modifiers: Default::default(),
        }, egui::Event::Text(value.into())], Default::default());
        self.settle();
    }

    fn select_feature(&mut self) {
        self.click("feature");
        let feature = &self.app.inspection.plan.as_ref().unwrap().characteristics[0];
        self.click_at(self.text_rect(&format!("{} · {}", feature.id, feature.label)).center());
    }

    fn prepare_via_widgets(&mut self) {
        self.select_feature();
        self.replace("group", "Pièce témoin");
        let revision = self.app.document_revision;
        self.click("prepare");
        assert_eq!(self.app.document_revision, revision, "Preparation must be read-only");
        assert!(self.app.measurements.is_empty());
        assert!(self.app.inspection.records.is_empty());
        self.replace("Centre X (plan)", "32");
        self.replace("Centre Y (plan)", "32");
        self.replace("Rayon (plan)", "11");
    }

    fn prepare_direct(&mut self) {
        let mut state = std::mem::take(&mut self.app.inspection_observation_ui);
        state.feature_id = self.app.inspection.plan.as_ref().unwrap().characteristics[0].id.clone();
        self.app.prepare_native_observation(&mut state).unwrap();
        state.draft.as_mut().unwrap().values = ["32", "32", "11", "0", "180"].map(str::to_owned);
        self.app.inspection_observation_ui = state;
    }

    fn run(&mut self, commit: bool) -> Result<(), String> {
        let mut state = std::mem::take(&mut self.app.inspection_observation_ui);
        let result = self.app.run_native_observation(&mut state, commit);
        self.app.inspection_observation_ui = state;
        result
    }
}

impl Drop for Fixture {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.path); }
}

fn repeated_fixture() -> (Fixture, String) {
    repeated_fixture_with_plan("Ø50 ±1 mm")
}

fn repeated_fixture_with_plan(text: &str) -> (Fixture, String) {
    use crate::dper_pdf_geometry::{parse_svg_geometry, DrawingGeometryCatalog};
    use crate::dper_pdf_layout::{PdfBox, PdfPageGeometry};
    use crate::mcp_server::{AssignDrawingOccurrencesArgs, BindingCandidatesArgs, DefineDrawingViewArgs};
    let mut fixture = Fixture::new();
    let app = &mut fixture.app;
    let plan = app.inspection.plan.as_mut().unwrap();
    plan.characteristics = crate::dper_drawing::parse_text_plan(text, "fixture.pdf").characteristics;
    plan.characteristics[0].confirmed = true;
    plan.characteristics[0].repetition = Some(2);
    plan.characteristics[0].source.page = Some(1);
    plan.characteristics[0].source.pdf_bbox = Some(PdfBox { x_min: 100.0, y_min: 18.0, x_max: 140.0, y_max: 27.0 });
    plan.page_geometry = vec![PdfPageGeometry { page: 1, width_pt: 256.0, height_pt: 128.0 }];
    let feature_id = plan.characteristics[0].id.clone();
    let fingerprint = plan.source_fingerprint.clone().unwrap();
    let geometry = parse_svg_geometry(
        r#"<svg width="256pt" height="128pt" viewBox="0 0 256 128"><circle cx="64" cy="64" r="24"/><circle cx="192" cy="64" r="24"/></svg>"#, 1).unwrap();
    let geometry_ids = geometry.primitives.iter().map(|primitive| primitive.id.clone()).collect();
    app.handle_drawing_request(DrawingRequest::InstallCatalog {
        catalog: DrawingGeometryCatalog { document_fingerprint: fingerprint, geometry },
        expected_revision: app.document_revision,
    }).unwrap();
    let view = app.handle_drawing_request(DrawingRequest::DefineView(DefineDrawingViewArgs {
        expected_revision: app.document_revision, page: 1, label: "Deux centres indépendants".into(),
        feature_ids: vec![feature_id.clone()], geometry_ids, replace_view_id: None,
    })).unwrap();
    let view_id = view["view"]["id"].as_str().unwrap().to_string();
    let candidates = app.handle_drawing_request(DrawingRequest::Candidates(BindingCandidatesArgs {
        feature_id: feature_id.clone(), view_id: view_id.clone(),
    })).unwrap();
    let candidate_ids = candidates["candidates"].as_array().unwrap().iter()
        .map(|candidate| candidate["id"].as_str().unwrap().to_owned()).collect();
    app.handle_drawing_request(DrawingRequest::AssignOccurrences(AssignDrawingOccurrencesArgs {
        expected_revision: app.document_revision, feature_id, view_id: view_id.clone(),
        candidate_ids, justification: "Deux cercles situés sur deux emplacements distincts.".into(),
    })).unwrap();
    let image = image::RgbImage::from_fn(256, 128, |x, y| {
        let left = (x as f64 - 64.0).hypot(y as f64 - 64.0) - 22.5;
        let right = (x as f64 - 192.0).hypot(y as f64 - 64.0) - 24.5;
        image::Rgb([(20.0 + 200.0 * left.min(right).clamp(0.0, 1.0)) as u8; 3])
    });
    app.image = Some(crate::ImageData { width: 256, height: 128, rgb: image.as_raw().clone(), source: "two-disks.png".into() });
    app.calibration.as_mut().unwrap().resolution = [256, 128];
    let pairs = [Point { x: 50.0, y: 45.0 }, Point { x: 205.0, y: 45.0 }, Point { x: 50.0, y: 80.0 }]
        .map(|plan| crate::dper_registration::Correspondence { plan, image: plan });
    app.inspection.registration = Some(crate::dper_registration::register_similarity(&pairs, 0.1).unwrap());
    app.inspection.registration_scope = Some(app.drawing_registration_scope(&view_id).unwrap());
    app.inspection.registration_capture_id = Some(app.capture_id.clone());
    app.inspection.registration_fingerprint = Some(dper_inspection::fingerprint(&image));
    fixture.settle();
    (fixture, view_id)
}

#[test]
fn occurrence_decisions_require_all_distinct_current_members() {
    use crate::dper_metrology::ConformityStatus;
    for (text, expected) in [
        ("Ø4.8 ±0.35 mm", ConformityStatus::Conforming),
        ("Ø4.6 ±0.15 mm", ConformityStatus::Nonconforming),
        ("Ø4.8 ±0.19 mm", ConformityStatus::Indeterminate),
    ] {
        let (mut fixture, _) = repeated_fixture_with_plan(text);
        fixture.select_feature();
        let members = fixture.app.inspection.occurrence_assignments[0].members.clone();
        for (index, member) in members.iter().enumerate() {
            fixture.click("occurrence");
            fixture.click_at(fixture.text_rect(&format!("Occurrence {} · {}", member.number, member.geometry_id)).center());
            fixture.click("prepare");
            fixture.run(false).unwrap();
            fixture.run(true).unwrap();

            let app = &mut fixture.app;
            let record_index = app.inspection.records.len() - 1;
            let mut record = app.inspection.records[record_index].clone();
            assert_eq!(record.outcome.conformity_status, ConformityStatus::Indeterminate);
            let image = app.inspection_image().unwrap();
            dper_inspection::attach_decision_evidence_with_catalogs(
                &mut record, &app.inspection, &image, &app.inspection_frame(), &app.measurements,
                dper_inspection::DecisionBasis {
                    qualification_reference: "Synthetic qualification fixture".into(),
                    qualification_scope: "Independent synthetic disks".into(),
                    expanded_uncertainty_mm: 0.04,
                    uncertainty_source: "Synthetic uncertainty fixture".into(),
                    reviewer: "Test operator".into(),
                }, &app.drawing_geometry_cache,
            ).unwrap();
            app.inspection.records[record_index] = record;
            let state = app.handle_inspection_request(crate::mcp_server::InspectionRequest::GetState).unwrap();
            let group = &state["drawing_occurrence_observations"][0];
            if index == 0 {
                assert_eq!(group["conformity"], "indeterminate");
                assert_eq!(group["decision_blocker"], "incomplete_observations");
            } else {
                assert_eq!(group["conformity"], serde_json::to_value(expected).unwrap());
            }
        }
        if expected == ConformityStatus::Conforming {
            let mut snapshot = fixture.app.current_inspection();
            let last = snapshot.records.last_mut().unwrap();
            last.group = "Groupe distinct".into();
            let group = &crate::drawing_structure::occurrence_observations(&snapshot)[0];
            assert_eq!(group.conformity, ConformityStatus::Indeterminate);
            assert_eq!(group.decision_blocker, Some("mixed_observation_groups"));
            let assignment = &mut snapshot.occurrence_assignments[0];
            assignment.members[1] = assignment.members[0].clone();
            let group = &crate::drawing_structure::occurrence_observations(&snapshot)[0];
            assert_eq!(group.decision_blocker, Some("assignment_unavailable"));

            fixture.click("prepare");
            fixture.run(false).unwrap();
            fixture.run(true).unwrap();
            let group = &crate::drawing_structure::occurrence_observations(&fixture.app.current_inspection())[0];
            assert_eq!(group.conformity, ConformityStatus::Indeterminate);
            assert_eq!(group.decision_blocker, Some("ambiguous_observations"));
            std::fs::write(&fixture.path, b"changed drawing source").unwrap();
            let group = &crate::drawing_structure::occurrence_observations(&fixture.app.current_inspection())[0];
            assert_eq!(group.conformity, ConformityStatus::Indeterminate);
            assert!(group.observed_occurrence_ids.is_empty());
        }
    }
}

#[test]
fn occurrence_native_picker_preview_and_two_distinct_measurements() {
    let (mut fixture, _) = repeated_fixture();
    fixture.select_feature();
    let members = fixture.app.inspection.occurrence_assignments[0].members.clone();
    for member in members {
        fixture.click("occurrence");
        fixture.click_at(fixture.text_rect(&format!("Occurrence {} · {}", member.number, member.geometry_id)).center());
        fixture.click("prepare");
        assert!(fixture.app.inspection_observation_ui.draft.is_some(), "{}", fixture.app.inspection_observation_ui.status);
        let count = fixture.app.measurements.len();
        fixture.click("record");
        assert_eq!(fixture.app.measurements.len(), count);
        fixture.click("preview");
        assert_eq!(fixture.app.measurements.len(), count);
        fixture.click("record");
        assert_eq!(fixture.app.measurements.len(), count + 1, "{}", fixture.app.inspection_observation_ui.status);
        let record = fixture.app.inspection.records.last().unwrap();
        assert_eq!(record.occurrence.as_ref().unwrap().member, member);
        assert_eq!(record.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Indeterminate);
        assert!(record.outcome.blocking_reasons.contains(&"drawing_occurrence_group_unassessed".into()));
        assert_eq!(record.outcome.geometry_state, crate::dper_metrology::GeometryState::Validated);
        let summary = crate::drawing_structure::occurrence_observations(&fixture.app.current_inspection());
        assert_eq!(summary[0].observed_occurrence_ids.len(), count + 1);
    }
    let mut values = fixture.app.inspection.records.iter().filter(|record| record.measurement_id.is_some())
        .map(|record| record.value_mm.unwrap()).collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    assert!((values[0] - 4.6).abs() < 0.05 && (values[1] - 5.0).abs() < 0.05, "{values:?}");
    fixture.click("prepare");
    fixture.click("preview");
    fixture.click("record");
    assert_eq!(fixture.app.measurements.len(), 3);
    assert_eq!(crate::drawing_structure::occurrence_observations(&fixture.app.current_inspection())[0]
        .observed_occurrence_ids.len(), 2, "A second recording of one member cannot add an occurrence");
}

#[test]
fn dpm20_guidance_targets_missing_member_and_rejects_stale_assignment() {
    let (mut fixture, _) = repeated_fixture();
    fixture.select_feature();
    let members = fixture.app.inspection.occurrence_assignments[0].members.clone();
    fixture.click("occurrence");
    fixture.click_at(fixture.text_rect(&format!("Occurrence {} · {}", members[0].number, members[0].geometry_id)).center());
    fixture.click("prepare");
    fixture.click("preview");
    fixture.click("record");
    let current = fixture.app.current_inspection();
    let progress = crate::inspection_progress::summarize(&current);
    assert_eq!(progress[0].next_action.code, "observe_contours");
    let observed = &current.records.last().unwrap().occurrence.as_ref().unwrap().member.id;
    let missing = members.iter().find(|member| &member.id != observed).unwrap();
    assert_eq!(progress[0].next_action.occurrence_id.as_deref(), Some(missing.id.as_str()));
    fixture.app.inspection.occurrence_assignments[0].document_fingerprint.push_str("changed");
    let before = serde_json::to_value(&fixture.app.inspection).unwrap();
    let current = fixture.app.current_inspection();
    assert_eq!(crate::inspection_progress::summarize(&current)[0].next_action.code, "assign_occurrences");
    assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), before);
}

#[test]
fn occurrence_missing_contour_overlapping_search_and_wrong_scope_are_refused() {
    for case in 0..4 {
        let (mut fixture, _) = repeated_fixture();
        let app = &mut fixture.app;
        let assignment = app.inspection.occurrence_assignments[0].clone();
        let mut options = VisionOptions::default();
        let occurrence_id = match case {
            0 => "unknown-occurrence".to_owned(),
            1 => {
                let image = app.image.as_mut().unwrap();
                for y in 0..128 { for x in 0..128 {
                    let offset = (y * 256 + x) * 3;
                    image.rgb[offset..offset + 3].fill(220);
                }}
                let pixels = image::RgbImage::from_raw(image.width, image.height, image.rgb.clone()).unwrap();
                app.inspection.registration_fingerprint = Some(dper_inspection::fingerprint(&pixels));
                // The first member belongs to the left disk in this fixture.
                assignment.members[0].id.clone()
            }
            2 => {
                options.search_radius_px = 32.0;
                app.inspection.registration.as_mut().unwrap().transform.scale = 0.5;
                assignment.members[0].id.clone()
            }
            _ => {
                app.inspection.registration_scope = None;
                assignment.members[0].id.clone()
            }
        };
        let revision = app.document_revision;
        let result = app.handle_drawing_request(DrawingRequest::InspectBound(InspectBoundFeatureArgs {
            source_measurement_ids: Vec::new(), capture_id: app.capture_id.clone(),
            expected_revision: revision, feature_id: assignment.feature_id.clone(), occurrence_id: Some(occurrence_id),
            group: "Refus contrôlé".into(), options: Some(options), commit: false,
        }));
        if case == 2 { assert!(result.as_ref().unwrap_err().contains("recouvrent")); }
        assert!(result.is_err(), "case {case}");
        assert_eq!(app.document_revision, revision);
        assert!(app.inspection.records.is_empty());
        assert!(app.measurements.is_empty());
    }
}

#[test]
fn occurrence_reassignment_invalidates_old_measurement_and_descendant_permanently() {
    let (mut fixture, view_id) = repeated_fixture();
    let app = &mut fixture.app;
    let assignment = app.inspection.occurrence_assignments[0].clone();
    let feature_id = assignment.feature_id.clone();
    let request = |app: &DperMeasureApp, commit| DrawingRequest::InspectBound(InspectBoundFeatureArgs {
        capture_id: app.capture_id.clone(), expected_revision: app.document_revision,
        source_measurement_ids: Vec::new(), feature_id: feature_id.clone(),
        occurrence_id: Some(assignment.members[0].id.clone()), group: "Deux contours".into(), options: None, commit,
    });
    let before = app.document_revision;
    assert!(app.handle_drawing_request(request(app, true)).is_err());
    assert_eq!(app.document_revision, before);
    app.handle_drawing_request(request(app, false)).unwrap();
    app.handle_drawing_request(request(app, true)).unwrap();
    let id = app.inspection.records.last().unwrap().measurement_id.unwrap();
    // Construct a dependent manually to exercise the invalidation graph even
    // while the whole repeated requirement remains unavailable as a parent.
    let child = app.add_measurement_with_construction(crate::Tool::Distance,
        vec![Point { x: 15.0, y: 15.0 }, Point { x: 20.0, y: 15.0 }], Some("Descendant".into()),
        app.capture_id.clone(), None, None, "manual".into(), None, true, vec![id]);
    assert!(app.validate_dependency_sources(&app.capture_id, &[id]).is_ok());
    for reversed in [true, false] {
        let mut candidate_ids = assignment.members.iter().map(|member| member.candidate_id.clone()).collect::<Vec<_>>();
        if reversed { candidate_ids.reverse(); }
        app.handle_drawing_request(DrawingRequest::AssignOccurrences(crate::mcp_server::AssignDrawingOccurrencesArgs {
            expected_revision: app.document_revision, feature_id: feature_id.clone(), view_id: view_id.clone(),
            candidate_ids, justification: assignment.justification.clone(),
        })).unwrap();
        assert!(app.measurements.iter().find(|item| item.id == id).unwrap().dependency_stale);
        assert!(app.measurements.iter().find(|item| item.id == child.id).unwrap().dependency_stale);
        let current = app.current_inspection();
        assert!(current.records.last().unwrap().outcome.blocking_reasons.contains(&"measurement_revision_changed".into()));
        assert!(app.validate_dependency_sources(&app.capture_id, &[id]).is_err());
    }
}

#[test]
fn native_pointer_keyboard_preview_edit_group_and_explicit_record_observe_photo() {
    let mut fixture = Fixture::new();
    let calibration = serde_json::to_value(&fixture.app.calibration).unwrap();
    fixture.prepare_via_widgets();
    fixture.click("preview");
    assert!(fixture.app.measurements.is_empty(), "Preview must not create a measurement");
    let first = fixture.app.inspection.records.last().expect("real contour preview");
    assert_eq!(first.group, "Pièce témoin");
    assert!(first.result.evidence.accepted_points.len() > 60);
    assert!((first.value_mm.unwrap() - 5.0).abs() < 0.1, "Independent 0.1 mm/px scale");
    match first.result.geometry {
        InspectionGeometry::Circle { radius, .. } => assert!((radius - 25.0).abs() < 0.5),
        _ => panic!("circle expected"),
    }
    assert_eq!(fixture.app.inspection.plan.as_ref().unwrap().characteristics[0].nominal, Some(50.0));
    fixture.replace("Rayon (plan)", "12");
    fixture.click("record");
    assert!(fixture.app.measurements.is_empty(), "Changed proposal needs a new preview");
    fixture.click("preview");
    fixture.click("record");
    assert_eq!(fixture.app.measurements.len(), 1);
    assert!(fixture.app.inspection_observation_ui.draft.is_none());
    let measured = fixture.app.inspection.records.last().unwrap();
    assert!(measured.measurement_id.is_some());
    assert_eq!(measured.group, "Pièce témoin");
    assert_eq!(measured.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Indeterminate);
    assert_eq!(serde_json::to_value(&fixture.app.calibration).unwrap(), calibration);
    assert_eq!(fixture.app.mm_per_pixel, Some(0.1));
    let revision = fixture.app.document_revision;
    fixture.click("reevaluate");
    assert_eq!(fixture.app.document_revision, revision, "Reevaluation is read-only");
    assert_eq!(fixture.app.measurements.len(), 1);
}

#[test]
fn native_observation_stale_context_refuses_preview_and_record_without_refresh() {
    for commit in [false, true] {
        for change in 0..7 {
            let mut fixture = Fixture::new();
            fixture.prepare_direct();
            if commit { fixture.run(false).unwrap(); }
            let prepared_revision = fixture.app.inspection_observation_ui.draft.as_ref().unwrap().context.revision;
            match change {
                0 => fixture.app.document_revision += 1,
                1 => fixture.app.capture_id = "other-capture".into(),
                2 => fixture.app.inspection.plan.as_mut().unwrap().characteristics[0].nominal = Some(999.0),
                3 => {
                    let length = std::fs::metadata(&fixture.path).unwrap().len();
                    std::fs::write(&fixture.path, vec![b'x'; length as usize]).unwrap();
                },
                4 => fixture.app.image.as_mut().unwrap().rgb[0] = 0,
                5 => fixture.app.inspection.registration.as_mut().unwrap().transform.scale = 3.0,
                _ => fixture.app.calibration.as_mut().unwrap().mm_per_pixel = 0.2,
            }
            let revision = fixture.app.document_revision;
            let workspace = serde_json::to_value(&fixture.app.inspection).unwrap();
            assert!(fixture.run(commit).is_err(), "commit={commit}, change={change}");
            assert_eq!(fixture.app.document_revision, revision);
            assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), workspace);
            assert!(fixture.app.measurements.is_empty());
            assert_eq!(fixture.app.inspection_observation_ui.draft.as_ref().unwrap().context.revision, prepared_revision);
        }
    }
}

#[test]
fn native_observation_invalid_draft_empty_contour_and_commit_without_preview_are_noops() {
    for invalid in 0..5 {
        let mut fixture = Fixture::new();
        fixture.prepare_direct();
        let draft = fixture.app.inspection_observation_ui.draft.as_mut().unwrap();
        match invalid {
            0 => draft.values[0] = "NaN".into(),
            1 => draft.values[2] = "-2".into(),
            2 => draft.options.sample_count = 300,
            3 => draft.values = ["8", "8", "3", "0", "180"].map(str::to_owned),
            _ => {}
        }
        let revision = fixture.app.document_revision;
        let workspace = serde_json::to_value(&fixture.app.inspection).unwrap();
        assert!(fixture.run(invalid == 4).is_err(), "invalid={invalid}");
        assert_eq!(fixture.app.document_revision, revision);
        assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), workspace);
        assert!(fixture.app.measurements.is_empty());
    }
}

#[test]
fn native_observation_group_change_and_duplicate_record_require_new_preparation() {
    let mut fixture = Fixture::new();
    fixture.prepare_via_widgets();
    fixture.click("preview");
    let revision = fixture.app.document_revision;
    fixture.replace("group", "Groupe suivant");
    assert!(fixture.app.inspection_observation_ui.draft.is_none());
    assert_eq!(fixture.app.document_revision, revision);
    assert!(fixture.app.measurements.is_empty());
    fixture.prepare_direct();
    fixture.run(false).unwrap();
    fixture.run(true).unwrap();
    let revision = fixture.app.document_revision;
    assert!(fixture.run(true).is_err());
    assert_eq!(fixture.app.document_revision, revision);
    assert_eq!(fixture.app.measurements.len(), 1);
}

#[test]
fn native_line_and_arc_modes_keep_endpoint_limit_and_line_commit_refusal() {
    let mut arc = Fixture::new();
    arc.app.inspection.plan.as_mut().unwrap().characteristics[0].kind = crate::dper_drawing::DrawingDimensionKind::Radius;
    arc.prepare_via_widgets();
    arc.click_at(arc.text_rect("Arc").center());
    arc.click("preview");
    let record = arc.app.inspection.records.last().expect("Observed arc preview");
    assert!(matches!(record.result.geometry, InspectionGeometry::Arc { .. }));
    assert!(!record.result.evidence.endpoint_extent_verified);
    assert!(arc.app.measurements.is_empty());
    arc.click("record");
    assert_eq!(arc.app.measurements.len(), 1, "Radius can be recorded without claiming arc extent");

    let mut line = Fixture::new();
    line.app.inspection.plan.as_mut().unwrap().characteristics[0].kind = crate::dper_drawing::DrawingDimensionKind::Linear;
    let image = image::RgbImage::from_fn(128, 128, |_, y| image::Rgb([if y < 64 { 20 } else { 220 }; 3]));
    line.app.image.as_mut().unwrap().rgb = image.as_raw().clone();
    line.app.inspection.registration_fingerprint = Some(dper_inspection::fingerprint(&image));
    line.select_feature();
    line.click("prepare");
    line.click_at(line.text_rect("Droite").center());
    for (key, value) in [("Départ X (plan)", "16"), ("Départ Y (plan)", "32"), ("Fin X (plan)", "48"), ("Fin Y (plan)", "32")] {
        line.replace(key, value);
    }
    line.click("preview");
    let record = line.app.inspection.records.last().expect("Observed support-line preview");
    assert!(matches!(record.result.geometry, InspectionGeometry::Line { .. }));
    assert!(!record.result.evidence.endpoint_extent_verified);
    assert!(record.value_mm.is_none(), "Support is not an observed endpoint-to-endpoint length");
    let revision = line.app.document_revision;
    line.click("record");
    assert_eq!(line.app.document_revision, revision);
    assert!(line.app.measurements.is_empty());
    assert!(line.app.inspection_observation_ui.status.contains("extrémités"));
}

#[test]
fn native_result_render_suspends_changed_pixels_or_same_size_pdf_without_revision() {
    for commit in [false, true] {
        for change_pdf in [false, true] {
            let mut fixture = Fixture::new();
            fixture.prepare_direct();
            fixture.run(false).unwrap();
            if commit { fixture.run(true).unwrap(); }
            let revision = fixture.app.document_revision;
            let original = serde_json::to_value(&fixture.app.inspection_observation_ui.result).unwrap();
            let prepared = fixture.app.inspection_observation_ui.draft.as_ref()
                .map(|draft| draft.context.revision);
            if change_pdf {
                let length = std::fs::metadata(&fixture.path).unwrap().len();
                std::fs::write(&fixture.path, vec![b'x'; length as usize]).unwrap();
            } else {
                fixture.app.image.as_mut().unwrap().rgb[0] = 0;
            }
            fixture.settle();
            let reason = if change_pdf { "drawing_source_changed" } else { "source_pixels_changed" };
            fixture.text_rect(crate::dper_metrology::reason_label(reason));
            let current = fixture.app.current_native_observation_result(
                fixture.app.inspection_observation_ui.result.as_ref().unwrap());
            let shared = fixture.app.handle_inspection_request(InspectionRequest::GetState).unwrap();
            let shared: crate::dper_inspection::InspectionWorkspace = serde_json::from_value(shared["inspection"].clone()).unwrap();
            assert_eq!(shared.records.last().unwrap().outcome, current.outcome,
                "Native display and MCP GetState must use the same suspension rules");
            assert_ne!(current.outcome.geometry_state, crate::dper_metrology::GeometryState::Validated);
            assert_eq!(current.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Indeterminate);
            fixture.text_rect(&format!("Contours : {} · conformité : {}",
                current.outcome.geometry_state.label(), current.outcome.conformity_status.label()));
            assert!(!fixture.shapes.iter().any(|clipped| match &clipped.shape {
                egui::epaint::Shape::Text(text) => text.galley.text().contains("Contours : vérifiée par logiciel"),
                _ => false,
            }), "The result must not keep its previous software-validation label");
            assert_eq!(fixture.app.document_revision, revision, "Rendering must be read-only");
            assert_eq!(serde_json::to_value(&fixture.app.inspection_observation_ui.result).unwrap(), original,
                "Reassess a display copy; preserve the original operation evidence");
            assert_eq!(fixture.app.inspection_observation_ui.draft.as_ref().map(|draft| draft.context.revision), prepared,
                "Rendering must not silently reprepare a draft");
        }
    }
}

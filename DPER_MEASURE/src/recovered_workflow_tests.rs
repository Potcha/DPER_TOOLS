//! Real pointer input against the recovered original workflow timeline.
use super::*;

struct WorkflowFixture {
    app: DperMeasureApp,
    ctx: egui::Context,
    shapes: Vec<egui::epaint::ClippedShape>,
    time: f64,
    source: Option<PathBuf>,
    registration_editor: bool,
    workspace_ui: bool,
}

impl WorkflowFixture {
    fn new() -> Self {
        let ctx = egui::Context::default();
        let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
        app.image = Some(ImageData { width: 100, height: 100, rgb: vec![128; 30_000], source: "workflow.png".into() });
        app.reference_document = Some(serde_json::from_value(serde_json::json!({})).unwrap());
        app.active_ribbon_tab = RibbonTab::Camera;
        app.mode = Tool::Coordinate;
        app.selection_mode = true;
        app.right_panel_annotations = true;
        let mut result = Self { app, ctx, shapes: Vec::new(), time: 0.0, source: None, registration_editor: false, workspace_ui: false };
        result.settle();
        result
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0 / 60.0;
        let app = &mut self.app;
        let registration_editor = self.registration_editor;
        let workspace_ui = self.workspace_ui;
        self.shapes = self.ctx.run(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1672.0, 1100.0))),
            time: Some(self.time), focused: true, events, ..Default::default()
        }, |ctx| {
            app.begin_registration_photo_pick_frame(ctx);
            app.recovered_chrome(ctx);
            if workspace_ui {
                app.handle_workspace_shortcuts(ctx);
                app.recovered_results_panel(ctx);
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                if registration_editor { app.registration_editor_panel(ui); }
                if workspace_ui { app.central_image(ui); }
            });
            app.patterns_window(ctx);
        }).shapes;
    }

    fn settle(&mut self) { for _ in 0..3 { self.frame(Vec::new()); } }

    fn labels(&self, text: &str) -> Vec<Pos2> {
        fn collect(shape: &egui::epaint::Shape, text: &str, found: &mut Vec<Pos2>) {
            match shape {
                egui::epaint::Shape::Text(label) if label.galley.text() == text =>
                    found.push(label.galley.rect.translate(label.pos.to_vec2()).center()),
                egui::epaint::Shape::Vec(shapes) => for shape in shapes { collect(shape, text, found); },
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &self.shapes { collect(&clipped.shape, text, &mut found); }
        found.sort_by(|a, b| a.y.total_cmp(&b.y));
        found
    }

    fn click(&mut self, point: Pos2) {
        for pressed in [true, false] {
            self.frame(vec![egui::Event::PointerMoved(point), egui::Event::PointerButton {
                pos: point, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::default()
            }]);
        }
        self.settle();
    }

    fn click_label(&mut self, text: &str, last: bool) {
        let matches = self.labels(text);
        let point = if last { matches.last() } else { matches.first() }
            .copied().unwrap_or_else(|| panic!("Missing rendered label: {text}"));
        self.click(point);
    }

    fn press_key(&mut self, key: egui::Key) {
        for pressed in [true, false] {
            self.frame(vec![egui::Event::Key {
                key, physical_key: Some(key), pressed, repeat: false, modifiers: egui::Modifiers::NONE,
            }]);
        }
        self.settle();
    }

    fn prepare_photo_pick(&mut self) {
        let source = std::env::temp_dir().join(format!("{}.pdf", new_id("workflow-source")));
        std::fs::write(&source, b"fingerprint-only registration fixture").unwrap();
        let mut plan = dper_drawing::parse_text_plan("Diametre 20 mm", "workflow.pdf");
        plan.source_file = Some(source.display().to_string());
        plan.source_fingerprint = Some(dper_source::fingerprint_file(&source).unwrap());
        plan.page_geometry = vec![dper_pdf_layout::PdfPageGeometry { page: 1, width_pt: 100.0, height_pt: 100.0 }];
        self.app.install_inspection_plan(plan);
        self.source = Some(source);
        self.registration_editor = true;
        self.settle();
        self.click_label("Nouveau brouillon (effacer la saisie)", false);
        self.click_label("Choisir sur la photo", false);
        assert!(self.app.registration_photo_pick_active());
    }
}

#[test]
fn llt_man_005_annotation_session_interactions() {
    let mut f = WorkflowFixture::new();
    f.workspace_ui = true;
    f.app.refresh_display_texture(&f.ctx);
    f.settle();

    f.click_label("Annotation / commentaire", false);
    let viewport = f.ctx.data(|data| data.get_temp::<Rect>(
        egui::Id::new("measurement-canvas-viewport"))).unwrap();
    let anchor = viewport.center();
    f.frame(vec![egui::Event::PointerMoved(anchor)]);
    assert!(f.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Circle(dot) if dot.center == anchor)
    }));
    f.click(anchor);
    assert!(f.app.annotations.is_empty());
    assert!(!f.labels("Commentaire").is_empty());
    f.frame(vec![egui::Event::Text("Premier commentaire".into())]);
    assert_eq!(f.app.annotation_text, "Premier commentaire");
    f.press_key(egui::Key::Enter);
    assert_eq!(f.app.annotations[0].text, "Premier commentaire");
    assert!(f.app.annotation_mode);
    assert!(f.app.annotation_points.is_empty());
    assert!(matches!(f.app.annotations[0].target,
        AnnotationTarget::Point { point } if point.dist(Point { x: 50.0, y: 50.0 }) < 1e-4));

    f.click(anchor);
    f.frame(vec![egui::Event::Text("Second commentaire".into())]);
    f.click(anchor - Vec2::new(45.0, 45.0));
    assert_eq!(f.app.annotations.len(), 2);
    assert_eq!(f.app.annotations[1].text, "Second commentaire");
    assert!(f.app.annotation_mode);
    f.click_label("OK", false);
    assert!(!f.app.annotation_mode);
    assert!(f.app.selection_mode);
    f.click(anchor);
    f.press_key(egui::Key::Delete);
    assert_eq!(f.app.annotations.len(), 1);
    f.settle();
    f.click_label("×", true);
    assert!(f.app.annotations.is_empty());

    f.click_label("Annotation / commentaire", false);
    f.click(anchor);
    f.frame(vec![egui::Event::Text("Enregistré par OK".into())]);
    f.click_label("OK", false);
    assert_eq!(f.app.annotations[0].text, "Enregistré par OK");
    assert!(!f.app.annotation_mode);

    f.click_label("Annotation / commentaire", false);
    f.click(anchor);
    f.click(anchor - Vec2::new(45.0, 45.0));
    assert_eq!(f.app.annotations.len(), 1);
    assert!(f.app.annotation_mode);
    assert!(f.app.annotation_draft_target().is_some());
    f.frame(vec![egui::Event::Text("Brouillon à abandonner".into())]);
    assert_eq!(f.app.annotation_text, "Brouillon à abandonner");
    f.press_key(egui::Key::Escape);
    assert!(!f.app.annotation_mode);
    assert!(f.app.annotation_text.is_empty());
    assert_eq!(f.app.annotations.len(), 1);

    let output = tempfile::tempdir().unwrap();
    let project = output.path().join("annotations.dperproj");
    f.app.save_project_to_path(&project, false).unwrap();
    let mut reopened = DperMeasureApp::new_with_services(egui::Context::default(), false);
    reopened.open_project_path(&f.ctx, &project).unwrap();
    assert_eq!(reopened.annotations.len(), 1);
    assert_eq!(reopened.annotations[0].text, "Enregistré par OK");
    assert!(matches!(reopened.annotations[0].target,
        AnnotationTarget::Point { point } if point.dist(Point { x: 50.0, y: 50.0 }) < 1e-4));
}

#[test]
fn mirror_axes_are_validated_and_existing_constructions_remain_dependencies() {
    let mut fixture = WorkflowFixture::new();
    let make_line = |id, label, points, is_construction| -> Measurement {
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": Tool::Distance, "points": points,
            "label": label, "capture_id": fixture.app.capture_id, "is_construction": is_construction
        })).unwrap()
    };
    let source = make_line(1, "Source", [Point { x: 15.0, y: 25.0 }, Point { x: 30.0, y: 25.0 }], false);
    let axis = make_line(2, "Guide", [Point { x: 50.0, y: 10.0 }, Point { x: 50.0, y: 90.0 }], true);
    fixture.app.measurements = vec![source, axis];
    fixture.app.next_id = 3;
    fixture.app.set_measurement_selection(Some(1));
    fixture.app.open_pattern(patterns::PatternKind::Mirror);
    fixture.settle();
    fixture.click_label("OK", false);
    assert_eq!(fixture.app.measurements.len(), 2);

    fixture.click_label("Tracer l’axe par deux clics", false);
    let collapsed = Point { x: 45.0, y: 10.0 };
    fixture.app.on_click(collapsed);
    fixture.app.on_click(collapsed);
    fixture.settle();
    assert!(fixture.app.live_pattern_preview().is_empty());
    fixture.click_label("OK", false);
    assert_eq!(fixture.app.measurements.len(), 2);

    fixture.click_label("Tracer l’axe par deux clics", false);
    fixture.app.on_click(Point { x: 90.0, y: 10.0 });
    fixture.app.on_click(Point { x: 90.0, y: 90.0 });
    fixture.settle();
    assert!(fixture.app.live_pattern_preview()[0].points[0].x > 100.0);
    fixture.click_label("OK", false);
    assert_eq!(fixture.app.measurements.len(), 2);

    fixture.click_label("Tracer l’axe par deux clics", false);
    fixture.app.on_click(Point { x: 45.0, y: 10.0 });
    fixture.app.on_click(Point { x: 45.0, y: 90.0 });
    fixture.settle();
    assert_eq!(fixture.app.live_pattern_preview()[0].points[0], Point { x: 75.0, y: 25.0 });
    assert_eq!(fixture.app.measurements.len(), 2);

    fixture.click_label("Axe tracé", false);
    fixture.click_label("Construction · Guide", false);
    assert_eq!(fixture.app.live_pattern_preview()[0].points[0], Point { x: 85.0, y: 25.0 });
    fixture.click_label("OK", false);
    let copy = &fixture.app.measurements[2];
    assert_eq!(copy.points, [Point { x: 85.0, y: 25.0 }, Point { x: 70.0, y: 25.0 }]);
    assert_eq!(copy.source_measurement_ids, [1, 2]);
    assert!(fixture.app.undo_measurement_action());
    assert_eq!(fixture.app.measurements.len(), 2);
}

#[test]
fn mirror_preview_tracks_source_drag_and_uses_exact_axis_orientation() {
    let mut f = WorkflowFixture::new();
    f.app.measurements.push(serde_json::from_value(serde_json::json!({
        "id": 1, "kind": Tool::Distance, "label": "Source",
        "points": [{"x": 10.0, "y": 25.0}, {"x": 40.0, "y": 25.0}],
        "capture_id": f.app.capture_id, "is_construction": true
    })).unwrap());
    f.app.next_id = 2;
    f.app.set_measurement_selection(Some(1));
    f.app.selection_mode = false;
    f.app.open_pattern(patterns::PatternKind::Mirror);
    f.settle();
    f.click_label("Horizontal", false);
    f.click_label("Tracer l’axe par deux clics", false);
    f.app.on_click(Point { x: 50.0, y: 50.0 });
    f.app.on_click(Point { x: 90.0, y: 49.0 });
    f.settle();
    assert_eq!(f.app.live_pattern_preview()[0].points,
        [Point { x: 10.0, y: 75.0 }, Point { x: 40.0, y: 75.0 }]);
    assert!(f.app.selection_mode);

    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
    let painter = f.ctx.layer_painter(egui::LayerId::background());
    f.app.begin_pattern_source_edit(egui::pos2(100.0, 100.0), rect, 100.0, 100.0, &painter);
    assert!(f.app.measurement_gesture_active());
    f.app.update_point_edit(egui::pos2(100.0, 50.0), rect, 100.0, 100.0);
    f.settle();
    assert_eq!(f.app.live_pattern_preview()[0].points,
        [Point { x: 10.0, y: 87.5 }, Point { x: 40.0, y: 87.5 }]);
    f.app.finish_point_edit(false);
    f.settle();
    assert_eq!(f.app.live_pattern_preview()[0].points[0].y, 87.5);

    f.click_label("Vertical", false);
    assert_eq!(f.app.live_pattern_preview()[0].points,
        [Point { x: 90.0, y: 12.5 }, Point { x: 60.0, y: 12.5 }]);
    f.click_label("OK", false);
    assert_eq!(f.app.measurements[1].points,
        [Point { x: 90.0, y: 12.5 }, Point { x: 60.0, y: 12.5 }]);
}

#[test]
fn repetitions_keep_the_preview_when_a_mirrored_circle_is_repositioned() {
    for (kind, count) in [(patterns::PatternKind::Circular, 5), (patterns::PatternKind::Rectangular, 3)] {
        let mut f = WorkflowFixture::new();
        f.app.measurements.push(serde_json::from_value(serde_json::json!({
            "id": 1, "kind": Tool::Circle, "label": "Source",
            "points": [{"x": 45.0, "y": 35.0}, {"x": 47.0, "y": 35.0}],
            "capture_id": f.app.capture_id, "is_construction": true
        })).unwrap());
        f.app.next_id = 2;
        f.app.set_measurement_selection(Some(1));
        f.app.open_pattern(patterns::PatternKind::Mirror);
        f.settle();
        f.click_label("Tracer l’axe par deux clics", false);
        f.app.on_click(Point { x: 10.0, y: 50.0 });
        f.app.on_click(Point { x: 90.0, y: 50.0 });
        f.settle();
        f.click_label("OK", false);
        assert_eq!(f.app.measurements.len(), 2);
        assert!(!f.app.measurements[1].dependency_stale);

        f.app.open_pattern(kind);
        f.settle();
        assert_eq!(f.app.live_pattern_preview().len(), count);
        let before = f.app.live_pattern_preview()[0].points.clone();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        let painter = f.ctx.layer_painter(egui::LayerId::background());
        f.app.begin_pattern_source_edit(egui::pos2(180.0, 260.0), rect, 100.0, 100.0, &painter);
        assert!(f.app.measurement_gesture_active());
        f.app.update_point_edit(egui::pos2(200.0, 260.0), rect, 100.0, 100.0);
        f.settle();
        assert_ne!(f.app.live_pattern_preview()[0].points, before);
        let moved = f.app.live_pattern_preview()[0].points.clone();
        f.app.finish_point_edit(false);
        f.settle();
        assert!(f.app.measurements[1].dependency_stale);
        assert!(f.app.validate_dependency_sources(&f.app.capture_id, &[2]).is_err());
        assert_eq!(f.app.live_pattern_preview().len(), count);
        assert_eq!(f.app.live_pattern_preview()[0].points, moved);

        f.click_label("OK", false);
        assert_eq!(f.app.measurements.len(), 2 + count);
        for copy in &f.app.measurements[2..] {
            assert_eq!(copy.source_measurement_ids, [2]);
            assert!(copy.dependency_stale);
        }
    }
}

#[test]
fn circular_and_rectangular_previews_follow_source_geometry_before_creation() {
    for kind in [patterns::PatternKind::Circular, patterns::PatternKind::Rectangular] {
        let mut f = WorkflowFixture::new();
        let source_kind = if kind == patterns::PatternKind::Circular { Tool::Circle } else { Tool::Rectangle };
        let measurement: Measurement = serde_json::from_value(serde_json::json!({
            "id": 1, "kind": source_kind,
            "points": [{"x": 20.0, "y": 20.0}, {"x": 24.0, "y": 24.0}],
            "label": "Original", "capture_id": f.app.capture_id
        })).unwrap();
        f.app.measurements = vec![measurement.clone()];
        f.app.set_measurement_selection(Some(1));
        f.app.open_pattern(kind);
        f.settle();
        let first = f.app.live_pattern_preview()[0].points.clone();
        assert_eq!(f.app.measurements.len(), 1);

        let before = f.app.measurements[0].clone();
        let mut after = before.clone();
        after.points = vec![Point { x: 23.0, y: 25.0 }, Point { x: 30.0, y: 32.0 }];
        f.app.measurements[0] = after.clone();
        f.app.record_measurement_action(MeasurementHistoryAction::Edited { id: 1, before, after });
        f.settle();
        let updated = &f.app.live_pattern_preview()[0].points;
        assert_ne!(*updated, first);
        assert!((updated[0].dist(updated[1]) - 7.0 * 2.0_f64.sqrt()).abs() < 1e-9);
        assert_eq!(f.app.measurements.len(), 1);
    }
}

#[test]
fn llt_man_004_01_repositioned_copy_matches_confirmation_and_cancel_discards_preview() {
    let mut f = WorkflowFixture::new();
    f.workspace_ui = true;
    f.app.refresh_display_texture(&f.ctx);
    for (id, y) in [(1, 20.0), (2, 38.0), (3, 70.0)] {
        f.app.measurements.push(serde_json::from_value(serde_json::json!({
            "id": id, "kind": Tool::Distance, "label": format!("M{id}"),
            "points": [{"x": 10.0, "y": y}, {"x": 18.0, "y": y}],
            "capture_id": f.app.capture_id,
        })).unwrap());
    }
    f.app.next_id = 4;
    f.app.set_measurements_selection(vec![1, 2]);
    f.app.open_pattern(patterns::PatternKind::Rectangular);
    f.settle();
    assert_eq!(f.app.measurements.len(), 3);
    assert_eq!(f.app.live_pattern_preview().len(), 6);
    assert!(f.app.live_pattern_preview().iter().all(|copy| copy.source_measurement_ids != [3]));
    f.click_label("Annuler", false);
    assert!(f.app.pattern_dialog.is_none());
    assert_eq!(f.app.measurements.len(), 3);

    f.app.open_pattern(patterns::PatternKind::Rectangular);
    f.settle();
    let before = f.app.live_pattern_preview()[0].points.clone();
    let viewport = f.ctx.data(|data| data.get_temp::<Rect>(
        egui::Id::new("measurement-canvas-viewport"))).unwrap();
    let image_rect = Rect::from_center_size(viewport.center(), Vec2::splat(viewport.size().min_elem()));
    let anchor = Point { x: (before[0].x + before[1].x) / 2.0, y: before[0].y };
    let start = DperMeasureApp::point_to_screen(anchor, image_rect, 100.0, 100.0);
    let end = start + Vec2::new(18.0, -22.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos, pressed, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE,
    };
    f.frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
    f.frame(vec![egui::Event::PointerMoved(end)]);
    f.frame(vec![button(end, false)]);
    f.settle();
    let adjusted = f.app.live_pattern_preview()[0].points.clone();
    assert_ne!(adjusted, before);
    assert_eq!(f.app.measurements.len(), 3);
    f.click_label("OK", false);
    assert_eq!(f.app.measurements[3].points, adjusted);
    assert_eq!(f.app.measurements.len(), 9);
}

impl Drop for WorkflowFixture {
    fn drop(&mut self) { if let Some(source) = &self.source { let _ = std::fs::remove_file(source); } }
}

#[test]
fn recovered_acquisition_keeps_stream_live_and_rejects_freezing_measurement_actions() {
    let mut f = WorkflowFixture::new();
    f.app.running = true;
    f.app.paused = false;
    f.app.timelapse_capture = Some(TimeLapseCapture {
        folder: PathBuf::from("unused-acquisition-fixture"), interval_seconds: 1,
        next_frame_at: Instant::now() + Duration::from_secs(60), started: Instant::now(),
        frame_limit: 2, frames_seen: 0, best_frame: None, best_sharpness: f64::NEG_INFINITY,
        manifest: TimeLapseManifest {
            format_version: 1, status: "test".into(), device: String::new(), device_id: String::new(),
            optical_setup: String::new(), capture_id: f.app.capture_id.clone(), resolution: [100, 100],
            camera_frame_rate: 30, camera_controls: Vec::new(), interval_seconds: 1, expected_frame_count: 2,
            started_at_unix_secs: 0, calibration_id: None, scale_x_mm_per_pixel: None,
            scale_y_mm_per_pixel: None, expanded_uncertainty_mm: None, protocol_id: None,
            protocol_version: None, quality_thresholds: Default::default(),
            display_correction: Default::default(), video_file: None, frames: Vec::new(),
        },
    });
    f.settle();
    let revision = f.app.document_revision;
    f.click_label("Étalonnage", true);
    assert!(!f.app.calibration_mode);
    f.app.start_calibration_mode();
    f.app.choose_measurement_tool(Tool::Distance);
    f.app.open_pattern(patterns::PatternKind::Rectangular);
    f.app.on_click(Point { x: 25.0, y: 25.0 });
    assert_eq!(f.app.mode, Tool::Coordinate);
    assert!(f.app.pending.is_empty());
    assert!(f.app.pattern_dialog.is_none());
    assert!(f.app.measurements.is_empty());
    assert!(f.app.running && !f.app.paused);
    assert!(f.app.timelapse_capture.is_some());
    assert_eq!(f.app.document_revision, revision);
}

#[test]
fn recovered_workflow_pointer_calibration_step_arms_original_calibration_mode() {
    let mut f = WorkflowFixture::new();
    f.click_label("Étalonnage", true);
    assert!(f.app.active_ribbon_tab == RibbonTab::Calibration);
    assert!(f.app.calibration_mode);
    assert!(!f.app.selection_mode);
    assert!(!f.app.right_panel_annotations);
    assert!(f.app.pending.is_empty());
    assert!(f.app.message.starts_with("Étalonnage"));
}

#[test]
fn recovered_workflow_pointer_measurement_step_activates_distance_tool() {
    let mut f = WorkflowFixture::new();
    f.click_label("Mesures", true);
    assert!(f.app.active_ribbon_tab == RibbonTab::Measurements);
    assert_eq!(f.app.mode, Tool::Distance);
    assert!(!f.app.selection_mode);
    assert!(!f.app.calibration_mode);
    assert!(!f.app.right_panel_annotations);
    assert!(f.app.message.contains("Outil Distance actif"));
}

#[test]
fn recovered_workflow_pointer_control_step_opens_existing_reference_without_picker() {
    let mut f = WorkflowFixture::new();
    let reference = serde_json::to_value(&f.app.reference_document).unwrap();
    f.click_label("Contrôle", true);
    assert!(f.app.active_ribbon_tab == RibbonTab::Inspection);
    assert!(f.app.workflow_open_reference_panel);
    assert!(!f.app.right_panel_annotations);
    assert_eq!(serde_json::to_value(&f.app.reference_document).unwrap(), reference);
    assert!(f.app.message.contains("Plan de référence ouvert"));
}

#[test]
fn recovered_workflow_pointer_capture_step_only_opens_original_menu() {
    let mut f = WorkflowFixture::new();
    f.click_label("Capture / image / projet", true);
    assert!(!f.labels("Démarrer le flux USB").is_empty());
    assert!(!f.labels("Ouvrir une image…").is_empty());
    assert!(!f.labels("Ouvrir un projet…").is_empty());
    assert!(!f.app.running);
    assert!(f.app.camera_task.is_none());
    assert_eq!(f.app.image.as_ref().unwrap().source, "workflow.png");
}

#[test]
fn recovered_workflow_pointer_steps_refuse_pending_editing_and_photo_pick_ownership() {
    for blocked in 0..3 {
        let mut f = WorkflowFixture::new();
        match blocked {
            0 => f.app.pending.push(Point { x: 20.0, y: 30.0 }),
            1 => f.app.annotation_mode = true,
            _ => f.prepare_photo_pick(),
        }
        f.settle();
        let pending = f.app.pending.clone();
        let revision = f.app.document_revision;
        let message = f.app.message.clone();
        for label in ["Étalonnage", "Mesures", "Contrôle"] {
            f.click_label(label, true);
            assert!(f.app.active_ribbon_tab == RibbonTab::Camera, "blocked case {blocked}, step {label}");
            assert_eq!(f.app.mode, Tool::Coordinate);
            assert!(!f.app.calibration_mode);
            assert!(!f.app.workflow_open_reference_panel);
            assert_eq!(f.app.pending, pending);
            assert_eq!(f.app.document_revision, revision);
            assert_eq!(f.app.message, message);
        }
        if blocked == 1 { assert!(f.app.annotation_mode); }
        if blocked == 2 { assert!(f.app.registration_photo_pick_active()); }
    }
}

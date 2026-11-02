// Included in registration_ui::tests for the isolated Fixture and private draft.
// Every photo gesture traverses central_image, including its ordinary tools.
struct PhotoPickInteraction {
    fixture: Fixture,
    requests: std::sync::mpsc::Sender<crate::mcp_server::UiRequest>,
    shapes: Vec<egui::epaint::ClippedShape>,
    viewport: egui::Rect,
    time: f64,
}

impl PhotoPickInteraction {
    fn new() -> Self {
        let mut fixture = Fixture::new();
        fixture.ctx.set_pixels_per_point(2.0);
        let app = &mut fixture.app;
        // This image passes the ordinary measurement gate, so an accidental
        // release really would create a coordinate measurement (see control).
        let image = app.image.as_mut().unwrap();
        for y in 0..100 {
            for x in 0..100 {
                let value = if (x / 8 + y / 8) % 2 == 0 { 60 } else { 196 };
                let offset = (y * 100 + x) * 3;
                image.rgb[offset..offset + 3].fill(value);
            }
        }
        app.quality_assessment = Some(crate::quality::assess_image_quality(
            &image.rgb, image.width, image.height, &app.quality_thresholds,
        ).unwrap());
        assert!(app.quality_assessment.as_ref().unwrap().is_exploitable);
        app.texture = Some(fixture.ctx.load_texture("photo-pick-test",
            egui::ColorImage::from_rgb([100, 100], &image.rgb), egui::TextureOptions::LINEAR));
        app.fit_view = false;
        app.zoom = 4.0;
        app.pan = egui::vec2(41.0, -27.0);
        app.mode = crate::Tool::Coordinate;
        app.selection_mode = false;
        app.edge_snap_enabled = false;
        let (requests, receiver) = std::sync::mpsc::channel();
        app.mcp_requests = receiver;
        fixture.prepare_valid();
        let mut interaction = Self {
            fixture, requests, shapes: Vec::new(), viewport: egui::Rect::NOTHING, time: 0.0,
        };
        interaction.settle();
        assert_eq!(interaction.fixture.ctx.pixels_per_point(), 2.0);
        interaction
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1500.0, 1000.0))),
            time: Some(self.time), focused: true, events, ..Default::default()
        };
        let app = &mut self.fixture.app;
        let viewport = &mut self.viewport;
        self.shapes = self.fixture.ctx.run(input, |ctx| {
            // Same input ordering as App::update, without camera or window IO.
            app.begin_registration_photo_pick_frame(ctx);
            app.registration_photo_pick_escape(ctx);
            app.handle_mcp_requests(ctx);
            egui::SidePanel::left("photo-test-editor").exact_width(800.0).show(ctx, |ui| {
                app.registration_editor_panel(ui);
            });
            egui::CentralPanel::default().show(ctx, |ui| {
                *viewport = ui.available_rect_before_wrap();
                app.central_image(ui);
            });
        }).shapes;
    }

    fn settle(&mut self) { for _ in 0..3 { self.frame(Vec::new()); } }

    fn label(&self, text: &str, row: usize) -> egui::Pos2 {
        fn collect(shape: &egui::epaint::Shape, text: &str, found: &mut Vec<egui::Pos2>) {
            match shape {
                egui::epaint::Shape::Text(label) if label.galley.text() == text => {
                    found.push(label.galley.rect.translate(label.pos.to_vec2()).center());
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes { collect(shape, text, found); }
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &self.shapes { collect(&clipped.shape, text, &mut found); }
        found.sort_by(|a, b| a.y.total_cmp(&b.y));
        *found.get(row).unwrap_or_else(|| panic!("Missing rendered row {row}: {text}"))
    }

    fn image_rect(&self) -> egui::Rect {
        let app = &self.fixture.app;
        let scale = app.zoom / self.fixture.ctx.pixels_per_point();
        egui::Rect::from_center_size(self.viewport.center() + app.pan, egui::vec2(100.0, 100.0) * scale)
    }

    fn photo(&self, x: f64, y: f64) -> egui::Pos2 {
        DperMeasureApp::point_to_screen(Point { x, y }, self.image_rect(), 100.0, 100.0)
    }

    fn button(&mut self, position: egui::Pos2, button: egui::PointerButton, pressed: bool) {
        self.frame(vec![egui::Event::PointerMoved(position), egui::Event::PointerButton {
            pos: position, button, pressed, modifiers: egui::Modifiers::default(),
        }]);
    }

    fn click(&mut self, position: egui::Pos2) {
        self.button(position, egui::PointerButton::Primary, true);
        self.button(position, egui::PointerButton::Primary, false);
        self.settle();
    }

    fn arm(&mut self, row: usize) {
        self.click(self.label("Choisir sur la photo", row));
        assert_eq!(self.fixture.app.registration_ui.photo_pick.as_ref().unwrap().row, row);
    }

    fn escape(&mut self) {
        self.frame(vec![egui::Event::Key {
            key: egui::Key::Escape, physical_key: Some(egui::Key::Escape), pressed: true,
            repeat: false, modifiers: egui::Modifiers::default(),
        }]);
    }

    fn rows(&self) -> Vec<[String; 4]> {
        self.fixture.app.registration_ui.draft.as_ref().unwrap().rows.clone()
    }

    fn assert_no_canvas_action(&self) {
        let app = &self.fixture.app;
        assert!(app.pending.is_empty());
        assert!(app.measurements.is_empty());
        assert!(app.annotations.is_empty());
        assert!(app.annotation_points.is_empty());
        assert!(app.point_edit.is_none());
        assert!(app.inspection.registration.is_none());
    }
}

#[test]
fn pointer_photo_pick_fills_only_target_image_coordinates_at_hidpi_zoom_and_pan() {
    for row in 0..3 {
        let mut ui = PhotoPickInteraction::new();
        let rows = ui.rows();
        let calibration = serde_json::to_value(&ui.fixture.app.calibration).unwrap();
        let inspection = serde_json::to_value(&ui.fixture.app.inspection).unwrap();
        let revision = ui.fixture.app.document_revision;
        ui.arm(row);
        ui.click(ui.photo(37.25, 61.5));
        let picked = ui.rows();
        for index in 0..3 {
            assert_eq!(&picked[index][..2], &rows[index][..2]);
            if index != row { assert_eq!(picked[index], rows[index]); }
        }
        assert!((picked[row][2].parse::<f64>().unwrap() - 37.25).abs() < 0.001);
        assert!((picked[row][3].parse::<f64>().unwrap() - 61.5).abs() < 0.001);
        assert!(!ui.fixture.app.registration_photo_pick_active());
        assert_eq!(ui.fixture.app.document_revision, revision);
        assert_eq!(serde_json::to_value(&ui.fixture.app.inspection).unwrap(), inspection);
        assert_eq!(serde_json::to_value(&ui.fixture.app.calibration).unwrap(), calibration);
        assert_eq!(ui.fixture.app.mm_per_pixel, Some(0.125));
        ui.assert_no_canvas_action();
    }
}

#[test]
fn pointer_photo_pick_rejects_outside_exclusive_edges_drag_and_navigation() {
    for case in 0..8 {
        let mut ui = PhotoPickInteraction::new();
        ui.arm(1);
        let rows = ui.rows();
        let start = ui.photo(30.0, 40.0);
        match case {
            0 => ui.click(ui.photo(-1.0, 40.0)),
            1 => ui.click(ui.photo(100.0, 40.0)),
            2 => ui.click(ui.photo(30.0, 100.0)),
            3 => {
                ui.button(start, egui::PointerButton::Primary, true);
                let end = start + egui::vec2(15.0, 0.0);
                ui.frame(vec![egui::Event::PointerMoved(end)]);
                ui.button(end, egui::PointerButton::Primary, false);
            }
            4 => {
                ui.button(start, egui::PointerButton::Primary, true);
                ui.button(ui.photo(101.0, 40.0), egui::PointerButton::Primary, false);
            }
            5 => {
                ui.button(ui.viewport.right_top() + egui::vec2(5.0, 5.0), egui::PointerButton::Primary, true);
                ui.button(start, egui::PointerButton::Primary, false);
            }
            6 => {
                ui.button(start, egui::PointerButton::Primary, true);
                let zoom = ui.fixture.app.zoom;
                ui.frame(vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, 80.0),
                    modifiers: egui::Modifiers::default(),
                }]);
                assert!(ui.fixture.app.zoom > zoom, "Wheel navigation must remain available");
                ui.button(start, egui::PointerButton::Primary, false);
            }
            _ => {
                ui.button(start, egui::PointerButton::Primary, true);
                ui.button(start, egui::PointerButton::Middle, true);
                let pan = ui.fixture.app.pan;
                let end = start + egui::vec2(20.0, 12.0);
                ui.frame(vec![egui::Event::PointerMoved(end)]);
                assert_ne!(ui.fixture.app.pan, pan, "Middle-button navigation must remain available");
                ui.button(end, egui::PointerButton::Middle, false);
                ui.button(end, egui::PointerButton::Primary, false);
            }
        }
        ui.settle();
        assert_eq!(ui.rows(), rows, "Rejected gesture {case} changed a row");
        assert!(ui.fixture.app.registration_photo_pick_active(), "Gesture {case} disarmed the row");
        ui.assert_no_canvas_action();
        ui.click(ui.photo(42.0, 53.0));
        assert!(!ui.fixture.app.registration_photo_pick_active(), "Gesture {case} prevented a later valid click");
        assert!((ui.rows()[1][2].parse::<f64>().unwrap() - 42.0).abs() < 0.001);
    }
}

#[test]
fn pointer_photo_pick_escape_and_mcp_cancel_own_the_next_frame_release() {
    for mcp in [false, true] {
        let mut ui = PhotoPickInteraction::new();
        ui.arm(0);
        let rows = ui.rows();
        let position = ui.photo(32.0, 44.0);
        ui.button(position, egui::PointerButton::Primary, true);
        assert!(ui.fixture.app.mcp_mutation_blocked());
        if mcp {
            let (reply, mut response) = tokio::sync::oneshot::channel();
            ui.requests.send(crate::mcp_server::UiRequest::CancelActiveAction { reply }).unwrap();
            ui.frame(Vec::new());
            assert!(response.try_recv().unwrap().is_ok());
        } else { ui.escape(); }
        assert!(!ui.fixture.app.registration_photo_pick_active());
        assert!(ui.fixture.app.mcp_mutation_blocked());
        ui.button(position, egui::PointerButton::Primary, false);
        assert!(ui.fixture.app.mcp_mutation_blocked(), "Cancelled release must keep this frame owned");
        assert_eq!(ui.rows(), rows);
        ui.assert_no_canvas_action();
        ui.frame(Vec::new());
        assert!(!ui.fixture.app.mcp_mutation_blocked(), "Ownership must end after the release frame");
        // Positive control: the exact same ordinary canvas click now measures.
        ui.click(position);
        assert_eq!(ui.fixture.app.measurements.len(), 1);
    }
}

#[test]
fn pointer_photo_pick_refuses_changed_revision_capture_plan_source_and_pixels() {
    for change in 0..5 {
        let mut ui = PhotoPickInteraction::new();
        ui.arm(2);
        let rows = ui.rows();
        let position = ui.photo(32.0, 44.0);
        ui.button(position, egui::PointerButton::Primary, true);
        match change {
            0 => ui.fixture.app.document_revision += 1,
            1 => ui.fixture.app.capture_id = "replacement-capture".into(),
            2 => ui.fixture.app.inspection.plan.as_mut().unwrap().source_name = "replacement-plan".into(),
            3 => {
                // Keep both size and timestamp: acceptance must inspect bytes,
                // even when the inexpensive editor stale check sees no change.
                let metadata = std::fs::metadata(&ui.fixture.path).unwrap();
                std::fs::write(&ui.fixture.path, vec![b'x'; metadata.len() as usize]).unwrap();
                std::fs::File::options().write(true).open(&ui.fixture.path).unwrap().set_times(
                    std::fs::FileTimes::new().set_modified(metadata.modified().unwrap()),
                ).unwrap();
                assert!(!ui.fixture.app.native_registration_stale());
            }
            _ => ui.fixture.app.image.as_mut().unwrap().rgb[0] ^= 1,
        }
        let revision = ui.fixture.app.document_revision;
        let inspection = serde_json::to_value(&ui.fixture.app.inspection).unwrap();
        ui.button(position, egui::PointerButton::Primary, false);
        ui.settle();
        assert_eq!(ui.rows(), rows, "Changed context {change} accepted a photo point");
        assert!(!ui.fixture.app.registration_photo_pick_active());
        assert_eq!(ui.fixture.app.document_revision, revision);
        assert_eq!(serde_json::to_value(&ui.fixture.app.inspection).unwrap(), inspection);
        ui.assert_no_canvas_action();
        assert!(ui.fixture.app.arm_registration_photo_pick(0).is_err());
    }
}

#[test]
fn pointer_photo_pick_rearm_and_row_removal_never_retarget_an_old_gesture() {
    let mut ui = PhotoPickInteraction::new();
    ui.arm(0);
    ui.arm(2);
    let rows = ui.rows();
    ui.click(ui.photo(31.0, 47.0));
    assert_eq!(ui.rows()[0], rows[0]);
    assert_eq!(ui.rows()[1], rows[1]);
    assert_eq!(ui.rows()[2][2], "31");
    ui.arm(2);
    let before_remove = ui.rows();
    ui.click(ui.label("Retirer", 0));
    assert!(!ui.fixture.app.registration_photo_pick_active());
    assert_eq!(ui.rows(), before_remove[1..]);
    assert!(ui.fixture.app.arm_registration_photo_pick(2).is_err());
    ui.assert_no_canvas_action();
}

#[test]
fn pointer_photo_pick_rearm_then_cancel_preserves_previous_held_release() {
    let mut ui = PhotoPickInteraction::new();
    ui.arm(0);
    let rows = ui.rows();
    let position = ui.photo(32.0, 44.0);
    ui.button(position, egui::PointerButton::Primary, true);
    // Defensive programmatic rearm while a photo press is still held.
    ui.fixture.app.arm_registration_photo_pick(1).unwrap();
    ui.fixture.app.cancel_registration_photo_pick();
    ui.button(position, egui::PointerButton::Primary, false);
    assert!(ui.fixture.app.mcp_mutation_blocked());
    assert_eq!(ui.rows(), rows);
    ui.assert_no_canvas_action();
    ui.frame(Vec::new());
    assert!(!ui.fixture.app.mcp_mutation_blocked());
}

#[test]
fn pointer_photo_pick_context_removal_drains_release_without_a_drawable_image() {
    let mut ui = PhotoPickInteraction::new();
    ui.arm(0);
    let position = ui.photo(32.0, 44.0);
    ui.button(position, egui::PointerButton::Primary, true);
    ui.fixture.app.image = None;
    ui.fixture.app.texture = None;
    ui.frame(Vec::new());
    assert!(!ui.fixture.app.registration_photo_pick_active());
    assert!(ui.fixture.app.mcp_mutation_blocked());
    ui.button(position, egui::PointerButton::Primary, false);
    assert!(ui.fixture.app.mcp_mutation_blocked());
    ui.frame(Vec::new());
    assert!(!ui.fixture.app.mcp_mutation_blocked());
    ui.assert_no_canvas_action();
}

#[test]
fn pointer_photo_pick_blocks_queued_mcp_mutations_while_armed_and_draining() {
    fn rejected<T>(
        ui: &mut PhotoPickInteraction,
        build: impl FnOnce(tokio::sync::oneshot::Sender<Result<T, String>>) -> crate::mcp_server::UiRequest,
    ) {
        let before = serde_json::to_value(ui.fixture.app.mcp_context()).unwrap();
        let (reply, mut response) = tokio::sync::oneshot::channel();
        ui.requests.send(build(reply)).unwrap();
        ui.frame(Vec::new());
        match response.try_recv().expect("Queued mutation did not receive a reply") {
            Err(error) => assert_eq!(error, "Terminez ou annulez le choix du repère photo avant cette action MCP."),
            Ok(_) => panic!("Concurrent MCP mutation was accepted"),
        }
        assert_eq!(serde_json::to_value(ui.fixture.app.mcp_context()).unwrap(), before);
        assert!(ui.fixture.app.mcp_mutation_blocked());
        ui.assert_no_canvas_action();
    }

    let mut ui = PhotoPickInteraction::new();
    ui.arm(1);
    let rows = ui.rows();
    let position = ui.photo(32.0, 44.0);
    ui.button(position, egui::PointerButton::Primary, true);
    // Existing source paths also make unexpected IO visible without allowing a
    // failed guard to overwrite anything or leave files behind in the test.
    let path = ui.fixture.path.clone();
    let source_bytes = std::fs::read(&path).unwrap();
    for draining in [false, true] {
        if draining {
            let (reply, mut response) = tokio::sync::oneshot::channel();
            ui.requests.send(crate::mcp_server::UiRequest::CancelActiveAction { reply }).unwrap();
            ui.frame(Vec::new());
            assert!(response.try_recv().unwrap().is_ok());
            assert!(!ui.fixture.app.registration_photo_pick_active());
        }
        let revision = ui.fixture.app.document_revision;
        rejected(&mut ui, |reply| crate::mcp_server::UiRequest::StartCamera { index: usize::MAX, reply });
        rejected(&mut ui, |reply| crate::mcp_server::UiRequest::StreamControl {
            action: "pause".into(), expected_revision: Some(revision), reply,
        });
        rejected(&mut ui, |reply| crate::mcp_server::UiRequest::OpenImage { path: path.clone(), reply });
        rejected(&mut ui, |reply| crate::mcp_server::UiRequest::OpenProject { path: path.clone(), reply });
        rejected(&mut ui, |reply| crate::mcp_server::UiRequest::SetWorkspace {
            optical_setup: Some("concurrent optical setup".into()), measurement_tool: Some("distance".into()),
            show_grid: Some(true), show_center_guides: Some(true), zoom_percent: Some(800.0), reply,
        });
        rejected(&mut ui, |reply| crate::mcp_server::UiRequest::SaveProject {
            path: path.clone(), overwrite: false, reply,
        });
        rejected(&mut ui, |reply| crate::mcp_server::UiRequest::CapturePhoto {
            path: path.clone(), overwrite: false, reply,
        });
        rejected(&mut ui, |reply| crate::mcp_server::UiRequest::PrepareReport { folder: None, reply });
        assert_eq!(ui.fixture.app.registration_photo_pick_active(), !draining);
        assert_eq!(ui.rows(), rows);
        assert_eq!(std::fs::read(&path).unwrap(), source_bytes);
    }
    ui.button(position, egui::PointerButton::Primary, false);
    assert!(ui.fixture.app.mcp_mutation_blocked());
    ui.assert_no_canvas_action();
    ui.frame(Vec::new());
    assert!(!ui.fixture.app.mcp_mutation_blocked());
}

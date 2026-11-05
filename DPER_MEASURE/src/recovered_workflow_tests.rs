//! Real pointer input against the recovered original workflow timeline.
use super::*;

struct WorkflowFixture {
    app: DperMeasureApp,
    ctx: egui::Context,
    shapes: Vec<egui::epaint::ClippedShape>,
    time: f64,
    source: Option<PathBuf>,
    registration_editor: bool,
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
        let mut result = Self { app, ctx, shapes: Vec::new(), time: 0.0, source: None, registration_editor: false };
        result.settle();
        result
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0 / 60.0;
        let app = &mut self.app;
        let registration_editor = self.registration_editor;
        self.shapes = self.ctx.run(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1672.0, 1100.0))),
            time: Some(self.time), focused: true, events, ..Default::default()
        }, |ctx| {
            app.begin_registration_photo_pick_frame(ctx);
            app.recovered_chrome(ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                if registration_editor { app.registration_editor_panel(ui); }
            });
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

impl Drop for WorkflowFixture {
    fn drop(&mut self) { if let Some(source) = &self.source { let _ = std::fs::remove_file(source); } }
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

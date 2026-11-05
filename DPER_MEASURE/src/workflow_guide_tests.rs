use super::*;

struct Guide {
    app: DperMeasureApp,
    ctx: egui::Context,
    effects: Vec<WorkflowExternal>,
    time: f64,
    directory: PathBuf,
}
impl Guide {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let ctx = egui::Context::default();
        let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join(format!(
            "workflow-guide-test-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        fs::create_dir_all(&directory).unwrap();
        app.directory = directory.clone();
        let mut guide = Self { app, ctx, effects: Vec::new(), time: 0.0, directory };
        guide.settle();
        guide
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0 / 60.0;
        let app = &mut self.app;
        let effects = &mut self.effects;
        let _ = self.ctx.run(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1800.0, 900.0))),
            time: Some(self.time), focused: true, events, ..Default::default()
        }, |ctx| {
            app.workflow_guide_with(ctx, |_, _, action| effects.push(action));
        });
    }
    fn settle(&mut self) { for _ in 0..3 { self.frame(Vec::new()); } }
    fn click_at(&mut self, pos: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }]);
        self.frame(vec![egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
        self.settle();
    }
    fn step(&mut self, step: usize) {
        let rect: Rect = self.ctx.data(|data| data.get_temp(egui::Id::new(("workflow-test", step)))).unwrap();
        self.click_at(rect.center());
    }
    fn acquisition(&mut self, label: &str) {
        self.step(0);
        let rect: Rect = self.ctx.data(|data| data.get_temp(egui::Id::new(("workflow-menu-test", label)))).unwrap();
        self.click_at(rect.center());
    }
    fn image(&mut self) {
        self.app.set_image(&self.ctx, ImageData { width: 32, height: 32,
            rgb: vec![127; 32*32*3], source: "synthetic-workflow".into() }, true);
    }
}
impl Drop for Guide {
    fn drop(&mut self) {
        // Only this fixture's explicitly created file-free directory is removed.
        fs::remove_dir(&self.directory).unwrap();
    }
}

#[test]
fn workflow_first_step_dispatches_each_native_acquisition_choice() {
    for (label, effect, tab) in [
        ("Démarrer le flux USB", WorkflowExternal::Stream, RibbonTab::Camera),
        ("Ouvrir une image…", WorkflowExternal::Image, RibbonTab::Files),
        ("Ouvrir un projet…", WorkflowExternal::Project, RibbonTab::Files),
    ] {
        let mut guide = Guide::new();
        guide.acquisition(label);
        assert_eq!(guide.effects, [effect]);
        assert_eq!(guide.app.active_ribbon_tab, tab);
        assert_eq!(guide.app.workflow_current_step(), 0);
        assert!(guide.app.measurements.is_empty());
    }
}

#[test]
fn workflow_plan_and_control_dispatch_import_and_remain_distinct() {
    let mut guide = Guide::new();
    guide.step(1);
    assert_eq!(guide.effects, [WorkflowExternal::Plan]);
    assert_eq!(guide.app.active_ribbon_tab, RibbonTab::Inspection);
    assert_eq!(guide.app.workflow_current_step(), 1);
    guide.effects.clear();
    guide.step(4);
    assert_eq!(guide.effects, [WorkflowExternal::Plan]);
    assert_eq!(guide.app.workflow_current_step(), 4);
    guide.app.inspection.plan = Some(dper_drawing::parse_text_plan("Ø4 ±0.1 mm", "synthetic.pdf"));
    guide.app.show_configuration_panel = true;
    guide.effects.clear();
    guide.step(4);
    assert!(guide.effects.is_empty(), "Existing plan must not reopen the import dialog");
    assert!(!guide.app.show_configuration_panel);
    assert_eq!(guide.app.workflow_current_step(), 4);
    guide.step(1);
    assert_eq!(guide.effects, [WorkflowExternal::Plan], "Plan step still explicitly permits replacing a plan");
    assert_eq!(guide.app.workflow_current_step(), 1);
}

#[test]
fn workflow_calibration_and_measurements_freeze_and_activate_actual_modes() {
    let mut guide = Guide::new();
    guide.image();
    guide.app.running = true;
    guide.app.paused = false;
    guide.app.selection_mode = true;
    guide.step(2);
    assert!(guide.app.calibration_mode && guide.app.paused);
    assert!(!guide.app.selection_mode);
    assert_eq!(guide.app.active_ribbon_tab, RibbonTab::Calibration);
    assert!(guide.app.calibration.is_none(), "Entering the mode is not a calibration");
    guide.app.escape_active_mode();
    guide.app.mode = Tool::Circle;
    guide.app.paused = false;
    guide.step(3);
    assert_eq!(guide.app.active_ribbon_tab, RibbonTab::Measurements);
    assert_eq!(guide.app.mode, Tool::Distance);
    assert!(guide.app.paused && !guide.app.selection_mode);
    assert!(guide.effects.is_empty());
    assert!(guide.app.measurements.is_empty());
}

#[test]
fn workflow_export_opens_actual_report_preparation_without_writing_report() {
    let mut guide = Guide::new();
    guide.image();
    guide.step(5);
    assert_eq!(guide.app.active_ribbon_tab, RibbonTab::Exports);
    assert!(guide.app.export_dialog.is_some());
    assert_eq!(fs::read_dir(&guide.directory).unwrap().count(), 0);
    assert!(guide.effects.is_empty());
    guide.app.export_dialog.as_mut().unwrap().preferences.title = "Titre conservé".into();
    guide.step(5);
    assert_eq!(guide.app.export_dialog.as_ref().unwrap().preferences.title, "Titre conservé");
}

#[test]
fn workflow_acquisition_and_annotation_protect_in_progress_user_work() {
    let mut guide = Guide::new();
    guide.image();
    guide.app.running = true;
    guide.app.paused = false;
    guide.app.capture_timer = Some(CaptureTimer { destination: guide.directory.join("unused.png"),
        capture_at: Instant::now() + Duration::from_secs(100), best_frame: None,
        best_sharpness: 0.0, frames_seen: 0 });
    for step in 1..6 { guide.step(step); }
    guide.acquisition("Ouvrir un projet…");
    assert!(!guide.app.paused && !guide.app.calibration_mode);
    assert!(guide.effects.is_empty() && guide.app.export_dialog.is_none());
    guide.app.start_ribbon_calibration();
    assert!(!guide.app.calibration_mode);
    guide.app.capture_timer = None;
    guide.app.annotation_mode = true;
    guide.app.annotation_points.push(Point { x: 5.0, y: 6.0 });
    for step in 1..6 { guide.step(step); }
    assert!(guide.app.annotation_mode);
    assert_eq!(guide.app.annotation_points, [Point { x: 5.0, y: 6.0 }]);
    assert!(!guide.app.paused && !guide.app.calibration_mode);
    assert!(guide.effects.is_empty());
}

#[test]
fn workflow_missing_image_provides_guidance_without_creating_measurement_or_report() {
    let mut guide = Guide::new();
    for step in [2, 3, 5] {
        guide.step(step);
        assert!(guide.app.message.contains("Ouvrez une image"));
        assert!(guide.app.pending.is_empty() && guide.app.measurements.is_empty());
        assert!(!guide.app.calibration_mode);
        assert!(guide.app.export_dialog.is_none());
    }
}

#[test]
fn workflow_busy_trace_and_calibration_preserve_state_and_reject_external_actions() {
    let mut guide = Guide::new();
    guide.image();
    guide.app.pending.push(Point { x: 3.0, y: 4.0 });
    guide.app.active_ribbon_tab = RibbonTab::Measurements;
    for step in 1..6 {
        guide.step(step);
        assert_eq!(guide.app.pending, [Point { x: 3.0, y: 4.0 }]);
        assert_eq!(guide.app.active_ribbon_tab, RibbonTab::Measurements);
        assert!(guide.app.message.contains("Terminez"));
    }
    guide.acquisition("Ouvrir une image…");
    assert!(guide.effects.is_empty());
    guide.app.pending.clear();
    guide.app.calibration_mode = true;
    guide.step(4);
    assert!(guide.app.calibration_mode);
    assert!(guide.effects.is_empty() && guide.app.export_dialog.is_none());
}

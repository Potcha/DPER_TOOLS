use super::*;

struct Harness {
    app: DperMeasureApp,
    ctx: egui::Context,
    shapes: Vec<egui::epaint::ClippedShape>,
    time: f64,
    width: f32,
    disabled: bool,
}
impl Harness {
    fn new(width: f32) -> Self {
        let ctx = egui::Context::default();
        let app = DperMeasureApp::new_with_services(ctx.clone(), false);
        let mut harness = Self { app, ctx, shapes: Vec::new(), time: 0.0, width, disabled: false };
        harness.settle();
        harness
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0 / 60.0;
        self.shapes = self.ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(self.width, 900.0))),
            focused: true, time: Some(self.time), events, ..Default::default()
        }, |ctx| {
            self.app.ribbon_tabs(ctx);
            egui::TopBottomPanel::top("ribbon-test-commands").show(ctx, |ui| {
                if self.disabled || self.app.registration_photo_pick_owns_frame() { ui.disable(); }
                self.app.ribbon_commands(ui, ctx);
            });
        }).shapes;
    }
    fn settle(&mut self) { for _ in 0..4 { self.frame(Vec::new()); } }
    fn rect(&self, label: &str) -> egui::Rect {
        self.ctx.data(|data| data.get_temp(egui::Id::new(("ribbon-test", label))))
            .unwrap_or_else(|| panic!("No interactive rectangle for {label:?}"))
    }
    fn click(&mut self, label: &str) {
        let pos = self.rect(label).center();
        self.frame(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default(),
        }]);
        self.frame(vec![egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default(),
        }]);
        self.settle();
    }
    fn hover(&mut self, label: &str) {
        let pos = self.rect(label).center();
        self.frame(vec![egui::Event::PointerMoved(pos)]);
        for _ in 0..30 { self.frame(Vec::new()); }
    }
}

#[test]
fn pointer_opens_measurement_tab_group_and_selects_non_promoted_tool() {
    let mut h = Harness::new(1800.0);
    h.click("Mesures");
    assert_eq!(h.app.active_ribbon_tab, RibbonTab::Measurements);
    h.click("Formes et contours v");
    h.hover("Cercles");
    h.click("Cercle ajusté");
    assert_eq!(h.app.mode, Tool::CircleFit);
    assert!(!h.app.selection_mode);
}

#[test]
fn pointer_on_disabled_tool_does_not_interrupt_pending_trace() {
    let mut h = Harness::new(1800.0);
    h.click("Mesures");
    h.app.mode = Tool::Distance;
    h.app.pending.push(crate::geometry::Point { x: 2.0, y: 3.0 });
    h.settle();
    h.click("Formes et contours v");
    h.hover("Cercles");
    h.click("Cercle ajusté");
    assert_eq!(h.app.mode, Tool::Distance);
    assert_eq!(h.app.pending.len(), 1);
}

#[test]
fn narrow_ribbon_overflow_reaches_hidden_group_and_tool() {
    let mut h = Harness::new(380.0);
    h.app.active_ribbon_tab = RibbonTab::Measurements;
    h.settle();
    h.click("Plus v");
    h.hover("Formes et contours");
    h.hover("Cercles");
    h.click("Cercle ajusté");
    assert_eq!(h.app.mode, Tool::CircleFit);
}

#[test]
fn ribbon_selection_freezes_the_live_capture_before_editing() {
    let mut h = Harness::new(1800.0);
    h.app.image = Some(ImageData { width: 16, height: 16, rgb: vec![127; 16 * 16 * 3], source: "synthetic".into() });
    h.app.running = true;
    h.app.paused = false;
    h.app.active_ribbon_tab = RibbonTab::Measurements;
    h.settle();
    h.click("Sélection");
    assert!(h.app.selection_mode);
    assert!(h.app.paused);
}

#[test]
fn disabled_ribbon_rejects_pointer_on_promoted_tool() {
    let mut h = Harness::new(1800.0);
    h.app.active_ribbon_tab = RibbonTab::Measurements;
    h.app.mode = Tool::Distance;
    h.disabled = true;
    h.settle();
    h.click("Cercle");
    assert_eq!(h.app.mode, Tool::Distance);
}

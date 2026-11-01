// Included inside registration_ui::tests to reuse its isolated application
// fixture. Actions below go through real pointer and keyboard egui events;
// neither preparation nor registration is called directly by this harness.
struct RegistrationInteraction {
    fixture: Fixture,
    shapes: Vec<egui::epaint::ClippedShape>,
    time: f64,
}

impl RegistrationInteraction {
    fn new() -> Self {
        let mut interaction = Self { fixture: Fixture::new(), shapes: Vec::new(), time: 0.0 };
        interaction.settle();
        interaction
    }

    fn frame(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        self.time += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 1800.0))),
            time: Some(self.time), focused: true, events, modifiers,
            ..Default::default()
        };
        let app = &mut self.fixture.app;
        self.shapes = self.fixture.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.registration_editor_panel(ui));
        }).shapes;
    }

    fn settle(&mut self) {
        for _ in 0..3 { self.frame(Vec::new(), egui::Modifiers::default()); }
    }

    fn text_rects(&self, needle: &str) -> Vec<egui::Rect> {
        fn collect(shape: &egui::epaint::Shape, needle: &str, rectangles: &mut Vec<egui::Rect>) {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == needle => {
                    rectangles.push(text.galley.rect.translate(text.pos.to_vec2()));
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes { collect(shape, needle, rectangles); }
                }
                _ => {}
            }
        }
        let mut rectangles = Vec::new();
        for clipped in &self.shapes { collect(&clipped.shape, needle, &mut rectangles); }
        rectangles
    }

    fn text_rect(&self, needle: &str) -> egui::Rect {
        let rectangles = self.text_rects(needle);
        assert_eq!(rectangles.len(), 1, "Expected one rendered widget label: {needle}");
        rectangles[0]
    }

    fn click_at(&mut self, pos: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true,
            modifiers: egui::Modifiers::default(),
        }], egui::Modifiers::default());
        self.frame(vec![egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: false,
            modifiers: egui::Modifiers::default(),
        }], egui::Modifiers::default());
        self.settle();
    }

    fn click_button(&mut self, label: &str) {
        self.click_at(self.text_rect(label).center());
    }

    fn replace_at(&mut self, pos: egui::Pos2, value: &str) {
        self.click_at(pos);
        assert!(self.fixture.ctx.memory(|memory| memory.focused()).is_some(), "Text input did not acquire focus");
        let select_all = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        self.frame(vec![egui::Event::Key {
            key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: true,
            repeat: false, modifiers: select_all,
        }], select_all);
        self.frame(vec![egui::Event::Key {
            key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: false,
            repeat: false, modifiers: egui::Modifiers::default(),
        }, egui::Event::Text(value.into())], egui::Modifiers::default());
        self.settle();
    }

    fn start(&mut self) {
        self.click_button("Nouveau brouillon (effacer la saisie)");
        let draft = self.fixture.app.registration_ui.draft.as_ref().expect("New-draft button did not prepare the editor");
        assert_eq!(draft.rows.len(), 3);
        assert!(draft.rows.iter().all(|row| row.iter().all(String::is_empty)));
    }

    fn fill_landmarks(&mut self) {
        let rows = [["5", "5", "20", "30"], ["25", "5", "60", "30"], ["5", "25", "20", "70"]];
        let headings = ["Plan X (pt)", "Plan Y (pt)", "Image X (px)", "Image Y (px)"];
        for (index, row) in rows.iter().enumerate() {
            for (column, value) in row.iter().enumerate() {
                let heading = self.text_rect(headings[column]);
                let plan_x = self.text_rect(headings[0]);
                let ordinal = self.text_rects(&(index + 1).to_string()).into_iter()
                    .find(|rect| rect.left() < plan_x.left())
                    .expect("Rendered landmark row ordinal missing");
                self.replace_at(egui::pos2(heading.left() + 8.0, ordinal.center().y), value);
                assert_eq!(self.fixture.app.registration_ui.draft.as_ref().unwrap().rows[index][column], *value,
                    "Keyboard input missed landmark {}, column {}", index + 1, column + 1);
            }
        }
    }

    fn threshold(&mut self, previous: &str, value: &str) {
        let label = self.text_rect("Seuil de résidu (pixels natifs)");
        let field = self.text_rects(previous).into_iter().find(|rect| {
            rect.left() > label.right() && (rect.center().y - label.center().y).abs() < 10.0
        }).expect("Rendered threshold input missing");
        self.replace_at(field.center(), value);
        assert_eq!(self.fixture.app.registration_ui.draft.as_ref().unwrap().threshold_px, value);
    }
}

#[test]
fn pointer_keyboard_registration_workflow_applies_known_similarity() {
    let mut ui = RegistrationInteraction::new();
    let revision = ui.fixture.app.document_revision;
    let calibration = serde_json::to_value(&ui.fixture.app.calibration).unwrap();
    ui.start();
    ui.fill_landmarks();
    ui.threshold("2", "0.1");
    ui.click_button("Calculer et appliquer la similitude");
    let report = ui.fixture.app.inspection.registration.as_ref().expect("Apply button did not install registration");
    assert!((report.transform.scale - 2.0).abs() < 1e-10);
    assert!(report.transform.rotation_rad.abs() < 1e-10);
    assert!((report.transform.translation.x - 10.0).abs() < 1e-10);
    assert!((report.transform.translation.y - 20.0).abs() < 1e-10);
    assert_eq!(report.inlier_indices.len(), 3);
    assert_eq!(ui.fixture.app.document_revision, revision + 1);
    assert_eq!(serde_json::to_value(&ui.fixture.app.calibration).unwrap(), calibration);
    assert_eq!(ui.fixture.app.mm_per_pixel, Some(0.125));
    assert!(ui.fixture.app.measurements.is_empty());
}

#[test]
fn pointer_keyboard_invalid_blank_and_negative_threshold_do_not_apply() {
    let mut ui = RegistrationInteraction::new();
    let revision = ui.fixture.app.document_revision;
    ui.start();
    ui.click_button("Calculer et appliquer la similitude");
    assert!(ui.fixture.app.registration_ui.status.contains("nombre fini"));
    assert!(ui.fixture.app.inspection.registration.is_none());
    ui.fill_landmarks();
    ui.threshold("2", "-1");
    ui.click_button("Calculer et appliquer la similitude");
    assert!(ui.fixture.app.registration_ui.status.contains("positif"));
    assert!(ui.fixture.app.inspection.registration.is_none());
    assert_eq!(ui.fixture.app.document_revision, revision);
    assert!(ui.fixture.app.measurements.is_empty());
}

#[test]
fn pointer_keyboard_apply_stays_disabled_after_same_source_reimport() {
    let mut ui = RegistrationInteraction::new();
    ui.start();
    ui.fill_landmarks();
    ui.threshold("2", "0.1");
    let rows = ui.fixture.app.registration_ui.draft.as_ref().unwrap().rows.clone();
    let plan = ui.fixture.app.inspection.plan.as_ref().unwrap().clone();
    // Simulate completion of the native import dialog through its real shared
    // plan installation operation, then exercise the stale editor by pointer.
    ui.fixture.app.install_inspection_plan(plan);
    let revision = ui.fixture.app.document_revision;
    ui.settle();
    assert!(ui.fixture.app.native_registration_stale());
    ui.click_button("Calculer et appliquer la similitude");
    assert!(ui.fixture.app.inspection.registration.is_none());
    assert_eq!(ui.fixture.app.document_revision, revision);
    assert_eq!(ui.fixture.app.registration_ui.draft.as_ref().unwrap().rows, rows);
    assert!(ui.fixture.app.measurements.is_empty());
}

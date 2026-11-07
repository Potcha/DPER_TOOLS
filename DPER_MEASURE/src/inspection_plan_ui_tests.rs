use super::*;
use crate::dper_pdf_geometry::DrawingGeometryCatalog;
use crate::dper_pdf_layout::{PdfBox, PdfPageGeometry};

#[test]
fn native_repeated_circles_are_assigned_in_click_order() {
    let mut fixture = Fixture::new();
    fixture.app.inspection.plan.as_mut().unwrap().characteristics[0].repetition = Some(2);
    fixture.define_view_by_pointer();
    fixture.prepare_binding_by_pointer();
    let candidates = fixture.app.inspection_plan_ui.binding.as_ref().unwrap().candidates.clone();
    assert_eq!(candidates.len(), 2);
    for candidate in candidates.iter().rev() {
        fixture.click(&format!("{} · rang {} · distance {:.2} pt", candidate.geometry_id, candidate.rank, candidate.distance_pt));
    }
    fixture.type_into_empty("Décrire le rappel de cote et le cercle choisi", "Deux emplacements repérés sur la vue.");
    fixture.click("Enregistrer les occurrences choisies");
    let workspace = &fixture.app.inspection;
    assert!(workspace.bindings.is_empty());
    assert!(workspace.records.is_empty());
    assert_eq!(workspace.occurrence_assignments.len(), 1, "{}", fixture.app.inspection_plan_ui.status);
    let assignment = &workspace.occurrence_assignments[0];
    assert_eq!(assignment.members[0].geometry_id, candidates[1].geometry_id);
    assert_eq!(assignment.members[1].geometry_id, candidates[0].geometry_id);
    assert!(fixture.app.validate_occurrence_assignment(assignment).is_ok());
    let feature_id = assignment.feature_id.clone();
    fixture.show_panel(TestPanel::Control);
    fixture.click("Répétitions et relations du plan");
    assert_eq!(fixture.text_rects(&format!("{feature_id} · affectation à vérifier sur le plan")).len(), 1);
    std::fs::write(&fixture.path, b"replacement PDF source").unwrap();
    fixture.settle();
    assert_eq!(fixture.text_rects(&format!("{feature_id} · affectation suspendue, rechargez les candidats et renouvelez le choix")).len(), 1);
}

#[path = "drawing_attachment_ui_tests.rs"]
mod drawing_attachment_tests;

#[derive(Clone, Copy)]
enum TestPanel { Plan, Registration, Observation, Decision, Control }

struct Fixture {
    app: DperMeasureApp,
    ctx: egui::Context,
    path: std::path::PathBuf,
    shapes: Vec<egui::epaint::ClippedShape>,
    time: f64,
    panel: TestPanel,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let ctx = egui::Context::default();
        let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("dper-native-plan-{}-{sequence}.pdf", std::process::id()));
        // Byte-bound PDF source; independent SVG proposals keep these tests free
        // from external extractors, industrial drawings, and live MCP services.
        std::fs::write(&path, include_bytes!("../tests/fixtures/drawing_geometry/01_circle_leader.pdf")).unwrap();
        let fingerprint = crate::dper_source::fingerprint_file(&path).unwrap();
        let mut plan = crate::dper_drawing::parse_text_plan("Ø20 mm", "synthetic.pdf");
        plan.source_file = Some(path.display().to_string());
        plan.source_fingerprint = Some(fingerprint.clone());
        plan.page_geometry = vec![PdfPageGeometry { page: 1, width_pt: 240.0, height_pt: 180.0 }];
        plan.characteristics[0].source.page = Some(1);
        plan.characteristics[0].source.pdf_bbox = Some(PdfBox { x_min: 114.0, y_min: 18.0, x_max: 126.0, y_max: 26.0 });
        plan.characteristics[0].source.word_ids = vec!["word-1".into()];
        app.install_inspection_plan(plan);
        let geometry = crate::dper_pdf_geometry::parse_svg_geometry(
            r#"<svg width="240pt" height="180pt" viewBox="0 0 240 180"><circle cx="60" cy="90" r="20"/><circle cx="180" cy="90" r="20"/></svg>"#, 1).unwrap();
        app.handle_drawing_request(DrawingRequest::InstallCatalog {
            catalog: DrawingGeometryCatalog { document_fingerprint: fingerprint, geometry },
            expected_revision: app.document_revision,
        }).unwrap();
        let mut fixture = Self { app, ctx, path, shapes: Vec::new(), time: 0.0, panel: TestPanel::Plan };
        fixture.settle();
        fixture
    }

    fn frame(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        self.time += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1800.0, 2400.0))),
            time: Some(self.time), focused: true, events, modifiers, ..Default::default()
        };
        let app = &mut self.app;
        let panel = self.panel;
        self.shapes = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| match panel {
                TestPanel::Plan => app.inspection_plan_editor_panel(ui),
                TestPanel::Registration => app.registration_editor_panel(ui),
                TestPanel::Observation => app.inspection_observation_panel(ui),
                TestPanel::Decision => app.inspection_decision_panel(ui),
                TestPanel::Control => app.automated_inspection_panel(ui),
            });
        }).shapes;
    }

    fn settle(&mut self) {
        for _ in 0..3 { self.frame(Vec::new(), egui::Modifiers::default()); }
    }

    fn text_rects(&self, needle: &str) -> Vec<egui::Rect> {
        fn collect(shape: &egui::epaint::Shape, needle: &str, found: &mut Vec<egui::Rect>) {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == needle => {
                    found.push(text.galley.rect.translate(text.pos.to_vec2()));
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes { collect(shape, needle, found); }
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &self.shapes { collect(&clipped.shape, needle, &mut found); }
        found
    }

    fn text_rect(&self, needle: &str) -> egui::Rect {
        let found = self.text_rects(needle);
        assert_eq!(found.len(), 1, "Expected one rendered widget: {needle}");
        found[0]
    }

    fn click(&mut self, label: &str) {
        let pos = self.text_rect(label).center();
        self.click_at(pos);
    }

    fn click_at(&mut self, pos: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::default(),
        }], egui::Modifiers::default());
        self.frame(vec![egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::default(),
        }], egui::Modifiers::default());
        self.settle();
    }

    fn replace_at(&mut self, pos: egui::Pos2, value: &str) {
        self.click_at(pos);
        assert!(self.ctx.memory(|memory| memory.focused()).is_some());
        let modifiers = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        self.frame(vec![egui::Event::Key {
            key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: true,
            repeat: false, modifiers,
        }], modifiers);
        self.frame(vec![egui::Event::Key {
            key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: false,
            repeat: false, modifiers: egui::Modifiers::default(),
        }, egui::Event::Text(value.into())], egui::Modifiers::default());
        self.settle();
    }

    fn show_panel(&mut self, panel: TestPanel) {
        self.panel = panel;
        self.settle();
    }

    fn type_into_empty(&mut self, hint: &str, text: &str) {
        self.click(hint);
        assert!(self.ctx.memory(|memory| memory.focused()).is_some());
        self.frame(vec![egui::Event::Text(text.into())], egui::Modifiers::default());
        self.settle();
    }

    fn define_view_by_pointer(&mut self) {
        self.click("Préparer / recharger la vue");
        self.type_into_empty("Ex. : vue de dessus", "Vue de dessus");
        let feature = feature_label(&self.app.inspection.plan.as_ref().unwrap().characteristics[0]);
        self.click(&feature);
        let primitives = self.app.drawing_geometry_cache[0].geometry.primitives.clone();
        for primitive in &primitives { self.click(&primitive_label(primitive)); }
        self.click("Enregistrer la vue explicite");
        assert_eq!(self.app.inspection.views.len(), 1, "{}", self.app.inspection_plan_ui.status);
    }

    fn prepare_binding_by_pointer(&mut self) {
        self.click("Choisir une vue");
        let view = self.app.inspection.views[0].clone();
        self.click(&format!("{} · page {} · {}", view.label, view.page, view.id));
        self.click("Choisir une caractéristique de la vue");
        let feature = feature_label(&self.app.inspection.plan.as_ref().unwrap().characteristics[0]);
        self.click(&feature);
        self.click("Préparer / recharger les associations");
    }

    fn prepare_valid_view_direct(&mut self) {
        self.app.inspection_plan_ui.page = 1;
        self.app.prepare_native_view().unwrap();
        let draft = self.app.inspection_plan_ui.view.as_mut().unwrap();
        draft.label = "Vue explicite".into();
        draft.feature_ids = draft.features.iter().map(|feature| feature.id.clone()).collect();
        draft.geometry_ids = draft.primitives.iter().map(|primitive| primitive.id.clone()).collect();
    }

    fn prepare_valid_binding_direct(&mut self) {
        self.prepare_valid_view_direct();
        self.app.submit_native_view().unwrap();
        self.app.inspection_plan_ui.binding_view_id = self.app.inspection.views[0].id.clone();
        self.app.inspection_plan_ui.binding_feature_id = self.app.inspection.views[0].feature_ids[0].clone();
        self.app.prepare_native_binding().unwrap();
        let draft = self.app.inspection_plan_ui.binding.as_mut().unwrap();
        draft.candidate_id = Some(draft.candidates[1].id.clone());
        draft.justification = "Cercle explicitement choisi après comparaison du rappel de cote.".into();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.path); }
}

#[test]
fn pointer_keyboard_defines_explicit_view_and_selects_second_circle_without_review() {
    let mut fixture = Fixture::new();
    let revision = fixture.app.document_revision;
    fixture.define_view_by_pointer();
    assert_eq!(fixture.app.inspection.views[0].geometry_ids.len(), 2);
    assert_eq!(fixture.app.document_revision, revision + 1);
    assert!(fixture.app.inspection.bindings.is_empty());
    fixture.prepare_binding_by_pointer();
    let draft = fixture.app.inspection_plan_ui.binding.as_ref().unwrap();
    assert_eq!(draft.candidates.len(), 2);
    assert!(draft.candidate_id.is_none(), "Proximity must not silently select the first candidate");
    let alternative = draft.candidates[1].clone();
    // Clicking a disabled submit cannot create an association without a choice.
    fixture.click("Enregistrer l'association choisie");
    assert!(fixture.app.inspection.bindings.is_empty());
    fixture.click(&format!("{} · rang {} · distance {:.2} pt", alternative.geometry_id, alternative.rank, alternative.distance_pt));
    fixture.type_into_empty("Décrire le rappel de cote et le cercle choisi", "Rappel de cote vérifié sur le cercle de droite de cette vue.");
    fixture.click("Enregistrer l'association choisie");
    assert_eq!(fixture.app.inspection.bindings.len(), 1, "{}", fixture.app.inspection_plan_ui.status);
    let selection = &fixture.app.inspection.bindings[0];
    assert_eq!(selection.candidate.id, alternative.id);
    assert!(!selection.reviewed);
    assert_eq!(fixture.app.document_revision, revision + 2);
    assert!(fixture.app.measurements.is_empty());
    assert!(fixture.app.calibration.is_none());
    assert!(fixture.app.inspection.registration.is_none());
}

#[test]
fn both_native_drafts_refuse_changed_revision_capture_plan_bytes_catalog_and_gesture() {
    for binding in [false, true] {
        for change in 0..7 {
            let mut fixture = Fixture::new();
            if binding { fixture.prepare_valid_binding_direct(); } else { fixture.prepare_valid_view_direct(); }
            let prepared_revision = if binding {
                fixture.app.inspection_plan_ui.binding.as_ref().unwrap().context.revision
            } else { fixture.app.inspection_plan_ui.view.as_ref().unwrap().context.revision };
            match change {
                0 => fixture.app.document_revision += 1,
                1 => fixture.app.capture_id = "changed-capture".into(),
                2 => fixture.app.inspection.plan.as_mut().unwrap().source_name = "different-plan".into(),
                3 => {
                    // Same byte count defeats length-only source checks.
                    let mut bytes = std::fs::read(&fixture.path).unwrap();
                    bytes[0] ^= 1;
                    std::fs::write(&fixture.path, bytes).unwrap();
                }
                4 => fixture.app.drawing_geometry_generation += 1,
                5 => fixture.app.pending.push(crate::geometry::Point { x: 1.0, y: 1.0 }),
                _ => fixture.app.drawing_geometry_cache[0].geometry.primitives[0].bbox.x_min += 1.0,
            }
            let before = serde_json::to_value(&fixture.app.inspection).unwrap();
            let revision = fixture.app.document_revision;
            let result = if binding { fixture.app.submit_native_binding() } else { fixture.app.submit_native_view() };
            assert!(result.is_err(), "binding={binding}, change={change}");
            assert_eq!(fixture.app.document_revision, revision);
            assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), before);
            let retained_revision = if binding {
                fixture.app.inspection_plan_ui.binding.as_ref().unwrap().context.revision
            } else { fixture.app.inspection_plan_ui.view.as_ref().unwrap().context.revision };
            assert_eq!(retained_revision, prepared_revision, "Submit must never refresh a stale draft");
        }
    }
}

#[test]
fn pointer_stale_view_requires_explicit_reload_and_replacement_is_explicit() {
    let mut fixture = Fixture::new();
    fixture.click("Préparer / recharger la vue");
    fixture.type_into_empty("Ex. : vue de dessus", "Brouillon conservé");
    let prepared = fixture.app.inspection_plan_ui.view.as_ref().unwrap().context.revision;
    fixture.app.document_revision += 1;
    fixture.settle();
    fixture.click("Enregistrer la vue explicite");
    assert!(fixture.app.inspection.views.is_empty());
    assert_eq!(fixture.app.inspection_plan_ui.view.as_ref().unwrap().context.revision, prepared);
    assert_eq!(fixture.app.inspection_plan_ui.view.as_ref().unwrap().label, "Brouillon conservé");
    fixture.click("Préparer / recharger la vue");
    assert_eq!(fixture.app.inspection_plan_ui.view.as_ref().unwrap().context.revision, fixture.app.document_revision);
    assert!(fixture.app.inspection_plan_ui.view.as_ref().unwrap().label.is_empty());
    fixture.define_view_by_pointer();
    let old = fixture.app.inspection.views[0].clone();
    fixture.click("Nouvelle vue");
    fixture.click(&format!("Remplacer : {} · {}", old.label, old.id));
    fixture.click("Préparer / recharger la vue");
    assert_eq!(fixture.app.inspection_plan_ui.view.as_ref().unwrap().replace_view_id.as_deref(), Some(old.id.as_str()));
    fixture.click("Enregistrer la vue explicite");
    assert_eq!(fixture.app.inspection.views.len(), 1);
    assert!(fixture.app.inspection.bindings.is_empty());
}

#[test]
fn empty_or_unknown_selection_and_missing_justification_do_not_create_binding() {
    let mut fixture = Fixture::new();
    fixture.prepare_valid_binding_direct();
    fixture.app.inspection_plan_ui.binding.as_mut().unwrap().candidate_id = None;
    assert!(fixture.app.submit_native_binding().is_err());
    fixture.app.inspection_plan_ui.binding.as_mut().unwrap().candidate_id = Some("unknown".into());
    assert!(fixture.app.submit_native_binding().is_err());
    let draft = fixture.app.inspection_plan_ui.binding.as_mut().unwrap();
    draft.candidate_id = Some(draft.candidates[0].id.clone());
    draft.justification.clear();
    assert!(fixture.app.submit_native_binding().is_err());
    assert!(fixture.app.inspection.bindings.is_empty());
}

#[test]
fn end_to_end_native_pdf_view_binding_registration_grouped_observation_and_current_refusals() {
    let mut fixture = Fixture::new();
    let independent_calibration = acceptance::composed_native_observation(&mut fixture, |_, _| {});
    assert_eq!(fixture.app.inspection.plan.as_ref().unwrap().characteristics[0].nominal, Some(20.0));
    let record = fixture.app.current_inspection().records.last().unwrap().clone();
    assert_eq!(record.group, "Pièce témoin intégrée");
    assert!(record.measurement_id.is_some());
    assert_eq!(record.outcome.conformity_status, crate::dper_metrology::ConformityStatus::NotApplicable);
    for reason in ["tolerance_missing", "calibration_unqualified", "uncertainty_unknown"] {
        assert!(record.outcome.blocking_reasons.iter().any(|actual| actual == reason), "Missing conservative gate: {reason}");
    }
    assert_eq!(serde_json::to_value(&fixture.app.calibration).unwrap(), independent_calibration);
    fixture.show_panel(TestPanel::Control);

    // Refresh current results after a pixel edit: software evidence cannot stay
    // validated simply because the original operation returned successfully.
    fixture.app.image.as_mut().unwrap().rgb[0] ^= 1;
    fixture.show_panel(TestPanel::Observation);
    fixture.click("Réévaluer les observations existantes");
    let current = fixture.app.current_inspection();
    let stale = current.records.last().unwrap();
    assert_ne!(stale.outcome.geometry_state, crate::dper_metrology::GeometryState::Validated);
    assert!(stale.outcome.blocking_reasons.iter().any(|reason| reason == "source_pixels_changed"));
    assert_eq!(fixture.app.measurements.len(), 1);

    // Changing the live PDF bytes also refuses a new preparation, rather than
    // silently blessing the saved association against a different source.
    let mut source_bytes = std::fs::read(&fixture.path).unwrap();
    source_bytes[0] ^= 1;
    std::fs::write(&fixture.path, source_bytes).unwrap();
    let record_count = fixture.app.inspection.records.len();
    fixture.click("Préparer une nouvelle observation");
    assert_eq!(fixture.app.inspection.records.len(), record_count);
    assert_eq!(fixture.app.measurements.len(), 1);
    assert!(fixture.app.current_drawing_source().is_err());
}

#[path = "inspection_acceptance_tests.rs"]
mod acceptance;

#[test]
fn documentary_unit_conflict_sources_are_inspectable_and_never_confirmed_by_opening() {
    let mut fixture = Fixture::new();
    let plan = crate::dper_drawing::parse_text_plan("Units: mm / inches\nØ4 ±0.1", "unit-conflict.pdf");
    fixture.app.install_inspection_plan(plan);
    fixture.show_panel(TestPanel::Control);
    fixture.text_rect("Unités du document : Déclarations contradictoires");
    fixture.click("Sources des déclarations d'unités");
    fixture.text_rect("1 · page 1 · Units: mm / inches");
    fixture.text_rect("Unités reconnues : [Millimeter, Inch]");
    let feature = fixture.app.inspection.plan.as_ref().unwrap().characteristics[0].clone();
    fixture.click(&format!("{} · {}", feature.id, feature.label));
    fixture.text_rect("Origine de l'unité : Unité non spécifiée");
    fixture.text_rect("Unité à résoudre avant toute comparaison de tolérance.");
    assert!(fixture.app.measurements.is_empty());
    let current = &fixture.app.inspection.plan.as_ref().unwrap().characteristics[0];
    assert!(!current.confirmed && current.nominal_mm().is_none() && current.limits_mm().is_none());
}

#[test]
fn old_plan_without_unit_provenance_renders_as_historical() {
    let mut fixture = Fixture::new();
    fixture.app.inspection.plan.as_mut().unwrap().unit_context = None;
    fixture.app.inspection_panel_cache = None;
    fixture.show_panel(TestPanel::Control);
    fixture.text_rect("Contexte d'unités historique : provenance non conservée. Réanalysez le document pour la retrouver.");
    assert!(!fixture.app.inspection.plan.as_ref().unwrap().characteristics[0].confirmed);
}

#[test]
fn view_scope_historical_reason_is_visible_without_granting_new_authority() {
    let mut fixture = Fixture::new();
    fixture.define_view_by_pointer();
    fixture.app.inspection.views[0].source_proof = None;
    let view = &fixture.app.inspection.views[0];
    let label = format!("Vue « {} » suspendue : Vue historique sans preuve de sources courante ; redéfinissez la vue et ses associations.", view.label);
    fixture.settle();
    fixture.text_rect(&label);
    assert!(fixture.app.inspection.bindings.is_empty());
    assert!(fixture.app.measurements.is_empty());
}

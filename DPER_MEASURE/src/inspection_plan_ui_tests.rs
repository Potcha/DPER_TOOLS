use super::*;
use crate::dper_pdf_geometry::DrawingGeometryCatalog;
use crate::dper_pdf_layout::{PdfBox, PdfPageGeometry};

#[derive(Clone, Copy)]
enum TestPanel { Plan, Registration, Observation, Control }

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
    fixture.define_view_by_pointer();
    fixture.prepare_binding_by_pointer();
    let candidate = fixture.app.inspection_plan_ui.binding.as_ref().unwrap().candidates[1].clone();
    fixture.click(&format!("{} · rang {} · distance {:.2} pt", candidate.geometry_id, candidate.rank, candidate.distance_pt));
    fixture.type_into_empty("Décrire le rappel de cote et le cercle choisi", "Rappel de cote explicitement rattaché au cercle droit de la vue de dessus.");
    fixture.click("Enregistrer l'association choisie");
    assert_eq!(fixture.app.inspection.bindings.len(), 1);
    assert!(!fixture.app.inspection.bindings[0].reviewed);

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
    assert_eq!(fixture.app.inspection.plan.as_ref().unwrap().characteristics[0].nominal, Some(20.0));
    fixture.click("Enregistrer explicitement l'observation");
    assert_eq!(fixture.app.measurements.len(), 1);
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

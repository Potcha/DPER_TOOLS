use super::*;
use crate::{DperMeasureApp, geometry::Point, mcp_server::{InspectionRequest, InspectFeatureArgs}};

fn observed_app() -> DperMeasureApp {
    let mut app = DperMeasureApp::new_with_services(egui::Context::default(), false);
    let mut plan = crate::dper_drawing::parse_text_plan("Ø5 ±0.2 mm", "synthetic drawing");
    plan.characteristics[0].confirmed = true;
    app.install_inspection_plan(plan);
    let image = image::RgbImage::from_fn(128, 128, |x, y| {
        let distance = (x as f64 - 64.0).hypot(y as f64 - 64.0);
        image::Rgb([(20.0 + 200.0 * (distance - 24.5).clamp(0.0, 1.0)) as u8; 3])
    });
    app.image = Some(crate::ImageData { width: 128, height: 128, rgb: image.as_raw().clone(), source: "synthetic.png".into() });
    let mut quality = crate::quality::assess_image_quality(image.as_raw(), 128, 128,
        &crate::quality::ImageQualityThresholds::default()).unwrap();
    quality.is_exploitable = true;
    app.quality_assessment = Some(quality);
    app.calibration = Some(serde_json::from_value(serde_json::json!({
        "id": "independent-evidence-reference", "device": "reference", "resolution": [128,128],
        "optical_setup": "synthetic", "mm_per_pixel": 0.1, "reference_mm": 10.0, "reference_px": 100.0,
    })).unwrap());
    app.mm_per_pixel = Some(0.1);
    let pairs = [Point { x: 10.0, y: 10.0 }, Point { x: 100.0, y: 10.0 }, Point { x: 10.0, y: 100.0 }]
        .map(|plan| crate::dper_registration::Correspondence { plan, image: plan });
    app.inspection.registration = Some(crate::dper_registration::register_similarity(&pairs, 0.1).unwrap());
    app.inspection.registration_capture_id = Some(app.capture_id.clone());
    app.inspection.registration_fingerprint = Some(crate::dper_inspection::fingerprint(&image));
    app
}

fn actual_options() -> crate::dper_vision::VisionOptions {
    crate::dper_vision::VisionOptions {
        sample_count: 128, search_radius_px: 6.0, minimum_gradient: 13.0,
        polarity: crate::dper_vision::EdgePolarity::Rising, minimum_coverage: 0.8,
        outlier_threshold_px: 1.7, maximum_rms_px: 0.65, minimum_arc_sweep_rad: 1.4,
    }
}

fn observe(app: &mut DperMeasureApp, group: &str, commit: bool) {
    let feature_id = app.inspection.plan.as_ref().unwrap().characteristics[0].id.clone();
    app.handle_inspection_request(InspectionRequest::Inspect(InspectFeatureArgs {
        source_measurement_ids: Vec::new(), capture_id: app.capture_id.clone(),
        expected_revision: app.document_revision, feature_id, group: group.into(),
        geometry_in_drawing: InspectionGeometry::Circle { center: Point { x: 64.0, y: 64.0 }, radius: 24.0 },
        options: Some(actual_options()), commit,
    })).unwrap();
}

#[test]
fn evidence_roles_use_current_observation_not_origin_and_keep_construction_separate() {
    let mut app = observed_app();
    observe(&mut app, "Pièce A", false);
    let preview = app.current_inspection();
    assert_eq!(record_role(&preview.records[0]), EvidenceRole::Proposed);
    observe(&mut app, "Pièce A", true);
    let current = app.current_inspection();
    let measurement = app.measurements[0].clone();
    assert_eq!(measurement_role(&measurement, &current), EvidenceRole::SoftwareValidated);
    let mut manual = measurement.clone();
    manual.id = 999;
    manual.origin = "vision".into();
    manual.method = Some("claimed validation cannot grant a status".into());
    assert_eq!(measurement_role(&manual, &current), EvidenceRole::Measured);
    manual.origin = "interface".into();
    assert_eq!(measurement_role(&manual, &current), EvidenceRole::Measured);
    manual.is_construction = true;
    assert_eq!(measurement_role(&manual, &current), EvidenceRole::Construction);
    app.image.as_mut().unwrap().rgb[0] = 0;
    let stale = app.current_inspection();
    assert_ne!(measurement_role(&measurement, &stale), EvidenceRole::SoftwareValidated);
    assert!(stale.records.last().unwrap().outcome.blocking_reasons.contains(&"source_pixels_changed".into()));
}

#[test]
fn evidence_groups_preserve_duplicate_features_and_preview_rows() {
    let mut app = observed_app();
    observe(&mut app, "Pièce A", false);
    observe(&mut app, "Pièce A", true);
    observe(&mut app, "Pièce B", true);
    let state = app.current_inspection();
    let groups = grouped_records(&state);
    assert_eq!(groups["Pièce A"].len(), 2);
    assert_eq!(groups["Pièce B"].len(), 1);
    assert_eq!(groups["Pièce A"][0].1.feature_id, groups["Pièce B"][0].1.feature_id);
    assert!(groups["Pièce A"][0].1.measurement_id.is_none());
    assert_ne!(groups["Pièce A"][1].1.measurement_id, groups["Pièce B"][0].1.measurement_id);
}

#[test]
fn real_observation_retains_actual_options_and_legacy_history_has_unknown_settings() {
    let mut app = observed_app();
    observe(&mut app, "Pièce A", true);
    let record = app.inspection.records.last().unwrap();
    assert_eq!(serde_json::to_value(record.vision_options.as_ref().unwrap()).unwrap(), serde_json::to_value(actual_options()).unwrap());
    let mut value = serde_json::to_value(record).unwrap();
    value.as_object_mut().unwrap().remove("vision_options");
    let imported: InspectionRecord = serde_json::from_value(value).unwrap();
    assert!(imported.vision_options.is_none());
    assert!(imported.imported);
    let roundtrip: InspectionRecord = serde_json::from_value(serde_json::to_value(record).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(roundtrip.vision_options).unwrap(), serde_json::to_value(&record.vision_options).unwrap());
}

struct EvidenceInteraction {
    app: DperMeasureApp,
    ctx: egui::Context,
    shapes: Vec<egui::epaint::ClippedShape>,
    time: f64,
    grouped: bool,
    selected: Option<u64>,
}

impl EvidenceInteraction {
    fn new(grouped: bool) -> Self {
        let mut app = observed_app();
        observe(&mut app, "Pièce A", true);
        let mut fixture = Self { app, ctx: egui::Context::default(), shapes: Vec::new(), time: 0.0, grouped, selected: None };
        fixture.ctx.style_mut(|style| style.animation_time = 0.0);
        fixture.settle();
        fixture
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0 / 60.0;
        let state = self.app.current_inspection();
        self.shapes = self.ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 2000.0))),
            time: Some(self.time), focused: true, events, ..Default::default()
        }, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if self.grouped {
                    if let Some(id) = grouped_inspection_results(ui, &state, &self.app.measurements) { self.selected = Some(id); }
                } else { measurement_evidence_card(ui, &self.app.measurements[0], &state); }
            });
        }).shapes;
    }

    fn settle(&mut self) { for _ in 0..3 { self.frame(Vec::new()); } }

    fn text_rects(&self, needle: &str) -> Vec<egui::Rect> {
        fn collect(shape: &egui::epaint::Shape, needle: &str, output: &mut Vec<egui::Rect>) {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == needle => output.push(text.galley.rect.translate(text.pos.to_vec2())),
                egui::epaint::Shape::Vec(shapes) => for shape in shapes { collect(shape, needle, output); },
                _ => {}
            }
        }
        let mut output = Vec::new();
        for clipped in &self.shapes { collect(&clipped.shape, needle, &mut output); }
        output
    }

    fn click(&mut self, needle: &str) {
        let rectangles = self.text_rects(needle);
        assert_eq!(rectangles.len(), 1, "Expected one native widget {needle}");
        let pos = rectangles[0].center();
        self.frame(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default(),
        }]);
        self.frame(vec![egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default(),
        }]);
        self.settle();
    }
}

#[test]
fn native_evidence_collapse_reveals_actual_search_and_current_software_thresholds() {
    let mut fixture = EvidenceInteraction::new(false);
    let revision = fixture.app.document_revision;
    let id = fixture.app.measurements[0].id;
    assert_eq!(fixture.text_rects("Vérifiée par logiciel").len(), 1);
    assert!(fixture.text_rects("Réglages de recherche conservés avec cette observation").is_empty());
    fixture.click(&format!("Preuves et limites · M{id}"));
    assert_eq!(fixture.text_rects("Réglages de recherche conservés avec cette observation").len(), 1);
    assert_eq!(fixture.text_rects("Profils : 128 · rayon de recherche : 6.000 px · gradient minimum : 13.000").len(), 1);
    assert_eq!(fixture.text_rects("Points minimum : 8 · RMS maximum : 0.500 px · écart maximum : 1.500 px · couverture minimum : 75.0 %").len(), 1);
    assert_eq!(fixture.text_rects("montage de mesure non qualifié").len(), 1);
    fixture.click(&format!("Preuves et limites · M{id}"));
    assert!(fixture.text_rects("Réglages de recherche conservés avec cette observation").is_empty());
    assert_eq!(fixture.app.document_revision, revision);
}

#[test]
fn native_group_details_and_correction_selection_preserve_distinct_rows() {
    let mut fixture = EvidenceInteraction::new(true);
    observe(&mut fixture.app, "Pièce A", false);
    observe(&mut fixture.app, "Pièce B", true);
    fixture.settle();
    assert_eq!(fixture.text_rects("Groupe : Pièce A · 2 observations · 1 aperçu").len(), 1);
    assert_eq!(fixture.text_rects("Groupe : Pièce B · 1 observation · 0 aperçus").len(), 1);
    let id = fixture.app.measurements[1].id;
    fixture.click(&format!("Sélectionner M{id} pour correction"));
    assert_eq!(fixture.selected, Some(id));
    assert!(fixture.text_rects("Aperçu : aucune mesure native enregistrée.").len() == 1);
}

#[test]
fn native_stale_or_impossible_2d_reasons_remain_visible_with_details_closed() {
    for impossible in [false, true] {
        let mut fixture = EvidenceInteraction::new(false);
        if impossible {
            fixture.app.inspection.plan.as_mut().unwrap().characteristics[0].measurability = crate::dper_drawing::DrawingMeasurability::NotMeasurable2d;
        } else { fixture.app.image.as_mut().unwrap().rgb[0] = 0; }
        fixture.settle();
        let reason = if impossible { "not_observable_in_2d" } else { "source_pixels_changed" };
        assert_eq!(fixture.text_rects(&format!("Suspendu : {}", dper_metrology::reason_label(reason))).len(), 1);
        assert!(fixture.text_rects("Vérifiée par logiciel").is_empty());
        assert!(fixture.text_rects("Réglages de recherche conservés avec cette observation").is_empty());
    }
}

#[test]
fn native_dependency_labels_report_sources_and_staleness_without_promoting_geometry() {
    let mut app = observed_app();
    observe(&mut app, "Pièce A", true);
    let mut measurement = app.measurements[0].clone();
    measurement.source_measurement_ids = vec![10, 20];
    measurement.dependency_stale = true;
    let state = app.current_inspection();
    assert_eq!(dependency_label(&measurement, &state), "Dépendances : M10, M20 · périmées, recalcul requis");
    assert_eq!(measurement_role(&measurement, &state), EvidenceRole::Measured);
}

#[test]
fn foreign_capture_history_cannot_replace_a_measurement_group_or_state() {
    let mut app = observed_app();
    observe(&mut app, "Pièce courante", true);
    let measurement = app.measurements[0].clone();
    let mut foreign = app.inspection.records[0].clone();
    foreign.capture_id = "foreign-capture".into();
    foreign.group = "Autre pièce".into();
    app.inspection.records.push(foreign);
    let state = app.current_inspection();
    assert_eq!(latest_record(&measurement, &state).unwrap().group, "Pièce courante");
    assert_eq!(measurement_role(&measurement, &state), EvidenceRole::SoftwareValidated);
    let foreign_only = InspectionWorkspace { records: vec![state.records.last().unwrap().clone()], ..Default::default() };
    assert!(latest_record(&measurement, &foreign_only).is_none());
    assert_eq!(measurement_role(&measurement, &foreign_only), EvidenceRole::Measured);
}

#[test]
fn suspended_pixel_evidence_never_shows_the_old_value_as_a_current_measurement() {
    let mut app = observed_app();
    observe(&mut app, "Stale value", true);
    let measurement = app.measurements[0].clone();
    let current = app.current_inspection();
    assert_eq!(measurement_display_value(&measurement, &current), crate::geometry::result(&measurement));
    app.image.as_mut().unwrap().rgb[0] ^= 1;
    let stale = app.current_inspection();
    assert_eq!(measurement_display_value(&measurement, &stale), "Valeur suspendue · nouvelle observation requise");
    assert!(suspended_value(stale.records.last().unwrap()));
    let mut imported = current;
    imported.records.last_mut().unwrap().imported = true;
    assert!(measurement_display_value(&measurement, &imported).contains("Valeur suspendue"));
}

#[test]
fn documentary_review_suspension_does_not_hide_valid_observed_contour_value() {
    let mut app = observed_app();
    observe(&mut app, "Documentary scope", true);
    let measurement = app.measurements[0].clone();
    let mut state = app.current_inspection();
    state.records[0].outcome.blocking_reasons.push("decision_evidence_changed".into());
    assert_eq!(measurement_display_value(&measurement, &state), crate::geometry::result(&measurement));
    assert_eq!(measurement_role(&measurement, &state), EvidenceRole::SoftwareValidated);
    state.records[0].outcome.blocking_reasons.push("source_pixels_changed".into());
    assert!(measurement_display_value(&measurement, &state).contains("Valeur suspendue"));
}

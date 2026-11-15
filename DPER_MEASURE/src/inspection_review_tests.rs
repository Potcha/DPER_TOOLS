use super::*;
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[test]
fn feature_review_invalidates_only_its_observations_and_descendants_permanently() {
    use crate::{geometry::Point, dper_inspection, dper_metrology::GeometryState};
    let mut app = DperMeasureApp::new_with_services(egui::Context::default(), false);
    let path = std::env::temp_dir().join(format!("feature-review-{}-{}.pdf", std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }
    let _cleanup = Cleanup(path.clone());
    std::fs::write(&path, b"synthetic drawing source, parser not invoked").unwrap();
    let mut plan = crate::dper_drawing::parse_text_plan("Ø5 ±0.2 mm\nØ6 ±0.2 mm", "review.pdf");
    assert_eq!(plan.characteristics.len(), 2);
    plan.source_file = Some(path.display().to_string());
    plan.source_fingerprint = Some(crate::dper_source::fingerprint_file(&path).unwrap());
    for feature in &mut plan.characteristics { feature.confirmed = true; }
    let features = plan.characteristics.iter().map(|f| f.id.clone()).collect::<Vec<_>>();
    app.install_inspection_plan(plan);
    let image = image::RgbImage::from_fn(128, 128, |x, y| {
        let distance = (x as f64 - 64.0).hypot(y as f64 - 64.0);
        image::Rgb([(20.0 + 200.0 * (distance - 24.5).clamp(0.0, 1.0)) as u8; 3])
    });
    app.image = Some(crate::ImageData { width: 128, height: 128,
        rgb: image.as_raw().clone(), source: "synthetic.png".into() });
    let mut quality = crate::quality::assess_image_quality(image.as_raw(), 128, 128,
        &crate::quality::ImageQualityThresholds::default()).unwrap();
    quality.is_exploitable = true;
    app.quality_assessment = Some(quality);
    app.calibration = Some(serde_json::from_value(serde_json::json!({
        "id":"independent-reference", "device":"fixture", "resolution":[128,128],
        "optical_setup":"fixture", "mm_per_pixel":0.1, "reference_mm":10.0,"reference_px":100.0,
    })).unwrap());
    app.mm_per_pixel = Some(0.1);
    let pairs = [Point{x:10.0,y:10.0},Point{x:30.0,y:10.0},Point{x:10.0,y:30.0}]
        .map(|plan| crate::dper_registration::Correspondence { plan, image: plan });
    app.inspection.registration = Some(crate::dper_registration::register_similarity(&pairs, 0.1).unwrap());
    app.inspection.registration_capture_id = Some(app.capture_id.clone());
    app.inspection.registration_fingerprint = Some(dper_inspection::fingerprint(&image));
    for feature_id in &features {
        app.perform_inspection(crate::mcp_server::InspectFeatureArgs {
            capture_id: app.capture_id.clone(), expected_revision: app.document_revision,
            feature_id: feature_id.clone(), source_measurement_ids: Vec::new(), group: feature_id.clone(),
            geometry_in_drawing: crate::dper_vision::InspectionGeometry::Circle {
                center: Point{x:64.0,y:64.0}, radius:25.0 }, options: None, commit: true,
        }, None, None).unwrap();
    }
    for index in 0..2 {
        let mut record = app.inspection.records[index].clone();
        dper_inspection::attach_decision_evidence_with_catalogs(&mut record, &app.inspection,
            &image, &app.inspection_frame(), &app.measurements, dper_inspection::DecisionBasis {
                qualification_reference:"Synthetic reference".into(), qualification_scope:"Synthetic disk".into(),
                expanded_uncertainty_mm:0.04, uncertainty_source:"Synthetic budget".into(), reviewer:"Test operator".into(),
            }, &app.drawing_geometry_cache).unwrap();
        app.inspection.records[index] = record;
    }
    let roots = app.inspection.records.iter().map(|r| r.measurement_id.unwrap()).collect::<Vec<_>>();
    let children = roots.iter().map(|id| app.add_measurement_with_construction(crate::Tool::Distance,
        vec![Point{x:10.0,y:10.0},Point{x:20.0,y:10.0}], Some(format!("Descendant M{id}")),
        app.capture_id.clone(), None, None, "manual".into(), None, true, vec![*id]).id).collect::<Vec<_>>();
    let before = serde_json::to_value(&app.inspection.records).unwrap();
    let independent_generation = app.inspection.dependencies.revision(roots[1]);
    let independent_child_generation = app.inspection.dependencies.revision(children[1]);
    let independent_outcome = serde_json::to_value(&app.current_inspection().records[1].outcome).unwrap();
    let calibration = serde_json::to_value(&app.calibration).unwrap();
    for confirmed in [false, true] {
        let revision = app.document_revision;
        app.review_native_feature(&features[0], confirmed, revision).unwrap();
        assert_eq!(app.document_revision, revision + 1);
        assert_eq!(app.inspection.dependencies.revision(roots[1]), independent_generation);
        assert_eq!(app.inspection.dependencies.revision(children[1]), independent_child_generation);
        for id in [roots[0], children[0]] {
            assert!(app.measurements.iter().find(|m| m.id == id).unwrap().dependency_stale);
            assert!(app.validate_dependency_sources(&app.capture_id, &[id]).is_err());
        }
        for id in [roots[1], children[1]] {
            assert!(!app.measurements.iter().find(|m| m.id == id).unwrap().dependency_stale);
        }
        let state = app.handle_inspection_request(crate::mcp_server::InspectionRequest::GetState).unwrap();
        assert_ne!(app.current_inspection().records[0].outcome.geometry_state, GeometryState::Validated);
        assert_eq!(serde_json::to_value(&app.current_inspection().records[1].outcome).unwrap(), independent_outcome);
        assert_eq!(state["characteristic_progress"][1]["current_observations"], 1);
        let exported = app.current_export_image().unwrap().session();
        assert!(exported.measurements.iter().find(|m| m.id == roots[0]).unwrap().dependency_stale);
        assert!(!exported.measurements.iter().find(|m| m.id == roots[1]).unwrap().dependency_stale);
        assert_eq!(serde_json::to_value(&app.inspection.records).unwrap(), before);
        assert_eq!(serde_json::to_value(&app.calibration).unwrap(), calibration);
    }
}

#[test]
fn native_review_rejects_stale_revision_active_action_and_source_replacement() {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx, false);
    let path = std::env::temp_dir().join(format!("native-review-{}-{}.pdf", std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }
    let _cleanup = Cleanup(path.clone());
    std::fs::write(&path, b"review source bytes, parser not used").unwrap();
    let mut plan = crate::dper_drawing::parse_text_plan("Diametre 4 mm", "review.pdf");
    plan.source_file = Some(path.display().to_string());
    plan.source_fingerprint = Some(crate::dper_source::fingerprint_file(&path).unwrap());
    app.install_inspection_plan(plan);
    let id = app.inspection.plan.as_ref().unwrap().characteristics[0].id.clone();
    let revision = app.document_revision;
    app.review_native_feature(&id, true, revision).unwrap();
    assert!(app.inspection.plan.as_ref().unwrap().characteristics[0].confirmed);
    assert!(app.review_native_feature(&id, false, revision).is_err());
    let revision = app.document_revision;
    app.pending.push(crate::geometry::Point { x: 1.0, y: 1.0 });
    assert!(app.review_native_feature(&id, false, revision).is_err());
    app.pending.clear();
    assert!(app.review_native_binding("absent", true, revision).is_err());
    let key = app.inspection_presentation_key();
    let metadata = std::fs::metadata(&path).unwrap();
    std::fs::write(&path, vec![b'x'; metadata.len() as usize]).unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_times(
        std::fs::FileTimes::new().set_modified(metadata.modified().unwrap())).unwrap();
    assert_ne!(app.inspection_presentation_key(), key, "Visible evidence key must detect same-size/date PDF replacement");
    assert!(app.review_native_feature(&id, false, revision).is_err());
    assert_eq!(app.document_revision, revision);
    assert!(app.inspection.plan.as_ref().unwrap().characteristics[0].confirmed);
}

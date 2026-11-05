//! Compatibility checks for recovered UI actions against current business rules.
use super::*;

fn app_with_profile() -> DperMeasureApp {
    let mut app = DperMeasureApp::new_with_services(egui::Context::default(), false);
    app.calibration_history = vec![serde_json::from_value(serde_json::json!({
        "id": "recovered-test-profile", "device": "synthetic", "device_id": "synthetic",
        "resolution": [100, 100], "optical_setup": "synthetic",
        "mm_per_pixel": 0.1, "scale_x_mm_per_pixel": 0.1, "scale_y_mm_per_pixel": 0.2,
        "expanded_uncertainty_mm": 0.025, "reference_mm": 10.0, "reference_px": 100.0
    })).unwrap()];
    app
}

fn add_native(app: &mut DperMeasureApp, origin: &str) -> Measurement {
    app.add_measurement_with_style(
        Tool::Distance, vec![Point { x: 10.0, y: 10.0 }, Point { x: 30.0, y: 10.0 }],
        None, app.capture_id.clone(), Some("recovered-test-profile".into()), Some(0.1),
        origin.into(), None, false, Some("Ligne — 2 extrémités".into()),
    )
}

#[test]
fn recovered_creation_preserves_native_profile_uncertainty_without_granting_it_to_vision() {
    let mut app = app_with_profile();
    let native = add_native(&mut app, "interface");
    let vision = add_native(&mut app, "vision");
    assert_eq!(native.expanded_uncertainty_mm, Some(0.025));
    assert_eq!(vision.expanded_uncertainty_mm, None);
    assert_eq!(native.mm_per_pixel_x, Some(0.1));
    assert_eq!(native.mm_per_pixel_y, Some(0.2));
    assert!(!app.inspection.dependencies.is_stale(native.id));
    assert!(app.inspection.dependencies.revision(native.id) > 0);
    assert!(app.mcp_server.is_none());
}

#[test]
fn recovered_pattern_links_source_without_transferring_uncertainty_and_edit_invalidates_descendant() {
    let mut app = app_with_profile();
    let parent = add_native(&mut app, "interface");
    let options = patterns::PatternOptions {
        rows: 1, cols: 2, step_x: 20.0, step_y: 0.0, ..Default::default()
    };
    let mut copies = patterns::generate(&[parent.clone()], patterns::PatternKind::Rectangular, &options).unwrap();
    assert_eq!(copies.len(), 1);
    let mut child = copies.remove(0);
    assert_eq!(child.source_measurement_ids, vec![parent.id]);
    assert_eq!(child.expanded_uncertainty_mm, None);
    assert_eq!(child.origin, "pattern");
    assert_ne!(child.points, parent.points);
    child.id = app.next_id;
    app.next_id += 1;
    let child_id = child.id;
    let index = app.measurements.len();
    app.measurements.push(child.clone());
    app.record_measurement_action(MeasurementHistoryAction::AddedMany {
        measurements: vec![(index, child)], selection: vec![parent.id],
    });
    app.inspection.dependencies.mark_created(child_id);
    app.refresh_dependency_flags();
    assert!(!app.measurements.iter().find(|m| m.id == child_id).unwrap().dependency_stale);
    assert!(app.ensure_dependency_deletion(&[parent.id]).is_err());

    let mut edited = parent.clone();
    edited.points[1].x += 5.0;
    *app.measurements.iter_mut().find(|m| m.id == parent.id).unwrap() = edited.clone();
    app.record_measurement_action(MeasurementHistoryAction::Edited {
        id: parent.id, before: parent, after: edited,
    });
    let descendant = app.measurements.iter().find(|m| m.id == child_id).unwrap();
    assert!(descendant.dependency_stale);
    assert!(geometry::result(descendant).contains("À recalculer"));
    assert!(app.validate_dependency_sources(&app.capture_id, &[child_id]).is_err());
    assert!(app.undo_measurement_action());
    assert!(app.measurements.iter().find(|m| m.id == child_id).unwrap().dependency_stale,
        "Restoring geometry through history must not restore dependency authority");
}

#[test]
fn recovered_oriented_rectangle_variant_survives_measurement_serde() {
    let mut app = app_with_profile();
    let variant = drawing_variants::DrawingVariant::Rectangle3;
    let (kind, points) = variant.resolve(&[
        Point { x: 10.0, y: 10.0 }, Point { x: 30.0, y: 20.0 }, Point { x: 20.0, y: 40.0 },
    ], 10.0).unwrap();
    assert_eq!(kind, Tool::Polygon);
    let measurement = app.add_measurement_with_style(kind, points, None, app.capture_id.clone(),
        Some("recovered-test-profile".into()), Some(0.1), "interface".into(), None,
        false, Some(variant.label().into()));
    let value = serde_json::to_value(&measurement).unwrap();
    assert_eq!(value["drawing_variant"], variant.label());
    let reopened: Measurement = serde_json::from_value(value).unwrap();
    assert_eq!(reopened.drawing_variant.as_deref(), Some(variant.label()));
    assert_eq!(reopened.points, measurement.points);
    assert_eq!(reopened.kind, Tool::Polygon);
    assert_eq!(geometry::result(&reopened), geometry::result(&measurement));
    assert!(geometry::result(&reopened).starts_with("L "));
}

#[test]
fn recovered_legacy_measurement_without_drawing_variant_remains_readable() {
    let legacy = serde_json::json!({
        "id": 4, "kind": "Distance", "points": [{"x": 0.0, "y": 0.0}, {"x": 3.0, "y": 4.0}],
        "label": "Legacy distance"
    });
    let measurement: Measurement = serde_json::from_value(legacy).unwrap();
    assert_eq!(measurement.drawing_variant, None);
    assert!(measurement.source_measurement_ids.is_empty());
    assert!(!measurement.dependency_stale);
    assert_eq!(geometry::result(&measurement), "5.0 px (non étalonné)");
    assert!(serde_json::to_value(&measurement).unwrap().get("drawing_variant").is_none());
}

#[test]
fn recovered_ui_keeps_current_session_schema_seventeen() {
    assert_eq!(storage::CURRENT_SCHEMA_VERSION, 17);
}

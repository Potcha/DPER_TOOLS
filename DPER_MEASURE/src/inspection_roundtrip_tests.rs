// Included beneath dper_inspection::tests; all imagery and calibration are synthetic.
fn observed_roundtrip() -> (RgbImage, InspectionWorkspace, FrameContext, InspectionRecord, Measurement) {
    let image = RgbImage::from_fn(240, 180, |x, y| {
        let distance = (f64::from(x) - 180.0).hypot(f64::from(y) - 90.0);
        Rgb([(20.0 + 200.0 * (distance - 22.0).clamp(0.0, 1.0)) as u8; 3])
    });
    let workspace = workspace(&image, "Ø4.5 ±0.5 mm", 1.0);
    let mut frame = frame();
    frame.calibration.as_mut().unwrap().resolution = [240, 180];
    let mut record = inspect_feature(&workspace, &image, &frame, "drawing-1", "JSON roundtrip",
        &InspectionGeometry::Circle { center: Point { x: 180.0, y: 90.0 }, radius: 20.0 },
        &VisionOptions::default()).unwrap();
    record.measurement_id = Some(1);
    record.measurement_revision = Some(0);
    let native = measurement(&record);
    assert_eq!(assess_record(&record, &workspace, &image, &frame, &[native.clone()]).geometry_state,
        GeometryState::Validated);
    let restored = serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    let native = serde_json::from_str(&serde_json::to_string(&native).unwrap()).unwrap();
    (image, workspace, frame, restored, native)
}

#[test]
fn known_decimal_rounding_is_historical_numerical_noise_only() {
    let original = 202.50703260036482_f64;
    let parsed: f64 = serde_json::from_str("202.50703260036482").unwrap();
    assert!(recomputed_number_matches(parsed, original, true));
    let next = f64::from_bits(original.to_bits() + 1);
    assert!(recomputed_number_matches(original, next, true));
    assert!(!recomputed_number_matches(original, next, false));
    // The same bound applies to a recomputed physical value, not a physical
    // tolerance: it never supplies U, qualification or current pixel evidence.
    let value = 4.501406520072964_f64;
    assert!(recomputed_number_matches(value, f64::from_bits(value.to_bits() + 1), true));
    assert!(!recomputed_number_matches(value, value + 1e-10, true));
    assert!(!recomputed_number_matches(original, original + 1e-10, true));
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(!recomputed_number_matches(invalid, invalid, true));
        assert!(!recomputed_number_matches(original, invalid, true));
    }
}

#[test]
fn observed_json_roundtrip_does_not_invent_edits_or_restore_import_authority() {
    let (image, workspace, frame, record, native) = observed_roundtrip();
    let outcome = assess_record(&record, &workspace, &image, &frame, &[native]);
    assert!(record.imported);
    assert_ne!(outcome.geometry_state, GeometryState::Validated);
    assert_eq!(outcome.conformity_status, ConformityStatus::Indeterminate);
    assert!(outcome.blocking_reasons.contains(&"imported_evidence_requires_reassessment".into()));
    assert!(!outcome.blocking_reasons.contains(&"inspection_points_changed".into()));
    assert!(!outcome.blocking_reasons.contains(&"physical_value_changed".into()));
}

#[test]
fn genuine_historical_point_or_value_changes_remain_explicit() {
    let (image, workspace, frame, record, native) = observed_roundtrip();
    let mut shifted = record.clone();
    shifted.measurement_points[1].x += 1e-8;
    let outcome = assess_record(&shifted, &workspace, &image, &frame, &[native.clone()]);
    assert!(outcome.blocking_reasons.contains(&"inspection_points_changed".into()));
    assert_ne!(outcome.geometry_state, GeometryState::Validated);
    let mut shifted = record;
    shifted.value_mm = Some(shifted.value_mm.unwrap() + 1e-8);
    assert!(assess_record(&shifted, &workspace, &image, &frame, &[native])
        .blocking_reasons.contains(&"physical_value_changed".into()));
}

#[test]
fn live_observation_keeps_exact_point_and_value_identity() {
    let (image, workspace, frame, mut record, _) = observed_roundtrip();
    // Establish an exact locally computed baseline; changing an import flag is
    // confined to this test and does not create review authority in production.
    record.imported = false;
    record.measurement_points = native_measurement_points(&record.result);
    let candidate = &workspace.plan.as_ref().unwrap().characteristics[0];
    record.value_mm = physical_value(candidate, &record.result, &image, &frame).unwrap();
    let native = measurement(&record);
    let mut changed = record.clone();
    changed.measurement_points[1].x = f64::from_bits(changed.measurement_points[1].x.to_bits() + 1);
    assert!(assess_record(&changed, &workspace, &image, &frame, &[native.clone()])
        .blocking_reasons.contains(&"inspection_points_changed".into()));
    let mut changed = record;
    changed.value_mm = Some(f64::from_bits(changed.value_mm.unwrap().to_bits() + 1));
    assert!(assess_record(&changed, &workspace, &image, &frame, &[native])
        .blocking_reasons.contains(&"physical_value_changed".into()));
}

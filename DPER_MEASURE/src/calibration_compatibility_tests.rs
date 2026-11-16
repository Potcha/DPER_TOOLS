//! DPM-47: LLT-MET-002-01, calibration compatibility.
use super::*;

fn fixture() -> (DperMeasureApp, egui::Context) {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    app.active_device = "Camera".into();
    app.active_device_id = "camera-a".into();
    app.optical_setup = "lens-a".into();
    app.image = Some(ImageData { width: 20, height: 20, rgb: vec![0; 1200], source: "test".into() });
    let profile: Calibration = serde_json::from_value(serde_json::json!({
        "id": "cal-a", "device": "Camera", "device_id": "camera-a",
        "resolution": [20, 20], "optical_setup": "lens-a",
        "mm_per_pixel": 0.1, "reference_mm": 1.0, "reference_px": 10.0
    })).unwrap();
    app.settings.calibrations = vec![profile];
    app.settings.protocols = vec![storage::MeasurementProtocol {
        id: "protocol-a".into(), name: "test".into(), version: 1, part_family: String::new(),
        device: "Camera".into(), device_id: "camera-a".into(), optical_setup: "lens-a".into(),
        resolution: [20, 20], calibration_profile_id: Some("cal-a".into()),
        mm_per_pixel: Some(0.1), scale_x_mm_per_pixel: None, scale_y_mm_per_pixel: None,
        expanded_uncertainty_mm: None, quality_thresholds: Default::default(),
        display_correction: Default::default(), expected_measurements: vec![],
        display_precision_mm: 3, tolerance_rule: storage::ToleranceRule::ExpandedUncertaintyGuardBand,
        created_at_unix_secs: 0,
    }];
    (app, ctx)
}

#[test]
fn protocol_requires_matching_camera_resolution_and_optics() {
    let (mut app, ctx) = fixture();
    app.activate_protocol("protocol-a", &ctx);
    assert_eq!(app.mm_per_pixel, Some(0.1));
    for mismatch in ["camera", "resolution", "optics", "unknown"] {
        let (mut app, ctx) = fixture();
        match mismatch {
            "camera" => app.active_device_id = "camera-b".into(),
            "resolution" => {
                let image = app.image.as_mut().unwrap();
                image.width = 19;
                image.rgb.truncate(19 * 20 * 3);
            }
            "optics" => app.optical_setup = "lens-b".into(),
            _ => app.active_device_id = "imported-image".into(),
        }
        app.activate_protocol("protocol-a", &ctx);
        assert!(app.calibration.is_none(), "{mismatch}");
        assert_eq!(app.mm_per_pixel, None, "{mismatch}");
        if mismatch == "optics" {
            assert_eq!(app.optical_setup, "lens-b");
            app.activate_protocol("protocol-a", &ctx);
            assert_eq!(app.mm_per_pixel, None);
        }
    }
}

#[test]
fn automatic_camera_selection_uses_identity_and_rejects_legacy_unknown_camera() {
    let (mut app, _) = fixture();
    let mut other = app.settings.calibrations[0].clone();
    other.id = "cal-b".into();
    other.device_id = "camera-b".into();
    other.mm_per_pixel = 0.2;
    app.settings.calibrations.push(other);
    app.activate_saved_calibration();
    assert_eq!(app.mm_per_pixel, Some(0.1));
    app.active_device_id = "camera-b".into();
    app.activate_saved_calibration();
    assert_eq!(app.mm_per_pixel, Some(0.2));
    app.settings.calibrations[1].device_id.clear();
    app.activate_saved_calibration();
    assert_eq!(app.mm_per_pixel, None);
}

#[test]
fn imports_of_same_size_do_not_inherit_camera_or_prior_calibration() {
    let (mut app, ctx) = fixture();
    app.activate_protocol("protocol-a", &ctx);
    let dir = std::env::temp_dir().join(new_id("dpm47-import"));
    std::fs::create_dir(&dir).unwrap();
    let first = dir.join("first.png");
    let second = dir.join("second.png");
    app.capture_photo_to_path(&first, false).unwrap();
    image::RgbImage::from_pixel(20, 20, image::Rgb([255, 255, 255])).save(&second).unwrap();
    app.session_saved = true;
    app.open_image_path(&ctx, &first).unwrap();
    assert_eq!(app.active_device_id, "camera-a");
    assert_eq!(app.optical_setup, "lens-a");
    assert_eq!(app.mm_per_pixel, None);
    let profile = app.settings.calibrations.pop().unwrap();
    let mut incompatible = profile.clone();
    incompatible.optical_setup = "lens-b".into();
    app.settings.calibrations.push(incompatible);
    assert!(app.apply_saved_calibration_to_import().is_err());
    assert_eq!(app.mm_per_pixel, None);
    app.settings.calibrations.push(profile);
    app.apply_saved_calibration_to_import().unwrap();
    assert_eq!(app.mm_per_pixel, Some(0.1));
    let first_measurement = app.add_measurement_with_style(Tool::Distance,
        vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }], None,
        app.capture_id.clone(), app.calibration.as_ref().map(|profile| profile.id.clone()),
        app.mm_per_pixel, "interface".into(), None, false, None);
    assert!(geometry::result(&first_measurement).contains("1.000 mm"));
    app.session_saved = true;
    let derived = dir.join("derived.png");
    app.save_derived_image(&ctx, derived.clone(), image::RgbImage::new(20, 20),
        serde_json::json!({"calibration_carried_over": false}));
    assert_eq!(app.active_device_id, "imported-image");
    assert!(app.calibration.is_none());
    assert!(app.apply_saved_calibration_to_import().is_err());
    app.calibrate_from_points(CalibrationType::Segment,
        &[Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }],
        2.0, "interface", None).unwrap();
    assert_eq!(app.mm_per_pixel, Some(0.2));
    app.session_saved = true;
    app.open_image_path(&ctx, &second).unwrap();
    assert_eq!(app.active_device_id, "imported-image");
    assert!(app.apply_saved_calibration_to_import().is_err());
    app.activate_protocol("protocol-a", &ctx);
    assert_eq!(app.mm_per_pixel, None);
    assert!(app.calibration.is_none());
    let second_measurement = app.add_measurement_with_style(Tool::Distance,
        vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }], None,
        app.capture_id.clone(), None, app.mm_per_pixel, "interface".into(), None, false, None);
    assert!(geometry::result(&second_measurement).contains("10.0 px"));
    std::fs::remove_file(first).unwrap();
    std::fs::remove_file(second).unwrap();
    std::fs::remove_file(derived).unwrap();
    std::fs::remove_file(dir.join("derived.dper_measure.json")).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn hlt_met_002_same_camera_and_resolution_different_optical_scales() {
    let (mut app, ctx) = fixture();
    let dir = std::env::temp_dir().join(new_id("dpm221-hlt"));
    std::fs::create_dir(&dir).unwrap();
    let first = dir.join("lens-a.png");
    let second = dir.join("lens-b.png");
    let mut save_mire = |path: &Path, optics: &str, span: u32| {
        app.optical_setup = optics.into();
        let mire = image::RgbImage::from_fn(20, 20, |x, y| {
            let level = if x == 4 || x == 4 + span { 215 } else if y % 4 < 2 { 65 } else { 135 };
            image::Rgb([level; 3])
        });
        app.image.as_mut().unwrap().rgb = mire.into_raw();
        app.capture_photo_to_path(path, false).unwrap();
    };
    // The same synthetic 1 mm reference spans 10 px with lens A and 5 px with lens B.
    save_mire(&first, "lens-a", 10);
    save_mire(&second, "lens-b", 5);
    app.edge_snap_enabled = false;

    let mut results = Vec::new();
    for (path, optics, span, apply_profile, expected) in [
        (&first, "lens-a", 10.0, false, "10.0 px (non étalonné)"),
        (&first, "lens-a", 10.0, true, "1.000 mm"),
        (&second, "lens-b", 5.0, false, "5.0 px (non étalonné)"),
    ] {
        app.session_saved = true;
        app.open_image_path(&ctx, path).unwrap();
        assert_eq!(app.active_device_id, "camera-a");
        assert_eq!(app.optical_setup, optics);
        assert!(app.calibration.is_none());
        if apply_profile {
            app.apply_saved_calibration_to_import().unwrap();
        }
        app.on_click(Point { x: 4.0, y: 10.0 });
        app.on_click(Point { x: 4.0 + span, y: 10.0 });
        let measurement = app.measurements.last().unwrap_or_else(|| panic!("{}", app.message));
        let result = geometry::result(measurement);
        assert_eq!(result, expected);
        results.push(result);
    }
    println!("HLT-MET-002, captures 20 × 20 px : {}", results.join(" → "));
    std::fs::remove_file(first).unwrap();
    std::fs::remove_file(second).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

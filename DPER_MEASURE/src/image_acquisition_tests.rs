//! DPM-87 / LLT-ACQ-003-01.
use super::*;

// DPM-91 / DPM-171 — LLT-IMG-003-01.
#[test]
fn llt_img_003_rectified_png_and_project_preserve_provenance_without_source_scale() {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    let dir = std::env::temp_dir().join(new_id("dpm91-rectification"));
    fs::create_dir(&dir).unwrap();
    let source = dir.join("target.png");
    let destination = dir.join("rectified.png");
    let target = image::RgbImage::from_fn(80, 64, |x, y| {
        image::Rgb([if (x / 8 + y / 8) % 2 == 0 { 48 } else { 208 }; 3])
    });
    target.save(&source).unwrap();
    let original_bytes = fs::read(&source).unwrap();
    app.open_image_path(&ctx, &source).unwrap();
    let capture_id = app.capture_id.clone();
    let profile = app.calibrate_from_points(
        CalibrationType::Segment,
        &[Point { x: 10.0, y: 16.0 }, Point { x: 60.0, y: 16.0 }],
        5.0, "mcp", None,
    ).unwrap();
    assert_eq!(app.mm_per_pixel, Some(0.1));
    app.display_correction.brightness = 7.0;
    app.session_saved = true;
    let corners = [(6.0, 8.0), (70.0, 10.0), (66.0, 57.0), (10.0, 55.0)];
    app.save_rectified_image(&ctx, destination.clone(), &target, &source.to_string_lossy(), corners);
    let manifest = app.image_processing.clone().expect(&app.message);
    assert!(app.calibration.is_none());
    assert_eq!(app.mm_per_pixel, None);
    assert_ne!(app.capture_id, capture_id);
    assert_eq!(manifest["processing"], "planar_projective_rectification");
    assert_eq!(manifest["source_image"], source.to_string_lossy().as_ref());
    assert_eq!(manifest["source_capture_id"], capture_id.as_str());
    assert_eq!(manifest["source_calibration_id"], profile.id.as_str());
    assert_eq!(manifest["source_display_correction"]["brightness"], 7.0);
    assert_eq!(manifest["display_correction_applied"], false);
    assert_eq!(manifest["source_corners_px"], serde_json::json!(corners));
    let (expected_image, expected_transform) = imaging::rectify_planar(&target, corners).unwrap();
    let transform = manifest["destination_to_source_homography"].as_array().unwrap();
    assert_eq!(transform.len(), expected_transform.len());
    for (stored, expected) in transform.iter().zip(expected_transform) {
        assert!((stored.as_f64().unwrap() - expected).abs() < 1e-12);
    }
    assert_eq!(manifest["calibration_status"], "recalibration_required");
    assert_eq!(image::open(&destination).unwrap().to_rgb8(), expected_image);
    let sidecar = capture_png::processing_sidecar(&destination);
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(&sidecar).unwrap()).unwrap();
    assert_eq!(stored, manifest);

    let moved = dir.join("moved.png");
    fs::rename(&destination, &moved).unwrap();
    app.open_image_path(&ctx, &moved).unwrap();
    assert_eq!(app.image_processing.as_ref(), Some(&manifest));
    let copy = dir.join("copy.png");
    app.capture_photo_to_path(&copy, false).unwrap();
    assert_eq!(capture_png::read_processing(&copy, [expected_image.width(), expected_image.height()]), Some(manifest.clone()));
    let project = dir.join("derived.dperproj");
    app.save_project_to_path(&project, false).unwrap();
    fs::remove_file(moved).unwrap();
    let mut reopened = DperMeasureApp::new_with_services(ctx.clone(), false);
    reopened.open_project_path(&ctx, &project).unwrap();
    assert_eq!(reopened.image_processing.as_ref(), Some(&manifest));
    assert_eq!(reopened.display_correction.brightness, 7.0);
    assert!(reopened.calibration.is_none());
    assert_eq!(reopened.mm_per_pixel, None);
    assert_eq!(fs::read(&source).unwrap(), original_bytes);
    reopened.open_image_path(&ctx, &source).unwrap();
    assert!(reopened.image_processing.is_none());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn llt_img_003_reads_legacy_sidecar_only_for_the_matching_png() {
    let dir = std::env::temp_dir().join(new_id("dpm91-legacy"));
    fs::create_dir(&dir).unwrap();
    let path = dir.join("legacy.png");
    image::RgbImage::new(8, 6).save(&path).unwrap();
    let manifest = serde_json::json!({
        "format_version": 1,
        "processing": "focus_stack_local_variance",
        "source_images": ["first.png", "second.png"],
        "output_file": "legacy.png",
        "output_dimensions": [8, 6],
        "calibration_carried_over": false
    });
    fs::write(capture_png::processing_sidecar(&path), serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(capture_png::read_processing(&path, [8, 6]), Some(manifest));
    assert!(capture_png::read_processing(&path, [6, 8]).is_none());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn llt_img_002_ambiguous_click_waits_for_manual_correction() {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    let image = image::RgbImage::from_fn(32, 32, |x, _| {
        let bright = (8..20).contains(&x) || x >= 24;
        image::Rgb([if bright { 205 } else { 45 }; 3])
    });
    app.set_image(&ctx, ImageData {
        width: 32, height: 32, rgb: image.into_raw(), source: "dpm89-edge-target".into(),
    }, true);
    app.edge_snap_enabled = true;
    app.edge_snap_radius = 6;

    app.on_click(Point { x: 7.0, y: 16.0 });
    assert_eq!(app.pending.len(), 1, "{}", app.message);
    assert!((app.pending[0].x - 7.5).abs() < 0.1);

    let ambiguous = Point { x: 22.0, y: 16.0 };
    app.on_click(ambiguous);
    assert_eq!(app.pending.len(), 1);
    assert!(app.measurements.is_empty());
    assert!(app.message.contains("bords concurrents"));

    app.edge_snap_enabled = false;
    let manual = Point { x: 23.0, y: 16.0 };
    app.on_click(manual);
    assert_eq!(app.measurements.len(), 1, "{}", app.message);
    assert!((app.measurements[0].points[0].x - 7.5).abs() < 0.1);
    assert_eq!(app.measurements[0].points[1], manual);
}

// DPM-86 / DPM-176 — LLT-IMG-001-01.
#[test]
fn llt_img_001_protocol_blocks_measurement_and_preserves_source() {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    let source = std::env::temp_dir().join(format!("{}.png", new_id("dpm86-quality")));
    let image = image::RgbImage::from_fn(16, 16, |x, _| {
        image::Rgb([((x / 4) * 64) as u8; 3])
    });
    image.save(&source).unwrap();
    let file_before = fs::read(&source).unwrap();
    app.open_image_path(&ctx, &source).unwrap();

    let strict = storage::MeasurementProtocol {
        id: "dpm86-strict".into(),
        name: "Protocole strict".into(),
        version: 1,
        part_family: "Pièce sombre".into(),
        device: String::new(),
        device_id: String::new(),
        optical_setup: String::new(),
        resolution: [16, 16],
        calibration_profile_id: None,
        mm_per_pixel: None,
        scale_x_mm_per_pixel: None,
        scale_y_mm_per_pixel: None,
        expanded_uncertainty_mm: None,
        quality_thresholds: ImageQualityThresholds {
            min_mean_luminance: 100.0,
            min_contrast_std_dev: 80.0,
            ..Default::default()
        },
        display_correction: DisplayCorrection::default(),
        expected_measurements: Vec::new(),
        display_precision_mm: 3,
        tolerance_rule: storage::ToleranceRule::ExpandedUncertaintyGuardBand,
        created_at_unix_secs: 0,
    };
    let standard = storage::MeasurementProtocol {
        id: "dpm86-standard".into(),
        name: "Protocole standard".into(),
        quality_thresholds: ImageQualityThresholds::default(),
        ..strict.clone()
    };
    app.settings.protocols.extend([strict, standard]);
    let capture_id = app.capture_id.clone();
    let measurement = || mcp_server::MeasurementInput {
        is_construction: false,
        source_measurement_ids: Vec::new(),
        kind: "distance".into(),
        points: vec![
            mcp_server::NativePoint { x: 3.0, y: 5.0 },
            mcp_server::NativePoint { x: 12.0, y: 5.0 },
        ],
        label: None,
        method: None,
    };

    app.activate_protocol("dpm86-strict", &ctx);
    assert!(!app.inspection_frame().quality_pass);
    let error = app.apply_mcp_measurements(&capture_id, None, vec![measurement()]).err().unwrap();
    assert!(error.contains("Sous-exposition"), "{error}");
    assert!(error.contains("Contraste insuffisant"), "{error}");
    assert!(app.measurements.is_empty());

    app.activate_protocol("dpm86-standard", &ctx);
    assert!(app.inspection_frame().quality_pass);
    let added = app.apply_mcp_measurements(&capture_id, None, vec![measurement()]).unwrap();
    assert_eq!(added.len(), 1);
    let file_after = fs::read(&source).unwrap();
    fs::remove_file(source).unwrap();
    assert_eq!(&app.image.as_ref().unwrap().rgb, image.as_raw());
    assert_eq!(file_after, file_before);
}

fn pointer_button(pos: Pos2, button: PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton { pos, button, pressed, modifiers: Default::default() }
}

#[test]
fn four_image_formats_keep_native_measurements_and_pixels_through_navigation_and_export() {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    let dir = std::env::temp_dir().join(new_id("dpm87-images"));
    fs::create_dir(&dir).unwrap();
    let (width, height) = (96, 64);
    let reference = image::RgbImage::from_fn(width, height, |x, y| {
        let level = if (x / 8 + y / 8) % 2 == 0 { 64 } else { 192 };
        image::Rgb([level, level + 12, level + 24])
    });
    let points = [Point { x: 20.0, y: 20.0 }, Point { x: 50.0, y: 20.0 }];
    let mut time = 0.0;
    let mut frame = |app: &mut DperMeasureApp, events| {
        time += 1.0 / 60.0;
        let _ = ctx.run(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 650.0))),
            focused: true, time: Some(time), events, ..Default::default()
        }, |ctx| {
            app.begin_registration_photo_pick_frame(ctx);
            egui::CentralPanel::default().show(ctx, |ui| app.central_image(ui));
        });
        ctx.data(|d| d.get_temp::<Rect>(egui::Id::new("measurement-canvas-viewport"))).unwrap()
    };

    for extension in ["png", "jpg", "bmp", "tiff"] {
        let source = dir.join(format!("source.{extension}"));
        reference.save(&source).unwrap();
        let original = fs::read(&source).unwrap();
        let decoded = image::open(&source).unwrap().to_rgb8();
        if extension == "jpg" {
            for (x, y) in [(4, 4), (12, 4), (28, 28)] {
                for (actual, expected) in decoded.get_pixel(x, y).0.iter().zip(reference.get_pixel(x, y).0) {
                    assert!(actual.abs_diff(expected) <= 5, "JPEG fixture pixel ({x}, {y})");
                }
            }
        } else {
            assert_eq!(decoded, reference, "{extension}");
        }
        app.session_saved = true;
        app.open_image_path(&ctx, &source).unwrap();
        app.edge_snap_enabled = false;
        app.fit_view = false;
        app.zoom = 3.0;
        let viewport = frame(&mut app, vec![]);
        let scale = app.zoom / ctx.pixels_per_point();
        let image_rect = Rect::from_center_size(viewport.center() + app.pan,
            Vec2::new(width as f32 * scale, height as f32 * scale));
        for point in points {
            let pos = DperMeasureApp::point_to_screen(point, image_rect, width as f32, height as f32);
            frame(&mut app, vec![egui::Event::PointerMoved(pos), pointer_button(pos, PointerButton::Primary, true)]);
            frame(&mut app, vec![pointer_button(pos, PointerButton::Primary, false)]);
        }
        assert_eq!(app.measurements.len(), 1, "{extension}: {}", app.message);
        for (actual, expected) in app.measurements[0].points.iter().zip(points) {
            assert!(actual.dist(expected) < 0.0001, "{extension}");
        }
        let before = app.measurements[0].points.clone();
        let value = geometry::result(&app.measurements[0]);
        assert_eq!(value, "30.0 px (non étalonné)");

        let start = viewport.center();
        let zoom = app.zoom;
        frame(&mut app, vec![egui::Event::PointerMoved(start), egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point, delta: Vec2::new(0.0, 190.0), modifiers: Default::default()
        }]);
        assert!(app.zoom > zoom);
        let pan = app.pan;
        frame(&mut app, vec![pointer_button(start, PointerButton::Middle, true)]);
        let end = start + Vec2::new(24.0, 20.0);
        frame(&mut app, vec![egui::Event::PointerMoved(end)]);
        frame(&mut app, vec![pointer_button(end, PointerButton::Middle, false)]);
        assert_ne!(app.pan, pan);
        assert_eq!(app.measurements[0].points, before);
        assert_eq!(geometry::result(&app.measurements[0]), value);

        let output = dir.join(format!("capture-{extension}.out"));
        let png = output.with_extension("png");
        assert_eq!(PathBuf::from(app.capture_photo_to_path(&output, false).unwrap()), png);
        assert!(fs::read(&png).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(image::open(&png).unwrap().to_rgb8(), decoded);
        assert_eq!(app.image.as_ref().unwrap().source, source.to_string_lossy());
        assert_eq!(fs::read(&source).unwrap(), original);
        fs::remove_file(png).unwrap();
        fs::remove_file(source).unwrap();
    }
    fs::remove_dir(dir).unwrap();
}

#[test]
fn export_cannot_overwrite_imported_png_even_via_a_hard_link() {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    let dir = std::env::temp_dir().join(new_id("dpm87-source"));
    fs::create_dir(&dir).unwrap();
    let source = dir.join("source.png");
    let alias = dir.join("alias.png");
    image::GrayImage::from_pixel(8, 8, image::Luma([113])).save(&source).unwrap();
    fs::hard_link(&source, &alias).unwrap();
    let original = fs::read(&source).unwrap();
    app.open_image_path(&ctx, &source).unwrap();
    for path in [&source, &alias] {
        let error = app.capture_photo_to_path(path, true).unwrap_err();
        assert!(error.contains("Impossible de remplacer l’image source"), "{error}");
    }
    assert_eq!(fs::read(&source).unwrap(), original);
    assert_eq!(fs::read(&alias).unwrap(), original);
    fs::remove_file(alias).unwrap();
    fs::remove_file(source).unwrap();
    fs::remove_dir(dir).unwrap();
}

//! DPM-87 / LLT-ACQ-003-01.
use super::*;

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

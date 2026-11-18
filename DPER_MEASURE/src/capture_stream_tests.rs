//! DPM-88 / LLT-ACQ-002-01.
use super::*;

fn frame(marker: u8) -> camera::Frame {
    let (width, height) = (96, 64);
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for y in 0..height {
        for x in 0..width {
            let value = if (x / 8 + y / 8) % 2 == 0 { 64 } else { 192 };
            rgb.extend_from_slice(&[value, value, value]);
        }
    }
    rgb[0] = marker;
    camera::Frame { width, height, frame_rate: 30, rgb }
}

fn streaming_app(ctx: &egui::Context) -> DperMeasureApp {
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    let (task, _) = camera::simulated_open();
    app.camera_task = Some(task);
    app.running = true;
    app.paused = false;
    app.image_is_imported = false;
    app.edge_snap_enabled = false;
    app
}

#[test]
fn slow_consumer_keeps_only_latest_frame_and_frozen_project_survives_resume() {
    let ctx = egui::Context::default();
    let mut app = streaming_app(&ctx);
    for marker in (1..=100).cycle().take(2000) {
        app.camera_task.as_ref().unwrap().publish_test_frame(frame(marker));
    }
    app.poll_camera(&ctx);
    assert_eq!(app.image.as_ref().unwrap().rgb[0], 100);
    assert!(app.camera_task.as_ref().unwrap().take_latest_frame().is_none());

    app.control_mcp_stream("freeze", None).unwrap();
    let frozen_id = app.capture_id.clone();
    let frozen_pixels = app.image.as_ref().unwrap().rgb.clone();
    for marker in 101..=120 {
        app.camera_task.as_ref().unwrap().publish_test_frame(frame(marker));
        app.poll_camera(&ctx);
        assert_eq!(app.capture_id, frozen_id);
        assert_eq!(app.image.as_ref().unwrap().rgb, frozen_pixels);
    }
    app.on_click(Point { x: 20.0, y: 20.0 });
    app.on_click(Point { x: 50.0, y: 20.0 });
    assert_eq!(app.measurements.len(), 1, "{}", app.message);
    let measurement = app.measurements[0].clone();
    assert_eq!(measurement.capture_id, frozen_id);
    assert_eq!(geometry::result(&measurement), "30.0 px (non étalonné)");
    assert!(app.control_mcp_stream("resume", None).is_err());

    let dir = std::env::temp_dir().join(new_id("dpm88-frozen"));
    let project = dir.join("capture.dperproj");
    app.save_project_to_path(&project, false).unwrap();
    app.camera_task.as_ref().unwrap().publish_test_frame(frame(121));
    app.control_mcp_stream("resume", None).unwrap();
    assert!(!app.paused);
    assert_ne!(app.capture_id, frozen_id);
    assert!(app.measurements.is_empty());
    app.poll_camera(&ctx);
    assert!(app.image.is_none(), "a frame queued during the freeze must not become live");
    assert!(app.control_mcp_stream("freeze", None).is_err());
    app.camera_task.as_ref().unwrap().publish_test_frame(frame(122));
    app.poll_camera(&ctx);
    assert_eq!(app.image.as_ref().unwrap().rgb[0], 122);

    let mut reopened = DperMeasureApp::new_with_services(ctx.clone(), false);
    reopened.open_project_path(&ctx, &project).unwrap();
    assert_eq!(reopened.capture_id, frozen_id);
    assert_eq!(reopened.image.as_ref().unwrap().rgb, frozen_pixels);
    assert_eq!(reopened.measurements[0].capture_id, frozen_id);
    assert_eq!(reopened.measurements[0].points, measurement.points);
    assert_eq!(geometry::result(&reopened.measurements[0]), geometry::result(&measurement));
    fs::remove_file(&reopened.image.as_ref().unwrap().source).unwrap();
    fs::remove_file(project).unwrap();
    fs::remove_dir(dir).unwrap();
}

#[test]
fn manual_freeze_cancels_delayed_photo_before_it_can_replace_the_frozen_pixels() {
    let ctx = egui::Context::default();
    let mut app = streaming_app(&ctx);
    app.camera_task.as_ref().unwrap().publish_test_frame(frame(80));
    app.poll_camera(&ctx);
    let destination = std::env::temp_dir().join(format!("{}.png", new_id("dpm88-delayed")));
    app.capture_timer = Some(CaptureTimer {
        destination: destination.clone(), capture_at: Instant::now(), best_frame: Some(frame(99)),
        best_sharpness: 0.0, frames_seen: 1,
    });
    app.control_mcp_stream("freeze", None).unwrap();
    let frozen_id = app.capture_id.clone();
    app.poll_capture_timer(&ctx);
    assert!(app.capture_timer.is_none());
    assert!(app.paused);
    assert_eq!(app.capture_id, frozen_id);
    assert_eq!(app.image.as_ref().unwrap().rgb[0], 80);
    assert!(!destination.exists());
}

//! DPM-84 / LLT-ACQ-001-01.
use super::*;
use nokhwa::utils::{CameraFormat, CameraIndex, FrameFormat};

#[test]
fn homonymous_camera_rejection_preserves_the_active_capture() {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    let active_mode = CameraFormat::new_from(1920, 1080, FrameFormat::MJPEG, 30);
    let requested = CameraFormat::new_from(1280, 720, FrameFormat::YUYV, 15);
    app.cameras = vec![
        (CameraIndex::Index(0), "Microscope".into(), "uvc-a".into()),
        (CameraIndex::Index(1), "Microscope".into(), "uvc-b".into()),
    ];
    app.active_device = "Microscope".into();
    app.active_device_id = "uvc-a".into();
    app.camera_frame_rate = 30;
    app.image = Some(ImageData { width: 1, height: 1, rgb: vec![17, 18, 19], source: "uvc-a".into() });
    app.camera_modes_camera = Some("uvc-b".into());
    app.camera_modes = vec![camera::CameraMode { format: requested, label: "requested".into() }];
    app.selected_camera_mode = Some(0);
    app.running = true;
    let (active, _active_sender) = camera::simulated_open();
    app.camera_task = Some(active);
    let before = (app.capture_id.clone(), app.document_revision);
    let (candidate, sender) = camera::simulated_open();
    app.start_camera_with(1, |index, id, mode, previous| {
        assert_eq!(index, CameraIndex::Index(1));
        assert_eq!(id, "uvc-b");
        assert_eq!(mode, Some(requested));
        assert!(previous.is_none());
        candidate
    }).unwrap();
    assert_eq!(app.active_device_id, "uvc-a");
    let rejection = camera::select_format(&[active_mode], Some(requested)).unwrap_err();
    sender.send(CameraMessage::Error(rejection.clone())).unwrap();
    app.poll_camera_open(&ctx);
    assert!(app.pending_camera.is_none());
    assert!(app.running && app.camera_task.is_some());
    assert_eq!(app.active_device_id, "uvc-a");
    assert_eq!(app.camera_frame_rate, 30);
    assert_eq!((app.capture_id.clone(), app.document_revision), before);
    assert_eq!(app.image.as_ref().unwrap().rgb, [17, 18, 19]);
    assert!(app.message.contains(&rejection));

    let (candidate, sender) = camera::simulated_open();
    app.start_camera_with(1, |_, _, _, _| candidate).unwrap();
    sender.send(CameraMessage::Opened {
        format: requested,
        modes: vec![camera::CameraMode { format: requested, label: "requested".into() }],
    }).unwrap();
    app.poll_camera_open(&ctx);
    assert_eq!(app.active_device_id, "uvc-b");
    assert_eq!(app.camera_modes_camera.as_deref(), Some("uvc-b"));
    assert_eq!(app.camera_modes[app.selected_camera_mode.unwrap()].format, requested);
    assert_eq!(app.camera_frame_rate, 15);
    assert_ne!(app.capture_id, before.0);
}

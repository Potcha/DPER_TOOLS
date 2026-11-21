//! DPM-85 / LLT-ACQ-004-01.
use super::*;
use nokhwa::utils::KnownCameraControl;

fn advertised_controls() -> Vec<camera::CameraControlSetting> {
    let setting = |id, name: &str, writable, active| camera::CameraControlSetting {
        id, name: name.into(), writable, active, flags: String::new(),
        value: camera::CameraControlValue::Integer { value: 10, range: Some((0, 20)), step: 1 },
    };
    vec![
        setting(KnownCameraControl::Brightness, "Luminosité", true, true),
        setting(KnownCameraControl::Contrast, "Contraste", false, true),
        setting(KnownCameraControl::Saturation, "Saturation", true, false),
    ]
}

#[test]
fn only_active_writable_controls_advertised_by_the_driver_are_accepted() {
    let controls = advertised_controls();
    assert!(camera::control_is_writable(&controls, KnownCameraControl::Brightness));
    for blocked in [KnownCameraControl::Contrast, KnownCameraControl::Saturation, KnownCameraControl::Gain] {
        assert!(!camera::control_is_writable(&controls, blocked));
    }
}

#[test]
#[ignore = "requires ffmpeg and ffprobe"]
fn mp4_and_png_preserve_control_metadata_and_acquisition_timestamps() {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    let (task, _messages) = camera::simulated_open();
    app.camera_task = Some(task);
    app.running = true;
    app.paused = false;
    app.active_device = "Simulated UVC".into();
    app.active_device_id = "sim-uvc".into();
    app.camera_controls = advertised_controls();
    app.timelapse_interval_seconds = 1;
    app.timelapse_frame_count = 2;
    let wall = UNIX_EPOCH + Duration::from_secs(1_790_000_000);
    let started = Instant::now();
    let publish = |app: &mut DperMeasureApp, marker, elapsed_ms| {
        let elapsed = Duration::from_millis(elapsed_ms);
        app.camera_task.as_ref().unwrap().publish_test_frame(camera::Frame {
            width: 16, height: 16, frame_rate: 30, rgb: vec![marker; 16 * 16 * 3],
            captured_at: wall + elapsed, received_at: started + elapsed,
        });
        app.poll_camera(&ctx);
    };
    let base_ms = camera::unix_ms(wall);
    publish(&mut app, 25, 0);
    let dir = std::env::temp_dir().join(new_id("dpm85-recording"));
    fs::create_dir(&dir).unwrap();
    let video = dir.join("video.mp4");
    app.start_video_recording_at(video.clone(), wall);
    assert!(app.video_recording.is_some(), "{}", app.message);
    publish(&mut app, 50, 25);
    publish(&mut app, 75, 125);
    let result = app.stop_video_recording("terminé").unwrap();
    let read_json = |path: &Path| serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap();
    let metadata = read_json(&dir.join("video.dper_measure.json"));
    assert_eq!(metadata["status"], "terminé", "{result}");
    assert_eq!(metadata["device_id"], "sim-uvc");
    assert_eq!(metadata["camera_controls"][0]["id"], "Brightness");
    assert_eq!(metadata["frame_rate"], 30);
    assert_eq!(metadata["frames_written"], 2);
    assert_eq!(metadata["frame_captured_at_unix_ms"], serde_json::json!([base_ms + 25, base_ms + 125]));
    let probe = |path: &Path| {
        let result = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0",
            "-show_entries", "stream=width,height,nb_frames", "-of", "json"]).arg(path).output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()
    };
    assert_eq!(probe(&video)["streams"][0]["nb_frames"], "2");

    app.start_time_lapse_at(&dir, started, wall);
    let folder = app.timelapse_capture.as_ref().unwrap().folder.clone();
    publish(&mut app, 99, 800);
    app.poll_time_lapse_at(started + Duration::from_millis(1300));
    publish(&mut app, 164, 1850);
    app.poll_time_lapse_at(started + Duration::from_millis(2300));
    let manifest = read_json(&folder.join("timelapse.json"));
    assert_eq!(manifest["status"], "terminé", "{}", app.message);
    assert_eq!(manifest["device_id"], "sim-uvc");
    assert_eq!(manifest["camera_controls"][1]["writable"], false);
    assert_eq!(manifest["interval_seconds"], 1);
    for (i, (marker, elapsed)) in [(99, 800), (164, 1850)].into_iter().enumerate() {
        let frame = &manifest["frames"][i];
        assert_eq!(frame["elapsed_ms"], elapsed);
        assert_eq!(frame["captured_at_unix_ms"], base_ms + elapsed);
        let png = image::open(folder.join(frame["file"].as_str().unwrap())).unwrap().to_rgb8();
        assert_eq!(png.get_pixel(0, 0).0, [marker; 3]);
    }
    assert_eq!(probe(&folder.join(manifest["video_file"].as_str().unwrap()))["streams"][0]["nb_frames"], "2");
    for file in fs::read_dir(&folder).unwrap() {
        fs::remove_file(file.unwrap().path()).unwrap();
    }
    fs::remove_dir(folder).unwrap();
    fs::remove_file(video).unwrap();
    fs::remove_file(dir.join("video.dper_measure.json")).unwrap();
    fs::remove_dir(dir).unwrap();
}


#[test]
#[cfg(windows)]
#[ignore = "runs child test processes with an isolated runtime PATH"]
fn llt_exp_004_01_optional_dependencies_degrade_independently() {
    let binary = std::env::current_exe().unwrap();
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    for missing in ["tesseract", "ffmpeg"] {
        let name = format!("{missing}.exe");
        let paths = std::env::split_paths(&inherited).filter(|path| !path.join(&name).is_file());
        let isolated = std::env::join_paths(paths).unwrap();
        let result = Command::new(&binary)
            .args(["--exact", "acquisition_recording_tests::optional_dependency_child", "--ignored", "--nocapture"])
            .env("DPER_TEST_MISSING_RUNTIME", missing)
            .env("PATH", isolated)
            .current_dir(std::env::temp_dir())
            .output().unwrap();
        assert!(result.status.success(), "{missing}:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        println!("{missing}: {}", String::from_utf8_lossy(&result.stdout).trim());
    }
}

#[test]
#[cfg(windows)]
#[ignore = "invoked by llt_exp_004_01_optional_dependencies_degrade_independently"]
fn optional_dependency_child() {
    let Ok(missing) = std::env::var("DPER_TEST_MISSING_RUNTIME") else { return };
    let output = Command::new(&missing).arg("--version").output();
    assert!(output.is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "{missing} is still available outside the process PATH");
    let source = std::env::temp_dir().join(format!("{}.png", new_id("runtime-capture")));
    let _cleanup = crate::report::test_support::RemoveOnDrop(source.clone());
    image::save_buffer(&source, &[120; 32 * 24 * 3], 32, 24, image::ColorType::Rgb8).unwrap();

    let message = match missing.as_str() {
        "tesseract" => reference::ocr_image_with_lines(&source).unwrap_err(),
        "ffmpeg" => {
            let video = source.with_extension("mp4");
            let error = camera::VideoRecorder::start(&video, 32, 24, 30).err()
                .expect("video cannot start without FFmpeg");
            assert!(!video.exists());
            error
        }
        _ => panic!("unknown dependency"),
    };
    let expected = if missing == "tesseract" {
        "OCR local indisponible (Tesseract)"
    } else {
        "FFmpeg indisponible; vidéo MP4 non démarrée"
    };
    assert!(message.contains(expected), "{message}");

    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
    app.open_image_path(&ctx, &source).unwrap();
    let capture = source.with_file_name(format!("{}.png", new_id("runtime-png")));
    let _cleanup_capture = crate::report::test_support::RemoveOnDrop(capture.clone());
    app.capture_photo_to_path(&capture, false).unwrap();
    assert_eq!(image::image_dimensions(&capture).unwrap(), (32, 24));
    let pdf = crate::report::test_support::temporary_path("pdf");
    let _cleanup_report = crate::report::test_support::RemoveOnDrop(pdf.clone());
    let photo = crate::report::test_support::historical_vision_photo();
    crate::report::write_folder_pdf(&pdf, &crate::report::test_support::dense_options(), &[photo]).unwrap();
    assert!(fs::read(pdf).unwrap().starts_with(b"%PDF"));
    println!("{message} | ouverture d'image, export PNG et rapport PDF disponibles");
}

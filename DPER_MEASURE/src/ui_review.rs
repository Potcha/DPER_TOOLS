//! Explicit native visual review; no camera, MCP listener or real project.
use super::*;

pub(crate) fn run(destination: Option<&str>) -> eframe::Result {
    let folder = PathBuf::from(destination.expect("--ui-review requires a new output directory"));
    assert!(!folder.exists(), "Choose a new output directory");
    fs::create_dir_all(&folder).expect("Create UI review directory");
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1680.0, 1000.0]),
        ..Default::default()
    };
    eframe::run_native("DPER_MEASURE · revue visuelle isolée", native, Box::new(move |cc| {
        cc.egui_ctx.set_visuals(dper_visuals());
        let mut app = DperMeasureApp::new_with_services(cc.egui_ctx.clone(), false);
        // Deliberately synthetic contours, never a user's photograph or project.
        let mut pixels = image::RgbImage::from_pixel(1200, 800, image::Rgb([205, 214, 224]));
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            let distance = ((x as f32 - 600.0).powi(2) + (y as f32 - 400.0).powi(2)).sqrt();
            if (distance - 180.0).abs() < 2.5 ||
                ((x == 340 || x == 860) && (140..=660).contains(&y)) ||
                ((y == 140 || y == 660) && (340..=860).contains(&x)) {
                *pixel = image::Rgb([46, 67, 89]);
            }
        }
        app.set_image(&cc.egui_ctx, ImageData { width: 1200, height: 800,
            rgb: pixels.into_raw(), source: "Démonstration synthétique · aucune mesure physique".into() }, true);
        Ok(Box::new(Review { app, folder, frame: 0, stage: 0, waiting: false, failed: false }))
    }))
}

struct Review {
    app: DperMeasureApp,
    folder: PathBuf,
    frame: usize,
    stage: usize,
    waiting: bool,
    failed: bool,
}
impl eframe::App for Review {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let tabs = [ribbon_ui::RibbonTab::Camera, ribbon_ui::RibbonTab::Measurements,
            ribbon_ui::RibbonTab::Calibration, ribbon_ui::RibbonTab::Inspection, ribbon_ui::RibbonTab::Image];
        let names = ["camera", "mesures", "calibration", "inspection", "image"];
        let images: Vec<_> = ctx.input(|input| input.events.iter().filter_map(|event|
            if let egui::Event::Screenshot { image, .. } = event { Some(image.clone()) } else { None }).collect());
        for image in images {
            let bytes: Vec<_> = image.pixels.iter().flat_map(|color| color.to_array()).collect();
            let result = image::save_buffer(self.folder.join(format!("{}.png", names[self.stage])),
                &bytes, image.size[0] as u32, image.size[1] as u32, image::ColorType::Rgba8);
            if let Err(error) = result { eprintln!("UI_REVIEW_FAILED: {error}"); self.failed = true; }
            self.stage += 1;
            self.waiting = false;
            self.frame = 0;
        }
        if self.stage >= tabs.len() || self.failed {
            fs::write(self.folder.join("result.json"), serde_json::json!({
                "completed": !self.failed, "screenshots": names, "camera_started": false,
                "mcp_started": false, "synthetic_workspace": true
            }).to_string()).expect("Write visual review status");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.app.active_ribbon_tab = tabs[self.stage];
        self.app.calibration_mode = tabs[self.stage] == ribbon_ui::RibbonTab::Calibration;
        eframe::App::update(&mut self.app, ctx, frame);
        self.frame += 1;
        if self.frame >= 8 && !self.waiting {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.waiting = true;
        }
        if self.frame > 300 { self.failed = true; }
        ctx.request_repaint();
    }
}

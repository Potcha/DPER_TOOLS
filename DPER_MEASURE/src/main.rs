mod drawing_variants;
mod drawing_ui;
mod patterns;
mod selection_geometry;
mod measurement_graphics;
mod recovered_editing_ui;
mod recovered_measurement_ui;
mod recovered_workspace_actions;
mod recovered_chrome_ui;
mod recovered_results_ui;
#[cfg(test)]
mod recovered_editing_ui_tests;
#[cfg(test)]
mod recovered_workflow_tests;
#[cfg(test)]
mod recovered_compatibility_tests;
mod ui_widgets;
mod ribbon;
mod ribbon_ui;
mod ui_review;
mod camera;
mod alignment;
mod dper_registration;
mod comparison;
mod folder_export;
mod panorama_features;
mod distortion_calibration;
mod geometry;
mod imaging;
mod dper_vision;
mod dper_clearance;
mod clearance_ui;
mod clearance_inspection;
mod exports;
mod mcp_server;
mod pptx;
mod reference;
mod dper_pdf_layout;
mod dper_source;
mod dper_drawing;
mod drawing_structure;
mod quality;
mod report;
mod storage;
mod uncertainty;
mod automation_cli;
mod dper_metrology;
mod dper_pdf_geometry;
mod dper_binding;
mod dper_inspection;
mod measurement_graph;
mod inspection_ui;
mod drawing_binding_ui;
mod drawing_geometry_ui;
mod registration_ui;
mod inspection_plan_ui;
mod inspection_observation_ui;
mod inspection_evidence_ui;
mod inspection_decision_ui;
mod inspection_decision_presentation;
#[cfg(test)]
mod inspection_mcp_host_tests;

use camera::{CameraMessage, CameraTask};
use ui_widgets::{Icon as UiIcon, MenuItemTone, RibbonIcon, Tone, UiTokens};
use ui_widgets::paint_icon as paint_ui_icon;
use std::collections::BTreeSet;
use eframe::egui::{self, Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, Vec2};
use geometry::{Annotation, AnnotationTarget, Measurement, Point, Tool};
use quality::{DisplayCorrection, ImageQualityAssessment, ImageQualityThresholds};
use reference::{ReferenceDimension, ReferenceDocument};
use std::{fs, path::{Path, PathBuf}, process::Command, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use storage::{Calibration, Session, Settings};

fn new_id(prefix: &str) -> String {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .unwrap_or_default().as_nanos();
    format!("{prefix}-{stamp}")
}

fn new_capture_id() -> String {
    new_id("capture")
}

type ProtocolCharacteristicSignature = (
    String, String, String, Option<(String, u64, u64, u64, bool)>,
);

#[derive(serde::Deserialize)]
struct DistortionCalibrationObservations {
    format_version: u32,
    image_width: u32,
    image_height: u32,
    inner_corners_cols: usize,
    inner_corners_rows: usize,
    square_spacing_mm: f64,
    #[serde(default)]
    camera: String,
    #[serde(default)]
    optical_setup: String,
    views: Vec<Vec<Point>>,
}

fn protocol_measurement_signature(
    measurements: &[Measurement],
    reference_document: Option<&ReferenceDocument>,
) -> Vec<ProtocolCharacteristicSignature> {
    let mut signature = measurements.iter().map(|measurement| {
        let reference = reference_document.and_then(|document| document.dimensions.iter()
            .find(|dimension| dimension.measurement_id == Some(measurement.id)))
            .map(|dimension| (
                dimension.label.trim().to_lowercase(), dimension.nominal_mm.to_bits(),
                dimension.tolerance_minus_mm.to_bits(), dimension.tolerance_plus_mm.to_bits(),
                dimension.confirmed,
            ));
        (
            if measurement.label.trim().is_empty()
                || measurement.label.trim().eq_ignore_ascii_case(&format!("M{}", measurement.id))
            { measurement.kind.label().to_lowercase() }
            else { measurement.label.trim().to_lowercase() },
            measurement.kind.label().to_owned(),
            measurement.method.as_deref().unwrap_or_default().trim().to_lowercase(),
            reference,
        )
    }).collect::<Vec<_>>();
    signature.sort();
    signature
}

fn protocol_expected_signature(
    characteristics: &[storage::ProtocolCharacteristic],
) -> Vec<ProtocolCharacteristicSignature> {
    let mut signature = characteristics.iter().map(|characteristic| {
        let reference = characteristic.reference.as_ref().map(|dimension| (
            dimension.label.trim().to_lowercase(), dimension.nominal_mm.to_bits(),
            dimension.tolerance_minus_mm.to_bits(), dimension.tolerance_plus_mm.to_bits(),
            dimension.confirmed,
        ));
        (
            characteristic.label.trim().to_lowercase(),
            characteristic.kind.clone(),
            characteristic.method.as_deref().unwrap_or_default().trim().to_lowercase(),
            reference,
        )
    }).collect::<Vec<_>>();
    signature.sort();
    signature
}

fn protocol_matches_session(
    protocol: &storage::MeasurementProtocol,
    image: Option<&ImageData>,
    active_device: &str,
    active_device_id: &str,
    optical_setup: &str,
    calibration: Option<&Calibration>,
    quality_thresholds: &ImageQualityThresholds,
    display_correction: DisplayCorrection,
    measurements: &[Measurement],
    reference_document: Option<&ReferenceDocument>,
) -> bool {
    let calibration_matches = match calibration {
        Some(profile) => protocol.calibration_profile_id.as_deref() == Some(profile.id.as_str())
            && protocol.device_id == profile.device_id
            && protocol.mm_per_pixel.is_some_and(|scale| scale.to_bits() == profile.mm_per_pixel.to_bits())
            && protocol.scale_x_mm_per_pixel.map(f64::to_bits)
                == profile.scale_x_mm_per_pixel.map(f64::to_bits)
            && protocol.scale_y_mm_per_pixel.map(f64::to_bits)
                == profile.scale_y_mm_per_pixel.map(f64::to_bits)
            && protocol.expanded_uncertainty_mm.map(f64::to_bits)
                == profile.expanded_uncertainty_mm.map(f64::to_bits),
        None => protocol.calibration_profile_id.is_none()
            && protocol.mm_per_pixel.is_none()
            && protocol.scale_x_mm_per_pixel.is_none()
            && protocol.scale_y_mm_per_pixel.is_none()
            && protocol.expanded_uncertainty_mm.is_none(),
    };
    let thresholds_match = [
        quality_thresholds.min_mean_luminance.to_bits(),
        quality_thresholds.max_mean_luminance.to_bits(),
        quality_thresholds.min_contrast_std_dev.to_bits(),
        quality_thresholds.min_laplacian_variance.to_bits(),
        quality_thresholds.min_edge_visibility_fraction.to_bits(),
        quality_thresholds.max_clipped_pixel_fraction.to_bits(),
        quality_thresholds.edge_gradient_threshold.to_bits(),
    ] == [
        protocol.quality_thresholds.min_mean_luminance.to_bits(),
        protocol.quality_thresholds.max_mean_luminance.to_bits(),
        protocol.quality_thresholds.min_contrast_std_dev.to_bits(),
        protocol.quality_thresholds.min_laplacian_variance.to_bits(),
        protocol.quality_thresholds.min_edge_visibility_fraction.to_bits(),
        protocol.quality_thresholds.max_clipped_pixel_fraction.to_bits(),
        protocol.quality_thresholds.edge_gradient_threshold.to_bits(),
    ];
    let correction_matches = display_correction.brightness.to_bits() == protocol.display_correction.brightness.to_bits()
        && display_correction.contrast.to_bits() == protocol.display_correction.contrast.to_bits()
        && display_correction.gamma.to_bits() == protocol.display_correction.gamma.to_bits();
    let schema_matches = !protocol.expected_measurements.is_empty()
        && !protocol.expected_measurements.iter().any(|characteristic|
            characteristic.reference.as_ref().is_some_and(|reference| !reference.confirmed))
        && protocol_expected_signature(&protocol.expected_measurements)
            == protocol_measurement_signature(measurements, reference_document);
    let measurement_versions_match = measurements.iter().all(|measurement|
        measurement.protocol_id.as_deref() == Some(protocol.id.as_str())
            && measurement.protocol_version == Some(protocol.version));

    !protocol.id.trim().is_empty()
        && protocol.version > 0
        && (protocol.device.is_empty() || protocol.device == active_device)
        && (protocol.device_id.is_empty() || protocol.device_id == active_device_id)
        && protocol.optical_setup == optical_setup
        && image.is_some_and(|image| protocol.resolution == [image.width, image.height])
        && calibration_matches
        && thresholds_match
        && correction_matches
        && schema_matches
        && measurement_versions_match
}

fn copy_reference_into_project(source: &Path, project_parent: &Path, project_stem: &str) -> Result<PathBuf, String> {
    let extension = source.extension().and_then(|value| value.to_str()).unwrap_or("pdf");
    for suffix in 0..1000u32 {
        let name = if suffix == 0 {
            format!("{project_stem}.reference.{extension}")
        } else {
            format!("{project_stem}.reference-{suffix}.{extension}")
        };
        let destination = project_parent.join(name);
        let mut output = match fs::OpenOptions::new().write(true).create_new(true).open(&destination) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Copie du document de référence impossible : {error}")),
        };
        let copied = (|| {
            let mut input = fs::File::open(source)?;
            std::io::copy(&mut input, &mut output)?;
            Ok::<(), std::io::Error>(())
        })();
        if let Err(error) = copied {
            let _ = fs::remove_file(&destination);
            return Err(format!("Copie du document de référence impossible : {error}"));
        }
        return Ok(destination);
    }
    Err("Impossible de créer un nom unique pour le document de référence dans le projet.".into())
}

fn replace_mcp_output(
    temporary: &Path,
    destination: &Path,
    overwrite: bool,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    if !destination.exists() {
        return fs::rename(temporary, destination)
            .map_err(|error| format!("Installation de {} impossible : {error}", destination.display()));
    }
    if !overwrite { return Err(format!("Le fichier existe déjà : {}", destination.display())); }
    if !destination.is_file() { return Err(format!("La destination n'est pas un fichier : {}", destination.display())); }
    let name = destination.file_name().and_then(|name| name.to_str()).unwrap_or("rapport");
    let backup = destination.with_file_name(format!(".{name}.{}.backup", new_id("mcp")));
    fs::rename(destination, &backup)
        .map_err(|error| format!("Impossible de mettre l'ancien fichier de côté : {error}"))?;
    if let Err(error) = fs::rename(temporary, destination) {
        let restore = fs::rename(&backup, destination);
        return Err(match restore {
            Ok(()) => format!("Installation impossible; l'ancien fichier a été restauré : {error}"),
            Err(restore_error) => format!("Installation impossible ({error}); sauvegarde conservée dans {} ({restore_error})", backup.display()),
        });
    }
    if let Err(error) = fs::remove_file(&backup) {
        warnings.push(format!("Ancienne version conservée dans {} ({error})", backup.display()));
    }
    Ok(())
}

fn dper_visuals() -> egui::Visuals {
    let ink = UiTokens::TEXT_900;
    let muted = UiTokens::TEXT_600;
    let blue = UiTokens::BLUE_700;
    let teal = UiTokens::TEAL_600;
    let mut visuals = egui::Visuals::light();
    visuals.override_text_color = Some(ink);
    visuals.weak_text_color = Some(muted);
    visuals.panel_fill = UiTokens::SURFACE_0;
    visuals.window_fill = Color32::WHITE;
    visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(215, 223, 232));
    visuals.window_corner_radius = 10.into();
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.text_edit_bg_color = Some(Color32::WHITE);
    visuals.faint_bg_color = Color32::from_rgb(241, 245, 249);
    visuals.selection.bg_fill = Color32::from_rgb(225, 237, 248);
    visuals.selection.stroke = Stroke::new(1.0, blue);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ink);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(251, 253, 255);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(206, 216, 227));
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.5, blue);
    visuals.widgets.inactive.corner_radius = 7.into();
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(235, 244, 250);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.4, teal);
    visuals.widgets.hovered.fg_stroke = Stroke::new(2.0, blue);
    visuals.widgets.active.bg_fill = blue;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(29, 79, 128));
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, Color32::WHITE);
    visuals.widgets.open = visuals.widgets.hovered;
    visuals
}

fn raised_button(
    ui: &mut egui::Ui,
    label: &str,
    min_size: Vec2,
    fill: Color32,
    border: Color32,
    text: Color32,
    radius: u8,
) -> egui::Response {
    raised_button_state(ui, true, label, min_size, fill, border, text, radius)
}

fn raised_button_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    min_size: Vec2,
    fill: Color32,
    border: Color32,
    text: Color32,
    radius: u8,
) -> egui::Response {
    raised_button_state(ui, enabled, label, min_size, fill, border, text, radius)
}

fn raised_button_state(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    min_size: Vec2,
    fill: Color32,
    _border: Color32,
    text: Color32,
    _radius: u8,
) -> egui::Response {
    let tone = if text.r() > 140 && text.g() < 100 && text.b() < 120 {
        Tone::Danger
    } else if fill.r() < 80 && fill.g() > 100 && fill.b() > 100 {
        Tone::Action
    } else if fill.r() < 80 && fill.b() > 80 {
        Tone::Primary
    } else { Tone::Secondary };
    ui.add_enabled_ui(enabled, |ui| ui_widgets::button(ui, label, None, tone, min_size)).inner
}

fn category_heading(ui: &mut egui::Ui, label: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgb(239, 245, 251))
        .stroke(Stroke::new(1.0, Color32::from_rgb(215, 226, 239)))
        .corner_radius(7)
        .inner_margin(egui::Margin::symmetric(9, 5))
        .shadow(egui::Shadow {
            offset: [0, 1],
            blur: 3,
            spread: 0,
            color: Color32::from_black_alpha(20),
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("▏").size(15.0).strong()
                    .color(Color32::from_rgb(26, 143, 153)));
                ui.label(egui::RichText::new(label).size(11.5).strong()
                    .color(Color32::from_rgb(38, 67, 101)));
            });
        });
}

fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    raised_button(ui, label, egui::vec2(0.0, 38.0),
        Color32::from_rgb(16, 132, 139), Color32::from_rgb(10, 105, 112),
        Color32::WHITE, 8)
}

fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    raised_button(ui, label, egui::vec2(0.0, 36.0), Color32::WHITE,
        Color32::from_rgb(205, 216, 228), Color32::from_rgb(48, 91, 137), 7)
}

fn toolbar_button(ui: &mut egui::Ui, label: &str, accent: bool) -> egui::Response {
    let fill = if accent { Color32::from_rgb(25, 143, 153) } else { Color32::from_rgb(43, 91, 155) };
    let border = if accent { Color32::from_rgb(72, 188, 193) } else { Color32::from_rgb(87, 127, 183) };
    raised_button(ui, label, egui::vec2(0.0, 34.0), fill, border, Color32::WHITE, 7)
}

fn raised_menu_button(ui: &mut egui::Ui, label: impl Into<String>, icon: UiIcon,
                      accent: bool, has_dropdown: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    let response = ui_widgets::navigation_menu_button(ui, &label.into(), icon, accent, has_dropdown);
    raised_menu_popup(&response, add_contents);
}

fn measurement_icon(tool: Tool) -> UiIcon {
    match tool {
        Tool::Distance | Tool::PointLineDistance => UiIcon::Distance,
        Tool::Polyline | Tool::Polygon => UiIcon::Polyline,
        Tool::Angle | Tool::Arc => UiIcon::Angle,
        Tool::Circle | Tool::CircleFit | Tool::Ellipse => UiIcon::Point,
        Tool::Rectangle => UiIcon::Image,
        Tool::ParallelDistance => UiIcon::Parallel,
        Tool::Coordinate | Tool::ObjectCount => UiIcon::Crosshair,
    }
}

fn measurement_menu_item(ui: &mut egui::Ui, label: &str, tool: Tool, selected: bool,
                         shortcut: Option<&str>) -> egui::Response {
    ui_widgets::menu_item(ui, label, measurement_icon(tool), shortcut, false, true, selected,
        MenuItemTone::Standard)
}

fn measurement_columns(measurement: &Measurement) -> (String, String) {
    if measurement.is_construction { return ("Repère".into(), String::new()); }
    let result = geometry::result(measurement);
    if matches!(measurement.kind, Tool::Rectangle | Tool::Ellipse | Tool::Coordinate) {
        return ("Détails".into(), String::new());
    }
    if measurement.kind == Tool::ObjectCount {
        return (result.split_whitespace().next().unwrap_or("").into(), "obj.".into());
    }
    let primary = result.split(" · ").next().unwrap_or(&result)
        .trim_start_matches("Ø ").trim_start_matches("Δ⊥ ").trim_start_matches("Arc ");
    for (suffix, unit) in [(" px² (non étalonné)", "px²"), (" px (non étalonné)", "px"),
                           (" mm²", "mm²"), (" mm", "mm"), ("°", "°")] {
        if let Some(value) = primary.strip_suffix(suffix) {
            return (value.replace('.', ","), unit.into());
        }
    }
    (primary.to_owned(), String::new())
}

fn graphic_section(ui: &mut egui::Ui, id: impl std::hash::Hash, title: &str,
                   icon: UiIcon, default_open: bool, body: impl FnOnce(&mut egui::Ui)) {
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(), ui.make_persistent_id(id), default_open);
    if ui_widgets::section_header(ui, title, icon, state.is_open()).clicked() { state.toggle(ui); }
    state.show_body_unindented(ui, |ui| { ui.add_space(5.0); body(ui); });
    state.store(ui.ctx());
}

fn status_indicator(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

fn status_group(ui: &mut egui::Ui, icon: UiIcon, text: &str) -> egui::Response {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::hover());
        paint_ui_icon(ui.painter(), rect.center(), icon, Color32::from_rgb(36, 96, 158));
        ui.label(egui::RichText::new(text).size(12.5).color(Color32::from_rgb(36, 79, 128)));
    }).response
}

fn brand_mark(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
    let center = rect.center();
    let color = Color32::from_rgb(34, 190, 225);
    let painter = ui.painter();
    let hexagon = |radius: f32| (0..=6).map(|index| {
        let angle = (-90.0 + 60.0 * index as f32).to_radians();
        center + Vec2::new(angle.cos() * radius, angle.sin() * radius)
    }).collect::<Vec<_>>();
    painter.add(egui::Shape::line(hexagon(16.0), Stroke::new(2.2, color)));
    painter.add(egui::Shape::line(hexagon(10.5), Stroke::new(1.6, color)));
    painter.circle_stroke(center, 5.0, Stroke::new(1.4, color));
    painter.circle_filled(center, 2.0, color);
}

fn toolbar_icon_button(ui: &mut egui::Ui, label: &str, accent: bool, icon: UiIcon) -> egui::Response {
    ui_widgets::button(ui, label, Some(icon), if accent { Tone::NavActive } else { Tone::Nav },
        egui::vec2(0.0, 44.0))
}

fn canvas_toolbar_button(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    ui_widgets::button(ui, label, None, if active { Tone::Action } else { Tone::Canvas },
        egui::vec2(0.0, 36.0))
}

fn canvas_icon_button(ui: &mut egui::Ui, label: &str, active: bool, icon: UiIcon) -> egui::Response {
    ui_widgets::button(ui, label, Some(icon), if active { Tone::Action } else { Tone::Canvas },
        egui::vec2(0.0, 36.0))
}

fn raised_menu_popup(response: &egui::Response, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Popup::menu(response)
        .frame(egui::Frame::new().fill(UiTokens::SURFACE_1)
            .stroke(Stroke::new(1.0, UiTokens::BORDER_300))
            .corner_radius(UiTokens::CARD_RADIUS)
            .inner_margin(egui::Margin::symmetric(9, 9))
            .shadow(egui::Shadow {
                offset: [0, 5], blur: 16, spread: 0,
                color: Color32::from_black_alpha(38),
            }))
        .show(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
            add_contents(ui);
        });
}

fn annotation_color(index: usize) -> Color32 {
    const COLORS: [Color32; 8] = [
        Color32::from_rgb(208, 91, 43), Color32::from_rgb(115, 83, 186),
        Color32::from_rgb(28, 130, 126), Color32::from_rgb(193, 65, 111),
        Color32::from_rgb(80, 116, 54), Color32::from_rgb(183, 127, 31),
        Color32::from_rgb(56, 101, 176), Color32::from_rgb(124, 85, 65),
    ];
    COLORS[index % COLORS.len()]
}

fn scan_project_search(root: &Path, query: &str) -> Result<(Vec<ProjectSearchResult>, String), String> {
    if !root.is_dir() {
        return Err(format!("Le dossier racine n’existe pas ou n’est pas accessible : {}", root.display()));
    }
    const MAX_SCANNED_ENTRIES: usize = 50_000;
    const MAX_PROJECT_FILES: usize = 2_000;
    let mut pending = vec![root.to_path_buf()];
    let mut candidates = Vec::new();
    let mut scanned_entries = 0usize;
    let mut truncated = false;
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else { continue; };
        for entry in entries.flatten() {
            scanned_entries += 1;
            if scanned_entries >= MAX_SCANNED_ENTRIES { truncated = true; break; }
            let Ok(kind) = entry.file_type() else { continue; };
            if kind.is_symlink() { continue; }
            let path = entry.path();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && path.extension().and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dperproj"))
            {
                candidates.push(path);
                if candidates.len() >= MAX_PROJECT_FILES { truncated = true; break; }
            }
        }
        if truncated { break; }
    }
    candidates.sort_by(|left, right| {
        let left_time = fs::metadata(left).and_then(|metadata| metadata.modified()).unwrap_or(UNIX_EPOCH);
        let right_time = fs::metadata(right).and_then(|metadata| metadata.modified()).unwrap_or(UNIX_EPOCH);
        right_time.cmp(&left_time)
    });
    let query = query.trim().to_lowercase();
    let mut results = Vec::new();
    let mut unreadable = 0usize;
    for path in candidates {
        if !fs::metadata(&path).is_ok_and(|metadata| metadata.len() <= 16 * 1024 * 1024) {
            unreadable += 1;
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => { unreadable += 1; continue; }
        };
        let session: Session = match serde_json::from_slice::<Session>(&bytes) {
            Ok(session) if session.schema_version <= storage::CURRENT_SCHEMA_VERSION => session,
            _ => { unreadable += 1; continue; }
        };
        let measurement_labels = session.measurements.iter().map(|measurement| format!(
            "{} {}", measurement.label, measurement.kind.label(),
        )).collect::<Vec<_>>().join(" ");
        let corpus = format!("{} {} {} {} {} {} {}", path.display(), session.capture_id,
            session.device, session.device_id, session.optical_setup, measurement_labels,
            session.reference_document.as_ref().map(|reference| reference.source_name.as_str()).unwrap_or_default());
        if !query.is_empty() && !corpus.to_lowercase().contains(&query) { continue; }
        let resolution = session.calibration.as_ref().map(|calibration|
            format!("{}×{} px", calibration.resolution[0], calibration.resolution[1]))
            .or_else(|| session.protocol.as_ref().map(|protocol|
                format!("{}×{} px", protocol.resolution[0], protocol.resolution[1])))
            .unwrap_or_else(|| "résolution non mémorisée".into());
        let modified = fs::metadata(&path).and_then(|metadata| metadata.modified()).ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| report::utc_timestamp_at(duration.as_secs()))
            .unwrap_or_else(|| "date inconnue".into());
        results.push(ProjectSearchResult {
            path,
            summary: format!("{} · {} · {} mesure(s) · capture {} · modifié {}",
                session.device, resolution, session.measurements.len(), session.capture_id, modified),
        });
    }
    let note = format!("{} projet(s) trouvé(s){}{}",
        results.len(),
        if unreadable > 0 { format!(" · {unreadable} fichier(s) ignoré(s) car illisible(s) ou trop volumineux") } else { String::new() },
        if truncated { " · recherche limitée par le plafond local de fichiers" } else { "" },
    );
    Ok((results, note))
}

#[cfg(test)]
mod drawing_cli_routing_tests {
    #[test]
    fn drawing_commands_reach_adapters_and_fail_without_creating_outputs() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("dper-drawing-cli-missing-{}-{stamp}", std::process::id()));
        let source = directory.join("absent.pdf").display().to_string();
        let output = directory.join("output.json").display().to_string();
        for arguments in [
            vec!["drawing-plan".into(), source.clone(), output.clone()],
            vec!["drawing-geometry".into(), source, "1".into(), output],
        ] {
            // A routed command reports a missing input (1), not unknown usage (2).
            assert_eq!(super::run_cli(&arguments), 1);
        }
        assert!(!directory.exists());
    }
}

fn run_cli(arguments: &[String]) -> i32 {
    if arguments.first().is_some_and(|argument| matches!(argument.as_str(),
        "photocraft-correct" | "solidcraft-sketch" | "drawing-plan" | "drawing-geometry"))
    {
        return automation_cli::run(arguments);
    }
    if arguments.len() < 3 || arguments[0] != "export" {
        eprintln!("Usage : dper_measure --cli export <session.dperproj|session.json> <csv|xlsx|json|svg|dxf|png|ome-tiff> [sortie]");
        eprintln!("       dper_measure --cli photocraft-correct <image> <recette.json> [sortie.png]");
        eprintln!("       dper_measure --cli solidcraft-sketch <session.dperproj|session.json> [sortie.dxf]");
        eprintln!("       dper_measure --cli drawing-plan <plan.pdf> <sortie.json>");
        eprintln!("       dper_measure --cli drawing-geometry <plan.pdf> <page> <sortie.json>");
        return 2;
    }
    let input_path = PathBuf::from(&arguments[1]);
    let format = arguments[2].to_ascii_lowercase();
    let Ok(bytes) = fs::read(&input_path) else {
        eprintln!("Lecture de la session impossible : {}", input_path.display());
        return 1;
    };
    let session: Session = match serde_json::from_slice(&bytes) {
        Ok(session) => session,
        Err(error) => { eprintln!("Session JSON illisible : {error}"); return 1; }
    };
    if session.schema_version > storage::CURRENT_SCHEMA_VERSION {
        eprintln!("Version de session non prise en charge : {}", session.schema_version);
        return 1;
    }
    let source_path = PathBuf::from(&session.image_file);
    let source_path = if source_path.is_absolute() { source_path }
        else { input_path.parent().unwrap_or_else(|| Path::new(".")).join(source_path) };
    let default_output = || {
        let stem = input_path.file_stem().and_then(|value| value.to_str()).unwrap_or("dper-measure");
        let (name, extension) = match format.as_str() {
            "csv" => (format!("{stem}.csv"), "csv"),
            "xlsx" => (format!("{stem}.xlsx"), "xlsx"),
            "json" => (format!("{stem}.normalized.json"), "json"),
            "svg" => (format!("{stem}.svg"), "svg"),
            "dxf" => (format!("{stem}.dxf"), "dxf"),
            "png" => (format!("{stem}.annotated.png"), "png"),
            "ome-tiff" | "ome_tiff" => (format!("{stem}.ome.tif"), "tif"),
            _ => (format!("{stem}.out"), "out"),
        };
        input_path.with_file_name(name).with_extension(extension)
    };
    let output_path = arguments.get(3).map(PathBuf::from).unwrap_or_else(default_output);
    let result = match format.as_str() {
        "csv" => exports::measurements_csv(&session),
        "xlsx" => match image::open(&source_path) {
            Ok(image) => {
                let image = image.to_rgb8();
                let (width, height) = image.dimensions();
                exports::measurements_xlsx_for_image(&session, image.as_raw(), width, height)
            }
            Err(_) => exports::measurements_xlsx(&session),
        },
        "json" => serde_json::to_vec_pretty(&session)
            .map_err(|error| format!("Sérialisation JSON impossible : {error}")),
        "svg" | "png" | "ome-tiff" | "ome_tiff" | "dxf" => {
            let image = match image::open(&source_path) {
                Ok(image) => image.to_rgb8(),
                Err(error) => { eprintln!("Image source absente ou illisible ({}): {error}", source_path.display()); return 1; }
            };
            let (width, height) = image.dimensions();
            let rgb = image.into_raw();
            match format.as_str() {
                "svg" => exports::measurement_svg(&session, &rgb, width, height),
                "png" => exports::annotated_png(&rgb, width, height, &session),
                "ome-tiff" | "ome_tiff" => exports::ome_tiff(&rgb, width, height, &session),
                "dxf" => exports::measurement_dxf(&session, height),
                _ => unreachable!(),
            }
        }
        _ => {
            eprintln!("Format non pris en charge : {format}");
            eprintln!("Formats disponibles : csv, xlsx, json, svg, dxf, png, ome-tiff.");
            return 2;
        }
    };
    match result.and_then(|bytes| fs::write(&output_path, bytes)
        .map_err(|error| format!("Écriture de {} impossible : {error}", output_path.display())))
    {
        Ok(()) => { println!("Export enregistré : {}", output_path.display()); 0 }
        Err(error) => { eprintln!("Export impossible : {error}"); 1 }
    }
}

fn main() -> eframe::Result {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|argument| argument == "--ui-review") {
        return ui_review::run(arguments.get(1).map(String::as_str));
    }
    if arguments.first().is_some_and(|argument| argument == "--cli") {
        std::process::exit(run_cli(&arguments[1..]));
    }
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native("DPER_Measure — Microscope & métrologie", native, Box::new(|cc| {
        cc.egui_ctx.set_visuals(dper_visuals());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
        style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
        style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(12.0));
        style.text_styles.insert(egui::TextStyle::Heading, FontId::proportional(19.0));
        style.spacing.interact_size = egui::vec2(42.0, 34.0);
        style.spacing.icon_width = 16.0;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        cc.egui_ctx.set_style(style);
        Ok(Box::new(DperMeasureApp::new(cc.egui_ctx.clone())))
    }))
}

struct ImageData {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    source: String,
}

struct CaptureTimer {
    destination: PathBuf,
    capture_at: Instant,
    best_frame: Option<camera::Frame>,
    best_sharpness: f64,
    frames_seen: u32,
}

#[derive(serde::Serialize)]
struct TimeLapseManifest {
    format_version: u32,
    status: String,
    device: String,
    device_id: String,
    optical_setup: String,
    capture_id: String,
    resolution: [u32; 2],
    camera_frame_rate: u32,
    camera_controls: Vec<CameraControlMetadata>,
    interval_seconds: u16,
    expected_frame_count: u16,
    started_at_unix_secs: u64,
    calibration_id: Option<String>,
    scale_x_mm_per_pixel: Option<f64>,
    scale_y_mm_per_pixel: Option<f64>,
    expanded_uncertainty_mm: Option<f64>,
    protocol_id: Option<String>,
    protocol_version: Option<u32>,
    quality_thresholds: ImageQualityThresholds,
    display_correction: DisplayCorrection,
    video_file: Option<String>,
    frames: Vec<TimeLapseFrameEntry>,
}

#[derive(serde::Serialize)]
struct CameraControlMetadata {
    id: String,
    name: String,
    value: String,
    flags: String,
    active: bool,
    writable: bool,
}

#[derive(serde::Serialize)]
struct TimeLapseFrameEntry {
    index: u16,
    file: String,
    captured_at_unix_secs: u64,
    elapsed_ms: u128,
    sharpness_score: Option<f64>,
}

struct TimeLapseCapture {
    folder: PathBuf,
    interval_seconds: u16,
    next_frame_at: Instant,
    started: Instant,
    frame_limit: u16,
    frames_seen: u32,
    best_frame: Option<camera::Frame>,
    best_sharpness: f64,
    manifest: TimeLapseManifest,
}

#[derive(serde::Serialize)]
struct VideoRecordingMetadata {
    format_version: u32,
    status: String,
    video_file: String,
    device: String,
    device_id: String,
    resolution: [u32; 2],
    frame_rate: u32,
    camera_controls: Vec<CameraControlMetadata>,
    optical_setup: String,
    capture_id: String,
    calibration_id: Option<String>,
    calibration_status: Option<String>,
    scale_x_mm_per_pixel: Option<f64>,
    scale_y_mm_per_pixel: Option<f64>,
    expanded_uncertainty_mm: Option<f64>,
    protocol_id: Option<String>,
    protocol_version: Option<u32>,
    quality_thresholds: ImageQualityThresholds,
    display_correction: DisplayCorrection,
    started_at_unix_secs: u64,
    completed_at_unix_secs: Option<u64>,
    frames_written: Option<u64>,
    frames_dropped: Option<u64>,
    error: Option<String>,
}

struct VideoRecording {
    path: PathBuf,
    metadata_path: PathBuf,
    recorder: Option<camera::VideoRecorder>,
    metadata: VideoRecordingMetadata,
}

impl VideoRecording {
    fn close(&mut self, status: &str) -> Result<(), String> {
        if let Some(recorder) = self.recorder.take() {
            match recorder.finish() {
                Ok((written, dropped)) => {
                    self.metadata.status = status.to_owned();
                    self.metadata.frames_written = Some(written);
                    self.metadata.frames_dropped = Some(dropped);
                    self.metadata.completed_at_unix_secs = Some(SystemTime::now()
                        .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                }
                Err(error) => {
                    self.metadata.status = "échec".into();
                    self.metadata.error = Some(error.clone());
                    self.metadata.completed_at_unix_secs = Some(SystemTime::now()
                        .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                    let _ = write_video_metadata(&self.metadata_path, &self.metadata);
                    return Err(error);
                }
            }
        }
        write_video_metadata(&self.metadata_path, &self.metadata)
    }
}

impl Drop for VideoRecording {
    fn drop(&mut self) { let _ = self.close("interrompu"); }
}

fn write_video_metadata(path: &Path, metadata: &VideoRecordingMetadata) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(metadata)
        .map_err(|error| format!("Sérialisation du sidecar vidéo impossible : {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("Écriture du sidecar vidéo impossible : {error}"))
}

fn write_timelapse_manifest(capture: &TimeLapseCapture) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(&capture.manifest)
        .map_err(|error| format!("Sérialisation du manifeste time-lapse impossible : {error}"))?;
    fs::write(capture.folder.join("timelapse.json"), bytes)
        .map_err(|error| format!("Écriture du manifeste time-lapse impossible : {error}"))
}

struct PointEditGesture {
    before: Measurement,
    target: EditTarget,
    pointer_offset: Point,
    start_native: Point,
    pointer_origin: Pos2,
    rejected_degenerate: bool,
}

#[derive(Clone, Copy)]
enum EditTarget {
    LabelCard,
    Vertex(usize),
    CircleCenter,
    TranslateSegment { first: usize, second: usize },
    RectangleCorner { x_point: usize, y_point: usize },
    RectangleEdgeX(usize),
    RectangleEdgeY(usize),
    CircleRadius,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationType {
    Segment,
    CircleDiameter,
    RectangleDiagonal,
    MultiPointXY,
}

impl CalibrationType {
    fn label(self) -> &'static str {
        match self {
            Self::Segment => "Segment",
            Self::CircleDiameter => "Cercle · diamètre",
            Self::RectangleDiagonal => "Rectangle · diagonale",
            Self::MultiPointXY => "Multipoint X/Y",
        }
    }

    fn preview_tool(self) -> Tool {
        match self {
            Self::Segment => Tool::Distance,
            Self::CircleDiameter => Tool::Circle,
            Self::RectangleDiagonal => Tool::Rectangle,
            Self::MultiPointXY => Tool::Polyline,
        }
    }

    fn reference_label(self) -> &'static str {
        match self {
            Self::Segment => "Longueur connue (mm)",
            Self::CircleDiameter => "Diamètre connu (mm)",
            Self::RectangleDiagonal => "Diagonale connue (mm)",
            Self::MultiPointXY => "Longueurs X, Y et contrôle (mm)",
        }
    }

    fn instruction(self) -> &'static str {
        match self {
            Self::Segment => "Cliquez les deux extrémités du segment connu.",
            Self::CircleDiameter => "Cliquez le centre, puis le bord du cercle de diamètre connu.",
            Self::RectangleDiagonal => "Cliquez deux coins opposés du rectangle de diagonale connue.",
            Self::MultiPointXY => "Au centre, cliquez X puis Y; mesurez ensuite la référence indépendante dans chacun des quatre coins.",
        }
    }

    fn reference_scale(self) -> f64 {
        if matches!(self, Self::CircleDiameter) { 2.0 } else { 1.0 }
    }
}

#[derive(Clone, Copy)]
struct CalibrationPointEditGesture {
    index: usize,
    before: Point,
    pointer_offset: Point,
    pointer_origin: Pos2,
}

struct SelectionMarquee {
    start: Point,
    end: Point,
    before: Vec<u64>,
    before_annotations: Vec<String>,
    additive: bool,
    toggle: bool,
    label_bounds: Vec<(u64, [f64; 4])>,
    annotation_label_bounds: Vec<(String, [f64; 4])>,
    annotation_connectors: Vec<(String, Point, Point)>,
    pointer_origin: Pos2,
    dragged: bool,
    preview: Vec<u64>,
    preview_annotations: Vec<String>,
}

struct MeasurementGroupMove {
    before: Vec<Measurement>,
    selection: Vec<u64>,
    start: Point,
    delta: Point,
    pointer_origin: Pos2,
    capture_id: String,
    revision: u64,
}

struct AnnotationVisual {
    id: String,
    index: usize,
    target: Pos2,
    badge: Option<Rect>,
}

#[derive(Clone, Copy)]
struct EditHandle {
    position: Pos2,
    image_point: Point,
    target: EditTarget,
}

enum MeasurementHistoryAction {
    AddedMany {
        measurements: Vec<(usize, Measurement)>,
        selection: Vec<u64>,
    },
    DisplayAdjusted {
        before: DisplayCorrection,
        after: DisplayCorrection,
    },
    Added {
        measurement: Measurement,
        index: usize,
        previous_selection: Option<u64>,
    },
    Edited {
        id: u64,
        before: Measurement,
        after: Measurement,
    },
    EditedMany {
        before: Vec<Measurement>,
        after: Vec<Measurement>,
        selection: Vec<u64>,
    },
    Removed {
        measurement: Measurement,
        index: usize,
        previous_selection: Option<u64>,
    },
    RemovedMany {
        measurements: Vec<(usize, Measurement)>,
        selection: Vec<u64>,
    },
    Cleared {
        measurements: Vec<Measurement>,
        previous_selection: Option<u64>,
    },
}

const MAX_MEASUREMENT_AUDIT_EVENTS: usize = 300;

#[derive(Clone, Copy)]
enum FolderExportFormat { Pdf, PowerPoint, BatchCsv, BatchXlsx }

#[derive(Clone, Copy, PartialEq, Eq)]
enum DataExportFormat { Csv, Xlsx, SessionJson, CalibrationJson, AnnotatedPng, Svg, Dxf, OmeTiff }
#[derive(Clone, Copy, PartialEq, Eq)]
enum AdvancedImageAction { FlatField, CalibrateDistortion, RadialDistortion, FocusStack, Panorama, RectifyPerspective }

struct PanoramaWizard {
    images: Vec<image::RgbImage>,
    paths: Vec<PathBuf>,
    transforms_source_to_panorama: Vec<[f64; 9]>,
    offsets: Vec<(i32, i32)>,
    alignment_confidences: Vec<Option<f64>>,
    alignment_details: Vec<String>,
}

struct RectificationWizard {
    source: image::RgbImage,
    source_name: String,
    preview: egui::TextureHandle,
    corners: Vec<(f64, f64)>,
}

struct ProjectSearchResult {
    path: PathBuf,
    summary: String,
}

struct ProjectSearchWindow {
    root: PathBuf,
    query: String,
    results: Vec<ProjectSearchResult>,
    note: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnnotationTargetMode { SelectedMeasurement, Point, Zone }

#[derive(Clone, Copy, PartialEq, Eq)]
enum RibbonTab { Camera, Capture, Files, Calibration, Measurements, Inspection, Image, Exports, Automation }

impl RibbonTab {
    const ALL: [(Self, &'static str, RibbonIcon); 9] = [
        (Self::Camera, "Caméra", RibbonIcon::Camera),
        (Self::Capture, "Capture", RibbonIcon::Capture),
        (Self::Files, "Fichiers", RibbonIcon::Files),
        (Self::Calibration, "Calibration", RibbonIcon::Calibration),
        (Self::Measurements, "Mesures", RibbonIcon::Measurements),
        (Self::Inspection, "Inspection", RibbonIcon::Inspection),
        (Self::Image, "Image", RibbonIcon::Image),
        (Self::Exports, "Exports", RibbonIcon::Exports),
        (Self::Automation, "Automatisation", RibbonIcon::Automation),
    ];
}

struct MeasurementCallout {
    title: Option<std::sync::Arc<egui::Galley>>,
    value: std::sync::Arc<egui::Galley>,
    size: Vec2,
    padding: Vec2,
    border: Color32,
    value_color: Color32,
    outlined: bool,
}

struct DperMeasureApp {
    pan_mode: bool,
    selected_measurements: BTreeSet<u64>,
    selected_annotations: BTreeSet<String>,
    selection_marquee: Option<SelectionMarquee>,
    measurement_group_move: Option<MeasurementGroupMove>,
    calibration_point_edit: Option<CalibrationPointEditGesture>,
    right_panel_annotations: bool,
    display_texture_dirty: bool,
    display_correction_drag_active: bool,
    display_history_priority: bool,
    drawing_variant: drawing_variants::DrawingVariant,
    drawing_radius_px: f64,
    construction_mode: bool,
    construction_snap_enabled: bool,
    pattern_dialog: Option<drawing_ui::PatternDialog>,
    confirm_close_image: bool,
    workflow_open_reference_panel: bool,
    active_ribbon_tab: RibbonTab,
    tab_icon_sheet: Option<egui::TextureHandle>,
    cameras: Vec<(nokhwa::utils::CameraIndex, String, String)>,
    selected_camera: usize,
    camera_modes: Vec<camera::CameraMode>,
    camera_modes_camera: Option<String>,
    selected_camera_mode: Option<usize>,
    camera_controls: Vec<camera::CameraControlSetting>,
    camera_controls_camera: Option<String>,
    show_camera_controls: bool,
    active_device: String,
    active_device_id: String,
    camera_task: Option<CameraTask>,
    camera_frame_rate: u32,
    video_recording: Option<VideoRecording>,
    capture_delay_seconds: u8,
    capture_timer: Option<CaptureTimer>,
    timelapse_interval_seconds: u16,
    timelapse_frame_count: u16,
    timelapse_capture: Option<TimeLapseCapture>,
    running: bool,
    paused: bool,
    live_alignment: alignment::AlignmentState,
    live_alignment_assessment: Option<alignment::AlignmentAssessment>,
    alignment_tolerance_degrees: f32,
    image: Option<ImageData>,
    texture: Option<egui::TextureHandle>,
    image_is_imported: bool,
    capture_id: String,
    zoom: f32,
    fit_view: bool,
    zoom_percent: f32,
    pan: Vec2,
    show_alignment_grid: bool,
    show_center_guides: bool,
    mode: Tool,
    selection_mode: bool,
    selected_measurement: Option<u64>,
    calibration_mode: bool,
    calibration_type: CalibrationType,
    pending: Vec<Point>,
    edge_snap_enabled: bool,
    edge_snap_radius: u32,
    edge_snap_preview: Option<quality::EdgeSnapResult>,
    pending_snap_enabled: bool,
    pending_snap_radius: Option<u32>,
    pending_snap_applied: Vec<bool>,
    point_edit: Option<PointEditGesture>,
    pending_calibration_id: Option<String>,
    pending_mm_per_pixel: Option<f64>,
    reference_mm: String,
    reference_mm_y: String,
    verification_reference_mm: String,
    expanded_uncertainty_mm: String,
    uncertainty_budget_components_mm: Vec<String>,
    uncertainty_coverage_factor: String,
    pending_uncertainty_budget: Option<uncertainty::UncertaintyBudget>,
    show_uncertainty_budget: bool,
    optical_setup: String,
    mm_per_pixel: Option<f64>,
    calibration: Option<Calibration>,
    calibration_history: Vec<Calibration>,
    settings: Settings,
    active_protocol: Option<storage::MeasurementProtocol>,
    protocol_name_draft: String,
    protocol_family_draft: String,
    measurements: Vec<Measurement>,
    annotations: Vec<Annotation>,
    annotation_text: String,
    annotation_target_mode: AnnotationTargetMode,
    annotation_mode: bool,
    annotation_points: Vec<Point>,
    quality_thresholds: ImageQualityThresholds,
    quality_assessment: Option<ImageQualityAssessment>,
    display_correction: DisplayCorrection,
    inspection: dper_inspection::InspectionWorkspace,
    drawing_geometry_cache: Vec<dper_pdf_geometry::DrawingGeometryCatalog>,
    drawing_geometry_generation: u64,
    drawing_geometry_ui: drawing_geometry_ui::DrawingGeometryUi,
    registration_ui: registration_ui::RegistrationUi,
    inspection_plan_ui: inspection_plan_ui::InspectionPlanUi,
    inspection_observation_ui: inspection_observation_ui::InspectionObservationUi,
    clearance_ui: clearance_ui::ClearanceUi,
    inspection_decision_ui: inspection_decision_ui::InspectionDecisionUi,
    inspection_panel_cache: Option<(u64, String, dper_inspection::InspectionWorkspace)>,
    reference_document: Option<ReferenceDocument>,
    ab_comparison: Option<comparison::AbComparison>,
    show_ab_comparison: bool,
    undo_history: Vec<MeasurementHistoryAction>,
    redo_history: Vec<MeasurementHistoryAction>,
    measurement_audit_history: Vec<storage::MeasurementAuditEvent>,
    next_id: u64,
    session_saved: bool,
    document_revision: u64,
    directory: PathBuf,
    export_dialog: Option<folder_export::Dialog>,
    panorama_wizard: Option<PanoramaWizard>,
    show_panorama_wizard: bool,
    rectification_wizard: Option<RectificationWizard>,
    show_rectification_wizard: bool,
    project_search: Option<ProjectSearchWindow>,
    show_project_search: bool,
    show_guided_examples: bool,
    mcp_bridge: mcp_server::Bridge,
    mcp_requests: mcp_server::UiRequestReceiver,
    mcp_server: Option<mcp_server::ServerHandle>,
    message: String,
}

impl DperMeasureApp {
    fn new(ctx: egui::Context) -> Self {
        Self::new_with_services(ctx, true)
    }

    fn new_with_services(ctx: egui::Context, start_services: bool) -> Self {
        ui_widgets::configure_fonts(&ctx);
        let cameras = if start_services { camera::devices() } else { Vec::new() };
        let directory = dirs::document_dir().unwrap_or_else(|| PathBuf::from("."))
            .join("DPER_Measure");
        let (mcp_bridge, mcp_requests) = mcp_server::create_bridge(ctx);
        let (mcp_server, startup_message) = if start_services {
            match mcp_server::start(mcp_bridge.clone()) {
                Ok(server) => (Some(server), format!("Serveur MCP prêt : {}", mcp_server::MCP_ENDPOINT)),
                Err(error) => (None, format!("Serveur MCP non démarré : {error}")),
            }
        } else {
            (None, String::new())
        };
        Self {
            pan_mode: false, selected_measurements: BTreeSet::new(), selected_annotations: BTreeSet::new(),
            selection_marquee: None, measurement_group_move: None, calibration_point_edit: None,
            right_panel_annotations: false, display_texture_dirty: false, display_correction_drag_active: false,
            display_history_priority: false, drawing_variant: drawing_variants::DrawingVariant::Line,
            drawing_radius_px: 10.0, construction_mode: false, construction_snap_enabled: true,
            pattern_dialog: None, confirm_close_image: false, workflow_open_reference_panel: false,
            active_ribbon_tab: RibbonTab::Camera,
            tab_icon_sheet: None,
            cameras, selected_camera: 0, camera_modes: Vec::new(), camera_modes_camera: None,
            selected_camera_mode: None,
            camera_controls: Vec::new(), camera_controls_camera: None, show_camera_controls: false,
            active_device: "Image importée".into(), active_device_id: "imported-image".into(),
            camera_task: None, camera_frame_rate: 30, video_recording: None,
            capture_delay_seconds: 0, capture_timer: None,
            timelapse_interval_seconds: 1, timelapse_frame_count: 10, timelapse_capture: None,
            running: false, paused: false,
            live_alignment: alignment::AlignmentState::Unknown,
            live_alignment_assessment: None, alignment_tolerance_degrees: 2.0,
            image: None, texture: None, image_is_imported: false,
            capture_id: new_capture_id(),
            zoom: 1.0, fit_view: true, zoom_percent: 100.0, pan: Vec2::ZERO,
            show_alignment_grid: true, show_center_guides: true,
            mode: Tool::Distance, selection_mode: false, selected_measurement: None,
            calibration_mode: false, calibration_type: CalibrationType::Segment, pending: Vec::new(),
            edge_snap_enabled: false, edge_snap_radius: 6,
            edge_snap_preview: None,
            pending_snap_enabled: false, pending_snap_radius: None, pending_snap_applied: Vec::new(),
            point_edit: None,
            pending_calibration_id: None, pending_mm_per_pixel: None,
            reference_mm: "10".into(), expanded_uncertainty_mm: String::new(),
            reference_mm_y: "10".into(), verification_reference_mm: "10".into(),
            uncertainty_budget_components_mm: vec![String::new(); 4],
            uncertainty_coverage_factor: "2".into(),
            pending_uncertainty_budget: None, show_uncertainty_budget: false,
            optical_setup: "objectif A / position 1".into(),
            mm_per_pixel: None, calibration: None, calibration_history: Vec::new(),
            settings: if start_services { storage::load_settings() } else { Settings::default() },
            active_protocol: None,
            protocol_name_draft: "Nouveau protocole".into(),
            protocol_family_draft: "Famille de pièce".into(),
            measurements: Vec::new(), annotations: Vec::new(), annotation_text: String::new(),
            annotation_target_mode: AnnotationTargetMode::SelectedMeasurement,
            annotation_mode: false, annotation_points: Vec::new(),
            quality_thresholds: ImageQualityThresholds::default(), quality_assessment: None,
            display_correction: DisplayCorrection::default(),
            ab_comparison: None, show_ab_comparison: false,
            undo_history: Vec::new(), redo_history: Vec::new(),
            measurement_audit_history: Vec::new(),
            inspection: Default::default(),
            drawing_geometry_cache: Vec::new(),
            drawing_geometry_generation: 0,
            drawing_geometry_ui: Default::default(),
            registration_ui: Default::default(),
            inspection_plan_ui: Default::default(),
            inspection_observation_ui: Default::default(),
            clearance_ui: Default::default(),
            inspection_decision_ui: Default::default(),
            inspection_panel_cache: None,
            reference_document: None,
            next_id: 1, session_saved: true, document_revision: 1,
            directory, export_dialog: None, panorama_wizard: None, show_panorama_wizard: false,
            rectification_wizard: None, show_rectification_wizard: false,
            project_search: None, show_project_search: false,
            show_guided_examples: false,
            mcp_bridge, mcp_requests, mcp_server,
            message: format!("Prêt : connecter une caméra UVC ou ouvrir une image. {startup_message}"),
        }
    }











    fn set_image(&mut self, ctx: &egui::Context, new_img: ImageData, reset_marks: bool) {
        self.inspection_panel_cache = None;
        if new_img.rgb.len() != (new_img.width as usize) * (new_img.height as usize) * 3 {
            self.message = "Image reçue invalide".into(); return;
        }
        let adjusted = if self.display_correction.brightness == 0.0
            && self.display_correction.contrast == 1.0
            && self.display_correction.gamma == 1.0
        {
            None
        } else {
            quality::corrected_display_rgb(&new_img.rgb, self.display_correction).ok()
        };
        let ci = egui::ColorImage::from_rgb(
            [new_img.width as usize, new_img.height as usize], adjusted.as_deref().unwrap_or(&new_img.rgb));
        if let Some(tex) = &mut self.texture {
            tex.set(ci, egui::TextureOptions::LINEAR);
        } else {
            self.texture = Some(ctx.load_texture("image-dper-measure", ci, egui::TextureOptions::LINEAR));
        }
        let size_changed = self.image.as_ref().is_none_or(|old| old.width != new_img.width || old.height != new_img.height);
        self.image = Some(new_img);
        if reset_marks || size_changed {
            self.clear_pending(); self.measurements.clear();
            if reset_marks {
            self.inspection.registration = None;
            self.inspection.records.clear(); self.annotations.clear(); }
            self.undo_history.clear(); self.redo_history.clear(); self.point_edit = None;
            self.selected_measurement = None;
            if reset_marks { self.measurement_audit_history.clear(); }
            self.zoom = 1.0; self.fit_view = true; self.pan = Vec2::ZERO;
            self.calibration_history.clear();
        }
        if reset_marks {
            self.capture_id = new_capture_id();
            self.document_revision = self.document_revision.wrapping_add(1);
            self.annotation_mode = false;
            self.annotation_points.clear();
            self.assess_current_quality();
        }
        if size_changed || reset_marks { self.activate_saved_calibration(); }
    }

    fn assess_current_quality(&mut self) {
        self.quality_assessment = self.image.as_ref().and_then(|image| {
            quality::assess_image_quality_with_correction(
                &image.rgb, image.width, image.height, &self.quality_thresholds,
                self.display_correction,
            ).ok()
        });
    }

    fn refresh_display_texture(&mut self, ctx: &egui::Context) {
        let Some(image) = self.image.as_ref() else { return; };
        let Ok(rgb) = quality::corrected_display_rgb(&image.rgb, self.display_correction) else { return; };
        let color_image = egui::ColorImage::from_rgb(
            [image.width as usize, image.height as usize], &rgb);
        if let Some(texture) = &mut self.texture {
            texture.set(color_image, egui::TextureOptions::LINEAR);
        } else {
            self.texture = Some(ctx.load_texture(
                "image-dper-measure", color_image, egui::TextureOptions::LINEAR,
            ));
        }
    }

    fn add_annotation(&mut self, target: AnnotationTarget) {
        let text = self.annotation_text.trim();
        if text.is_empty() {
            self.message = "Saisissez un commentaire avant de l'ajouter.".into();
            return;
        }
        self.annotations.push(Annotation {
            id: new_id("annotation"),
            text: text.chars().take(2000).collect(),
            target,
            created_at_unix_secs: SystemTime::now().duration_since(UNIX_EPOCH)
                .unwrap_or_default().as_secs(),
        });
        self.annotation_text.clear();
        self.annotation_mode = false;
        self.annotation_points.clear();
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        self.message = "Commentaire ajouté à la session.".into();
    }

    fn handle_annotation_click(&mut self, point: Point) {
        if self.annotation_text.trim().is_empty() {
            self.annotation_mode = false;
            self.annotation_points.clear();
            self.message = "Saisissez le commentaire avant de cibler l'image.".into();
            return;
        }
        match self.annotation_target_mode {
            AnnotationTargetMode::SelectedMeasurement => {
                self.message = "Sélectionnez une mesure puis ajoutez le commentaire depuis le panneau.".into();
                self.annotation_mode = false;
            }
            AnnotationTargetMode::Point => self.add_annotation(AnnotationTarget::Point { point }),
            AnnotationTargetMode::Zone => {
                self.annotation_points.push(point);
                if self.annotation_points.len() == 1 {
                    self.message = "Cliquez sur le coin opposé pour définir la zone du commentaire.".into();
                } else {
                    let first = self.annotation_points[0];
                    self.add_annotation(AnnotationTarget::Zone { first, second: point });
                }
            }
        }
    }

    fn annotation_target_label(&self, target: &AnnotationTarget) -> String {
        match target {
            AnnotationTarget::Measurement { measurement_id } => self.measurements.iter()
                .find(|measurement| measurement.id == *measurement_id)
                .map(|measurement| format!("Mesure M{} · {}", measurement.id, measurement.label))
                .unwrap_or_else(|| format!("Mesure M{measurement_id} (absente de l'image)")),
            AnnotationTarget::Point { point } => format!("Point ({:.1}; {:.1}) px", point.x, point.y),
            AnnotationTarget::Zone { first, second } => format!(
                "Zone ({:.1}; {:.1})–({:.1}; {:.1}) px", first.x, first.y, second.x, second.y,
            ),
        }
    }

    fn save_protocol_version(&mut self) {
        let Some(image) = self.image.as_ref() else {
            self.message = "Ouvrez une image avant d'enregistrer un protocole.".into();
            return;
        };
        let name = self.protocol_name_draft.trim();
        let part_family = self.protocol_family_draft.trim();
        if name.is_empty() || part_family.is_empty() {
            self.message = "Le nom du protocole et la famille de pièce sont obligatoires.".into();
            return;
        }
        if self.measurements.is_empty() {
            self.message = "Ajoutez au moins une caractéristique mesurée avant de créer le gabarit versionné.".into();
            return;
        }
        if self.reference_document.as_ref().is_some_and(|document| document.dimensions.iter().any(|dimension| {
            dimension.measurement_id.is_some_and(|id|
                self.measurements.iter().any(|measurement| measurement.id == id)) && !dimension.confirmed
        })) {
            self.message = "Confirmez les cotes de référence associées avant d’enregistrer le protocole.".into();
            return;
        }
        let protocol_id = self.active_protocol.as_ref()
            .map(|protocol| protocol.id.clone())
            .unwrap_or_else(|| new_id("protocol"));
        let version = self.settings.protocols.iter()
            .filter(|protocol| protocol.id == protocol_id)
            .map(|protocol| protocol.version)
            .max().unwrap_or(0) + 1;
        let expected_measurements = self.measurements.iter().map(|measurement| {
            let reference = self.reference_document.as_ref().and_then(|document| document.dimensions.iter()
                .find(|dimension| dimension.measurement_id == Some(measurement.id)))
                .map(|dimension| storage::ProtocolReferenceDimension {
                    label: dimension.label.trim().to_owned(),
                    nominal_mm: dimension.nominal_mm,
                    tolerance_minus_mm: dimension.tolerance_minus_mm,
                    tolerance_plus_mm: dimension.tolerance_plus_mm,
                    confirmed: dimension.confirmed,
                });
            storage::ProtocolCharacteristic {
                label: if measurement.label.trim().is_empty()
                    || measurement.label.trim().eq_ignore_ascii_case(&format!("M{}", measurement.id))
                {
                    measurement.kind.label().to_owned()
                } else { measurement.label.trim().to_owned() },
                kind: measurement.kind.label().to_owned(),
                method: measurement.method.as_deref().map(str::trim)
                    .filter(|method| !method.is_empty()).map(str::to_owned),
                reference,
            }
        }).collect();
        let protocol = storage::MeasurementProtocol {
            id: protocol_id,
            name: name.to_owned(),
            version,
            part_family: part_family.to_owned(),
            device: self.active_device.clone(),
            device_id: self.active_device_id.clone(),
            optical_setup: self.optical_setup.clone(),
            resolution: [image.width, image.height],
            calibration_profile_id: self.calibration.as_ref().map(|profile| profile.id.clone()),
            mm_per_pixel: self.mm_per_pixel,
            scale_x_mm_per_pixel: self.calibration.as_ref().and_then(|profile| profile.scale_x_mm_per_pixel),
            scale_y_mm_per_pixel: self.calibration.as_ref().and_then(|profile| profile.scale_y_mm_per_pixel),
            expanded_uncertainty_mm: self.calibration.as_ref()
                .and_then(|profile| profile.expanded_uncertainty_mm),
            quality_thresholds: self.quality_thresholds.clone(),
            display_correction: self.display_correction,
            expected_measurements,
            display_precision_mm: 3,
            tolerance_rule: storage::ToleranceRule::ExpandedUncertaintyGuardBand,
            created_at_unix_secs: SystemTime::now().duration_since(UNIX_EPOCH)
                .unwrap_or_default().as_secs(),
        };
        let mut updated_settings = self.settings.clone();
        updated_settings.protocols.push(protocol.clone());
        match storage::save_settings(&updated_settings) {
            Ok(()) => {
                self.settings = updated_settings;
                self.assign_protocol_to_current_measurements(&protocol);
                self.invalidate_inspection_inputs();
                self.active_protocol = Some(protocol.clone());
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
                self.message = format!("Protocole {} · version {} enregistré et appliqué aux mesures de référence.",
                    protocol.name, protocol.version);
            }
            Err(error) => self.message = format!("Protocole non enregistré : {error}"),
        }
    }

    fn assign_protocol_to_current_measurements(&mut self, protocol: &storage::MeasurementProtocol) {
        let assign = |measurement: &mut Measurement| {
            measurement.protocol_id = Some(protocol.id.clone());
            measurement.protocol_version = Some(protocol.version);
        };
        for measurement in &mut self.measurements { assign(measurement); }
        for action in self.undo_history.iter_mut().chain(self.redo_history.iter_mut()) {
            match action {
                MeasurementHistoryAction::AddedMany { measurements, .. }
                | MeasurementHistoryAction::RemovedMany { measurements, .. } => {
                    for (_, measurement) in measurements { assign(measurement); }
                }
                MeasurementHistoryAction::EditedMany { before, after, .. } => {
                    for measurement in before.iter_mut().chain(after) { assign(measurement); }
                }
                MeasurementHistoryAction::DisplayAdjusted { .. } => {},
                MeasurementHistoryAction::Added { measurement, .. }
                | MeasurementHistoryAction::Removed { measurement, .. } => assign(measurement),
                MeasurementHistoryAction::Edited { before, after, .. } => {
                    assign(before);
                    assign(after);
                }
                MeasurementHistoryAction::Cleared { measurements, .. } => {
                    for measurement in measurements { assign(measurement); }
                }
            }
        }
    }

    fn activate_protocol(&mut self, protocol_id: &str, ctx: &egui::Context) {
        let Some(protocol) = self.settings.protocols.iter()
            .filter(|protocol| protocol.id == protocol_id)
            .max_by_key(|protocol| protocol.version).cloned() else { return; };
        let resolution_matches = self.image.as_ref().is_some_and(|image|
            protocol.resolution == [image.width, image.height]);
        let device_matches = if !protocol.device_id.is_empty() && !self.active_device_id.is_empty() {
            protocol.device_id == self.active_device_id
        } else { protocol.device.is_empty() || protocol.device == self.active_device };
        self.protocol_name_draft = protocol.name.clone();
        self.protocol_family_draft = protocol.part_family.clone();
        self.optical_setup = protocol.optical_setup.clone();
        self.quality_thresholds = protocol.quality_thresholds.clone();
        self.display_correction = protocol.display_correction;
        if let Some(profile_id) = protocol.calibration_profile_id.as_deref() {
            let profile = self.calibration_history.iter().chain(self.settings.calibrations.iter())
                .find(|profile| profile.id == profile_id && resolution_matches
                    && profile.device_id == protocol.device_id
                    && profile.resolution == protocol.resolution
                    && profile.optical_setup == protocol.optical_setup
                    && profile.validation_status != "failed").cloned()
                .or_else(|| self.calibration.as_ref()
                    .filter(|profile| profile.id == profile_id && resolution_matches
                        && profile.device_id == protocol.device_id
                        && profile.resolution == protocol.resolution
                        && profile.optical_setup == protocol.optical_setup
                        && profile.validation_status != "failed").cloned());
            self.calibration = profile.clone();
            self.mm_per_pixel = profile.as_ref().map(|profile| profile.mm_per_pixel);
            self.expanded_uncertainty_mm = profile.as_ref()
                .and_then(|profile| profile.expanded_uncertainty_mm)
                .map(|value| value.to_string()).unwrap_or_default();
            if let Some(profile) = profile {
                if !self.calibration_history.iter().any(|existing| existing.id == profile.id) {
                    self.calibration_history.push(profile);
                }
            }
        } else {
            self.calibration = None;
            self.mm_per_pixel = None;
            self.expanded_uncertainty_mm.clear();
        }
        self.invalidate_inspection_inputs();
        self.active_protocol = Some(protocol.clone());
        self.assess_current_quality();
        self.refresh_display_texture(ctx);
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        self.message = if resolution_matches && device_matches {
            format!("Protocole {} · v{} activé.", protocol.name, protocol.version)
        } else {
            let difference = [(!device_matches).then_some("caméra différente"),
                (!resolution_matches).then_some("résolution différente")].into_iter()
                .flatten().collect::<Vec<_>>().join(" et ");
            format!("Protocole {} · v{} chargé, {difference} : verdict de série suspendu.",
                protocol.name, protocol.version)
        };
    }

    fn activate_saved_calibration(&mut self) {
        if self.image_is_imported {
            return;
        }
        let Some(img) = &self.image else { return; };
        let selected = self.settings.calibrations.iter().rev().find(|c| {
            ((!c.device_id.is_empty() && !self.active_device_id.is_empty() && c.device_id == self.active_device_id)
                || (c.device_id.is_empty() && c.device == self.active_device))
                && c.resolution == [img.width, img.height]
                && c.optical_setup == self.optical_setup
                && c.validation_status != "failed"
        }).cloned();
        if let Some(profile) = &selected {
            if !self.calibration_history.iter().any(|saved| saved.id == profile.id) {
                self.calibration_history.push(profile.clone());
            }
        }
        if serde_json::to_value(&self.calibration).ok() != serde_json::to_value(&selected).ok() {
            self.invalidate_inspection_inputs();
        }
        self.mm_per_pixel = selected.as_ref().map(|p| p.mm_per_pixel);
        self.expanded_uncertainty_mm = selected.as_ref()
            .and_then(|profile| profile.expanded_uncertainty_mm)
            .map(|value| value.to_string()).unwrap_or_default();
        self.calibration = selected;
    }

    fn refresh_cameras(&mut self) {
        self.cameras = camera::devices();
        self.selected_camera = 0;
        self.camera_modes.clear();
        self.camera_modes_camera = None;
        self.selected_camera_mode = None;
        self.camera_controls.clear();
        self.camera_controls_camera = None;
        self.message = format!("{} caméra(s) trouvée(s)", self.cameras.len());
    }

    fn camera_control_metadata(&self) -> Vec<CameraControlMetadata> {
        self.camera_controls.iter().map(|control| CameraControlMetadata {
            id: format!("{:?}", control.id),
            name: control.name.clone(),
            value: format!("{:?}", control.value),
            flags: control.flags.clone(),
            active: control.active,
            writable: control.writable,
        }).collect()
    }

    fn start_camera(&mut self) {
        if self.timelapse_capture.is_some() || self.capture_timer.is_some() || self.video_recording.is_some() {
            self.message = "Arrêtez la capture différée ou le time-lapse avant de changer de caméra.".into();
            return;
        }
        match self.start_camera_index(self.selected_camera) {
            Ok(()) => self.message = "Ouverture du flux USB UVC…".into(),
            Err(error) => self.message = error,
        }
    }

    fn freeze_capture(&mut self) {
        if self.running && !self.paused {
            self.paused = true;
            self.capture_id = new_capture_id();
            self.document_revision = self.document_revision.wrapping_add(1);
            self.assess_current_quality();
            self.message = format!("Image figée pour le contexte {}", self.capture_id);
        }
    }

    fn toggle_stream_pause(&mut self) {
        if !self.paused {
            self.freeze_capture();
            return;
        }
        if !self.pending.is_empty() || self.point_edit.is_some() || self.calibration_mode {
            self.message = "Terminez ou annulez l'action en cours avant de reprendre le flux.".into();
            return;
        }
        if !self.session_saved && !self.measurements.is_empty() {
            self.message = "Enregistrez le contexte avant la reprise : les mesures liées à l'image seraient effacées.".into();
            return;
        }
        self.measurements.clear();
        let reference_associations_reset = if let Some(document) = self.reference_document.as_mut() {
            for dimension in &mut document.dimensions { dimension.measurement_id = None; }
            true
        } else { false };
        self.clear_pending();
        self.undo_history.clear(); self.redo_history.clear(); self.point_edit = None;
        self.calibration_history.clear();
        if let Some(profile) = self.calibration.clone() { self.calibration_history.push(profile); }
        self.selected_measurement = None;
        self.selection_mode = false;
        self.capture_id = new_capture_id();
        self.document_revision = self.document_revision.wrapping_add(1);
        self.session_saved = !reference_associations_reset;
        self.paused = false;
        self.message = "Flux repris : les mesures suivantes appartiendront à une nouvelle capture.".into();
    }

    fn poll_camera(&mut self, ctx: &egui::Context) {
        if !self.running { return; }
        let mut latest = None;
        let mut error = None;
        let mut video_format_changed = false;
        if let Some(task) = &self.camera_task {
            // Ne traiter que la dernière trame : les anciennes sont obsolètes.
            while let Ok(msg) = task.receiver.try_recv() {
                match msg {
                    CameraMessage::Modes(modes) => {
                        let same_camera = self.camera_modes_camera.as_deref()
                            == Some(self.active_device_id.as_str());
                        let default_mode = modes.iter().position(|mode| mode.format.width() == 1920
                            && mode.format.height() == 1080 && mode.format.frame_rate() == 30)
                            .or_else(|| modes.iter().enumerate()
                                .max_by_key(|(_, mode)| mode.format.frame_rate()).map(|(index, _)| index));
                        if !same_camera || self.selected_camera_mode.is_none_or(|index| index >= modes.len()) {
                            self.selected_camera_mode = default_mode;
                        }
                        self.camera_modes = modes;
                        self.camera_modes_camera = Some(self.active_device_id.clone());
                    }
                    CameraMessage::Controls(controls) => {
                        self.camera_controls = controls;
                        self.camera_controls_camera = Some(self.active_device_id.clone());
                    }
                    CameraMessage::ControlUpdated(setting) => {
                        if let Some(current) = self.camera_controls.iter_mut()
                            .find(|current| current.id == setting.id)
                        { *current = setting; }
                    }
                    CameraMessage::ControlError(err) => {
                        self.message = format!("Réglage caméra refusé par le pilote : {err}");
                    }
                    CameraMessage::Error(err) => error = Some(err),
                }
            }
            latest = task.take_latest_frame();
        }
        if let Some(frame) = latest.as_ref() {
            if let Some(recording) = self.video_recording.as_ref() {
                if frame.width != recording.metadata.resolution[0]
                    || frame.height != recording.metadata.resolution[1]
                    || frame.frame_rate != recording.metadata.frame_rate
                {
                    video_format_changed = true;
                } else if let Some(recorder) = recording.recorder.as_ref() {
                    let _ = recorder.submit_frame(frame);
                }
            }
        }
        if video_format_changed {
            let notice = self.stop_video_recording("interrompu");
            self.message = notice.unwrap_or_else(||
                "Enregistrement MP4 arrêté : le format ou la cadence caméra a changé.".into());
        }
        if let Some(err) = error {
            self.message = format!("Erreur caméra : {err}");
            self.running = false;
            self.camera_task = None;
            if let Some(notice) = self.stop_video_recording("interrompu") {
                self.message.push_str(" · ");
                self.message.push_str(&notice);
            }
            if let Some(notice) = self.stop_time_lapse("interrompu", true) {
                self.message.push_str(" · ");
                self.message.push_str(&notice);
            }
        }
        if let Some(frame) = latest {
            if !self.paused {
                self.camera_frame_rate = frame.frame_rate.max(1);
                if let Some(timer) = self.capture_timer.as_mut() {
                    timer.frames_seen = timer.frames_seen.wrapping_add(1);
                    if timer.frames_seen % 8 == 0 {
                        let score = quality::assess_image_quality(
                            &frame.rgb, frame.width, frame.height, &self.quality_thresholds,
                        ).map(|assessment| assessment.metrics.laplacian_variance)
                            .unwrap_or(f64::NEG_INFINITY);
                        if score.is_finite() && score > timer.best_sharpness {
                            timer.best_sharpness = score;
                            timer.best_frame = Some(camera::Frame {
                                width: frame.width, height: frame.height,
                                frame_rate: frame.frame_rate, rgb: frame.rgb.clone(),
                            });
                        }
                    }
                }
                if let Some(capture) = self.timelapse_capture.as_mut() {
                    capture.frames_seen = capture.frames_seen.wrapping_add(1);
                    if capture.frames_seen % 8 == 0 {
                        let score = quality::assess_image_quality(
                            &frame.rgb, frame.width, frame.height, &self.quality_thresholds,
                        ).map(|assessment| assessment.metrics.laplacian_variance)
                            .unwrap_or(f64::NEG_INFINITY);
                        if score.is_finite() && score > capture.best_sharpness {
                            capture.best_sharpness = score;
                            capture.best_frame = Some(camera::Frame {
                                width: frame.width, height: frame.height,
                                frame_rate: frame.frame_rate, rgb: frame.rgb.clone(),
                            });
                        }
                    }
                }
                let assessment = alignment::assess(
                    &frame.rgb, frame.width, frame.height, self.alignment_tolerance_degrees,
                );
                self.live_alignment = assessment.state;
                self.live_alignment_assessment = Some(assessment);
                self.set_image(ctx, ImageData {
                    width: frame.width, height: frame.height,
                    rgb: frame.rgb, source: "Flux USB direct".into(),
                }, false);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(35));
    }

    fn poll_capture_timer(&mut self, ctx: &egui::Context) {
        let ready = self.capture_timer.as_ref().is_some_and(|timer| Instant::now() >= timer.capture_at);
        if !ready { return; }
        let Some(mut timer) = self.capture_timer.take() else { return; };
        let frame = timer.best_frame.take().or_else(|| self.image.as_ref().map(|image| camera::Frame {
            width: image.width, height: image.height,
            frame_rate: self.camera_frame_rate, rgb: image.rgb.clone(),
        }));
        let Some(frame) = frame else {
            self.message = "Capture différée annulée : aucune image disponible.".into();
            return;
        };
        self.set_image(ctx, ImageData {
            width: frame.width, height: frame.height, rgb: frame.rgb,
            source: "Flux USB direct · meilleur piqué observé".into(),
        }, false);
        match self.capture_photo_to_path(&timer.destination, false) {
            Ok(path) => self.message = format!(
                "Photo différée enregistrée : {path} · image la plus nette observée pendant la fenêtre de capture.",
            ),
            Err(error) => self.message = error,
        }
    }

    fn start_time_lapse(&mut self) {
        if !self.running || self.paused {
            self.message = "Le time-lapse nécessite un flux caméra en direct.".into();
            return;
        }
        if self.capture_timer.is_some() || self.timelapse_capture.is_some() || self.video_recording.is_some() {
            self.message = "Une capture différée ou un time-lapse est déjà en cours.".into();
            return;
        }
        let Some(image) = self.image.as_ref() else {
            self.message = "Aucune image caméra disponible pour le time-lapse.".into();
            return;
        };
        let Some(parent) = rfd::FileDialog::new().set_directory(&self.directory).pick_folder() else { return; };
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let folder = parent.join(format!("DPER_MEASURE_timelapse_{stamp}"));
        if let Err(error) = fs::create_dir_all(&folder) {
            self.message = format!("Dossier du time-lapse inaccessible : {error}");
            return;
        }
        let now = Instant::now();
        let manifest = TimeLapseManifest {
            format_version: 1,
            status: "en cours".into(),
            device: self.active_device.clone(), device_id: self.active_device_id.clone(),
            optical_setup: self.optical_setup.clone(), capture_id: self.capture_id.clone(),
            resolution: [image.width, image.height], interval_seconds: self.timelapse_interval_seconds,
            camera_frame_rate: self.camera_frame_rate,
            camera_controls: self.camera_control_metadata(),
            expected_frame_count: self.timelapse_frame_count,
            started_at_unix_secs: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            calibration_id: self.calibration.as_ref().map(|profile| profile.id.clone()),
            scale_x_mm_per_pixel: self.calibration.as_ref()
                .and_then(|profile| profile.scale_x_mm_per_pixel.or(Some(profile.mm_per_pixel))),
            scale_y_mm_per_pixel: self.calibration.as_ref()
                .and_then(|profile| profile.scale_y_mm_per_pixel.or(Some(profile.mm_per_pixel))),
            expanded_uncertainty_mm: self.calibration.as_ref()
                .and_then(|profile| profile.expanded_uncertainty_mm),
            protocol_id: self.active_protocol.as_ref().map(|protocol| protocol.id.clone()),
            protocol_version: self.active_protocol.as_ref().map(|protocol| protocol.version),
            quality_thresholds: self.quality_thresholds.clone(),
            display_correction: self.display_correction,
            video_file: None, frames: Vec::new(),
        };
        self.timelapse_capture = Some(TimeLapseCapture {
            folder, interval_seconds: self.timelapse_interval_seconds,
            next_frame_at: now + Duration::from_secs(self.timelapse_interval_seconds as u64),
            started: now, frame_limit: self.timelapse_frame_count,
            frames_seen: 0, best_frame: None, best_sharpness: f64::NEG_INFINITY, manifest,
        });
        if let Some(capture) = self.timelapse_capture.as_ref() {
            if let Err(error) = write_timelapse_manifest(capture) {
                self.timelapse_capture = None;
                self.message = error;
                return;
            }
            self.message = format!(
                "Time-lapse démarré · {} images toutes les {} s · piqué maximal par intervalle.",
                capture.frame_limit, capture.interval_seconds,
            );
        }
    }

    fn poll_time_lapse(&mut self) {
        let ready = self.timelapse_capture.as_ref()
            .is_some_and(|capture| Instant::now() >= capture.next_frame_at);
        if !ready { return; }
        let fallback = self.image.as_ref().map(|image| camera::Frame {
            width: image.width, height: image.height,
            frame_rate: self.camera_frame_rate, rgb: image.rgb.clone(),
        });
        let outcome: Result<bool, String> = (|| {
            let capture = self.timelapse_capture.as_mut()
                .ok_or_else(|| "Time-lapse interrompu.".to_owned())?;
            let frame = capture.best_frame.take().or(fallback)
                .ok_or_else(|| "Aucune trame disponible pour le time-lapse.".to_owned())?;
            let index = capture.manifest.frames.len() as u16 + 1;
            let file = format!("frame-{index:04}.png");
            let path = capture.folder.join(&file);
            let sharpness = capture.best_sharpness.is_finite().then_some(capture.best_sharpness);
            image::save_buffer_with_format(
                &path, &frame.rgb, frame.width, frame.height,
                image::ColorType::Rgb8, image::ImageFormat::Png,
            ).map_err(|error| format!("Écriture du time-lapse impossible : {error}"))?;
            capture.manifest.frames.push(TimeLapseFrameEntry {
                index, file, captured_at_unix_secs: SystemTime::now()
                    .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                elapsed_ms: capture.started.elapsed().as_millis(), sharpness_score: sharpness,
            });
            capture.best_sharpness = f64::NEG_INFINITY;
            capture.best_frame = None;
            capture.frames_seen = 0;
            capture.next_frame_at += Duration::from_secs(capture.interval_seconds as u64);
            write_timelapse_manifest(capture)?;
            Ok(capture.manifest.frames.len() >= capture.frame_limit as usize)
        })();
        match outcome {
            Ok(true) => {
                let notice = self.stop_time_lapse("terminé", true);
                self.message = notice.unwrap_or_else(|| "Time-lapse terminé et vidéo MP4 créée.".into());
            }
            Ok(false) => {
                if let Some(capture) = self.timelapse_capture.as_ref() {
                    self.message = format!("Time-lapse · {} / {} images enregistrées",
                        capture.manifest.frames.len(), capture.frame_limit);
                }
            }
            Err(error) => {
                let _ = self.stop_time_lapse("interrompu", false);
                self.message = error;
            }
        }
    }

    fn stop_time_lapse(&mut self, status: &str, encode_video: bool) -> Option<String> {
        let mut capture = self.timelapse_capture.take()?;
        capture.manifest.status = status.to_owned();
        let mut notice = None;
        if encode_video && capture.manifest.frames.len() >= 2 {
            let pattern = capture.folder.join("frame-%04d.png");
            let output_path = capture.folder.join("timelapse.mp4");
            let frame_rate = format!("1/{}", capture.interval_seconds.max(1));
            let result = Command::new("ffmpeg").args([
                "-y", "-framerate", &frame_rate, "-start_number", "1", "-i",
            ]).arg(pattern).args([
                "-vf", "crop=trunc(iw/2)*2:trunc(ih/2)*2", "-c:v", "libx264", "-pix_fmt", "yuv420p",
            ]).arg(&output_path).output();
            match result {
                Ok(result) if result.status.success() => {
                    capture.manifest.video_file = Some("timelapse.mp4".into());
                }
                Ok(result) => notice = Some(format!(
                    "Images du time-lapse conservées; FFmpeg n'a pas créé la vidéo : {}",
                    String::from_utf8_lossy(&result.stderr).trim(),
                )),
                Err(error) => notice = Some(format!(
                    "Images du time-lapse conservées; installez FFmpeg pour créer le MP4 : {error}"
                )),
            }
        }
        if let Err(error) = write_timelapse_manifest(&capture) {
            notice = Some(format!("Manifeste du time-lapse impossible à écrire : {error}"));
        }
        notice.or_else(|| {
            let count = capture.manifest.frames.len();
            let message = capture.manifest.video_file.as_ref()
                .map(|file| format!("{count} trames enregistrées · MP4 : {}", capture.folder.join(file).display()))
                .unwrap_or_else(|| format!("{count} trames PNG et le manifeste ont été enregistrés dans {}", capture.folder.display()));
            Some(message)
        })
    }

    fn start_video_recording(&mut self) {
        if !self.running || self.paused {
            self.message = "L'enregistrement MP4 nécessite un flux caméra en direct.".into();
            return;
        }
        if self.video_recording.is_some() || self.capture_timer.is_some() || self.timelapse_capture.is_some() {
            self.message = "Un enregistrement, une capture différée ou un time-lapse est déjà en cours.".into();
            return;
        }
        let Some(image) = self.image.as_ref() else {
            self.message = "Aucune image caméra disponible pour l'enregistrement.".into();
            return;
        };
        let Some(mut path) = rfd::FileDialog::new().set_directory(&self.directory)
            .set_file_name("inspection.mp4").add_filter("Vidéo MP4", &["mp4"]).save_file() else { return; };
        if path.extension().and_then(|extension| extension.to_str())
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("mp4"))
        { path.set_extension("mp4"); }
        if path.exists() {
            self.message = format!("Le fichier vidéo existe déjà : {}", path.display());
            return;
        }
        let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or("inspection");
        let metadata_path = path.with_file_name(format!("{stem}.dper_measure.json"));
        if metadata_path.exists() {
            self.message = format!("Le sidecar vidéo existe déjà : {}", metadata_path.display());
            return;
        }
        let mut metadata = VideoRecordingMetadata {
            format_version: 1, status: "en cours".into(),
            video_file: path.file_name().and_then(|value| value.to_str()).unwrap_or("inspection.mp4").to_owned(),
            device: self.active_device.clone(), device_id: self.active_device_id.clone(),
            resolution: [image.width, image.height], frame_rate: self.camera_frame_rate.max(1),
            camera_controls: self.camera_control_metadata(),
            optical_setup: self.optical_setup.clone(), capture_id: self.capture_id.clone(),
            calibration_id: self.calibration.as_ref().map(|profile| profile.id.clone()),
            calibration_status: self.calibration.as_ref().map(|profile| profile.validation_status.clone()),
            scale_x_mm_per_pixel: self.calibration.as_ref()
                .and_then(|profile| profile.scale_x_mm_per_pixel.or(Some(profile.mm_per_pixel))),
            scale_y_mm_per_pixel: self.calibration.as_ref()
                .and_then(|profile| profile.scale_y_mm_per_pixel.or(Some(profile.mm_per_pixel))),
            expanded_uncertainty_mm: self.calibration.as_ref()
                .and_then(|profile| profile.expanded_uncertainty_mm),
            protocol_id: self.active_protocol.as_ref().map(|protocol| protocol.id.clone()),
            protocol_version: self.active_protocol.as_ref().map(|protocol| protocol.version),
            quality_thresholds: self.quality_thresholds.clone(), display_correction: self.display_correction,
            started_at_unix_secs: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            completed_at_unix_secs: None, frames_written: None, frames_dropped: None, error: None,
        };
        if let Err(error) = write_video_metadata(&metadata_path, &metadata) {
            self.message = error;
            return;
        }
        let recorder = match camera::VideoRecorder::start(
            &path, image.width, image.height, self.camera_frame_rate,
        ) {
            Ok(recorder) => recorder,
            Err(error) => {
                metadata.status = "échec".into();
                metadata.error = Some(error.clone());
                metadata.completed_at_unix_secs = Some(SystemTime::now()
                    .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                let _ = write_video_metadata(&metadata_path, &metadata);
                self.message = error;
                return;
            }
        };
        self.video_recording = Some(VideoRecording {
            path: path.clone(), metadata_path, recorder: Some(recorder), metadata,
        });
        self.message = format!(
            "Enregistrement MP4 démarré à {} i/s · sidecar métrologique associé.",
            self.camera_frame_rate.max(1),
        );
    }

    fn stop_video_recording(&mut self, status: &str) -> Option<String> {
        let mut recording = self.video_recording.take()?;
        match recording.close(status) {
            Ok(()) => Some(format!(
                "Vidéo {} · {} trames écrites, {} perdues · sidecar {}",
                recording.path.display(), recording.metadata.frames_written.unwrap_or(0),
                recording.metadata.frames_dropped.unwrap_or(0), recording.metadata_path.display(),
            )),
            Err(error) => Some(format!("Arrêt vidéo incomplet : {error} · sidecar {}",
                recording.metadata_path.display())),
        }
    }

    fn open_image(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg", "bmp", "tif", "tiff"]).pick_file() else { return; };
        match self.open_image_path(ctx, &path) {
            Ok(()) => self.message = format!("Ouvert : {}", path.display()),
            Err(error) => self.message = error,
        }
    }

    fn open_demo_calibration_image(&mut self, ctx: &egui::Context) {
        let mut candidates = Vec::new();
        if let Some(executable_dir) = std::env::current_exe().ok().and_then(|path| path.parent().map(Path::to_path_buf)) {
            candidates.push(executable_dir.join("assets").join("demo_calibration_virtuelle.png"));
        }
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets").join("demo_calibration_virtuelle.png"));
        let Some(path) = candidates.into_iter().find(|path| path.is_file()) else {
            self.message = "Mire de démonstration introuvable; vérifiez le dossier assets de l'installation.".into();
            return;
        };
        match self.open_image_path(ctx, &path) {
            Ok(()) => self.message = "Mire virtuelle ouverte · démonstration uniquement, pas un étalon physique.".into(),
            Err(error) => self.message = error,
        }
    }

    fn guided_examples_window(&mut self, ctx: &egui::Context) {
        if !self.show_guided_examples { return; }
        let mut open_demo = false;
        egui::Window::new("Parcours guidés")
            .open(&mut self.show_guided_examples)
            .default_width(560.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Calibrer puis mesurer une image, associer une cote de plan et préparer un dossier.")
                    .strong());
                ui.add_space(8.0);
                ui.collapsing("1 · Calibration virtuelle et mesure", |ui| {
                    ui.label("Ouvrez la mire de démonstration, choisissez l'outil Calibration par segment, puis cliquez sur les graduations 0 et 10. Saisissez 10 mm comme longueur de référence. Une graduation représente alors 1 mm dans cet exemple.");
                    ui.add_space(4.0);
                    if ui.button("Ouvrir la mire virtuelle").clicked() { open_demo = true; }
                    ui.weak("Mire synthétique pour apprendre l'interface; elle ne constitue pas un étalon ni une validation métrologique.");
                    ui.label("Après le calibrage, choisissez Distance et mesurez entre deux graduations voisines. Enregistrez le projet pour conserver calibration, méthode et points.");
                });
                ui.collapsing("2 · Comparaison à un plan", |ui| {
                    ui.label("Ouvrez une image de pièce et le PDF ou l'image du plan. Vérifiez les cotes reconnues par OCR, corrigez les valeurs si nécessaire, confirmez-les, puis associez chaque cote à la mesure correspondante.");
                    ui.label("Le statut reste à confirmer ou suspendu si l'incertitude, la qualité d'image ou la comparabilité du protocole ne permet pas de conclure.");
                });
                ui.collapsing("3 · Rapport par groupes", |ui| {
                    ui.label("Pour chaque image, ajoutez et nommez les mesures par caractéristique. Dans Exporter dossier, sélectionnez uniquement les images avec mesures, complétez les informations d'audit, puis produisez le PDF groupé et ses tableaux.");
                    ui.label("Les groupes sont ordonnés par caractéristique/type puis divisés par zone si nécessaire; chaque statut reprend l'incertitude et les règles de suspension.");
                });
            });
        if open_demo { self.open_demo_calibration_image(ctx); }
    }

    fn image_derivation_ready(&self) -> Result<(), String> {
        if self.running && !self.paused {
            return Err("Figez le flux caméra avant un traitement d'image.".into());
        }
        if self.capture_timer.is_some() || self.timelapse_capture.is_some() || self.video_recording.is_some() {
            return Err("Arrêtez les captures en cours avant un traitement d'image.".into());
        }
        if !self.session_saved || !self.pending.is_empty() || self.mcp_mutation_blocked() {
            return Err("Enregistrez le projet et terminez ou annulez le tracé avant de créer une image dérivée.".into());
        }
        Ok(())
    }

    fn save_derived_image(
        &mut self,
        ctx: &egui::Context,
        mut destination: PathBuf,
        derived: image::RgbImage,
        mut manifest: serde_json::Value,
    ) {
        if destination.extension().and_then(|extension| extension.to_str())
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("png"))
        { destination.set_extension("png"); }
        let stem = destination.file_stem().and_then(|value| value.to_str()).unwrap_or("image_derivee");
        let metadata_path = destination.with_file_name(format!("{stem}.dper_measure.json"));
        if destination.exists() || metadata_path.exists() {
            self.message = format!("Le fichier image ou son manifeste existe déjà : {}", destination.display());
            return;
        }
        if let Err(error) = derived.save(&destination) {
            self.message = format!("Écriture de l'image dérivée impossible : {error}");
            return;
        }
        if let Some(object) = manifest.as_object_mut() {
            object.insert("output_file".into(), serde_json::Value::String(
                destination.file_name().and_then(|name| name.to_str()).unwrap_or("image.png").to_owned()));
            object.insert("output_dimensions".into(), serde_json::json!([
                derived.width(), derived.height()
            ]));
        }
        let bytes = match serde_json::to_vec_pretty(&manifest) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.message = format!("Image écrite, mais le manifeste n'a pas pu être sérialisé : {error}");
                return;
            }
        };
        if let Err(error) = fs::write(&metadata_path, bytes) {
            self.message = format!("Image écrite, mais le manifeste associé est inaccessible : {error}");
            return;
        }
        match self.open_image_path(ctx, &destination) {
            Ok(()) => self.message = format!("Image dérivée créée : {} · source conservée · recalibration requise.", destination.display()),
            Err(error) => self.message = format!("Image et manifeste créés; réouverture différée : {error}"),
        }
    }

    fn apply_flat_field(&mut self, ctx: &egui::Context) {
        if let Err(error) = self.image_derivation_ready() { self.message = error; return; }
        let Some(source) = self.image.as_ref() else {
            self.message = "Ouvrez une image source avant la correction flat-field.".into();
            return;
        };
        let source_rgb = image::RgbImage::from_raw(source.width, source.height, source.rgb.clone());
        let Some(source_rgb) = source_rgb else {
            self.message = "Image source invalide pour la correction flat-field.".into();
            return;
        };
        let Some(flat_path) = rfd::FileDialog::new().set_title("Image flat-field uniforme")
            .add_filter("Image", &["png", "jpg", "jpeg", "tif", "tiff", "bmp"]).pick_file() else { return; };
        let flat = match image::open(&flat_path) {
            Ok(image) => image.to_rgb8(),
            Err(error) => { self.message = format!("Image flat-field illisible : {error}"); return; }
        };
        let dark_path = rfd::FileDialog::new().set_title("Image sombre facultative · Annuler pour ignorer")
            .add_filter("Image", &["png", "jpg", "jpeg", "tif", "tiff", "bmp"]).pick_file();
        let dark = match dark_path.as_ref().map(|path| image::open(path).map(|image| image.to_rgb8())) {
            Some(Ok(image)) => Some(image),
            Some(Err(error)) => { self.message = format!("Image sombre illisible : {error}"); return; }
            None => None,
        };
        let corrected = match imaging::flat_field_correct(&source_rgb, &flat, dark.as_ref()) {
            Ok(image) => image,
            Err(error) => { self.message = error; return; }
        };
        let source_stem = Path::new(&source.source).file_stem().and_then(|stem| stem.to_str())
            .unwrap_or("image");
        let suggestion = format!("{source_stem}_flatfield.png");
        let Some(destination) = rfd::FileDialog::new().set_file_name(suggestion)
            .add_filter("PNG", &["png"]).save_file() else { return; };
        let manifest = serde_json::json!({
            "format_version": 1,
            "processing": "flat_field_division",
            "source_image": source.source,
            "flat_field_image": flat_path.to_string_lossy(),
            "dark_frame_image": dark_path.map(|path| path.to_string_lossy().into_owned()),
            "algorithm": "(source - dark) * mean(flat - dark) / max(flat - dark, 2)",
            "display_correction_applied": false,
            "calibration_carried_over": false
        });
        self.save_derived_image(ctx, destination, corrected, manifest);
    }

    fn apply_optical_distortion_correction(&mut self, ctx: &egui::Context) {
        if let Err(error) = self.image_derivation_ready() { self.message = error; return; }
        let Some(source) = self.image.as_ref() else {
            self.message = "Ouvrez une image source avant la correction de distorsion optique.".into();
            return;
        };
        let source_rgb = image::RgbImage::from_raw(source.width, source.height, source.rgb.clone());
        let Some(source_rgb) = source_rgb else {
            self.message = "Image source invalide pour la correction de distorsion optique.".into();
            return;
        };
        let source_name = source.source.clone();
        let source_stem = Path::new(&source_name).file_stem().and_then(|stem| stem.to_str())
            .unwrap_or("image");
        let Some(calibration_path) = rfd::FileDialog::new().set_title("Profil de distorsion optique JSON")
            .add_filter("Profil optique JSON", &["json"]).pick_file() else { return; };
        let calibration_bytes = match fs::read(&calibration_path) {
            Ok(bytes) => bytes,
            Err(error) => { self.message = format!("Lecture du profil optique impossible : {error}"); return; }
        };
        let calibration = match serde_json::from_slice::<imaging::RadialDistortionCalibration>(&calibration_bytes) {
            Ok(calibration) => calibration,
            Err(error) => { self.message = format!("Profil optique JSON invalide : {error}"); return; }
        };
        let corrected = match imaging::radial_distortion_correct(&source_rgb, &calibration) {
            Ok(image) => image,
            Err(error) => { self.message = error; return; }
        };
        let suggestion = format!("{source_stem}_distorsion_corrigee.png");
        let Some(destination) = rfd::FileDialog::new().set_file_name(suggestion)
            .add_filter("PNG", &["png"]).save_file() else { return; };
        let manifest = serde_json::json!({
            "format_version": 1,
            "processing": "radial_tangential_distortion_correction",
            "source_image": source_name,
            "calibration_profile": calibration_path.to_string_lossy(),
            "camera_intrinsics_px": {
                "fx": calibration.fx_px, "fy": calibration.fy_px,
                "cx": calibration.cx_px, "cy": calibration.cy_px
            },
            "radial_coefficients": {"k1": calibration.k1, "k2": calibration.k2, "k3": calibration.k3},
            "tangential_coefficients": {"p1": calibration.p1, "p2": calibration.p2},
            "model": "x_d=x*(1+k1*r2+k2*r4+k3*r6)+2*p1*x*y+p2*(r2+2*x2); y_d=y*(1+k1*r2+k2*r4+k3*r6)+p1*(r2+2*y2)+2*p2*x*y",
            "interpolation": "bilinear",
            "border_policy": "pixels mapped outside the source are black",
            "tangential_distortion_corrected": true,
            "calibration_carried_over": false
        });
        self.save_derived_image(ctx, destination, corrected, manifest);
    }

    fn calibrate_optical_distortion(&mut self) {
        let Some(observations_path) = rfd::FileDialog::new()
            .set_title("Observations des coins de mire JSON")
            .add_filter("Observations de calibration", &["json"])
            .pick_file() else { return; };
        let bytes = match fs::read(&observations_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.message = format!("Lecture des observations de mire impossible : {error}");
                return;
            }
        };
        let observations = match serde_json::from_slice::<DistortionCalibrationObservations>(&bytes) {
            Ok(observations) if observations.format_version == 1 => observations,
            Ok(_) => {
                self.message = "Version du fichier d'observations de mire non prise en charge.".into();
                return;
            }
            Err(error) => {
                self.message = format!("Fichier d'observations de mire JSON invalide : {error}");
                return;
            }
        };
        let result = match distortion_calibration::calibrate_brown_conrady(
            observations.image_width,
            observations.image_height,
            observations.inner_corners_cols,
            observations.inner_corners_rows,
            observations.square_spacing_mm,
            &observations.views,
        ) {
            Ok(result) => result,
            Err(error) => {
                self.message = format!("Calibration de distorsion refusée : {error}");
                return;
            }
        };

        let profile = result.profile;
        let profile_json = serde_json::json!({
            "format_version": 1,
            "profile_type": "brown_conrady_optical_distortion",
            "camera": observations.camera,
            "optical_setup": observations.optical_setup,
            "image_width": profile.image_width,
            "image_height": profile.image_height,
            "fx_px": profile.fx_px,
            "fy_px": profile.fy_px,
            "cx_px": profile.cx_px,
            "cy_px": profile.cy_px,
            "k1": profile.k1,
            "k2": profile.k2,
            "k3": profile.k3,
            "p1": profile.p1,
            "p2": profile.p2,
            "method": "brown_conrady_multi_view_checkerboard",
            "inner_corners": [observations.inner_corners_cols, observations.inner_corners_rows],
            "square_spacing_mm": observations.square_spacing_mm,
            "view_count": observations.views.len(),
            "reprojection_rms_px": result.reprojection_rms_px,
            "reprojection_max_px": result.reprojection_max_px,
            "per_view_rms_px": result.per_view_rms_px,
            "observed_width_fraction": result.observed_width_fraction,
            "observed_height_fraction": result.observed_height_fraction,
            "observed_corner_radius_fraction": result.observed_corner_radius_fraction,
            "accepted_iterations": result.accepted_iterations,
            "validation_status": "estimated_unverified",
            "metrology_uncertainty_mm": null,
            "physical_qualification": "not_performed",
            "source_observations_file": observations_path.file_name()
                .and_then(|name| name.to_str()).unwrap_or("observations.json"),
        });
        let Some(mut destination) = rfd::FileDialog::new()
            .set_file_name("profil_distorsion_optique.json")
            .add_filter("Profil optique JSON", &["json"])
            .save_file() else { return; };
        if destination.extension().is_none() { destination.set_extension("json"); }
        let encoded = match serde_json::to_vec_pretty(&profile_json) {
            Ok(encoded) => encoded,
            Err(error) => {
                self.message = format!("Sérialisation du profil optique impossible : {error}");
                return;
            }
        };
        if let Err(error) = fs::write(&destination, encoded) {
            self.message = format!("Écriture du profil optique impossible : {error}");
            return;
        }
        self.message = format!(
            "Profil estimé enregistré : {} · RMS {:.3} px, maximum {:.3} px, {} vue(s). Résultat non qualifié; vérifier avant correction.",
            destination.display(), result.reprojection_rms_px, result.reprojection_max_px,
            observations.views.len(),
        );
    }

    fn create_focus_stack(&mut self, ctx: &egui::Context) {
        if let Err(error) = self.image_derivation_ready() { self.message = error; return; }
        let Some(paths) = rfd::FileDialog::new().set_title("Images de la pile de mise au point")
            .add_filter("Images", &["png", "jpg", "jpeg", "tif", "tiff", "bmp"]).pick_files() else { return; };
        if paths.len() < 2 {
            self.message = "Sélectionnez au moins deux images de mise au point.".into();
            return;
        }
        if paths.len() > 32 {
            self.message = "La pile locale est limitée à 32 images.".into();
            return;
        }
        let mut images = Vec::with_capacity(paths.len());
        for path in &paths {
            match image::open(path) {
                Ok(image) => images.push(image.to_rgb8()),
                Err(error) => { self.message = format!("Image de pile illisible ({}): {error}", path.display()); return; }
            }
        }
        let stacked = match imaging::focus_stack(&images) {
            Ok(image) => image,
            Err(error) => { self.message = error; return; }
        };
        let Some(destination) = rfd::FileDialog::new().set_file_name("empilement_focus.png")
            .add_filter("PNG", &["png"]).save_file() else { return; };
        let manifest = serde_json::json!({
            "format_version": 1,
            "processing": "focus_stack_local_variance",
            "source_images": paths.iter().map(|path| path.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "patch_size_pixels": 32,
            "patch_stride_pixels": 16,
            "algorithm": "local Laplacian variance selection with feathered patch blending",
            "calibration_carried_over": false
        });
        self.save_derived_image(ctx, destination, stacked, manifest);
    }

    fn start_assisted_panorama(&mut self) {
        if let Err(error) = self.image_derivation_ready() { self.message = error; return; }
        let Some(paths) = rfd::FileDialog::new().set_title("Sélectionner les images du panorama")
            .add_filter("Images", &["png", "jpg", "jpeg", "tif", "tiff", "bmp"]).pick_files() else { return; };
        if !(2..=32).contains(&paths.len()) {
            self.message = "Sélectionnez entre 2 et 32 images pour le panorama.".into();
            return;
        }
        let mut images = Vec::with_capacity(paths.len());
        let mut input_pixels = 0_u64;
        for path in &paths {
            let image = match image::open(path) {
                Ok(image) => image.to_rgb8(),
                Err(error) => { self.message = format!("Image de panorama illisible ({}): {error}", path.display()); return; }
            };
            input_pixels = match (image.width() as u64).checked_mul(image.height() as u64)
                .and_then(|pixels| input_pixels.checked_add(pixels))
            {
                Some(total) if total <= 30_000_000 => total,
                _ => { self.message = "La sélection dépasse la limite mémoire locale du panorama.".into(); return; }
            };
            images.push(image);
        }
        let mut offsets: Vec<(i32, i32)> = vec![(0, 0)];
        let mut transforms_source_to_panorama = vec![[
            1.0, 0.0, 0.0,
            0.0, 1.0, 0.0,
            0.0, 0.0, 1.0,
        ]];
        let mut alignment_confidences = vec![None];
        let mut alignment_details = vec!["Image d’ancrage".to_owned()];
        for index in 1..images.len() {
            let previous = &images[index - 1];
            let current = &images[index];
            let feature_alignment = panorama_features::estimate_homography(current, previous);
            let feature_failure = feature_alignment.as_ref().err().cloned()
                .unwrap_or_else(|| "composition projective indisponible".to_owned());
            let feature_transform = feature_alignment.ok().and_then(|estimate| {
                imaging::multiply_homographies(
                    &transforms_source_to_panorama[index - 1], &estimate.source_to_target,
                ).map(|transform| (transform, estimate))
            });
            let (transform, confidence, detail) = if let Some((transform, estimate)) = feature_transform {
                let confidence = estimate.inliers as f64 / estimate.matches.max(1) as f64;
                (transform, Some(confidence), format!(
                    "Caractéristiques · {} / {} inliers · RMS {:.2} px",
                    estimate.inliers, estimate.matches, estimate.rms_error_px,
                ))
            } else {
                let estimated = imaging::estimate_translation(previous, current);
                let relative = estimated.map(|value| (value.0, value.1)).unwrap_or_else(|| (
                    (previous.width() as i64 * 3 / 4).clamp(0, i32::MAX as i64) as i32,
                    0,
                ));
                let local_transform = [
                    1.0, 0.0, relative.0 as f64,
                    0.0, 1.0, relative.1 as f64,
                    0.0, 0.0, 1.0,
                ];
                let Some(transform) = imaging::multiply_homographies(
                    &transforms_source_to_panorama[index - 1], &local_transform,
                ) else {
                    self.message = "Composition des alignements du panorama impossible.".into();
                    return;
                };
                let detail = format!("Translation · {feature_failure}");
                (transform, estimated.map(|value| value.2), detail)
            };
            let Some((origin_x, origin_y)) = imaging::project_homography(&transform, (0.0, 0.0)) else {
                self.message = "L'origine projetée d'une image du panorama est invalide.".into();
                return;
            };
            if origin_x.abs() > i32::MAX as f64 || origin_y.abs() > i32::MAX as f64 {
                self.message = "L'origine projetée dépasse les limites ajustables du panorama.".into();
                return;
            }
            offsets.push((origin_x.round() as i32, origin_y.round() as i32));
            transforms_source_to_panorama.push(transform);
            alignment_confidences.push(confidence);
            alignment_details.push(detail);
        }
        self.panorama_wizard = Some(PanoramaWizard {
            images, paths, transforms_source_to_panorama, offsets,
            alignment_confidences, alignment_details,
        });
        self.show_panorama_wizard = true;
    }

    fn panorama_window(&mut self, ctx: &egui::Context) {
        let Some(wizard) = self.panorama_wizard.as_mut() else { return; };
        let mut create = false;
        let mut cancel = false;
        egui::Window::new("Panorama assisté")
            .open(&mut self.show_panorama_wizard)
            .default_width(660.0)
            .show(ctx, |ui| {
                ui.label("Les images originales restent intactes. Les coordonnées de chaque image sont relatives à l’origine de l’image 1.");
                egui::ScrollArea::vertical().max_height(390.0).show(ui, |ui| {
                    for index in 0..wizard.images.len() {
                        let image = &wizard.images[index];
                        ui.horizontal(|ui| {
                            ui.label(format!("Image {} · {} × {} · {}", index + 1, image.width(), image.height(),
                                wizard.paths[index].file_name().and_then(|name| name.to_str()).unwrap_or("image")));
                            if index == 0 {
                                ui.weak("origine 0, 0");
                            } else {
                                ui.add(egui::DragValue::new(&mut wizard.offsets[index].0).prefix("X ").speed(1));
                                ui.add(egui::DragValue::new(&mut wizard.offsets[index].1).prefix("Y ").speed(1));
                                if let Some(confidence) = wizard.alignment_confidences[index] {
                                    ui.weak(format!("support {:.0} %", confidence * 100.0));
                                }
                            }
                        });
                        ui.weak(wizard.alignment_details[index].as_str());
                    }
                });
                ui.label("Les caractéristiques estiment rotation, échelle et perspective. Vérifiez chaque alignement; X/Y déplace l’origine projetée pour corriger un décalage résiduel. Le recouvrement est fondu progressivement.");
                ui.horizontal(|ui| {
                    if ui.button("Créer le panorama…").clicked() { create = true; }
                    if ui.button("Annuler").clicked() { cancel = true; }
                });
            });
        if create {
            self.show_panorama_wizard = false;
            if let Some(wizard) = self.panorama_wizard.take() { self.finish_panorama_mosaic(ctx, wizard); }
        } else if cancel || !self.show_panorama_wizard {
            self.panorama_wizard = None;
            self.show_panorama_wizard = false;
        }
    }

    fn camera_controls_window(&mut self, ctx: &egui::Context) {
        if !self.show_camera_controls { return; }
        let mut commands = Vec::new();
        let updates_allowed = self.running && self.capture_timer.is_none()
            && self.timelapse_capture.is_none() && self.video_recording.is_none();
        egui::Window::new("Réglages exposés par la caméra")
            .open(&mut self.show_camera_controls)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.label("Seuls les contrôles fournis par le pilote apparaissent. Le matériel peut ignorer ou refuser une valeur.");
                if !updates_allowed {
                    ui.weak("Les commandes sont verrouillées pendant une capture, une vidéo ou un time-lapse.");
                }
                if self.camera_controls.is_empty() {
                    ui.label("Aucun réglage n’est exposé par ce pilote.");
                }
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for setting in &mut self.camera_controls {
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            match &mut setting.value {
                                camera::CameraControlValue::Integer { value, range, step } => {
                                    ui.label(&setting.name);
                                    let response = if let Some((minimum, maximum)) = *range {
                                        ui.add_enabled(setting.writable && updates_allowed,
                                            egui::Slider::new(value, minimum..=maximum).step_by(*step as f64))
                                    } else {
                                        ui.add_enabled(setting.writable && updates_allowed,
                                            egui::DragValue::new(value).speed(*step as f64))
                                    };
                                    changed = response.changed();
                                }
                                camera::CameraControlValue::Float { value, range, step } => {
                                    ui.label(&setting.name);
                                    let response = if let Some((minimum, maximum)) = *range {
                                        ui.add_enabled(setting.writable && updates_allowed,
                                            egui::Slider::new(value, minimum..=maximum).step_by(*step))
                                    } else {
                                        ui.add_enabled(setting.writable && updates_allowed,
                                            egui::DragValue::new(value).speed(*step))
                                    };
                                    changed = response.changed();
                                }
                                camera::CameraControlValue::Boolean { value } => {
                                    changed = ui.add_enabled(setting.writable && updates_allowed,
                                        egui::Checkbox::new(value, &setting.name)).changed();
                                }
                                camera::CameraControlValue::Enum { value, options } => {
                                    ui.label(&setting.name);
                                    let previous = *value;
                                    egui::ComboBox::from_id_salt(format!("camera-control-{:?}", setting.id))
                                        .selected_text(value.to_string())
                                        .show_ui(ui, |ui| {
                                            for option in options.iter() {
                                                ui.add_enabled_ui(setting.writable && updates_allowed, |ui| {
                                                    ui.selectable_value(value, *option, option.to_string());
                                                });
                                            }
                                        });
                                    changed = *value != previous;
                                }
                                camera::CameraControlValue::Text { value } => {
                                    ui.label(&setting.name);
                                    changed = ui.add_enabled(setting.writable && updates_allowed,
                                        egui::TextEdit::singleline(value).desired_width(170.0)).changed();
                                }
                                camera::CameraControlValue::ReadOnly(display) => {
                                    ui.label(format!("{} · {}", setting.name, display));
                                }
                            }
                        });
                        if changed {
                            let command = match &setting.value {
                                camera::CameraControlValue::Integer { value, .. } =>
                                    Some(nokhwa::utils::ControlValueSetter::Integer(*value)),
                                camera::CameraControlValue::Float { value, .. } =>
                                    Some(nokhwa::utils::ControlValueSetter::Float(*value)),
                                camera::CameraControlValue::Boolean { value } =>
                                    Some(nokhwa::utils::ControlValueSetter::Boolean(*value)),
                                camera::CameraControlValue::Enum { value, .. } =>
                                    Some(nokhwa::utils::ControlValueSetter::EnumValue(*value)),
                                camera::CameraControlValue::Text { value } =>
                                    Some(nokhwa::utils::ControlValueSetter::String(value.clone())),
                                camera::CameraControlValue::ReadOnly(_) => None,
                            };
                            if let Some(command) = command { commands.push((setting.id, command)); }
                        }
                        ui.label(egui::RichText::new(format!("{} · {} · {}",
                            if setting.active { "actif" } else { "inactif" },
                            if setting.writable { "modifiable" } else { "lecture seule" },
                            setting.flags)).small().weak());
                        ui.separator();
                    }
                });
            });
        if !commands.is_empty() {
            let mut errors = Vec::new();
            if let Some(task) = self.camera_task.as_ref() {
                for (id, value) in commands {
                    if let Err(error) = task.set_control(id, value) { errors.push(error); }
                }
            } else {
                errors.push("Le flux caméra n'est plus actif.".into());
            }
            if !errors.is_empty() { self.message = errors.join(" · "); }
        }
    }

    fn finish_panorama_mosaic(&mut self, ctx: &egui::Context, wizard: PanoramaWizard) {
        let mut transforms = wizard.transforms_source_to_panorama.clone();
        for (index, transform) in transforms.iter_mut().enumerate() {
            let Some((projected_x, projected_y)) = imaging::project_homography(transform, (0.0, 0.0)) else {
                self.message = format!("L'origine projetée de l'image {} est invalide.", index + 1);
                return;
            };
            let desired = wizard.offsets[index];
            let adjustment = [
                1.0, 0.0, desired.0 as f64 - projected_x,
                0.0, 1.0, desired.1 as f64 - projected_y,
                0.0, 0.0, 1.0,
            ];
            let Some(adjusted) = imaging::multiply_homographies(&adjustment, transform) else {
                self.message = format!("Correction de l'origine de l'image {} impossible.", index + 1);
                return;
            };
            *transform = adjusted;
        }
        let (image, canvas_origin) = match imaging::panorama_mosaic_projective(&wizard.images, &transforms) {
            Ok(result) => result,
            Err(error) => { self.message = error; return; }
        };
        let Some(destination) = rfd::FileDialog::new().set_file_name("panorama_mosaique.png")
            .add_filter("PNG", &["png"]).save_file() else { return; };
        let manifest = serde_json::json!({
            "format_version": 1,
            "processing": "feature_homography_or_translation_mosaic",
            "source_images": wizard.paths.iter().map(|path| path.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "source_to_panorama_homographies_px": transforms,
            "canvas_origin_in_first_image_coordinates_px": canvas_origin,
            "projected_image_origins_in_first_image_coordinates_px": wizard.offsets,
            "alignment_confidence": wizard.alignment_confidences,
            "alignment_details": wizard.alignment_details,
            "blend": "bilinear sampling with source-edge weighted overlap blending",
            "calibration_carried_over": false
        });
        self.save_derived_image(ctx, destination, image, manifest);
    }

    fn start_rectification(&mut self, ctx: &egui::Context) {
        if let Err(error) = self.image_derivation_ready() { self.message = error; return; }
        let Some((width, height, rgb, source_name)) = self.image.as_ref().map(|image| (
            image.width, image.height, image.rgb.clone(), image.source.clone(),
        )) else {
            self.message = "Ouvrez ou figez une image avant sa rectification.".into();
            return;
        };
        let Some(source) = image::RgbImage::from_raw(width, height, rgb) else {
            self.message = "Image source invalide pour la rectification.".into();
            return;
        };
        let preview_image = egui::ColorImage::from_rgb([width as usize, height as usize], source.as_raw());
        let preview = ctx.load_texture("rectification-source", preview_image, egui::TextureOptions::LINEAR);
        self.rectification_wizard = Some(RectificationWizard {
            source, source_name, preview, corners: Vec::with_capacity(4),
        });
        self.show_rectification_wizard = true;
    }

    fn rectification_window(&mut self, ctx: &egui::Context) {
        let Some(wizard) = self.rectification_wizard.as_mut() else { return; };
        let mut create = false;
        let mut cancel = false;
        let mut undo = false;
        egui::Window::new("Rectification plane par homographie")
            .open(&mut self.show_rectification_wizard)
            .default_width(740.0)
            .show(ctx, |ui| {
                ui.label("Cliquez les quatre coins d’une surface plane dans l’ordre autour du contour. Cette correction traite la perspective; elle ne corrige pas la distorsion optique radiale.");
                ui.label(format!("Source · {} × {} · {}", wizard.source.width(), wizard.source.height(), wizard.source_name));
                let available = ui.available_size();
                let scale = (available.x / wizard.source.width() as f32)
                    .min((available.y - 94.0).max(120.0) / wizard.source.height() as f32)
                    .min(1.0).max(0.05);
                let display_size = egui::vec2(wizard.source.width() as f32 * scale,
                    wizard.source.height() as f32 * scale);
                let (image_rect, response) = ui.allocate_exact_size(display_size, Sense::click());
                let painter = ui.painter_at(image_rect);
                painter.image(wizard.preview.id(), image_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                let positions = wizard.corners.iter().map(|(x, y)| Pos2::new(
                    image_rect.left() + *x as f32 * scale,
                    image_rect.top() + *y as f32 * scale,
                )).collect::<Vec<_>>();
                for index in 0..positions.len() {
                    if index > 0 {
                        painter.line_segment([positions[index-1], positions[index]],
                            Stroke::new(2.0, Color32::YELLOW));
                    }
                    painter.circle_filled(positions[index], 5.0, Color32::from_rgb(234, 88, 46));
                    painter.text(positions[index] + egui::vec2(8.0, -8.0), Align2::LEFT_TOP,
                        format!("{}", index + 1), FontId::proportional(16.0), Color32::WHITE);
                }
                if positions.len() == 4 {
                    painter.line_segment([positions[3], positions[0]], Stroke::new(2.0, Color32::YELLOW));
                }
                if response.clicked() && wizard.corners.len() < 4 {
                    if let Some(position) = response.interact_pointer_pos() {
                        let x = ((position.x - image_rect.left()) / scale)
                            .clamp(0.0, wizard.source.width().saturating_sub(1) as f32) as f64;
                        let y = ((position.y - image_rect.top()) / scale)
                            .clamp(0.0, wizard.source.height().saturating_sub(1) as f32) as f64;
                        wizard.corners.push((x, y));
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(format!("{} / 4 points", wizard.corners.len()));
                    if ui.add_enabled(!wizard.corners.is_empty(), egui::Button::new("Retirer le dernier point"))
                        .clicked() { undo = true; }
                    if ui.add_enabled(wizard.corners.len() == 4, egui::Button::new("Rectifier…"))
                        .clicked() { create = true; }
                    if ui.button("Annuler").clicked() { cancel = true; }
                });
            });
        if undo {
            if let Some(wizard) = self.rectification_wizard.as_mut() { wizard.corners.pop(); }
        }
        if create {
            self.show_rectification_wizard = false;
            if let Some(wizard) = self.rectification_wizard.take() { self.finish_rectification(ctx, wizard); }
        } else if cancel || !self.show_rectification_wizard {
            self.rectification_wizard = None;
            self.show_rectification_wizard = false;
        }
    }

    fn finish_rectification(&mut self, ctx: &egui::Context, wizard: RectificationWizard) {
        let corners: [(f64, f64); 4] = match wizard.corners.clone().try_into() {
            Ok(corners) => corners,
            Err(_) => { self.message = "Il faut quatre coins pour rectifier l'image.".into(); return; }
        };
        let (rectified, homography) = match imaging::rectify_planar(&wizard.source, corners) {
            Ok(result) => result,
            Err(error) => { self.message = error; return; }
        };
        let source_stem = Path::new(&wizard.source_name).file_stem()
            .and_then(|stem| stem.to_str()).unwrap_or("image");
        let Some(destination) = rfd::FileDialog::new()
            .set_file_name(format!("{source_stem}_rectifiee.png"))
            .add_filter("PNG", &["png"]).save_file() else { return; };
        let manifest = serde_json::json!({
            "format_version": 1,
            "processing": "planar_projective_rectification",
            "source_image": wizard.source_name,
            "source_corners_px": corners,
            "destination_to_source_homography": homography,
            "radial_lens_distortion_corrected": false,
            "calibration_carried_over": false
        });
        self.save_derived_image(ctx, destination, rectified, manifest);
    }

    fn open_reference_document(&mut self) {
        if !self.pending.is_empty() || self.point_edit.is_some() || self.calibration_mode {
            self.message = "Terminez ou annulez l'action de mesure en cours avant d'importer un plan.".into();
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Plan de référence", &["pdf", "png", "jpg", "jpeg", "bmp", "tif", "tiff"])
            .pick_file() else { return; };
        match dper_drawing::analyze_path(&path) {
            Ok(plan) => {
                let count = plan.characteristics.len();
                // The assisted plan uses exactly the same spatial extraction as MCP.
                // Retain the existing manual comparison document when available.
                let manual_reference = reference::analyze_document(&path).ok();
                self.install_inspection_plan(plan);
                self.reference_document = manual_reference;
                self.message = format!("Plan de contrôle importé : {count} caractéristique(s) à vérifier sur le document source.");
            }
            Err(error) => self.message = format!("Import du document de référence impossible : {error}"),
        }
    }

    fn reference_document_panel(&mut self, ui: &mut egui::Ui) {
        self.registration_editor_panel(ui);
        ui.add_enabled_ui(!self.registration_photo_pick_owns_frame(), |ui| {
        self.automated_inspection_panel(ui);
        ui.separator();
        ui.label(egui::RichText::new("DOCUMENT DE RÉFÉRENCE").size(11.5).strong()
            .color(Color32::from_rgb(87, 106, 129)));
        if secondary_button(ui, "Importer un plan PDF / image…").clicked() {
            self.open_reference_document();
        }

        let measurements = &self.measurements;
        let mut changed = false;
        let mut remove_document = false;
        let mut add_dimension = false;
        if let Some(document) = self.reference_document.as_mut() {
            ui.label(egui::RichText::new(&document.source_name).strong()
                .color(Color32::from_rgb(31, 78, 139)));
            ui.label(egui::RichText::new(if document.extracted_text.is_empty() {
                "Texte OCR indisponible; ajoutez les cotes manuellement."
            } else {
                "Extraction locale automatique · vérifiez chaque cote détectée."
            }).small().color(Color32::from_rgb(105, 124, 146)));
            let quality_exploitable = self.quality_assessment.as_ref()
                .is_some_and(|assessment| assessment.is_exploitable);
            let protocol_consistent = match self.active_protocol.as_ref() {
                Some(protocol) => protocol_matches_session(
                    protocol, self.image.as_ref(), &self.active_device, &self.active_device_id, &self.optical_setup,
                    self.calibration.as_ref(), &self.quality_thresholds, self.display_correction,
                    &self.measurements, Some(document),
                ),
                None => self.measurements.iter().all(|measurement|
                    measurement.protocol_id.is_none() && measurement.protocol_version.is_none()),
            };
            let comparison_allowed = quality_exploitable && protocol_consistent;
            if comparison_allowed {
                let part_status = reference::part_status(document, measurements);
                let (status_color, status_label) = match part_status {
                    reference::PartStatus::InTolerance => (Color32::from_rgb(35, 122, 76), part_status.label()),
                    reference::PartStatus::OutOfTolerance => (Color32::from_rgb(178, 50, 45), part_status.label()),
                    reference::PartStatus::Incomplete => (Color32::from_rgb(155, 111, 16), part_status.label()),
                    reference::PartStatus::NoDimensions => (Color32::from_rgb(103, 120, 140), part_status.label()),
                };
                ui.colored_label(status_color, egui::RichText::new(format!("Pièce · {status_label}")).strong());
            } else {
                let quality_reason = self.quality_assessment.as_ref()
                    .map(|assessment| assessment.issues.iter().map(|issue| issue.label())
                        .collect::<Vec<_>>().join(", "))
                    .filter(|reason| !reason.is_empty())
                    .unwrap_or_else(|| "qualité non évaluée".into());
                let reason = [
                    (!quality_exploitable).then_some(quality_reason.as_str()),
                    (!protocol_consistent).then_some("mesures issues d'une autre version du protocole"),
                ].into_iter().flatten().collect::<Vec<_>>().join("; ");
                ui.colored_label(Color32::from_rgb(155, 111, 16),
                    egui::RichText::new(format!("Pièce · verdict suspendu : {reason}")).strong());
            }
            ui.horizontal(|ui| {
                if secondary_button(ui, "Ajouter une cote").clicked() { add_dimension = true; }
                if ui.small_button("Retirer le document").clicked() { remove_document = true; }
            });

            let mut remove_at = None;
            for (index, dimension) in document.dimensions.iter_mut().enumerate() {
                egui::Frame::new().fill(Color32::from_rgb(246, 249, 252))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(220, 229, 238)))
                    .corner_radius(7).inner_margin(7.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("Cote {}", index + 1)).strong());
                            changed |= ui.add(egui::TextEdit::singleline(&mut dimension.label)
                                .hint_text("Nom / repère").desired_width(125.0)).changed();
                            if ui.small_button("×").clicked() { remove_at = Some(index); }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Nominal");
                            changed |= ui.add(egui::DragValue::new(&mut dimension.nominal_mm)
                                .range(0.001..=1_000_000.0).speed(0.01).suffix(" mm")).changed();
                            ui.label("− tol.");
                            changed |= ui.add(egui::DragValue::new(&mut dimension.tolerance_minus_mm)
                                .range(0.0..=1_000_000.0).speed(0.01)).changed();
                            ui.label("+ tol.");
                            changed |= ui.add(egui::DragValue::new(&mut dimension.tolerance_plus_mm)
                                .range(0.0..=1_000_000.0).speed(0.01)).changed();
                            ui.label("mm");
                        });
                        let selected_measurement = dimension.measurement_id
                            .and_then(|id| measurements.iter().find(|measurement| measurement.id == id))
                            .map(|measurement| format!("M{} · {}", measurement.id, measurement.label))
                            .unwrap_or_else(|| "Associer à une mesure…".into());
                        egui::ComboBox::from_id_salt(("reference-measurement", dimension.id.as_str()))
                            .selected_text(selected_measurement)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                changed |= ui.selectable_value(&mut dimension.measurement_id, None, "Aucune mesure")
                                    .changed();
                                for measurement in measurements.iter()
                                    .filter(|measurement| reference::measurement_mm(measurement).is_some())
                                {
                                    let label = if measurement.label.trim().is_empty() {
                                        format!("M{} · {}", measurement.id, measurement.kind.label())
                                    } else {
                                        format!("M{} · {}", measurement.id, measurement.label)
                                    };
                                    changed |= ui.selectable_value(&mut dimension.measurement_id,
                                        Some(measurement.id), label).changed();
                                }
                            });
                        changed |= ui.checkbox(&mut dimension.confirmed, "Confirmer la cote extraite").changed();
                        if !dimension.source_text.is_empty() {
                            ui.label(egui::RichText::new(&dimension.source_text).small()
                                .color(Color32::from_rgb(105, 124, 146)));
                        }
                        if let Some(bbox) = dimension.ocr_bbox.as_ref() {
                            let confidence = dimension.ocr_confidence
                                .map(|value| format!("{value:.0}%"))
                                .unwrap_or_else(|| "non fournie".into());
                            ui.label(egui::RichText::new(format!(
                                "Zone OCR · page {} · confiance {} · x {}, y {}, {} × {} px",
                                dimension.ocr_page.unwrap_or(1), confidence,
                                bbox.left, bbox.top, bbox.width, bbox.height,
                            )).small().color(Color32::from_rgb(87, 106, 129)));
                        } else if let Some(page) = dimension.ocr_page {
                            ui.label(egui::RichText::new(format!(
                                "Texte de référence · page {page} · boîte OCR non applicable",
                            )).small().color(Color32::from_rgb(87, 106, 129)));
                        }
                        let comparison = reference::compare_dimension(dimension, measurements);
                        let color = if !comparison_allowed {
                            Color32::from_rgb(155, 111, 16)
                        } else {
                            match comparison.status {
                                reference::ComparisonStatus::InTolerance => Color32::from_rgb(35, 122, 76),
                                reference::ComparisonStatus::OutOfTolerance => Color32::from_rgb(178, 50, 45),
                                reference::ComparisonStatus::ToConfirm => Color32::from_rgb(155, 111, 16),
                                _ => Color32::from_rgb(103, 120, 140),
                            }
                        };
                        let result = comparison.measured_mm.map(|value| {
                            comparison.expanded_uncertainty_mm
                                .map(|uncertainty| format!("{value:.3} ± {uncertainty:.3} mm · "))
                                .unwrap_or_else(|| format!("{value:.3} mm · U inconnue · "))
                        }).unwrap_or_default();
                        let status = if comparison_allowed {
                            comparison.status.label().to_owned()
                        } else if !protocol_consistent {
                            "Verdict suspendu · protocole incompatible".to_owned()
                        } else {
                            "Verdict suspendu · qualité image non exploitable".to_owned()
                        };
                        ui.colored_label(color, format!("{result}{status}"));
                    });
            }
            if let Some(index) = remove_at {
                document.dimensions.remove(index);
                changed = true;
            }
            if add_dimension {
                document.dimensions.push(ReferenceDimension {
                    id: new_id("manual-dimension"), label: "Cote manuelle".into(),
                    nominal_mm: 1.0, tolerance_minus_mm: 0.0, tolerance_plus_mm: 0.0,
                    source_text: "Ajout manuel".into(), confirmed: false, measurement_id: None,
                    ocr_page: None, ocr_bbox: None, ocr_confidence: None,
                });
                changed = true;
            }
        }
        if remove_document {
            self.reference_document = None;
            changed = true;
        }
        if changed {
            self.session_saved = false;
            self.document_revision = self.document_revision.wrapping_add(1);
        }
        });
    }

    fn open_image_path(&mut self, ctx: &egui::Context, path: &Path) -> Result<(), String> {
        if self.capture_timer.is_some() || self.timelapse_capture.is_some() || self.video_recording.is_some() {
            return Err("Attendez ou arrêtez la capture différée/time-lapse avant d'ouvrir une autre image.".into());
        }
        if (self.image.is_some() && !self.session_saved) || !self.pending.is_empty() || self.mcp_mutation_blocked() {
            return Err("Enregistrez le contexte et terminez ou annulez l'action en cours avant d'ouvrir une autre image.".into());
        }
        let img = image::open(path).map_err(|error| format!("Erreur d'ouverture : {error}"))?.to_rgb8();
        self.running = false;
        self.camera_task = None;
        self.paused = true;
        self.image_is_imported = true;
        self.active_device = "Image importée".into();
        self.mm_per_pixel = None;
        self.calibration = None;
        self.calibration_history.clear();
        self.expanded_uncertainty_mm.clear();
        let (width, height) = img.dimensions();
        self.set_image(ctx, ImageData {
            width, height, rgb: img.into_raw(), source: path.to_string_lossy().into_owned(),
        }, true);
        if let Some(document) = self.reference_document.as_mut() {
            for dimension in &mut document.dimensions { dimension.measurement_id = None; }
        }
        self.session_saved = self.reference_document.is_none();
        self.selection_mode = false;
        self.calibration_mode = false;
        self.message = format!("Ouvert : {}", path.display());
        Ok(())
    }

    fn capture_photo(&mut self) {
        if self.video_recording.is_some() {
            self.message = "Arrêtez l'enregistrement MP4 avant de figer le flux pour une photo.".into();
            return;
        }
        if self.timelapse_capture.is_some() {
            self.message = "Arrêtez le time-lapse avant une capture photo ponctuelle.".into();
            return;
        }
        if self.capture_delay_seconds > 0 {
            if !self.running || self.paused {
                self.message = "La capture différée nécessite un flux caméra en direct.".into();
                return;
            }
            if self.capture_timer.is_some() {
                self.message = "Une capture différée est déjà en cours.".into();
                return;
            }
            if let Err(error) = fs::create_dir_all(&self.directory) {
                self.message = format!("Dossier inaccessible : {error}");
                return;
            }
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let unique_stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            let destination = self.directory.join(format!("dper_measure_{timestamp}_{unique_stamp}.png"));
            self.capture_timer = Some(CaptureTimer {
                destination, capture_at: Instant::now() + Duration::from_secs(self.capture_delay_seconds as u64),
                best_frame: None, best_sharpness: f64::NEG_INFINITY, frames_seen: 0,
            });
            self.message = format!(
                "Capture dans {} s · sélection automatique de la trame au piqué le plus élevé.",
                self.capture_delay_seconds,
            );
            return;
        }
        if let Err(e) = fs::create_dir_all(&self.directory) {
            self.message = format!("Dossier inaccessible : {e}"); return;
        }
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_secs();
        let unique_stamp = SystemTime::now().duration_since(UNIX_EPOCH)
            .unwrap_or_default().as_nanos();
        let path = self.directory.join(format!("dper_measure_{timestamp}_{unique_stamp}.png"));
        match self.capture_photo_to_path(&path, false) {
            Ok(path) => self.message = format!("Photo PNG sauvegardée et image figée : {path}"),
            Err(error) => self.message = error,
        }
    }

    fn capture_photo_to_path(&mut self, path: &Path, overwrite: bool) -> Result<String, String> {
        if self.video_recording.is_some() || self.timelapse_capture.is_some() || self.capture_timer.is_some() {
            return Err("Terminez l'enregistrement ou la capture programmée avant de figer une photo.".into());
        }
        let image = self.image.as_ref().ok_or_else(|| "Aucune image à enregistrer.".to_string())?;
        let mut path = path.to_path_buf();
        if path.extension().and_then(|ext| ext.to_str()).is_none_or(|ext| !ext.eq_ignore_ascii_case("png")) {
            path.set_extension("png");
        }
        if path.exists() && !overwrite {
            return Err(format!("Le fichier existe déjà : {}. Passez overwrite=true pour le remplacer.", path.display()));
        }
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|error| format!("Dossier de sortie inaccessible : {error}"))?;
        }
        image::save_buffer_with_format(&path, &image.rgb, image.width, image.height,
            image::ColorType::Rgb8, image::ImageFormat::Png)
            .map_err(|error| format!("Échec photo : {error}"))?;
        let was_running = self.running && !self.paused;
        self.paused = true;
        self.assess_current_quality();
        if was_running {
            self.capture_id = new_capture_id();
            self.document_revision = self.document_revision.wrapping_add(1);
        }
        self.message = format!("Photo PNG sauvegardée et image figée : {}", path.display());
        if let Some(data) = &mut self.image { data.source = path.to_string_lossy().into_owned(); }
        self.document_revision = self.document_revision.wrapping_add(1);
        Ok(path.to_string_lossy().into_owned())
    }

    fn export_current_data(&mut self, format: DataExportFormat) {
        if !self.pending.is_empty() || self.point_edit.is_some() || self.calibration_mode {
            self.message = "Terminez ou annulez l'action en cours avant l'export.".into();
            return;
        }
        if self.running && !self.paused { self.freeze_capture(); }
        if self.image.is_none() {
            self.message = "Ouvrez une image avant l'export des données.".into();
            return;
        }
        let (extension, label, filter_name, default_name) = match format {
            DataExportFormat::Csv => ("csv", "CSV", "Mesures CSV", "mesures.csv"),
            DataExportFormat::Xlsx => ("xlsx", "XLSX", "Mesures Excel", "mesures.xlsx"),
            DataExportFormat::SessionJson => ("json", "JSON session", "Session JSON", "session.json"),
            DataExportFormat::CalibrationJson => ("json", "Profils JSON", "Profils de calibration JSON", "calibrations.json"),
            DataExportFormat::AnnotatedPng => ("png", "PNG annoté", "Image PNG", "mesures-annotees.png"),
            DataExportFormat::Svg => ("svg", "SVG", "Image vectorielle SVG", "mesures.svg"),
            DataExportFormat::Dxf => ("dxf", "DXF", "Dessin DXF", "mesures.dxf"),
            DataExportFormat::OmeTiff => ("tif", "OME-TIFF", "Image OME-TIFF", "capture.ome.tif"),
        };
        let Some(mut path) = rfd::FileDialog::new()
            .set_file_name(default_name)
            .add_filter(filter_name, &[extension])
            .save_file() else { return; };
        if path.extension().and_then(|value| value.to_str())
            .is_none_or(|value| !value.eq_ignore_ascii_case(extension))
        {
            path.set_extension(extension);
        }

        let Some(snapshot) = self.current_export_image() else {
            self.message = "Aucune image active à exporter.".into();
            return;
        };
        let mut session = snapshot.session();
        let mut session_source_sidecar = None;
        if matches!(format, DataExportFormat::SessionJson) {
            let source_name = format!("session-source-{}.png", new_id("export"));
            let source_path = path.with_file_name(&source_name);
            if let Err(error) = image::save_buffer_with_format(
                &source_path, &snapshot.rgb, snapshot.width, snapshot.height,
                image::ColorType::Rgb8, image::ImageFormat::Png,
            ) {
                self.message = format!("Image source de la session non enregistrée : {error}");
                return;
            }
            session.image_file = source_name;
            session_source_sidecar = Some(source_path);
        }
        let result = match format {
            DataExportFormat::Csv => exports::measurements_csv(&session),
            DataExportFormat::Xlsx => exports::measurements_xlsx_for_image(
                &session, &snapshot.rgb, snapshot.width, snapshot.height,
            ),
            DataExportFormat::SessionJson => serde_json::to_vec_pretty(&session)
                .map_err(|error| format!("Sérialisation JSON impossible : {error}")),
            DataExportFormat::CalibrationJson => {
                let mut profiles = session.calibrations.clone();
                if let Some(active) = session.calibration.as_ref()
                    .filter(|active| !profiles.iter().any(|saved| saved.id == active.id))
                {
                    profiles.push(active.clone());
                }
                serde_json::to_vec_pretty(&profiles)
                    .map_err(|error| format!("Sérialisation des profils impossible : {error}"))
            }
            DataExportFormat::AnnotatedPng => exports::annotated_png(
                &snapshot.rgb, snapshot.width, snapshot.height, &session,
            ),
            DataExportFormat::Svg => exports::measurement_svg(
                &session, &snapshot.rgb, snapshot.width, snapshot.height,
            ),
            DataExportFormat::Dxf => exports::measurement_dxf(&session, snapshot.height),
            DataExportFormat::OmeTiff => exports::ome_tiff(
                &snapshot.rgb, snapshot.width, snapshot.height, &session,
            ),
        };
        let write_result = result.and_then(|bytes| fs::write(&path, bytes)
            .map_err(|error| format!("Écriture de {} impossible : {error}", path.display())));
        if write_result.is_err() {
            if let Some(sidecar) = session_source_sidecar.as_ref() { let _ = fs::remove_file(sidecar); }
        }
        match write_result {
            Ok(()) => self.message = format!("Export {label} enregistré : {}", path.display()),
            Err(error) => self.message = format!("Export {label} impossible : {error}"),
        }
    }

    fn compare_with_session(&mut self) {
        if !self.pending.is_empty() || self.point_edit.is_some() || self.calibration_mode {
            self.message = "Terminez ou annulez l'action en cours avant une comparaison A/B.".into();
            return;
        }
        if self.running && !self.paused { self.freeze_capture(); }
        let Some(snapshot) = self.current_export_image() else {
            self.message = "Figez ou ouvrez une image avant la comparaison A/B.".into();
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Session ou projet DPER_MEASURE", &["dperproj", "json"])
            .pick_file() else { return; };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => { self.message = format!("Lecture de la session B impossible : {error}"); return; }
        };
        let mut session_b: Session = match serde_json::from_slice(&bytes) {
            Ok(session) => session,
            Err(error) => { self.message = format!("Session B illisible : {error}"); return; }
        };
        if session_b.schema_version > storage::CURRENT_SCHEMA_VERSION {
            self.message = format!("Version de session B non prise en charge : {}", session_b.schema_version);
            return;
        }
        let mut session_a = snapshot.session();
        let assessment_a = quality::assess_image_quality_with_correction(
            &snapshot.rgb, snapshot.width, snapshot.height,
            &session_a.quality_thresholds, session_a.display_correction,
        ).ok();
        session_a.quality_assessment = assessment_a;
        let stored_image = PathBuf::from(&session_b.image_file);
        let source_b = if stored_image.is_absolute() {
            stored_image
        } else {
            path.parent().unwrap_or_else(|| Path::new(".")).join(stored_image)
        };
        let source_image_b = image::open(&source_b);
        let source_b_available = source_image_b.is_ok();
        let assessment_b = source_image_b.ok().and_then(|image| {
            let image = image.to_rgb8();
            let (width, height) = image.dimensions();
            quality::assess_image_quality_with_correction(
                image.as_raw(), width, height,
                &session_b.quality_thresholds, session_b.display_correction,
            ).ok()
        });
        session_b.quality_assessment = assessment_b;
        let name_a = self.image.as_ref().and_then(|image| Path::new(&image.source).file_name())
            .and_then(|name| name.to_str()).unwrap_or("Session A").to_owned();
        let name_b = path.file_name().and_then(|name| name.to_str()).unwrap_or("Session B").to_owned();
        let mut result = comparison::compare_sessions(&session_a, &session_b, name_a, name_b);
        if !source_b_available {
            result.comparable = false;
            result.reasons.push("Image source B absente ou illisible; qualité non vérifiée".into());
        }
        self.ab_comparison = Some(result);
        self.show_ab_comparison = true;
        self.message = "Comparaison A/B prête; consultez les réserves de comparabilité avant d'interpréter les écarts.".into();
    }

    fn export_ab_comparison(&mut self) {
        let Some(comparison) = self.ab_comparison.as_ref() else { return; };
        let bytes = match comparison::to_csv(comparison) {
            Ok(bytes) => bytes,
            Err(error) => { self.message = error; return; }
        };
        let Some(mut path) = rfd::FileDialog::new().set_file_name("comparaison-ab.csv")
            .add_filter("Comparaison A/B CSV", &["csv"]).save_file() else { return; };
        if path.extension().and_then(|extension| extension.to_str())
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("csv"))
        { path.set_extension("csv"); }
        match fs::write(&path, bytes) {
            Ok(()) => self.message = format!("Comparaison A/B exportée : {}", path.display()),
            Err(error) => self.message = format!("Export CSV A/B impossible : {error}"),
        }
    }

    fn comparison_window(&mut self, ctx: &egui::Context) {
        let Some(comparison) = self.ab_comparison.as_ref() else { return; };
        let mut export_csv = false;
        egui::Window::new("Comparaison A/B")
            .open(&mut self.show_ab_comparison)
            .default_size([900.0, 560.0])
            .show(ctx, |ui| {
                ui.label(format!("A · {}    |    B · {}", comparison.a_name, comparison.b_name));
                let (status, color) = if comparison.comparable {
                    ("Protocoles et conditions compatibles pour comparaison exploratoire",
                        Color32::from_rgb(35, 122, 76))
                } else {
                    ("Comparabilité non établie · écarts indicatifs seulement",
                        Color32::from_rgb(155, 111, 16))
                };
                ui.colored_label(color, egui::RichText::new(status).strong());
                for reason in &comparison.reasons {
                    ui.label(egui::RichText::new(format!("• {reason}")).small()
                        .color(Color32::from_rgb(155, 111, 16)));
                }
                ui.separator();
                egui::ScrollArea::vertical().max_height(390.0).show(ui, |ui| {
                    egui::Grid::new("ab-comparison-table").striped(true).show(ui, |ui| {
                        for header in ["Mesure", "A", "U A (mm)", "B", "U B (mm)", "Δ B−A", "État"] {
                            ui.strong(header);
                        }
                        ui.end_row();
                        for row in &comparison.rows {
                            ui.label(format!("{} · {}", row.kind, row.label));
                            let is_count = row.count_a.is_some() || row.count_b.is_some();
                            let count_label = |value: usize| {
                                format!("{value} objet{}", if value == 1 { "" } else { "s" })
                            };
                            ui.label(if is_count {
                                row.count_a.map(count_label).unwrap_or_else(|| "—".into())
                            } else {
                                row.value_a_mm.map(|value| format!("{value:.6} mm")).unwrap_or_else(|| "—".into())
                            });
                            ui.label(row.uncertainty_a_mm.map(|value| format!("{value:.6}")).unwrap_or_else(|| "—".into()));
                            ui.label(if is_count {
                                row.count_b.map(count_label).unwrap_or_else(|| "—".into())
                            } else {
                                row.value_b_mm.map(|value| format!("{value:.6} mm")).unwrap_or_else(|| "—".into())
                            });
                            ui.label(row.uncertainty_b_mm.map(|value| format!("{value:.6}")).unwrap_or_else(|| "—".into()));
                            ui.label(if is_count {
                                row.delta_count_b_minus_a.map(|value| format!("{value:+} objet{}",
                                    if value == 1 || value == -1 { "" } else { "s" })).unwrap_or_else(|| "—".into())
                            } else {
                                row.delta_b_minus_a_mm.map(|value| format!("{value:+.6} mm")).unwrap_or_else(|| "—".into())
                            });
                            ui.label(&row.status);
                            ui.end_row();
                        }
                    });
                });
                if ui.button("Exporter CSV…").clicked() { export_csv = true; }
            });
        if export_csv { self.export_ab_comparison(); }
    }

    fn apply_uncertainty_budget_draft(&mut self) {
        const COMPONENT_NAMES: [&str; 4] = [
            "Étalon / référence",
            "Répétabilité de calibration",
            "Pointage des extrémités",
            "Environnement et montage",
        ];
        let mut components = Vec::with_capacity(COMPONENT_NAMES.len());
        for (index, name) in COMPONENT_NAMES.iter().enumerate() {
            let Some(raw) = self.uncertainty_budget_components_mm.get(index) else {
                self.message = "Le budget doit contenir ses quatre composantes.".into();
                return;
            };
            let Some(value) = raw.trim().replace(',', ".").parse::<f64>().ok() else {
                self.message = format!("Saisissez l'incertitude-type en mm pour « {name} »; utilisez 0 si le terme est écarté.");
                return;
            };
            if !value.is_finite() || value < 0.0 {
                self.message = format!("L'incertitude-type de « {name} » doit être finie et positive ou nulle.");
                return;
            }
            components.push(uncertainty::UncertaintyBudgetComponent {
                name: (*name).to_owned(), standard_uncertainty_mm: value,
            });
        }
        let coverage_factor = match self.uncertainty_coverage_factor.trim().replace(',', ".").parse::<f64>() {
            Ok(value) if value.is_finite() && value > 0.0 => value,
            _ => {
                self.message = "Le facteur de couverture k doit être fini et strictement positif.".into();
                return;
            }
        };
        let budget = uncertainty::UncertaintyBudget {
            method: "root_sum_square_standard_uncertainties_v1".into(),
            coverage_factor,
            components,
            combination_assumption: "independent_components".into(),
        };
        match budget.expanded_uncertainty_mm() {
            Ok(expanded) => {
                self.expanded_uncertainty_mm = expanded.to_string();
                self.pending_uncertainty_budget = Some(budget);
                self.show_uncertainty_budget = false;
                self.message = format!(
                    "Budget saisi par l'opérateur : U = k·√Σuᵢ² = {:.9} mm. Les composantes n'ont pas été mesurées automatiquement.",
                    expanded,
                );
            }
            Err(error) => self.message = error,
        }
    }

    fn uncertainty_budget_window(&mut self, ctx: &egui::Context) {
        if !self.show_uncertainty_budget { return; }
        const COMPONENT_NAMES: [&str; 4] = [
            "Étalon / référence",
            "Répétabilité de calibration",
            "Pointage des extrémités",
            "Environnement et montage",
        ];
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Budget d'incertitude")
            .open(&mut self.show_uncertainty_budget)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.label("Saisir les incertitudes-types uᵢ en millimètres. Le calcul suppose des composantes indépendantes.");
                for (index, name) in COMPONENT_NAMES.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(*name);
                        if let Some(value) = self.uncertainty_budget_components_mm.get_mut(index) {
                            ui.add(egui::TextEdit::singleline(value).hint_text("mm · requis, 0 si écarté")
                                .desired_width(150.0));
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Facteur de couverture k");
                    ui.add(egui::TextEdit::singleline(&mut self.uncertainty_coverage_factor)
                        .desired_width(80.0));
                });
                ui.weak("U = k × √Σuᵢ². Les valeurs restent celles fournies par l'opérateur; le logiciel ne mesure pas la répétabilité, l'étalon ni l'environnement.");
                ui.horizontal(|ui| {
                    if ui.button("Calculer U").clicked() { apply = true; }
                    if ui.button("Annuler").clicked() { cancel = true; }
                });
            });
        if apply { self.apply_uncertainty_budget_draft(); }
        else if cancel || !self.show_uncertainty_budget { self.show_uncertainty_budget = false; }
    }

    fn apply_calibration(&mut self) {
        if self.running && !self.paused {
            self.message = "Figez l'image avant de l'étalonner.".into();
            return;
        }
        if self.calibration_type == CalibrationType::MultiPointXY {
            if self.pending.len() != 12 {
                self.message = "Il faut douze clics : références X/Y au centre, puis un segment de contrôle dans chacun des quatre coins.".into();
                return;
            }
            let parse = |value: &str| value.trim().replace(',', ".").parse::<f64>().ok();
            let (Some(reference_x_mm), Some(reference_y_mm), Some(check_mm)) = (
                parse(&self.reference_mm), parse(&self.reference_mm_y), parse(&self.verification_reference_mm),
            ) else {
                self.message = "Saisissez les longueurs X, Y et contrôle en millimètres.".into();
                return;
            };
            let points = self.pending.clone();
            match self.calibrate_xy_from_points(&points, reference_x_mm, reference_y_mm, check_mm, "interface", None) {
                Ok(profile) if profile.validation_status == "failed" => self.message = format!(
                    "Contrôle d'homogénéité échoué : résidus HG/HD/BG/BD [{}] mm, au moins un dépasse U. Profil conservé mais non activé.",
                    profile.corner_verification_residuals_mm.iter()
                        .map(|residual| format!("{residual:+.4}"))
                        .collect::<Vec<_>>().join(" / "),
                ),
                Ok(profile) => self.message = format!(
                    "Étalonnage X/Y {} · {:.8}/{:.8} mm/px · résidus coins [{}] mm",
                    profile.validation_status, profile.scale_x_mm_per_pixel.unwrap_or_default(),
                    profile.scale_y_mm_per_pixel.unwrap_or_default(),
                    profile.corner_verification_residuals_mm.iter()
                        .map(|residual| format!("{residual:+.4}"))
                        .collect::<Vec<_>>().join(" / "),
                ),
                Err(error) => self.message = error,
            }
            return;
        }
        if self.pending.len() != 2 { self.message = "Cliquer deux points de référence".into(); return; }
        let Ok(real_mm) = self.reference_mm.trim().replace(',', ".").parse::<f64>() else {
            self.message = "Entrer une longueur en millimètres (ex. 10)".into(); return;
        };
        let reference_type = self.calibration_type;
        let reference_points = self.pending.clone();
        match self.calibrate_from_points(reference_type, &reference_points, real_mm, "interface", None) {
            Ok(profile) => self.message = format!(
                "Étalonnage {} actif : {:.8} mm/pixel · référence {:.4} mm / {:.2} px",
                reference_type.label(), profile.mm_per_pixel, profile.reference_mm, profile.reference_px),
            Err(error) => self.message = error,
        }
    }

    fn calibration_uncertainty(
        &self,
        origin: &str,
        uncertainty_override: Option<f64>,
    ) -> Result<(Option<f64>, Option<uncertainty::UncertaintyBudget>), String> {
        if origin == "mcp" {
            if uncertainty_override.is_some_and(|value| !value.is_finite() || value <= 0.0) {
                return Err("L'incertitude élargie U doit être finie et strictement positive.".into());
            }
            return Ok((uncertainty_override, None));
        }
        if let Some(budget) = self.pending_uncertainty_budget.clone() {
            return budget.expanded_uncertainty_mm().map(|expanded| (Some(expanded), Some(budget)));
        }
        if self.expanded_uncertainty_mm.trim().is_empty() {
            return Ok((None, None));
        }
        let uncertainty = self.expanded_uncertainty_mm.trim().replace(',', ".")
            .parse::<f64>()
            .map_err(|_| "Entrer U en millimètres ou laisser vide si elle est inconnue".to_owned())?;
        if !uncertainty.is_finite() || uncertainty <= 0.0 {
            return Err("L'incertitude élargie U doit être strictement positive.".into());
        }
        Ok((Some(uncertainty), None))
    }

    fn calibrate_from_points(
        &mut self,
        reference_type: CalibrationType,
        points: &[Point],
        real_mm: f64,
        origin: &str,
        uncertainty_override: Option<f64>,
    ) -> Result<Calibration, String> {
        if self.running && !self.paused { return Err("Figez l'image avant de l'étalonner.".into()); }
        let image = self.image.as_ref().ok_or_else(|| "Aucune image à étalonner.".to_string())?;
        if points.len() != 2 { return Err("L'étalonnage requiert exactement deux points.".into()); }
        if points.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()
            || point.x < 0.0 || point.y < 0.0
            || point.x > image.width as f64 || point.y > image.height as f64)
        {
            return Err("Un point de référence sort des limites de l'image ou n'est pas fini.".into());
        }
        let reference_scale = reference_type.reference_scale();
        let (expanded_uncertainty_mm, uncertainty_budget) =
            self.calibration_uncertainty(origin, uncertainty_override)?;
        let scale = geometry::calibration_mm_per_px(points[0], points[1], real_mm / reference_scale)
            .map_err(str::to_owned)?;
        let reference_px = points[0].dist(points[1]) * reference_scale;
        let profile = Calibration {
            id: new_id("calibration"),
            device: self.active_device.clone(), resolution: [image.width, image.height],
            device_id: self.active_device_id.clone(),
            optical_setup: self.optical_setup.clone(),
            mm_per_pixel: scale, reference_mm: real_mm, reference_px,
            scale_x_mm_per_pixel: Some(scale), scale_y_mm_per_pixel: Some(scale),
            expanded_uncertainty_mm,
            uncertainty_budget,
            reference_x_mm: None, reference_y_mm: None, verification_reference_mm: None,
            verification_residual_mm: None, validation_status: "single_reference_unverified".into(),
            reference_type: match reference_type {
                CalibrationType::Segment => "segment",
                CalibrationType::CircleDiameter => "circle_diameter",
                CalibrationType::RectangleDiagonal => "rectangle_diagonal",
                CalibrationType::MultiPointXY => return Err("Utilisez la procédure multipoint pour l'étalonnage X/Y.".into()),
            }.into(),
            reference_points_native_px: [points[0], points[1]],
            secondary_reference_points_native_px: [Point::default(); 2],
            validation_points_native_px: [Point::default(); 2],
            corner_verification_residuals_mm: Vec::new(),
            additional_corner_validation_segments_native_px: Vec::new(),
            author: std::env::var("USERNAME").unwrap_or_default(),
            reference_image: image.source.clone(),
            origin: origin.to_owned(),
            created_at_unix_secs: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        };
        self.commit_calibration_profile(profile)
    }

    fn commit_calibration_profile(&mut self, profile: Calibration) -> Result<Calibration, String> {
        self.invalidate_inspection_inputs();
        if !self.image_is_imported {
            if profile.validation_status != "failed" {
                self.settings.calibrations.retain(|saved| !(saved.device == profile.device
                    && saved.resolution == profile.resolution && saved.optical_setup == profile.optical_setup
                    && saved.validation_status != "failed"));
            }
            self.settings.calibrations.push(profile.clone());
        }
        if profile.validation_status == "failed" {
            self.mm_per_pixel = None;
            self.expanded_uncertainty_mm.clear();
            self.calibration = None;
        } else {
            self.mm_per_pixel = Some(profile.mm_per_pixel);
            self.expanded_uncertainty_mm = profile.expanded_uncertainty_mm
                .map(|value| value.to_string()).unwrap_or_default();
            self.calibration = Some(profile.clone());
        }
        self.calibration_history.push(profile.clone());
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        self.calibration_mode = false;
        self.pending_uncertainty_budget = None;
        self.clear_pending();
        if !self.image_is_imported {
            if let Err(error) = storage::save_settings(&self.settings) {
                self.message = format!("Étalonnage actif, sauvegarde du profil impossible : {error}");
            }
        }
        Ok(profile)
    }

    fn calibrate_xy_from_points(
        &mut self, points: &[Point], reference_x_mm: f64, reference_y_mm: f64,
        verification_reference_mm: f64, origin: &str, uncertainty_override: Option<f64>,
    ) -> Result<Calibration, String> {
        if self.running && !self.paused { return Err("Figez l'image avant de l'étalonner.".into()); }
        let image = self.image.as_ref().ok_or_else(|| "Aucune image à étalonner.".to_string())?;
        if points.len() != 12 { return Err("L'étalonnage X/Y multipoint requiert douze clics : X/Y au centre et quatre segments de contrôle aux coins.".into()); }
        if points.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()
            || point.x < 0.0 || point.y < 0.0 || point.x > image.width as f64 || point.y > image.height as f64)
        { return Err("Un point de référence sort de l'image ou n'est pas fini.".into()); }
        let known = [reference_x_mm, reference_y_mm, verification_reference_mm];
        if known.iter().any(|value| !value.is_finite() || *value <= 0.0) {
            return Err("Les trois longueurs connues doivent être finies et strictement positives.".into());
        }
        let dx = (points[1].x - points[0].x).abs();
        let x_drift = (points[1].y - points[0].y).abs();
        let dy = (points[3].y - points[2].y).abs();
        let y_drift = (points[3].x - points[2].x).abs();
        if dx < 2.0 || dy < 2.0 || x_drift > dx * 0.10 || y_drift > dy * 0.10 {
            return Err("Placez les deux premières références au centre : X quasi horizontal, puis Y quasi vertical.".into());
        }
        let center_reference_midpoints = [
            Point { x: (points[0].x + points[1].x) * 0.5, y: (points[0].y + points[1].y) * 0.5 },
            Point { x: (points[2].x + points[3].x) * 0.5, y: (points[2].y + points[3].y) * 0.5 },
        ];
        if center_reference_midpoints.iter().any(|midpoint| {
            (midpoint.x - image.width as f64 * 0.5).abs() > image.width as f64 * 0.20
                || (midpoint.y - image.height as f64 * 0.5).abs() > image.height as f64 * 0.20
        }) {
            return Err("Les références X/Y doivent rester dans la zone centrale de l'image.".into());
        }
        let scale_x = reference_x_mm / dx;
        let scale_y = reference_y_mm / dy;
        let corner_segments = (0..4).map(|corner| [points[4 + corner * 2], points[5 + corner * 2]])
            .collect::<Vec<_>>();
        let corner_zones = [(false, false), (true, false), (false, true), (true, true)];
        for (segment, (right, bottom)) in corner_segments.iter().zip(corner_zones) {
            let center_x = (segment[0].x + segment[1].x) * 0.5;
            let center_y = (segment[0].y + segment[1].y) * 0.5;
            let edge_x = if right { image.width as f64 * 0.65 } else { image.width as f64 * 0.35 };
            let edge_y = if bottom { image.height as f64 * 0.65 } else { image.height as f64 * 0.35 };
            if segment[0].dist(segment[1]) < 2.0
                || (!right && center_x > edge_x) || (right && center_x < edge_x)
                || (!bottom && center_y > edge_y) || (bottom && center_y < edge_y)
            {
                return Err("Chaque contrôle doit être un segment distinct placé dans le coin indiqué, à au moins deux pixels des extrémités.".into());
            }
        }
        let residuals = corner_segments.iter().map(|segment| {
            let dx = segment[1].x - segment[0].x;
            let dy = segment[1].y - segment[0].y;
            (dx.powi(2) * scale_x.powi(2) + dy.powi(2) * scale_y.powi(2)).sqrt()
                - verification_reference_mm
        }).collect::<Vec<_>>();
        let residual = residuals.iter().copied()
            .max_by(|first, second| first.abs().total_cmp(&second.abs())).unwrap_or_default();
        let (expanded_uncertainty_mm, uncertainty_budget) =
            self.calibration_uncertainty(origin, uncertainty_override)?;
        let validation_status = match expanded_uncertainty_mm {
            Some(uncertainty) if residuals.iter().all(|residual| residual.abs() <= uncertainty) => "verified",
            Some(_) => "failed",
            None => "unverified",
        };
        let width = image.width;
        let height = image.height;
        let reference_image = image.source.clone();
        let effective_scale = (scale_x * scale_y).sqrt();
        let profile = Calibration {
            id: new_id("calibration"), device: self.active_device.clone(), resolution: [width, height],
            device_id: self.active_device_id.clone(),
            optical_setup: self.optical_setup.clone(), mm_per_pixel: effective_scale,
            scale_x_mm_per_pixel: Some(scale_x), scale_y_mm_per_pixel: Some(scale_y),
            expanded_uncertainty_mm, uncertainty_budget,
            reference_mm: (reference_x_mm * reference_y_mm).sqrt(),
            reference_px: (dx * dy).sqrt(), reference_x_mm: Some(reference_x_mm),
            reference_y_mm: Some(reference_y_mm), verification_reference_mm: Some(verification_reference_mm),
            verification_residual_mm: Some(residual), corner_verification_residuals_mm: residuals,
            validation_status: validation_status.into(),
            reference_type: "multipoint_xy_center_corner".into(),
            reference_points_native_px: [points[0], points[1]],
            secondary_reference_points_native_px: [points[2], points[3]],
            validation_points_native_px: [points[4], points[5]],
            additional_corner_validation_segments_native_px: corner_segments.into_iter().skip(1).collect(),
            author: std::env::var("USERNAME").unwrap_or_default(), reference_image,
            origin: origin.to_owned(),
            created_at_unix_secs: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        };
        self.commit_calibration_profile(profile)
    }

    fn on_click(&mut self, p: Point) {
        if self.video_recording.is_some() || self.timelapse_capture.is_some() {
            self.message = "Arrêtez l’acquisition avant de tracer ou d’étalonner.".into();
            return;
        }
        if self.handle_pattern_pick(p) { return; }
        if self.pattern_dialog.is_some() { return; }
        self.display_history_priority = false;
        if self.running && !self.paused { self.freeze_capture(); }
        if self.image.is_none() {
            self.message = "Aucune image à mesurer".into();
            return;
        }
        if self.calibration_mode {
            let required = if self.calibration_type == CalibrationType::MultiPointXY { 12 } else { 2 };
            if self.pending.len() >= required { self.clear_pending(); }
            self.pending.push(p);
            return;
        }
        if !self.construction_mode && !self.quality_assessment.as_ref().is_some_and(|assessment| assessment.is_exploitable) {
            self.message = self.quality_gate_message();
            return;
        }
        let snap_enabled = self.edge_snap_enabled && self.mode != Tool::ObjectCount;
        if self.pending.is_empty() {
            self.pending_calibration_id = self.calibration.as_ref().map(|profile| profile.id.clone());
            self.pending_mm_per_pixel = self.mm_per_pixel;
            self.pending_snap_enabled = snap_enabled;
            self.pending_snap_radius = snap_enabled.then_some(self.edge_snap_radius);
        }
        let (point, snapped) = if let Some(center) = self.construction_snap(p) {
            (center, true)
        } else if snap_enabled {
            self.image.as_ref().and_then(|image| quality::analyze_edge_subpixel(
                &image.rgb, image.width, image.height, p, self.edge_snap_radius,
                self.quality_thresholds.edge_gradient_threshold,
            )).map(|result| (result.point, true)).unwrap_or((p, false))
        } else { (p, false) };
        self.pending.push(point);
        self.pending_snap_applied.push(snapped);
        let completed = self.drawing_variant.required_points().is_some_and(|n| self.pending.len() >= n);
        if completed { self.finish_measurement(); }
    }

    fn finish_measurement(&mut self) {
        if !self.construction_mode && !self.quality_assessment.as_ref().is_some_and(|assessment| assessment.is_exploitable) {
            self.clear_pending();
            self.message = self.quality_gate_message();
            return;
        }
        let min = self.drawing_variant.required_points().unwrap_or_else(|| match self.mode {
            Tool::Polygon | Tool::CircleFit => 3,
            Tool::Polyline => 2,
            Tool::ObjectCount => 1,
            _ => self.mode.required_points().unwrap_or(2),
        });
        if self.pending.len() < min {
            self.message = format!("Il faut au moins {min} points"); return;
        }
        let (kind, resolved_points) = match self.drawing_variant.resolve(&self.pending, self.drawing_radius_px) {
            Ok(resolved) => resolved,
            Err(error) => { self.message = error; return; }
        };
        if self.image.as_ref().is_some_and(|image| geometry::exceeds_image_bounds(kind, &resolved_points, image.width, image.height)) {
            self.message = "La géométrie reconstruite dépasse l’image. Ajustez les points ou le rayon avant de terminer.".into();
            return;
        }
        let calibration_id = self.pending_calibration_id.take();
        let mm_per_pixel = self.pending_mm_per_pixel.take();
        self.pending.clear();
        let points = resolved_points;
        let snap_enabled = std::mem::replace(&mut self.pending_snap_enabled, false);
        let snap_radius = self.pending_snap_radius.take().unwrap_or(self.edge_snap_radius);
        let snap_results = std::mem::take(&mut self.pending_snap_applied);
        let snapped_points = snap_results.iter().filter(|snapped| **snapped).count();
        let method = (snap_enabled || snapped_points > 0).then(|| format!(
            "Accrochage bords/constructions · {snapped_points}/{} point(s) ajusté(s) · zone ±{snap_radius} px · seuil {:.1}",
            snap_results.len(),
            self.quality_thresholds.edge_gradient_threshold,
        ));
        self.add_measurement_with_style(
            kind,
            points,
            None,
            self.capture_id.clone(),
            calibration_id,
            mm_per_pixel,
            "interface".into(),
            method,
            self.construction_mode,
            Some(self.drawing_variant.label().into()),
        );
    }

    fn quality_gate_message(&self) -> String {
        let details = self.quality_assessment.as_ref()
            .map(|assessment| assessment.issues.iter().map(|issue| issue.label())
                .collect::<Vec<_>>().join(", "))
            .filter(|details| !details.is_empty())
            .unwrap_or_else(|| "qualité non évaluée".into());
        format!("Mesure bloquée : image non exploitable selon les seuils provisoires ({details}). Ajustez l'affichage ou réévaluez l'image.")
    }

    fn add_measurement(
        &mut self,
        kind: Tool,
        points: Vec<Point>,
        label: Option<String>,
        capture_id: String,
        calibration_id: Option<String>,
        mm_per_pixel: Option<f64>,
        origin: String,
        method: Option<String>,
    ) -> Measurement {
        self.add_measurement_with_style(kind, points, label, capture_id, calibration_id,
            mm_per_pixel, origin, method, false, None)
    }

    fn add_measurement_with_construction(
        &mut self,
        kind: Tool,
        points: Vec<Point>,
        label: Option<String>,
        capture_id: String,
        calibration_id: Option<String>,
        mm_per_pixel: Option<f64>,
        origin: String,
        method: Option<String>,
        is_construction: bool,
        source_measurement_ids: Vec<u64>,
    ) -> Measurement {
        let id = self.next_id;
        let expanded_uncertainty_mm = (origin != "vision").then_some(calibration_id.as_deref()).flatten().and_then(|calibration_id| {
            self.calibration_history.iter()
                .find(|profile| profile.id == calibration_id)
                .and_then(|profile| profile.expanded_uncertainty_mm)
        });
        let calibration_profile = calibration_id.as_deref().and_then(|calibration_id| self.calibration_history.iter()
            .find(|profile| profile.id == calibration_id));
        let mm_per_pixel_x = calibration_profile.and_then(|profile| profile.scale_x_mm_per_pixel).or(mm_per_pixel);
        let mm_per_pixel_y = calibration_profile.and_then(|profile| profile.scale_y_mm_per_pixel).or(mm_per_pixel);
        let protocol_id = self.active_protocol.as_ref().map(|protocol| protocol.id.clone());
        let protocol_version = self.active_protocol.as_ref().map(|protocol| protocol.version);
        let measurement = Measurement { drawing_variant: None,
            id,
            kind,
            is_construction,
            source_measurement_ids,
            dependency_stale: false,
            points,
            label: label.unwrap_or_else(|| format!("M{id}")),
            label_position_native_px: None,
            capture_id: capture_id.clone(),
            calibration_id,
            mm_per_pixel,
            mm_per_pixel_x,
            mm_per_pixel_y,
            expanded_uncertainty_mm,
            origin,
            method,
            protocol_id,
            protocol_version,
        };
        let exceeds_image = self.image.as_ref().is_some_and(|image| {
            geometry::exceeds_image_bounds(measurement.kind, &measurement.points, image.width, image.height)
        });
        self.next_id += 1;
        let previous_selection = self.selected_measurement;
        let index = self.measurements.len();
        self.measurements.push(measurement.clone());
        self.selected_measurement = Some(id);
        self.record_measurement_action(MeasurementHistoryAction::Added {
            measurement: measurement.clone(),
            index,
            previous_selection,
        });
        self.inspection.dependencies.mark_created(id);
        self.refresh_dependency_flags();
        if exceeds_image {
            self.message = "Attention : la géométrie dépasse le bord de l'image source.".into();
        } else {
            self.message = format!("Mesure {} ajoutée à la capture {}", measurement.label, capture_id);
        }
        measurement
    }

    fn mcp_context(&self) -> mcp_server::McpContext {
        let image = self.image.as_ref().map(|image| {
            let name = Path::new(&image.source).file_name()
                .and_then(|name| name.to_str()).unwrap_or("image active");
            (name, image.width, image.height)
        });
        mcp_server::build_context(
            image,
            &self.capture_id,
            self.document_revision,
            self.session_saved,
            self.running,
            self.paused,
            !self.undo_history.is_empty(),
            !self.redo_history.is_empty(),
            self.selected_measurement,
            &self.active_device,
            &self.active_device_id,
            &self.optical_setup,
            self.image.is_some() && (!self.running || self.paused),
            self.mm_per_pixel,
            self.calibration.clone(),
            &self.calibration_history,
            &self.settings.calibrations,
            &self.measurements,
            self.quality_assessment.clone(),
            self.active_protocol.clone(),
            &self.annotations,
        )
    }

    fn mcp_mutation_blocked(&self) -> bool {
        self.registration_photo_pick_owns_frame() || self.other_action_blocked()
    }

    fn other_action_blocked(&self) -> bool {
        self.calibration_mode || !self.pending.is_empty() || self.point_edit.is_some()
            || self.export_dialog.is_some()
    }

    fn handle_mcp_requests(&mut self, ctx: &egui::Context) {
        // Some camera/project/export paths have their own guards rather than
        // mcp_mutation_blocked. Reserve all mutations for the complete gesture.
        macro_rules! guard_photo_pick {
            ($reply:ident) => {
                if self.registration_photo_pick_owns_frame() {
                    let _ = $reply.send(Err("Terminez ou annulez le choix du repère photo avant cette action MCP.".into()));
                    continue;
                }
            };
        }
        while let Ok(request) = self.mcp_requests.try_recv() {
            self.invalidate_changed_drawing_dependencies();
            match request {
                mcp_server::UiRequest::Inspection { request, reply } => {
                    let result = self.handle_inspection_request(request);
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::GetContext { reply } => {
                    let _ = reply.send(Ok(self.mcp_context()));
                }
                mcp_server::UiRequest::FreezeImage { reply } => {
                    guard_photo_pick!(reply);
                    if self.mcp_mutation_blocked() {
                        let _ = reply.send(Err("Terminez ou annulez l'action en cours dans l'interface avant de laisser l'IA figer l'image.".into()));
                        continue;
                    }
                    self.freeze_capture();
                    let result = if let Some(image) = self.image.as_ref() {
                        Ok(mcp_server::ImagePayload {
                            context: self.mcp_context(),
                            x: 0,
                            y: 0,
                            width: image.width,
                            height: image.height,
                            rgb: image.rgb.clone(),
                        })
                    } else {
                        Err("Aucune image active. Ouvrez une image ou démarrez la caméra.".to_string())
                    };
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::GetCrop { capture_id, x, y, width, height, reply } => {
                    let result = (|| {
                        let image = self.image.as_ref().ok_or_else(|| "Aucune image active.".to_string())?;
                        if self.running && !self.paused {
                            return Err("Le flux est en direct. Appelez freeze_active_image avant de demander un recadrage.".into());
                        }
                        if capture_id != self.capture_id {
                            return Err("La capture a changé. Rappelez freeze_active_image et utilisez son nouvel identifiant.".into());
                        }
                        let rgb = mcp_server::crop_rgb(&image.rgb, image.width, image.height, x, y, width, height)?;
                        Ok(mcp_server::ImagePayload {
                            context: self.mcp_context(), x, y, width, height, rgb,
                        })
                    })();
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::AddMeasurements { capture_id, expected_revision, measurements, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.apply_mcp_measurements(&capture_id, expected_revision, measurements);
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::UpdateMeasurements { capture_id, expected_revision, updates, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.update_mcp_measurements(&capture_id, expected_revision, updates);
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::DeleteMeasurements { capture_id, expected_revision, ids, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.delete_mcp_measurements(&capture_id, expected_revision, &ids);
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::ClearMeasurements { capture_id, expected_revision, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.clear_mcp_measurements(&capture_id, expected_revision);
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::MeasurementHistory { action, expected_revision, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.apply_mcp_history(&action, expected_revision);
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::CancelActiveAction { reply } => {
                    self.cancel_mcp_active_action();
                    let _ = reply.send(Ok(self.mcp_context()));
                }
                mcp_server::UiRequest::SelectMeasurement { capture_id, id, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.select_mcp_measurement(&capture_id, id).map(|()| self.mcp_context());
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::SetCalibration {
                    capture_id, expected_revision, reference_type, points, known_mm,
                    expanded_uncertainty_mm, reply,
                } => {
                    guard_photo_pick!(reply);
                    let result = self.set_mcp_calibration(
                        &capture_id, expected_revision, &reference_type, &points, known_mm,
                        expanded_uncertainty_mm);
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::ListCameras { reply } => {
                    self.refresh_cameras();
                    let cameras = self.cameras.iter().enumerate().map(|(index, (_, name, device_id))|
                        mcp_server::CameraView { index, name: name.clone(), device_id: device_id.clone(), selected: index == self.selected_camera })
                        .collect();
                    let _ = reply.send(Ok(cameras));
                }
                mcp_server::UiRequest::StartCamera { index, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.start_camera_index(index).map(|()| self.mcp_context());
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::StreamControl { action, expected_revision, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.control_mcp_stream(&action, expected_revision).map(|()| self.mcp_context());
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::OpenImage { path, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.open_image_path(ctx, &path).map(|()| self.mcp_context());
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::OpenProject { path, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.open_project_path(ctx, &path).map(|()| self.mcp_context());
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::SaveProject { path, overwrite, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.save_project_to_path(&path, overwrite);
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::CapturePhoto { path, overwrite, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.capture_photo_to_path(&path, overwrite);
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::SetWorkspace {
                    optical_setup, measurement_tool, show_grid, show_center_guides, zoom_percent, reply,
                } => {
                    guard_photo_pick!(reply);
                    let result = self.set_mcp_workspace(
                        optical_setup, measurement_tool.as_deref(), show_grid, show_center_guides, zoom_percent)
                        .map(|()| self.mcp_context());
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::PrepareReport { folder, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.prepare_mcp_report(ctx, folder.as_deref());
                    let _ = reply.send(result);
                }
                mcp_server::UiRequest::ExportReport { args, reply } => {
                    guard_photo_pick!(reply);
                    let result = self.export_mcp_report(ctx, args);
                    let _ = reply.send(result);
                }
            }
        }
    }

    fn apply_mcp_measurements(
        &mut self,
        capture_id: &str,
        expected_revision: Option<u64>,
        inputs: Vec<mcp_server::MeasurementInput>,
    ) -> Result<Vec<mcp_server::AddedMeasurement>, String> {
        let image = self.image.as_ref().ok_or_else(|| "Aucune image active.".to_string())?;
        if self.running && !self.paused {
            return Err("Figez l'image avec freeze_active_image avant d'ajouter des mesures.".into());
        }
        if capture_id != self.capture_id {
            return Err("L'identifiant ne correspond plus à la capture active. Reprenez l'image et réessayez.".into());
        }
        if expected_revision.is_some_and(|revision| revision != self.document_revision) {
            return Err(format!("Le document a changé (révision actuelle {}). Relisez le contexte avant d'ajouter des mesures.", self.document_revision));
        }
        if self.mcp_mutation_blocked() {
            return Err("Terminez ou annulez l'action en cours dans l'interface avant d'ajouter des mesures IA.".into());
        }
        if !self.quality_assessment.as_ref().is_some_and(|assessment| assessment.is_exploitable) {
            return Err(self.quality_gate_message());
        }
        if inputs.is_empty() { return Err("Fournissez au moins une mesure.".into()); }

        let width = image.width as f64;
        let height = image.height as f64;
        let current_capture = self.capture_id.clone();
        let calibration_id = self.calibration.as_ref().map(|profile| profile.id.clone());
        let mm_per_pixel = self.mm_per_pixel.filter(|scale| scale.is_finite() && *scale > 0.0);
        let mut prepared = Vec::with_capacity(inputs.len());
        for input in inputs {
            let kind = match input.kind.trim().to_ascii_lowercase().as_str() {
                "distance" => Tool::Distance,
                "polyline" => Tool::Polyline,
                "angle" => Tool::Angle,
                "circle" => Tool::Circle,
                "rectangle" => Tool::Rectangle,
                "polygon" => Tool::Polygon,
                "point_line_distance" | "point-line-distance" => Tool::PointLineDistance,
                "parallel_distance" | "parallel-distance" => Tool::ParallelDistance,
                "circle_fit" | "circle-fit" => Tool::CircleFit,
                "ellipse" => Tool::Ellipse,
                "arc" => Tool::Arc,
                "coordinate" => Tool::Coordinate,
                "object_count" | "count_objects" | "object-count" => Tool::ObjectCount,
                _ => return Err(format!("Type de mesure inconnu : {}.", input.kind)),
            };
            if input.points.is_empty() { return Err("Chaque mesure doit contenir au moins un point.".into()); }
            let expected_points = kind.required_points();
            if let Some(expected) = expected_points {
                if input.points.len() != expected {
                    return Err(format!("{} requiert exactement {expected} points.", kind.label()));
                }
            } else {
                let minimum = match kind {
                    Tool::Polygon | Tool::CircleFit | Tool::Ellipse | Tool::Arc | Tool::PointLineDistance => 3,
                    Tool::ParallelDistance => 4,
                    Tool::ObjectCount => 1,
                    _ => 2,
                };
                if input.points.len() < minimum {
                    return Err(format!("{} requiert au moins {minimum} points.", kind.label()));
                }
            }
            let points: Vec<Point> = input.points.iter().map(Point::from).collect();
            if points.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()
                || point.x < 0.0 || point.y < 0.0 || point.x > width || point.y > height)
            {
                return Err("Un point est non fini ou sort des limites de l'image source.".into());
            }
            if !geometry::is_nondegenerate(kind, &points) {
                return Err(format!("La géométrie {} est dégénérée ou incomplète.", kind.label()));
            }
            let label = input.label.map(|label| label.trim().to_string()).filter(|label| !label.is_empty());
            self.validate_dependency_sources(capture_id, &input.source_measurement_ids)?;
            prepared.push((kind, points, label, input.method, input.is_construction, input.source_measurement_ids));
        }

        let mut added = Vec::with_capacity(prepared.len());
        for (kind, points, label, method, is_construction, sources) in prepared {
            let measurement = self.add_measurement_with_construction(
                kind,
                points,
                label,
                current_capture.clone(),
                calibration_id.clone(),
                mm_per_pixel,
                "mcp".into(),
                method,
                is_construction,
                sources,
            );
            added.push(mcp_server::AddedMeasurement {
                id: measurement.id,
                is_construction: measurement.is_construction,
                source_measurement_ids: measurement.source_measurement_ids.clone(),
                dependency_stale: measurement.dependency_stale,
                kind: match measurement.kind {
                    Tool::Distance => "distance",
                    Tool::Polyline => "polyline",
                    Tool::Angle => "angle",
                    Tool::Circle => "circle",
                    Tool::Rectangle => "rectangle",
                    Tool::Polygon => "polygon",
                    Tool::PointLineDistance => "point_line_distance",
                    Tool::ParallelDistance => "parallel_distance",
                    Tool::CircleFit => "circle_fit",
                    Tool::Ellipse => "ellipse",
                    Tool::Arc => "arc",
                    Tool::Coordinate => "coordinate",
                    Tool::ObjectCount => "object_count",
                }.into(),
                label: measurement.label.clone(),
                capture_id: measurement.capture_id.clone(),
                calibration_id: measurement.calibration_id.clone(),
                mm_per_pixel: measurement.mm_per_pixel,
                mm_per_pixel_x: measurement.mm_per_pixel_x,
                mm_per_pixel_y: measurement.mm_per_pixel_y,
                protocol_id: measurement.protocol_id.clone(),
                protocol_version: measurement.protocol_version,
                origin: measurement.origin.clone(),
                method: measurement.method.clone(),
                points_native_px: measurement.points.clone(),
                components: mcp_server::MeasurementView::from_measurement(&measurement).components,
                value: geometry::result(&measurement),
            });
        }
        self.message = format!("{} mesure(s) ajoutée(s) par l'IA; vérifier les points sur l'image.", added.len());
        Ok(added)
    }

    fn update_mcp_measurements(
        &mut self,
        capture_id: &str,
        expected_revision: Option<u64>,
        updates: Vec<mcp_server::MeasurementUpdateInput>,
    ) -> Result<Vec<mcp_server::MeasurementView>, String> {
        if self.running && !self.paused { return Err("Figez l'image avant de modifier des mesures.".into()); }
        if capture_id != self.capture_id { return Err("La capture a changé; relisez le contexte MCP.".into()); }
        if expected_revision.is_some_and(|revision| revision != self.document_revision) {
            return Err(format!("Le document a changé (révision actuelle {}). Relisez le contexte avant de modifier.", self.document_revision));
        }
        if self.mcp_mutation_blocked() { return Err("Terminez ou annulez l'action en cours dans l'interface avant de modifier les mesures.".into()); }
        if !self.quality_assessment.as_ref().is_some_and(|assessment| assessment.is_exploitable) {
            return Err(self.quality_gate_message());
        }
        let image = self.image.as_ref().ok_or_else(|| "Aucune image active.".to_string())?;
        if updates.is_empty() { return Err("Fournissez au moins une modification.".into()); }
        let width = image.width as f64;
        let height = image.height as f64;
        let mut prepared = Vec::with_capacity(updates.len());
        let mut used_ids = Vec::new();
        for update in updates {
            if used_ids.contains(&update.id) { return Err(format!("L'identifiant {} apparaît plusieurs fois.", update.id)); }
            used_ids.push(update.id);
            let before = self.measurements.iter().find(|measurement| measurement.id == update.id)
                .cloned().ok_or_else(|| format!("Mesure {} introuvable.", update.id))?;
            let mut after = before.clone();
            if let Some(points) = update.points {
                let points = points.iter().map(Point::from).collect::<Vec<_>>();
                let expected = after.kind.required_points();
                if expected.is_some_and(|count| points.len() != count) {
                    return Err(format!("{} requiert exactement {} points.", after.kind.label(), expected.unwrap()));
                }
                let minimum = match after.kind {
                    Tool::Polygon | Tool::CircleFit | Tool::Ellipse | Tool::Arc | Tool::PointLineDistance => 3,
                    Tool::ParallelDistance => 4,
                    Tool::Polyline => 2,
                    _ => 1,
                };
                if points.len() < minimum {
                    return Err(format!("La géométrie requiert au moins {minimum} points."));
                }
                if points.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()
                    || point.x < 0.0 || point.y < 0.0 || point.x > width || point.y > height)
                {
                    return Err("Un point est non fini ou sort des limites de l'image source.".into());
                }
                if !geometry::is_nondegenerate(after.kind, &points) {
                    return Err(format!("La géométrie de la mesure {} serait dégénérée.", after.id));
                }
                after.points = points;
            }
            if let Some(label) = update.label {
                let label = label.trim();
                after.label = if label.is_empty() { format!("M{}", after.id) } else { label.to_owned() };
            }
            if after.points == before.points && after.label == before.label { continue; }
            prepared.push((before, after));
        }
        if prepared.is_empty() { return Err("Aucune mesure ne change après normalisation.".into()); }
        let mut changed = Vec::with_capacity(prepared.len());
        for (before, after) in prepared {
            let id = after.id;
            if let Some(current) = self.measurements.iter_mut().find(|measurement| measurement.id == id) {
                *current = after.clone();
            }
            self.selected_measurement = Some(id);
            self.record_measurement_action(MeasurementHistoryAction::Edited { id, before, after: after.clone() });
            changed.push(mcp_server::MeasurementView::from_measurement(
                self.measurements.iter().find(|measurement| measurement.id == id).unwrap()));
        }
        self.message = format!("{} mesure(s) modifiée(s) par MCP.", changed.len());
        Ok(changed)
    }

    fn delete_mcp_measurements(&mut self, capture_id: &str, expected_revision: Option<u64>, ids: &[u64]) -> Result<mcp_server::McpContext, String> {
        self.ensure_mcp_editable_capture(capture_id)?;
        self.ensure_mcp_revision(expected_revision)?;
        if ids.is_empty() { return Err("Fournissez au moins un identifiant de mesure.".into()); }
        let mut unique = Vec::new();
        for id in ids {
            if unique.contains(id) { return Err(format!("L'identifiant {id} apparaît plusieurs fois.")); }
            unique.push(*id);
            if !self.measurements.iter().any(|measurement| measurement.id == *id) {
                return Err(format!("Mesure {id} introuvable; aucune suppression effectuée."));
            }
        }
        self.ensure_dependency_deletion(&unique)?;
        // Delete children first so each undo restores parents before children.
        let mut targets = Vec::new();
        while !unique.is_empty() {
            let next = unique.iter().position(|id| !self.measurements.iter().any(|child|
                unique.contains(&child.id) && child.source_measurement_ids.contains(id)))
                .ok_or("Dépendances cycliques : aucune suppression effectuée.")?;
            targets.push(unique.remove(next));
        }
        for id in targets {
            let index = self.measurements.iter().position(|measurement| measurement.id == id).unwrap();
            let measurement = self.measurements.remove(index);
            let previous_selection = self.selected_measurement;
            if previous_selection == Some(id) { self.selected_measurement = None; }
            self.record_measurement_action(MeasurementHistoryAction::Removed { measurement, index, previous_selection });
        }
        self.message = format!("{} mesure(s) supprimée(s) par MCP; restauration possible avec undo.", unique.len());
        Ok(self.mcp_context())
    }

    fn clear_mcp_measurements(&mut self, capture_id: &str, expected_revision: Option<u64>) -> Result<mcp_server::McpContext, String> {
        self.ensure_mcp_editable_capture(capture_id)?;
        self.ensure_mcp_revision(expected_revision)?;
        if !self.measurements.is_empty() {
            let previous_selection = self.selected_measurement;
            let measurements = self.measurements.clone();
            self.measurements.clear();
            self.selected_measurement = None;
            self.record_measurement_action(MeasurementHistoryAction::Cleared { measurements, previous_selection });
        }
        self.message = "Mesures effacées par MCP.".into();
        Ok(self.mcp_context())
    }

    fn apply_mcp_history(&mut self, action: &str, expected_revision: Option<u64>) -> Result<mcp_server::McpContext, String> {
        if self.mcp_mutation_blocked() { return Err("Terminez ou annulez l'action en cours avant d'utiliser l'historique.".into()); }
        self.ensure_mcp_revision(expected_revision)?;
        let changed = match action.trim().to_ascii_lowercase().as_str() {
            "undo" => self.undo_measurement_action(),
            "redo" => self.redo_measurement_action(),
            _ => return Err("Action invalide : choisissez undo ou redo.".into()),
        };
        if !changed { return Err(format!("Aucune action à {}.", if action.eq_ignore_ascii_case("undo") { "annuler" } else { "rétablir" })); }
        self.message = format!("Historique MCP : {action}.");
        Ok(self.mcp_context())
    }

    fn cancel_mcp_active_action(&mut self) {
        if self.registration_photo_pick_active() {
            self.cancel_registration_photo_pick();
            self.message = "Choix du repère photo annulé par MCP.".into();
        } else if self.export_dialog.take().is_some() {
            self.message = "Fenêtre de rapport fermée par MCP.".into();
        } else if self.point_edit.is_some() {
            self.finish_point_edit(true);
        } else if self.calibration_mode {
            self.calibration_mode = false;
            self.clear_pending();
            self.selection_mode = true;
            self.message = "Étalonnage en cours annulé par MCP.".into();
        } else if !self.pending.is_empty() {
            self.clear_pending();
            self.message = "Tracé en cours annulé par MCP.".into();
        } else {
            self.selected_measurement = None;
            self.message = "Sélection effacée par MCP.".into();
        }
    }

    fn select_mcp_measurement(&mut self, capture_id: &str, id: Option<u64>) -> Result<(), String> {
        if capture_id != self.capture_id { return Err("La capture a changé; relisez le contexte MCP.".into()); }
        if let Some(id) = id {
            if !self.measurements.iter().any(|measurement| measurement.id == id) {
                return Err(format!("Mesure {id} introuvable dans la capture active."));
            }
        }
        self.selected_measurement = id;
        self.selection_mode = true;
        Ok(())
    }

    fn set_mcp_calibration(
        &mut self,
        capture_id: &str,
        expected_revision: Option<u64>,
        reference_type: &str,
        points: &[mcp_server::NativePoint],
        known_mm: f64,
        expanded_uncertainty_mm: Option<f64>,
    ) -> Result<Calibration, String> {
        self.ensure_mcp_editable_capture(capture_id)?;
        self.ensure_mcp_revision(expected_revision)?;
        let calibration_type = match reference_type.trim().to_ascii_lowercase().as_str() {
            "segment" => CalibrationType::Segment,
            "circle_diameter" => CalibrationType::CircleDiameter,
            "rectangle_diagonal" => CalibrationType::RectangleDiagonal,
            _ => return Err("Type invalide : segment, circle_diameter ou rectangle_diagonal.".into()),
        };
        let points = points.iter().map(Point::from).collect::<Vec<_>>();
        let profile = self.calibrate_from_points(
            calibration_type, &points, known_mm, "mcp", expanded_uncertainty_mm,
        )?;
        self.message = format!("Étalonnage MCP {} enregistré : {:.8} mm/px.", calibration_type.label(), profile.mm_per_pixel);
        Ok(profile)
    }

    fn ensure_mcp_editable_capture(&self, capture_id: &str) -> Result<(), String> {
        if self.running && !self.paused { return Err("Figez l'image avant de modifier la capture.".into()); }
        if self.image.is_none() { return Err("Aucune image active.".into()); }
        if capture_id != self.capture_id { return Err("La capture a changé; relisez le contexte MCP.".into()); }
        if self.mcp_mutation_blocked() { return Err("Terminez ou annulez l'action en cours dans l'interface avant l'action MCP.".into()); }
        Ok(())
    }

    fn ensure_mcp_revision(&self, expected_revision: Option<u64>) -> Result<(), String> {
        if expected_revision.is_some_and(|revision| revision != self.document_revision) {
            return Err(format!("Le document a changé (révision actuelle {}). Relisez le contexte MCP avant de modifier.", self.document_revision));
        }
        Ok(())
    }

    fn start_camera_index(&mut self, index: usize) -> Result<(), String> {
        if self.capture_timer.is_some() || self.timelapse_capture.is_some() || self.video_recording.is_some() {
            return Err("Arrêtez la capture différée ou le time-lapse avant de changer de caméra.".into());
        }
        if !self.session_saved || !self.pending.is_empty() || self.calibration_mode || self.point_edit.is_some() {
            return Err("Enregistrez le contexte et terminez ou annulez le tracé avant de changer d'image.".into());
        }
        if index >= self.cameras.len() { return Err(format!("Caméra d'indice {index} inexistante; appelez list_cameras.")); }
        self.selected_camera = index;
        let (camera_index, name, device_id) = self.cameras[index].clone();
        let same_mode_device = self.camera_modes_camera.as_deref() == Some(device_id.as_str());
        let requested_mode = same_mode_device.then(|| self.selected_camera_mode
            .and_then(|mode_index| self.camera_modes.get(mode_index))
            .map(|mode| mode.format)).flatten();
        if !same_mode_device {
            self.camera_modes.clear();
            self.camera_modes_camera = None;
            self.selected_camera_mode = None;
            self.camera_controls.clear();
            self.camera_controls_camera = None;
        }
        self.camera_task = None;
        self.live_alignment = alignment::AlignmentState::Unknown;
        self.live_alignment_assessment = None;
        self.active_device = name;
        self.active_device_id = device_id;
        self.image_is_imported = false;
        self.capture_id = new_capture_id();
        self.document_revision = self.document_revision.wrapping_add(1);
        self.camera_task = Some(camera::start(camera_index, requested_mode));
        self.running = true; self.paused = false;
        self.image = None; self.texture = None;
        self.measurements.clear(); self.annotations.clear(); self.clear_pending();
        self.measurement_audit_history.clear();
        self.quality_assessment = None;
        self.undo_history.clear(); self.redo_history.clear(); self.point_edit = None;
        self.selected_measurement = None;
        self.session_saved = true;
        self.mm_per_pixel = None; self.calibration = None;
        self.calibration_history.clear();
        self.message = "Ouverture du flux USB UVC depuis MCP…".into();
        Ok(())
    }

    fn control_mcp_stream(&mut self, action: &str, expected_revision: Option<u64>) -> Result<(), String> {
        self.ensure_mcp_revision(expected_revision)?;
        if self.video_recording.is_some() || self.timelapse_capture.is_some() {
            return Err("Arrêtez l'enregistrement vidéo/time-lapse avant de modifier l'état du flux.".into());
        }
        match action.trim().to_ascii_lowercase().as_str() {
            "freeze" => {
                if self.image.is_none() { return Err("Aucune image disponible à figer.".into()); }
                self.freeze_capture();
            }
            "resume" => {
                if !self.running { return Err("Aucun flux caméra à reprendre.".into()); }
                if !self.paused { return Err("Le flux caméra est déjà en direct.".into()); }
                if !self.pending.is_empty() || self.point_edit.is_some() || self.calibration_mode {
                    return Err("Terminez ou annulez l'action en cours avant de reprendre le flux.".into());
                }
                if !self.session_saved && !self.measurements.is_empty() {
                    return Err("Enregistrez le contexte avant la reprise : les mesures liées à l'image seraient effacées.".into());
                }
                self.toggle_stream_pause();
            }
            "stop" => {
                if self.running && !self.paused { self.freeze_capture(); }
                self.camera_task = None;
                self.running = false;
                self.paused = true;
                self.message = "Flux caméra arrêté; la dernière image reste disponible.".into();
            }
            _ => return Err("Action invalide : choisissez freeze, resume ou stop.".into()),
        }
        Ok(())
    }

    fn set_mcp_workspace(
        &mut self,
        optical_setup: Option<String>,
        measurement_tool: Option<&str>,
        show_grid: Option<bool>,
        show_center_guides: Option<bool>,
        zoom_percent: Option<f32>,
    ) -> Result<(), String> {
        if let Some(setup) = optical_setup {
            let setup = setup.trim();
            if setup.is_empty() || setup.chars().count() > 120 { return Err("La configuration optique doit contenir entre 1 et 120 caractères.".into()); }
            self.optical_setup = setup.to_owned();
            if !self.image_is_imported { self.activate_saved_calibration(); }
            self.session_saved = false;
            self.document_revision = self.document_revision.wrapping_add(1);
        }
        if let Some(tool) = measurement_tool {
            let normalized = tool.trim().to_ascii_lowercase();
            if normalized == "select" {
                self.selection_mode = true;
                self.clear_pending();
            } else {
                self.mode = match normalized.as_str() {
                    "distance" => Tool::Distance,
                    "polyline" => Tool::Polyline,
                    "angle" => Tool::Angle,
                    "circle" => Tool::Circle,
                    "rectangle" => Tool::Rectangle,
                    "polygon" => Tool::Polygon,
                    "point_line_distance" | "point-line-distance" => Tool::PointLineDistance,
                    "parallel_distance" | "parallel-distance" => Tool::ParallelDistance,
                    "circle_fit" | "circle-fit" => Tool::CircleFit,
                    "ellipse" => Tool::Ellipse,
                    "arc" => Tool::Arc,
                    "coordinate" => Tool::Coordinate,
                    "object_count" | "count_objects" | "object-count" => Tool::ObjectCount,
                    _ => return Err("Outil invalide : distance, polyline, angle, circle, rectangle, polygon, point_line_distance, parallel_distance, circle_fit, ellipse, arc, coordinate, object_count ou select.".into()),
                };
                self.selection_mode = false;
                self.drawing_variant = drawing_variants::DrawingVariant::for_tool(self.mode);
                self.pan_mode = false;
                self.annotation_mode = false;
                self.annotation_points.clear();
            }
        }
        if let Some(value) = show_grid { self.show_alignment_grid = value; }
        if let Some(value) = show_center_guides { self.show_center_guides = value; }
        if let Some(zoom) = zoom_percent {
            if !zoom.is_finite() || !(5.0..=3200.0).contains(&zoom) {
                return Err("Le zoom demandé est hors de la plage du visualiseur (5 à 3200 %).".into());
            }
            self.zoom_percent = zoom;
            self.zoom = zoom / 100.0;
            self.fit_view = false;
        }
        Ok(())
    }

    fn prepare_mcp_report(
        &mut self,
        ctx: &egui::Context,
        requested_folder: Option<&Path>,
    ) -> Result<mcp_server::ReportPreparation, String> {
        if !self.pending.is_empty() || self.point_edit.is_some() || self.calibration_mode {
            return Err("Terminez ou annulez l'action de mesure avant de préparer le rapport.".into());
        }
        if self.running && !self.paused { self.freeze_capture(); }
        let folder = requested_folder.map(Path::to_path_buf).or_else(|| self.image.as_ref()
            .and_then(|image| Path::new(&image.source).is_file().then(|| Path::new(&image.source).parent().map(Path::to_path_buf)).flatten()))
            .unwrap_or_else(|| self.directory.clone());
        let dialog = folder_export::Dialog::new(ctx, folder.clone(), self.current_export_image());
        let items = dialog.rows.iter().map(|row| {
            let mut calibrations = row.session.calibrations.clone();
            if let Some(active) = row.session.calibration.as_ref()
                .filter(|active| !calibrations.iter().any(|saved| saved.id == active.id))
            {
                calibrations.push(active.clone());
            }
            mcp_server::ReportItemView {
                key: row.key.clone(), name: row.name.clone(), width: row.width, height: row.height,
                selected: row.selected, measurement_count: row.session.measurements.len(),
                calibration_id: row.session.calibration.as_ref().map(|profile| profile.id.clone()),
                calibrations,
                measurements: row.session.measurements.iter()
                    .map(mcp_server::MeasurementView::from_measurement).collect(),
                warning: row.warning.clone(), error: row.error.clone(),
            }
        }).collect::<Vec<_>>();
        let selected_count = dialog.selected_count();
        Ok(mcp_server::ReportPreparation {
            folder: folder.to_string_lossy().into_owned(), capture_id: self.capture_id.clone(),
            document_revision: self.document_revision,
            items, selected_count,
        })
    }

    fn export_mcp_report(
        &mut self,
        ctx: &egui::Context,
        args: mcp_server::ExportReportArgs,
    ) -> Result<mcp_server::ReportExportResult, String> {
        let inspection_notes = self.inspection_audit_notes();
        if self.running && !self.paused { return Err("Figez l'image ou appelez prepare_report avant l'export.".into()); }
        if args.capture_id != self.capture_id { return Err("La capture a changé; appelez prepare_report et utilisez son capture_id.".into()); }
        self.ensure_mcp_revision(args.expected_revision)?;
        if !self.pending.is_empty() || self.point_edit.is_some() || self.calibration_mode {
            return Err("Terminez ou annulez l'action de mesure avant l'export.".into());
        }
        let (extension, format_label) = match args.format.trim().to_ascii_lowercase().as_str() {
            "pdf" => ("pdf", "PDF"),
            "pptx" => ("pptx", "PowerPoint"),
            _ => return Err("Format invalide : choisissez pdf ou pptx.".into()),
        };
        let folder = args.folder.as_deref().map(PathBuf::from).or_else(|| self.image.as_ref()
            .and_then(|image| Path::new(&image.source).is_file().then(|| Path::new(&image.source).parent().map(Path::to_path_buf)).flatten()))
            .unwrap_or_else(|| self.directory.clone());
        let mut dialog = folder_export::Dialog::new(ctx, folder.clone(), self.current_export_image());
        if let Some(keys) = args.selected_image_keys.as_ref() {
            for row in &mut dialog.rows {
                row.selected = !row.session.measurements.is_empty()
                    && row.error.is_none() && keys.iter().any(|key| key == &row.key);
            }
            for key in keys {
                if !dialog.rows.iter().any(|row| &row.key == key) {
                    return Err(format!("Clé d'image inconnue ou périmée : {key}. Appelez prepare_report."));
                }
            }
        }
        if let Some(metadata) = args.photo_metadata.as_ref() {
            for item in metadata {
                let row = dialog.rows.iter_mut().find(|row| row.key == item.key)
                    .ok_or_else(|| format!("Clé d'image inconnue pour les métadonnées : {}.", item.key))?;
                if let Some(description) = item.description.as_ref() {
                    row.description = description.clone();
                }
                if let Some(include_timestamp) = item.include_timestamp { row.include_timestamp = include_timestamp; }
            }
        }
        if let Some(title) = args.title.as_ref() {
            if title.chars().count() > 240 { return Err("Le titre du rapport ne peut pas dépasser 240 caractères.".into()); }
            dialog.preferences.title = title.clone();
        }
        if let Some(value) = args.include_title { dialog.preferences.include_title = value; }
        if let Some(value) = args.include_folder_name { dialog.preferences.include_folder_name = value; }
        if let Some(value) = args.include_picture_count { dialog.preferences.include_picture_count = value; }
        if let Some(value) = args.include_generation_date { dialog.preferences.include_generation_date = value; }
        if let Some(value) = args.include_sequence_numbers { dialog.preferences.include_sequence_numbers = value; }
        if let Some(value) = args.include_file_names { dialog.preferences.include_file_names = value; }
        if let Some(value) = args.include_descriptions { dialog.preferences.include_descriptions = value; }
        if let Some(value) = args.include_measurements { dialog.preferences.include_measurements = value; }
        if let Some(value) = args.operator_name.as_ref() {
            if value.chars().count() > 160 { return Err("Le champ opérateur dépasse 160 caractères.".into()); }
            dialog.preferences.operator_name = value.clone();
        }
        if let Some(value) = args.part_reference.as_ref() {
            if value.chars().count() > 160 { return Err("Le champ référence pièce dépasse 160 caractères.".into()); }
            dialog.preferences.part_reference = value.clone();
        }
        if let Some(value) = args.drawing_revision.as_ref() {
            if value.chars().count() > 160 { return Err("Le champ révision dépasse 160 caractères.".into()); }
            dialog.preferences.drawing_revision = value.clone();
        }
        if let Some(value) = args.lot_number.as_ref() {
            if value.chars().count() > 160 { return Err("Le champ lot dépasse 160 caractères.".into()); }
            dialog.preferences.lot_number = value.clone();
        }
        if let Some(value) = args.approved_by.as_ref() {
            if value.chars().count() > 160 { return Err("Le champ validation dépasse 160 caractères.".into()); }
            dialog.preferences.approved_by = value.clone();
        }
        if let Some(status) = args.approval_status.as_ref() {
            if !["À renseigner", "Approuvé", "Refusé", "À confirmer"].contains(&status.as_str()) {
                return Err("Décision d’audit invalide.".into());
            }
            dialog.preferences.approval_status = status.clone();
        }
        if let Some(notes) = args.audit_notes.as_ref() {
            if notes.chars().count() > 4000 { return Err("Les notes d’audit dépassent 4000 caractères.".into()); }
            dialog.preferences.audit_notes = notes.clone();
        }
        if let Some(value) = args.include_audit_page { dialog.preferences.include_audit_page = value; }
        let selected_rows = dialog.rows.iter().filter(|row| row.selected && row.error.is_none()
            && !row.session.measurements.is_empty()).collect::<Vec<_>>();
        if selected_rows.is_empty() { return Err("Aucune image valide n'est sélectionnée pour le rapport.".into()); }
        let photos = dialog.report_photos()?;
        let folder_name = folder.file_name().and_then(|name| name.to_str())
            .unwrap_or_else(|| folder.to_str().unwrap_or("Dossier")).to_owned();
        let mut options = report::FolderReportOptions {
            title: dialog.preferences.title.clone(), include_title: dialog.preferences.include_title,
            folder_name: folder_name.clone(), include_folder_name: dialog.preferences.include_folder_name,
            include_picture_count: dialog.preferences.include_picture_count,
            include_generation_date: dialog.preferences.include_generation_date,
            include_sequence_numbers: dialog.preferences.include_sequence_numbers,
            include_file_names: dialog.preferences.include_file_names,
            include_descriptions: dialog.preferences.include_descriptions,
            include_measurements: dialog.preferences.include_measurements,
            include_audit_page: dialog.preferences.include_audit_page,
            operator_name: dialog.preferences.operator_name.clone(),
            part_reference: dialog.preferences.part_reference.clone(),
            drawing_revision: dialog.preferences.drawing_revision.clone(),
            lot_number: dialog.preferences.lot_number.clone(),
            audit_notes: dialog.preferences.audit_notes.clone(),
            approval_status: dialog.preferences.approval_status.clone(),
            approved_by: dialog.preferences.approved_by.clone(),
        };
        // Only operator notes are subject to the editable 4000-character limit.
        // Generated inspection context is separate and must not be persisted as
        // the operator's text or force an optional audit page on their behalf.
        if !inspection_notes.is_empty() {
            options.audit_notes.push('\n');
            options.audit_notes.push_str(&inspection_notes);
        }
        let mut destination = PathBuf::from(args.destination);
        if destination.extension().and_then(|ext| ext.to_str()).is_none_or(|ext| !ext.eq_ignore_ascii_case(extension)) {
            destination.set_extension(extension);
        }
        let parent = destination.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| format!("Dossier de sortie inaccessible : {error}"))?;
        let report_name = destination.file_name().and_then(|name| name.to_str()).unwrap_or("rapport");
        let manifest = destination.with_file_name(format!("{report_name}.dper_measure.json"));
        if !args.overwrite && (destination.exists() || manifest.exists()) {
            return Err(format!("Le rapport ou son manifeste existe déjà ({} / {}). Passez overwrite=true pour remplacer les deux.", destination.display(), manifest.display()));
        }
        let generated_at = report::utc_timestamp_for_export();
        let metadata = serde_json::json!({
            "format_version": 2,
            "application": "DPER_Measure",
            "application_version": env!("CARGO_PKG_VERSION"),
            "format": format_label,
            "folder": folder.to_string_lossy(),
            "title": &options.title,
            "generated_at_utc": generated_at,
            "options": {
                "include_title": options.include_title,
                "include_folder_name": options.include_folder_name,
                "include_picture_count": options.include_picture_count,
                "include_generation_date": options.include_generation_date,
                "include_sequence_numbers": options.include_sequence_numbers,
                "include_file_names": options.include_file_names,
                "include_descriptions": options.include_descriptions,
                "include_measurements": options.include_measurements
            },
            "audit": {
                "include_audit_page": options.include_audit_page,
                "operator_name": &options.operator_name,
                "part_reference": &options.part_reference,
                "drawing_revision": &options.drawing_revision,
                "lot_number": &options.lot_number,
                "audit_notes": &options.audit_notes,
                "approval_status": &options.approval_status,
                "approved_by": &options.approved_by
            },
            "quality_notice": "La décision utilise l'intervalle mesuré ± U; sans U élargie documentée, qualité d'image exploitable et protocole de série comparable, le verdict reste suspendu. Les seuils de qualité sont provisoires et non qualifiés.",
            "pictures": photos.iter().map(|photo| serde_json::json!({
                "name": &photo.name,
                "description": &photo.description,
                "warning": &photo.warning,
                "timestamp": &photo.timestamp,
                "include_timestamp": photo.include_timestamp,
                "series_notice": &photo.series_notice,
                "session": &photo.session,
                "reference_comparison": photo.reference.as_ref().map(|reference| serde_json::json!({
                    "source_name": &reference.source_name,
                    "source_error": &reference.source_error,
                    "overall_status": &reference.overall_status,
                    "dimensions": reference.dimensions.iter().map(|dimension| serde_json::json!({
                        "id": &dimension.id,
                        "label": &dimension.label,
                        "source_text": &dimension.source_text,
                        "ocr_page": dimension.ocr_page,
                        "ocr_bbox": &dimension.ocr_bbox,
                        "ocr_confidence": dimension.ocr_confidence,
                        "nominal_mm": dimension.nominal_mm,
                        "tolerance_minus_mm": dimension.tolerance_minus_mm,
                        "tolerance_plus_mm": dimension.tolerance_plus_mm,
                        "measured_mm": dimension.measured_mm,
                        "expanded_uncertainty_mm": dimension.expanded_uncertainty_mm,
                        "lower_limit_mm": dimension.lower_limit_mm,
                        "upper_limit_mm": dimension.upper_limit_mm,
                        "status": &dimension.status
                    })).collect::<Vec<_>>()
                }))
            })).collect::<Vec<_>>()
        });
        let manifest_bytes = serde_json::to_vec_pretty(&metadata)
            .map_err(|error| format!("Manifeste JSON invalide : {error}"))?;
        let nonce = new_id("mcp-export");
        let temporary_report = destination.with_file_name(format!(".{report_name}.{nonce}.tmp.{extension}"));
        let manifest_name = manifest.file_name().and_then(|name| name.to_str()).unwrap_or("rapport.dper_measure.json");
        let temporary_manifest = manifest.with_file_name(format!(".{manifest_name}.{nonce}.tmp"));
        let write_result = (|| {
            let writer_warning = match extension {
                "pdf" => {
                    report::write_folder_pdf(&temporary_report, &options, &photos)?;
                    None
                }
                _ => pptx::write_folder_pptx(&temporary_report, &options, &photos)?,
            };
            fs::OpenOptions::new().write(true).create_new(true).open(&temporary_manifest)
                .and_then(|mut file| std::io::Write::write_all(&mut file, &manifest_bytes))
                .map_err(|error| format!("Écriture du manifeste impossible : {error}"))?;
            Ok::<Option<String>, String>(writer_warning)
        })();
        let writer_warning = match write_result {
            Ok(warning) => warning,
            Err(error) => {
                let _ = fs::remove_file(&temporary_report);
                let _ = fs::remove_file(&temporary_manifest);
                return Err(error);
            }
        };
        let mut warnings = Vec::new();
        if let Some(warning) = writer_warning { warnings.push(warning); }
        replace_mcp_output(&temporary_report, &destination, args.overwrite, &mut warnings)?;
        let manifest_written = match replace_mcp_output(&temporary_manifest, &manifest, args.overwrite, &mut warnings) {
            Ok(()) => true,
            Err(error) => {
                let _ = fs::remove_file(&temporary_manifest);
                warnings.push(format!("Le rapport existe à {}, mais le manifeste {} n'a pas été installé : {error}",
                    destination.display(), manifest.display()));
                false
            }
        };
        if args.persist_preferences {
            if let Err(error) = dialog.save_preferences() {
                warnings.push(format!("Préférences du dossier non enregistrées : {error}"));
            }
        }
        let warning = if warnings.is_empty() { None } else { Some(warnings.join("; ")) };
        self.message = if manifest_written {
            format!("Rapport {format_label} MCP exporté : {} · manifeste {}", destination.display(), manifest.display())
        } else {
            format!("Rapport {format_label} MCP exporté sans manifeste : {}", destination.display())
        };
        Ok(mcp_server::ReportExportResult {
            format: format_label.into(), report_path: destination.to_string_lossy().into_owned(),
            manifest_path: manifest.to_string_lossy().into_owned(), manifest_written,
            image_count: photos.len(), warning,
        })
    }

    fn current_export_image(&self) -> Option<folder_export::CurrentImage> {
        let image = self.image.as_ref()?;
        let mut inspection = self.inspection.clone();
        let mut measurements = self.measurements.clone();
        let changed_ids = self.changed_drawing_dependency_ids();
        if !changed_ids.is_empty() {
            // Export owns a conservative snapshot even before the UI refreshes.
            inspection.dependencies.invalidate(&measurements, &changed_ids);
            inspection.apply_dependency_flags(&mut measurements);
        }
        Some(folder_export::CurrentImage {
            automated_inspection: Some(inspection),
            source: image.source.clone(), capture_id: self.capture_id.clone(),
            device: self.active_device.clone(), optical_setup: self.optical_setup.clone(),
            device_id: self.active_device_id.clone(),
            calibration: self.calibration.clone(), calibrations: self.calibration_history.clone(),
            measurements, rgb: image.rgb.clone(),
            annotations: self.annotations.clone(),
            measurement_audit_history: self.measurement_audit_history.clone(),
            quality_thresholds: self.quality_thresholds.clone(),
            quality_assessment: self.quality_assessment.clone(),
            display_correction: self.display_correction,
            protocol: self.active_protocol.clone(),
            reference_document: self.reference_document.clone(),
            width: image.width, height: image.height,
        })
    }

    fn open_folder_export(&mut self, ctx: &egui::Context) {
        if !self.pending.is_empty() {
            self.message = "Terminez ou annulez le tracé en cours avant l'export.".into();
            return;
        }
        let folder = self.image.as_ref()
            .and_then(|image| {
                let source = Path::new(&image.source);
                source.is_file().then(|| source.parent().map(Path::to_path_buf)).flatten()
            })
            .unwrap_or_else(|| self.directory.clone());
        let mut dialog = folder_export::Dialog::new(ctx, folder, self.current_export_image());
        if dialog.preferences.title.trim().is_empty() {
            let folder = dialog.folder.file_name().and_then(|name| name.to_str()).unwrap_or("DPER_Measure");
            dialog.preferences.title = format!("{folder} — rapport DPER_Measure");
        }
        self.export_dialog = Some(dialog);
    }

    fn export_folder_report(&mut self, format: FolderExportFormat) {
        let Some(dialog) = self.export_dialog.as_ref() else { return; };
        if matches!(format, FolderExportFormat::BatchCsv | FolderExportFormat::BatchXlsx) {
            let (bytes, extension, filter_name, default_suffix, format_label) = match format {
                FolderExportFormat::BatchCsv => (
                    dialog.measurements_csv_batch(), "csv", "Mesures du lot CSV", "mesures.csv", "CSV",
                ),
                FolderExportFormat::BatchXlsx => (
                    dialog.measurements_xlsx_batch(), "xlsx", "Mesures du lot Excel", "mesures.xlsx", "XLSX",
                ),
                _ => unreachable!(),
            };
            let bytes = match bytes {
                Ok(bytes) => bytes,
                Err(error) => { self.message = error; return; }
            };
            let folder_name = dialog.folder.file_name().and_then(|name| name.to_str()).unwrap_or("lot");
            let default_name = format!("{folder_name}_{default_suffix}");
            let Some(mut path) = rfd::FileDialog::new().set_directory(&dialog.folder)
                .set_file_name(&default_name).add_filter(filter_name, &[extension]).save_file() else { return; };
            if path.extension().and_then(|extension| extension.to_str())
                .is_none_or(|value| !value.eq_ignore_ascii_case(extension))
            { path.set_extension(extension); }
            if let Err(error) = dialog.validate_selected_drawing_sources() {
                self.message = error;
                return;
            }
            let preferences_result = dialog.save_preferences();
            match fs::write(&path, bytes) {
                Ok(()) => self.message = match preferences_result {
                    Ok(()) => format!("{format_label} de lot exporté : {}", path.display()),
                    Err(error) => format!("{format_label} de lot exporté : {} · préférences non enregistrées : {error}", path.display()),
                },
                Err(error) => self.message = format!("{format_label} de lot impossible à écrire : {error}"),
            }
            self.export_dialog = None;
            return;
        }
        let photos = match dialog.report_photos() {
            Ok(photos) => photos,
            Err(error) => { self.message = error; return; }
        };
        let folder_name = dialog.folder.file_name().and_then(|name| name.to_str())
            .unwrap_or_else(|| dialog.folder.to_str().unwrap_or("Dossier"));
        let options = report::FolderReportOptions {
            title: dialog.preferences.title.clone(),
            include_title: dialog.preferences.include_title,
            folder_name: folder_name.to_owned(),
            include_folder_name: dialog.preferences.include_folder_name,
            include_picture_count: dialog.preferences.include_picture_count,
            include_generation_date: dialog.preferences.include_generation_date,
            include_sequence_numbers: dialog.preferences.include_sequence_numbers,
            include_file_names: dialog.preferences.include_file_names,
            include_descriptions: dialog.preferences.include_descriptions,
            include_measurements: dialog.preferences.include_measurements,
            include_audit_page: dialog.preferences.include_audit_page,
            operator_name: dialog.preferences.operator_name.clone(),
            part_reference: dialog.preferences.part_reference.clone(),
            drawing_revision: dialog.preferences.drawing_revision.clone(),
            lot_number: dialog.preferences.lot_number.clone(),
            audit_notes: dialog.preferences.audit_notes.clone(),
            approval_status: dialog.preferences.approval_status.clone(),
            approved_by: dialog.preferences.approved_by.clone(),
        };
        let safe_title = dialog.preferences.title.trim().chars().map(|character| {
            if matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' }
            else { character }
        }).collect::<String>();
        let (extension, filter_name, output_label) = match format {
            FolderExportFormat::Pdf => ("pdf", "Rapport PDF", "PDF"),
            FolderExportFormat::PowerPoint => ("pptx", "Présentation PowerPoint", "PowerPoint"),
            FolderExportFormat::BatchCsv | FolderExportFormat::BatchXlsx => unreachable!(),
        };
        let default_name = format!("{}.{}", if safe_title.is_empty() { "rapport_dossier" } else { &safe_title }, extension);
        let Some(mut path) = rfd::FileDialog::new().set_directory(&dialog.folder)
            .set_file_name(&default_name).add_filter(filter_name, &[extension]).save_file() else { return; };
        if path.extension().and_then(|ext| ext.to_str()).is_none_or(|ext| !ext.eq_ignore_ascii_case(extension)) {
            path.set_extension(extension);
        }
        if let Err(error) = dialog.validate_selected_drawing_sources() {
            self.message = error;
            return;
        }
        let write_result = match format {
            FolderExportFormat::Pdf => report::write_folder_pdf(&path, &options, &photos).map(|()| None),
            FolderExportFormat::PowerPoint => pptx::write_folder_pptx(&path, &options, &photos),
            FolderExportFormat::BatchCsv | FolderExportFormat::BatchXlsx => unreachable!(),
        };
        let export_warning = match write_result {
            Ok(warning) => warning,
            Err(error) => { self.message = format!("Échec de création du rapport : {error}"); return; }
        };

        let metadata = serde_json::json!({
            "format_version": 1,
            "folder": folder_name,
            "title": &options.title,
            "generated_at_utc": report::utc_timestamp_for_export(),
            "options": {
                "include_title": options.include_title,
                "include_folder_name": options.include_folder_name,
                "include_picture_count": options.include_picture_count,
                "include_generation_date": options.include_generation_date,
                "include_sequence_numbers": options.include_sequence_numbers,
                "include_file_names": options.include_file_names,
                "include_descriptions": options.include_descriptions,
                "include_measurements": options.include_measurements
            },
            "audit": {
                "include_audit_page": options.include_audit_page,
                "operator_name": &options.operator_name,
                "part_reference": &options.part_reference,
                "drawing_revision": &options.drawing_revision,
                "lot_number": &options.lot_number,
                "audit_notes": &options.audit_notes,
                "approval_status": &options.approval_status,
                "approved_by": &options.approved_by
            },
            "quality_notice": "La décision utilise l'intervalle mesuré ± U; sans U élargie documentée, qualité d'image exploitable et protocole de série comparable, le verdict reste suspendu. Les seuils de qualité sont provisoires et non qualifiés.",
            "pictures": photos.iter().map(|photo| serde_json::json!({
                "name": &photo.name,
                "description": &photo.description,
                "warning": &photo.warning,
                "timestamp": &photo.timestamp,
                "include_timestamp": photo.include_timestamp,
                "series_notice": &photo.series_notice,
                "session": &photo.session,
                "reference_comparison": photo.reference.as_ref().map(|reference| serde_json::json!({
                    "source_name": &reference.source_name,
                    "source_error": &reference.source_error,
                    "overall_status": &reference.overall_status,
                    "dimensions": reference.dimensions.iter().map(|dimension| serde_json::json!({
                        "id": &dimension.id,
                        "label": &dimension.label,
                        "source_text": &dimension.source_text,
                        "ocr_page": dimension.ocr_page,
                        "ocr_bbox": &dimension.ocr_bbox,
                        "ocr_confidence": dimension.ocr_confidence,
                        "nominal_mm": dimension.nominal_mm,
                        "tolerance_minus_mm": dimension.tolerance_minus_mm,
                        "tolerance_plus_mm": dimension.tolerance_plus_mm,
                        "measured_mm": dimension.measured_mm,
                        "expanded_uncertainty_mm": dimension.expanded_uncertainty_mm,
                        "lower_limit_mm": dimension.lower_limit_mm,
                        "upper_limit_mm": dimension.upper_limit_mm,
                        "status": &dimension.status
                    })).collect::<Vec<_>>()
                }))
            })).collect::<Vec<_>>()
        });
        let report_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("rapport");
        let json_path = path.with_file_name(format!("{report_name}.dper_measure.json"));
        let json_result = serde_json::to_vec_pretty(&metadata)
            .map_err(std::io::Error::other)
            .and_then(|body| fs::OpenOptions::new().write(true).create_new(true).open(&json_path)
                .and_then(|mut file| std::io::Write::write_all(&mut file, &body)));
        let preferences_result = self.export_dialog.as_ref()
            .map(folder_export::Dialog::save_preferences).unwrap_or(Ok(()));
        let warning_suffix = export_warning.map(|warning| format!(" ({warning})")).unwrap_or_default();
        self.message = match (json_result, preferences_result) {
            (Ok(()), Ok(())) => format!("Rapport {output_label} et données JSON exportés : {} · {}{}", path.display(), json_path.display(), warning_suffix),
            (Err(error), _) => format!("Rapport {output_label} créé, mais JSON impossible à enregistrer : {error}{warning_suffix}"),
            (Ok(()), Err(error)) => format!("Rapport exporté, mais préférences du dossier non enregistrées : {error}{warning_suffix}"),
        };
        self.export_dialog = None;
    }

    fn show_folder_export_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.export_dialog.as_mut() else { return; };
        let mut choose_folder = false;
        let mut refresh_folder = false;
        let mut select_all = false;
        let mut select_none = false;
        let mut cancel = false;
        let mut export_format = None;
        let mut apply_template = None::<String>;
        let mut save_template = false;
        let mut delete_template = false;
        let mut folder_text = dialog.folder.to_string_lossy().into_owned();
        let include_descriptions = dialog.preferences.include_descriptions;
        let selected_count = dialog.selected_count();

        let window_frame = egui::Frame::window(&ctx.style())
            .fill(Color32::WHITE)
            .stroke(Stroke::new(1.0, Color32::from_rgb(196, 211, 229)))
            .corner_radius(14)
            .inner_margin(18.0);
        let modal_response = egui::Modal::new(egui::Id::new("folder-export-dialog"))
            .frame(window_frame)
            .show(ctx, |ui| {
                ui.set_width(1000.0);
                ui.scope(|ui| {
                    {
                        let visuals = ui.visuals_mut();
                        visuals.dark_mode = false;
                        visuals.override_text_color = Some(Color32::from_rgb(43, 59, 79));
                        visuals.weak_text_color = Some(Color32::from_rgb(112, 132, 154));
                        visuals.extreme_bg_color = Color32::WHITE;
                        visuals.text_edit_bg_color = Some(Color32::WHITE);
                        visuals.selection.bg_fill = Color32::from_rgb(26, 143, 153);
                        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);

                        visuals.widgets.noninteractive.fg_stroke =
                            Stroke::new(1.0, Color32::from_rgb(43, 59, 79));
                        visuals.widgets.inactive.bg_fill = Color32::WHITE;
                        visuals.widgets.inactive.bg_stroke =
                            Stroke::new(1.0, Color32::from_rgb(173, 191, 212));
                        visuals.widgets.inactive.fg_stroke =
                            Stroke::new(2.0, Color32::from_rgb(26, 143, 153));
                        visuals.widgets.inactive.corner_radius = 6.into();
                        visuals.widgets.hovered.bg_fill = Color32::from_rgb(235, 247, 249);
                        visuals.widgets.hovered.bg_stroke =
                            Stroke::new(1.2, Color32::from_rgb(26, 143, 153));
                        visuals.widgets.hovered.fg_stroke =
                            Stroke::new(2.0, Color32::from_rgb(20, 125, 137));
                        visuals.widgets.active.bg_fill = Color32::from_rgb(26, 143, 153);
                        visuals.widgets.active.bg_stroke =
                            Stroke::new(1.0, Color32::from_rgb(20, 125, 137));
                        visuals.widgets.active.fg_stroke = Stroke::new(2.2, Color32::WHITE);
                        visuals.widgets.open = visuals.widgets.hovered;
                    }
                    {
                        let style = ui.style_mut();
                        style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
                        style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.5));
                        style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.5));
                    }
                    {
                        let spacing = ui.spacing_mut();
                        spacing.icon_width = 18.0;
                        spacing.item_spacing = egui::vec2(10.0, 8.0);
                    }
                    ui.colored_label(Color32::from_rgb(31, 78, 139),
                        egui::RichText::new("Export du dossier d'images").size(23.0).strong());
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_rgb(66, 91, 120), "Dossier des images");
                        ui.add(egui::TextEdit::singleline(&mut folder_text)
                            .desired_width(620.0).interactive(false));
                        if ui.add(egui::Button::new("Parcourir…")
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(183, 199, 218)))
                            .corner_radius(7)).clicked() { choose_folder = true; }
                        if ui.add(egui::Button::new("Actualiser")
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(183, 199, 218)))
                            .corner_radius(7)).clicked() { refresh_folder = true; }
                    });
                    ui.weak("Les images du dossier sont listées directement. Les images sans mesure associée sont exclues du rapport.");
                    ui.add_space(8.0);

                    egui::Frame::new()
                        .fill(Color32::from_rgb(248, 250, 253))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(211, 222, 236)))
                        .corner_radius(10)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.colored_label(Color32::from_rgb(44, 89, 145),
                                egui::RichText::new("EN-TÊTE DU RAPPORT").size(12.5).strong());
                            ui.horizontal(|ui| {
                                ui.label("Modèle");
                                let selection_before = dialog.preferences.selected_template.clone();
                                egui::ComboBox::from_id_salt("folder-export-template-select")
                                    .selected_text(if selection_before.is_empty() { "Aucun modèle" }
                                        else { selection_before.as_str() })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut dialog.preferences.selected_template,
                                            String::new(), "Aucun modèle");
                                        for template in &dialog.preferences.templates {
                                            ui.selectable_value(&mut dialog.preferences.selected_template,
                                                template.name.clone(), &template.name);
                                        }
                                    });
                                if dialog.preferences.selected_template != selection_before
                                    && !dialog.preferences.selected_template.is_empty()
                                {
                                    apply_template = Some(dialog.preferences.selected_template.clone());
                                }
                                ui.add(egui::TextEdit::singleline(&mut dialog.preferences.template_name_draft)
                                    .hint_text("Nom du modèle").char_limit(80).desired_width(170.0));
                                if ui.button("Enregistrer modèle").clicked() { save_template = true; }
                                if ui.add_enabled(!dialog.preferences.selected_template.is_empty(),
                                    egui::Button::new("Supprimer modèle")).clicked() { delete_template = true; }
                            });
                            ui.weak("Les modèles nommés sont conservés avec les préférences de ce dossier.");
                            ui.add_space(5.0);
                            ui.checkbox(&mut dialog.preferences.include_title, "Inclure le titre du rapport");
                            ui.colored_label(Color32::from_rgb(99, 119, 143),
                                egui::RichText::new("TITRE DE SESSION / RAPPORT").size(10.5).strong());
                            ui.add_enabled(dialog.preferences.include_title,
                                egui::TextEdit::singleline(&mut dialog.preferences.title)
                                    .desired_width(f32::INFINITY).margin(egui::vec2(10.0, 8.0)));
                            ui.weak("Le titre et les descriptions sont mémorisés dans ce dossier pour les prochains exports.");
                            ui.add_space(7.0);
                            egui::Grid::new("folder-export-heading-options")
                                .num_columns(2).spacing(egui::vec2(34.0, 4.0)).show(ui, |ui| {
                                ui.checkbox(&mut dialog.preferences.include_folder_name, "Inclure le nom du dossier");
                                ui.checkbox(&mut dialog.preferences.include_picture_count, "Inclure le nombre d'images");
                                ui.end_row();
                                ui.checkbox(&mut dialog.preferences.include_generation_date, "Inclure la date de génération");
                                ui.label("");
                            });
                            ui.add_space(9.0);
                            ui.separator();
                            ui.checkbox(&mut dialog.preferences.include_audit_page,
                                "Ajouter la fiche de contrôle et d’audit au rapport");
                            if dialog.preferences.include_audit_page {
                                egui::Grid::new("folder-export-audit-fields")
                                    .num_columns(2).spacing(egui::vec2(10.0, 5.0)).show(ui, |ui| {
                                    ui.label("Opérateur");
                                    ui.add(egui::TextEdit::singleline(&mut dialog.preferences.operator_name)
                                        .hint_text("Nom de l’opérateur").char_limit(160).desired_width(f32::INFINITY));
                                    ui.end_row();
                                    ui.label("Référence pièce");
                                    ui.add(egui::TextEdit::singleline(&mut dialog.preferences.part_reference)
                                        .hint_text("Référence").char_limit(160).desired_width(f32::INFINITY));
                                    ui.end_row();
                                    ui.label("Révision du plan");
                                    ui.add(egui::TextEdit::singleline(&mut dialog.preferences.drawing_revision)
                                        .hint_text("Révision").char_limit(160).desired_width(f32::INFINITY));
                                    ui.end_row();
                                    ui.label("Lot / série");
                                    ui.add(egui::TextEdit::singleline(&mut dialog.preferences.lot_number)
                                        .hint_text("Numéro de lot").char_limit(160).desired_width(f32::INFINITY));
                                    ui.end_row();
                                    ui.label("Décision");
                                    egui::ComboBox::from_id_salt("folder-export-approval-status")
                                        .selected_text(&dialog.preferences.approval_status)
                                        .show_ui(ui, |ui| {
                                            for status in ["À renseigner", "Approuvé", "Refusé", "À confirmer"] {
                                                ui.selectable_value(&mut dialog.preferences.approval_status,
                                                    status.to_owned(), status);
                                            }
                                        });
                                    ui.end_row();
                                    ui.label("Validation par");
                                    ui.add(egui::TextEdit::singleline(&mut dialog.preferences.approved_by)
                                        .hint_text("Nom du valideur").char_limit(160).desired_width(f32::INFINITY));
                                    ui.end_row();
                                });
                                ui.label("Notes d’inspection");
                                ui.add(egui::TextEdit::multiline(&mut dialog.preferences.audit_notes)
                                    .hint_text("Contexte, anomalies, conditions ou remarques")
                                    .char_limit(4000).desired_rows(3).desired_width(f32::INFINITY));
                                ui.weak("Le PDF inclut aussi l’appareil, le profil d’étalonnage, U et l’historique des corrections enregistrées dans le projet.");
                            }
                        });

                    ui.add_space(8.0);
                    egui::Frame::new()
                        .fill(Color32::from_rgb(248, 250, 253))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(211, 222, 236)))
                        .corner_radius(10)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.colored_label(Color32::from_rgb(44, 89, 145),
                                egui::RichText::new("IMAGES").size(12.5).strong());
                            egui::Grid::new("folder-export-picture-options")
                                .num_columns(2).spacing(egui::vec2(34.0, 4.0)).show(ui, |ui| {
                                ui.checkbox(&mut dialog.preferences.include_sequence_numbers, "Inclure les numéros de séquence");
                                ui.checkbox(&mut dialog.preferences.include_file_names, "Inclure les noms de fichiers");
                                ui.end_row();
                                ui.checkbox(&mut dialog.preferences.include_descriptions, "Inclure les descriptions");
                                ui.checkbox(&mut dialog.preferences.include_measurements, "Inclure les mesures et annotations vectorielles");
                                ui.end_row();
                            });
                        });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_rgb(26, 143, 153), "●");
                        ui.weak("DPER_Measure et le pied de page sont inclus automatiquement.");
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_rgb(35, 49, 70),
                            egui::RichText::new("Sélectionner les images").strong().size(17.0));
                        ui.add_space(8.0);
                        if ui.add(egui::Button::new("Sélectionner toutes les images mesurées")
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(183, 199, 218)))
                            .corner_radius(7)).clicked() { select_all = true; }
                        if ui.add(egui::Button::new("Tout désélectionner")
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(183, 199, 218)))
                            .corner_radius(7)).clicked() { select_none = true; }
                    });
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        if dialog.rows.is_empty() {
                            ui.weak("Aucune image PNG ou JPEG dans ce dossier.");
                        }
                        for row in &mut dialog.rows {
                            let row_fill = if row.selected {
                                Color32::from_rgb(241, 249, 250)
                            } else {
                                Color32::WHITE
                            };
                            let row_stroke = if row.selected {
                                Stroke::new(1.0, Color32::from_rgb(164, 211, 216))
                            } else {
                                Stroke::new(1.0, Color32::from_rgb(217, 226, 237))
                            };
                            egui::Frame::new()
                                .fill(row_fill)
                                .stroke(row_stroke)
                                .corner_radius(9)
                                .inner_margin(9.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add_enabled_ui(row.error.is_none() && !row.session.measurements.is_empty(), |ui| {
                                            ui.checkbox(&mut row.selected, "");
                                        });
                                        if let Some(texture) = &row.thumbnail {
                                            egui::Frame::new()
                                                .fill(Color32::WHITE)
                                                .stroke(Stroke::new(1.0, Color32::from_rgb(211, 222, 236)))
                                                .corner_radius(5)
                                                .inner_margin(2.0)
                                                .show(ui, |ui| {
                                                    ui.add(egui::Image::new((texture.id(), egui::vec2(112.0, 72.0))));
                                                });
                                        } else {
                                            ui.allocate_exact_size(egui::vec2(116.0, 76.0), Sense::hover());
                                        }
                                        ui.vertical(|ui| {
                                            ui.label(egui::RichText::new(&row.name).size(14.0).strong());
                                            if let Some(error) = &row.error {
                                                ui.colored_label(Color32::RED, format!("Image illisible : {error}"));
                                            } else {
                                                if let Some(warning) = &row.warning { ui.weak(warning); }
                                                if row.session.measurements.is_empty() {
                                                    ui.weak("Exclue du rapport : aucune mesure associée.");
                                                }
                                                if include_descriptions {
                                                    ui.add(egui::TextEdit::singleline(&mut row.description)
                                                        .hint_text("Description de l'image")
                                                        .desired_width(500.0).margin(egui::vec2(8.0, 5.0)));
                                                } else if row.description.trim().is_empty() {
                                                    ui.weak("Aucune description");
                                                }
                                            }
                                        });
                                        if row.error.is_none() {
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.vertical(|ui| {
                                                    ui.checkbox(&mut row.include_timestamp, "Horodatage");
                                                    if let Some(timestamp) = &row.timestamp { ui.weak(timestamp); }
                                                });
                                            });
                                        }
                                    });
                                });
                            ui.add_space(4.0);
                        }
                    });
                    egui::Frame::new()
                        .fill(Color32::from_rgb(242, 247, 251))
                        .corner_radius(7)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.colored_label(Color32::from_rgb(81, 105, 132),
                                format!("{selected_count} image(s) mesurée(s) sélectionnée(s)"));
                        });
                    ui.separator();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("Exporter PDF").strong().color(Color32::WHITE))
                            .fill(Color32::from_rgb(25, 143, 153))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(20, 125, 137)))
                            .corner_radius(8)).clicked() { export_format = Some(FolderExportFormat::Pdf); }
                        if ui.add(egui::Button::new(egui::RichText::new("Exporter PowerPoint").strong()
                            .color(Color32::from_rgb(39, 87, 145)))
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(157, 183, 212)))
                            .corner_radius(8)).clicked() { export_format = Some(FolderExportFormat::PowerPoint); }
                        if ui.add(egui::Button::new(egui::RichText::new("CSV mesures du lot")
                            .strong().color(Color32::from_rgb(39, 87, 145)))
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(157, 183, 212)))
                            .corner_radius(8)).clicked() { export_format = Some(FolderExportFormat::BatchCsv); }
                        if ui.add(egui::Button::new(egui::RichText::new("XLSX mesures du lot")
                            .strong().color(Color32::from_rgb(39, 87, 145)))
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(157, 183, 212)))
                            .corner_radius(8)).clicked() { export_format = Some(FolderExportFormat::BatchXlsx); }
                        if ui.add(egui::Button::new(egui::RichText::new("Annuler")
                            .color(Color32::from_rgb(76, 94, 116)))
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(217, 226, 237)))
                            .corner_radius(8)).clicked() { cancel = true; }
                    });
                });
            });
        let visible = !modal_response.should_close();

        if cancel || !visible {
            self.export_dialog = None;
            return;
        }
        if let Some(name) = apply_template {
            dialog.preferences.apply_template(&name);
        }
        if save_template {
            self.message = match dialog.save_template() {
                Ok(()) => format!("Modèle de rapport « {} » enregistré.", dialog.preferences.selected_template),
                Err(error) => error,
            };
        }
        if delete_template {
            self.message = match dialog.delete_selected_template() {
                Ok(true) => "Modèle de rapport supprimé.".into(),
                Ok(false) => "Aucun modèle sélectionné.".into(),
                Err(error) => error,
            };
        }
        if select_all { dialog.select_all(true); }
        if select_none { dialog.select_all(false); }
        if choose_folder {
            let current = self.current_export_image();
            if let Some(dialog) = self.export_dialog.as_mut() {
                dialog.choose_folder(ctx, current);
                if dialog.preferences.title.trim().is_empty() {
                    let folder = dialog.folder.file_name().and_then(|name| name.to_str()).unwrap_or("DPER_Measure");
                    dialog.preferences.title = format!("{folder} — rapport DPER_Measure");
                }
            }
        }
        if refresh_folder {
            let current = self.current_export_image();
            if let Some(dialog) = self.export_dialog.as_mut() {
                dialog.refresh(ctx, current);
                if dialog.preferences.title.trim().is_empty() {
                    let folder = dialog.folder.file_name().and_then(|name| name.to_str()).unwrap_or("DPER_Measure");
                    dialog.preferences.title = format!("{folder} — rapport DPER_Measure");
                }
            }
        }
        if let Some(format) = export_format { self.export_folder_report(format); }
    }

    fn save_project(&mut self) {
        if !self.pending.is_empty() {
            self.message = "Terminez ou annulez le tracé en cours avant d'enregistrer le projet.".into();
            return;
        }
        let Some(mut project_path) = rfd::FileDialog::new()
            .set_file_name("inspection.dperproj")
            .add_filter("Projet DPER_MEASURE", &["dperproj"])
            .save_file() else { return; };
        if project_path.extension().and_then(|ext| ext.to_str())
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("dperproj"))
        {
            project_path.set_extension("dperproj");
        }
        match self.save_project_to_path(&project_path, true) {
            Ok(path) => self.message = format!("Projet enregistré avec sa capture source : {path}"),
            Err(error) => self.message = format!("Erreur d'enregistrement du projet : {error}"),
        }
    }

    fn save_project_to_path(&mut self, requested_path: &Path, overwrite: bool) -> Result<String, String> {
        if self.capture_timer.is_some() || self.timelapse_capture.is_some() || self.video_recording.is_some() {
            return Err("Attendez la fin de la capture différée/time-lapse avant d'enregistrer le projet.".into());
        }
        if !self.pending.is_empty() || self.point_edit.is_some() || self.calibration_mode {
            return Err("Terminez ou annulez l'action en cours avant d'enregistrer le projet.".into());
        }
        if self.running && !self.paused { self.freeze_capture(); }
        let image = self.image.as_ref().ok_or_else(|| "Aucune image à enregistrer dans le projet.".to_string())?;
        let mut project_path = requested_path.to_path_buf();
        if project_path.extension().and_then(|ext| ext.to_str()).is_none_or(|ext| !ext.eq_ignore_ascii_case("dperproj")) {
            project_path.set_extension("dperproj");
        }
        if project_path.exists() && !overwrite {
            return Err(format!("Le projet existe déjà : {}. Passez overwrite=true pour le remplacer.", project_path.display()));
        }
        let parent = project_path.parent().filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| format!("Dossier de projet inaccessible : {error}"))?;
        let stem = project_path.file_stem().and_then(|name| name.to_str()).unwrap_or("inspection");
        let mut source_name = format!("{stem}.{}.png", self.capture_id);
        let mut source_path = parent.join(&source_name);
        while source_path.exists() {
            source_name = format!("{stem}.{}.png", new_capture_id());
            source_path = parent.join(&source_name);
        }
        image::save_buffer_with_format(&source_path, &image.rgb, image.width, image.height,
            image::ColorType::Rgb8, image::ImageFormat::Png)
            .map_err(|error| format!("Impossible d'enregistrer la capture du projet : {error}"))?;
        let mut live_reference = self.reference_document.clone();
        let mut saved_reference = self.reference_document.clone();
        if let Some(document) = self.reference_document.as_ref() {
            let reference_path = PathBuf::from(&document.source_file);
            if reference_path.is_file() {
                let project_parent = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
                let source_parent = reference_path.parent().and_then(|path| fs::canonicalize(path).ok());
                let project_reference = if source_parent.as_ref() == Some(&project_parent) {
                    reference_path.clone()
                } else {
                    copy_reference_into_project(&reference_path, parent, stem)?
                };
                if let Some(saved) = saved_reference.as_mut() {
                    saved.source_file = project_reference.file_name().and_then(|name| name.to_str())
                        .unwrap_or(&document.source_name).to_owned();
                }
                if let Some(live) = live_reference.as_mut() {
                    live.source_file = project_reference.to_string_lossy().into_owned();
                }
            }
        }
        let session = Session {
            automated_inspection: Some(self.inspection.clone()),
            schema_version: storage::CURRENT_SCHEMA_VERSION,
            capture_id: self.capture_id.clone(), image_file: source_name,
            device: self.active_device.clone(), optical_setup: self.optical_setup.clone(),
            device_id: self.active_device_id.clone(),
            calibration: self.calibration.clone(), calibrations: self.calibration_history.clone(),
            measurements: self.measurements.clone(),
            annotations: self.annotations.clone(),
            measurement_audit_history: self.measurement_audit_history.clone(),
            quality_thresholds: self.quality_thresholds.clone(),
            quality_assessment: self.quality_assessment.clone(),
            display_correction: self.display_correction,
            protocol: self.active_protocol.clone(),
            reference_document: saved_reference,
        };
        let bytes = serde_json::to_vec_pretty(&session).map_err(|error| error.to_string())?;
        let write_result = if overwrite {
            fs::write(&project_path, bytes)
        } else {
            fs::OpenOptions::new().write(true).create_new(true).open(&project_path)
                .and_then(|mut file| std::io::Write::write_all(&mut file, &bytes))
        };
        write_result.map_err(|error| format!("Écriture du projet impossible : {error}"))?;
        if let Some(active_image) = &mut self.image {
            active_image.source = source_path.to_string_lossy().into_owned();
        }
        self.reference_document = live_reference;
        self.session_saved = true;
        self.message = format!("Projet enregistré avec sa capture source : {}", project_path.display());
        Ok(project_path.to_string_lossy().into_owned())
    }

    fn open_project(&mut self, ctx: &egui::Context) {
        let Some(project_path) = rfd::FileDialog::new()
            .add_filter("Projet DPER_MEASURE ou export JSON", &["dperproj", "json"])
            .pick_file() else { return; };
        if let Err(error) = self.open_project_path(ctx, &project_path) { self.message = error; }
    }

    fn start_project_search(&mut self) {
        self.project_search = Some(ProjectSearchWindow {
            root: self.directory.clone(), query: String::new(), results: Vec::new(),
            note: "Choisissez un dossier racine puis lancez la recherche.".into(),
        });
        self.show_project_search = true;
    }

    fn project_search_window(&mut self, ctx: &egui::Context) {
        if !self.show_project_search { return; }
        let Some(window) = self.project_search.as_mut() else { return; };
        let mut choose_root = false;
        let mut search = false;
        let mut open = None::<PathBuf>;
        let mut root_text = window.root.to_string_lossy().into_owned();
        egui::Window::new("Rechercher un projet")
            .open(&mut self.show_project_search)
            .default_width(720.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Dossier racine");
                    ui.add(egui::TextEdit::singleline(&mut root_text)
                        .desired_width(470.0).interactive(false));
                    if ui.button("Parcourir…").clicked() { choose_root = true; }
                });
                ui.horizontal(|ui| {
                    ui.label("Nom, capture, caméra, optique ou mesure");
                    ui.add(egui::TextEdit::singleline(&mut window.query)
                        .hint_text("Texte à rechercher").desired_width(290.0));
                    if ui.button("Rechercher les projets").clicked() { search = true; }
                });
                ui.weak(&window.note);
                egui::ScrollArea::vertical().max_height(430.0).show(ui, |ui| {
                    if window.results.is_empty() {
                        ui.weak("Aucun résultat affiché.");
                    }
                    for result in &window.results {
                        ui.horizontal(|ui| {
                            if ui.button("Ouvrir").clicked() { open = Some(result.path.clone()); }
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(result.path.file_name()
                                    .and_then(|name| name.to_str()).unwrap_or("projet.dperproj")).strong());
                                ui.weak(&result.summary);
                                ui.small(result.path.to_string_lossy().to_string());
                            });
                        });
                        ui.separator();
                    }
                });
            });
        if choose_root {
            if let Some(root) = rfd::FileDialog::new().set_directory(&window.root).pick_folder() {
                window.root = root;
                window.results.clear();
                window.note = "Lancez la recherche dans ce dossier et ses sous-dossiers.".into();
            }
        }
        if search {
            match scan_project_search(&window.root, &window.query) {
                Ok((results, note)) => { window.results = results; window.note = note; }
                Err(error) => { window.results.clear(); window.note = error; }
            }
        }
        if let Some(path) = open {
            match self.open_project_path(ctx, &path) {
                Ok(()) => self.show_project_search = false,
                Err(error) => self.message = error,
            }
        }
    }

    fn open_project_path(&mut self, ctx: &egui::Context, project_path: &Path) -> Result<(), String> {
        if self.capture_timer.is_some() || self.timelapse_capture.is_some() || self.video_recording.is_some() {
            return Err("Attendez ou arrêtez la capture différée/time-lapse avant d'ouvrir un projet.".into());
        }
        if !self.session_saved || !self.pending.is_empty() || self.point_edit.is_some() || self.calibration_mode {
            return Err("Enregistrez le contexte et terminez ou annulez l'action en cours avant d'ouvrir un projet.".into());
        }
        let bytes = fs::read(project_path).map_err(|error| format!("Lecture du projet impossible : {error}"))?;
        let mut session: Session = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Projet JSON illisible : {error}"))?;
        let imported_dependencies = crate::measurement_graph::DependencyGraph::restore_import(&session.measurements)?;
        if session.schema_version > storage::CURRENT_SCHEMA_VERSION {
            return Err(format!("Version de projet non prise en charge : {}", session.schema_version));
        }
        if quality::assess_image_quality(&[0; 27], 3, 3, &session.quality_thresholds).is_err()
            || quality::corrected_display_rgb(&[], session.display_correction).is_err()
        {
            return Err("Le projet contient des seuils ou corrections d'image invalides.".into());
        }
        if session.protocol.as_ref().is_some_and(|protocol| {
            protocol.id.trim().is_empty() || protocol.version == 0
                || protocol.resolution.contains(&0)
                || protocol.mm_per_pixel.is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
                || protocol.scale_x_mm_per_pixel.is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
                || protocol.scale_y_mm_per_pixel.is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
                || protocol.expanded_uncertainty_mm
                    .is_some_and(|uncertainty| !uncertainty.is_finite() || uncertainty <= 0.0)
                || protocol.expected_measurements.iter().any(|characteristic| {
                    characteristic.label.trim().is_empty() || characteristic.kind.trim().is_empty()
                        || characteristic.reference.as_ref().is_some_and(|dimension| {
                            !dimension.nominal_mm.is_finite() || dimension.nominal_mm <= 0.0
                                || !dimension.tolerance_minus_mm.is_finite()
                                || dimension.tolerance_minus_mm < 0.0
                                || !dimension.tolerance_plus_mm.is_finite()
                                || dimension.tolerance_plus_mm < 0.0
                        })
                })
        }) {
            return Err("Le protocole versionné du projet contient des paramètres invalides.".into());
        }
        let mut loaded_reference = session.reference_document.clone();
        if let Some(document) = loaded_reference.as_mut() {
            let resolved = reference::resolve_source(&document.source_file,
                project_path.parent().unwrap_or_else(|| Path::new(".")));
            document.source_file = resolved.to_string_lossy().into_owned();
        }
        let stored_image = PathBuf::from(&session.image_file);
        let image_path = if stored_image.is_absolute() { stored_image }
            else { project_path.parent().unwrap_or_else(|| Path::new(".")).join(stored_image) };
        let source_image = image::open(&image_path).map_err(|error| format!(
            "Image source absente ou illisible ({}): {error}", image_path.display()))?.to_rgb8();
        let (width, height) = source_image.dimensions();
        let valid_profile = |profile: &Calibration| {
            profile.resolution == [width, height]
                && profile.mm_per_pixel.is_finite() && profile.mm_per_pixel > 0.0
                && profile.scale_x_mm_per_pixel.is_none_or(|scale| scale.is_finite() && scale > 0.0)
                && profile.scale_y_mm_per_pixel.is_none_or(|scale| scale.is_finite() && scale > 0.0)
                && profile.reference_mm.is_finite() && profile.reference_mm > 0.0
                && profile.reference_px.is_finite() && profile.reference_px > 0.0
                && profile.expanded_uncertainty_mm
                    .is_none_or(|uncertainty| uncertainty.is_finite() && uncertainty > 0.0)
                && profile.uncertainty_budget.as_ref().is_none_or(|budget|
                    budget.expanded_uncertainty_mm().is_ok_and(|budget_u|
                        profile.expanded_uncertainty_mm.is_some_and(|stored_u|
                            (stored_u - budget_u).abs() <= 1.0e-10 * stored_u.abs().max(1.0))))
                && profile.verification_residual_mm.is_none_or(f64::is_finite)
                && ((profile.corner_verification_residuals_mm.is_empty()
                    && profile.additional_corner_validation_segments_native_px.is_empty())
                    || (profile.corner_verification_residuals_mm.len() == 4
                        && profile.additional_corner_validation_segments_native_px.len() == 3))
                && profile.corner_verification_residuals_mm.iter().all(|residual| residual.is_finite())
                && profile.validation_points_native_px.iter().all(|point| {
                    point.x.is_finite() && point.y.is_finite()
                        && point.x >= 0.0 && point.y >= 0.0
                        && point.x <= width as f64 && point.y <= height as f64
                })
                && profile.additional_corner_validation_segments_native_px.iter().flatten()
                    .all(|point| point.x.is_finite() && point.y.is_finite()
                        && point.x >= 0.0 && point.y >= 0.0
                        && point.x <= width as f64 && point.y <= height as f64)
        };
        if session.calibration.as_ref().is_some_and(|profile| !valid_profile(profile))
            || session.calibrations.iter().any(|profile| !valid_profile(profile)) {
            return Err("Le profil de calibration du projet est incompatible avec l'image source.".into());
        }
        if session.measurements.iter().any(|measurement| {
            measurement.mm_per_pixel.is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
                || measurement.mm_per_pixel_x.is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
                || measurement.mm_per_pixel_y.is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
                || measurement.expanded_uncertainty_mm
                    .is_some_and(|uncertainty| !uncertainty.is_finite() || uncertainty <= 0.0)
                || measurement.protocol_id.is_some() != measurement.protocol_version.is_some()
                || measurement.protocol_id.as_ref().is_some_and(|id| id.trim().is_empty())
                || measurement.points.iter().any(|point| {
                    !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0
                        || point.x > width as f64 || point.y > height as f64
                })
                || measurement.label_position_native_px.is_some_and(|point| {
                    !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0
                        || point.x > width as f64 || point.y > height as f64
                })
                || !geometry::is_nondegenerate(measurement.kind, &measurement.points)
        }) {
            return Err("Le projet contient des points de mesure hors de l'image, invalides ou dégénérés.".into());
        }
        let valid_annotation_point = |point: &Point| {
            point.x.is_finite() && point.y.is_finite()
                && point.x >= 0.0 && point.y >= 0.0
                && point.x <= width as f64 && point.y <= height as f64
        };
        if session.annotations.iter().any(|annotation| annotation.text.trim().is_empty()
            || annotation.text.chars().count() > 2000
            || match &annotation.target {
            AnnotationTarget::Measurement { measurement_id } =>
                !session.measurements.iter().any(|measurement| measurement.id == *measurement_id),
            AnnotationTarget::Point { point } => !valid_annotation_point(point),
            AnnotationTarget::Zone { first, second } =>
                !valid_annotation_point(first) || !valid_annotation_point(second),
        }) {
            return Err("Le projet contient une annotation dont la cible sort de l'image.".into());
        }
        if let Some(profile) = session.calibration.as_mut() {
            if profile.id.is_empty() { profile.id = new_id("calibration"); }
        }
        for profile in &mut session.calibrations {
            if profile.id.is_empty() { profile.id = new_id("calibration"); }
        }
        if let Some(profile) = &session.calibration {
            if !session.calibrations.iter().any(|saved| saved.id == profile.id) {
                session.calibrations.push(profile.clone());
            }
        }
        let migrate_legacy_calibration = session.schema_version < 2;
        let migrate_legacy_protocol = session.schema_version < 8;
        if session.capture_id.is_empty() { session.capture_id = new_capture_id(); }
        for measurement in &mut session.measurements {
            if measurement.capture_id.is_empty() { measurement.capture_id = session.capture_id.clone(); }
            if measurement.capture_id != session.capture_id {
                return Err("Le projet mélange plusieurs captures dans une même image.".into());
            }
            if migrate_legacy_calibration && measurement.mm_per_pixel.is_none() {
                measurement.mm_per_pixel = session.calibration.as_ref().map(|profile| profile.mm_per_pixel);
            }
            if migrate_legacy_calibration && measurement.calibration_id.as_deref().unwrap_or("").is_empty() {
                measurement.calibration_id = session.calibration.as_ref().map(|profile| profile.id.clone());
            }
            if migrate_legacy_protocol {
                if let Some(protocol) = session.protocol.as_ref() {
                    measurement.protocol_id = Some(protocol.id.clone());
                    measurement.protocol_version = Some(protocol.version);
                }
            }
        }
        if session.schema_version >= 3 && session.measurements.iter().any(|measurement| {
            measurement.calibration_id.as_ref().is_some_and(|id| {
                !session.calibrations.iter().any(|profile| &profile.id == id)
            })
        }) {
            return Err("Une mesure référence un profil de calibration absent de l'historique du projet.".into());
        }
        if session.measurements.iter().any(|measurement| measurement.calibration_id.as_ref().is_some_and(|id|
            session.calibrations.iter().any(|profile| &profile.id == id && profile.validation_status == "failed")))
        {
            return Err("Une mesure utilise un profil dont le contrôle indépendant a échoué.".into());
        }
        self.running = false;
        self.camera_task = None;
        self.paused = true;
        self.image_is_imported = true;
        self.calibration = None;
        self.mm_per_pixel = None;
        self.active_device = if session.device.is_empty() {
            session.calibration.as_ref()
                .map(|c| c.device.clone()).unwrap_or_else(|| "Projet DPER_MEASURE".into())
        } else { session.device.clone() };
        self.active_device_id = if session.device_id.is_empty() {
            session.calibration.as_ref().map(|profile| profile.device_id.clone())
                .filter(|device_id| !device_id.is_empty()).unwrap_or_else(|| "imported-image".into())
        } else { session.device_id.clone() };
        if !session.optical_setup.is_empty() {
            self.optical_setup = session.optical_setup.clone();
        } else if let Some(profile) = &session.calibration {
            self.optical_setup = profile.optical_setup.clone();
        }
        let image = ImageData {
            width, height, rgb: source_image.into_raw(),
            source: image_path.to_string_lossy().into_owned(),
        };
        self.quality_thresholds = session.quality_thresholds.clone();
        self.display_correction = session.display_correction;
        self.active_protocol = session.protocol.clone();
        if let Some(protocol) = &self.active_protocol {
            self.protocol_name_draft = protocol.name.clone();
            self.protocol_family_draft = protocol.part_family.clone();
        }
        self.set_image(ctx,image,true);
        self.capture_id = session.capture_id;
        self.calibration = session.calibration.filter(|profile| profile.validation_status != "failed");
        self.calibration_history = session.calibrations;
        self.mm_per_pixel = self.calibration.as_ref().map(|c| c.mm_per_pixel);
        self.expanded_uncertainty_mm = self.calibration.as_ref()
            .and_then(|profile| profile.expanded_uncertainty_mm)
            .map(|value| value.to_string()).unwrap_or_default();
        self.measurements = session.measurements;
        self.annotations = session.annotations;
        self.measurement_audit_history = session.measurement_audit_history;
        self.reference_document = loaded_reference;
        self.inspection = session.automated_inspection.unwrap_or_default();
        self.inspection.dependencies = imported_dependencies;
        self.refresh_dependency_flags();
        self.drawing_geometry_cache.clear();
        self.next_id = self.measurements.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        self.selected_measurement = None;
        self.undo_history.clear(); self.redo_history.clear(); self.point_edit = None;
        self.selection_mode = false;
        self.calibration_mode = false;
        self.clear_pending();
        self.session_saved = true;
        self.message = format!("Projet rouvert : {} mesure(s), capture {}",self.measurements.len(),self.capture_id);
        Ok(())
    }

    fn point_to_screen(p: Point, rect: Rect, image_w: f32, image_h: f32) -> Pos2 {
        Pos2::new(rect.min.x + p.x as f32 / image_w * rect.width(),
                  rect.min.y + p.y as f32 / image_h * rect.height())
    }

    fn point_from_screen(pointer: Pos2, rect: Rect, image_w: f32, image_h: f32) -> Point {
        Point {
            x: ((pointer.x - rect.left()) / rect.width() * image_w) as f64,
            y: ((pointer.y - rect.top()) / rect.height() * image_h) as f64,
        }
    }

    fn image_is_editable(&self) -> bool {
        self.image.is_some() && (!self.running || self.paused)
    }





















    fn measurement_label_at(&self, pointer: Pos2, rect: Rect, image_w: f32, image_h: f32,
                            painter: &egui::Painter) -> Option<u64> {
        self.measurements.iter().rev().find(|measurement| {
            self.measurement_label_card_rect(painter, rect, image_w, image_h, measurement)
                .is_some_and(|card_rect| card_rect.contains(pointer))
        }).map(|measurement| measurement.id)
    }

    fn select_measurement_at(&mut self, pointer: Pos2, rect: Rect, image_w: f32, image_h: f32,
                             painter: &egui::Painter) {
        let label_target = self.measurement_label_at(pointer, rect, image_w, image_h, painter);
        if label_target.is_some() {
            self.selected_measurement = label_target;
            return;
        }
        let mut nearest: Option<(u64,f32)> = None;
        for measurement in &self.measurements {
            if let Some((_, distance)) = Self::nearest_edit_handle(measurement, pointer, rect, image_w, image_h) {
                if nearest.is_none_or(|(_,best)| distance < best) {
                    nearest = Some((measurement.id,distance));
                }
            }
        }
        self.selected_measurement = nearest.map(|(id,_)|id);
    }





    fn paint_angle_arc(painter: &egui::Painter, a: Pos2, vertex: Pos2, b: Pos2, color: Color32) {
        let va = a - vertex;
        let vb = b - vertex;
        if va.length_sq() <= 1.0 || vb.length_sq() <= 1.0 { return; }
        let start = va.y.atan2(va.x);
        let end = vb.y.atan2(vb.x);
        let delta = (end - start + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let radius = 20.0_f32.min(va.length() * 0.35).min(vb.length() * 0.35).max(8.0);
        let arc: Vec<Pos2> = (0..=24).map(|step| {
            let angle = start + delta * step as f32 / 24.0;
            vertex + Vec2::new(angle.cos(), angle.sin()) * radius
        }).collect();
        for pair in arc.windows(2) {
            Self::paint_measurement_segment(painter, pair[0], pair[1], color, 1.8);
        }
    }







    fn render_annotation_targets(&self, painter: &egui::Painter, image_rect: Rect, w: f32, h: f32) {
        let mut occupied = Vec::<Rect>::new();
        for measurement in &self.measurements {
            if let Some(label_rect) = self.measurement_label_card_rect(painter, image_rect, w, h, measurement) {
                occupied.push(label_rect.expand(3.0));
            }
            let source_points = geometry::sampled_curve(measurement.kind, &measurement.points, 96)
                .unwrap_or_else(|| measurement.points.clone());
            let points = source_points.iter()
                .map(|point| Self::point_to_screen(*point, image_rect, w, h))
                .collect::<Vec<_>>();
            for segment in points.windows(2) {
                occupied.push(Rect::from_two_pos(segment[0], segment[1]).expand(5.0));
            }
        }

        for (index, annotation) in self.annotations.iter().enumerate() {
            let anchor = match &annotation.target {
                AnnotationTarget::Measurement { measurement_id } => self.measurements.iter()
                    .find(|measurement| measurement.id == *measurement_id)
                    .and_then(|measurement| {
                        if measurement.points.is_empty() { return None; }
                        let count = measurement.points.len() as f64;
                        Some(Point {
                            x: measurement.points.iter().map(|point| point.x).sum::<f64>() / count,
                            y: measurement.points.iter().map(|point| point.y).sum::<f64>() / count,
                        })
                    }),
                AnnotationTarget::Point { point } => Some(*point),
                AnnotationTarget::Zone { first, second } => Some(Point {
                    x: (first.x + second.x) * 0.5,
                    y: (first.y + second.y) * 0.5,
                }),
            };
            let Some(anchor) = anchor else { continue; };
            let target = Self::point_to_screen(anchor, image_rect, w, h);
            if !image_rect.contains(target) { continue; }
            let color = annotation_color(index);

            let badge_size = Vec2::new(30.0, 20.0);
            let mut badge = None;
            'rings: for radius in [16.0_f32, 26.0, 38.0, 52.0, 68.0, 88.0] {
                for direction in 0..16 {
                    let angle = direction as f32 * std::f32::consts::TAU / 16.0;
                    let center = target + Vec2::new(angle.cos(), angle.sin()) * radius;
                    let candidate = Rect::from_center_size(center, badge_size);
                    if candidate.left() < image_rect.left() || candidate.right() > image_rect.right()
                        || candidate.top() < image_rect.top() || candidate.bottom() > image_rect.bottom()
                        || occupied.iter().any(|rect| rect.intersects(candidate))
                    {
                        continue;
                    }
                    badge = Some(candidate);
                    break 'rings;
                }
            }

            if let Some(badge_rect) = badge {
                let badge_center = badge_rect.center();
                painter.line_segment([target, badge_center], Stroke::new(3.2,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 225)));
                painter.line_segment([target, badge_center], Stroke::new(1.25, color));
                painter.circle_filled(target, 4.0, Color32::WHITE);
                painter.circle_stroke(target, 4.0, Stroke::new(1.5, color));
                painter.rect_filled(badge_rect, 4.0, color);
                painter.rect_stroke(badge_rect, 4.0, Stroke::new(1.0, Color32::WHITE),
                    egui::StrokeKind::Outside);
                painter.text(badge_center, Align2::CENTER_CENTER, format!("A{}", index + 1),
                    FontId::proportional(9.0), Color32::WHITE);
                occupied.push(badge_rect.expand(3.0));
            } else {
                // The note remains readable in the side panel when the image has no free label space.
                painter.circle_stroke(target, 5.0, Stroke::new(1.5, color));
            }
        }
    }





    fn paint_alignment_analysis(
        painter: &egui::Painter,
        image_rect: Rect,
        assessment: alignment::AlignmentAssessment,
    ) {
        let [left, top, right, bottom] = assessment.analysis_zone;
        let map_point = |x: f32, y: f32| Pos2::new(
            image_rect.left() + x * image_rect.width(),
            image_rect.top() + y * image_rect.height(),
        );
        let zone_color = Color32::from_rgba_unmultiplied(245, 205, 70, 200);
        let zone_stroke = Stroke::new(1.4, zone_color);
        for (start, end) in [
            (map_point(left, top), map_point(right, top)),
            (map_point(right, top), map_point(right, bottom)),
            (map_point(right, bottom), map_point(left, bottom)),
            (map_point(left, bottom), map_point(left, top)),
        ] {
            painter.line_segment([start, end], zone_stroke);
        }
        if let Some(contour) = assessment.contour {
            let start = map_point(contour.start[0], contour.start[1]);
            let end = map_point(contour.end[0], contour.end[1]);
            let color = match assessment.state {
                alignment::AlignmentState::Aligned => Color32::from_rgb(65, 235, 125),
                alignment::AlignmentState::Misaligned => Color32::from_rgb(255, 85, 82),
                alignment::AlignmentState::Unknown => Color32::from_rgb(245, 190, 65),
            };
            painter.line_segment(
                [start, end],
                Stroke::new(5.0, Color32::from_rgba_unmultiplied(5, 12, 20, 200)),
            );
            painter.line_segment([start, end], Stroke::new(2.4, color));
            painter.circle_filled(start, 3.5, color);
            painter.circle_filled(end, 3.5, color);
        }
    }

    fn central_image(&mut self, ui: &mut egui::Ui) {
        let correction_before = self.display_correction;
        let mut correction_changed=false;
        egui::Frame::new().fill(Color32::from_rgb(15,31,48))
            .stroke(Stroke::new(1.0,Color32::from_rgb(35,57,77)))
            .corner_radius(UiTokens::CARD_RADIUS)
            .inner_margin(egui::Margin::symmetric(10,6)).show(ui, |ui| {
                if self.registration_photo_pick_owns_frame() { ui.disable(); }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x=6.0;
                    if canvas_icon_button(ui,"",false,UiIcon::ZoomOut).on_hover_text("Zoom arrière").clicked() {
                        self.fit_view=false; self.zoom=(self.zoom/1.2).clamp(0.05,32.0);
                    }
                    ui.label(egui::RichText::new("Zoom").size(12.0).color(Color32::from_rgb(211,225,239)));
                    let zoom=canvas_toolbar_button(ui,&format!("{:.0} %",self.zoom_percent),false);
                    egui::Popup::menu(&zoom).show(|ui| {
                        for percent in [25.0,50.0,100.0,200.0,400.0] {
                            if ui.button(format!("{percent:.0} %")).clicked() {
                                self.fit_view=false; self.zoom=percent/100.0; ui.close();
                            }
                        }
                        if ui.button("Ajuster à l’image").clicked() {
                            self.fit_view=true; self.pan=Vec2::ZERO; ui.close();
                        }
                    });
                    if canvas_icon_button(ui,"",false,UiIcon::ZoomIn).on_hover_text("Zoom avant").clicked() {
                        self.fit_view=false; self.zoom=(self.zoom*1.2).clamp(0.05,32.0);
                    }
                    if canvas_icon_button(ui,"",false,UiIcon::Fit).on_hover_text("Ajuster à l’image").clicked() {
                        self.fit_view=true; self.pan=Vec2::ZERO;
                    }
                    ui.separator();
                    let can_pan=self.image.is_some() && self.pending.is_empty() && !self.calibration_mode
                        && !self.annotation_mode && !self.measurement_gesture_active();
                    if ui.add_enabled_ui(can_pan,|ui|canvas_icon_button(ui,"",self.pan_mode,UiIcon::Hand))
                        .inner.on_hover_text("Déplacer l’image avec le bouton gauche. Échap quitte ce mode.").clicked() {
                        self.pan_mode=!self.pan_mode;
                    }
                    ui.separator();
                    if canvas_icon_button(ui,"",self.show_alignment_grid,UiIcon::Grid).on_hover_text("Afficher la grille").clicked() {
                        self.show_alignment_grid=!self.show_alignment_grid;
                    }
                    if canvas_icon_button(ui,"",self.show_center_guides,UiIcon::Crosshair).on_hover_text("Afficher les axes").clicked() {
                        self.show_center_guides=!self.show_center_guides;
                    }
                    let overlays=canvas_icon_button(ui,"",false,UiIcon::Layers).on_hover_text("Options de superposition");
                    egui::Popup::menu(&overlays)
                        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
                        .show(|ui| {
                        ui.set_min_width(260.0);
                        ui.checkbox(&mut self.show_alignment_grid,"Grille d’alignement");
                        ui.checkbox(&mut self.show_center_guides,"Axes et réticule central");
                        ui.separator();
                        ui.label(egui::RichText::new("Aides de positionnement").strong());
                        ui.add(egui::Slider::new(&mut self.alignment_tolerance_degrees, 0.5..=15.0)
                            .suffix("°").text("Tolérance angulaire"));
                        if self.running && !self.paused {
                            let (status, color) = match self.live_alignment {
                                alignment::AlignmentState::Aligned => ("Aligné", Color32::from_rgb(53, 194, 67)),
                                alignment::AlignmentState::Misaligned => ("À aligner", Color32::from_rgb(220, 90, 84)),
                                alignment::AlignmentState::Unknown => ("Alignement indéterminé", Color32::from_rgb(238, 174, 67)),
                            };
                            ui.colored_label(color, status);
                        }
                        ui.separator();
                        if ui.button("Terminé").clicked() { ui.close(); }
                    });
                    let adjustments=canvas_icon_button(ui,"",false,UiIcon::Sun).on_hover_text("Réglages d’affichage");
                    egui::Popup::menu(&adjustments)
                        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
                        .show(|ui| {
                        ui.set_min_width(250.0);
                        ui.label(egui::RichText::new("Réglages d’affichage").strong());
                        correction_changed |= ui.add(egui::Slider::new(&mut self.display_correction.brightness,-100.0..=100.0).text("Luminosité")).changed();
                        correction_changed |= ui.add(egui::Slider::new(&mut self.display_correction.contrast,0.0..=4.0).text("Contraste")).changed();
                        correction_changed |= ui.add(egui::Slider::new(&mut self.display_correction.gamma,0.2..=5.0).text("Gamma")).changed();
                        if ui.button("Valeurs par défaut").clicked() {
                            self.display_correction=DisplayCorrection::default(); correction_changed=true;
                        }
                        ui.separator();
                        if ui.button("Terminé").clicked() { ui.close(); }
                    });
                    ui.separator();
                    if canvas_icon_button(ui,"",false,UiIcon::Undo).on_hover_text("Annuler").clicked() { self.undo_current_action(); }
                    if canvas_icon_button(ui,"",false,UiIcon::Redo).on_hover_text("Rétablir").clicked() { self.redo_current_action(); }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                        if canvas_icon_button(ui,"",false,UiIcon::Camera).on_hover_text("Capturer une photo PNG").clicked() { self.capture_photo(); }
                    });
                });
                if self.image.is_some() && !self.calibration_mode {
                    ui.add_space(3.0);
                    ui.horizontal_wrapped(|ui| {
                        if self.annotation_mode {
                            ui.label(egui::RichText::new("Annotation").strong().color(Color32::from_rgb(40, 213, 236)));
                            ui.label("Cliquez sur l’image pour placer le commentaire.");
                            if ui.button("Annuler").clicked() { self.escape_active_mode(); }
                        } else {
                            self.manual_tool_options(ui);
                        }
                    });
                }
            });
        if correction_changed {
            self.record_display_correction_change(correction_before, ui.input(|input| input.pointer.primary_down()));
            self.refresh_display_texture(ui.ctx()); self.assess_current_quality();
            self.session_saved=false; self.document_revision=self.document_revision.wrapping_add(1);
        }
        self.refresh_changed_display(ui.ctx());
        ui.add_space(3.0);
        let (viewport, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let paint = ui.painter_at(viewport);
        paint.rect_filled(viewport, 3.0, UiTokens::CANVAS_900);
        let (Some(img), Some(tex)) = (&self.image, &self.texture) else {
            if viewport.width() >= 440.0 && viewport.height() >= 260.0 {
                let card = Rect::from_center_size(viewport.center(), egui::vec2(420.0, 210.0));
                paint.rect_filled(card, 14.0, Color32::from_rgb(24, 38, 55));
                paint.rect_stroke(card, 14.0, Stroke::new(1.0, Color32::from_rgb(49, 72, 94)),
                    egui::StrokeKind::Inside);
                let icon_center = Pos2::new(card.center().x, card.top() + 44.0);
                let accent = Color32::from_rgb(44, 191, 215);
                paint.circle_stroke(icon_center, 15.0, Stroke::new(1.8, accent));
                paint.circle_filled(icon_center, 3.0, accent);
                paint.line_segment([icon_center - Vec2::new(23.0, 0.0), icon_center + Vec2::new(23.0, 0.0)],
                    Stroke::new(1.2, accent));
                paint.line_segment([icon_center - Vec2::new(0.0, 23.0), icon_center + Vec2::new(0.0, 23.0)],
                    Stroke::new(1.2, accent));
                paint.text(Pos2::new(card.center().x, card.top() + 86.0), Align2::CENTER_TOP,
                    "Votre espace de mesure", FontId::proportional(21.0),
                    Color32::from_rgb(234, 242, 250));
                paint.text(Pos2::new(card.center().x, card.top() + 124.0), Align2::CENTER_TOP,
                    "Ouvrez une image ou démarrez le flux USB.\nLes commandes sont accessibles dans le bandeau supérieur.",
                    FontId::proportional(14.0), Color32::from_rgb(169, 188, 207));
            } else {
                paint.text(viewport.center(), Align2::CENTER_CENTER,
                    "Ouvrez une image ou démarrez le flux USB",
                    FontId::proportional(15.0), Color32::from_rgb(190, 208, 224));
            }
            return;
        };
        let w = img.width as f32; let h = img.height as f32;
        let fit_scale = (viewport.width()/w).min(viewport.height()/h).max(f32::EPSILON);
        let pixels_per_point = ui.ctx().pixels_per_point();
        let mut scale = if self.fit_view { fit_scale } else { self.zoom / pixels_per_point };
        // Clic molette maintenu pour se déplacer (pan)
        if !self.measurement_gesture_active() && (response.dragged_by(PointerButton::Middle)
            || (self.pan_mode && response.dragged_by(PointerButton::Primary))) {
            self.pan += ui.input(|i|i.pointer.delta());
        }
        if self.pan_mode && response.hovered() {
            ui.ctx().set_cursor_icon(if response.dragged() {egui::CursorIcon::Grabbing} else {egui::CursorIcon::Grab});
        }
        if response.hovered() && !self.measurement_gesture_active() {
            let (scroll, pointer) = ui.input(|i| (i.raw_scroll_delta.y, i.pointer.latest_pos()));
            if scroll.abs() > 0.001 {
                let old_scale = scale;
                let old_center = viewport.center() + self.pan;
                let new_zoom = (old_scale * pixels_per_point * (scroll * 0.001).exp())
                    .clamp(0.05, 32.0);
                scale = new_zoom / pixels_per_point;
                if let Some(pointer) = pointer {
                    let new_center = pointer + (old_center - pointer) * (scale / old_scale);
                    self.pan = new_center - viewport.center();
                }
                self.zoom = new_zoom;
                self.fit_view = false;
            }
        }
        self.zoom_percent = if self.fit_view { fit_scale * pixels_per_point * 100.0 } else { self.zoom * 100.0 };
        let size = Vec2::new(w*scale,h*scale);
        let center = viewport.center() + self.pan;
        let image_rect = Rect::from_center_size(center,size);
        #[cfg(test)]
        ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new("measurement-canvas-viewport"), viewport));
        paint.image(tex.id(),image_rect,Rect::from_min_max(Pos2::ZERO,Pos2::new(1.0,1.0)),Color32::WHITE);
        self.edge_snap_preview = None;
        if self.edge_snap_enabled && self.mode != Tool::ObjectCount {
            let hover = response.interact_pointer_pos()
                .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                .filter(|pointer| image_rect.contains(*pointer));
            if let Some(pointer) = hover {
                let origin = Self::point_from_screen(pointer, image_rect, w, h);
                let preview = quality::analyze_edge_subpixel(
                    &img.rgb, img.width, img.height, origin, self.edge_snap_radius,
                    self.quality_thresholds.edge_gradient_threshold,
                );
                self.edge_snap_preview = preview;
                let half_x = self.edge_snap_radius as f32 * image_rect.width() / w;
                let half_y = self.edge_snap_radius as f32 * image_rect.height() / h;
                let roi = Rect::from_center_size(pointer, Vec2::new(half_x * 2.0, half_y * 2.0));
                paint.rect_stroke(roi, 0.0, Stroke::new(1.3, Color32::from_rgb(255, 203, 58)),
                    egui::StrokeKind::Inside);
                if let Some(result) = preview {
                    let snapped = Self::point_to_screen(result.point, image_rect, w, h);
                    paint.line_segment([pointer, snapped], Stroke::new(2.0, Color32::from_rgb(47, 224, 195)));
                    paint.circle_stroke(snapped, 6.0, Stroke::new(2.0, Color32::WHITE));
                    paint.circle_filled(snapped, 2.7, Color32::from_rgb(47, 224, 195));
                }
            }
        }
        let grid_step = if self.show_alignment_grid {
            Some(Self::paint_alignment_grid(&paint, viewport, image_rect, scale, w, h))
        } else {
            None
        };
        if self.running && !self.paused {
            if let Some(assessment) = self.live_alignment_assessment {
                Self::paint_alignment_analysis(&paint, image_rect, assessment);
            }
        }
        if self.show_center_guides {
            let live_alignment = (self.running && !self.paused).then_some(self.live_alignment);
            Self::paint_center_guides(&paint, viewport, live_alignment);
        }
        if self.pan_mode || !self.selection_mode || self.annotation_mode || self.calibration_mode {
            self.cancel_selection_gesture();
        }
        let registration_owns_input = self.route_registration_photo_pick(ui, &response, viewport, image_rect, w, h);
        if !self.pan_mode {
        let mut calibration_point_edit_handled = false;
        let (primary_pressed, primary_down, primary_released) = ui.input(|input| (
            input.pointer.button_pressed(PointerButton::Primary),
            input.pointer.button_down(PointerButton::Primary),
            input.pointer.button_released(PointerButton::Primary),
        ));
        if primary_pressed && !registration_owns_input {
            if let Some(mouse) = response.interact_pointer_pos() {
                if viewport.contains(mouse) && self.calibration_mode {
                    calibration_point_edit_handled = self.begin_calibration_point_edit(mouse, image_rect, w, h);
                } else if viewport.contains(mouse) && self.selection_mode && !self.annotation_mode {
                    let (additive,toggle)=ui.input(|input|(input.modifiers.shift,input.modifiers.command || input.modifiers.ctrl));
                    self.begin_selection_gesture(mouse,image_rect,w,h,&paint,additive,toggle);
                }
            }
        }
        let drag_pointer = response.interact_pointer_pos()
            .or_else(|| ui.input(|input| input.pointer.latest_pos()));
        if let Some(mouse) = drag_pointer.filter(|_| !registration_owns_input && (primary_down || primary_released)) {
            if let Some(edit) = self.calibration_point_edit.as_ref() {
                if response.dragged_by(PointerButton::Primary) || mouse.distance(edit.pointer_origin) >= 6.0 {
                    self.update_calibration_point_edit(mouse, image_rect, w, h);
                }
            } else {
                let crossed_drag_threshold = self.point_edit.as_ref().is_some_and(|edit| {
                    response.dragged_by(PointerButton::Primary) || mouse.distance(edit.pointer_origin) >= 6.0
                });
                if crossed_drag_threshold {
                    self.update_point_edit(mouse,image_rect,w,h);
                }
            }
            self.update_selection_gesture(mouse,image_rect,w,h);
        }
        if primary_released && !registration_owns_input {
            calibration_point_edit_handled |= self.finish_calibration_point_edit(false);
            self.finish_point_edit(false);
            self.finish_selection_gesture();
        } else if !primary_down {
            self.cancel_selection_gesture();
        }
        // Selection is handled on press/release above; do not toggle it a second time as a click.
        if !registration_owns_input && response.clicked_by(PointerButton::Primary) && !self.selection_mode && !calibration_point_edit_handled {
            if let Some(mouse) = response.interact_pointer_pos() {
                let label_hit = self.selection_mode && !self.annotation_mode
                    && self.measurement_label_at(mouse, image_rect, w, h, &paint).is_some();
                if image_rect.contains(mouse) || label_hit {
                    if self.annotation_mode {
                        let px = (mouse.x - image_rect.left()) / image_rect.width() * w;
                        let py = (mouse.y - image_rect.top()) / image_rect.height() * h;
                        self.handle_annotation_click(Point { x: px as f64, y: py as f64 });
                    } else {
                        let px = (mouse.x - image_rect.left()) / image_rect.width() * w;
                        let py = (mouse.y - image_rect.top()) / image_rect.height() * h;
                        self.on_click(Point {x:px as f64,y:py as f64});
                    }
                }
            }
        }
        }
        for m in &self.measurements {
            let selected = self.measurement_is_selected(m.id);
            let color = if selected { Color32::from_rgb(239, 177, 42) } else { measurement_graphics::CYAN };
            let preview=self.measurement_group_move.as_ref()
                .filter(|movement|movement.selection.contains(&m.id))
                .map(|movement|Self::translated_measurement(m,movement.delta));
            self.render_measurement(&paint,image_rect,w,h,preview.as_ref().unwrap_or(m),color,selected);
        }
        if let Some(marquee)=self.selection_marquee.as_ref().filter(|marquee|marquee.dragged) {
            let area=Rect::from_two_pos(Self::point_to_screen(marquee.start,image_rect,w,h),
                Self::point_to_screen(marquee.end,image_rect,w,h));
            let color=measurement_graphics::CYAN;
            paint.rect_filled(area,2,Color32::from_rgba_unmultiplied(0,160,225,35));
            paint.rect_stroke(area,2,Stroke::new(2.0,color),egui::StrokeKind::Inside);
            let badge=format!("{} mesure(s) · {} annotation(s)",
                marquee.preview.len(),marquee.preview_annotations.len());
            let text=paint.layout_no_wrap(badge,ui_widgets::font(13.0,true),Color32::WHITE);
            let top=(area.top()-text.size().y-10.0).max(viewport.top()+4.0);
            let left=area.left().clamp(viewport.left(),(viewport.right()-text.size().x-12.0).max(viewport.left()));
            let badge=Rect::from_min_size(Pos2::new(left,top),text.size()+Vec2::new(12.0,6.0));
            paint.rect_filled(badge,3,Color32::from_rgb(7,28,45));
            paint.galley(badge.min+Vec2::new(6.0,3.0),text,Color32::WHITE);
        }
        self.paint_inspection_evidence(&paint, image_rect, w, h);
        self.render_annotation_targets(&paint, image_rect, w, h);
        self.paint_pattern_preview(&paint, image_rect, w, h);
        if !self.pending.is_empty() {
            let (draft_kind, draft_points) = if self.calibration_mode {
                (self.calibration_type.preview_tool(), self.pending.clone())
            } else {
                self.drawing_variant.resolve(&self.pending, self.drawing_radius_px)
                    .unwrap_or_else(|_| (Tool::Polyline, self.pending.clone()))
            };
            let draft = Measurement {
                id:0,
                is_construction: !self.calibration_mode && self.construction_mode,
                drawing_variant: (!self.calibration_mode).then(|| self.drawing_variant.label().into()),
                kind:draft_kind,
                points:draft_points, label:"".into(), label_position_native_px: None,
                capture_id:self.capture_id.clone(),
                calibration_id:self.pending_calibration_id.clone(),
                mm_per_pixel:self.pending_mm_per_pixel,
                mm_per_pixel_x: self.pending_calibration_id.as_deref().and_then(|id| self.calibration_history.iter()
                    .find(|profile| profile.id == id).and_then(|profile| profile.scale_x_mm_per_pixel)),
                mm_per_pixel_y: self.pending_calibration_id.as_deref().and_then(|id| self.calibration_history.iter()
                    .find(|profile| profile.id == id).and_then(|profile| profile.scale_y_mm_per_pixel)),
                expanded_uncertainty_mm: self.calibration.as_ref().and_then(|profile| profile.expanded_uncertainty_mm),
                origin:"preview".into(),
                method:None,
                protocol_id:None,
                protocol_version:None,
                source_measurement_ids: Vec::new(), dependency_stale: false,
            };
            self.render_measurement(&paint,image_rect,w,h,&draft,Color32::from_rgb(26, 143, 153),false);
        }
        if let Some(step) = grid_step {
            let spacing = if let Some(mm_per_pixel) = self.mm_per_pixel {
                let mm = format!("{:.2}", step as f64 * mm_per_pixel).replace('.', ",");
                format!("Grille · {:.0} px / {mm} mm", step)
            } else {
                format!("Grille · {:.0} px", step)
            };
            let label_pos = viewport.left_top() + Vec2::new(14.0, 12.0);
            let font = FontId::proportional(12.0);
            for offset in [Vec2::new(-1.0,0.0),Vec2::new(1.0,0.0),Vec2::new(0.0,-1.0),Vec2::new(0.0,1.0)] {
                paint.text(label_pos + offset, Align2::LEFT_TOP, spacing.clone(), font.clone(), Color32::from_rgb(10, 20, 31));
            }
            paint.text(label_pos, Align2::LEFT_TOP, spacing, font, Color32::WHITE);
        }
        // Barre d'échelle physiquement calculée (50 pixels d'image native).
        if let Some(k) = self.mm_per_pixel {
            let displayed_len = 100.0 * scale;
            let a = viewport.left_bottom() + Vec2::new(28.0,-32.0);
            let b = a + Vec2::new(displayed_len,0.0);
            paint.line_segment([a,b],Stroke::new(3.0,Color32::WHITE));
            paint.text(a+Vec2::new(0.0,-20.0),Align2::LEFT_TOP,
                format!("{:.3} mm / 100 px",k*100.0),FontId::proportional(12.0),Color32::WHITE);
        } else {
            paint.text(viewport.left_bottom()+Vec2::new(18.0,-22.0),Align2::LEFT_BOTTOM,
                "NON ÉTALONNÉ · Valeurs en pixels",FontId::proportional(15.0),Color32::YELLOW);
        }
    }
}

impl eframe::App for DperMeasureApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.begin_registration_photo_pick_frame(ctx);
        self.poll_drawing_geometry_analysis(ctx);
        if !ctx.input(|input| input.pointer.primary_down()) {
            self.display_correction_drag_active = false;
        }
        let wants_keyboard = ctx.wants_keyboard_input();
        let (enter_pressed, escape_pressed, undo_pressed, redo_pressed, delete_pressed) = ctx.input(|input| (
            input.key_pressed(egui::Key::Enter),
            input.key_pressed(egui::Key::Escape),
            input.modifiers.ctrl && input.key_pressed(egui::Key::Z),
            input.modifiers.ctrl && input.key_pressed(egui::Key::Y),
            input.key_pressed(egui::Key::Delete),
        ));
        let registration_escape_handled = self.registration_photo_pick_escape(ctx);
        if self.export_dialog.is_none() && !self.confirm_close_image {
            if escape_pressed && !registration_escape_handled && !self.registration_photo_pick_owns_frame()
                && (!wants_keyboard || self.point_edit.is_some()) {
                self.escape_active_mode();
            }
            if !wants_keyboard && !self.registration_photo_pick_owns_frame() {
                if undo_pressed { self.undo_current_action(); }
                if redo_pressed { self.redo_current_action(); }
                if delete_pressed && self.pending.is_empty() && self.point_edit.is_none() {
                    self.delete_selected_measurement();
                }
                if enter_pressed && !self.calibration_mode && !self.selection_mode && !self.pending.is_empty() {
                    self.finish_measurement();
                }
            }
        }
        self.poll_camera(ctx);
        self.poll_capture_timer(ctx);
        self.poll_time_lapse();
        self.handle_mcp_requests(ctx);
        self.invalidate_changed_drawing_dependencies();
        self.recovered_chrome(ctx);
        self.recovered_results_panel(ctx);
        self.refresh_changed_display(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(UiTokens::SURFACE_0)
                .inner_margin(egui::Margin::same(10)))
            .show(ctx, |ui| self.central_image(ui));
        // Preserve already-open dialogs, but suspend their mutating controls
        // until the photo-selection gesture (including its release) is over.
        if !self.registration_photo_pick_owns_frame() {
            self.show_folder_export_dialog(ctx);
            self.comparison_window(ctx);
            self.guided_examples_window(ctx);
            self.panorama_window(ctx);
            self.project_search_window(ctx);
            self.rectification_window(ctx);
            self.uncertainty_budget_window(ctx);
            self.camera_controls_window(ctx);
            self.show_close_image_dialog(ctx);
            self.patterns_window(ctx);
        }
    }
}

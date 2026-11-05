//! Interactive components reconstructed from the 1672 × 941 reference pack.
//! Colours and dimensions follow specifications/DPER_MEASURE_UI_egui.md;
//! crops are visual references and are never used as clickable screen images.
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Response, RichText, Stroke, StrokeKind, Ui, Vec2};

#[derive(Clone, Copy, Debug)]
pub enum Icon {
    Camera, Folder, Ruler, Check, Image, Document, Settings, Select, Distance, Angle,
    Point, Rectangle, Ellipse, Polyline, Parallel, Grid, Crosshair, Undo, Redo, ZoomIn, ZoomOut, Fit,
    Chart, Coordinates, Shapes, Database, Comment, Hand, Sun, Layers, Help,
    Play, Video, Clock, Save, Close, Filter, Crop, Export, Refresh,
    Construction, Mirror, CircularPattern, RectangularPattern,
}

#[derive(Clone, Copy, Debug)]
pub enum RibbonIcon { Camera, Capture, Files, Calibration, Measurements, Inspection, Image, Exports, Automation }

impl RibbonIcon {
    pub fn uv(self) -> Rect {
        let (column, row) = match self {
            Self::Camera => (0, 0),
            Self::Capture => (1, 0),
            Self::Files => (2, 0),
            Self::Calibration => (0, 1),
            Self::Measurements => (1, 1),
            Self::Inspection => (2, 1),
            Self::Image => (0, 2),
            Self::Exports => (1, 2),
            Self::Automation => (2, 2),
        };
        Rect::from_min_max(
            Pos2::new(column as f32 / 3.0, row as f32 / 3.0),
            Pos2::new((column + 1) as f32 / 3.0, (row + 1) as f32 / 3.0),
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Tone { Primary, Secondary, Nav, NavActive, Canvas, Danger, Action }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItemTone { Standard, Success, Danger }

/// Shared visual tokens from the DPER_MEASURE reference pack, in egui points.
pub struct UiTokens;
impl UiTokens {
    pub const NAVY_900: Color32 = Color32::from_rgb(8, 49, 92);
    pub const NAVY_800: Color32 = Color32::from_rgb(18, 67, 111);
    pub const BLUE_700: Color32 = Color32::from_rgb(7, 86, 165);
    pub const BLUE_500: Color32 = Color32::from_rgb(22, 136, 207);
    pub const TEAL_600: Color32 = Color32::from_rgb(2, 139, 176);
    pub const SURFACE_0: Color32 = Color32::from_rgb(243, 246, 250);
    pub const SURFACE_1: Color32 = Color32::WHITE;
    pub const BORDER_300: Color32 = Color32::from_rgb(216, 226, 238);
    pub const TEXT_900: Color32 = Color32::from_rgb(23, 56, 95);
    pub const TEXT_600: Color32 = Color32::from_rgb(82, 107, 137);
    pub const CANVAS_900: Color32 = Color32::from_rgb(6, 18, 36);
    pub const SUCCESS_500: Color32 = Color32::from_rgb(56, 179, 74);
    pub const HEADER_HEIGHT: f32 = 66.0;
    pub const STATUS_HEIGHT: f32 = 37.0;
    pub const RIGHT_WIDTH: f32 = 356.0;
    pub const NAV_HEIGHT: f32 = 46.0;
    pub const TILE_HEIGHT: f32 = 61.0;
    pub const TILE_GAP: f32 = 5.0;
    pub const MENU_ITEM_HEIGHT: f32 = 46.0;
    pub const MENU_ICON_CENTER_X: f32 = 21.0;
    pub const MENU_ICON_SIZE: f32 = 24.0;
    pub const MENU_TEXT_LEFT: f32 = 52.0;
    pub const CARD_RADIUS: u8 = 10;
    pub const CONTROL_RADIUS: u8 = 8;
    pub const SECTION_TITLE_SIZE: f32 = 15.0;
    pub const ROW_HEIGHT: f32 = 40.0;
}
const INK: Color32 = UiTokens::BLUE_700;
const BORDER: Color32 = UiTokens::BORDER_300;

/// Scoped dark chrome; popups restore the application's light visual style.
pub fn dark_chrome(ui: &mut Ui) {
    let mut visuals = egui::Visuals::dark();
    let ink = Color32::from_rgb(220, 235, 247);
    visuals.override_text_color = Some(ink);
    visuals.weak_text_color = Some(Color32::from_rgb(164, 188, 210));
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(36, 60, 82);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(36, 60, 82);
    visuals.widgets.inactive.fg_stroke.color = ink;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(46, 88, 119);
    visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
    visuals.selection.bg_fill = Color32::from_rgb(0, 133, 182);
    visuals.selection.stroke.color = Color32::WHITE;
    *ui.visuals_mut() = visuals;
}

pub fn configure_fonts(ctx: &egui::Context) {
    let mut definitions = egui::FontDefinitions::default();
    let directory = std::env::var_os("WINDIR").map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("C:/Windows")).join("Fonts");
    for (file, name, strong) in [("segoeui.ttf", "dper-regular", false), ("seguisb.ttf", "dper-semibold", true)] {
        if let Ok(bytes) = std::fs::read(directory.join(file)) {
            definitions.font_data.insert(name.into(), egui::FontData::from_owned(bytes).into());
            if !strong {
                definitions.families.entry(egui::FontFamily::Proportional).or_default().insert(0, name.into());
            }
        }
    }
    let mut strong = definitions.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    if definitions.font_data.contains_key("dper-semibold") { strong.insert(0, "dper-semibold".into()); }
    definitions.families.insert(egui::FontFamily::Name("DPER Semibold".into()), strong);
    ctx.set_fonts(definitions);
}

pub fn font(size: f32, strong: bool) -> FontId {
    if strong { FontId::new(size, egui::FontFamily::Name("DPER Semibold".into())) }
    else { FontId::proportional(size) }
}

// A small rounded triangle fan gives each vertex the colour for its vertical position.
fn gradient(painter: &Painter, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 { return; }
    let r = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let mut mesh = egui::epaint::Mesh::default();
    let colour = |y: f32| {
        let t = ((y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color32::from_rgb(mix(top.r(), bottom.r()), mix(top.g(), bottom.g()), mix(top.b(), bottom.b()))
    };
    let mut points = Vec::with_capacity(20);
    for (center, start) in [
        (pos2(rect.right() - r, rect.top() + r), -90.0_f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ] {
        for step in 0..=4 {
            let angle = (start + 90.0 * step as f32 / 4.0).to_radians();
            points.push(center + vec2(angle.cos() * r, angle.sin() * r));
        }
    }
    mesh.vertices.push(egui::epaint::Vertex { pos: rect.center(), uv: egui::epaint::WHITE_UV, color: colour(rect.center().y) });
    for p in &points {
        mesh.vertices.push(egui::epaint::Vertex { pos: *p, uv: egui::epaint::WHITE_UV, color: colour(p.y) });
    }
    for i in 0..points.len() {
        mesh.indices.extend_from_slice(&[0, i as u32 + 1, ((i + 1) % points.len()) as u32 + 1]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn interactive_surface(ui: &mut Ui, label: &str, size: Vec2) -> Response {
    ui.add_sized(size, egui::Button::new(RichText::new(label).strong().color(Color32::TRANSPARENT)).frame(false))
}

fn outline(painter: &Painter, rect: Rect, radius: u8, color: Color32, width: f32) {
    painter.rect_stroke(rect, radius, Stroke::new(width, color), StrokeKind::Inside);
}

pub fn button(ui: &mut Ui, label: &str, icon: Option<Icon>, tone: Tone, min_size: Vec2) -> Response {
    button_impl(ui, label, icon, tone, min_size, false)
}

pub fn compact_button(ui: &mut Ui, label: &str, icon: Option<Icon>, tone: Tone, min_size: Vec2) -> Response {
    button_impl(ui, label, icon, tone, min_size, true)
}

pub fn navigation_menu_button(ui: &mut Ui, label: &str, icon: Icon, active: bool, has_dropdown: bool) -> Response {
    navigation_menu_button_impl(ui, label, active, has_dropdown, 30.0, 20.0, 48.0, |painter, icon_rect| {
        paint_icon(painter, icon_rect.center(), icon, Color32::WHITE);
    })
}

pub fn navigation_tab_button(
    ui: &mut Ui,
    label: &str,
    texture: &egui::TextureHandle,
    uv: Rect,
    active: bool,
) -> Response {
    let response = navigation_menu_button_impl(ui, label, active, false, 36.0, 23.0, 52.0, |painter, icon_rect| {
        painter.image(texture.id(), icon_rect, uv, Color32::WHITE);
    });
    #[cfg(test)]
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("ribbon-test", label)), response.rect));
    response
}

fn navigation_menu_button_impl(
    ui: &mut Ui,
    label: &str,
    active: bool,
    has_dropdown: bool,
    icon_size: f32,
    icon_center_x: f32,
    label_left: f32,
    paint_navigation_icon: impl FnOnce(&Painter, Rect),
) -> Response {
    let label_font = font(14.0, true);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), label_font.clone(), Color32::WHITE);
    let width = galley.size().x + label_left + if has_dropdown { 44.0 } else { 26.0 };
    let response = ui.add_sized(
        vec2(width, UiTokens::NAV_HEIGHT),
        egui::Button::new(RichText::new(label).strong().color(Color32::TRANSPARENT)).frame(false),
    );
    let rect = response.rect;
    let (mut top, mut bottom, border) = if active {
        (Color32::from_rgb(10, 163, 225), Color32::from_rgb(0, 119, 190), Color32::from_rgb(38, 187, 235))
    } else {
        (Color32::from_rgb(39, 76, 115), Color32::from_rgb(24, 57, 91), Color32::from_rgb(53, 92, 132))
    };
    if response.is_pointer_button_down_on() { std::mem::swap(&mut top, &mut bottom); }
    else if response.hovered() { top = top.gamma_multiply(1.12); bottom = bottom.gamma_multiply(1.12); }
    let painter = ui.painter_at(rect);
    gradient(&painter, rect, 5.0, top, bottom);
    outline(&painter, rect, 5, border, if response.has_focus() { 2.0 } else { 1.0 });
    let icon_rect = Rect::from_center_size(pos2(rect.left() + icon_center_x, rect.center().y), vec2(icon_size, icon_size));
    paint_navigation_icon(&painter, icon_rect);
    painter.text(pos2(rect.left() + label_left, rect.center().y), Align2::LEFT_CENTER,
        label, label_font, Color32::WHITE);
    if has_dropdown {
        let chevron = pos2(rect.right() - 13.0, rect.center().y + 1.0);
        painter.line_segment([chevron + vec2(-3.0, -2.0), chevron], Stroke::new(1.5, Color32::from_rgb(224, 239, 251)));
        painter.line_segment([chevron, chevron + vec2(3.0, -2.0)], Stroke::new(1.5, Color32::from_rgb(224, 239, 251)));
    }
    if active {
        painter.line_segment([pos2(rect.center().x - 12.0, rect.bottom() - 2.0),
            pos2(rect.center().x + 12.0, rect.bottom() - 2.0)], Stroke::new(2.5, Color32::WHITE));
    }
    response
}

pub fn ribbon_command_button(
    ui: &mut Ui,
    label: &str,
    icon: Icon,
    tone: Tone,
    selected: bool,
    enabled: bool,
    compact: bool,
) -> Response {
    let width = if compact { 60.0 } else { 78.0 };
    let response = ui.add_enabled_ui(enabled, |ui| {
        ui.add_sized(
            vec2(width, 74.0),
            egui::Button::new(RichText::new(label).color(Color32::TRANSPARENT)).frame(false),
        )
    }).inner;
    let rect = response.rect;
    let painter = ui.painter_at(rect);
    let (mut fill, border, icon_color, text_color) = if !enabled {
        (Color32::from_rgb(20, 34, 49), Color32::from_rgb(41, 57, 72),
            Color32::from_rgb(87, 104, 120), Color32::from_rgb(116, 132, 148))
    } else if matches!(tone, Tone::Danger) {
        (Color32::from_rgb(67, 31, 42), Color32::from_rgb(146, 59, 72),
            Color32::from_rgb(255, 126, 137), Color32::from_rgb(255, 223, 225))
    } else if selected || matches!(tone, Tone::Action | Tone::Primary) {
        (Color32::from_rgb(0, 113, 175), Color32::from_rgb(27, 190, 239),
            Color32::WHITE, Color32::WHITE)
    } else {
        (Color32::from_rgb(15, 37, 59), Color32::from_rgb(41, 69, 97),
            Color32::from_rgb(162, 220, 249), Color32::from_rgb(225, 237, 248))
    };
    if response.is_pointer_button_down_on() {
        fill = fill.gamma_multiply(0.78);
    } else if response.hovered() && enabled {
        fill = fill.gamma_multiply(1.22);
    }
    painter.rect_filled(rect.translate(vec2(0.0, 2.0)), 7, Color32::from_rgb(7, 17, 29));
    painter.rect_filled(rect, 7, fill);
    painter.rect_stroke(rect, 7, Stroke::new(if response.has_focus() { 2.0 } else { 1.0 },
        if response.has_focus() { Color32::WHITE } else { border }), StrokeKind::Inside);
    painter.line_segment(
        [pos2(rect.left() + 7.0, rect.top() + 1.5), pos2(rect.right() - 7.0, rect.top() + 1.5)],
        Stroke::new(1.0, border.gamma_multiply(0.78)),
    );
    paint_icon(&painter, pos2(rect.center().x, rect.top() + 22.0), icon, icon_color);
    let mut label_job = egui::text::LayoutJob::simple(
        label.to_owned(),
        font(10.8, selected),
        text_color,
        (rect.width() - 8.0).max(32.0),
    );
    label_job.wrap.max_rows = 2;
    label_job.wrap.break_anywhere = true;
    let label_galley = painter.layout_job(label_job);
    let label_x = rect.center().x - label_galley.size().x * 0.5;
    painter.galley(pos2(label_x, rect.top() + 39.0), label_galley, text_color);
    #[cfg(test)]
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("ribbon-test", label)), response.rect));
    response.on_hover_text(label)
}

pub fn menu_item(ui: &mut Ui, label: &str, icon: Icon, shortcut: Option<&str>, submenu: bool,
                 enabled: bool, selected: bool, tone: MenuItemTone) -> Response {
    let response = ui.add_enabled_ui(enabled, |ui| {
        ui.add_sized(vec2(ui.available_width(), UiTokens::MENU_ITEM_HEIGHT),
            egui::Button::new(RichText::new(label).color(Color32::TRANSPARENT)).frame(false))
    }).inner;
    paint_menu_item(ui, &response, label, icon, shortcut, submenu, enabled, selected, tone);
    #[cfg(test)]
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("ribbon-test", label)), response.rect));
    response.on_hover_text(label)
}

/// Return the natural width for a menu row, including its icon/text columns and end padding.
pub fn menu_item_width(ui: &Ui, label: &str, shortcut: Option<&str>, submenu: bool) -> f32 {
    let label_width = ui.painter()
        .layout_no_wrap(label.to_owned(), font(14.0, true), UiTokens::TEXT_900)
        .size()
        .x;
    let right_reserved = menu_item_right_reserved(ui, shortcut, submenu);
    (UiTokens::MENU_TEXT_LEFT + label_width + right_reserved + 12.0).max(104.0)
}

/// Use the same explicit icon and text columns for submenu triggers as action rows.
/// The underlying native submenu retains click, hover and keyboard behavior.
pub fn submenu_item(ui: &mut Ui, label: &str, icon: Icon,
    add_contents: impl FnOnce(&mut Ui)) -> Response {
    let button = egui::Button::new(RichText::new(label).color(Color32::TRANSPARENT))
        .min_size(vec2(ui.available_width(), UiTokens::MENU_ITEM_HEIGHT)).frame(false);
    let (response, popup) = egui::containers::menu::SubMenuButton::from_button(button).ui(ui, add_contents);
    paint_menu_item(ui, &response, label, icon, None, true, true, popup.is_some(), MenuItemTone::Standard);
    #[cfg(test)]
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("ribbon-test", label)), response.rect));
    response.on_hover_text(label)
}

#[allow(clippy::too_many_arguments)]
fn paint_menu_item(ui: &Ui, response: &Response, label: &str, icon: Icon,
    shortcut: Option<&str>, submenu: bool, enabled: bool, selected: bool, tone: MenuItemTone) {
    let rect = response.rect;
    let (text_color, icon_color, hover_fill, active_fill) = match tone {
        MenuItemTone::Standard => (UiTokens::TEXT_900, UiTokens::BLUE_700,
            Color32::from_rgb(236, 246, 255), Color32::from_rgb(218, 237, 254)),
        MenuItemTone::Success => (Color32::from_rgb(20, 112, 75), Color32::from_rgb(21, 158, 89),
            Color32::from_rgb(230, 249, 238), Color32::from_rgb(216, 246, 228)),
        MenuItemTone::Danger => (Color32::from_rgb(196, 38, 55), Color32::from_rgb(219, 43, 61),
            Color32::from_rgb(255, 240, 242), Color32::from_rgb(255, 226, 230)),
    };
    let disabled_text = Color32::from_rgb(146, 161, 177);
    let text_color = if enabled { text_color } else { disabled_text };
    let icon_color = if enabled { icon_color } else { disabled_text };
    let painter = ui.painter_at(rect);
    if selected || response.hovered() || response.has_focus() || response.is_pointer_button_down_on() {
        let fill = if selected || response.is_pointer_button_down_on() { active_fill } else { hover_fill };
        painter.rect_filled(rect.shrink(1.0), 7, fill);
        if response.has_focus() || selected {
            painter.rect_stroke(rect.shrink(1.0), 7, Stroke::new(1.0, UiTokens::BLUE_500), StrokeKind::Inside);
        }
    }
    let icon_center = pos2(rect.left() + UiTokens::MENU_ICON_CENTER_X, rect.center().y);
    let icon_rect = Rect::from_center_size(icon_center, Vec2::splat(UiTokens::MENU_ICON_SIZE));
    paint_icon(&painter.with_clip_rect(icon_rect), icon_center, icon, icon_color);
    let right_reserved = menu_item_right_reserved(ui, shortcut, submenu);
    let label_rect = Rect::from_min_max(pos2(rect.left() + UiTokens::MENU_TEXT_LEFT, rect.top()),
        pos2((rect.right() - right_reserved).max(rect.left() + UiTokens::MENU_TEXT_LEFT + 2.0), rect.bottom()));
    painter.with_clip_rect(label_rect).text(pos2(label_rect.left(), rect.center().y), Align2::LEFT_CENTER,
        label, font(14.0, selected), text_color);
    if let Some(shortcut) = shortcut {
        painter.text(pos2(rect.right() - if submenu { 25.0 } else { 10.0 }, rect.center().y),
            Align2::RIGHT_CENTER, shortcut, font(12.0, false),
            if enabled { UiTokens::TEXT_600 } else { disabled_text });
    }
    if submenu {
        let center = pos2(rect.right() - 12.0, rect.center().y);
        let color = if enabled { UiTokens::TEXT_600 } else { disabled_text };
        painter.line_segment([center + vec2(-2.0, -4.0), center + vec2(2.0, 0.0)], Stroke::new(1.5, color));
        painter.line_segment([center + vec2(2.0, 0.0), center + vec2(-2.0, 4.0)], Stroke::new(1.5, color));
    }
}

fn menu_item_right_reserved(ui: &Ui, shortcut: Option<&str>, submenu: bool) -> f32 {
    if let Some(shortcut) = shortcut {
        let shortcut_width = ui.painter()
            .layout_no_wrap(shortcut.to_owned(), font(12.0, false), UiTokens::TEXT_600)
            .size()
            .x;
        (if submenu { 25.0 } else { 10.0 }) + shortcut_width + 10.0
    } else if submenu {
        30.0
    } else {
        12.0
    }
}

pub fn menu_section(ui: &mut Ui, label: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), egui::Sense::hover());
    ui.painter().text(pos2(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER,
        label, font(11.5, true), UiTokens::TEXT_600);
}

pub fn menu_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), egui::Sense::hover());
    ui.painter().line_segment([pos2(rect.left() + 8.0, rect.center().y),
        pos2(rect.right() - 8.0, rect.center().y)], Stroke::new(1.0, UiTokens::BORDER_300));
}

fn button_impl(ui: &mut Ui, label: &str, icon: Option<Icon>, tone: Tone, min_size: Vec2, compact: bool) -> Response {
    let label_font = font(if compact { 12.0 } else { 14.0 }, true);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), label_font.clone(), Color32::WHITE);
    let icon_only = icon.is_some() && label.is_empty();
    let icon_gap = if compact { 32.0 } else { 34.0 };
    let padding = if compact { 14.0 } else { 26.0 };
    let content_width = if icon_only { 36.0 } else { galley.size().x + if icon.is_some() { icon_gap } else { 0.0 } + padding };
    let size = vec2(min_size.x.max(content_width), min_size.y.max(if matches!(tone, Tone::Nav | Tone::NavActive) {
        UiTokens::NAV_HEIGHT
    } else { 34.0 }));
    let response = if compact {
        ui.add_sized(size, egui::Button::new(RichText::new(label).size(12.0).strong().color(Color32::TRANSPARENT)).frame(false))
    } else { interactive_surface(ui, label, size) };
    let rect = response.rect;
    let (mut top, mut bottom, mut text, mut border) = match tone {
        Tone::Action => (Color32::from_rgb(5, 163, 194), UiTokens::TEAL_600, Color32::WHITE, Color32::from_rgb(0, 125, 158)),
        Tone::Primary => (Color32::from_rgb(39, 157, 231), Color32::from_rgb(11, 116, 195), Color32::WHITE, Color32::from_rgb(10, 113, 180)),
        Tone::Secondary => (Color32::WHITE, Color32::from_rgb(235, 243, 250), INK, BORDER),
        Tone::Nav => (Color32::from_rgb(40, 78, 117), UiTokens::NAVY_800, Color32::WHITE, Color32::from_rgb(55, 101, 143)),
        Tone::NavActive => (Color32::from_rgb(8, 168, 202), Color32::from_rgb(0, 126, 169), Color32::WHITE, Color32::from_rgb(67, 199, 222)),
        Tone::Canvas => (Color32::from_rgb(35, 55, 76), Color32::from_rgb(25, 43, 62), Color32::from_rgb(230, 242, 250), Color32::from_rgb(54, 77, 99)),
        Tone::Danger => (Color32::WHITE, Color32::from_rgb(255, 240, 242), Color32::from_rgb(164, 48, 62), Color32::from_rgb(235, 194, 201)),
    };
    if !ui.is_enabled() {
        top = Color32::from_rgb(246, 248, 250); bottom = Color32::from_rgb(234, 239, 244);
        text = Color32::from_rgb(143, 157, 172); border = Color32::from_rgb(222, 230, 238);
    } else if response.is_pointer_button_down_on() { std::mem::swap(&mut top, &mut bottom); }
    else if response.hovered() { top = top.gamma_multiply(1.12); bottom = bottom.gamma_multiply(1.12); }
    let p = ui.painter_at(rect);
    gradient(&p, rect, UiTokens::CONTROL_RADIUS as f32, top, bottom);
    outline(&p, rect, UiTokens::CONTROL_RADIUS, if response.has_focus() { Color32::from_rgb(73, 192, 240) } else { border }, if response.has_focus() { 2.0 } else { 1.0 });
    let group_width = galley.size().x + if icon_only { 18.0 } else if icon.is_some() { icon_gap } else { 0.0 };
    let left = rect.center().x - group_width * 0.5;
    if let Some(icon) = icon { paint_icon(&p, pos2(left + 9.0, rect.center().y), icon, text); }
    let text_left = left + if icon.is_some() { icon_gap } else { 0.0 };
    let text_rect = Rect::from_min_max(pos2(text_left, rect.top()), rect.max - vec2(7.0, 0.0));
    if text_rect.is_positive() {
        p.with_clip_rect(text_rect).text(pos2(text_left, rect.center().y), Align2::LEFT_CENTER, label, label_font, text);
    }
    response
}

pub fn tool_tile(ui: &mut Ui, label: &str, icon: Icon, selected: bool, width: f32) -> Response {
    let response = interactive_surface(ui, label, vec2(width.max(1.0), UiTokens::TILE_HEIGHT));
    let rect = response.rect;
    let p = ui.painter_at(rect);
    let (top, bottom, color, border) = if selected {
        (Color32::from_rgb(55, 164, 237), Color32::from_rgb(23, 119, 201), Color32::WHITE, Color32::from_rgb(22, 112, 190))
    } else if response.hovered() {
        (Color32::WHITE, Color32::from_rgb(226, 241, 253), INK, Color32::from_rgb(152, 197, 226))
    } else {
        (Color32::WHITE, Color32::from_rgb(240, 247, 253), INK, BORDER)
    };
    gradient(&p, rect, 7.0, if response.is_pointer_button_down_on() { bottom } else { top }, bottom);
    outline(&p, rect, 7, if response.has_focus() { Color32::from_rgb(35, 157, 224) } else { border }, if response.has_focus() { 2.0 } else { 1.0 });
    paint_icon(&p, pos2(rect.center().x, rect.top() + 19.0), icon, color);
    let text_rect = Rect::from_min_max(pos2(rect.left() + 3.0, rect.top() + 32.0), rect.max - vec2(3.0, 2.0));
    p.with_clip_rect(text_rect).text(pos2(rect.center().x, rect.top() + 44.0), Align2::CENTER_CENTER, label, font(12.0, selected), color);
    response
}

pub fn card() -> egui::Frame {
    egui::Frame::NONE.fill(Color32::from_rgb(248, 251, 255)).stroke(Stroke::new(1.0, BORDER)).corner_radius(UiTokens::CARD_RADIUS).inner_margin(egui::Margin::symmetric(14, 10))
}

pub fn section_header(ui: &mut Ui, label: &str, icon: Icon, open: bool) -> Response {
    let width = ui.available_width().max(1.0);
    let response = interactive_surface(ui, label, vec2(width, 36.0));
    let rect = response.rect;
    let p = ui.painter_at(rect);
    if response.hovered() { p.rect_filled(rect, 5, Color32::from_rgb(232, 243, 252)); }
    if response.has_focus() { outline(&p, rect, 5, Color32::from_rgb(35, 157, 224), 2.0); }
    paint_icon(&p, pos2(rect.left() + 12.0, rect.center().y), icon, INK);
    let text_rect = Rect::from_min_max(pos2(rect.left() + 36.0, rect.top()), pos2((rect.right() - 30.0).max(rect.left() + 36.0), rect.bottom()));
    p.with_clip_rect(text_rect).text(pos2(text_rect.left(), rect.center().y), Align2::LEFT_CENTER, label, font(UiTokens::SECTION_TITLE_SIZE, true), INK);
    let c = pos2(rect.right() - 11.0, rect.center().y);
    let points = if open { vec![c + vec2(-4.0, 2.0), c + vec2(0.0, -2.0), c + vec2(4.0, 2.0)] } else { vec![c + vec2(-4.0, -2.0), c + vec2(0.0, 2.0), c + vec2(4.0, -2.0)] };
    p.add(egui::Shape::line(points, Stroke::new(1.8, INK)));
    response
}

pub fn gauge(ui: &mut Ui, label: &str, value_text: &str, fraction: f32, good: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width().max(1.0), 30.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let text_font = font(12.0, false);
    let label_galley = p.layout_no_wrap(label.to_owned(), text_font.clone(), UiTokens::TEXT_600);
    let value_galley = p.layout_no_wrap(value_text.to_owned(), text_font, UiTokens::TEXT_900);
    let label_width = (rect.width() * 0.32).clamp(70.0, 100.0).min(rect.width() * 0.5);
    let value_width = (rect.width() * 0.17).clamp(40.0, 60.0).min(rect.width() - label_width);
    let label_rect = Rect::from_min_max(rect.min, pos2(rect.left() + label_width, rect.bottom()));
    let value_rect = Rect::from_min_max(pos2(rect.right() - value_width, rect.top()), rect.max);
    p.with_clip_rect(label_rect).galley(pos2(rect.left(), rect.center().y - label_galley.size().y * 0.5), label_galley, UiTokens::TEXT_600);
    p.with_clip_rect(value_rect).galley(pos2(rect.right() - value_galley.size().x, rect.center().y - value_galley.size().y * 0.5), value_galley, UiTokens::TEXT_900);
    let track_left = (rect.left() + label_width + 10.0).min(rect.right());
    let track_right = (rect.right() - value_width - 10.0).max(track_left);
    let track = Rect::from_min_max(pos2(track_left, rect.center().y - 5.0), pos2(track_right, rect.center().y + 5.0));
    p.rect_filled(track, 5, Color32::from_rgb(226, 234, 241));
    let amount = if fraction.is_finite() { fraction.clamp(0.0, 1.0) } else { 0.0 };
    if amount > 0.0 && track.width() > 0.0 {
        let fill = Rect::from_min_max(track.min, pos2(track.left() + track.width() * amount, track.bottom()));
        let (top,bottom) = if good { (Color32::from_rgb(76, 201, 65), UiTokens::SUCCESS_500) }
            else { (Color32::from_rgb(249,177,73),Color32::from_rgb(230,138,37)) };
        gradient(&p,fill,4.0,top,bottom);
    }
    response
}

pub fn tab(ui: &mut Ui, label: &str, count: usize, selected: bool, width: f32) -> Response {
    let response = interactive_surface(ui,label,vec2(width,36.0));
    let rect=response.rect;
    let p=ui.painter_at(rect);
    if selected || response.hovered() { p.rect_filled(rect,7,Color32::from_rgb(250,253,255)); }
    if selected { outline(&p,rect,7,Color32::from_rgb(37,139,239),1.1); }
    let text_color=if selected { Color32::from_rgb(0,103,205) } else { INK };
    let galley=p.layout_no_wrap(label.to_owned(),font(13.0,true),text_color);
    let left=rect.center().x-(galley.size().x+36.0)*0.5;
    p.galley(pos2(left,rect.center().y-galley.size().y*0.5),galley,text_color);
    let badge=Rect::from_center_size(pos2(rect.right()-22.0,rect.center().y),vec2(25.0,23.0));
    p.rect_filled(badge,11,if selected {Color32::from_rgb(0,143,236)} else {Color32::from_rgb(203,219,235)});
    p.text(badge.center(),Align2::CENTER_CENTER,count.to_string(),font(12.5,true),if selected {Color32::WHITE} else {INK});
    response
}

pub fn table_header(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 5, Color32::from_rgb(235, 242, 249));
    for (label, offset) in [("#", 8.0), ("Type", 39.0), ("Valeur", rect.width() * 0.54 + 5.0), ("Unité", rect.width() * 0.77 + 3.0)] {
        p.text(pos2(rect.left() + offset, rect.center().y), Align2::LEFT_CENTER, label, font(12.0, true), INK);
    }
}

pub fn table_row(ui: &mut Ui, id: &str, kind: &str, value: &str, unit: &str,
                 icon: Icon, icon_color: Color32, selected: bool) -> (Response, Response) {
    let accessible = format!("Mesure {id}");
    let response = interactive_surface(ui, &accessible, vec2(ui.available_width(), UiTokens::ROW_HEIGHT));
    let rect = response.rect;
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0, if selected { Color32::from_rgb(225, 240, 254) }
        else if response.hovered() { Color32::from_rgb(239, 247, 253) } else { Color32::from_rgb(250, 252, 255) });
    p.line_segment([rect.left_bottom(),rect.right_bottom()], Stroke::new(1.0,Color32::from_rgb(231,239,247)));
    if response.has_focus() { outline(&p,rect,2,Color32::from_rgb(31,136,215),1.5); }
    let value_start = rect.left() + rect.width() * 0.54;
    let unit_start = rect.left() + rect.width() * 0.77;
    let cells = [
        (id, rect.left()+5.0, rect.left()+31.0, false),
        (kind, rect.left()+57.0, value_start-3.0, false),
        (value, value_start+3.0, unit_start-7.0, true),
        (unit, unit_start+3.0, rect.right()-25.0, false),
    ];
    for (text,left,right,strong) in cells {
        let clip = Rect::from_min_max(pos2(left,rect.top()),pos2(right.max(left+1.0),rect.bottom()));
        p.with_clip_rect(clip).text(pos2(left,rect.center().y),Align2::LEFT_CENTER,text,font(12.5,strong),INK);
    }
    paint_icon(&p,pos2(rect.left()+42.0,rect.center().y),icon,icon_color);
    let menu_rect = Rect::from_min_max(pos2(rect.right()-24.0,rect.top()+3.0),rect.max-vec2(1.0,3.0));
    let menu = ui.interact(menu_rect,response.id.with("options"),egui::Sense::click())
        .on_hover_text("Options de la mesure");
    menu.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,ui.is_enabled(),"Options de la mesure"));
    for d in [-4.0,0.0,4.0] { p.circle_filled(menu_rect.center()+vec2(d,0.0),1.2,INK); }
    (response,menu)
}

pub fn paint_icon(p: &Painter, center: Pos2, icon: Icon, color: Color32) {
    let s = Stroke::new(1.6, color);
    let pt = |x: f32, y: f32| center + vec2(x, y);
    let line = |a: (f32, f32), b: (f32, f32)| { p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], s); };
    let path = |coords: &[(f32, f32)]| { p.add(egui::Shape::line(coords.iter().map(|&(x,y)| pt(x,y)).collect(), s)); };
    let box_at = |x: f32, y: f32, w: f32, h: f32| { p.rect_stroke(Rect::from_min_size(pt(x,y), vec2(w,h)), 2, s, StrokeKind::Inside); };
    match icon {
        Icon::Camera => { box_at(-9.0,-6.0,18.0,13.0); path(&[(-6.0,-6.0),(-4.0,-9.0),(2.0,-9.0),(4.0,-6.0)]); p.circle_stroke(center, 4.0, s); }
        Icon::Folder => path(&[(-9.0,7.0),(-9.0,-7.0),(-2.0,-7.0),(0.0,-4.0),(9.0,-4.0),(9.0,7.0),(-9.0,7.0)]),
        Icon::Ruler => { path(&[(-9.0,5.0),(5.0,-9.0),(9.0,-5.0),(-5.0,9.0),(-9.0,5.0)]); for d in [-4.0,0.0,4.0] { line((d-2.0,-d),(d,-d+2.0)); } }
        Icon::Check => { p.circle_stroke(center, 9.0, s); path(&[(-4.0,0.0),(-1.0,3.0),(5.0,-4.0)]); }
        Icon::Image => { box_at(-9.0,-7.0,18.0,14.0); p.circle_filled(pt(-4.0,-3.0), 1.4, color); path(&[(-8.0,5.0),(-2.0,-1.0),(1.0,2.0),(4.0,-1.0),(8.0,5.0)]); }
        Icon::Document => { path(&[(-7.0,-9.0),(3.0,-9.0),(7.0,-5.0),(7.0,9.0),(-7.0,9.0),(-7.0,-9.0)]); line((-4.0,0.0),(4.0,0.0)); line((-4.0,4.0),(4.0,4.0)); }
        Icon::Settings => { p.circle_stroke(center, 6.0, s); p.circle_stroke(center, 2.0, s); for (x,y) in [(0.0,-9.0),(0.0,9.0),(-9.0,0.0),(9.0,0.0),(-6.5,-6.5),(6.5,-6.5),(-6.5,6.5),(6.5,6.5)] { line((x*0.75,y*0.75),(x,y)); } }
        Icon::Select => { p.add(egui::Shape::convex_polygon(vec![pt(-7.0,-9.0),pt(-5.0,7.0),pt(7.0,0.0)], color, Stroke::NONE)); p.add(egui::Shape::convex_polygon(vec![pt(-2.0,1.0),pt(2.0,9.0),pt(5.0,7.0),pt(1.0,0.0)],color,Stroke::NONE)); }
        Icon::Distance => { line((-7.0,7.0),(7.0,-7.0)); p.circle_filled(pt(-7.0,7.0),2.0,color); p.circle_filled(pt(7.0,-7.0),2.0,color); }
        Icon::Angle => { path(&[(2.0,-8.0),(-8.0,7.0),(8.0,7.0)]); path(&[(-2.0,7.0),(-2.5,4.0),(-4.0,2.0)]); }
        Icon::Point => { p.circle_stroke(center, 6.0, s); }
        Icon::Rectangle => box_at(-8.0, -6.0, 16.0, 12.0),
        Icon::Ellipse => { p.add(egui::Shape::ellipse_stroke(center, vec2(8.0, 5.0), s)); }
        Icon::Polyline => { path(&[(-8.0,6.0),(-3.0,-5.0),(2.0,2.0),(8.0,-7.0)]); for (x,y) in [(-8.0,6.0),(-3.0,-5.0),(2.0,2.0),(8.0,-7.0)] { p.circle_filled(pt(x,y), 2.0, color); } }
        Icon::Parallel => { line((-9.0,4.0),(4.0,-9.0)); line((-6.0,7.0),(7.0,-6.0)); line((-3.0,10.0),(10.0,-3.0)); }
        Icon::Grid => { box_at(-8.0,-8.0,16.0,16.0); line((-3.0,-8.0),(-3.0,8.0)); line((3.0,-8.0),(3.0,8.0)); line((-8.0,-3.0),(8.0,-3.0)); line((-8.0,3.0),(8.0,3.0)); }
        Icon::Crosshair => { p.circle_stroke(center, 6.0, s); line((-10.0,0.0),(10.0,0.0)); line((0.0,-10.0),(0.0,10.0)); }
        Icon::Undo => paint_history_arrow(p, center, 1.0, color),
        Icon::Redo => paint_history_arrow(p, center, -1.0, color),
        Icon::ZoomIn | Icon::ZoomOut => { p.circle_stroke(pt(-2.0,-2.0), 6.0, s); line((2.5,2.5),(8.0,8.0)); line((-5.0,-2.0),(1.0,-2.0)); if matches!(icon, Icon::ZoomIn) { line((-2.0,-5.0),(-2.0,1.0)); } }
        Icon::Fit => { for (x,y) in [(-1.0,-1.0),(1.0,-1.0),(-1.0,1.0),(1.0,1.0)] { path(&[(x*4.0,y*8.0),(x*8.0,y*8.0),(x*8.0,y*4.0)]); } }
        Icon::Chart => { box_at(-8.0,2.0,3.0,7.0); box_at(-2.0,-3.0,3.0,12.0); box_at(4.0,-9.0,3.0,18.0); }
        Icon::Coordinates => { p.text(center, Align2::CENTER_CENTER,"1 2\n2 3",font(10.0,true),color); }
        Icon::Shapes => path(&[(-8.0,-8.0),(8.0,-8.0),(8.0,3.0),(4.0,7.0),(0.0,10.0),(-4.0,7.0),(-8.0,3.0),(-8.0,-8.0)]),
        Icon::Database => { p.add(egui::Shape::ellipse_stroke(pt(0.0,-6.0), vec2(8.0,3.0), s)); line((-8.0,-6.0),(-8.0,6.0)); line((8.0,-6.0),(8.0,6.0)); path(&[(-8.0,0.0),(-5.0,2.0),(0.0,3.0),(5.0,2.0),(8.0,0.0)]); path(&[(-8.0,6.0),(-5.0,8.0),(0.0,9.0),(5.0,8.0),(8.0,6.0)]); }
        Icon::Hand => path(&[(-6.0,-1.0),(-6.0,-7.0),(-5.0,-9.0),(-3.0,-9.0),(-3.0,-2.0),(-2.0,-7.0),(0.0,-7.0),(0.0,-2.0),(1.0,-6.0),(3.0,-6.0),(3.0,-1.0),(4.0,-4.0),(6.0,-4.0),(6.0,3.0),(4.0,8.0),(-2.0,9.0),(-6.0,5.0),(-9.0,1.0),(-8.0,-1.0),(-6.0,-1.0)]),
        Icon::Sun => { p.circle_stroke(center, 4.0, s); for (x,y) in [(0.0,-9.0),(0.0,9.0),(-9.0,0.0),(9.0,0.0),(-6.5,-6.5),(6.5,-6.5),(-6.5,6.5),(6.5,6.5)] { line((x*0.72,y*0.72),(x,y)); } }
        Icon::Layers => { path(&[(0.0,-8.0),(9.0,-3.0),(0.0,2.0),(-9.0,-3.0),(0.0,-8.0)]); path(&[(-9.0,1.0),(0.0,6.0),(9.0,1.0)]); path(&[(-9.0,5.0),(0.0,10.0),(9.0,5.0)]); }
        Icon::Help => { p.circle_stroke(center, 9.0, s); path(&[(-3.0,-3.0),(-2.0,-5.0),(1.0,-5.0),(3.0,-3.0),(3.0,-1.0),(0.0,1.0),(0.0,3.0)]); p.circle_filled(pt(0.0,6.0),1.0,color); }
        Icon::Comment => path(&[(-9.0,-7.0),(9.0,-7.0),(9.0,5.0),(1.0,5.0),(-4.0,9.0),(-4.0,5.0),(-9.0,5.0),(-9.0,-7.0)]),
        Icon::Play => { p.add(egui::Shape::convex_polygon(vec![pt(-5.0,-8.0),pt(7.0,0.0),pt(-5.0,8.0)], color, Stroke::NONE)); }
        Icon::Video => { box_at(-9.0,-6.0,12.0,12.0); path(&[(3.0,-3.5),(9.0,-7.0),(9.0,7.0),(3.0,3.5)]); }
        Icon::Clock => { p.circle_stroke(center, 8.0, s); line((0.0,-5.0),(0.0,0.0)); line((0.0,0.0),(4.0,2.0)); }
        Icon::Save => { box_at(-8.0,-8.0,16.0,16.0); box_at(-4.0,-8.0,8.0,5.0); box_at(-5.0,2.0,10.0,6.0); }
        Icon::Close => { line((-7.0,-7.0),(7.0,7.0)); line((7.0,-7.0),(-7.0,7.0)); }
        Icon::Filter => { line((-8.0,-6.0),(8.0,-6.0)); line((-5.0,0.0),(5.0,0.0)); line((-2.0,6.0),(2.0,6.0)); p.circle_filled(pt(-3.0,-6.0),2.0,color); p.circle_filled(pt(4.0,0.0),2.0,color); }
        Icon::Crop => { path(&[(-7.0,-9.0),(-7.0,6.0),(8.0,6.0)]); path(&[(0.0,-8.0),(0.0,1.0),(8.0,1.0)]); line((-9.0,-6.0),(-5.0,-6.0)); line((6.0,8.0),(6.0,4.0)); }
        Icon::Export => { box_at(-8.0,-3.0,12.0,11.0); line((-2.0,1.0),(8.0,-9.0)); line((2.0,-9.0),(8.0,-9.0)); line((8.0,-9.0),(8.0,-3.0)); }
        Icon::Refresh => { p.circle_stroke(center, 7.0, s); path(&[(2.0,-9.0),(8.0,-8.0),(7.0,-2.0)]); }
        Icon::Construction => {
            for index in 0..4 { let x = -7.0 + index as f32 * 4.0; line((x, -x), (x+2.0, -x-2.0)); }
            p.circle_stroke(pt(-7.0, 7.0), 2.0, s); p.circle_stroke(pt(7.0, -7.0), 2.0, s);
        }
        Icon::Mirror => {
            for y in [-8.0, -2.0, 4.0] { line((0.0,y),(0.0,y+3.0)); }
            path(&[(-3.0,-7.0),(-9.0,7.0),(-3.0,7.0),(-3.0,-7.0)]);
            path(&[(3.0,-7.0),(9.0,7.0),(3.0,7.0),(3.0,-7.0)]);
        }
        Icon::CircularPattern => {
            p.circle_stroke(center, 2.0, s);
            for index in 0..6 {
                let angle = index as f32 * std::f32::consts::TAU / 6.0;
                p.circle_stroke(pt(angle.cos()*8.0, angle.sin()*8.0), 1.8, s);
            }
        }
        Icon::RectangularPattern => {
            for y in [-6.0, 0.0, 6.0] { for x in [-7.0, 0.0, 7.0] { box_at(x-1.7, y-1.7, 3.4, 3.4); } }
        }
    }
}

fn paint_history_arrow(p: &Painter, center: Pos2, direction: f32, color: Color32) {
    let point = |x: f32, y: f32| center + vec2(direction * x, y);
    let t = |value: f32| 1.0 - value;
    // A sampled cubic gives the return stroke a smooth, balanced turn at icon size.
    let mut curve: Vec<Pos2> = (0..=20).map(|step| {
        let u = step as f32 / 20.0;
        let a = t(u);
        let x = a*a*a*4.0 + 3.0*a*a*u*8.0 + 3.0*a*u*u*8.0 + u*u*u*2.0;
        let y = a*a*a*7.0 + 3.0*a*a*u*7.0 + 3.0*a*u*u*(-3.0) + u*u*u*(-3.0);
        point(x, y)
    }).collect();
    curve.push(point(-3.5, -3.0));

    let shadow = Color32::from_rgb(13, 28, 44);
    let shadow_curve: Vec<Pos2> = curve.iter().map(|pos| *pos + vec2(0.0, 0.8)).collect();
    p.add(egui::Shape::line(shadow_curve, Stroke::new(4.4, shadow)));
    p.add(egui::Shape::line(curve, Stroke::new(2.5, color)));

    let tip = point(-9.0, -3.0);
    let head = vec![tip, point(-3.5, -7.0), point(-3.5, 1.0)];
    p.add(egui::Shape::convex_polygon(
        head.iter().map(|&pos| pos + vec2(0.0, 0.8)).collect(),
        shadow,
        Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(head, color, Stroke::new(0.6, shadow)));
    p.circle_filled(point(4.0, 7.0), 1.25, color);
}

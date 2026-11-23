//! Drawing variants, auxiliary guides and geometric repetitions in the manual workspace.
use super::*;
use crate::drawing_variants::DrawingVariant;
use crate::patterns::{PatternKind, PatternOptions};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MirrorOrientation {
    Free,
    Horizontal,
    Vertical,
}

impl MirrorOrientation {
    fn constrain(self, anchor: Point, mut point: Point) -> Point {
        match self {
            Self::Free => {}
            Self::Horizontal => point.y = anchor.y,
            Self::Vertical => point.x = anchor.x,
        }
        point
    }
}

pub(super) struct PatternDialog {
    kind: PatternKind,
    options: PatternOptions,
    source_ids: Vec<u64>,
    capture_id: String,
    pub(super) picking: Option<u8>, // 0: center; 1/2: mirror endpoints; 3: existing segment
    mirror_axis_defined: bool,
    mirror_segment_id: Option<u64>,
    mirror_orientation: MirrorOrientation,
    axis_handle: Option<(usize, Vec2)>,
    selected_copy: usize,
    copy_offsets: std::collections::BTreeMap<usize, Point>,
    copy_scales: std::collections::BTreeMap<usize, f64>,
    copy_drag: Option<(usize, Point, Point)>,
    show_preview: bool,
    preview: Vec<Measurement>,
    note: String,
    pub(super) previous_selection_mode: bool,
}

impl PatternDialog {
    fn set_mirror_segment(&mut self, segment: &Measurement) {
        self.mirror_segment_id = Some(segment.id);
        self.mirror_axis_defined = true;
        self.axis_handle = None;
        self.picking = None;
        self.preview.clear();
        self.note.clear();
    }
}

fn pattern_button(ui: &mut egui::Ui, label: &str, icon: UiIcon, selected: bool) -> egui::Response {
    ui_widgets::compact_button(ui, label, Some(icon),
        if selected { Tone::Primary } else { Tone::Secondary }, egui::Vec2::ZERO)
}

fn pattern_quantity(ui: &mut egui::Ui, quantity: &mut u32, min: u32, max: u32) -> bool {
    let before = *quantity;
    ui.horizontal(|ui| {
        if ui.add_enabled(*quantity > min, egui::Button::new("−")).clicked() {
            *quantity -= 1;
        }
        ui.add_sized([70.0, 26.0], egui::DragValue::new(quantity).range(min..=max));
        if ui.add_enabled(*quantity < max, egui::Button::new("+")).clicked() {
            *quantity += 1;
        }
    });
    *quantity != before
}

impl DperMeasureApp {
    pub(super) fn choose_drawing_variant(&mut self, variant: DrawingVariant) {
        if !self.pending.is_empty() || self.calibration_mode || self.measurement_gesture_active() {
            self.message = "Terminez ou annulez le tracé avant de changer de méthode.".into();
            return;
        }
        self.choose_measurement_tool(variant.base_tool());
        self.drawing_variant = variant;
        self.right_panel_annotations = false;
        self.message = variant.instruction().into();
    }

    pub(super) fn set_construction_mode(&mut self, enabled: bool) {
        if !self.pending.is_empty() || self.calibration_mode || self.measurement_gesture_active() {
            self.message = "Terminez ou annulez le tracé avant de changer sa nature.".into();
            return;
        }
        self.construction_mode = enabled;
        self.selection_mode = false;
        self.pan_mode = false;
        self.message = if enabled { "Construction : tracés violets pointillés, sans verdict de mesure." }
            else { "Mesure : tracés cyan pleins." }.into();
    }

    pub(super) fn guide_center(measurement: &Measurement) -> Option<Point> {
        let points = &measurement.points;
        match measurement.kind {
            Tool::Circle | Tool::Ellipse | Tool::Coordinate => points.first().copied(),
            Tool::CircleFit => geometry::fit_circle(points).map(|fit| fit.center),
            Tool::Arc => geometry::arc_geometry(points).map(|arc| arc.center),
            Tool::Distance | Tool::Rectangle if points.len() >= 2 => Some(Point {
                x: (points[0].x + points[1].x) * 0.5, y: (points[0].y + points[1].y) * 0.5,
            }),
            Tool::Polygon if !points.is_empty() && measurement.drawing_variant.as_deref().is_some_and(|name| name.starts_with("Rectangle")) => {
                let min_x = points.iter().map(|point| point.x).fold(f64::INFINITY, f64::min);
                let max_x = points.iter().map(|point| point.x).fold(f64::NEG_INFINITY, f64::max);
                let min_y = points.iter().map(|point| point.y).fold(f64::INFINITY, f64::min);
                let max_y = points.iter().map(|point| point.y).fold(f64::NEG_INFINITY, f64::max);
                Some(Point { x: min_x + (max_x-min_x)*0.5, y: min_y + (max_y-min_y)*0.5 })
            }
            _ => None,
        }
    }

    fn guide_path(measurement: &Measurement) -> Vec<Point> {
        if let Some(curve) = geometry::sampled_curve(measurement.kind, &measurement.points, 192) { return curve; }
        let points = &measurement.points;
        if measurement.kind == Tool::Rectangle && points.len() >= 2 {
            let (a, b) = (points[0], points[1]);
            return vec![a, Point { x: b.x, y: a.y }, b, Point { x: a.x, y: b.y }, a];
        }
        let mut path = points.clone();
        if measurement.kind == Tool::Polygon && points.len() >= 3 { path.push(points[0]); }
        path
    }

    pub(super) fn paint_dashed_geometry(painter: &egui::Painter, rect: Rect, w: f32, h: f32,
        measurement: &Measurement, color: Color32, width: f32) {
        let points = Self::guide_path(measurement).into_iter()
            .map(|point| Self::point_to_screen(point, rect, w, h)).collect::<Vec<_>>();
        let mut phase = 0.0_f32;
        let mut fragments = 0_usize;
        for pair in points.windows(2) {
            let delta = pair[1] - pair[0];
            let length = delta.length();
            if !length.is_finite() || length <= 0.001 { continue; }
            let direction = delta / length;
            let mut offset = 0.0;
            while offset < length {
                if fragments >= 32768 { return; }
                fragments += 1;
                let cycle = phase.rem_euclid(15.0);
                let step = (if cycle < 9.0 { 9.0 - cycle } else { 15.0 - cycle })
                    .min(length - offset).max(0.001);
                if cycle < 9.0 {
                    measurement_graphics::segment(painter, pair[0] + direction * offset,
                        pair[0] + direction * (offset + step).min(length), color, width);
                }
                offset += step;
                phase += step;
            }
        }
    }

    pub(super) fn render_construction(&self, painter: &egui::Painter, rect: Rect, w: f32, h: f32,
        measurement: &Measurement, selected: bool) {
        let color = if selected { Color32::from_rgb(235, 158, 255) } else { Color32::from_rgb(190, 110, 255) };
        Self::paint_dashed_geometry(painter, rect, w, h, measurement, color, if selected { 4.0 } else { 3.0 });
        if let Some(center) = Self::guide_center(measurement) {
            let center = Self::point_to_screen(center, rect, w, h);
            for delta in [Vec2::new(11.0, 0.0), Vec2::new(0.0, 11.0)] {
                measurement_graphics::dashed_segment(painter, center - delta, center + delta, color, 2.0, 4.0, 3.0);
            }
            Self::paint_vertex(painter, center, color, 3.5);
        }
        if measurement.kind == Tool::Rectangle && measurement.points.len() >= 2 {
            let path = Self::guide_path(measurement);
            for (a, b) in [(path[0], path[2]), (path[1], path[3])] {
                measurement_graphics::dashed_segment(painter, Self::point_to_screen(a, rect, w, h),
                    Self::point_to_screen(b, rect, w, h), color, 2.0, 7.0, 5.0);
            }
        } else if measurement.kind == Tool::Polygon && measurement.points.len() == 4 {
            for (a, b) in [(0, 2), (1, 3)] {
                measurement_graphics::dashed_segment(painter,
                    Self::point_to_screen(measurement.points[a], rect, w, h),
                    Self::point_to_screen(measurement.points[b], rect, w, h), color, 2.0, 7.0, 5.0);
            }
        }
        if selected {
            for handle in Self::edit_handles(measurement, rect, w, h) {
                Self::paint_vertex(painter, handle.position, color, 4.5);
            }
        }
    }

    pub(super) fn construction_snap(&self, point: Point) -> Option<(Point, Vec<u64>)> {
        if !self.construction_snap_enabled { return None; }
        let mut candidates = Vec::new();
        let mut segments = Vec::new();
        for guide in self.measurements.iter().filter(|m| m.is_construction && m.capture_id == self.capture_id
            && !m.dependency_stale && !self.inspection.dependencies.is_stale(m.id)) {
            candidates.extend(guide.points.iter().copied().map(|p| (p, vec![guide.id])));
            candidates.extend(Self::guide_center(guide).map(|p| (p, vec![guide.id])));
            if matches!(guide.kind, Tool::Distance | Tool::Polyline | Tool::Rectangle | Tool::Polygon) {
                let path = Self::guide_path(guide);
                for pair in path.windows(2).take(128_usize.saturating_sub(segments.len())) {
                    segments.push((pair[0], pair[1], guide.id));
                }
            }
        }
        for (index, &(a, b, first)) in segments.iter().enumerate() {
            for &(c, d, second) in &segments[index + 1..] {
                let (dx, dy, ex, ey) = (b.x-a.x, b.y-a.y, d.x-c.x, d.y-c.y);
                let denominator = dx*ey-dy*ex;
                if denominator.abs() < 1e-9 { continue; }
                let t = ((c.x-a.x)*ey-(c.y-a.y)*ex)/denominator;
                let u = ((c.x-a.x)*dy-(c.y-a.y)*dx)/denominator;
                if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                    let sources = if first == second { vec![first] } else { vec![first, second] };
                    candidates.push((Point { x: a.x+t*dx, y: a.y+t*dy }, sources));
                }
            }
        }
        candidates.into_iter().filter(|(candidate, _)| candidate.dist(point) <= self.edge_snap_radius.max(5) as f64)
            .min_by(|a, b| a.0.dist(point).total_cmp(&b.0.dist(point)))
    }

    pub(super) fn open_pattern(&mut self, kind: PatternKind) {
        if self.registration_photo_pick_owns_frame() || self.video_recording.is_some()
            || self.timelapse_capture.is_some() {
            self.message = "Terminez la sélection ou l’acquisition avant de créer une répétition.".into();
            return;
        }
        if !self.pending.is_empty() || self.calibration_mode || self.measurement_gesture_active() || self.image.is_none() {
            self.message = "Terminez le tracé avant de créer une répétition.".into(); return;
        }
        if self.running && !self.paused { self.freeze_capture(); }
        let sources = self.measurements.iter().filter(|m| m.capture_id == self.capture_id && self.measurement_is_selected(m.id)).collect::<Vec<_>>();
        if sources.is_empty() { self.message = "Sélectionnez les mesures ou constructions à répéter.".into(); return; }
        let points = sources.iter().flat_map(|source| source.points.iter()).collect::<Vec<_>>();
        if points.is_empty() { self.message = "Les tracés sélectionnés ne contiennent aucun point.".into(); return; }
        let center = Point { x: points.iter().map(|p| p.x).sum::<f64>() / points.len() as f64,
            y: points.iter().map(|p| p.y).sum::<f64>() / points.len() as f64 };
        let bounds = |x_axis: bool| points.iter().map(|p| if x_axis { p.x } else { p.y })
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), v| (min.min(v), max.max(v)));
        let (left, right) = bounds(true);
        let (top, bottom) = bounds(false);
        let spacing = sources.iter().filter_map(|source| {
            if source.kind == Tool::Circle { Some(2.0 * source.points.first()?.dist(*source.points.get(1)?)) }
            else { None }
        }).fold(24.0, f64::max);
        let circular_offset = spacing.max(bottom - top) * 1.2;
        let height = self.image.as_ref().map_or(0.0, |image| f64::from(image.height));
        let circular_y = if center.y <= height * 0.5 {
            center.y + circular_offset
        } else {
            center.y - circular_offset
        };
        let circular_center = Point { x: center.x, y: circular_y.clamp(0.0, height) };
        let mut options = PatternOptions { center, axis_start: center,
            axis_end: Point { x: center.x, y: center.y + 100.0 }, ..PatternOptions::default() };
        if kind != PatternKind::Mirror {
            options.center = circular_center;
            options.step_x = (right - left).max(spacing) * 1.2;
            options.step_y = (bottom - top).max(spacing) * 1.2;
        }
        self.pattern_dialog = Some(PatternDialog {
            kind, options,
            source_ids: sources.iter().map(|source| source.id).collect(), capture_id: self.capture_id.clone(),
            picking: None, preview: Vec::new(), note: String::new(),
            mirror_axis_defined: false, mirror_segment_id: None, axis_handle: None,
            mirror_orientation: MirrorOrientation::Free,
            selected_copy: 0, copy_offsets: Default::default(), copy_scales: Default::default(), copy_drag: None,
            show_preview: true,
            previous_selection_mode: self.selection_mode,
        });
        self.pan_mode = false;
        self.discard_annotation_draft();
        self.selection_mode = true;
    }

    pub(super) fn handle_pattern_pick(&mut self, point: Point) -> bool {
        let Some(dialog) = self.pattern_dialog.as_mut() else { return false; };
        let Some(picking) = dialog.picking else { return false; };
        dialog.preview.clear();
        match picking {
            0 => { dialog.options.center = point; dialog.picking = None; }
            1 => { dialog.options.axis_start = point; dialog.options.axis_end = point; dialog.picking = Some(2); }
            2 => { dialog.options.axis_end = point; dialog.mirror_axis_defined = true; dialog.picking = None; }
            _ => return false,
        }
        if dialog.picking.is_none() { self.selection_mode = true; }
        true
    }

    fn mirror_endpoints(&self, dialog: &PatternDialog) -> Option<[Point; 2]> {
        if let Some(id) = dialog.mirror_segment_id {
            let line = self.measurements.iter().find(|line| line.id == id
                && line.capture_id == dialog.capture_id && line.kind == Tool::Distance
                && geometry::is_nondegenerate(line.kind, &line.points))?;
            return Some([line.points[0], line.points[1]]);
        }
        let start = dialog.options.axis_start;
        Some([start, dialog.mirror_orientation.constrain(start, dialog.options.axis_end)])
    }

    pub(super) fn route_mirror_axis(&mut self, ui: &egui::Ui, response: &egui::Response, rect: Rect, w: f32, h: f32) -> bool {
        let Some(dialog) = self.pattern_dialog.as_mut() else { return false; };
        if dialog.kind != PatternKind::Mirror || dialog.capture_id != self.capture_id {
            return false;
        }
        if dialog.picking == Some(3) {
            if let Some(pointer) = response.interact_pointer_pos()
                .filter(|pointer| response.clicked_by(egui::PointerButton::Primary) && rect.contains(*pointer))
            {
                let closest = self.measurements.iter().filter(|line| {
                    line.capture_id == dialog.capture_id && line.kind == Tool::Distance
                        && geometry::is_nondegenerate(line.kind, &line.points)
                }).map(|line| {
                    let [a, b] = [line.points[0], line.points[1]].map(|p| Self::point_to_screen(p, rect, w, h));
                    (line, Self::distance_to_segment(pointer, a, b))
                }).filter(|(_, distance)| *distance <= 9.0).min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((line, _)) = closest {
                    dialog.set_mirror_segment(line);
                    self.selection_mode = true;
                } else {
                    dialog.note = "Cliquez sur un segment de mesure ou de construction.".into();
                }
            }
            return true;
        }
        if dialog.picking.is_some() || dialog.mirror_segment_id.is_some() || !dialog.mirror_axis_defined {
            return false;
        }
        let start = dialog.options.axis_start;
        let end = dialog.mirror_orientation.constrain(start, dialog.options.axis_end);
        let (pressed, released) = ui.input(|input| (input.pointer.primary_pressed(), input.pointer.primary_released()));
        if pressed {
            if let Some(pointer) = response.interact_pointer_pos() {
                dialog.axis_handle = [start, end].into_iter().enumerate()
                    .map(|(index, point)| (index, Self::point_to_screen(point, rect, w, h) - pointer))
                    .filter(|(_, offset)| offset.length() <= 9.0)
                    .min_by(|a, b| a.1.length_sq().total_cmp(&b.1.length_sq()));
            }
        }
        let Some((handle, offset)) = dialog.axis_handle else { return false; };
        if let Some(pointer) = ui.input(|input| input.pointer.latest_pos()).filter(|_| !pressed) {
            let position = (pointer + offset).clamp(rect.min, rect.max);
            let point = Self::point_from_screen(position, rect, w, h);
            let (current, next) = if handle == 0 {
                (&mut dialog.options.axis_start, dialog.mirror_orientation.constrain(end, point))
            } else {
                (&mut dialog.options.axis_end, dialog.mirror_orientation.constrain(start, point))
            };
            if current.dist(next) > 1e-4 {
                *current = next;
                ui.ctx().request_repaint();
            }
        }
        if released || !ui.input(|input| input.pointer.primary_down()) { dialog.axis_handle = None; }
        true
    }

    pub(super) fn live_pattern_source_edit(&self) -> bool {
        self.pattern_dialog.as_ref().is_some_and(|dialog|
            dialog.capture_id == self.capture_id && dialog.picking.is_none())
    }

    pub(super) fn begin_pattern_source_edit(&mut self, pointer: Pos2, rect: Rect, w: f32, h: f32,
        painter: &egui::Painter) {
        let Some(dialog) = self.pattern_dialog.as_ref() else { return; };
        let closest = self.measurements.iter()
            .filter(|m| m.capture_id == dialog.capture_id && dialog.source_ids.contains(&m.id))
            .filter_map(|m| {
                let distance = Self::nearest_edit_handle(m, pointer, rect, w, h)
                    .map(|(_, distance)| distance)
                    .or_else(|| self.measurement_label_card_rect(painter, rect, w, h, m)
                        .filter(|card| card.contains(pointer)).map(|card| card.center().distance(pointer)))?;
                Some((m.id, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((id, _)) = closest {
            self.begin_point_edit_for(id, pointer, rect, w, h, painter);
        }
    }

    #[cfg(test)]
    pub(super) fn live_pattern_preview(&self) -> &[Measurement] {
        self.pattern_dialog.as_ref().map_or(&[], |dialog| dialog.preview.as_slice())
    }

    fn pattern_copy_anchor(copy: &Measurement) -> Point {
        Self::guide_center(copy).unwrap_or_else(|| Point {
            x: copy.points.iter().map(|p| p.x).sum::<f64>() / copy.points.len() as f64,
            y: copy.points.iter().map(|p| p.y).sum::<f64>() / copy.points.len() as f64,
        })
    }

    pub(super) fn route_pattern_copy(&mut self, ui: &egui::Ui, response: &egui::Response, rect: Rect, w: f32, h: f32) -> bool {
        if !self.live_pattern_source_edit() { return false; }
        let dialog = self.pattern_dialog.as_mut().unwrap();
        if dialog.kind == PatternKind::Mirror || !dialog.show_preview { return false; }
        if ui.input(|input| input.pointer.primary_pressed()) {
            if let Some(pointer) = response.interact_pointer_pos().filter(|p| rect.contains(*p)) {
                let nearest = dialog.preview.iter().enumerate()
                    .map(|(index, copy)| {
                        (index, Self::point_to_screen(Self::pattern_copy_anchor(copy), rect, w, h).distance(pointer))
                    }).filter(|(_, distance)| *distance <= 10.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((index, _)) = nearest {
                    dialog.selected_copy = index;
                    let start = Self::point_from_screen(pointer, rect, w, h);
                    let offset = dialog.copy_offsets.get(&index).copied().unwrap_or_default();
                    dialog.copy_drag = Some((index, start, offset));
                }
            }
        }
        let Some((index, start, offset)) = dialog.copy_drag else { return false; };
        if let Some(pointer) = ui.input(|input| input.pointer.latest_pos()) {
            let point = Self::point_from_screen(pointer.clamp(rect.min, rect.max), rect, w, h);
            dialog.copy_offsets.insert(index, Point { x: offset.x + point.x - start.x, y: offset.y + point.y - start.y });
        }
        if !ui.input(|input| input.pointer.primary_down()) { dialog.copy_drag = None; }
        true
    }

    pub(super) fn paint_pattern_preview(&self, painter: &egui::Painter, rect: Rect, w: f32, h: f32) {
        let Some(dialog) = self.pattern_dialog.as_ref() else { return; };
        if dialog.capture_id != self.capture_id { return; }
        if dialog.kind == PatternKind::Mirror && (dialog.mirror_axis_defined || dialog.picking == Some(2)) {
            let Some([start, end]) = self.mirror_endpoints(dialog) else { return; };
            let first = Self::point_to_screen(start, rect, w, h);
            let end = if dialog.picking == Some(2) {
                painter.ctx().pointer_hover_pos().filter(|pointer| rect.contains(*pointer))
                    .map_or(first, |pointer| {
                        let point = Self::point_from_screen(pointer, rect, w, h);
                        Self::point_to_screen(dialog.mirror_orientation.constrain(start, point), rect, w, h)
                    })
            } else {
                Self::point_to_screen(end, rect, w, h)
            };
            let color = Color32::from_rgb(207, 159, 255);
            painter.line_segment([first, end], Stroke::new(5.0, Color32::from_rgb(32, 26, 52)));
            painter.line_segment([first, end], Stroke::new(2.0, color));
            for point in [first, end] {
                painter.circle(point, 6.0, Color32::from_rgb(32, 26, 52), Stroke::new(2.0, color));
            }
        }
        if !dialog.show_preview { return; }
        for (index, copy) in dialog.preview.iter().enumerate() {
            let selected = dialog.kind != PatternKind::Mirror && index == dialog.selected_copy;
            let color = if selected { Color32::from_rgb(104, 226, 255) } else { Color32::from_rgb(111, 244, 172) };
            Self::paint_dashed_geometry(painter, rect, w, h, copy, color, 2.0);
            if dialog.kind != PatternKind::Mirror {
                let anchor = Self::point_to_screen(Self::pattern_copy_anchor(copy), rect, w, h);
                painter.circle(anchor, 5.0, Color32::from_rgb(23, 38, 55), Stroke::new(2.0, color));
            }
        }
    }

    pub(super) fn patterns_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.pattern_dialog.take() else { return; };
        let mut open = true;
        let mut apply = false;
        let mut changed = false;
        let mut cancel = false;
        let live = dialog.kind != PatternKind::Mirror;
        let title = match dialog.kind {
            PatternKind::Mirror => "Symétrie miroir",
            PatternKind::Rectangular => "Répétition rectangulaire",
            PatternKind::Circular => "Répétition circulaire",
        };
        let frame = egui::Frame::window(&ctx.style())
            .fill(Color32::from_rgb(247, 250, 251))
            .stroke(Stroke::new(1.0, Color32::from_rgb(143, 167, 179)))
            .inner_margin(egui::Margin::same(13));
        egui::Window::new(title).id(egui::Id::new("drawing-pattern-dialog")).open(&mut open)
            .frame(frame).default_width(335.0).collapsible(false).resizable(false).show(ctx, |ui| {
                *ui.visuals_mut() = dper_visuals();
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.set_min_width(335.0);
                match dialog.kind {
                    PatternKind::Rectangular => {
                        egui::Grid::new("rectangular-pattern-fields").spacing([12.0, 7.0]).show(ui, |ui| {
                            ui.strong("Direction X");
                            ui.weak("Horizontale");
                            ui.end_row();
                            ui.label("Colonnes");
                            let columns = pattern_quantity(ui, &mut dialog.options.cols, 1, 100);
                            ui.end_row();
                            ui.label("Espacement");
                            changed |= ui.add_sized([132.0, 25.0], egui::DragValue::new(&mut dialog.options.step_x).suffix(" px")).changed();
                            ui.end_row();
                            ui.strong("Direction Y");
                            ui.weak("Verticale");
                            ui.end_row();
                            ui.label("Lignes");
                            let rows = pattern_quantity(ui, &mut dialog.options.rows, 1, 100);
                            ui.end_row();
                            ui.label("Espacement");
                            changed |= ui.add_sized([132.0, 25.0], egui::DragValue::new(&mut dialog.options.step_y).suffix(" px")).changed();
                            ui.end_row();
                            if columns || rows {
                                dialog.copy_offsets.clear();
                                dialog.copy_scales.clear();
                                dialog.selected_copy = 0;
                                changed = true;
                            }
                        });
                    }
                    PatternKind::Circular => {
                        egui::Grid::new("circular-pattern-fields").spacing([12.0, 7.0]).show(ui, |ui| {
                            ui.label("Centre");
                            if pattern_button(ui, "Choisir le centre sur l’image", UiIcon::Crosshair, dialog.picking == Some(0)).clicked() {
                                dialog.picking = Some(0);
                                self.selection_mode = false;
                            }
                            ui.end_row();
                            ui.label("Quantité totale");
                            changed |= pattern_quantity(ui, &mut dialog.options.count, 2, 1000);
                            ui.end_row();
                            ui.label("Angle total");
                            changed |= ui.add_sized([132.0, 25.0], egui::DragValue::new(&mut dialog.options.angle_degrees).range(-360.0..=360.0).suffix(" °")).changed();
                            ui.end_row();
                        });
                        ui.collapsing("Coordonnées du centre", |ui| {
                            ui.horizontal(|ui| {
                                changed |= ui.add(egui::DragValue::new(&mut dialog.options.center.x).prefix("X  ").suffix(" px")).changed();
                                changed |= ui.add(egui::DragValue::new(&mut dialog.options.center.y).prefix("Y  ").suffix(" px")).changed();
                            });
                        });
                    }
                    PatternKind::Mirror => {
                        ui.strong("Axe de symétrie");
                        let selected = dialog.mirror_segment_id.and_then(|id| self.measurements.iter().find(|m| m.id == id));
                        let selected_label = selected.map(|m| m.label.as_str()).unwrap_or(if dialog.mirror_axis_defined { "Axe tracé" } else { "Choisir un segment existant" });
                        let mut selected_id = dialog.mirror_segment_id;
                        egui::ComboBox::from_id_salt("mirror-axis-segment").width(235.0)
                            .selected_text(selected_label).show_ui(ui, |ui| {
                                ui.selectable_value(&mut selected_id, None, "Aucun segment existant");
                                for line in self.measurements.iter().filter(|m| {
                                    m.capture_id == dialog.capture_id && m.kind == Tool::Distance
                                        && geometry::is_nondegenerate(m.kind, &m.points)
                                }) {
                                    let kind = if line.is_construction { "Construction" } else { "Mesure" };
                                    ui.selectable_value(&mut selected_id, Some(line.id), format!("{kind} · {}", line.label));
                                }
                            });
                        if selected_id != dialog.mirror_segment_id {
                            dialog.mirror_segment_id = selected_id;
                            dialog.picking = None;
                            dialog.mirror_axis_defined = false;
                            dialog.axis_handle = None;
                            if let Some(segment) = self.measurements.iter().find(|line| Some(line.id) == selected_id) {
                                dialog.set_mirror_segment(segment);
                            }
                            self.selection_mode = true;
                            changed = true;
                        }
                        ui.horizontal_wrapped(|ui| {
                            if pattern_button(ui, "Tracer l’axe par deux clics", UiIcon::Distance, matches!(dialog.picking, Some(1 | 2))).clicked() {
                                dialog.mirror_segment_id = None;
                                dialog.mirror_axis_defined = false;
                                dialog.axis_handle = None;
                                dialog.picking = Some(1);
                                self.selection_mode = false;
                                changed = true;
                            }
                            if pattern_button(ui, "Choisir un segment sur l’image", UiIcon::Select, dialog.picking == Some(3)).clicked() {
                                dialog.picking = Some(3);
                                dialog.axis_handle = None;
                                self.selection_mode = false;
                                changed = true;
                            }
                        });
                        if dialog.mirror_segment_id.is_none() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Orientation");
                                for (value, label) in [
                                    (MirrorOrientation::Free, "Libre"),
                                    (MirrorOrientation::Horizontal, "Horizontal"),
                                    (MirrorOrientation::Vertical, "Vertical"),
                                ] {
                                    changed |= ui.selectable_value(&mut dialog.mirror_orientation, value, label).changed();
                                }
                            });
                        }
                        if dialog.mirror_axis_defined && dialog.picking.is_none() {
                            ui.small("Glissez une poignée du tracé source pour actualiser son reflet.");
                        }
                    }
                }
                if live {
                    ui.separator();
                    ui.collapsing("Ajuster une copie", |ui| {
                    ui.small("Glissez un repère cyan ou vert sur l’image pour déplacer une copie.");
                    if !dialog.preview.is_empty() {
                        dialog.selected_copy = dialog.selected_copy.min(dialog.preview.len() - 1);
                        egui::ComboBox::from_id_salt("pattern-copy-list").width(235.0)
                            .selected_text(&dialog.preview[dialog.selected_copy].label).show_ui(ui, |ui| {
                                for (index, copy) in dialog.preview.iter().enumerate() {
                                    ui.selectable_value(&mut dialog.selected_copy, index, &copy.label);
                                }
                            });
                        let index = dialog.selected_copy;
                        let mut offset = dialog.copy_offsets.get(&index).copied().unwrap_or_default();
                        let mut size = dialog.copy_scales.get(&index).copied().unwrap_or(1.0) * 100.0;
                        ui.horizontal_wrapped(|ui| {
                            let x = ui.add(egui::DragValue::new(&mut offset.x).prefix("ΔX  ").suffix(" px")).changed();
                            let y = ui.add(egui::DragValue::new(&mut offset.y).prefix("ΔY  ").suffix(" px")).changed();
                            if x || y { dialog.copy_offsets.insert(index, offset); }
                            if ui.add(egui::DragValue::new(&mut size).range(1.0..=1000.0).prefix("Taille  ").suffix(" %")).changed() {
                                dialog.copy_scales.insert(index, size / 100.0);
                            }
                        });
                        if ui.button("Réinitialiser cette copie").clicked() {
                            dialog.copy_offsets.remove(&index);
                            dialog.copy_scales.remove(&index);
                        }
                    }
                    });
                }
                if let Some(picking) = dialog.picking {
                    ui.colored_label(Color32::from_rgb(140, 78, 18), match picking {
                        0 => "Cliquez le centre sur l’image.",
                        1 => "Cliquez le début de l’axe sur l’image.",
                        2 => "Cliquez la fin de l’axe sur l’image.",
                        _ => "Cliquez sur un segment de mesure ou de construction.",
                    });
                }
                if !dialog.note.is_empty() { ui.label(&dialog.note); }
                ui.separator();
                if ui.checkbox(&mut dialog.show_preview, "Afficher les copies en pointillé").changed() {
                    dialog.copy_drag = None;
                    ctx.request_repaint();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = dialog.picking.is_none() && dialog.axis_handle.is_none()
                        && (dialog.kind != PatternKind::Mirror || dialog.mirror_axis_defined);
                    apply = ui.add_enabled_ui(ready && !changed && !dialog.preview.is_empty()
                        && !self.measurement_gesture_active(), |ui| {
                        pattern_button(ui, "OK", UiIcon::Check, true).clicked()
                    }).inner;
                    cancel = pattern_button(ui, "Annuler", UiIcon::Close, false).clicked();
                });
            });
        if changed { dialog.preview.clear(); dialog.note.clear(); ctx.request_repaint(); }
        if cancel || !open { self.selection_mode = dialog.previous_selection_mode; return; }
        if live || (dialog.mirror_axis_defined && dialog.picking.is_none()) {
            if dialog.capture_id != self.capture_id {
                dialog.preview.clear();
                dialog.note = "L’image a changé. Rouvrez l’outil sur l’image active.".into();
            } else {
                let mut dependencies = dialog.source_ids.clone();
                if let Some(id) = dialog.mirror_segment_id.filter(|id| !dependencies.contains(id)) {
                    dependencies.push(id);
                }
                let templates: Vec<_> = dialog.source_ids.iter().copied()
                    .filter(|id| Some(*id) != dialog.mirror_segment_id).collect();
                if let Err(error) = self.validate_dependency_sources_with_templates(
                    &dialog.capture_id, &dependencies, &templates,
                ) {
                    dialog.note = error;
                    dialog.preview.clear();
                    self.pattern_dialog = Some(dialog);
                    return;
                }
                let sources = self.measurements.iter().filter(|m| dialog.source_ids.contains(&m.id) && m.capture_id == dialog.capture_id)
                    .cloned().collect::<Vec<_>>();
                let mut options = dialog.options.clone();
                if dialog.kind == PatternKind::Mirror {
                    let Some([start, end]) = self.mirror_endpoints(&dialog) else {
                        dialog.preview.clear();
                        dialog.note = "Choisissez un segment d’axe valide.".into();
                        self.pattern_dialog = Some(dialog);
                        return;
                    };
                    options.axis_start = start;
                    options.axis_end = end;
                }
                match crate::patterns::generate(&sources, dialog.kind, &options) {
                    Ok(mut copies) => {
                        for (index, copy) in copies.iter_mut().enumerate() {
                            if let Some(scale) = dialog.copy_scales.get(&index) {
                                let anchor = Self::pattern_copy_anchor(copy);
                                for point in copy.points.iter_mut().chain(copy.label_position_native_px.iter_mut()) {
                                    *point = Point {
                                        x: anchor.x + (point.x - anchor.x) * scale,
                                        y: anchor.y + (point.y - anchor.y) * scale,
                                    };
                                }
                            }
                            if let Some(&offset) = dialog.copy_offsets.get(&index) {
                                *copy = Self::translated_measurement(copy, offset);
                            }
                        }
                        let outside = copies.iter().any(|copy| {
                            copy.points.iter().any(|point| !point.x.is_finite() || !point.y.is_finite())
                                || self.image.as_ref().is_some_and(|image| geometry::exceeds_image_bounds(
                                    copy.kind, &copy.points, image.width, image.height))
                        });
                        dialog.note = if outside { "Des copies dépassent l’image. Ajustez les paramètres avant de les créer.".into() }
                            else if dialog.show_preview && copies.len() > 200 { format!("{} copies · 200 visibles dans l’aperçu", copies.len()) }
                            else if copies.iter().any(|copy| copy.dependency_stale) {
                                format!("{} copie(s) à créer · contrôle suspendu pour les sources à recalculer", copies.len())
                            }
                            else { format!("{} copie(s) à créer", copies.len()) };
                        if apply && !outside {
                            let selection = self.measurement_selection_ids();
                            let mut added = Vec::new();
                            for mut copy in copies {
                                if let Some(id) = dialog.mirror_segment_id.filter(|id| !copy.source_measurement_ids.contains(id)) {
                                    copy.source_measurement_ids.push(id);
                                }
                                copy.id = self.next_id; self.next_id += 1;
                                copy.label = format!("M{} · {}", copy.id, copy.label);
                                let index = self.measurements.len();
                                self.measurements.push(copy.clone());
                                added.push((index, copy));
                            }
                            self.set_measurements_selection(added.iter().map(|(_, m)| m.id).collect());
                            let count = added.len();
                            let current_ids = added.iter().filter(|(_, measurement)| !measurement.dependency_stale)
                                .map(|(_, measurement)| measurement.id).collect::<Vec<_>>();
                            self.record_measurement_action(MeasurementHistoryAction::AddedMany { measurements: added, selection });
                            for id in current_ids { self.inspection.dependencies.mark_created(id); }
                            self.refresh_dependency_flags();
                            self.selection_mode = true;
                            self.message = format!("{count} copies créées. Vérifiez et ajustez leurs contours sur les pixels.");
                            return;
                        }
                        let next_preview: Vec<_> = copies.into_iter().take(200).collect();
                        if next_preview.len() != dialog.preview.len()
                            || next_preview.iter().zip(&dialog.preview)
                                .any(|(next, old)| next.points != old.points || next.kind != old.kind)
                        {
                            ctx.request_repaint();
                        }
                        dialog.preview = next_preview;
                    }
                    Err(error) => { dialog.note = error; dialog.preview.clear(); }
                }
            }
        }
        self.pattern_dialog = Some(dialog);
    }
}

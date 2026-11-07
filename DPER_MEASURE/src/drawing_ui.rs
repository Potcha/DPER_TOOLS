//! Drawing variants, auxiliary guides and geometric repetitions in the manual workspace.
use super::*;
use crate::drawing_variants::DrawingVariant;
use crate::patterns::{PatternKind, PatternOptions};

pub(super) struct PatternDialog {
    kind: PatternKind,
    options: PatternOptions,
    source_ids: Vec<u64>,
    capture_id: String,
    revision: u64,
    pub(super) picking: Option<u8>, // 0: center; 1/2: mirror axis endpoints
    preview: Vec<Measurement>,
    note: String,
    pub(super) previous_selection_mode: bool,
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
        self.pattern_dialog = Some(PatternDialog {
            kind, options: PatternOptions { center, axis_start: center,
                axis_end: Point { x: center.x, y: center.y + 100.0 }, ..PatternOptions::default() },
            source_ids: sources.iter().map(|source| source.id).collect(), capture_id: self.capture_id.clone(),
            revision: self.document_revision, picking: None, preview: Vec::new(), note: String::new(),
            previous_selection_mode: self.selection_mode,
        });
        self.pan_mode = false;
        self.annotation_mode = false;
        self.selection_mode = true;
    }

    pub(super) fn handle_pattern_pick(&mut self, point: Point) -> bool {
        let Some(dialog) = self.pattern_dialog.as_mut() else { return false; };
        let Some(picking) = dialog.picking else { return false; };
        dialog.preview.clear();
        match picking {
            0 => { dialog.options.center = point; dialog.picking = None; }
            1 => { dialog.options.axis_start = point; dialog.picking = Some(2); }
            _ => { dialog.options.axis_end = point; dialog.picking = None; }
        }
        if dialog.picking.is_none() { self.selection_mode = dialog.previous_selection_mode; }
        true
    }

    pub(super) fn paint_pattern_preview(&self, painter: &egui::Painter, rect: Rect, w: f32, h: f32) {
        let Some(dialog) = self.pattern_dialog.as_ref() else { return; };
        if dialog.capture_id != self.capture_id || dialog.revision != self.document_revision { return; }
        for copy in &dialog.preview {
            Self::paint_dashed_geometry(painter, rect, w, h, copy, Color32::from_rgb(111, 244, 172), 2.0);
        }
    }

    pub(super) fn patterns_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.pattern_dialog.take() else { return; };
        let mut open = true;
        let mut preview = false;
        let mut apply = false;
        let mut changed = false;
        let title = match dialog.kind { PatternKind::Mirror => "Symétrie miroir", PatternKind::Rectangular => "Répétition rectangulaire", PatternKind::Circular => "Répétition circulaire" };
        egui::Window::new(title).id(egui::Id::new("drawing-pattern-dialog")).open(&mut open)
            .default_width(440.0).resizable(false).show(ctx, |ui| {
                ui.label(format!("{} tracé(s) source · coordonnées et décalages en pixels natifs", dialog.source_ids.len()));
                ui.label("Les copies sont géométriques : ajustez-les ensuite aux contours réels de l’image.");
                match dialog.kind {
                    PatternKind::Rectangular => {
                        ui.horizontal(|ui| {
                            changed |= ui.add(egui::DragValue::new(&mut dialog.options.cols).range(1..=100).prefix("Colonnes : ")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut dialog.options.rows).range(1..=100).prefix("Lignes : ")).changed();
                        });
                        ui.horizontal(|ui| {
                            changed |= ui.add(egui::DragValue::new(&mut dialog.options.step_x).prefix("Pas X : ").suffix(" px")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut dialog.options.step_y).prefix("Pas Y : ").suffix(" px")).changed();
                        });
                    }
                    PatternKind::Circular => {
                        changed |= ui.add(egui::DragValue::new(&mut dialog.options.count).range(2..=1000).prefix("Nombre total : ")).changed();
                        changed |= ui.add(egui::DragValue::new(&mut dialog.options.angle_degrees).range(-360.0..=360.0).prefix("Angle total : ").suffix("°")).changed();
                        ui.horizontal(|ui| {
                            changed |= ui.add(egui::DragValue::new(&mut dialog.options.center.x).prefix("Centre X : ")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut dialog.options.center.y).prefix("Y : ")).changed();
                        });
                        if ui.button("Choisir le centre sur l’image").clicked() { dialog.picking = Some(0); self.selection_mode = false; }
                    }
                    PatternKind::Mirror => {
                        for (label, point) in [("Début de l’axe", &mut dialog.options.axis_start), ("Fin de l’axe", &mut dialog.options.axis_end)] {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                changed |= ui.add(egui::DragValue::new(&mut point.x).prefix("X : ")).changed();
                                changed |= ui.add(egui::DragValue::new(&mut point.y).prefix("Y : ")).changed();
                            });
                        }
                        if ui.button("Définir l’axe par deux clics sur l’image").clicked() { dialog.picking = Some(1); self.selection_mode = false; }
                    }
                }
                if let Some(picking) = dialog.picking {
                    ui.colored_label(Color32::from_rgb(145, 70, 190), match picking {
                        0 => "Cliquez le centre sur l’image.", 1 => "Cliquez le début de l’axe sur l’image.", _ => "Cliquez la fin de l’axe sur l’image.",
                    });
                }
                if !dialog.note.is_empty() { ui.label(&dialog.note); }
                ui.horizontal(|ui| {
                    preview = ui.add_enabled(dialog.picking.is_none(), egui::Button::new("Prévisualiser")).clicked();
                    apply = ui.add_enabled(dialog.picking.is_none(), egui::Button::new("Créer les copies")).clicked();
                });
            });
        if changed { dialog.preview.clear(); dialog.note.clear(); }
        if !open { self.selection_mode = dialog.previous_selection_mode; return; }
        if preview || apply {
            if dialog.capture_id != self.capture_id || dialog.revision != self.document_revision {
                dialog.preview.clear();
                dialog.note = "L’image ou ses tracés ont changé. Fermez cette fenêtre et sélectionnez à nouveau les sources.".into();
            } else {
                if let Err(error) = self.validate_dependency_sources(&dialog.capture_id, &dialog.source_ids) {
                    dialog.note = error;
                    dialog.preview.clear();
                    self.pattern_dialog = Some(dialog);
                    return;
                }
                let sources = self.measurements.iter().filter(|m| dialog.source_ids.contains(&m.id) && m.capture_id == dialog.capture_id)
                    .cloned().collect::<Vec<_>>();
                match crate::patterns::generate(&sources, dialog.kind, &dialog.options) {
                    Ok(copies) => {
                        let outside = self.image.as_ref().is_some_and(|image| copies.iter().any(|m|
                            geometry::exceeds_image_bounds(m.kind, &m.points, image.width, image.height)));
                        dialog.note = if outside { "Des copies dépassent l’image. Ajustez les paramètres avant de les créer.".into() }
                            else { format!("{} copie(s) ; aperçu limité aux 200 premières.", copies.len()) };
                        if apply && !outside {
                            let selection = self.measurement_selection_ids();
                            let mut added = Vec::new();
                            for mut copy in copies {
                                copy.id = self.next_id; self.next_id += 1;
                                copy.label = format!("M{} · {}", copy.id, copy.label);
                                let index = self.measurements.len();
                                self.measurements.push(copy.clone());
                                added.push((index, copy));
                            }
                            self.set_measurements_selection(added.iter().map(|(_, m)| m.id).collect());
                            let count = added.len();
                            let added_ids = added.iter().map(|(_, measurement)| measurement.id).collect::<Vec<_>>();
                            self.record_measurement_action(MeasurementHistoryAction::AddedMany { measurements: added, selection });
                            for id in added_ids { self.inspection.dependencies.mark_created(id); }
                            self.refresh_dependency_flags();
                            self.selection_mode = true;
                            self.message = format!("{count} copies créées. Vérifiez et ajustez leurs contours sur les pixels.");
                            return;
                        }
                        dialog.preview = copies.into_iter().take(200).collect();
                    }
                    Err(error) => { dialog.note = error; dialog.preview.clear(); }
                }
            }
        }
        self.pattern_dialog = Some(dialog);
    }
}

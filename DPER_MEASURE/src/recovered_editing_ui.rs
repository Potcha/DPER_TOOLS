//! Original native editing interactions recovered from the pre-sync local source.
//! Application-owned dependency invalidation and guarded deletion are retained.
use super::*;
use std::collections::BTreeSet;

impl DperMeasureApp {
    pub(super) fn clear_pending(&mut self) {
        self.cancel_selection_gesture();
        self.calibration_point_edit = None;
        self.pending.clear();
        self.pending_snap_enabled = false;
        self.pending_snap_radius = None;
        self.pending_snap_applied.clear();
        self.pending_calibration_id = None;
        self.pending_mm_per_pixel = None;
    }

    pub(super) fn record_measurement_action(&mut self, action: MeasurementHistoryAction) {
        self.display_correction_drag_active = false;
        self.display_history_priority = false;
        self.record_measurement_audit(&action, false);
        self.invalidate_measurement_action(&action);
        self.undo_history.push(action);
        self.redo_history.clear();
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
    }

    pub(super) fn record_measurement_audit(&mut self, action: &MeasurementHistoryAction, inverse: bool) {
        let (label, before, after): (&str, Vec<&Measurement>, Vec<&Measurement>) = match (action, inverse) {
            (MeasurementHistoryAction::AddedMany { measurements, .. }, false) =>
                ("Ajout groupé", Vec::new(), measurements.iter().map(|(_, m)| m).collect()),
            (MeasurementHistoryAction::AddedMany { measurements, .. }, true) =>
                ("Annulation de l’ajout groupé", measurements.iter().map(|(_, m)| m).collect(), Vec::new()),
            (MeasurementHistoryAction::DisplayAdjusted { .. }, _) => return,
            (MeasurementHistoryAction::Added { measurement, .. }, false) => ("Ajout", Vec::new(), vec![measurement]),
            (MeasurementHistoryAction::Added { measurement, .. }, true) => ("Annulation de l’ajout", vec![measurement], Vec::new()),
            (MeasurementHistoryAction::Edited { before, after, .. }, false) => ("Correction", vec![before], vec![after]),
            (MeasurementHistoryAction::Edited { before, after, .. }, true) => ("Annulation de la correction", vec![after], vec![before]),
            (MeasurementHistoryAction::EditedMany { before, after, .. }, false) =>
                ("Correction groupée", before.iter().collect(), after.iter().collect()),
            (MeasurementHistoryAction::EditedMany { before, after, .. }, true) =>
                ("Annulation de la correction groupée", after.iter().collect(), before.iter().collect()),
            (MeasurementHistoryAction::Removed { measurement, .. }, false) => ("Suppression", vec![measurement], Vec::new()),
            (MeasurementHistoryAction::Removed { measurement, .. }, true) => ("Annulation de la suppression", Vec::new(), vec![measurement]),
            (MeasurementHistoryAction::RemovedMany { measurements, .. }, false) =>
                ("Suppression groupée", measurements.iter().map(|(_, measurement)| measurement).collect(), Vec::new()),
            (MeasurementHistoryAction::RemovedMany { measurements, .. }, true) =>
                ("Annulation de la suppression groupée", Vec::new(), measurements.iter().map(|(_, measurement)| measurement).collect()),
            (MeasurementHistoryAction::Cleared { measurements, .. }, false) => ("Effacement", measurements.iter().collect(), Vec::new()),
            (MeasurementHistoryAction::Cleared { measurements, .. }, true) => ("Annulation de l’effacement", Vec::new(), measurements.iter().collect()),
        };
        let snapshot = |measurement: &Measurement| storage::MeasurementAuditValue {
            id: measurement.id,
            label: measurement.label.clone(),
            kind: measurement.kind.label().to_owned(),
            result: geometry::result(measurement),
        };
        let before = before.into_iter().map(snapshot).collect::<Vec<_>>();
        let after = after.into_iter().map(snapshot).collect::<Vec<_>>();
        let mut measurement_ids = before.iter().chain(&after).map(|value| value.id).collect::<Vec<_>>();
        measurement_ids.sort_unstable();
        measurement_ids.dedup();
        self.measurement_audit_history.push(storage::MeasurementAuditEvent {
            timestamp_unix_secs: SystemTime::now().duration_since(UNIX_EPOCH)
                .unwrap_or_default().as_secs(),
            action: label.to_owned(), measurement_ids, before, after,
        });
        if self.measurement_audit_history.len() > MAX_MEASUREMENT_AUDIT_EVENTS {
            let excess = self.measurement_audit_history.len() - MAX_MEASUREMENT_AUDIT_EVENTS;
            self.measurement_audit_history.drain(0..excess);
        }
    }

    pub(super) fn undo_measurement_action(&mut self) -> bool {
        if let Some(action) = self.undo_history.last() {
            let ids = match action {
                MeasurementHistoryAction::Added { measurement, .. } => vec![measurement.id],
                MeasurementHistoryAction::AddedMany { measurements, .. } => measurements.iter().map(|(_, m)| m.id).collect(),
                _ => Vec::new(),
            };
            if let Err(error) = self.ensure_dependency_deletion(&ids) { self.message = error; return false; }
        }
        let Some(mut action) = self.undo_history.pop() else { return false; };
        self.record_measurement_audit(&action, true);
        match &mut action {
            MeasurementHistoryAction::AddedMany { measurements, selection } => {
                self.measurements.retain(|m| !measurements.iter().any(|(_, added)| added.id == m.id));
                self.set_measurements_selection(selection.clone());
            }
            MeasurementHistoryAction::DisplayAdjusted { before, .. } => {
                self.display_correction = *before;
                self.display_texture_dirty = true;
                self.display_history_priority = true;
            }
            MeasurementHistoryAction::Added { measurement, index, previous_selection } => {
                if let Some(position) = self.measurements.iter().position(|m| m.id == measurement.id) {
                    *index = position;
                    *measurement = self.measurements.remove(position);
                }
                self.set_measurement_selection(*previous_selection);
            }
            MeasurementHistoryAction::Edited { id, before, .. } => {
                if let Some(measurement) = self.measurements.iter_mut().find(|m| m.id == *id) {
                    *measurement = before.clone();
                }
                self.set_measurement_selection(Some(*id));
            }
            MeasurementHistoryAction::EditedMany { before, selection, .. } => {
                for saved in before.iter() {
                    if let Some(measurement) = self.measurements.iter_mut().find(|m| m.id == saved.id) {
                        *measurement = saved.clone();
                    }
                }
                self.set_measurements_selection(selection.clone());
            }
            MeasurementHistoryAction::Removed { measurement, index, previous_selection } => {
                let position = (*index).min(self.measurements.len());
                self.measurements.insert(position, measurement.clone());
                self.set_measurement_selection(*previous_selection);
            }
            MeasurementHistoryAction::RemovedMany { measurements, selection } => {
                measurements.sort_by_key(|(index, _)| *index);
                for (index, measurement) in measurements.iter() {
                    let position = (*index).min(self.measurements.len());
                    self.measurements.insert(position, measurement.clone());
                }
                self.set_measurements_selection(selection.clone());
            }
            MeasurementHistoryAction::Cleared { measurements, previous_selection } => {
                self.measurements.clone_from(measurements);
                self.set_measurement_selection(*previous_selection);
            }
        }
        self.invalidate_measurement_action(&action);
        self.redo_history.push(action);
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        true
    }

    pub(super) fn redo_measurement_action(&mut self) -> bool {
        if let Some(action) = self.redo_history.last() {
            let ids = match action {
                MeasurementHistoryAction::Removed { measurement, .. } => vec![measurement.id],
                MeasurementHistoryAction::RemovedMany { measurements, .. } => measurements.iter().map(|(_, m)| m.id).collect(),
                MeasurementHistoryAction::Cleared { .. } => self.measurements.iter().map(|m| m.id).collect(),
                _ => Vec::new(),
            };
            if let Err(error) = self.ensure_dependency_deletion(&ids) { self.message = error; return false; }
        }
        let Some(mut action) = self.redo_history.pop() else { return false; };
        self.record_measurement_audit(&action, false);
        match &mut action {
            MeasurementHistoryAction::AddedMany { measurements, .. } => {
                for (index, measurement) in measurements.iter() {
                    self.measurements.insert((*index).min(self.measurements.len()), measurement.clone());
                }
                self.set_measurements_selection(measurements.iter().map(|(_, m)| m.id).collect());
            }
            MeasurementHistoryAction::DisplayAdjusted { after, .. } => {
                self.display_correction = *after;
                self.display_texture_dirty = true;
                self.display_history_priority = true;
            }
            MeasurementHistoryAction::Added { measurement, index, .. } => {
                let position = (*index).min(self.measurements.len());
                self.measurements.insert(position, measurement.clone());
                self.set_measurement_selection(Some(measurement.id));
            }
            MeasurementHistoryAction::Edited { id, after, .. } => {
                if let Some(measurement) = self.measurements.iter_mut().find(|m| m.id == *id) {
                    *measurement = after.clone();
                }
                self.set_measurement_selection(Some(*id));
            }
            MeasurementHistoryAction::EditedMany { after, selection, .. } => {
                for saved in after.iter() {
                    if let Some(measurement) = self.measurements.iter_mut().find(|m| m.id == saved.id) {
                        *measurement = saved.clone();
                    }
                }
                self.set_measurements_selection(selection.clone());
            }
            MeasurementHistoryAction::Removed { measurement, index, .. } => {
                if let Some(position) = self.measurements.iter().position(|m| m.id == measurement.id) {
                    *index = position;
                    *measurement = self.measurements.remove(position);
                }
                self.set_measurement_selection(None);
            }
            MeasurementHistoryAction::RemovedMany { measurements, selection } => {
                for (_, saved) in measurements.iter() {
                    if let Some(position) = self.measurements.iter().position(|m| m.id == saved.id) {
                        self.measurements.remove(position);
                    }
                }
                self.set_measurements_selection(selection.clone());
            }
            MeasurementHistoryAction::Cleared { .. } => {
                self.measurements.clear();
                self.set_measurement_selection(None);
            }
        }
        self.invalidate_measurement_action(&action);
        self.undo_history.push(action);
        self.session_saved = false;
        self.document_revision = self.document_revision.wrapping_add(1);
        true
    }

    pub(super) fn edit_handles(m: &Measurement, rect: Rect, image_w: f32, image_h: f32) -> Vec<EditHandle> {
        let mut handles = Vec::new();
        let points = &m.points;
        match m.kind {
            Tool::Rectangle if points.len() >= 2 => {
                let p0 = points[0];
                let p1 = points[1];
                let p0_is_left = p0.x <= p1.x;
                let p0_is_top = p0.y <= p1.y;
                let left = p0.x.min(p1.x);
                let right = p0.x.max(p1.x);
                let top = p0.y.min(p1.y);
                let bottom = p0.y.max(p1.y);
                for (x, y, is_left, is_top) in [
                    (left, top, true, true),
                    (right, top, false, true),
                    (right, bottom, false, false),
                    (left, bottom, true, false),
                ] {
                    let x_point = if is_left == p0_is_left { 0 } else { 1 };
                    let y_point = if is_top == p0_is_top { 0 } else { 1 };
                    let image_point = Point { x, y };
                    handles.push(EditHandle {
                        position: Self::point_to_screen(image_point, rect, image_w, image_h),
                        image_point,
                        target: EditTarget::RectangleCorner { x_point, y_point },
                    });
                }
                let left_point = if p0_is_left { 0 } else { 1 };
                let right_point = if p0_is_left { 1 } else { 0 };
                let top_point = if p0_is_top { 0 } else { 1 };
                let bottom_point = if p0_is_top { 1 } else { 0 };
                for (image_point, target) in [
                    (Point { x: left, y: (top + bottom) * 0.5 }, EditTarget::RectangleEdgeX(left_point)),
                    (Point { x: right, y: (top + bottom) * 0.5 }, EditTarget::RectangleEdgeX(right_point)),
                    (Point { x: (left + right) * 0.5, y: top }, EditTarget::RectangleEdgeY(top_point)),
                    (Point { x: (left + right) * 0.5, y: bottom }, EditTarget::RectangleEdgeY(bottom_point)),
                ] {
                    handles.push(EditHandle {
                        position: Self::point_to_screen(image_point, rect, image_w, image_h),
                        image_point,
                        target,
                    });
                }
            }
            Tool::Circle if points.len() >= 2 => {
                let center = points[0];
                let radius = center.dist(points[1]);
                handles.push(EditHandle {
                    position: Self::point_to_screen(center, rect, image_w, image_h),
                    image_point: center,
                    target: EditTarget::CircleCenter,
                });
                handles.push(EditHandle {
                    position: Self::point_to_screen(points[1], rect, image_w, image_h),
                    image_point: points[1],
                    target: EditTarget::CircleRadius,
                });
                for step in 0..4 {
                    let angle = std::f64::consts::FRAC_PI_2 * step as f64;
                    let image_point = Point {
                        x: center.x + radius * angle.cos(),
                        y: center.y + radius * angle.sin(),
                    };
                    handles.push(EditHandle {
                        position: Self::point_to_screen(image_point, rect, image_w, image_h),
                        image_point,
                        target: EditTarget::CircleRadius,
                    });
                }
            }
            _ => {
                for (index, image_point) in points.iter().copied().enumerate() {
                    handles.push(EditHandle {
                        position: Self::point_to_screen(image_point, rect, image_w, image_h),
                        image_point,
                        target: EditTarget::Vertex(index),
                    });
                }
                let segment_count = match m.kind {
                    Tool::Polygon if points.len() >= 3 => points.len(),
                    Tool::Distance | Tool::Polyline | Tool::Angle => points.len().saturating_sub(1),
                    _ => 0,
                };
                for index in 0..segment_count {
                    let next = (index + 1) % points.len();
                    let a = points[index];
                    let b = points[next];
                    let image_point = Point { x: (a.x + b.x) * 0.5, y: (a.y + b.y) * 0.5 };
                    handles.push(EditHandle {
                        position: Self::point_to_screen(image_point, rect, image_w, image_h),
                        image_point,
                        target: EditTarget::TranslateSegment { first: index, second: next },
                    });
                }
            }
        }
        handles
    }

    pub(super) fn consider_edit_handle(
        best: &mut Option<(EditHandle, f32)>,
        handle: EditHandle,
        distance: f32,
    ) {
        if distance <= 12.0 && best.as_ref().is_none_or(|(_, best_distance)| distance < *best_distance) {
            *best = Some((handle, distance));
        }
    }

    pub(super) fn nearest_edit_handle(
        m: &Measurement,
        pointer: Pos2,
        rect: Rect,
        image_w: f32,
        image_h: f32,
    ) -> Option<(EditHandle, f32)> {
        let mut best = Self::edit_handles(m, rect, image_w, image_h).into_iter()
            .map(|handle| (handle, handle.position.distance(pointer)))
            .min_by(|left, right| left.1.total_cmp(&right.1));
        let consider_segment = |best: &mut Option<(EditHandle, f32)>, a: Pos2, b: Pos2, target: EditTarget| {
            let axis = b - a;
            let length_sq = axis.length_sq();
            if length_sq <= f32::EPSILON { return; }
            let t = ((pointer - a).dot(axis) / length_sq).clamp(0.0, 1.0);
            let projected = a + axis * t;
            let handle = EditHandle {
                position: projected,
                image_point: Self::point_from_screen(projected, rect, image_w, image_h),
                target,
            };
            Self::consider_edit_handle(best, handle, projected.distance(pointer));
        };

        if m.kind == Tool::Circle && m.points.len() >= 2 {
            let center = Self::point_to_screen(m.points[0], rect, image_w, image_h);
            let radius = center.distance(Self::point_to_screen(m.points[1], rect, image_w, image_h));
            let direction = pointer - center;
            let distance = direction.length();
            if radius > 0.0 && distance > f32::EPSILON {
                let projected = center + direction * (radius / distance);
                let handle = EditHandle {
                    position: projected,
                    image_point: Self::point_from_screen(projected, rect, image_w, image_h),
                    target: EditTarget::CircleRadius,
                };
                Self::consider_edit_handle(&mut best, handle, (distance - radius).abs());
            }
        } else if m.kind == Tool::Rectangle && m.points.len() >= 2 {
            let p0 = m.points[0];
            let p1 = m.points[1];
            let p0_is_left = p0.x <= p1.x;
            let p0_is_top = p0.y <= p1.y;
            let left = p0.x.min(p1.x);
            let right = p0.x.max(p1.x);
            let top = p0.y.min(p1.y);
            let bottom = p0.y.max(p1.y);
            let corners = [
                Point { x: left, y: top },
                Point { x: right, y: top },
                Point { x: right, y: bottom },
                Point { x: left, y: bottom },
            ];
            let screen: Vec<Pos2> = corners.iter()
                .map(|point| Self::point_to_screen(*point, rect, image_w, image_h))
                .collect();
            let left_point = if p0_is_left { 0 } else { 1 };
            let right_point = if p0_is_left { 1 } else { 0 };
            let top_point = if p0_is_top { 0 } else { 1 };
            let bottom_point = if p0_is_top { 1 } else { 0 };
            for (a, b, target) in [
                (screen[0], screen[3], EditTarget::RectangleEdgeX(left_point)),
                (screen[1], screen[2], EditTarget::RectangleEdgeX(right_point)),
                (screen[0], screen[1], EditTarget::RectangleEdgeY(top_point)),
                (screen[3], screen[2], EditTarget::RectangleEdgeY(bottom_point)),
            ] {
                consider_segment(&mut best, a, b, target);
            }
        } else {
            let points = &m.points;
            let segment_count = match m.kind {
                Tool::Polygon if points.len() >= 3 => points.len(),
                Tool::Distance | Tool::Polyline | Tool::Angle => points.len().saturating_sub(1),
                _ => 0,
            };
            for index in 0..segment_count {
                let next = (index + 1) % points.len();
                let a = Self::point_to_screen(points[index], rect, image_w, image_h);
                let b = Self::point_to_screen(points[next], rect, image_w, image_h);
                consider_segment(&mut best, a, b, EditTarget::TranslateSegment { first: index, second: next });
            }
        }
        if m.kind==Tool::Distance && m.points.len()>=2 {
            let a=Self::point_to_screen(m.points[0],rect,image_w,image_h);
            let b=Self::point_to_screen(m.points[1],rect,image_w,image_h);
            let(first,second)=Self::distance_construction(a,b,rect);
            consider_segment(&mut best,first,second,EditTarget::TranslateSegment{first:0,second:1});
        }
        best.filter(|(_, distance)| *distance <= 12.0)
    }

    pub(super) fn set_measurement_selection(&mut self, id: Option<u64>) {
        self.set_measurements_selection(id.into_iter().collect());
    }

    pub(super) fn set_measurements_selection(&mut self, ids: Vec<u64>) {
        self.set_combined_selection(ids, Vec::new());
    }

    pub(super) fn set_combined_selection(&mut self, ids: Vec<u64>, annotation_ids: Vec<String>) {
        let ids=ids.into_iter().filter(|id|self.measurements.iter().any(|m|m.id==*id)).collect::<Vec<_>>();
        let annotation_ids=annotation_ids.into_iter()
            .filter(|id|self.annotations.iter().any(|annotation|annotation.id==*id)).collect::<Vec<_>>();
        self.cancel_selection_gesture();
        self.selected_measurement=ids.first().copied();
        self.selected_measurements=ids.into_iter().collect();
        self.selected_annotations=annotation_ids.into_iter().collect();
        if !self.selected_annotations.is_empty() { self.right_panel_annotations=true; }
        else if !self.selected_measurements.is_empty() { self.right_panel_annotations=false; }
    }

    pub(super) fn measurement_selection_ids(&self) -> Vec<u64> {
        self.selected_measurement.into_iter().chain(self.selected_measurements.iter().copied()
            .filter(|id|Some(*id)!=self.selected_measurement)).collect()
    }

    pub(super) fn measurement_is_selected(&self, id: u64) -> bool {
        self.selection_marquee.as_ref().filter(|marquee|marquee.dragged)
            .map_or_else(||self.selected_measurements.contains(&id),|marquee|marquee.preview.contains(&id))
    }

    pub(super) fn annotation_is_selected(&self, id: &str) -> bool {
        self.selection_marquee.as_ref().filter(|marquee|marquee.dragged)
            .map_or_else(||self.selected_annotations.contains(id),|marquee|
                marquee.preview_annotations.iter().any(|selected|selected==id))
    }

    pub(super) fn measurement_gesture_active(&self) -> bool {
        self.point_edit.is_some() || self.selection_marquee.is_some() || self.measurement_group_move.is_some()
    }

    pub(super) fn cancel_selection_gesture(&mut self) {
        // Both multi-selection gestures are previews; the model changes only on release.
        self.selection_marquee=None;
        self.measurement_group_move=None;
    }

    pub(super) fn marquee_selection(&self, marquee: &SelectionMarquee, rect: Rect, w:f32, h:f32) -> Vec<u64> {
        let bounds=[marquee.start.x.min(marquee.end.x),marquee.start.y.min(marquee.end.y),
            marquee.start.x.max(marquee.end.x),marquee.start.y.max(marquee.end.y)];
        let screen_intersects=|a:Pos2,b:Pos2|selection_geometry::segment_intersects_selection(
            Self::point_from_screen(a,rect,w,h),Self::point_from_screen(b,rect,w,h),bounds);
        let mut ids=if marquee.additive || marquee.toggle {
            marquee.before.iter().copied().collect::<BTreeSet<_>>()
        } else {BTreeSet::new()};
        for measurement in &self.measurements {
            let label_hit=marquee.label_bounds.iter().any(|(id,label)|*id==measurement.id
                && label[0]<=bounds[2] && label[2]>=bounds[0] && label[1]<=bounds[3] && label[3]>=bounds[1]);
            let mut hit=label_hit || selection_geometry::intersects_selection(measurement,bounds);
            if !hit && measurement.kind==Tool::Distance && measurement.points.len()>=2 {
                let a=Self::point_to_screen(measurement.points[0],rect,w,h);
                let b=Self::point_to_screen(measurement.points[1],rect,w,h);
                let(first,second)=Self::distance_construction(a,b,rect);
                hit=screen_intersects(first,second) || screen_intersects(a,first) || screen_intersects(b,second);
            }
            if !hit && measurement.kind==Tool::Angle && measurement.points.len()>=3 {
                let points=measurement.points.iter().map(|p|Self::point_to_screen(*p,rect,w,h)).collect::<Vec<_>>();
                hit=measurement_graphics::angle_geometry(points[0],points[1],points[2])
                    .is_some_and(|arc|arc.points.windows(2).any(|pair|screen_intersects(pair[0],pair[1])));
            }
            if !hit && measurement.kind==Tool::Rectangle && measurement.points.len()>=2 {
                let(a,b)=(measurement.points[0],measurement.points[1]);
                hit=selection_geometry::segment_intersects_selection(a,b,bounds)
                    || selection_geometry::segment_intersects_selection(Point{x:a.x,y:b.y},Point{x:b.x,y:a.y},bounds);
            }
            if hit {
                if marquee.toggle && !ids.insert(measurement.id) {ids.remove(&measurement.id);}
                else if !marquee.toggle {ids.insert(measurement.id);}
            }
        }
        // Preserve the primary selection while it remains in the frame selection.
        marquee.before.first().copied().filter(|id|ids.remove(id)).into_iter().chain(ids).collect()
    }

    pub(super) fn marquee_annotation_selection(&self, marquee: &SelectionMarquee, bounds: [f64; 4],
                                    measurement_ids: &[u64]) -> Vec<String> {
        let mut ids=if marquee.additive || marquee.toggle {
            marquee.before_annotations.iter().cloned().collect::<BTreeSet<_>>()
        } else {BTreeSet::new()};
        for annotation in &self.annotations {
            let label_hit=marquee.annotation_label_bounds.iter().any(|(id,label)|id==&annotation.id
                && label[0]<=bounds[2] && label[2]>=bounds[0] && label[1]<=bounds[3] && label[3]>=bounds[1]);
            let connector_hit=marquee.annotation_connectors.iter().any(|(id,first,second)|id==&annotation.id
                && selection_geometry::segment_intersects_selection(*first,*second,bounds));
            let linked_measurement_hit=matches!(&annotation.target, AnnotationTarget::Measurement { measurement_id }
                if measurement_ids.contains(measurement_id));
            let target_hit=match &annotation.target {
                AnnotationTarget::Measurement { .. } => self.annotation_target_anchor(annotation)
                    .is_some_and(|point|point.x>=bounds[0] && point.x<=bounds[2]
                        && point.y>=bounds[1] && point.y<=bounds[3]),
                AnnotationTarget::Point { point } => point.x>=bounds[0] && point.x<=bounds[2]
                    && point.y>=bounds[1] && point.y<=bounds[3],
                AnnotationTarget::Zone { first, second } => {
                    first.x.min(second.x)<=bounds[2] && first.x.max(second.x)>=bounds[0]
                        && first.y.min(second.y)<=bounds[3] && first.y.max(second.y)>=bounds[1]
                }
            };
            if label_hit || connector_hit || linked_measurement_hit || target_hit {
                if marquee.toggle && !ids.insert(annotation.id.clone()) {ids.remove(&annotation.id);}
                else if !marquee.toggle {ids.insert(annotation.id.clone());}
            }
        }
        ids.into_iter().collect()
    }

    pub(super) fn begin_selection_gesture(&mut self, pointer:Pos2, rect:Rect,w:f32,h:f32,
                               painter:&egui::Painter, additive:bool,toggle:bool) {
        if let Some(annotation_id)=self.annotation_at(pointer,rect,w,h,painter) {
            let measurement_ids=if additive || toggle {self.measurement_selection_ids()} else {Vec::new()};
            let mut annotation_ids=if additive || toggle {
                self.selected_annotations.iter().cloned().collect::<Vec<_>>()
            } else {Vec::new()};
            if toggle && annotation_ids.contains(&annotation_id) {
                annotation_ids.retain(|selected|selected!=&annotation_id);
            } else if !annotation_ids.contains(&annotation_id) {annotation_ids.push(annotation_id);}
            self.set_combined_selection(measurement_ids,annotation_ids);
            self.message=format!("Sélection : {} mesure(s), {} annotation(s).",
                self.selected_measurements.len(),self.selected_annotations.len());
            return;
        }
        if let Some(id)=self.measurement_at(pointer,rect,w,h,painter) {
            if additive || toggle {
                let mut ids=self.measurement_selection_ids();
                if toggle && ids.contains(&id) {ids.retain(|selected|*selected!=id);}
                else if !ids.contains(&id) {ids.push(id);}
                let annotations=self.selected_annotations.iter().cloned().collect();
                self.set_combined_selection(ids,annotations);
            } else if self.selected_measurements.len()>1 && self.selected_measurements.contains(&id) {
                if !self.image_is_editable() {return;}
                let only_guides = self.measurements.iter().filter(|m| self.selected_measurements.contains(&m.id)).all(|m| m.is_construction);
                if !only_guides && !self.quality_assessment.as_ref().is_some_and(|quality|quality.is_exploitable) {
                    self.message=self.quality_gate_message();return;
                }
                let selection=self.measurement_selection_ids();
                let before=self.measurements.iter().filter(|m|self.selected_measurements.contains(&m.id)).cloned().collect();
                self.measurement_group_move=Some(MeasurementGroupMove{before,selection,
                    start:Self::point_from_screen(pointer,rect,w,h),delta:Point{x:0.0,y:0.0},
                    pointer_origin:pointer,capture_id:self.capture_id.clone(),revision:self.document_revision});
            } else {
                self.set_measurement_selection(Some(id));
                self.begin_point_edit(pointer,rect,w,h,painter);
            }
            return;
        }
        let label_bounds=self.measurements.iter().filter_map(|m| {
            let card=self.measurement_label_card_rect(painter,rect,w,h,m)?;
            let a=Self::point_from_screen(card.min,rect,w,h);
            let b=Self::point_from_screen(card.max,rect,w,h);
            Some((m.id,[a.x,a.y,b.x,b.y]))
        }).collect();
        let annotation_label_bounds=self.annotation_label_bounds(painter,rect,w,h);
        let annotation_connectors=self.annotation_connector_segments(painter,rect,w,h);
        let start=Self::point_from_screen(pointer,rect,w,h);
        self.selection_marquee=Some(SelectionMarquee{start,end:start,
            before:self.measurement_selection_ids(),
            before_annotations:self.selected_annotations.iter().cloned().collect(),
            additive,toggle,label_bounds,annotation_label_bounds,annotation_connectors,pointer_origin:pointer,
            dragged:false,preview:Vec::new(),preview_annotations:Vec::new()});
    }

    pub(super) fn update_selection_gesture(&mut self,pointer:Pos2,rect:Rect,w:f32,h:f32) {
        if let Some(marquee)=self.selection_marquee.as_mut() {
            marquee.end=Self::point_from_screen(pointer,rect,w,h);
            marquee.dragged |= pointer.distance(marquee.pointer_origin)>=6.0;
        }
        if let Some(marquee)=self.selection_marquee.as_ref().filter(|m|m.dragged) {
            let ids=self.marquee_selection(marquee,rect,w,h);
            let bounds=[marquee.start.x.min(marquee.end.x),marquee.start.y.min(marquee.end.y),
                marquee.start.x.max(marquee.end.x),marquee.start.y.max(marquee.end.y)];
            let annotation_ids=self.marquee_annotation_selection(marquee,bounds,&ids);
            if let Some(marquee)=self.selection_marquee.as_mut() {
                marquee.preview=ids;
                marquee.preview_annotations=annotation_ids;
            }
        }
        if let Some(movement)=self.measurement_group_move.as_mut() {
            if pointer.distance(movement.pointer_origin)<6.0 && movement.delta== (Point{x:0.0,y:0.0}) {return;}
            let native=Self::point_from_screen(pointer,rect,w,h);
            let points=movement.before.iter().flat_map(|m|m.points.iter()).collect::<Vec<_>>();
            if points.is_empty() {return;}
            let min_x=points.iter().map(|p|-p.x).fold(f64::NEG_INFINITY,f64::max);
            let max_x=points.iter().map(|p|w as f64-p.x).fold(f64::INFINITY,f64::min);
            let min_y=points.iter().map(|p|-p.y).fold(f64::NEG_INFINITY,f64::max);
            let max_y=points.iter().map(|p|h as f64-p.y).fold(f64::INFINITY,f64::min);
            if ![min_x,max_x,min_y,max_y,native.x,native.y].iter().all(|n|n.is_finite())
                || min_x>max_x || min_y>max_y {return;}
            movement.delta=Point{x:(native.x-movement.start.x).clamp(min_x,max_x),
                y:(native.y-movement.start.y).clamp(min_y,max_y)};
        }
    }

    pub(super) fn translated_measurement(measurement:&Measurement,delta:Point) -> Measurement {
        let mut translated=measurement.clone();
        for point in &mut translated.points {point.x+=delta.x;point.y+=delta.y;}
        if let Some(label)=translated.label_position_native_px.as_mut() {label.x+=delta.x;label.y+=delta.y;}
        translated
    }

    pub(super) fn finish_selection_gesture(&mut self) {
        if let Some(marquee)=self.selection_marquee.take() {
            if marquee.dragged {
                self.set_combined_selection(marquee.preview,marquee.preview_annotations);
                self.message=format!("{} mesure(s) et {} annotation(s) sélectionnée(s).",
                    self.selected_measurements.len(),self.selected_annotations.len());
            } else if !marquee.additive && !marquee.toggle {
                self.set_combined_selection(Vec::new(),Vec::new());
            }
        }
        let Some(movement)=self.measurement_group_move.take() else {return;};
        if movement.delta==(Point{x:0.0,y:0.0}) {return;}
        if movement.capture_id!=self.capture_id || movement.revision!=self.document_revision
            || !self.image_is_editable() {self.message="Déplacement annulé : la capture ou la session a changé.".into();return;}
        if !movement.before.iter().all(|m| m.is_construction)
            && !self.quality_assessment.as_ref().is_some_and(|quality|quality.is_exploitable) {
            self.message=self.quality_gate_message();return;
        }
        let after=movement.before.iter().map(|m|Self::translated_measurement(m,movement.delta)).collect::<Vec<_>>();
        for translated in &after {
            if let Some(m)=self.measurements.iter_mut().find(|m|m.id==translated.id) {*m=translated.clone();}
        }
        self.record_measurement_action(MeasurementHistoryAction::EditedMany {
            before:movement.before,after,selection:movement.selection,
        });
        self.message="Mesures déplacées ensemble. Ctrl+Z pour annuler le déplacement.".into();
    }

    pub(super) fn begin_calibration_point_edit(&mut self, pointer: Pos2, rect: Rect, image_w: f32, image_h: f32) -> bool {
        let required = if self.calibration_type == CalibrationType::MultiPointXY { 12 } else { 2 };
        if !self.calibration_mode || self.pending.len() != required { return false; }

        let mut nearest: Option<(usize, Point, f32)> = None;
        for (index, point) in self.pending.iter().enumerate() {
            let screen = Self::point_to_screen(point.clone(), rect, image_w, image_h);
            let distance = screen.distance(pointer);
            if distance <= 14.0 && nearest.as_ref().is_none_or(|(_, _, best)| distance < *best) {
                nearest = Some((index, point.clone(), distance));
            }
        }
        let Some((index, before, _)) = nearest else { return false; };
        let pointer_native = Self::point_from_screen(pointer, rect, image_w, image_h);
        self.calibration_point_edit = Some(CalibrationPointEditGesture {
            index,
            before: before.clone(),
            pointer_offset: Point { x: before.x - pointer_native.x, y: before.y - pointer_native.y },
            pointer_origin: pointer,
        });
        true
    }

    pub(super) fn update_calibration_point_edit(&mut self, pointer: Pos2, rect: Rect, image_w: f32, image_h: f32) {
        let Some(edit) = self.calibration_point_edit.as_ref() else { return; };
        let index = edit.index;
        let offset = edit.pointer_offset.clone();
        let mut pointer_native = Self::point_from_screen(pointer, rect, image_w, image_h);
        pointer_native.x = pointer_native.x.clamp(0.0, image_w as f64);
        pointer_native.y = pointer_native.y.clamp(0.0, image_h as f64);
        if let Some(point) = self.pending.get_mut(index) {
            point.x = (pointer_native.x + offset.x).clamp(0.0, image_w as f64);
            point.y = (pointer_native.y + offset.y).clamp(0.0, image_h as f64);
        }
    }

    pub(super) fn finish_calibration_point_edit(&mut self, cancel: bool) -> bool {
        let Some(edit) = self.calibration_point_edit.take() else { return false; };
        let changed = self.pending.get(edit.index).is_some_and(|point| point != &edit.before);
        if cancel {
            if let Some(point) = self.pending.get_mut(edit.index) { *point = edit.before; }
            self.message = "Ajustement du point d’étalonnage annulé.".into();
        } else if changed {
            self.message = "Point de référence ajusté ; la longueur en pixels est recalculée.".into();
        }
        true
    }

    pub(super) fn begin_point_edit(&mut self, pointer: Pos2, rect: Rect, image_w: f32, image_h: f32,
                        painter: &egui::Painter) -> bool {
        if !self.selection_mode || !self.image_is_editable() { return false; }
        let Some(id) = self.selected_measurement else { return false; };
        let Some(measurement) = self.measurements.iter().find(|m| m.id == id).cloned() else {
            return false;
        };
        if !measurement.is_construction && !self.quality_assessment.as_ref().is_some_and(|assessment| assessment.is_exploitable) {
            self.message = self.quality_gate_message();
            return false;
        }
        if let Some(card_rect) = self.measurement_label_card_rect(painter, rect, image_w, image_h, &measurement)
            .filter(|card_rect| card_rect.contains(pointer)) {
            let pointer_native = Self::point_from_screen(pointer, rect, image_w, image_h);
            let card_native = Self::point_from_screen(card_rect.min, rect, image_w, image_h);
            self.point_edit = Some(PointEditGesture {
                before: measurement,
                target: EditTarget::LabelCard,
                pointer_offset: Point { x: card_native.x - pointer_native.x, y: card_native.y - pointer_native.y },
                start_native: pointer_native,
                pointer_origin: pointer,
                rejected_degenerate: false,
            });
            return true;
        }
        let Some((handle, _distance)) = Self::nearest_edit_handle(&measurement, pointer, rect, image_w, image_h) else {
            return false;
        };
        let pointer_native = Self::point_from_screen(pointer, rect, image_w, image_h);
        self.point_edit = Some(PointEditGesture {
            before: measurement,
            target: handle.target,
            pointer_offset: Point {
                x: handle.image_point.x - pointer_native.x,
                y: handle.image_point.y - pointer_native.y,
            },
            start_native: pointer_native,
            pointer_origin: pointer,
            rejected_degenerate: false,
        });
        true
    }

    pub(super) fn update_point_edit(&mut self, pointer: Pos2, rect: Rect, image_w: f32, image_h: f32) {
        let Some(edit) = self.point_edit.as_mut() else { return; };
        let mut pointer_native = Self::point_from_screen(pointer, rect, image_w, image_h);
        pointer_native.x = pointer_native.x.clamp(0.0, image_w as f64);
        pointer_native.y = pointer_native.y.clamp(0.0, image_h as f64);
        let id = edit.before.id;
        let target = edit.target;
        let offset = edit.pointer_offset;
        let start_native = edit.start_native;
        let mut proposed = edit.before.points.clone();
        let edited_point = Point {
            x: (pointer_native.x + offset.x).clamp(0.0, image_w as f64),
            y: (pointer_native.y + offset.y).clamp(0.0, image_h as f64),
        };
        match target {
            EditTarget::LabelCard => {}
            EditTarget::Vertex(index) => proposed[index] = edited_point,
            EditTarget::CircleCenter => {
                let mut delta = Point {
                    x: edited_point.x - proposed[0].x,
                    y: edited_point.y - proposed[0].y,
                };
                let min_dx = proposed.iter().map(|point| -point.x).fold(f64::NEG_INFINITY, f64::max);
                let max_dx = proposed.iter().map(|point| image_w as f64 - point.x).fold(f64::INFINITY, f64::min);
                let min_dy = proposed.iter().map(|point| -point.y).fold(f64::NEG_INFINITY, f64::max);
                let max_dy = proposed.iter().map(|point| image_h as f64 - point.y).fold(f64::INFINITY, f64::min);
                delta.x = delta.x.clamp(min_dx, max_dx);
                delta.y = delta.y.clamp(min_dy, max_dy);
                for point in &mut proposed {
                    point.x += delta.x;
                    point.y += delta.y;
                }
            }
            EditTarget::TranslateSegment { first, second } => {
                let mut delta = Point {
                    x: pointer_native.x - start_native.x,
                    y: pointer_native.y - start_native.y,
                };
                let min_dx = (-proposed[first].x).max(-proposed[second].x);
                let max_dx = (image_w as f64 - proposed[first].x)
                    .min(image_w as f64 - proposed[second].x);
                let min_dy = (-proposed[first].y).max(-proposed[second].y);
                let max_dy = (image_h as f64 - proposed[first].y)
                    .min(image_h as f64 - proposed[second].y);
                delta.x = delta.x.clamp(min_dx, max_dx);
                delta.y = delta.y.clamp(min_dy, max_dy);
                proposed[first].x += delta.x;
                proposed[first].y += delta.y;
                proposed[second].x += delta.x;
                proposed[second].y += delta.y;
            }
            EditTarget::RectangleCorner { x_point, y_point } => {
                proposed[x_point].x = edited_point.x;
                proposed[y_point].y = edited_point.y;
            }
            EditTarget::RectangleEdgeX(index) => proposed[index].x = edited_point.x,
            EditTarget::RectangleEdgeY(index) => proposed[index].y = edited_point.y,
            EditTarget::CircleRadius => proposed[1] = edited_point,
        }
        let Some(measurement) = self.measurements.iter_mut().find(|m| m.id == id) else { return; };
        if matches!(target, EditTarget::LabelCard) {
            measurement.label_position_native_px = Some(edited_point);
            edit.rejected_degenerate = false;
        } else if geometry::is_nondegenerate(measurement.kind, &proposed) {
            measurement.points = proposed;
            edit.rejected_degenerate = false;
        } else {
            edit.rejected_degenerate = true;
        }
    }

    pub(super) fn finish_point_edit(&mut self, cancel: bool) {
        let quality_blocked = !cancel
            && self.point_edit.as_ref().is_some_and(|edit| !edit.before.is_construction && !matches!(edit.target, EditTarget::LabelCard))
            && !self.quality_assessment.as_ref().is_some_and(|assessment| assessment.is_exploitable);
        let Some(edit) = self.point_edit.take() else { return; };
        let id = edit.before.id;
        let Some(measurement) = self.measurements.iter_mut().find(|m| m.id == id) else { return; };
        if cancel || quality_blocked {
            *measurement = edit.before;
            self.message = if quality_blocked {
                self.quality_gate_message()
            } else {
                "Modification de la mesure annulée.".into()
            };
            return;
        }
        let points_changed = measurement.points != edit.before.points;
        let label_moved = measurement.label_position_native_px != edit.before.label_position_native_px;
        if points_changed || label_moved {
            let before = edit.before;
            let after = measurement.clone();
            let exceeds_image = self.image.as_ref().is_some_and(|image| {
                geometry::exceeds_image_bounds(measurement.kind, &measurement.points, image.width, image.height)
            });
            self.set_measurement_selection(Some(id));
            self.record_measurement_action(MeasurementHistoryAction::Edited { id, before, after });
            self.message = if label_moved && !points_changed {
                "Libellé déplacé ; sa position est enregistrée dans la session.".into()
            } else if exceeds_image {
                "Attention : la géométrie dépasse le bord de l'image source.".into()
            } else if edit.rejected_degenerate {
                "Géométrie modifiée ; les positions dégénérées ont été refusées.".into()
            } else {
                "Mesure modifiée ; résultat recalculé.".into()
            };
        } else if edit.rejected_degenerate {
            self.message = "Modification refusée : elle rendrait la mesure dégénérée.".into();
        }
    }

    pub(super) fn undo_current_action(&mut self) {
        let display_action = matches!(self.undo_history.last(), Some(MeasurementHistoryAction::DisplayAdjusted { .. }));
        self.display_correction_drag_active = false;
        if display_action && self.display_history_priority && !self.measurement_gesture_active() {
            if self.undo_measurement_action() { self.message = "Dernier réglage d’image annulé.".into(); }
        } else if self.selection_marquee.is_some() || self.measurement_group_move.is_some() {
            self.cancel_selection_gesture();
            self.message="Sélection ou déplacement groupé annulé.".into();
        } else if self.point_edit.is_some() {
            self.finish_point_edit(true);
        } else if !self.pending.is_empty() {
            self.pending.pop();
            if self.pending.is_empty() { self.clear_pending(); }
            self.message = "Dernier point du tracé annulé.".into();
        } else if self.undo_measurement_action() {
            self.message = if display_action { "Dernier réglage d’image annulé." } else { "Dernière mesure ou modification annulée." }.into();
        }
    }

    pub(super) fn redo_current_action(&mut self) {
        let display_action = matches!(self.redo_history.last(), Some(MeasurementHistoryAction::DisplayAdjusted { .. }));
        self.display_correction_drag_active = false;
        if self.measurement_gesture_active() || (!self.pending.is_empty() && !(display_action && self.display_history_priority)) { return; }
        if self.redo_measurement_action() {
            self.message = if display_action { "Dernier réglage d’image rétabli." } else { "Dernière mesure ou modification rétablie." }.into();
        }
    }

    pub(super) fn delete_selected_measurement(&mut self) -> bool {
        if self.measurement_gesture_active() {return false;}
        if self.selected_measurements.len()>1 {
            let selection=self.measurement_selection_ids();
            if let Err(error) = self.ensure_dependency_deletion(&selection) { self.message = error; return false; }
            let measurements=self.measurements.iter().enumerate()
                .filter(|(_,m)|self.selected_measurements.contains(&m.id))
                .map(|(index,m)|(index,m.clone())).collect::<Vec<_>>();
            let count=measurements.len();
            if count==0 {self.set_measurement_selection(None);return false;}
            self.measurements.retain(|m|!self.selected_measurements.contains(&m.id));
            self.set_measurement_selection(None);
            self.record_measurement_action(MeasurementHistoryAction::RemovedMany {measurements,selection});
            self.message=format!("{count} mesures supprimées. Ctrl+Z pour les rétablir ensemble.");
            return true;
        }
        let Some(id) = self.selected_measurement else { return false; };
        if let Err(error) = self.ensure_dependency_deletion(&[id]) { self.message = error; return false; }
        let Some(index) = self.measurements.iter().position(|measurement| measurement.id == id) else {
            self.set_measurement_selection(None);
            return false;
        };
        let previous_selection = self.selected_measurement;
        let measurement = self.measurements.remove(index);
        self.set_measurement_selection(None);
        self.record_measurement_action(MeasurementHistoryAction::Removed {
            measurement,
            index,
            previous_selection,
        });
        self.message = "Mesure sélectionnée supprimée. Ctrl+Z pour la rétablir.".into();
        true
    }

    pub(super) fn escape_active_mode(&mut self) {
        if let Some(dialog) = self.pattern_dialog.as_mut() {
            if dialog.picking.take().is_some() {
                self.selection_mode = dialog.previous_selection_mode;
                self.message = "Choix du repère de répétition annulé.".into();
                return;
            }
        }
        self.pan_mode = false;
        if self.selection_marquee.is_some() || self.measurement_group_move.is_some() {
            self.cancel_selection_gesture();
            self.message="Sélection ou déplacement groupé annulé.".into();
        } else if self.calibration_point_edit.is_some() {
            self.finish_calibration_point_edit(true);
        } else if self.point_edit.is_some() {
            self.finish_point_edit(true);
        } else if self.annotation_mode {
            self.annotation_mode = false;
            self.annotation_points.clear();
            self.message = "Placement du commentaire annulé.".into();
        } else if self.calibration_mode {
            self.calibration_mode = false;
            self.clear_pending();
            self.selection_mode = true;
            self.message = "Étalonnage annulé ; mode sélection actif.".into();
        } else if !self.selection_mode {
            self.clear_pending();
            self.selection_mode = true;
            self.message = "Mode de tracé quitté ; mode sélection actif.".into();
        } else {
            self.set_measurement_selection(None);
            self.message = "Mesure désélectionnée.".into();
        }
    }

    pub(super) fn measurement_at(&self, pointer: Pos2, rect: Rect, image_w: f32, image_h: f32,
                      painter: &egui::Painter) -> Option<u64> {
        let label_target = self.measurement_label_at(pointer, rect, image_w, image_h, painter);
        if label_target.is_some() {return label_target;}
        let mut nearest: Option<(u64,f32)> = None;
        for measurement in &self.measurements {
            if let Some((_, distance)) = Self::nearest_edit_handle(measurement, pointer, rect, image_w, image_h) {
                if nearest.is_none_or(|(_,best)| distance < best) {
                    nearest = Some((measurement.id,distance));
                }
            }
        }
        nearest.map(|(id,_)|id)
    }
}

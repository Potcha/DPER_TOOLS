//! Acceptance checks for the recovered native handles and selection gestures.
use super::*;

#[test]
fn dpm30_construction_center_translates_rectangle_and_diagonals_are_selectable() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Rectangle, &[[20.0, 20.0], [60.0, 60.0]], true);
    f.app.set_measurement_selection(Some(1));
    assert!(selection_geometry::intersects_selection(&f.app.measurements[0], [39.0, 39.0, 41.0, 41.0]));
    let start = f.screen(40.0, 40.0);
    let end = f.screen(50.0, 55.0);
    f.drag(start, end);
    assert_eq!(f.app.measurements[0].points, [Point { x: 30.0, y: 35.0 }, Point { x: 70.0, y: 75.0 }]);
    assert!(f.app.measurements[0].is_construction);
    assert!(reference::measurement_mm(&f.app.measurements[0]).is_none());
    assert!(f.app.undo_measurement_action());
    assert_eq!(f.app.measurements[0].points[0], Point { x: 20.0, y: 20.0 });
    f.app.measurements[0].is_construction = false;
    assert!(!selection_geometry::intersects_selection(&f.app.measurements[0], [39.0, 39.0, 41.0, 41.0]));
}

#[test]
fn dpm30_free_rectangle_vertex_becomes_polygon_and_undo_restores_variant() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Polygon, &[[20.0, 20.0], [60.0, 30.0], [55.0, 50.0], [15.0, 40.0]], false);
    let variant = drawing_variants::DrawingVariant::Rectangle3.label().to_owned();
    f.app.measurements[0].drawing_variant = Some(variant.clone());
    f.app.set_measurement_selection(Some(1));
    let start = f.screen(60.0, 30.0);
    let intermediate = f.screen(63.0, 33.0);
    let end = f.screen(65.0, 35.0);
    f.frame(|app, painter, rect| {
        assert!(app.begin_point_edit(start, rect, 100.0, 100.0, painter));
        app.update_point_edit(intermediate, rect, 100.0, 100.0);
        assert_eq!(app.measurements[0].drawing_variant, None);
        app.update_point_edit(end, rect, 100.0, 100.0);
        assert_eq!(app.measurements[0].drawing_variant, None);
        app.finish_point_edit(false);
    });
    assert_eq!(f.app.measurements[0].drawing_variant, None);
    assert!(!geometry::result(&f.app.measurements[0]).starts_with("L "));
    assert!(f.app.message.contains("polygone"));
    let reopened: Measurement = serde_json::from_value(serde_json::to_value(&f.app.measurements[0]).unwrap()).unwrap();
    assert_eq!(reopened.drawing_variant, None);
    assert!(f.app.undo_measurement_action());
    assert_eq!(f.app.measurements[0].drawing_variant, Some(variant));
}

#[test]
fn dpm30_snap_records_both_intersecting_guides_and_rejects_stale_guides() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Distance, &[[10.0, 40.0], [90.0, 40.0]], true);
    f.add(2, Tool::Distance, &[[40.0, 10.0], [40.0, 90.0]], true);
    let (point, sources) = f.app.construction_snap(Point { x: 40.0, y: 40.0 }).unwrap();
    assert_eq!(point, Point { x: 40.0, y: 40.0 });
    assert_eq!(sources, [1, 2]);
    f.app.drawing_variant = drawing_variants::DrawingVariant::Line;
    f.app.mode = Tool::Distance;
    f.app.on_click(point);
    f.app.on_click(Point { x: 65.0, y: 65.0 });
    let child = f.app.measurements.last().unwrap();
    assert_eq!(child.source_measurement_ids, [1, 2]);
    let child_id = child.id;
    let before = f.app.measurements[0].clone();
    let mut after = before.clone();
    after.points[0].x += 1.0;
    f.app.measurements[0] = after.clone();
    f.app.record_measurement_action(MeasurementHistoryAction::Edited { id: 1, before, after });
    assert!(f.app.measurements.iter().find(|m| m.id == child_id).unwrap().dependency_stale);
    f.app.measurements[0].dependency_stale = true;
    f.app.measurements[1].dependency_stale = true;
    assert!(f.app.construction_snap(point).is_none());
}

#[test]
fn dpm30_changed_guide_between_clicks_refuses_measurement_and_cancel_clears_sources() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Rectangle, &[[20.0, 20.0], [60.0, 60.0]], true);
    f.app.mode = Tool::Distance;
    f.app.drawing_variant = drawing_variants::DrawingVariant::Line;
    f.app.on_click(Point { x: 40.0, y: 40.0 });
    assert_eq!(f.app.pending_construction_sources.len(), 1);
    f.app.inspection.dependencies.mark_created(1);
    f.app.on_click(Point { x: 80.0, y: 80.0 });
    assert_eq!(f.app.measurements.len(), 1);
    assert!(f.app.message.contains("source modifiée"));
    assert!(f.app.pending_construction_sources.is_empty());
    f.app.on_click(Point { x: 40.0, y: 40.0 });
    f.app.clear_pending();
    assert!(f.app.pending_construction_sources.is_empty());
}

#[test]
fn dpm30_undo_point_removes_only_its_construction_dependency() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Rectangle, &[[20.0, 20.0], [60.0, 60.0]], true);
    f.app.mode = Tool::Polyline;
    f.app.drawing_variant = drawing_variants::DrawingVariant::Standard(Tool::Polyline);
    f.app.on_click(Point { x: 80.0, y: 80.0 });
    f.app.on_click(Point { x: 40.0, y: 40.0 });
    f.app.undo_current_action();
    f.app.on_click(Point { x: 85.0, y: 90.0 });
    f.app.finish_measurement();
    assert_eq!(f.app.measurements.len(), 2);
    assert!(f.app.measurements[1].source_measurement_ids.is_empty());
}

fn assert_point_close(actual: Point, expected: Point) {
    assert!(actual.dist(expected) < 0.0001, "{actual:?} != {expected:?}");
}

struct EditingFixture {
    app: DperMeasureApp,
    ctx: egui::Context,
    rect: Rect,
}

impl EditingFixture {
    fn new() -> Self {
        let ctx = egui::Context::default();
        let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
        let mut rgb = vec![0; 100 * 100 * 3];
        for y in 0..100 {
            for x in 0..100 {
                let value = if (x / 8 + y / 8) % 2 == 0 { 60 } else { 196 };
                rgb[(y * 100 + x) * 3..(y * 100 + x) * 3 + 3].fill(value);
            }
        }
        app.quality_assessment = Some(quality::assess_image_quality(
            &rgb, 100, 100, &app.quality_thresholds).unwrap());
        assert!(app.quality_assessment.as_ref().unwrap().is_exploitable);
        app.image = Some(ImageData { width: 100, height: 100, rgb, source: "recovered-editing.png".into() });
        app.selection_mode = true;
        app.edge_snap_enabled = false;
        Self { app, ctx, rect: Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(800.0, 800.0)) }
    }

    fn add(&mut self, id: u64, kind: Tool, points: &[[f64; 2]], construction: bool) {
        let points = points.iter().map(|p| Point { x: p[0], y: p[1] }).collect::<Vec<_>>();
        let m: Measurement = serde_json::from_value(serde_json::json!({
            "id": id, "kind": kind, "points": points, "label": format!("Fixture {id}"),
            "capture_id": self.app.capture_id, "is_construction": construction,
            "label_position_native_px": {"x": 2.0, "y": 2.0}
        })).unwrap();
        self.app.measurements.push(m);
        self.app.next_id = id + 1;
    }

    fn screen(&self, x: f64, y: f64) -> Pos2 {
        DperMeasureApp::point_to_screen(Point { x, y }, self.rect, 100.0, 100.0)
    }

    fn frame(&mut self, action: impl FnOnce(&mut DperMeasureApp, &egui::Painter, Rect)) {
        let mut action = Some(action);
        let rect = self.rect;
        let app = &mut self.app;
        let _ = self.ctx.run(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 1000.0))),
            ..Default::default()
        }, |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("editing-fixture")));
            action.take().unwrap()(app, &painter, rect);
        });
    }

    fn drag(&mut self, start: Pos2, end: Pos2) {
        self.frame(|app, painter, rect| {
            assert!(app.begin_point_edit(start, rect, 100.0, 100.0, painter));
            app.update_point_edit(end, rect, 100.0, 100.0);
            app.finish_point_edit(false);
        });
    }
}

#[test]
fn recovered_circle_radius_handle_resizes_without_moving_center_and_history_restores_it() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Circle, &[[50.0, 50.0], [70.0, 50.0]], false);
    f.app.set_measurement_selection(Some(1));
    let start = f.screen(70.0, 50.0);
    let end = f.screen(50.0, 80.0);
    let (handle, _) = DperMeasureApp::nearest_edit_handle(&f.app.measurements[0], start, f.rect, 100.0, 100.0).unwrap();
    assert!(matches!(handle.target, EditTarget::CircleRadius));
    f.drag(start, end);
    assert_eq!(f.app.measurements[0].points, vec![Point { x: 50.0, y: 50.0 }, Point { x: 50.0, y: 80.0 }]);
    assert!(f.app.undo_measurement_action());
    assert_eq!(f.app.measurements[0].points[0].dist(f.app.measurements[0].points[1]), 20.0);
    assert!(f.app.redo_measurement_action());
    assert_eq!(f.app.measurements[0].points[0].dist(f.app.measurements[0].points[1]), 30.0);
}

#[test]
fn recovered_segment_endpoint_changes_direction_while_opposite_endpoint_stays_fixed() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Distance, &[[30.0, 50.0], [70.0, 50.0]], false);
    f.app.set_measurement_selection(Some(1));
    let start = f.screen(70.0, 50.0);
    let end = f.screen(60.0, 75.0);
    let (handle, _) = DperMeasureApp::nearest_edit_handle(&f.app.measurements[0], start, f.rect, 100.0, 100.0).unwrap();
    assert!(matches!(handle.target, EditTarget::Vertex(1)));
    f.drag(start, end);
    assert_eq!(f.app.measurements[0].points[0], Point { x: 30.0, y: 50.0 });
    assert_point_close(f.app.measurements[0].points[1], Point { x: 60.0, y: 75.0 });
    assert!(matches!(f.app.undo_history.last(), Some(MeasurementHistoryAction::Edited { id: 1, .. })));
}

#[test]
fn recovered_cartouche_drag_keeps_geometry_calibration_and_dependency_revision_unchanged() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Distance, &[[30.0, 50.0], [70.0, 50.0]], false);
    f.app.measurements[0].mm_per_pixel = Some(0.125);
    f.app.set_measurement_selection(Some(1));
    let before = f.app.measurements[0].clone();
    let dependency_revision = f.app.inspection.dependencies.revision(1);
    f.frame(|app, painter, rect| {
        let card = app.measurement_label_card_rect(painter, rect, 100.0, 100.0, &app.measurements[0]).unwrap();
        assert!(app.begin_point_edit(card.center(), rect, 100.0, 100.0, painter));
        assert!(matches!(app.point_edit.as_ref().unwrap().target, EditTarget::LabelCard));
        app.update_point_edit(card.center() + egui::vec2(80.0, 40.0), rect, 100.0, 100.0);
        app.finish_point_edit(false);
    });
    let after = &f.app.measurements[0];
    assert_eq!(after.points, before.points);
    assert_eq!(after.mm_per_pixel, before.mm_per_pixel);
    assert_point_close(after.label_position_native_px.unwrap(), Point { x: 12.0, y: 7.0 });
    assert_eq!(f.app.inspection.dependencies.revision(1), dependency_revision);
    assert!(f.app.undo_measurement_action());
    assert_eq!(f.app.measurements[0].label_position_native_px, before.label_position_native_px);
}

#[test]
fn recovered_multiselection_moves_as_one_action_and_restores_selection_on_undo_redo() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Distance, &[[20.0, 40.0], [40.0, 40.0]], false);
    f.add(2, Tool::Distance, &[[50.0, 70.0], [70.0, 70.0]], false);
    let before = f.app.measurements.iter().map(|m| m.points.clone()).collect::<Vec<_>>();
    f.app.set_measurement_selection(Some(1));
    let second = f.screen(60.0, 70.0);
    let start = f.screen(30.0, 40.0);
    let end = f.screen(40.0, 45.0);
    f.frame(|app, painter, rect| {
        app.begin_selection_gesture(second, rect, 100.0, 100.0, painter, true, false);
        assert_eq!(app.measurement_selection_ids(), vec![1, 2]);
        app.begin_selection_gesture(start, rect, 100.0, 100.0, painter, false, false);
        assert!(app.measurement_group_move.is_some());
        app.update_selection_gesture(end, rect, 100.0, 100.0);
        // Group gestures preview until release; cancellation cannot corrupt geometry.
        assert_eq!(app.measurements[0].points, before[0]);
        app.finish_selection_gesture();
    });
    for (measurement, original) in f.app.measurements.iter().zip(&before) {
        for (moved, point) in measurement.points.iter().zip(original) {
            assert_point_close(*moved, Point { x: point.x + 10.0, y: point.y + 5.0 });
        }
    }
    assert_eq!(f.app.undo_history.len(), 1);
    assert!(matches!(f.app.undo_history.last(), Some(MeasurementHistoryAction::EditedMany { .. })));
    assert!(f.app.undo_measurement_action());
    assert_eq!(f.app.measurements.iter().map(|m| m.points.clone()).collect::<Vec<_>>(), before);
    assert_eq!(f.app.measurement_selection_ids(), vec![1, 2]);
    assert!(f.app.redo_measurement_action());
    assert_point_close(f.app.measurements[0].points[0], Point { x: 30.0, y: 45.0 });
    assert_eq!(f.app.measurement_selection_ids(), vec![1, 2]);
}

#[test]
fn recovered_construction_can_resize_without_measurement_quality_but_parent_deletion_is_guarded() {
    let mut f = EditingFixture::new();
    f.add(1, Tool::Circle, &[[50.0, 50.0], [70.0, 50.0]], true);
    f.app.quality_assessment = None;
    f.app.set_measurement_selection(Some(1));
    f.drag(f.screen(70.0, 50.0), f.screen(80.0, 50.0));
    assert!(f.app.measurements[0].is_construction);
    assert_eq!(f.app.measurements[0].points[1], Point { x: 80.0, y: 50.0 });
    assert!(geometry::result(&f.app.measurements[0]).starts_with("Construction"));
    f.add(2, Tool::Distance, &[[20.0, 20.0], [30.0, 20.0]], false);
    f.app.measurements[1].source_measurement_ids = vec![1];
    f.app.set_measurement_selection(Some(1));
    assert!(!f.app.delete_selected_measurement());
    assert_eq!(f.app.measurements.len(), 2);
    assert!(f.app.message.contains("dépend"));
    f.app.set_measurements_selection(vec![1, 2]);
    assert!(f.app.delete_selected_measurement());
    assert!(f.app.measurements.is_empty());
    assert!(f.app.undo_measurement_action());
    assert_eq!(f.app.measurements[1].source_measurement_ids, vec![1]);
}


fn handle_regression_rect(scale: f32) -> Rect {
    // Native 100×100 image displayed at another zoom and a nonzero pan offset.
    Rect::from_min_size(egui::pos2(173.0, 81.0), egui::vec2(100.0, 100.0) * scale)
}

fn handle_regression_offsets() -> Vec<Vec2> {
    let mut offsets = Vec::new();
    for pixels in [2.0, 5.0, 8.0] {
        for (x, y) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0),
            (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            offsets.push(egui::vec2(x * pixels, y * pixels));
        }
    }
    offsets
}

#[test]
fn dpm45_offset_segment_endpoints_keep_vertex_priority_and_resize_both_axes_at_different_zooms() {
    for scale in [2.0, 8.0] {
        for endpoint in 0..2 {
            for offset in handle_regression_offsets() {
                let mut f = EditingFixture::new();
                f.rect = handle_regression_rect(scale);
                f.add(1, Tool::Distance, &[[30.0, 40.0], [70.0, 60.0]], false);
                f.app.set_measurement_selection(Some(1));
                let before = f.app.measurements[0].points.clone();
                let point = before[endpoint];
                let start = f.screen(point.x, point.y) + offset;
                let (handle, _) = DperMeasureApp::nearest_edit_handle(
                    &f.app.measurements[0], start, f.rect, 100.0, 100.0).unwrap();
                assert!(matches!(handle.target, EditTarget::Vertex(index) if index == endpoint),
                    "Endpoint {endpoint}, zoom {scale}, offset {offset:?}");
                f.drag(start, start + egui::vec2(7.0, 9.0) * scale);
                assert_point_close(f.app.measurements[0].points[endpoint],
                    Point { x: point.x + 7.0, y: point.y + 9.0 });
                assert_eq!(f.app.measurements[0].points[1 - endpoint], before[1 - endpoint]);
            }
        }
    }
}

#[test]
fn dpm45_offset_rectangle_corners_keep_both_axis_priority_for_every_stored_corner_order() {
    for scale in [2.0, 8.0] {
        for stored in [[[20.0, 20.0], [80.0, 80.0]], [[80.0, 80.0], [20.0, 20.0]],
            [[20.0, 80.0], [80.0, 20.0]], [[80.0, 20.0], [20.0, 80.0]]] {
            for (x, y) in [(20.0, 20.0), (80.0, 20.0), (80.0, 80.0), (20.0, 80.0)] {
                for offset in handle_regression_offsets() {
                    let mut f = EditingFixture::new();
                    f.rect = handle_regression_rect(scale);
                    f.add(1, Tool::Rectangle, &stored, false);
                    f.app.set_measurement_selection(Some(1));
                    let before = f.app.measurements[0].points.clone();
                    let x_index = if before[0].x == x { 0 } else { 1 };
                    let y_index = if before[0].y == y { 0 } else { 1 };
                    let start = f.screen(x, y) + offset;
                    let (handle, _) = DperMeasureApp::nearest_edit_handle(
                        &f.app.measurements[0], start, f.rect, 100.0, 100.0).unwrap();
                    assert!(matches!(handle.target, EditTarget::RectangleCorner { x_point, y_point }
                        if x_point == x_index && y_point == y_index),
                        "Corner ({x},{y}), zoom {scale}, offset {offset:?}");
                    let dx = if x == 20.0 { 5.0 } else { -5.0 };
                    let dy = if y == 20.0 { 7.0 } else { -7.0 };
                    f.drag(start, start + egui::vec2(dx as f32, dy as f32) * scale);
                    let mut expected = before;
                    expected[x_index].x += dx;
                    expected[y_index].y += dy;
                    for (actual, expected) in f.app.measurements[0].points.iter().zip(expected) {
                        assert_point_close(*actual, expected);
                    }
                }
            }
        }
    }
}

#[test]
fn dpm45_offset_construction_center_still_translates_whole_rectangle_without_resizing() {
    for scale in [2.0, 8.0] {
        for offset in handle_regression_offsets() {
            let mut f = EditingFixture::new();
            f.rect = handle_regression_rect(scale);
            f.add(1, Tool::Rectangle, &[[80.0, 80.0], [20.0, 20.0]], true);
            f.app.set_measurement_selection(Some(1));
            let start = f.screen(50.0, 50.0) + offset;
            let (handle, _) = DperMeasureApp::nearest_edit_handle(
                &f.app.measurements[0], start, f.rect, 100.0, 100.0).unwrap();
            assert!(matches!(handle.target, EditTarget::TranslateShape));
            f.drag(start, start + egui::vec2(5.0, 7.0) * scale);
            assert_point_close(f.app.measurements[0].points[0], Point { x: 85.0, y: 87.0 });
            assert_point_close(f.app.measurements[0].points[1], Point { x: 25.0, y: 27.0 });
            assert!(f.app.measurements[0].is_construction);
        }
    }
}

#[test]
fn dpm45_projected_line_and_rectangle_edges_remain_draggable_away_from_explicit_handles() {
    for scale in [2.0, 8.0] {
        let mut line = EditingFixture::new();
        line.rect = handle_regression_rect(scale);
        line.add(1, Tool::Distance, &[[20.0, 40.0], [80.0, 40.0]], false);
        line.app.set_measurement_selection(Some(1));
        let start = line.screen(35.0, 40.0) + egui::vec2(0.0, 4.0);
        let (handle, _) = DperMeasureApp::nearest_edit_handle(
            &line.app.measurements[0], start, line.rect, 100.0, 100.0).unwrap();
        assert!(matches!(handle.target, EditTarget::TranslateSegment { first: 0, second: 1 }));
        line.drag(start, start + egui::vec2(5.0, 7.0) * scale);
        assert_point_close(line.app.measurements[0].points[0], Point { x: 25.0, y: 47.0 });
        assert_point_close(line.app.measurements[0].points[1], Point { x: 85.0, y: 47.0 });

        let mut rectangle = EditingFixture::new();
        rectangle.rect = handle_regression_rect(scale);
        rectangle.add(1, Tool::Rectangle, &[[20.0, 20.0], [80.0, 80.0]], false);
        rectangle.app.set_measurement_selection(Some(1));
        let start = rectangle.screen(20.0, 35.0) + egui::vec2(-4.0, 0.0);
        let (handle, _) = DperMeasureApp::nearest_edit_handle(
            &rectangle.app.measurements[0], start, rectangle.rect, 100.0, 100.0).unwrap();
        assert!(matches!(handle.target, EditTarget::RectangleEdgeX(0)));
        rectangle.drag(start, start + egui::vec2(5.0, 7.0) * scale);
        assert_point_close(rectangle.app.measurements[0].points[0], Point { x: 25.0, y: 20.0 });
        assert_eq!(rectangle.app.measurements[0].points[1], Point { x: 80.0, y: 80.0 });
    }
}

#[test]
fn dpm45_circle_contour_projection_keeps_radius_edit_behavior_away_from_explicit_handles() {
    for scale in [2.0, 8.0] {
        let mut f = EditingFixture::new();
        f.rect = handle_regression_rect(scale);
        f.add(1, Tool::Circle, &[[50.0, 50.0], [70.0, 50.0]], false);
        f.app.set_measurement_selection(Some(1));
        let direction = egui::vec2(std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2);
        let start = f.screen(50.0, 50.0) + direction * (20.0 * scale + 4.0);
        let (handle, _) = DperMeasureApp::nearest_edit_handle(
            &f.app.measurements[0], start, f.rect, 100.0, 100.0).unwrap();
        assert!(matches!(handle.target, EditTarget::CircleRadius));
        f.drag(start, start + direction * (5.0 * scale));
        assert_eq!(f.app.measurements[0].points[0], Point { x: 50.0, y: 50.0 });
        let radius = f.app.measurements[0].points[0].dist(f.app.measurements[0].points[1]);
        assert!((radius - 25.0).abs() < 0.0001);
    }
}


#[test]
fn segment_dimension_offset_is_continuous_across_angles_center_edges_and_short_lengths() {
    let image=Rect::from_min_size(Pos2::ZERO,Vec2::splat(800.0));
    let display=|a,b|DperMeasureApp::distance_construction(a,b,image);
    // Both former angular cutoffs, all quadrants, and reversed endpoint order.
    for degrees in [0.0_f32,7.4696,82.5304,90.0,97.4696,172.5304,180.0,187.4696,262.5304,270.0,277.4696,352.5304] {
        let angle=degrees.to_radians();
        let point=|t:f32|Pos2::new(180.0+180.0*t.cos(),240.0+180.0*t.sin());
        let a=Pos2::new(180.0,240.0);
        let before=display(a,point(angle-0.00001));
        let after=display(a,point(angle+0.00001));
        assert!(before.0.distance(after.0)<0.1 && before.1.distance(after.1)<0.1,"Angular jump near {degrees}");
        let reverse=display(point(angle),a);
        let forward=display(a,point(angle));
        assert!(reverse.0.distance(forward.1)<0.001 && reverse.1.distance(forward.0)<0.001);
        assert!(((forward.1-forward.0)-(point(angle)-a)).length()<0.001);
    }
    for x in [6.0_f32,32.0,400.0,768.0,794.0] {
        let before=display(Pos2::new(x-0.001,200.0),Pos2::new(x-0.001,600.0));
        let after=display(Pos2::new(x+0.001,200.0),Pos2::new(x+0.001,600.0));
        assert!(before.0.distance(after.0)<0.1,"Center or boundary jump at {x}");
    }
    let before=display(Pos2::new(100.0,100.0),Pos2::new(119.999,100.0));
    let after=display(Pos2::new(100.0,100.0),Pos2::new(120.001,100.0));
    assert!(before.0.distance(after.0)<0.1 && before.1.distance(after.1)<0.1);
}

#[test]
fn segment_endpoint_circular_drag_keeps_fixed_endpoint_and_dimension_trace_smooth() {
    for scale in [3.0_f32,8.0] {
        let mut f=EditingFixture::new();
        f.rect=Rect::from_min_size(Pos2::new(90.0,120.0),Vec2::splat(100.0*scale));
        f.add(1,Tool::Distance,&[[40.0,30.0],[40.0,70.0]],false);
        f.app.set_measurement_selection(Some(1));
        let start=f.screen(40.0,30.0);
        f.frame(|app,painter,rect| {
            assert!(app.begin_point_edit(start,rect,100.0,100.0,painter));
            let mut previous=None;
            for step in 0..=720 {
                let angle=(step as f32*0.5).to_radians();
                let native=Point{x:40.0+8.0*angle.cos() as f64,y:30.0+8.0*angle.sin() as f64};
                let pointer=DperMeasureApp::point_to_screen(native,rect,100.0,100.0);
                app.update_point_edit(pointer,rect,100.0,100.0);
                let m=&app.measurements[0];
                assert_eq!(m.points[1],Point{x:40.0,y:70.0});
                assert!(m.points[0].dist(native)<0.0001);
                let a=DperMeasureApp::point_to_screen(m.points[0],rect,100.0,100.0);
                let b=DperMeasureApp::point_to_screen(m.points[1],rect,100.0,100.0);
                let trace=DperMeasureApp::distance_construction(a,b,rect);
                if let Some((first,second))=previous {
                    assert!(trace.0.distance(first)<2.0 && trace.1.distance(second)<2.0,"Dimension jumped at step {step}, scale {scale}");
                }
                previous=Some(trace);
            }
            app.finish_point_edit(false);
        });
        assert_eq!(f.app.undo_history.len(),1);
        assert!(f.app.undo_measurement_action());
        assert_eq!(f.app.measurements[0].points[0],Point{x:40.0,y:30.0});
    }
}

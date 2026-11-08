//! Original measurement overlay code recovered from the pre-sync local source ZIP.
//! Layout, drawing primitives and card styles are retained from that source.
use crate::{DperMeasureApp, geometry::{self, Measurement, Point, Tool}, CalibrationType,
    alignment, measurement_graphics, ui_widgets, inspection_evidence_ui};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};

pub(super) struct MeasurementCallout {
    title: Option<std::sync::Arc<egui::Galley>>,
    value: std::sync::Arc<egui::Galley>,
    size: Vec2,
    padding: Vec2,
    border: Color32,
    value_color: Color32,
    outlined: bool,
}


impl DperMeasureApp {
    // Keep native independent measurements independent of inspection evidence.
    // Associated values use the current reassessed service result, preserving
    // suppression when source evidence is stale or unavailable.
    fn recovered_measurement_display_value(&self, _painter: &egui::Painter, m: &Measurement) -> String {
        if self.inspection.records.iter().any(|record|
            record.measurement_id == Some(m.id) && record.capture_id == m.capture_id) {
            inspection_evidence_ui::measurement_display_value(m, &self.current_inspection())
        } else { geometry::result(m) }
    }

    pub(super) fn paint_measurement_segment(painter: &egui::Painter, a: Pos2, b: Pos2,
                                 color: Color32, width: f32) {
        measurement_graphics::segment(painter,a,b,color,width);
    }

    pub(super) fn paint_vertex(painter: &egui::Painter, point: Pos2, color: Color32, radius: f32) {
        if !point.x.is_finite() || !point.y.is_finite() {return;}
        let radius=radius.max(3.5);
        painter.circle_filled(point,radius+1.2,Color32::from_rgba_unmultiplied(3,12,22,220));
        painter.circle_filled(point,radius,Color32::from_rgb(242,251,255));
        painter.circle_stroke(point,radius,Stroke::new(1.6,color));
    }

    pub(super) fn measurement_callout(&self,painter:&egui::Painter,m:&Measurement,selected:bool)->Option<MeasurementCallout> {
        if m.is_construction { return None; }
        let result=if m.id==0 && self.calibration_mode && self.calibration_type==CalibrationType::MultiPointXY {
            format!("Points : {}/12",m.points.len())
        } else if m.id==0 && self.calibration_mode && m.points.len()==2 {
            format!("Référence : {:.1} px",m.points[0].dist(m.points[1])*self.calibration_type.reference_scale())
        } else {self.recovered_measurement_display_value(painter,m)};
        let result = result.strip_prefix("Géométrie imposée · ").unwrap_or(&result).to_owned();
        if result=="Mesure incomplète" {return None;}
        let title=if m.id==0 {
            Some(if self.calibration_mode {format!("Étalonnage · {}",self.calibration_type.label())}
                else {format!("Aperçu · {}",m.kind.label())})
        } else if m.is_numeric_resize() {
            Some(crate::numeric_resize_ui::measurement_name(m))
        } else if selected {
            Some(crate::numeric_resize_ui::measurement_name(m))
        } else {None};
        let border=if selected {Color32::from_rgb(239,177,42)} else {measurement_graphics::CYAN};
        let value_color=if m.kind==Tool::Angle {measurement_graphics::YELLOW} else {Color32::from_rgb(244,251,255)};
        let text=if selected || m.id==0 {result} else {result.split(" · ").next().unwrap_or(&result).to_owned()};
        let value=painter.layout_no_wrap(text.replace('.',","),ui_widgets::font(16.0,true),value_color);
        let title=title.map(|title|painter.layout_no_wrap(title,ui_widgets::font(11.5,false),Color32::from_rgb(168,204,226)));
        let padding=Vec2::new(9.0,5.0);
        let title_width=title.as_ref().map_or(0.0,|t|t.size().x);
        let title_height=title.as_ref().map_or(0.0,|t|t.size().y+2.0);
        let size=Vec2::new(value.size().x.max(title_width)+padding.x*2.0,value.size().y+title_height+padding.y*2.0);
        Some(MeasurementCallout{title,value,size,padding,border,value_color,outlined:m.kind!=Tool::Angle || selected})
    }

    pub(super) fn label_card_min(m: &Measurement, points: &[Pos2], rect: Rect, w: f32, h: f32,
                      clip: Rect, card_size: Vec2) -> Pos2 {
        let anchor = match m.kind {
            Tool::ObjectCount if !points.is_empty() => {
                let count = points.len() as f32;
                Pos2::new(points.iter().map(|point| point.x).sum::<f32>() / count,
                    points.iter().map(|point| point.y).sum::<f32>() / count)
            }
            Tool::Circle if points.len() >= 2 => points[0] + Vec2::new(0.0, -points[0].distance(points[1])),
            Tool::Rectangle if points.len() >= 2 => {
                let left = points[0].x.min(points[1].x);
                let right = points[0].x.max(points[1].x);
                Pos2::new((left + right) * 0.5, points[0].y.min(points[1].y))
            }
            Tool::Angle if points.len() >= 3 => points[1],
            Tool::Polygon if !points.is_empty() => *points.iter().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap(),
            _ if points.len() >= 2 => {
                let a = points[points.len() - 2];
                let b = points[points.len() - 1];
                let delta = b - a;
                let mut normal = if delta.length_sq() > 1.0 {
                    Vec2::new(-delta.y, delta.x).normalized()
                } else { Vec2::new(0.0, -1.0) };
                if normal.y > 0.0 { normal = -normal; }
                a + (b - a) * 0.5 + normal * 12.0
            }
            _ => points.last().copied().unwrap_or_else(|| rect.center()),
        };
        let default_min = match m.kind {
            Tool::Distance if points.len()>=2 => {
                let (a, b) = (points[0], points[1]);
                let middle=a+(b-a)*0.5;
                if (b.x-a.x).abs()>=(b.y-a.y).abs() {
                    Pos2::new(middle.x-card_size.x*0.5,middle.y-card_size.y-6.0)
                } else {middle-card_size*0.5}
            }
            Tool::Angle if points.len()>=3 => measurement_graphics::angle_geometry(points[0],points[1],points[2])
                .map(|arc| {
                    let direction=(arc.label_anchor-points[1]).normalized();
                    arc.label_anchor+direction*(card_size.y*0.5+10.0)-card_size*0.5
                }).unwrap_or(anchor-card_size*0.5),
            Tool::Circle if points.len()>=2 => {
                let radius=points[0].distance(points[1]);
                points[0]+Vec2::new(radius*0.75+14.0,-radius*0.75-card_size.y-10.0)
            }
            Tool::CircleFit if points.len()>=3 => geometry::fit_circle(&m.points).map(|fit| {
                let center=Self::point_to_screen(fit.center,rect,w,h);
                center+Vec2::new(fit.radius as f32*rect.width()/w*0.75+14.0,
                    -fit.radius as f32*rect.height()/h*0.75-card_size.y-10.0)
            }).unwrap_or(anchor-card_size*0.5),
            _ => Pos2::new(anchor.x-card_size.x*0.5,anchor.y-card_size.y-12.0),
        };
        let preferred_min = m.label_position_native_px
            .map(|position| Self::point_to_screen(position, rect, w, h))
            .unwrap_or(default_min);
        let max_x = (clip.right() - card_size.x - 5.0).max(clip.left() + 5.0);
        let max_y = (clip.bottom() - card_size.y - 5.0).max(clip.top() + 5.0);
        Pos2::new(preferred_min.x.clamp(clip.left() + 5.0, max_x),
            preferred_min.y.clamp(clip.top() + 5.0, max_y))
    }

    pub(super) fn measurement_label_card_rect(&self,painter:&egui::Painter,rect:Rect,
                                   w:f32,h:f32,m:&Measurement)->Option<Rect> {
        let callout=self.measurement_callout(painter,m,self.selected_measurement == Some(m.id))?;
        let points=m.points.iter().map(|p|Self::point_to_screen(*p,rect,w,h)).collect::<Vec<_>>();
        let min=Self::label_card_min(m,&points,rect,w,h,painter.clip_rect(),callout.size);
        Some(Rect::from_min_size(min,callout.size))
    }

    pub(super) fn render_measurement(&self, painter: &egui::Painter, rect: Rect,
                          w: f32, h: f32, m: &Measurement, color: Color32, selected: bool) {
        if m.is_construction {
            self.render_construction(painter, rect, w, h, m, selected);
            return;
        }
        let points: Vec<Pos2> = m.points.iter().map(|p| Self::point_to_screen(*p, rect, w, h)).collect();
        let width = if selected { measurement_graphics::SELECTED_STROKE_WIDTH }
                    else { measurement_graphics::STROKE_WIDTH };
        match m.kind {
            Tool::Polyline if m.id == 0 && self.calibration_mode
                && self.calibration_type == CalibrationType::MultiPointXY => {
                let segment_labels = ["X", "Y", "HG", "HD", "BG", "BD"];
                let segment_colors = [
                    Color32::from_rgb(28, 125, 205), Color32::from_rgb(232, 132, 35),
                    Color32::from_rgb(62, 154, 94), Color32::from_rgb(154, 88, 190),
                    Color32::from_rgb(204, 75, 86), Color32::from_rgb(34, 156, 157),
                ];
                for (index, segment) in points.chunks_exact(2).enumerate() {
                    let color = segment_colors[index.min(segment_colors.len() - 1)];
                    Self::paint_measurement_segment(painter, segment[0], segment[1], color, width);
                    for point in segment { Self::paint_vertex(painter, *point, color, 3.4); }
                    let midpoint = segment[0] + (segment[1] - segment[0]) * 0.5;
                    painter.text(midpoint, Align2::CENTER_CENTER, segment_labels[index],
                        FontId::proportional(11.0), color);
                }
                if points.len() % 2 == 1 {
                    Self::paint_vertex(painter, *points.last().unwrap(), segment_colors[points.len() / 2], 3.8);
                }
            }
            Tool::ObjectCount => {
                for point in &points {
                    Self::paint_vertex(painter, *point, color, if selected { 4.5 } else { 3.2 });
                }
            }
            Tool::Distance if points.len()>=2 => {
                let (a, b) = (points[0], points[1]);
                measurement_graphics::dimension(painter,a,b,color,width);
                for point in [points[0],points[1]] {Self::paint_vertex(painter,point,color,if selected {3.5} else {2.4});}
            }
            Tool::Polyline if points.len()>=2 => {
                for pair in points.windows(2) {measurement_graphics::segment(painter,pair[0],pair[1],color,width);}
                measurement_graphics::arrowhead(painter,points[0],points[0]-points[1],color,
                    14.0_f32.min(points[0].distance(points[1])*0.35));
                let last=points.len()-1;
                measurement_graphics::arrowhead(painter,points[last],points[last]-points[last-1],color,
                    14.0_f32.min(points[last].distance(points[last-1])*0.35));
                for point in &points {Self::paint_vertex(painter,*point,color,if selected {3.5} else {2.4});}
            }
            Tool::Angle if points.len()>=3 => {
                measurement_graphics::segment(painter,points[1],points[0],color,width);
                measurement_graphics::segment(painter,points[1],points[2],color,width);
                measurement_graphics::angle_arc(painter,points[0],points[1],points[2]);
                for point in &points {Self::paint_vertex(painter,*point,color,if selected {3.5} else {2.4});}
            }
            Tool::CircleFit | Tool::Ellipse | Tool::Arc if !points.is_empty() => {
                if let Some(curve) = geometry::sampled_curve(m.kind, &m.points, 96) {
                    let curve = curve.iter().map(|point| Self::point_to_screen(*point, rect, w, h)).collect::<Vec<_>>();
                    for pair in curve.windows(2) {
                        Self::paint_measurement_segment(painter, pair[0], pair[1], color, width);
                    }
                    if m.kind==Tool::Arc && curve.len()>=2 {
                        let arrow_length=14.0_f32.min(curve.windows(2)
                            .map(|pair|pair[0].distance(pair[1])).sum::<f32>()*0.35);
                        measurement_graphics::arrowhead(painter,curve[0],curve[0]-curve[1],color,arrow_length);
                        let last=curve.len()-1;
                        measurement_graphics::arrowhead(painter,curve[last],curve[last]-curve[last-1],color,arrow_length);
                    }
                }
                if m.kind == Tool::CircleFit {
                    if let Some(fit) = geometry::fit_circle(&m.points) {
                        let center = Self::point_to_screen(fit.center, rect, w, h);
                        let radius_x = fit.radius as f32 * rect.width() / w;
                        let radius_y = fit.radius as f32 * rect.height() / h;
                        measurement_graphics::center_mark(painter,center,radius_x,radius_y);
                        let dx=m.points[0].x-fit.center.x;
                        let dy=m.points[0].y-fit.center.y;
                        let length=dx.hypot(dy);
                        let (dx,dy)=if length>1e-9 {(dx/length,dy/length)} else {(std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2)};
                        let rim=Self::point_to_screen(Point{x:fit.center.x+dx*fit.radius,y:fit.center.y+dy*fit.radius},rect,w,h);
                        measurement_graphics::dimension(painter,center-(rim-center),rim,color,width);
                    }
                } else if m.kind == Tool::Ellipse {
                    if let Some(ellipse) = geometry::ellipse_geometry(&m.points) {
                        let center = Self::point_to_screen(ellipse.center, rect, w, h);
                        for axis in [ellipse.first_axis, ellipse.second_axis] {
                            let end = Self::point_to_screen(Point {
                                x: ellipse.center.x + axis.x,
                                y: ellipse.center.y + axis.y,
                            }, rect, w, h);
                            measurement_graphics::dimension(painter,center-(end-center),end,color,width);
                        }
                        Self::paint_vertex(painter, center, color, if selected { 4.5 } else { 3.4 });
                    }
                }
                let assisted = m.method.as_deref()
                    .is_some_and(|method| method.starts_with("Détection géométrique guidée"));
                if !assisted {
                    for point in &points {
                        Self::paint_vertex(painter, *point, color, if selected { 4.0 } else { 2.8 });
                    }
                }
            }
            Tool::PointLineDistance if points.len() >= 3 => {
                let (point, first, second) = (points[0], points[1], points[2]);
                measurement_graphics::dashed_segment(painter,first,second,measurement_graphics::CONSTRUCTION,1.0,7.0,5.0);
                let axis = second - first;
                let projection = first + axis * ((point - first).dot(axis) / axis.length_sq().max(f32::EPSILON));
                measurement_graphics::dimension(painter,point,projection,color,width);
                if axis.length_sq()>0.001 {
                    let along=axis.normalized();
                    let delta=point-projection;
                    if delta.length_sq()>0.001 {
                        let normal=delta.normalized();
                        measurement_graphics::segment(painter,projection+along*7.0,projection+along*7.0+normal*7.0,measurement_graphics::CONSTRUCTION,measurement_graphics::GUIDE_WIDTH);
                        measurement_graphics::segment(painter,projection+along*7.0+normal*7.0,projection+normal*7.0,measurement_graphics::CONSTRUCTION,measurement_graphics::GUIDE_WIDTH);
                    }
                }
                for handle in [point, first, second, projection] { Self::paint_vertex(painter, handle, color, if selected { 4.0 } else { 2.8 }); }
            }
            Tool::ParallelDistance if points.len() >= 4 => {
                measurement_graphics::dashed_segment(painter,points[0],points[1],measurement_graphics::CONSTRUCTION,1.0,7.0,5.0);
                measurement_graphics::dashed_segment(painter,points[2],points[3],measurement_graphics::CONSTRUCTION,1.0,7.0,5.0);
                let first_axis = points[1] - points[0];
                let second_midpoint = Pos2::new((points[2].x + points[3].x) * 0.5, (points[2].y + points[3].y) * 0.5);
                let first_midpoint = Pos2::new((points[0].x + points[1].x) * 0.5, (points[0].y + points[1].y) * 0.5);
                if first_axis.length_sq()>0.001 {
                    let normal=Vec2::new(-first_axis.y,first_axis.x).normalized();
                    let projection=first_midpoint+normal*(second_midpoint-first_midpoint).dot(normal);
                    measurement_graphics::dimension(painter,first_midpoint,projection,color,width);
                }
                for point in &points { Self::paint_vertex(painter, *point, color, if selected { 4.0 } else { 2.8 }); }
            }
            Tool::Circle if points.len()>=2 => {
                let radius=points[0].distance(points[1]);
                painter.circle_stroke(points[0],radius,Stroke::new(width+1.8,Color32::from_rgba_unmultiplied(3,12,22,220)));
                painter.circle_stroke(points[0],radius,Stroke::new(width,color));
                measurement_graphics::center_mark(painter,points[0],radius,radius);
                measurement_graphics::dimension(painter,points[0]-(points[1]-points[0]),points[1],color,width);
                Self::paint_vertex(painter,points[1],color,if selected {3.8} else {2.6});
            }
            Tool::Rectangle if points.len() >= 2 => {
                let rectangle = Rect::from_two_pos(points[0], points[1]);
                painter.rect_stroke(rectangle, 0,
                    Stroke::new(width + 3.0, Color32::from_rgba_unmultiplied(7, 20, 38, 190)),
                    egui::StrokeKind::Outside);
                painter.rect_stroke(rectangle, 0, Stroke::new(width, color), egui::StrokeKind::Outside);
                let opposite = Pos2::new(points[1].x, points[0].y);
                let other_opposite = Pos2::new(points[0].x, points[1].y);
                measurement_graphics::dashed_segment(painter,points[0],points[1],measurement_graphics::CONSTRUCTION,1.0,7.0,5.0);
                measurement_graphics::dashed_segment(painter,opposite,other_opposite,measurement_graphics::CONSTRUCTION,1.0,7.0,5.0);
                let center = rectangle.center();
                Self::paint_vertex(painter, center, measurement_graphics::CONSTRUCTION, 3.8);
            }
            Tool::Polygon if points.len() >= 3 => {
                for pair in points.windows(2) {
                    Self::paint_measurement_segment(painter, pair[0], pair[1], color, width);
                }
                Self::paint_measurement_segment(painter, *points.last().unwrap(), points[0], color, width);
            }
            _ => {
                for pair in points.windows(2) {Self::paint_measurement_segment(painter,pair[0],pair[1],color,width);}
                if m.kind==Tool::Coordinate && points.len()==1 {
                    let point=points[0];
                    measurement_graphics::center_mark(painter,point,8.0,8.0);
                }
                for point in &points {Self::paint_vertex(painter,*point,color,if selected {3.5} else {2.4});}
            }
        }

        if selected {
            for handle in Self::edit_handles(m, rect, w, h) {
                painter.circle_filled(handle.position, 7.5,
                    Color32::from_rgba_unmultiplied(7, 20, 38, 190));
                painter.circle_filled(handle.position, 5.5, Color32::WHITE);
                painter.circle_filled(handle.position, 3.2, color);
            }
        }

        if self.numeric_resize_dialog.as_ref().is_some_and(|dialog| dialog.measurement_id() == m.id) { return; }
        let Some(callout)=self.measurement_callout(painter,m,selected) else {return;};
        let card=Rect::from_min_size(Self::label_card_min(m,&points,rect,w,h,painter.clip_rect(),callout.size),callout.size);
        let anchor=match m.kind {
            Tool::Distance if points.len()>=2 => {
                points[0] + (points[1] - points[0]) * 0.5
            }
            Tool::Angle if points.len()>=3 => measurement_graphics::angle_geometry(points[0],points[1],points[2])
                .map(|arc|arc.label_anchor).unwrap_or(points[1]),
            Tool::Circle if points.len()>=2 => {
                let axis=card.center()-points[0];
                let length=axis.length();
                if length>0.001 {points[0]+axis/length*points[0].distance(points[1])} else {points[1]}
            }
            Tool::CircleFit | Tool::Ellipse | Tool::Arc => geometry::sampled_curve(m.kind,&m.points,96)
                .and_then(|curve|curve.into_iter().map(|p|Self::point_to_screen(p,rect,w,h))
                    .min_by(|a,b|a.distance_sq(card.center()).total_cmp(&b.distance_sq(card.center()))))
                .unwrap_or_else(||points.last().copied().unwrap_or(rect.center())),
            _ if points.len()>=2 => points[0]+(points[points.len()-1]-points[0])*0.5,
            _ => points.last().copied().unwrap_or(rect.center()),
        };
        let leader_color=if m.kind==Tool::Angle {measurement_graphics::YELLOW} else {callout.border};
        measurement_graphics::leader(painter,anchor,card,leader_color);
        if matches!(m.kind,Tool::Circle|Tool::CircleFit|Tool::Ellipse) {
            Self::paint_vertex(painter,anchor,callout.border,2.6);
        }
        painter.rect_filled(card,1,Color32::from_rgba_unmultiplied(3,10,20,235));
        if callout.outlined {painter.rect_stroke(card,1,Stroke::new(2.0,callout.border),egui::StrokeKind::Inside);}
        let mut value_pos=card.min+callout.padding;
        if let Some(title)=callout.title {
            value_pos.y+=title.size().y+2.0;
            painter.galley(card.min+callout.padding,title,Color32::from_rgb(168,204,226));
        }
        painter.galley(value_pos,callout.value,callout.value_color);
    }









    pub(super) fn paint_alignment_grid(painter: &egui::Painter, viewport: Rect, image_rect: Rect,
                            scale: f32, image_w: f32, image_h: f32) -> f32 {
        let visible = viewport.intersect(image_rect);
        if !visible.is_positive() { return 0.0; }

        let target_step = (48.0 / scale.max(f32::EPSILON)).max(1.0);
        let magnitude = 10.0_f32.powf(target_step.log10().floor());
        let normalized = target_step / magnitude;
        let multiple = if normalized <= 1.0 { 1.0 }
            else if normalized <= 2.0 { 2.0 }
            else if normalized <= 5.0 { 5.0 }
            else { 10.0 };
        let step = (magnitude * multiple).max(1.0);
        let first_x = (((visible.left() - image_rect.left()) / scale) / step).ceil() as i64;
        let last_x = (((visible.right() - image_rect.left()) / scale).min(image_w) / step).floor() as i64;
        let first_y = (((visible.top() - image_rect.top()) / scale) / step).ceil() as i64;
        let last_y = (((visible.bottom() - image_rect.top()) / scale).min(image_h) / step).floor() as i64;

        let draw_grid_line = |segment: [Pos2; 2], major: bool| {
            let (halo_alpha, grid_alpha, grid_width) = if major {
                (112, 190, 1.1)
            } else {
                (72, 118, 0.7)
            };
            // The dark underlay keeps the grid legible over both bright and dark image areas.
            painter.line_segment(
                segment,
                Stroke::new(grid_width + 1.2, Color32::from_rgba_unmultiplied(5, 18, 32, halo_alpha)),
            );
            painter.line_segment(
                segment,
                Stroke::new(grid_width, Color32::from_rgba_unmultiplied(80, 205, 230, grid_alpha)),
            );
        };
        for index in first_x..=last_x {
            let x = image_rect.left() + index as f32 * step * scale;
            let major = index.rem_euclid(5) == 0;
            let segment = [Pos2::new(x, visible.top()), Pos2::new(x, visible.bottom())];
            draw_grid_line(segment, major);
        }
        for index in first_y..=last_y {
            let y = image_rect.top() + index as f32 * step * scale;
            let major = index.rem_euclid(5) == 0;
            let segment = [Pos2::new(visible.left(), y), Pos2::new(visible.right(), y)];
            draw_grid_line(segment, major);
        }
        step
    }

    pub(super) fn paint_center_guides(
        painter: &egui::Painter,
        viewport: Rect,
        live_alignment: Option<alignment::AlignmentState>,
    ) {
        let center = viewport.center();
        let guide_color = match live_alignment {
            Some(alignment::AlignmentState::Aligned) => Color32::from_rgb(45, 205, 105),
            Some(alignment::AlignmentState::Misaligned) => Color32::from_rgb(235, 65, 72),
            Some(alignment::AlignmentState::Unknown) => Color32::from_rgb(244, 180, 55),
            None => measurement_graphics::CYAN,
        };
        for (a, b) in [
            (Pos2::new(viewport.left(), center.y), Pos2::new(viewport.right(), center.y)),
            (Pos2::new(center.x, viewport.top()), Pos2::new(center.x, viewport.bottom())),
        ] {
            measurement_graphics::dashed_segment(painter,a,b,guide_color,1.2,6.0,5.0);
        }
        measurement_graphics::center_mark(painter,center,8.0,8.0);
    }

}

//! Native inline editing and transaction regressions; no camera, server or user files.
use super::*;

struct Fixture {
    app: DperMeasureApp,
    ctx: egui::Context,
    time: f64,
    viewport: Rect,
    texts: Vec<String>,
    shapes: Vec<egui::Shape>,
}
impl Fixture {
    fn new() -> Self {
        let ctx = egui::Context::default();
        let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
        let mut rgb = vec![0; 100 * 100 * 3];
        for y in 0..100 {
            for x in 0..100 {
                let value = if (x / 8 + y / 8) % 2 == 0 { 60 } else { 196 };
                rgb[(y*100+x)*3..(y*100+x)*3+3].fill(value);
            }
        }
        app.quality_assessment = Some(quality::assess_image_quality(&rgb,100,100,&app.quality_thresholds).unwrap());
        assert!(app.quality_assessment.as_ref().unwrap().is_exploitable);
        app.texture = Some(ctx.load_texture("numeric-resize-fixture", egui::ColorImage::from_rgb([100,100], &rgb), egui::TextureOptions::LINEAR));
        app.image = Some(ImageData { width:100,height:100,rgb,source:"synthetic-numeric-resize".into() });
        app.fit_view = false;
        app.zoom = 4.0;
        app.pan = Vec2::new(23.0,-17.0);
        app.selection_mode = true;
        app.edge_snap_enabled = false;
        let measurement: Measurement = serde_json::from_value(serde_json::json!({
            "id":1,"kind":"Distance","label":"Synthetic segment","capture_id":app.capture_id,
            "points":[{"x":20.0,"y":50.0},{"x":40.0,"y":50.0}],
            "label_position_native_px":{"x":10.0,"y":15.0},
            "expanded_uncertainty_mm":0.02,"origin":"interface","method":"manual fixture"
        })).unwrap();
        app.measurements.push(measurement);
        app.inspection.dependencies.mark_created(1);
        app.next_id = 2;
        let mut fixture=Self { app,ctx,time:0.0,viewport:Rect::NOTHING, texts:Vec::new(), shapes:Vec::new() };
        fixture.settle();
        fixture
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0/60.0;
        let app=&mut self.app;
        let viewport=&mut self.viewport;
        let output=self.ctx.run(egui::RawInput {
            screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(1200.0,900.0))),
            focused:true,time:Some(self.time),events,..Default::default()
        }, |ctx| {
            app.begin_registration_photo_pick_frame(ctx);
            egui::CentralPanel::default().show(ctx,|ui| {
                app.central_image(ui);
                *viewport=ctx.data(|data|data.get_temp(egui::Id::new("measurement-canvas-viewport"))).unwrap();
            });
            app.numeric_resize_window(ctx);
        });
        self.shapes = output.shapes.iter().map(|shape| shape.shape.clone()).collect();
        self.texts.clear();
        for shape in &output.shapes { collect_text(&shape.shape, &mut self.texts); }
    }
    fn settle(&mut self) { for _ in 0..3 { self.frame(Vec::new()); } }
    fn key(&mut self,key:egui::Key) {
        self.frame(vec![egui::Event::Key { key,physical_key:None,pressed:true,repeat:false,modifiers:Default::default() }]);
        self.frame(vec![egui::Event::Key { key,physical_key:None,pressed:false,repeat:false,modifiers:Default::default() }]);
    }
    fn click(&mut self,pos:Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton {
            pos,button:PointerButton::Primary,pressed:true,modifiers:Default::default() }]);
        self.frame(vec![egui::Event::PointerButton {
            pos,button:PointerButton::Primary,pressed:false,modifiers:Default::default() }]);
    }
    fn card_center(&self)->Pos2 {
        let scale=self.app.zoom/self.ctx.pixels_per_point();
        let rect=Rect::from_center_size(self.viewport.center()+self.app.pan,Vec2::splat(100.0*scale));
        let painter=self.ctx.layer_painter(egui::LayerId::new(egui::Order::Middle,egui::Id::new("numeric-test-card")))
            .with_clip_rect(self.viewport);
        self.app.measurement_label_card_rect(&painter,rect,100.0,100.0,&self.app.measurements[0]).unwrap().center()
    }
    fn snapshot(&self)->serde_json::Value {
        serde_json::json!({"measurements":self.app.measurements,"revision":self.app.document_revision,
            "capture":self.app.capture_id,"audit":self.app.measurement_audit_history,
            "undo":self.app.undo_history.len(),"redo":self.app.redo_history.len(),"saved":self.app.session_saved})
    }
}

#[test]
fn numeric_resize_real_cartouche_double_click_opens_inline_editor_without_changing_geometry() {
    let mut f=Fixture::new();
    let before=f.snapshot();
    let card=f.card_center();
    f.click(card);
    assert!(f.app.numeric_resize_dialog.is_none());
    f.click(card);
    assert!(f.app.numeric_resize_dialog.is_some(),"The actual canvas double click must open the inline numeric editor");
    assert_eq!(f.snapshot(),before);
    assert!(f.app.point_edit.is_none());
}

#[test]
fn numeric_resize_inline_enter_commits_one_transaction_and_undo_redo_restore_provenance() {
    let mut f=Fixture::new();
    let original=f.app.measurements[0].clone();
    let revision=f.app.document_revision;
    f.app.open_numeric_resize(1);
    f.app.numeric_resize_dialog.as_mut().unwrap().primary="30".into();
    f.settle();
    f.key(egui::Key::Enter);
    assert!(f.app.numeric_resize_dialog.is_none());
    assert_eq!(f.app.document_revision,revision+1);
    assert_eq!(f.app.undo_history.len(),1);
    assert_eq!(f.app.measurement_audit_history.len(),1);
    assert!(matches!(f.app.undo_history.last(),Some(MeasurementHistoryAction::Edited {id:1,..})));
    let imposed=f.app.measurements[0].clone();
    assert_eq!(imposed.points,vec![Point{x:20.0,y:50.0},Point{x:50.0,y:50.0}]);
    assert!(imposed.is_numeric_resize());
    assert_eq!(imposed.origin,"numeric_resize");
    assert!(imposed.method.as_deref().unwrap().contains("[dper:numeric_resize:v1]"));
    assert!(imposed.method.as_deref().unwrap().contains("manual fixture"));
    assert_eq!(imposed.expanded_uncertainty_mm,None);
    f.key(egui::Key::Enter);
    assert_eq!(f.app.undo_history.len(),1);
    assert!(f.app.undo_measurement_action());
    assert_eq!(serde_json::to_value(&f.app.measurements[0]).unwrap(),serde_json::to_value(original).unwrap());
    assert!(f.app.redo_measurement_action());
    assert_eq!(serde_json::to_value(&f.app.measurements[0]).unwrap(),serde_json::to_value(imposed).unwrap());
}

#[test]
fn numeric_resize_actual_inline_escape_cancels_preview_without_mutation() {
    let mut f=Fixture::new();
    f.app.open_numeric_resize(1);
    let before=f.snapshot();
    f.app.numeric_resize_dialog.as_mut().unwrap().primary="35".into();
    f.settle();
    assert!(f.app.numeric_resize_dialog.is_some());
    assert!(f.app.mcp_mutation_blocked(), "MCP mutations must be blocked while the modal owns the edit");
    f.key(egui::Key::Escape);
    assert!(f.app.numeric_resize_dialog.is_none());
    assert!(!f.app.mcp_mutation_blocked(), "Cancelling must release the mutation guard");
    assert_eq!(f.snapshot(),before);
}

#[test]
fn numeric_resize_invalid_bounds_and_stale_revision_refuse_commit_without_mutation() {
    let mut f=Fixture::new();
    f.app.open_numeric_resize(1);
    f.app.numeric_resize_dialog.as_mut().unwrap().primary="500".into();
    let before=f.snapshot();
    assert!(f.app.commit_numeric_resize().unwrap_err().contains("image"));
    assert_eq!(f.snapshot(),before);
    assert!(f.app.numeric_resize_dialog.is_some());
    f.app.numeric_resize_dialog.as_mut().unwrap().primary="30".into();
    f.app.document_revision+=1;
    let before=f.snapshot();
    assert!(f.app.commit_numeric_resize().unwrap_err().contains("contexte"));
    assert_eq!(f.snapshot(),before);
    assert!(f.app.numeric_resize_dialog.is_some());
}

#[test]
fn numeric_resize_suspends_existing_descendants_and_cannot_be_used_as_observed_source() {
    let mut f=Fixture::new();
    let mut descendant=f.app.measurements[0].clone();
    descendant.id=2;
    descendant.source_measurement_ids=vec![1];
    f.app.measurements.push(descendant);
    f.app.inspection.dependencies.mark_created(2);
    f.app.refresh_dependency_flags();
    f.app.open_numeric_resize(1);
    f.app.numeric_resize_dialog.as_mut().unwrap().primary="30".into();
    f.app.commit_numeric_resize().unwrap();
    assert!(f.app.measurements[1].dependency_stale);
    assert!(geometry::result(&f.app.measurements[1]).contains("À recalculer"));
    assert!(f.app.validate_dependency_sources(&f.app.capture_id,&[1]).is_err());
    assert!(crate::measurement_graph::validate_sources(&f.app.measurements,&f.app.capture_id,&[1]).is_err());
    let persisted:Measurement=serde_json::from_value(serde_json::to_value(&f.app.measurements[0]).unwrap()).unwrap();
    assert!(persisted.is_numeric_resize());
    assert_eq!(reference::measurement_mm(&persisted),None);
    assert_eq!(f.app.undo_history.len(),1);
}


#[test]
fn numeric_resize_real_construction_rectangle_double_click_opens_two_dimensions_without_cartouche() {
    let mut f=Fixture::new();
    let rectangle:Measurement=serde_json::from_value(serde_json::json!({
        "id":1,"kind":"Rectangle","label":"Synthetic construction","capture_id":f.app.capture_id,
        "points":[{"x":20.0,"y":30.0},{"x":80.0,"y":70.0}],
        "is_construction":true,"origin":"interface"
    })).unwrap();
    f.app.measurements[0]=rectangle;
    f.settle();
    let before=f.snapshot();
    let scale=f.app.zoom/f.ctx.pixels_per_point();
    let image_rect=Rect::from_center_size(f.viewport.center()+f.app.pan,Vec2::splat(100.0*scale));
    // Select on the visible left edge, away from corners and centre handles.
    let position=DperMeasureApp::point_to_screen(Point{x:20.0,y:40.0},image_rect,100.0,100.0);
    let painter=f.ctx.layer_painter(egui::LayerId::new(egui::Order::Middle,egui::Id::new("construction-numeric-test")))
        .with_clip_rect(f.viewport);
    assert!(f.app.measurement_label_card_rect(&painter,image_rect,100.0,100.0,&f.app.measurements[0]).is_none());
    f.click(position);
    assert!(f.app.numeric_resize_dialog.is_none());
    f.click(position);
    let dialog=f.app.numeric_resize_dialog.as_ref().expect("A double click on a construction trace opens the numeric editor");
    assert_eq!(numeric_resize::parse_value(&dialog.primary).unwrap(),60.0);
    assert_eq!(numeric_resize::parse_value(dialog.secondary.as_deref().unwrap()).unwrap(),40.0);
    assert!(dialog.before.is_construction);
    assert_eq!(f.snapshot(),before);
    f.key(egui::Key::Escape);
    assert!(f.app.numeric_resize_dialog.is_none());
    assert_eq!(f.snapshot(),before);
}

fn collect_text(shape: &egui::Shape, texts: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => texts.push(text.galley.job.text.clone()),
        egui::Shape::Vec(shapes) => for shape in shapes { collect_text(shape, texts); },
        _ => {}
    }
}

#[test]
fn numeric_resize_inline_typing_replaces_value_at_cartouche_and_keeps_name_and_value_after_commit() {
    let mut f=Fixture::new();
    f.app.open_numeric_resize(1);
    let original_card=f.card_center();
    f.settle();
    let position=f.app.numeric_resize_dialog.as_ref().unwrap().position.unwrap();
    assert!(position.distance(original_card)<150.0, "Editor must stay at the cartouche");
    assert!(!f.texts.join(" ").contains("Redimensionner la géométrie"));
    assert!(!f.texts.join(" ").contains("Géométrie imposée"));
    f.frame(vec![egui::Event::Text("35,5".into())]);
    assert_eq!(f.app.numeric_resize_dialog.as_ref().unwrap().primary,"35,5");
    f.key(egui::Key::Enter);
    assert!(f.app.numeric_resize_dialog.is_none());
    assert!((f.app.measurements[0].points[1].x-55.5).abs()<1e-9);
    assert!(f.app.measurements[0].is_numeric_resize());
    f.app.set_measurement_selection(None);
    f.settle();
    let texts=f.texts.join(" ");
    assert!(texts.contains("Synthetic segment"), "Name must remain visible: {texts}");
    assert!(texts.contains("35,5"), "Value must remain visible: {texts}");
    assert!(!texts.contains("Géométrie imposée"));
}

#[test]
fn numeric_resize_inline_outside_click_and_unchanged_rounded_value_leave_geometry_intact() {
    let mut f=Fixture::new();
    f.app.measurements[0].points[1].x=20.999999999999996;
    f.app.open_numeric_resize(1);
    assert_eq!(f.app.numeric_resize_dialog.as_ref().unwrap().primary,"1");
    let before=f.snapshot();
    f.settle();
    f.key(egui::Key::Enter);
    assert_eq!(f.snapshot(),before);
    f.app.open_numeric_resize(1);
    f.app.numeric_resize_dialog.as_mut().unwrap().primary="10".into();
    f.settle();
    f.click(Pos2::new(25.0,25.0));
    assert!(f.app.numeric_resize_dialog.is_none());
    assert_eq!(f.snapshot(),before);
}

#[test]
fn numeric_resize_inline_values_have_contrasting_background_in_light_and_dark_themes() {
    fn inspect(shape: &egui::Shape, backgrounds: &mut usize, values: &mut usize) {
        match shape {
            egui::Shape::Rect(rect) if rect.fill==Color32::from_rgb(18,38,58) => *backgrounds+=1,
            egui::Shape::Text(text) if matches!(text.galley.job.text.as_str(), "20" | "30") => {
                let color=text.override_text_color.unwrap_or(text.galley.job.sections[0].format.color);
                assert_eq!(color,Color32::WHITE,"The numeric value must contrast with its dark input");
                *values+=1;
            }
            egui::Shape::Vec(shapes) => for shape in shapes { inspect(shape,backgrounds,values); },
            _ => {}
        }
    }
    for dark in [false,true] {
        let mut f=Fixture::new();
        f.ctx.set_visuals(if dark {egui::Visuals::dark()} else {egui::Visuals::light()});
        f.app.measurements[0].kind=Tool::Rectangle;
        f.app.measurements[0].points=vec![Point{x:20.0,y:30.0},Point{x:40.0,y:60.0}];
        f.app.open_numeric_resize(1);
        f.settle();
        // Let the Area opening fade finish before checking exact paint colours.
        for _ in 0..30 { f.frame(Vec::new()); }
        for focused in [true,false] {
            if !focused { f.ctx.memory_mut(|m|m.surrender_focus(egui::Id::new("numeric-resize-primary"))); f.settle(); }
            let (mut backgrounds,mut values)=(0,0);
            for shape in &f.shapes { inspect(shape,&mut backgrounds,&mut values); }
            assert!(backgrounds>=2,"Both numeric fields need dark backgrounds (dark={dark}, focused={focused})");
            assert!(values>=2,"Both prefilled numeric values must be painted");
        }
    }
}

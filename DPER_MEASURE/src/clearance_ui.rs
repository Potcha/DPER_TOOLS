//! Native draft for fixed-center clearance; execution shares the MCP adapter.
use crate::{
    dper_clearance::{ClearanceReference, ClearanceRequest, ClearanceStop, ClearanceStrip},
    dper_inspection::InspectionRecord,
    dper_vision::{EdgePolarity, VisionOptions},
    geometry::{Measurement, Point, Tool},
    mcp_server::{InspectClearanceArgs, InspectionRequest},
    DperMeasureApp,
};
use eframe::egui;

pub(crate) struct ClearanceUi {
    feature: String,
    group: String,
    draft: Option<ClearanceDraft>,
    status: String,
    result: Option<InspectionRecord>,
    #[cfg(test)]
    widgets: std::collections::BTreeMap<&'static str, egui::Rect>,
}

impl Default for ClearanceUi {
    fn default() -> Self {
        Self {
            feature: "Dégagement centré".into(), group: "Groupe 1".into(),
            draft: None, status: String::new(), result: None,
            #[cfg(test)]
            widgets: Default::default(),
        }
    }
}

struct ClearanceDraft {
    capture_id: String,
    revision: u64,
    reference_id: u64,
    stop_ids: Vec<u64>,
    request: ClearanceRequest,
    preview: Option<serde_json::Value>,
}

impl ClearanceDraft {
    fn source_ids(&self) -> Vec<u64> {
        let mut ids = vec![self.reference_id];
        for id in &self.stop_ids {
            if !ids.contains(id) { ids.push(*id); }
        }
        ids
    }

    fn signature(&self, feature: &str, group: &str) -> serde_json::Value {
        serde_json::json!({ "capture": self.capture_id, "request": self.request,
            "feature": feature, "group": group, "sources": self.source_ids() })
    }

    fn can_commit(&self, capture_id: &str, revision: u64, feature: &str, group: &str) -> bool {
        self.capture_id == capture_id && self.revision == revision
            && self.preview.as_ref() == Some(&self.signature(feature, group))
    }
}

fn reference_edges(measurement: &Measurement) -> Result<[ClearanceStrip; 4], String> {
    if !measurement.is_construction {
        return Err("Sélectionnez une construction rectangle, exclue des cotes.".into());
    }
    let corners: [Point; 4] = match (measurement.kind, measurement.points.as_slice()) {
        (Tool::Rectangle, [a, b]) => [*a, Point { x: b.x, y: a.y }, *b, Point { x: a.x, y: b.y }],
        (Tool::Polygon, points) if points.len() == 4 => [points[0], points[1], points[2], points[3]],
        _ => return Err("La référence exige un rectangle à deux coins ou un polygone à quatre sommets cycliques.".into()),
    };
    if corners.iter().any(|point| !point.x.is_finite() || !point.y.is_finite())
        || (0..4).any(|index| corners[index].dist(corners[(index + 1) % 4]) <= 1.0)
    {
        return Err("Les quatre côtés doivent être finis et non dégénérés.".into());
    }
    Ok(std::array::from_fn(|index| ClearanceStrip {
        start: corners[index], end: corners[(index + 1) % 4], polarity: EdgePolarity::Rising,
    }))
}

fn strip_editor(ui: &mut egui::Ui, strip: &mut ClearanceStrip) {
    ui.horizontal(|ui| {
        ui.label("Départ X / Y");
        ui.add(egui::DragValue::new(&mut strip.start.x).speed(0.25).range(-1e9..=1e9));
        ui.add(egui::DragValue::new(&mut strip.start.y).speed(0.25).range(-1e9..=1e9));
    });
    ui.horizontal(|ui| {
        ui.label("Fin X / Y");
        ui.add(egui::DragValue::new(&mut strip.end.x).speed(0.25).range(-1e9..=1e9));
        ui.add(egui::DragValue::new(&mut strip.end.y).speed(0.25).range(-1e9..=1e9));
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Vers la normale gauche départ → fin :");
        ui.selectable_value(&mut strip.polarity, EdgePolarity::Rising, "Plus clair");
        ui.selectable_value(&mut strip.polarity, EdgePolarity::Falling, "Plus sombre");
    });
}

impl DperMeasureApp {
    fn selected_clearance_source(&self) -> Result<&Measurement, String> {
        let id = self.selected_measurement.ok_or("Sélectionnez une construction dans l'image.")?;
        let mut matches = self.measurements.iter().filter(|measurement| measurement.id == id);
        let measurement = matches.next().ok_or("La sélection n'existe plus.")?;
        if matches.next().is_some() || measurement.capture_id != self.capture_id
            || !measurement.is_construction || measurement.dependency_stale
        {
            return Err("La source doit être une construction unique, actuelle et de cette capture.".into());
        }
        Ok(measurement)
    }

    fn prepare_clearance_draft(&self, state: &mut ClearanceUi) -> Result<(), String> {
        let measurement = self.selected_clearance_source()?;
        let edges = reference_edges(measurement)?;
        state.draft = Some(ClearanceDraft {
            capture_id: self.capture_id.clone(), revision: self.document_revision,
            reference_id: measurement.id, stop_ids: Vec::new(), preview: None,
            request: ClearanceRequest {
                reference: ClearanceReference::ObservedRectangle { edges },
                stops: Vec::new(), options: VisionOptions::default(),
            },
        });
        state.result = None;
        Ok(())
    }

    fn add_clearance_stop(&self, state: &mut ClearanceUi) -> Result<(), String> {
        let draft = state.draft.as_mut().ok_or("Préparez d'abord la référence rectangle.")?;
        if draft.capture_id != self.capture_id || draft.revision != self.document_revision {
            return Err("Brouillon périmé : préparez de nouveau la référence et les butées.".into());
        }
        let measurement = self.selected_clearance_source()?;
        if draft.request.stops.len() >= 16 { return Err("16 butées au maximum.".into()); }
        if draft.source_ids().contains(&measurement.id) {
            return Err("Cette construction est déjà utilisée dans le brouillon.".into());
        }
        let [start, end] = measurement.points.as_slice() else {
            return Err("Une butée exige une construction droite à deux points.".into());
        };
        if measurement.kind != Tool::Distance {
            return Err("Sélectionnez une construction Distance pour localiser la butée.".into());
        }
        draft.request.stops.push(ClearanceStop {
            id: format!("M{}", measurement.id),
            strip: ClearanceStrip { start: *start, end: *end, polarity: EdgePolarity::Rising },
        });
        draft.stop_ids.push(measurement.id);
        draft.preview = None;
        state.result = None;
        Ok(())
    }

    fn run_clearance_draft(&mut self, state: &mut ClearanceUi, commit: bool) -> Result<(), String> {
        let draft = state.draft.as_ref().ok_or("Préparez d'abord le brouillon.")?;
        if draft.capture_id != self.capture_id || draft.revision != self.document_revision {
            return Err("Brouillon périmé : capture ou document modifié. Préparez une nouvelle référence.".into());
        }
        if state.feature.trim().is_empty() || state.group.trim().is_empty() {
            return Err("Renseignez le nom de caractéristique et le groupe.".into());
        }
        if commit && !draft.can_commit(&self.capture_id, self.document_revision, &state.feature, &state.group) {
            return Err("Prévisualisez les réglages actuels avant d'enregistrer.".into());
        }
        let args = InspectClearanceArgs {
            capture_id: draft.capture_id.clone(), expected_revision: draft.revision,
            feature_id: state.feature.clone(), group: state.group.clone(),
            request: draft.request.clone(), source_measurement_ids: draft.source_ids(), commit,
        };
        self.handle_inspection_request(InspectionRequest::InspectClearance(args))?;
        state.result = self.inspection.records.last().cloned();
        if commit { state.draft = None; }
        else if let Some(draft) = state.draft.as_mut() {
            draft.revision = self.document_revision;
            draft.preview = Some(draft.signature(&state.feature, &state.group));
        }
        Ok(())
    }

    pub(crate) fn clearance_panel(&mut self, ui: &mut egui::Ui) {
        let mut state = std::mem::take(&mut self.clearance_ui);
        let mut prepare = false;
        let mut add_stop = false;
        let mut preview = false;
        let mut commit = false;
        egui::CollapsingHeader::new("Dégagement à centre fixe entre butées")
            .default_open(false).show(ui, |ui| {
                ui.label("Le centre est construit à partir des quatre côtés observés du rectangle. Le rayon est la plus petite distance aux butées déclarées ; aucune couverture circulaire continue n'est établie.");
                let mut edited = false;
                ui.horizontal(|ui| { ui.label("Caractéristique"); edited |= ui.text_edit_singleline(&mut state.feature).changed(); });
                ui.horizontal(|ui| {
                    ui.label("Groupe de contrôle");
                    let response = ui.text_edit_singleline(&mut state.group);
                    edited |= response.changed();
                    #[cfg(test)]
                    state.widgets.insert("group", response.rect);
                });
                let response = ui.button("Préparer depuis la construction rectangle sélectionnée");
                prepare = response.clicked();
                #[cfg(test)]
                state.widgets.insert("prepare", response.rect);
                ui.label("Sélectionnez ensuite chaque construction droite puis ajoutez-la comme butée. Les constructions localisent les recherches ; seuls les pixels fournissent les contours.");
                if let Some(draft) = state.draft.as_mut() {
                    let before = draft.signature(&state.feature, &state.group);
                    let stale = draft.capture_id != self.capture_id || draft.revision != self.document_revision;
                    if stale { ui.colored_label(egui::Color32::YELLOW, "Brouillon périmé : préparez de nouveau le rectangle et les butées."); }
                    ui.label(format!("Sources : {:?} · coordonnées en pixels natifs", draft.source_ids()));
                    if let ClearanceReference::ObservedRectangle { edges } = &mut draft.request.reference {
                        ui.collapsing("Quatre recherches de référence, ordre cyclique", |ui| {
                            for (index, edge) in edges.iter_mut().enumerate() {
                                ui.push_id(("clearance_reference", index), |ui| {
                                    ui.label(format!("Côté {}", index + 1)); strip_editor(ui, edge);
                                });
                            }
                        });
                    }
                    let response = ui.add_enabled(!stale, egui::Button::new("Ajouter la droite sélectionnée comme butée"));
                    add_stop = response.clicked();
                    #[cfg(test)]
                    state.widgets.insert("add", response.rect);
                    let mut remove = None;
                    for (index, stop) in draft.request.stops.iter_mut().enumerate() {
                        ui.push_id(("clearance_stop", index), |ui| {
                            ui.collapsing(format!("Butée {}", stop.id), |ui| {
                                ui.horizontal(|ui| { ui.label("Identifiant"); ui.text_edit_singleline(&mut stop.id); });
                                strip_editor(ui, &mut stop.strip);
                                if ui.button("Retirer cette butée du brouillon").clicked() { remove = Some(index); }
                            });
                        });
                    }
                    if let Some(index) = remove { draft.request.stops.remove(index); draft.stop_ids.remove(index); }
                    ui.collapsing("Réglages de recherche", |ui| {
                        let options = &mut draft.request.options;
                        ui.horizontal(|ui| { ui.label("Profils"); ui.add(egui::DragValue::new(&mut options.sample_count).range(16..=256)); });
                        ui.horizontal(|ui| { ui.label("Demi-largeur (px)"); ui.add(egui::DragValue::new(&mut options.search_radius_px).speed(0.1).range(1.0..=32.0)); });
                        ui.horizontal(|ui| { ui.label("Gradient minimum"); ui.add(egui::DragValue::new(&mut options.minimum_gradient).speed(0.1).range(0.1..=255.0)); });
                        ui.horizontal(|ui| { ui.label("Couverture de chaque support"); ui.add(egui::DragValue::new(&mut options.minimum_coverage).speed(0.01).range(0.5..=1.0)); });
                        ui.horizontal(|ui| { ui.label("Rejet des écarts (px)"); ui.add(egui::DragValue::new(&mut options.outlier_threshold_px).speed(0.05).range(0.05..=5.0)); });
                        ui.horizontal(|ui| { ui.label("RMS maximum (px)"); ui.add(egui::DragValue::new(&mut options.maximum_rms_px).speed(0.01).range(0.01..=options.outlier_threshold_px)); });
                    });
                    edited |= before != draft.signature(&state.feature, &state.group);
                    if edited { draft.preview = None; }
                    let response = ui.add_enabled(!stale, egui::Button::new("Prévisualiser le dégagement"));
                    preview = response.clicked();
                    #[cfg(test)]
                    state.widgets.insert("preview", response.rect);
                    let response = ui.add_enabled(draft.can_commit(&self.capture_id, self.document_revision, &state.feature, &state.group),
                        egui::Button::new("Enregistrer explicitement le dégagement"));
                    commit = response.clicked();
                    #[cfg(test)]
                    state.widgets.insert("commit", response.rect);
                }
                if edited { state.status.clear(); state.result = None; }
                if let Some(record) = &state.result {
                    ui.separator();
                    ui.label(if record.measurement_id.is_some() { "Dernier dégagement enregistré" } else { "Dernier aperçu : aucune mesure créée" });
                    if let Some(proof) = &record.clearance {
                        let result = &proof.result;
                        ui.label(format!("Centre ({:.3}, {:.3}) px · rayon {:.3} px · diamètre {:.3} px · butée limitante {}",
                            result.center.x, result.center.y, result.radius_px, result.diameter_px, result.limiting_stop_id));
                        for stop in &result.stops {
                            ui.label(format!("{} : distance {:.3} px · jeu {:.3} px · contact ({:.3}, {:.3}) · RMS {:.3} px · couverture du support {:.0} %",
                                stop.id, stop.distance_px, stop.gap_px, stop.closest_point.x, stop.closest_point.y,
                                stop.evidence.rms_residual_px, stop.evidence.coverage_fraction * 100.0));
                        }
                    }
                    ui.label("Résultat de la dernière opération ; toute modification des sources impose une nouvelle observation. Conformité physique et incertitude totale restent à établir.");
                }
                if !state.status.is_empty() { ui.label(&state.status); }
            });
        if prepare {
            state.status = match self.prepare_clearance_draft(&mut state) {
                Ok(()) => "Référence préparée ; vérifiez les recherches et leur polarité, puis ajoutez les butées.".into(), Err(error) => error,
            };
        }
        if add_stop {
            state.status = match self.add_clearance_stop(&mut state) {
                Ok(()) => "Butée ajoutée : vérifiez le côté recherché et la polarité.".into(), Err(error) => error,
            };
        }
        if preview || commit {
            state.status = match self.run_clearance_draft(&mut state, commit) {
                Ok(()) if commit => "Dégagement enregistré à partir des supports observés.".into(),
                Ok(()) => "Aperçu calculé ; vérifiez chaque contact avant l'enregistrement explicite.".into(),
                Err(error) => error,
            };
        }
        self.clearance_ui = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        app: DperMeasureApp,
        ctx: egui::Context,
        shapes: Vec<egui::epaint::ClippedShape>,
        time: f64,
        guide_ids: Vec<u64>,
    }

    impl Fixture {
        fn new() -> Self {
            let ctx = egui::Context::default();
            let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
            let transition = |distance: f64| 0.5 + 0.5 * (distance / 0.65).tanh();
            let image = image::RgbImage::from_fn(280, 280, |x, y| {
                let x = x as f64;
                let y = y as f64;
                let rectangle = |left: f64, top: f64, right: f64, bottom: f64|
                    transition(x - left) * transition(right - x)
                        * transition(y - top) * transition(bottom - y);
                let occupancy = rectangle(124.0, 129.0, 156.0, 151.0)
                    .max(rectangle(200.0, 110.0, 212.0, 170.0))
                    .max(rectangle(65.0, 110.0, 77.0, 170.0))
                    .max(rectangle(110.0, 69.0, 170.0, 81.0))
                    .max(rectangle(110.0, 205.0, 170.0, 217.0));
                image::Rgb([(20.0 + 210.0 * occupancy).round() as u8; 3])
            });
            app.image = Some(crate::ImageData {
                width: 280, height: 280, rgb: image.as_raw().clone(), source: "clearance-ui-synthetic.png".into(),
            });
            let mut quality = crate::quality::assess_image_quality(image.as_raw(), 280, 280,
                &crate::quality::ImageQualityThresholds::default()).unwrap();
            // Isolate UI/contour gates; this is not physical qualification.
            quality.is_exploitable = true;
            app.quality_assessment = Some(quality);
            app.calibration = Some(serde_json::from_value(serde_json::json!({
                "id": "ui-independent-calibration", "device": "fixture", "resolution": [280, 280],
                "optical_setup": "fixture", "mm_per_pixel": 0.1,
                "reference_mm": 10.0, "reference_px": 100.0, "expanded_uncertainty_mm": 0.002
            })).unwrap());
            app.mm_per_pixel = Some(0.1);
            let guides = [
                (Tool::Rectangle, [(124.0, 129.0), (156.0, 151.0)]),
                (Tool::Distance, [(200.0, 162.0), (200.0, 118.0)]),
                (Tool::Distance, [(77.0, 118.0), (77.0, 162.0)]),
                (Tool::Distance, [(162.0, 81.0), (118.0, 81.0)]),
                (Tool::Distance, [(118.0, 205.0), (162.0, 205.0)]),
            ];
            let mut guide_ids = Vec::new();
            for (kind, points) in guides {
                let measurement = app.add_measurement_with_construction(kind,
                    points.into_iter().map(|(x, y)| Point { x, y }).collect(), None,
                    app.capture_id.clone(), Some("ui-independent-calibration".into()), Some(0.1),
                    "interface".into(), None, true, Vec::new());
                guide_ids.push(measurement.id);
            }
            app.selected_measurement = Some(guide_ids[0]);
            let mut fixture = Self { app, ctx, shapes: Vec::new(), time: 0.0, guide_ids };
            fixture.settle();
            let heading = fixture.text_rect("Dégagement à centre fixe entre butées");
            fixture.click_at(heading.center());
            fixture
        }

        fn frame(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
            self.time += 1.0 / 60.0;
            self.shapes = self.ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 3000.0))),
                time: Some(self.time), focused: true, events, modifiers, ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| self.app.clearance_panel(ui));
            }).shapes;
        }

        fn settle(&mut self) { for _ in 0..3 { self.frame(Vec::new(), Default::default()); } }

        fn text_rect(&self, needle: &str) -> egui::Rect {
            fn collect(shape: &egui::epaint::Shape, needle: &str, result: &mut Vec<egui::Rect>) {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == needle =>
                        result.push(text.galley.rect.translate(text.pos.to_vec2())),
                    egui::epaint::Shape::Vec(shapes) => for shape in shapes { collect(shape, needle, result); },
                    _ => (),
                }
            }
            let mut found = Vec::new();
            for shape in &self.shapes { collect(&shape.shape, needle, &mut found); }
            *found.first().unwrap_or_else(|| panic!("Visible text absent: {needle}"))
        }

        fn click_at(&mut self, pos: egui::Pos2) {
            self.frame(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
                pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default(),
            }], Default::default());
            self.frame(vec![egui::Event::PointerButton {
                pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default(),
            }], Default::default());
            self.settle();
        }

        fn click(&mut self, key: &'static str) {
            self.click_at(self.app.clearance_ui.widgets[key].center());
        }

        fn replace_group(&mut self, text: &str) {
            self.click("group");
            let modifiers = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
            self.frame(vec![egui::Event::Key {
                key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: true,
                repeat: false, modifiers,
            }], modifiers);
            self.frame(vec![egui::Event::Key {
                key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: false,
                repeat: false, modifiers: Default::default(),
            }, egui::Event::Text(text.into())], Default::default());
            self.settle();
        }

        fn prepare_via_widgets(&mut self) {
            let revision = self.app.document_revision;
            self.click("prepare");
            assert_eq!(self.app.document_revision, revision);
            assert_eq!(self.app.measurements.len(), 5);
            // Localize the straight portions, leaving corner transitions outside
            // the searches; the full rectangle guides are not measured corners.
            let draft = self.app.clearance_ui.draft.as_mut().unwrap();
            if let ClearanceReference::ObservedRectangle { edges } = &mut draft.request.reference {
                for edge in edges {
                    let start = edge.start;
                    let end = edge.end;
                    edge.start = Point { x: start.x * 0.8 + end.x * 0.2, y: start.y * 0.8 + end.y * 0.2 };
                    edge.end = Point { x: start.x * 0.2 + end.x * 0.8, y: start.y * 0.2 + end.y * 0.8 };
                }
            }
            for id in self.guide_ids[1..].to_vec() {
                self.app.selected_measurement = Some(id);
                self.click("add");
            }
            assert_eq!(self.app.clearance_ui.draft.as_ref().unwrap().request.stops.len(), 4);
            assert!(self.app.inspection.records.is_empty());
        }
    }

    #[test]
    fn native_pointer_prepare_add_preview_and_commit_record_only_observed_clearance() {
        let mut fixture = Fixture::new();
        let calibration = serde_json::to_value(&fixture.app.calibration).unwrap();
        fixture.replace_group("Pièce témoin");
        fixture.prepare_via_widgets();
        // A real polarity control must reach the shared detector, rather than
        // merely changing a label or preserving the previous successful fit.
        let reference_header = fixture.text_rect("Quatre recherches de référence, ordre cyclique");
        fixture.click_at(reference_header.center());
        let falling = fixture.text_rect("Plus sombre");
        fixture.click_at(falling.center());
        fixture.click("preview");
        assert!(fixture.app.inspection.records.is_empty(), "Wrong polarity must fail without an audit result");
        assert_eq!(fixture.app.measurements.len(), 5);
        let rising = fixture.text_rect("Plus clair");
        fixture.click_at(rising.center());
        fixture.click("preview");
        assert_eq!(fixture.app.measurements.len(), 5, "Preview creates no native objects");
        let preview = fixture.app.inspection.records.last().unwrap_or_else(|| panic!("{}", fixture.app.clearance_ui.status));
        assert_eq!(preview.group, "Pièce témoin");
        assert!(preview.measurement_id.is_none());
        let proof = preview.clearance.as_ref().unwrap();
        assert!((proof.result.center.x - 140.0).abs() < 0.5);
        assert!((proof.result.center.y - 140.0).abs() < 0.5);
        assert!((preview.value_mm.unwrap() - 11.8).abs() < 0.15);
        assert_eq!(proof.result.stops.len(), 4);
        assert!(proof.result.stops.iter().all(|stop| !stop.evidence.accepted_points.is_empty()));
        fixture.click("commit");
        let record = fixture.app.inspection.records.last().unwrap();
        let id = record.measurement_id.unwrap_or_else(|| panic!("{}", fixture.app.clearance_ui.status));
        assert_eq!(fixture.app.measurements.iter().filter(|measurement| !measurement.is_construction).count(), 1);
        let circle = fixture.app.measurements.iter().find(|measurement| measurement.id == id).unwrap();
        assert_eq!(circle.kind, Tool::Circle);
        assert!(fixture.guide_ids.iter().all(|id| circle.source_measurement_ids.contains(id)));
        assert_eq!(fixture.app.measurements.len(), 13, "Five guides, seven derived constructions, one circle");
        assert!(record.result.evidence.accepted_points.is_empty());
        assert_eq!(record.result.evidence.sample_count, 0);
        assert_eq!(record.result.evidence.coverage_fraction, 0.0);
        assert_eq!(record.outcome.conformity_status, crate::dper_metrology::ConformityStatus::Indeterminate);
        assert!(fixture.app.clearance_ui.draft.is_none());
        assert_eq!(serde_json::to_value(&fixture.app.calibration).unwrap(), calibration);
    }

    #[test]
    fn native_controls_refuse_commit_after_group_strip_revision_or_capture_change() {
        for change in 0..4 {
            let mut fixture = Fixture::new();
            fixture.prepare_via_widgets();
            fixture.click("preview");
            assert!(fixture.app.clearance_ui.draft.as_ref().unwrap().preview.is_some(), "{}", fixture.app.clearance_ui.status);
            match change {
                0 => fixture.replace_group("Autre groupe"),
                1 => {
                    fixture.app.clearance_ui.draft.as_mut().unwrap().request.stops[0].strip.start.x += 0.5;
                    fixture.settle();
                }
                2 => { fixture.app.document_revision += 1; fixture.settle(); }
                _ => { fixture.app.capture_id = "another-capture".into(); fixture.settle(); }
            }
            let revision = fixture.app.document_revision;
            let inspection = serde_json::to_value(&fixture.app.inspection).unwrap();
            let measurements = serde_json::to_value(&fixture.app.measurements).unwrap();
            fixture.click("commit");
            assert_eq!(fixture.app.document_revision, revision, "change={change}");
            assert_eq!(serde_json::to_value(&fixture.app.inspection).unwrap(), inspection, "change={change}");
            assert_eq!(serde_json::to_value(&fixture.app.measurements).unwrap(), measurements, "change={change}");
            assert!(fixture.app.inspection.records.iter().all(|record| record.measurement_id.is_none()));
        }
    }

    #[test]
    fn reference_searches_are_cyclic_and_require_an_auxiliary_construction() {
        let mut rectangle: Measurement = serde_json::from_value(serde_json::json!({
            "id": 1, "kind": "Rectangle", "label": "Référence",
            "is_construction": true,
            "points": [{"x": 10.0, "y": 20.0}, {"x": 90.0, "y": 80.0}]
        })).unwrap();
        let edges = reference_edges(&rectangle).unwrap();
        assert_eq!(edges[0].start, Point { x: 10.0, y: 20.0 });
        assert_eq!(edges[0].end, Point { x: 90.0, y: 20.0 });
        for index in 0..4 { assert_eq!(edges[index].end, edges[(index + 1) % 4].start); }
        rectangle.is_construction = false;
        assert!(reference_edges(&rectangle).is_err());
        rectangle.is_construction = true;
        rectangle.points[1] = rectangle.points[0];
        assert!(reference_edges(&rectangle).is_err());
    }

    #[test]
    fn preview_identity_binds_edits_sources_capture_and_document_revision() {
        let mut draft = ClearanceDraft {
            capture_id: "capture".into(), revision: 7, reference_id: 1, stop_ids: vec![2, 3],
            request: ClearanceRequest {
                reference: ClearanceReference::SuppliedCenter { center: Point { x: 50.0, y: 50.0 } },
                stops: Vec::new(), options: VisionOptions::default(),
            }, preview: None,
        };
        assert!(!draft.can_commit("capture", 7, "F", "G"));
        draft.preview = Some(draft.signature("F", "G"));
        assert!(draft.can_commit("capture", 7, "F", "G"));
        assert!(!draft.can_commit("capture", 8, "F", "G"));
        assert!(!draft.can_commit("other", 7, "F", "G"));
        assert!(!draft.can_commit("capture", 7, "F", "other"));
        draft.stop_ids.push(4);
        assert!(!draft.can_commit("capture", 7, "F", "G"));
        draft.stop_ids.pop();
        draft.request.options.search_radius_px += 1.0;
        assert!(!draft.can_commit("capture", 7, "F", "G"));
    }
}

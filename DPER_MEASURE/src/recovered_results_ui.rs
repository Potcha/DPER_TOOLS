//! Original results panel recovered from the pre-sync source.
use super::*;

impl DperMeasureApp {
    pub(super) fn recovered_results_panel(&mut self, ctx: &egui::Context) {
        let display_correction_before = self.display_correction;
        let mut display_correction_changed = false;
        let mut quality_thresholds_changed = false;
        let mut selected_protocol_id = self.active_protocol.as_ref()
            .map(|protocol| protocol.id.clone()).unwrap_or_default();
        let mut latest_protocols = Vec::<storage::MeasurementProtocol>::new();
        for protocol in &self.settings.protocols {
            if let Some(existing) = latest_protocols.iter_mut()
                .find(|existing| existing.id == protocol.id)
            {
                if protocol.version > existing.version { *existing = protocol.clone(); }
            } else {
                latest_protocols.push(protocol.clone());
            }
        }
        latest_protocols.sort_by(|left, right| left.name.cmp(&right.name));
        egui::SidePanel::right("results").default_width(UiTokens::RIGHT_WIDTH).min_width(300.0)
            .frame(egui::Frame::new().fill(UiTokens::SURFACE_0)
                .stroke(Stroke::new(1.0, Color32::from_rgb(214, 224, 236)))
                .inner_margin(egui::Margin::symmetric(12, 10))
                .shadow(egui::Shadow {
                    offset: [-1, 0], blur: 5, spread: 0,
                    color: Color32::from_black_alpha(18),
                }))
            .show(ctx,|ui| {
            let mut requested_selection = None;
            let mut requested_annotation_selection = None;
            let mut label_edits = Vec::new();
            let mut annotation_text_changed = false;
            let mut removed_annotations = Vec::new();
            let photo_pick_owns_frame = self.registration_photo_pick_owns_frame() || self.numeric_resize_dialog.is_some();
            if !self.right_panel_annotations && !self.selected_measurements.is_empty() {
                let multiple = self.selected_measurements.len() > 1;
                let actions_height = if multiple { 82.0 } else { 62.0 };
                egui::TopBottomPanel::bottom("results-actions")
                    .exact_height(actions_height)
                    .show_separator_line(false)
                    .frame(egui::Frame::new().fill(UiTokens::SURFACE_0))
                    .show_inside(ui, |ui| {
                        if photo_pick_owns_frame { ui.disable(); }
                        ui_widgets::card().show(ui, |ui| {
                            if multiple {
                                ui.label(egui::RichText::new(format!(
                                    "{} mesures sélectionnées · déplacement groupé disponible",
                                    self.selected_measurements.len(),
                                )).small().strong().color(Color32::from_rgb(35, 110, 165)));
                            }
                            let delete_label = if multiple {
                                format!("Supprimer la sélection ({})", self.selected_measurements.len())
                            } else {
                                "Supprimer la mesure sélectionnée".to_owned()
                            };
                            let delete_button_width = ui.available_width();
                            if ui_widgets::compact_button(ui, &delete_label, None, Tone::Danger,
                                egui::vec2(delete_button_width, 32.0)).clicked() {
                                self.delete_selected_measurement();
                            }
                        });
                    });
            }
            egui::ScrollArea::vertical().id_salt("results-panel-scroll")
                .auto_shrink([false, false]).show(ui, |ui| {
            ui.add_enabled_ui(!photo_pick_owns_frame, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(229, 237, 246))
                .corner_radius(9)
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let tab_width = (ui.available_width() - ui.spacing().item_spacing.x) * 0.5;
                        let count = self.measurements.iter().filter(|m| !m.is_construction).count();
                        if ui_widgets::tab(ui,"Mesures",count,!self.right_panel_annotations,tab_width).clicked() {
                            self.right_panel_annotations=false;
                        }
                        if ui_widgets::tab(ui,"Annotations",self.annotations.len(),self.right_panel_annotations,tab_width).clicked() {
                            self.right_panel_annotations=true;
                        }
                    });
                });
            let construction_count = self.measurements.iter().filter(|m| m.is_construction).count();
            if !self.right_panel_annotations && construction_count > 0 {
                ui.colored_label(Color32::from_rgb(155, 75, 205), format!("{construction_count} construction(s) · repères violets pointillés"));
            }
            ui.label(egui::RichText::new(if self.right_panel_annotations {
                "Cliquez un commentaire ou encadrez ses repères avec l’outil Sélection."
            } else {
                "Sélectionnez une mesure ; le cadre inclut aussi ses commentaires et annotations."
            }).small().color(Color32::from_rgb(105, 124, 146)));
            if let Some(image) = &self.image {
                let image_name = Path::new(&image.source).file_name().and_then(|name| name.to_str())
                    .unwrap_or(&image.source);
                egui::Frame::new().fill(Color32::from_rgb(242, 247, 251))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(218, 228, 239)))
                    .corner_radius(8).inner_margin(9.0).show(ui, |ui| {
                        ui.label(egui::RichText::new("IMAGE ACTIVE").size(10.5).strong()
                            .color(Color32::from_rgb(97, 118, 142)));
                        ui.label(egui::RichText::new(image_name).strong()
                            .color(Color32::from_rgb(42, 69, 101)));
                        ui.label(egui::RichText::new(format!("{} × {} px", image.width, image.height)).small()
                            .color(Color32::from_rgb(105, 124, 146)));
                    });
            }
            let measurement_inspection = self.current_inspection();
            let quality_allows_measurement_edits = self.quality_assessment.as_ref()
                .is_some_and(|assessment| assessment.is_exploitable);
            let measurement_edit_tooltip = if quality_allows_measurement_edits {
                "Le nom de mesure est modifiable.".to_owned()
            } else {
                self.quality_gate_message()
            };
            ui_widgets::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if !self.right_panel_annotations { ui_widgets::table_header(ui); }
            egui::ScrollArea::vertical()
                .id_salt("measurement-results-list")
                .max_height(if self.right_panel_annotations { 300.0 } else { 330.0 })
                .show(ui,|ui| {
                if !self.right_panel_annotations {
                if self.measurements.is_empty() {
                    egui::Frame::new().fill(Color32::from_rgb(247, 249, 252))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(222, 230, 239)))
                        .corner_radius(8).inner_margin(12.0).show(ui, |ui| {
                            ui.label(egui::RichText::new("Aucune mesure pour le moment")
                                .strong().color(Color32::from_rgb(83, 103, 128)));
                            ui.label(egui::RichText::new("Choisissez un outil puis tracez sur l’image.")
                                .small().color(Color32::from_rgb(113, 132, 153)));
                        });
                }
                for m in &mut self.measurements {
                    let before_label_edit = m.clone();
                    let selected = self.selected_measurements.contains(&m.id);
                    let displayed_result = inspection_evidence_ui::measurement_display_value(m, &measurement_inspection);
                    let stale_dependency = m.dependency_stale || measurement_inspection.dependencies.is_stale(m.id);
                    let suspended = stale_dependency || displayed_result != geometry::result(m);
                    let full_result = if stale_dependency {
                        "Valeur suspendue · dépendances périmées, recalcul requis".to_owned()
                    } else { displayed_result };
                    let (value, unit) = if suspended { ("Suspendue".to_owned(), String::new()) }
                        else { measurement_columns(m) };
                    let kind = if m.is_construction { "Construction" } else if m.drawing_variant.is_some() { m.type_label() } else { match m.kind {
                        Tool::Circle | Tool::CircleFit => "Diamètre",
                        Tool::PointLineDistance => "Point-ligne",
                        Tool::ParallelDistance => "Parallèles",
                        Tool::ObjectCount => "Comptage",
                        _ => m.kind.short_label(),
                    } };
                    let icon_color = if m.is_construction { Color32::from_rgb(163, 79, 218) }
                        else if m.kind == Tool::Angle { Color32::from_rgb(139, 61, 233) }
                        else if m.kind == Tool::Distance { Color32::from_rgb(0, 159, 186) }
                        else { Color32::from_rgb(34, 133, 226) };
                    let (row, menu) = ui_widgets::table_row(ui, &m.id.to_string(), kind,
                        &value, &unit, measurement_icon(m.kind), icon_color, selected);
                    if row.on_hover_text(format!("M{} · {}\n{}", m.id, m.label, full_result)).clicked() {
                        let (additive,toggle)=ui.input(|input|(input.modifiers.shift,input.modifiers.command || input.modifiers.ctrl));
                        requested_selection = Some((m.id,additive,toggle));
                    }
                    egui::Popup::menu(&menu).show(|ui| {
                        if ui.button("Afficher les détails").clicked() {
                            requested_selection = Some((m.id,false,false)); ui.close();
                        }
                        if ui.button("Copier le résultat").clicked() {
                            ui.ctx().copy_text(full_result.clone()); ui.close();
                        }
                    });
                    if selected && self.selected_measurement==Some(m.id) {
                        egui::Frame::new().fill(Color32::from_rgb(239, 247, 254))
                            .inner_margin(8).show(ui, |ui| {
                            ui.label(egui::RichText::new(&full_result).strong()
                                .color(Color32::from_rgb(25, 78, 133)));
                            if ui.add_enabled(quality_allows_measurement_edits,
                                egui::TextEdit::singleline(&mut m.label)
                                    .hint_text("Nom de la mesure").desired_width(f32::INFINITY)
                                    .margin(egui::vec2(7.0, 4.0)))
                                .on_hover_text(measurement_edit_tooltip.clone()).changed() {
                                label_edits.push((before_label_edit, m.clone()));
                            }
                            inspection_evidence_ui::measurement_evidence_card(ui, m, &measurement_inspection);
                            ui.collapsing("Méthode et traçabilité", |ui| {
                                ui.label(egui::RichText::new(format!("Capture {} · profil {} · origine {}",
                                    m.capture_id, m.calibration_id.as_deref().unwrap_or("aucun"), m.origin)).small());
                                if let Some(method) = m.method.as_deref().filter(|method| !method.trim().is_empty()) {
                                    ui.label(egui::RichText::new(method).small());
                                }
                            });
                        });
                    }
                }
                }
                if self.right_panel_annotations {
                graphic_section(ui, "annotations-card", "Commentaires & annotations", UiIcon::Comment, true, |ui| {
                    ui.label(egui::RichText::new(
                        "Le texte reste dans le panneau; il ne recouvre pas les tracés.",
                    ).small().color(Color32::from_rgb(105, 124, 146)));
                    annotation_text_changed |= ui.add(egui::TextEdit::multiline(&mut self.annotation_text)
                        .hint_text("Commentaire sur la mesure ou la zone…")
                        .desired_rows(3).desired_width(f32::INFINITY)).changed();
                    egui::ComboBox::from_id_salt("annotation-target-mode")
                        .selected_text(match self.annotation_target_mode {
                            AnnotationTargetMode::SelectedMeasurement => "Mesure sélectionnée",
                            AnnotationTargetMode::Point => "Point sur l'image",
                            AnnotationTargetMode::Zone => "Zone rectangulaire",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.annotation_target_mode,
                                AnnotationTargetMode::SelectedMeasurement, "Mesure sélectionnée");
                            ui.selectable_value(&mut self.annotation_target_mode,
                                AnnotationTargetMode::Point, "Point sur l'image");
                            ui.selectable_value(&mut self.annotation_target_mode,
                                AnnotationTargetMode::Zone, "Zone rectangulaire");
                        });
                    if self.annotation_mode {
                        ui.label(egui::RichText::new(if self.annotation_target_mode == AnnotationTargetMode::Zone
                            && self.annotation_points.len() == 1 {
                                "Cliquez le coin opposé de la zone. Échap annule."
                            } else if self.annotation_target_mode == AnnotationTargetMode::Zone {
                                "Cliquez deux coins de la zone. Échap annule."
                            } else {
                                "Cliquez le point de l'image. Échap annule."
                            }).small().color(Color32::from_rgb(31, 110, 132)));
                        if secondary_button(ui, "Annuler le placement").clicked() {
                            self.annotation_mode = false;
                            self.annotation_points.clear();
                        }
                    } else {
                        let can_add = !self.annotation_text.trim().is_empty()
                            && self.image.is_some()
                            && (!self.running || self.paused)
                            && (self.annotation_target_mode != AnnotationTargetMode::SelectedMeasurement
                                || self.selected_measurement.is_some());
                        if raised_button_enabled(ui, can_add, "Annotation / commentaire",
                            egui::vec2(0.0, 36.0),
                            if can_add { Color32::from_rgb(26, 143, 153) } else { Color32::from_rgb(244, 247, 250) },
                            if can_add { Color32::from_rgb(20, 112, 123) } else { Color32::from_rgb(220, 227, 235) },
                            if can_add { Color32::WHITE } else { Color32::from_rgb(142, 154, 169) }, 7).clicked() {
                            match self.annotation_target_mode {
                                AnnotationTargetMode::SelectedMeasurement => {
                                    if let Some(measurement_id) = self.selected_measurement {
                                        self.add_annotation(AnnotationTarget::Measurement { measurement_id });
                                    }
                                }
                                AnnotationTargetMode::Point | AnnotationTargetMode::Zone => {
                                    self.freeze_capture();
                                    self.clear_pending();
                                    self.selection_mode = false;
                                    self.calibration_mode = false;
                                    self.annotation_points.clear();
                                    self.annotation_mode = true;
                                    self.pan_mode = false;
                                    self.message = if self.annotation_target_mode == AnnotationTargetMode::Zone {
                                        "Cliquez deux coins pour définir la zone du commentaire.".into()
                                    } else {
                                        "Cliquez sur le point à commenter.".into()
                                    };
                                }
                            }
                        }
                    }
                    ui.add_space(5.0);
                    let target_labels = self.annotations.iter()
                        .map(|annotation| self.annotation_target_label(&annotation.target))
                        .collect::<Vec<_>>();
                    for (annotation_index, (annotation, target_label)) in self.annotations.iter_mut()
                        .zip(target_labels).enumerate()
                    {
                        let selected=self.selected_annotations.contains(&annotation.id);
                        let annotation_color = if selected {Color32::from_rgb(206,139,27)}
                            else {annotation_color(annotation_index)};
                        egui::Frame::new().fill(if selected {Color32::from_rgb(255,246,220)}
                                else {Color32::from_rgb(246,249,252)})
                            .stroke(Stroke::new(if selected {2.0} else {1.2}, annotation_color))
                            .corner_radius(7).inner_margin(7.0).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (marker, _) = ui.allocate_exact_size(
                                        egui::vec2(10.0, 10.0), egui::Sense::hover());
                                    ui.painter().circle_filled(marker.center(), 3.5, annotation_color);
                                    if ui.selectable_label(selected,format!("A{} · {}",annotation_index+1,target_label))
                                        .clicked() {
                                        let (additive,toggle)=ui.input(|input|(input.modifiers.shift,
                                            input.modifiers.command || input.modifiers.ctrl));
                                        requested_annotation_selection=Some((annotation.id.clone(),additive,toggle));
                                    }
                                    if ui.small_button("×").clicked() {
                                        removed_annotations.push(annotation.id.clone());
                                    }
                                });
                                annotation_text_changed |= ui.add(
                                    egui::TextEdit::multiline(&mut annotation.text)
                                        .desired_rows(2).desired_width(f32::INFINITY),
                                ).changed();
                            });
                    }
                });
                }
            });
            });
            });
            if !self.right_panel_annotations {
                ui.add_space(6.0);
                ui_widgets::card().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    if self.workflow_open_reference_panel {
                        let id = ui.make_persistent_id("reference-document-card");
                        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(), id, false);
                        state.set_open(true);
                        state.store(ui.ctx());
                        self.workflow_open_reference_panel = false;
                    }
                    graphic_section(ui, "reference-document-card", "Plan de référence",
                        UiIcon::Layers, false, |ui| self.reference_document_panel(ui));
                });
            }
            ui.add_enabled_ui(!photo_pick_owns_frame, |ui| {
            ui.add_space(6.0);
            ui_widgets::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            graphic_section(ui, "control-protocol-card", "Protocole de contrôle", UiIcon::Document, true, |ui| {
                let current_label = self.active_protocol.as_ref()
                    .map(|protocol| format!("{} · v{}", protocol.name, protocol.version))
                    .unwrap_or_else(|| "Aucun protocole commun".into());
                egui::ComboBox::from_id_salt("measurement-protocol")
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected_protocol_id, String::new(), "Aucun protocole");
                        for protocol in &latest_protocols {
                            ui.selectable_value(&mut selected_protocol_id, protocol.id.clone(),
                                format!("{} · v{}", protocol.name, protocol.version));
                        }
                    });
                ui.add(egui::TextEdit::singleline(&mut self.protocol_name_draft)
                    .hint_text("Nom du protocole").desired_width(f32::INFINITY));
                ui.add(egui::TextEdit::singleline(&mut self.protocol_family_draft)
                    .hint_text("Famille de pièce").desired_width(f32::INFINITY));
                ui.label(egui::RichText::new(self.active_protocol.as_ref().map(|protocol|
                    format!("Version active {} · caméra {} · résolution {}×{} · {} caractéristique(s) · précision {} décimales",
                        protocol.version, protocol.device, protocol.resolution[0],
                        protocol.resolution[1], protocol.expected_measurements.len(),
                        protocol.display_precision_mm))
                    .unwrap_or_else(|| "Enregistrez un protocole pour partager les réglages entre séries.".into()))
                    .small().color(Color32::from_rgb(105, 124, 146)));
                let can_save_protocol = self.image.is_some();
                if raised_button_enabled(ui, can_save_protocol, "Enregistrer une nouvelle version",
                    egui::vec2(0.0, 36.0),
                    if can_save_protocol { Color32::WHITE } else { Color32::from_rgb(244, 247, 250) },
                    if can_save_protocol { Color32::from_rgb(166, 188, 215) } else { Color32::from_rgb(220, 227, 235) },
                    if can_save_protocol { Color32::from_rgb(39, 87, 145) } else { Color32::from_rgb(142, 154, 169) },
                    7).clicked()
                {
                    self.save_protocol_version();
                }
            });
            });
            ui.add_space(6.0);
            ui_widgets::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            graphic_section(ui, "image-quality-card", "Qualité de l’image", UiIcon::Chart, true, |ui| {
                match self.quality_assessment.as_ref() {
                    Some(assessment) if assessment.is_exploitable => {
                        ui.colored_label(Color32::from_rgb(35, 122, 76),
                            "Image exploitable selon les seuils provisoires");
                    }
                    Some(assessment) => {
                        ui.colored_label(Color32::from_rgb(178, 50, 45), "Image non exploitable");
                        for issue in &assessment.issues {
                            ui.label(egui::RichText::new(format!("• {}", issue.label())).small()
                                .color(Color32::from_rgb(137, 66, 42)));
                        }
                    }
                    None => {
                        ui.colored_label(Color32::from_rgb(155, 111, 16),
                            "Qualité à évaluer après import ou gel de l'image");
                    }
                }
                if let Some(assessment) = self.quality_assessment.as_ref() {
                    let m = &assessment.metrics;
                    let t = &self.quality_thresholds;
                    let attainment = |value:f64, threshold:f64| if threshold > 0.0 {
                        (value / threshold).clamp(0.0,1.0) as f32
                    } else { 1.0 };
                    ui_widgets::gauge(ui, "Netteté", &format!("{:.1}",m.laplacian_variance),
                        attainment(m.laplacian_variance,t.min_laplacian_variance),
                        m.laplacian_variance >= t.min_laplacian_variance)
                        .on_hover_text("Valeur brute de netteté. La barre atteint sa fin au seuil provisoire configuré.");
                    ui_widgets::gauge(ui, "Contraste", &format!("{:.1}",m.contrast_std_dev),
                        attainment(m.contrast_std_dev,t.min_contrast_std_dev),
                        m.contrast_std_dev >= t.min_contrast_std_dev)
                        .on_hover_text("Écart-type mesuré. La barre atteint sa fin au seuil provisoire configuré.");
                    ui_widgets::gauge(ui, "Luminosité", &format!("{:.0} %",m.mean_luminance / 255.0 * 100.0),
                        (m.mean_luminance / 255.0).clamp(0.0,1.0) as f32,
                        m.mean_luminance >= t.min_mean_luminance && m.mean_luminance <= t.max_mean_luminance)
                        .on_hover_text(format!("Luminance moyenne : {:.1} / 255. Le pourcentage exprime cette échelle.",m.mean_luminance));
                    ui.label(egui::RichText::new(format!("Contours visibles : {:.1} %",m.edge_visibility_fraction*100.0)).small());
                } else {
                    for label in ["Netteté", "Contraste", "Luminosité"] {
                        ui_widgets::gauge(ui,label,"—",0.0,false).on_hover_text("Évaluation disponible après chargement d’une image.");
                    }
                }
                ui.label(egui::RichText::new(
                    "Les seuils sont provisoires et doivent être qualifiés pour chaque configuration optique.",
                ).small().color(Color32::from_rgb(137, 102, 30)));
                ui.label(egui::RichText::new(
                    "Correction d’affichage réversible ; les pixels d’origine sont conservés.",
                ).small().color(Color32::from_rgb(105, 124, 146)));
                let can_reassess_quality = self.image.is_some() && (!self.running || self.paused);
                if raised_button_enabled(ui, can_reassess_quality,
                    "Réévaluer la qualité de l'image", egui::vec2(0.0, 36.0),
                    if can_reassess_quality { Color32::WHITE } else { Color32::from_rgb(244, 247, 250) },
                    if can_reassess_quality { Color32::from_rgb(166, 188, 215) } else { Color32::from_rgb(220, 227, 235) },
                    if can_reassess_quality { Color32::from_rgb(39, 87, 145) } else { Color32::from_rgb(142, 154, 169) },
                    7).clicked()
                {
                    self.assess_current_quality();
                }
                if secondary_button(ui, "Rétablir les valeurs par défaut")
                    .on_hover_text("Réinitialise les corrections d'affichage et les seuils de dépistage.")
                    .clicked()
                {
                    let default_thresholds = ImageQualityThresholds::default();
                    if self.quality_thresholds != default_thresholds {
                        self.quality_thresholds = default_thresholds;
                        quality_thresholds_changed = true;
                    }
                    let default_correction = DisplayCorrection::default();
                    if self.display_correction != default_correction {
                        self.display_correction = default_correction;
                        display_correction_changed = true;
                    }
                }
                ui.collapsing("Corrections d'affichage (source préservée)", |ui| {
                    display_correction_changed |= ui.add(egui::Slider::new(
                        &mut self.display_correction.brightness, -100.0..=100.0,
                    ).text("Luminosité")).changed();
                    display_correction_changed |= ui.add(egui::Slider::new(
                        &mut self.display_correction.contrast, 0.0..=4.0,
                    ).text("Contraste")).changed();
                    display_correction_changed |= ui.add(egui::Slider::new(
                        &mut self.display_correction.gamma, 0.2..=5.0,
                    ).text("Gamma")).changed();
                });
                ui.collapsing("Seuils de dépistage (provisoires)", |ui| {
                    quality_thresholds_changed |= ui.add(egui::Slider::new(
                        &mut self.quality_thresholds.min_mean_luminance, 0.0..=255.0,
                    ).text("Luminance min.")).changed();
                    quality_thresholds_changed |= ui.add(egui::Slider::new(
                        &mut self.quality_thresholds.max_mean_luminance, 0.0..=255.0,
                    ).text("Luminance max.")).changed();
                    quality_thresholds_changed |= ui.add(egui::Slider::new(
                        &mut self.quality_thresholds.min_contrast_std_dev, 0.0..=120.0,
                    ).text("Contraste min.")).changed();
                    quality_thresholds_changed |= ui.add(egui::Slider::new(
                        &mut self.quality_thresholds.min_laplacian_variance, 0.0..=500.0,
                    ).text("Netteté min.")).changed();
                    quality_thresholds_changed |= ui.add(egui::Slider::new(
                        &mut self.quality_thresholds.min_edge_visibility_fraction, 0.0..=0.5,
                    ).text("Contours min.")).changed();
                    quality_thresholds_changed |= ui.add(egui::Slider::new(
                        &mut self.quality_thresholds.edge_gradient_threshold, 0.0..=1020.0,
                    ).text("Gradient min. pour l’accrochage")).changed();
                    quality_thresholds_changed |= ui.add(egui::Slider::new(
                        &mut self.quality_thresholds.max_clipped_pixel_fraction, 0.0..=1.0,
                    ).text("Saturation max.")).changed();
                });
            });
            });
            });
            });
            if let Some((id,additive,toggle)) = requested_selection {
                if additive || toggle {
                    let mut ids=self.measurement_selection_ids();
                    if toggle && ids.contains(&id) {ids.retain(|selected|*selected!=id);}
                    else if !ids.contains(&id) {ids.push(id);}
                    let annotations=self.selected_annotations.iter().cloned().collect();
                    self.set_combined_selection(ids,annotations);
                } else {self.set_measurement_selection(Some(id));}
            }
            if let Some((id,additive,toggle))=requested_annotation_selection {
                let measurements=if additive || toggle {self.measurement_selection_ids()} else {Vec::new()};
                let mut annotations=if additive || toggle {
                    self.selected_annotations.iter().cloned().collect::<Vec<_>>()
                } else {Vec::new()};
                if toggle && annotations.contains(&id) {annotations.retain(|selected|selected!=&id);}
                else if !annotations.contains(&id) {annotations.push(id);}
                self.set_combined_selection(measurements,annotations);
                self.message=format!("Sélection : {} mesure(s), {} annotation(s).",
                    self.selected_measurements.len(),self.selected_annotations.len());
            }
            if !removed_annotations.is_empty() {
                self.annotations.retain(|annotation| !removed_annotations.contains(&annotation.id));
                self.selected_annotations.retain(|id|!removed_annotations.contains(id));
                annotation_text_changed = true;
            }
            if annotation_text_changed {
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
            }
            for (before, after) in label_edits {
                self.record_measurement_action(MeasurementHistoryAction::Edited {
                    id: after.id, before, after,
                });
            }
        });
        let current_protocol_id = self.active_protocol.as_ref()
            .map(|protocol| protocol.id.as_str()).unwrap_or("");
        if selected_protocol_id != current_protocol_id {
            if selected_protocol_id.is_empty() {
                self.invalidate_inspection_inputs();
                self.active_protocol = None;
                self.session_saved = false;
                self.document_revision = self.document_revision.wrapping_add(1);
            } else {
                self.activate_protocol(&selected_protocol_id, ctx);
            }
        }
        if display_correction_changed || quality_thresholds_changed {
            if display_correction_changed {
                self.record_display_correction_change(display_correction_before, ctx.input(|input| input.pointer.primary_down()));
                self.refresh_display_texture(ctx);
            }
            self.assess_current_quality();
            self.session_saved = false;
            self.document_revision = self.document_revision.wrapping_add(1);
        }
        self.refresh_changed_display(ctx);
    }
}

//! Original header, ribbon, workflow and status from the pre-sync backup.
use super::*;
impl DperMeasureApp {
    pub(super) fn recovered_chrome(&mut self, ctx: &egui::Context) {
        self.refresh_changed_display(ctx);
        let workflow_has_image = self.image.is_some();
        let workflow_has_reference = self.reference_document.is_some();
        let workflow_has_calibration = self.mm_per_pixel.is_some();
        let workflow_has_measurements = self.measurements.iter().any(|m| !m.is_construction);
        let workflow_reference_ready = self.reference_document.as_ref().is_some_and(|document| {
            !document.dimensions.is_empty()
                && document.dimensions.iter().all(|dimension| {
                    dimension.confirmed && dimension.measurement_id.is_some()
                })
        });
        let workflow_current_step = if !workflow_has_image {
            0
        } else if !workflow_has_reference && !workflow_has_calibration && !workflow_has_measurements {
            1
        } else if workflow_has_reference && !workflow_has_calibration {
            2
        } else if !workflow_has_measurements {
            3
        } else if workflow_has_reference && !workflow_reference_ready {
            4
        } else {
            5
        };
        let workflow_guidance = if !workflow_has_image {
            "Commencez par une capture, l’ouverture d’une image ou le chargement d’un projet."
        } else if !workflow_has_reference && !workflow_has_calibration && !workflow_has_measurements {
            "Le plan est facultatif : importez-le depuis Inspection pour contrôler les tolérances, ou passez à l’étalonnage ou aux mesures."
        } else if workflow_has_reference && !workflow_has_calibration {
            "Pour comparer les mesures aux tolérances du plan, étalonnez l’image; vous pouvez aussi continuer en pixels."
        } else if !workflow_has_measurements {
            "Choisissez un outil et relevez les cotes sur l’image."
        } else if workflow_has_reference && !workflow_reference_ready {
            "Associez et confirmez les cotes du plan dans Plan de référence, ou exportez sans contrôle de conformité."
        } else if !workflow_has_calibration {
            "Export possible; sans étalonnage, les résultats restent en pixels et ne donnent pas de verdict de conformité."
        } else if !workflow_has_reference {
            "Export possible; sans plan de référence, aucune comparaison aux tolérances n’est réalisée."
        } else {
            "Vous pouvez exporter un rapport ou les données de mesure."
        };
        if self.tab_icon_sheet.is_none() {
            let decoded = image::load_from_memory(include_bytes!(
                "../assets/ui/ribbon_tabs/atlas_onglets.png"
            )).expect("The embedded DPER_MEASURE ribbon icon atlas must be a valid PNG");
            let rgba = decoded.to_rgba8();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            );
            self.tab_icon_sheet = Some(ctx.load_texture(
                "dper-measure-ribbon-icons",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }
        let tab_icon_sheet = self.tab_icon_sheet.clone()
            .expect("The DPER_MEASURE ribbon icon atlas was loaded above");
        let display_correction_before = self.display_correction;
        let mut display_correction_changed = false;

        egui::TopBottomPanel::top("toolbar")
            .min_height(UiTokens::HEADER_HEIGHT)
            .frame(egui::Frame::new().fill(UiTokens::NAVY_900)
                .inner_margin(egui::Margin::symmetric(14, 9)))
            .show(ctx, |ui| {
                egui::ScrollArea::horizontal().id_salt("work-ribbon-tabs")
                    .auto_shrink([false, true]).show(ui, |ui| {
                if self.registration_photo_pick_owns_frame() || self.numeric_resize_dialog.is_some() { ui.disable(); }
                ui.horizontal(|ui| {
                    ui.horizontal(|ui| {
                        brand_mark(ui);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("DPER_MEASURE").size(20.0).strong().color(Color32::WHITE));
                            ui.label(egui::RichText::new("Microscope & métrologie").size(11.5)
                                .color(Color32::from_rgb(181, 207, 230)));
                        });
                    });
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(4.0);
                    for (tab, label, icon) in RibbonTab::ALL {
                        let response = ui_widgets::navigation_tab_button(
                            ui,
                            label,
                            &tab_icon_sheet,
                            icon.uv(),
                            self.active_ribbon_tab == tab,
                        );
                        if response.clicked() {
                            self.active_ribbon_tab = tab;
                            match tab {
                                RibbonTab::Measurements | RibbonTab::Inspection => {
                                    self.right_panel_annotations = false;
                                }
                                _ => {}
                            }
                        }
                    }
                });
                });
            });
        display_correction_changed |= self.command_ribbon_ui(ctx);
        let steps = [
            ("Capture / image / projet", workflow_has_image, false),
            ("Plan PDF / image", workflow_has_reference, true),
            ("Étalonnage", workflow_has_calibration, true),
            ("Mesures", workflow_has_measurements, false),
            ("Contrôle", workflow_reference_ready, !workflow_has_reference),
            ("Export", false, false),
        ];
        let current_step = workflow_current_step.min(steps.len().saturating_sub(1));
        egui::TopBottomPanel::top("workflow-guide")
            .exact_height(86.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(225, 234, 243))
                .stroke(Stroke::new(1.0, Color32::from_rgb(190, 207, 223)))
                .inner_margin(egui::Margin::symmetric(14, 3)))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                if self.registration_photo_pick_owns_frame() || self.has_unfinished_image_action()
                    || self.video_recording.is_some() || self.timelapse_capture.is_some()
                    || self.export_dialog.is_some() || self.confirm_close_image {
                    ui.disable();
                }
                let painter = ui.painter_at(rect);
                let guide = Rect::from_center_size(rect.center(), Vec2::new(rect.width().min(2000.0), rect.height()));
                let step_count = steps.len();
                let side_inset = 105.0_f32.min(guide.width() * 0.12);
                let first_x = guide.left() + side_inset;
                let last_x = guide.right() - side_inset;
                let node_y = guide.top() + 25.0;
                let centers = (0..step_count).map(|index| {
                    let t = index as f32 / (step_count.saturating_sub(1).max(1) as f32);
                    Pos2::new(first_x + (last_x - first_x) * t, node_y)
                }).collect::<Vec<_>>();
                if let (Some(first), Some(last)) = (centers.first(), centers.last()) {
                    painter.line_segment([*first, *last],
                        Stroke::new(3.5, Color32::from_rgb(177, 198, 219)));
                    if current_step > 0 {
                        painter.line_segment([*first, centers[current_step]],
                            Stroke::new(3.5, Color32::from_rgb(0, 145, 196)));
                    }
                }
                for (index, (label, complete, optional)) in steps.iter().enumerate() {
                    let step_left = if index == 0 { guide.left() }
                        else { (centers[index - 1].x + centers[index].x) * 0.5 };
                    let step_right = if index + 1 == step_count { guide.right() }
                        else { (centers[index].x + centers[index + 1].x) * 0.5 };
                    let step_rect = Rect::from_min_max(
                        Pos2::new(step_left, guide.top()),
                        Pos2::new(step_right, guide.bottom()),
                    );
                    let step_response = ui.interact(step_rect,
                        ui.id().with(("workflow-step", index)), Sense::click())
                        .on_hover_text(format!("{}{}\n{}{}", label, if *optional { " (facultatif)" } else { "" }, match index {
                            0 => "Choisir une caméra, ouvrir une image ou charger un projet",
                            1 => "Importer un plan PDF ou une image de référence",
                            2 => "Lancer l’étalonnage de l’image",
                            3 => "Activer un outil de mesure",
                            4 => "Ouvrir les cotes du plan et leur contrôle",
                            _ => "Ouvrir les options d’export et le rapport",
                        }, if index == current_step { format!("\n{workflow_guidance}") } else { String::new() }));
                    if step_response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let current = index == current_step;
                    if step_response.hovered() {
                        painter.circle_filled(centers[index], 14.0,
                            Color32::from_rgba_unmultiplied(36, 115, 177, 28));
                    }
                    let (fill, border, marker, marker_color, label_color) = if current {
                        (Color32::from_rgb(0, 137, 190), Color32::from_rgb(0, 91, 142),
                            (index + 1).to_string(), Color32::WHITE, Color32::from_rgb(0, 76, 119))
                    } else if *complete {
                        (Color32::from_rgb(40, 161, 97), Color32::from_rgb(27, 132, 77),
                            String::new(), Color32::WHITE, Color32::from_rgb(42, 104, 70))
                    } else if *optional {
                        (Color32::from_rgb(230, 237, 244), Color32::from_rgb(174, 192, 209),
                            (index + 1).to_string(), Color32::from_rgb(83, 105, 128), Color32::from_rgb(77, 99, 122))
                    } else {
                        (Color32::WHITE, Color32::from_rgb(145, 174, 201),
                            (index + 1).to_string(), Color32::from_rgb(39, 91, 137), Color32::from_rgb(48, 73, 98))
                    };
                    let center = centers[index];
                    painter.circle_filled(center, 9.5, fill);
                    painter.circle_stroke(center, 9.5, Stroke::new(if current { 2.0 } else { 1.3 }, border));
                    painter.text(center, egui::Align2::CENTER_CENTER, marker,
                        egui::FontId::proportional(11.5), marker_color);
                    if *complete && !current {
                        painter.add(egui::Shape::line(vec![center + Vec2::new(-4.0, 0.0), center + Vec2::new(-1.0, 3.0),
                            center + Vec2::new(5.0, -4.0)], Stroke::new(1.8, Color32::WHITE)));
                    }
                    let label_font = ui_widgets::font(11.0, current);
                    let label_galley = painter.layout_no_wrap((*label).to_owned(), label_font.clone(), label_color);
                    let label_center = Pos2::new(center.x, node_y + 24.0);
                    let label_bounds = Rect::from_center_size(label_center,
                        Vec2::new(label_galley.size().x + 18.0, 20.0));
                    if current {
                        painter.rect_filled(label_bounds, 5.0, Color32::from_rgb(211, 232, 247));
                        painter.rect_stroke(label_bounds, 5.0, Stroke::new(1.0, Color32::from_rgb(0, 145, 196)), egui::StrokeKind::Inside);
                    }
                    if step_response.hovered() {
                        painter.rect_filled(label_bounds, 5.0,
                            Color32::from_rgba_unmultiplied(36, 115, 177, 22));
                    }
                    painter.with_clip_rect(step_rect).galley(
                        Pos2::new(center.x - label_galley.size().x * 0.5, label_center.y - label_galley.size().y * 0.5),
                        label_galley, label_color);
                    if *optional {
                        painter.with_clip_rect(step_rect).text(Pos2::new(center.x, node_y + 37.0),
                            egui::Align2::CENTER_TOP, "facultatif", ui_widgets::font(9.0, false),
                            Color32::from_rgb(113, 135, 157));
                    }
                    if index == 0 {
                        egui::Popup::menu(&step_response).show(|ui| {
                            let section_width = ui.painter()
                                .layout_no_wrap("Capture / image / projet".to_owned(),
                                    ui_widgets::font(11.5, true), Color32::from_rgb(82, 107, 137))
                                .size().x + 16.0;
                            let command_width = [
                                ui_widgets::menu_item_width(ui, "Démarrer le flux USB", Some("F5"), false),
                                ui_widgets::menu_item_width(ui, "Ouvrir une image…", Some("Ctrl+O"), false),
                                ui_widgets::menu_item_width(ui, "Ouvrir un projet…", None, false),
                            ].into_iter().fold(0.0_f32, f32::max);
                            ui.set_width(section_width.max(command_width));
                            ui_widgets::menu_section(ui, "Capture / image / projet");
                            if ui_widgets::menu_item(ui, "Démarrer le flux USB", UiIcon::Play, Some("F5"),
                                false, true, false, MenuItemTone::Success).clicked() {
                                self.active_ribbon_tab = RibbonTab::Camera;
                                self.start_camera();
                                ui.close();
                            }
                            if ui_widgets::menu_item(ui, "Ouvrir une image…", UiIcon::Image, Some("Ctrl+O"),
                                false, true, false, MenuItemTone::Standard).clicked() {
                                self.active_ribbon_tab = RibbonTab::Files;
                                self.open_image(ctx);
                                ui.close();
                            }
                            if ui_widgets::menu_item(ui, "Ouvrir un projet…", UiIcon::Folder, None,
                                false, true, false, MenuItemTone::Standard).clicked() {
                                self.active_ribbon_tab = RibbonTab::Files;
                                self.open_project(ctx);
                                ui.close();
                            }
                        });
                    } else if step_response.clicked() {
                        match index {
                            1 => {
                                self.active_ribbon_tab = RibbonTab::Inspection;
                                self.right_panel_annotations = false;
                                self.workflow_open_reference_panel = true;
                                self.open_reference_document();
                            }
                            2 => {
                                self.active_ribbon_tab = RibbonTab::Calibration;
                                self.right_panel_annotations = false;
                                self.start_calibration_mode();
                            }
                            3 => {
                                self.active_ribbon_tab = RibbonTab::Measurements;
                                self.right_panel_annotations = false;
                                if self.image.is_some() {
                                    self.choose_measurement_tool(Tool::Distance);
                                    if !self.calibration_mode && self.pending.is_empty() {
                                        self.message = "Outil Distance actif · cliquez deux points sur l’image.".into();
                                    }
                                } else {
                                    self.message = "Ouvrez une image ou démarrez le flux avant de mesurer.".into();
                                }
                            }
                            4 => {
                                self.active_ribbon_tab = RibbonTab::Inspection;
                                self.right_panel_annotations = false;
                                self.workflow_open_reference_panel = true;
                                if self.reference_document.is_none() {
                                    self.open_reference_document();
                                } else {
                                    self.message = "Plan de référence ouvert · associez et confirmez les cotes.".into();
                                }
                            }
                            5 => {
                                self.active_ribbon_tab = RibbonTab::Exports;
                                self.open_folder_export(ctx);
                            }
                            _ => {}
                        }
                    }
                }
            });
        egui::TopBottomPanel::bottom("status")
            .min_height(UiTokens::STATUS_HEIGHT)
            .frame(egui::Frame::new().fill(Color32::from_rgb(239, 244, 250))
                .stroke(Stroke::new(1.0, Color32::from_rgb(208, 223, 239)))
                .inner_margin(egui::Margin::symmetric(12, 7)))
            .show(ctx, |ui| {
                let bar = ui.horizontal_wrapped(|ui| {
                    let (state, color) = if self.running && !self.paused {
                        ("Caméra connectée", Color32::from_rgb(53, 194, 67))
                    } else if self.running {
                        ("Flux en pause", Color32::from_rgb(42, 133, 210))
                    } else if self.image.is_some() {
                        ("Image chargée", Color32::from_rgb(42, 133, 210))
                    } else { ("Prêt", Color32::from_rgb(125, 145, 165)) };
                    ui.horizontal(|ui| {
                        status_indicator(ui,color);
                        ui.label(egui::RichText::new(state).strong().size(13.0)
                            .color(Color32::from_rgb(25,78,133)));
                        if self.running { ui.label(egui::RichText::new(&self.active_device).size(12.5)); }
                    });
                    ui.separator();
                    status_group(ui,UiIcon::ZoomIn,&if self.image.is_some() {
                        format!("Zoom : {:.0} %",self.zoom_percent)
                    } else { "Zoom : —".into() });
                    ui.separator();
                    status_group(ui,UiIcon::Crosshair,&self.mm_per_pixel.map(|value|
                        format!("Calibration : {:.5} mm/pixel",value))
                        .unwrap_or_else(|| "Calibration : absente".into()));
                    if let Some(image)=&self.image {
                        ui.separator();
                        status_group(ui,UiIcon::Image,&format!("{} × {}",image.width,image.height))
                            .on_hover_text(format!("Capture {}",self.capture_id));
                    }
                    if let Some(server)=&self.mcp_server {
                        ui.separator();
                        let (label,color)=match server.status() {
                            mcp_server::ServerStatus::Running =>
                                (format!("Serveur MCP : {}",mcp_server::configured_endpoint()),Color32::from_rgb(53,194,67)),
                            mcp_server::ServerStatus::Starting => ("Serveur MCP : démarrage".into(),Color32::from_rgb(238,153,49)),
                            mcp_server::ServerStatus::Failed(_) => ("Serveur MCP : indisponible".into(),Color32::from_rgb(193,64,64)),
                            mcp_server::ServerStatus::Stopped => ("Serveur MCP : arrêté".into(),Color32::from_rgb(125,145,165)),
                        };
                        ui.horizontal(|ui| { status_indicator(ui,color); status_group(ui,UiIcon::Database,&label); });
                    }
                    ui.separator();
                    ui.label(egui::RichText::new(format!("DPER_MEASURE  v{}",env!("CARGO_PKG_VERSION"))).size(12.0).color(UiTokens::TEXT_600));
                    ui.add(egui::Label::new(egui::RichText::new(&self.message).size(12.0)
                        .color(Color32::from_rgb(90,113,139))).truncate()).on_hover_text(&self.message);
                });
                bar.response.on_hover_text(&self.message);
            });

        if display_correction_changed { self.record_display_correction_change(display_correction_before, ctx.input(|input| input.pointer.primary_down())); }

    }
}

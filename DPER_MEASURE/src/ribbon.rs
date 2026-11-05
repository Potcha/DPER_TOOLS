//! DPER command descriptions shared by promoted buttons and native menus.
//! Written for DPER; no SolidCraft implementation or artwork is embedded.
use egui::{self, Color32, RichText, Ui, Vec2};
use crate::ui_widgets::{self, Icon, MenuItemTone, Tone};

pub struct Command<A> {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: Icon,
    pub action: A,
    pub enabled: bool,
    pub selected: bool,
    pub destructive: bool,
    pub shortcut: Option<&'static str>,
    pub category: Option<&'static str>,
}

impl<A> Command<A> {
    pub fn new(id: &'static str, label: &'static str, icon: Icon, action: A) -> Self {
        Self { id, label, icon, action, enabled: true, selected: false,
            destructive: false, shortcut: None, category: None }
    }
    pub fn enabled(mut self, enabled: bool) -> Self { self.enabled = enabled; self }
    pub fn selected(mut self, selected: bool) -> Self { self.selected = selected; self }
    pub fn shortcut(mut self, shortcut: &'static str) -> Self { self.shortcut = Some(shortcut); self }
    pub fn destructive(mut self) -> Self { self.destructive = true; self }
    pub fn category(mut self, category: &'static str) -> Self { self.category = Some(category); self }
}

pub struct Group<A> {
    pub id: &'static str,
    pub title: &'static str,
    pub promoted: usize,
    pub commands: Vec<Command<A>>,
}

impl<A> Group<A> {
    pub fn new(id: &'static str, title: &'static str, promoted: usize, commands: Vec<Command<A>>) -> Self {
        Self { id, title, promoted, commands }
    }
    fn width(&self, compact: bool) -> f32 {
        let count = self.promoted.min(self.commands.len());
        (count as f32 * if compact { 64.0 } else { 82.0 })
            .max(self.title.chars().count() as f32 * 6.5 + 24.0) + 12.0
    }
}

fn menu_row<A: Copy>(ui: &mut Ui, command: &Command<A>, chosen: &mut Option<A>) {
    ui.push_id(command.id, |ui| {
        if ui_widgets::menu_item(ui, command.label, command.icon, command.shortcut, false,
            command.enabled, command.selected,
            if command.destructive { MenuItemTone::Danger } else { MenuItemTone::Standard }).clicked()
        {
            *chosen = Some(command.action);
            ui.close();
        }
    });
}

fn commands_menu_width<'a, A: 'a>(ui: &Ui, commands: impl Iterator<Item = &'a Command<A>>) -> f32 {
    commands.fold(104.0_f32, |width, command| {
        width.max(ui_widgets::menu_item_width(ui, command.label, command.shortcut, false))
    })
}

fn group_menu_width<A>(ui: &Ui, group: &Group<A>) -> f32 {
    let mut width = 104.0_f32;
    for (index, command) in group.commands.iter().enumerate() {
        if let Some(category) = command.category {
            if group.commands[..index].iter().any(|other| other.category == Some(category)) { continue; }
            width = width.max(ui_widgets::menu_item_width(ui, category, None, true));
        } else {
            width = width.max(ui_widgets::menu_item_width(ui, command.label, command.shortcut, false));
        }
    }
    width
}

fn menu_contents<A: Copy>(ui: &mut Ui, group: &Group<A>, chosen: &mut Option<A>) {
    *ui.visuals_mut() = crate::dper_visuals();
    let width = group_menu_width(ui, group);
    ui.set_width(width);
    for (index, command) in group.commands.iter().enumerate() {
        if let Some(category) = command.category {
            if group.commands[..index].iter().any(|other| other.category == Some(category)) { continue; }
            ui_widgets::submenu_item(ui, category, command.icon, |ui| {
                let width = commands_menu_width(ui, group.commands.iter()
                    .filter(|child| child.category == Some(category)));
                ui.set_width(width);
                for child in group.commands.iter().filter(|child| child.category == Some(category)) {
                    menu_row(ui, child, chosen);
                }
            });
        } else { menu_row(ui, command, chosen); }
    }
}

/// Layout reserves a real footer row for the group menu: no caption is painted over a tile.
pub fn show<A: Copy>(ui: &mut Ui, groups: &[Group<A>]) -> Option<A> {
    show_with_trailing(ui, groups, 0.0, |_| {})
}

/// Draw the ribbon groups and place a caller-provided control directly after the last group.
pub fn show_with_trailing<A: Copy>(
    ui: &mut Ui,
    groups: &[Group<A>],
    trailing_width: f32,
    trailing: impl FnOnce(&mut Ui),
) -> Option<A> {
    show_with_inserted_control(ui, groups, groups.len(), trailing_width, trailing)
}

/// Draw the ribbon groups and place a caller-provided control after `after_group` groups.
pub fn show_with_inserted_control<A: Copy>(
    ui: &mut Ui,
    groups: &[Group<A>],
    after_group: usize,
    control_width: f32,
    control: impl FnOnce(&mut Ui),
) -> Option<A> {
    let after_group = after_group.min(groups.len());
    let available = ui.available_width();
    let compact = groups.iter().map(|group| group.width(false) + 8.0).sum::<f32>()
        + control_width > available;
    let mut chosen = None;
    let mut control = Some(control);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
        for (index, group) in groups.iter().enumerate() {
            if index == after_group {
                if let Some(render_control) = control.take() { render_control(ui); }
            }
            let remaining = ui.available_width();
            let reserve = (if index < after_group { control_width } else { 0.0 })
                + if index + 1 < groups.len() { 76.0 } else { 0.0 };
            let width = group.width(compact);
            if width > remaining - reserve {
                let overflow_response = ui.menu_button(RichText::new("Plus v").color(Color32::WHITE), |ui| {
                    *ui.visuals_mut() = crate::dper_visuals();
                    let width = groups[index..].iter().fold(104.0_f32, |width, hidden| {
                        width.max(ui_widgets::menu_item_width(ui, hidden.title, None, true))
                    });
                    ui.set_width(width);
                    for hidden in &groups[index..] {
                        let icon = hidden.commands.first().map_or(Icon::Layers, |command| command.icon);
                        ui_widgets::submenu_item(ui, hidden.title, icon,
                            |ui| menu_contents(ui, hidden, &mut chosen));
                    }
                });
                #[cfg(test)]
                ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("ribbon-test", "Plus v")), overflow_response.response.rect));
                break;
            }
            ui.push_id(group.id, |ui| {
                let response = ui.allocate_ui_with_layout(Vec2::new(width, 104.0),
                    egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(width);
                        ui.allocate_ui_with_layout(Vec2::new(width, 68.0),
                            egui::Layout::left_to_right(egui::Align::Min), |ui| {
                                for command in group.commands.iter().take(group.promoted) {
                                    ui.push_id(command.id, |ui| {
                                        if ui_widgets::ribbon_command_button(ui, command.label, command.icon,
                                            if command.destructive { Tone::Danger } else { Tone::Canvas },
                                            command.selected, command.enabled, compact).clicked()
                                        { chosen = Some(command.action); }
                                    });
                                }
                                // allocate_ui may shrink to its contents; explicitly reserve the row height.
                                ui.set_min_height(68.0);
                            });
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.spacing_mut().interact_size.y = 20.0;
                            let footer_response = ui.menu_button(RichText::new(format!("{} v", group.title))
                                .size(11.5).color(Color32::from_rgb(170, 190, 210)),
                                |ui| menu_contents(ui, group, &mut chosen));
                            #[cfg(test)]
                            ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("ribbon-test", format!("{} v", group.title))), footer_response.response.rect));
                        });
                    }).response;
                ui.painter().line_segment([
                    response.rect.right_top() + Vec2::new(1.0, 4.0),
                    response.rect.right_bottom() + Vec2::new(1.0, -4.0)],
                    egui::Stroke::new(1.0, Color32::from_rgb(53, 80, 108)));
            });
        }
        if let Some(render_control) = control.take() { render_control(ui); }
    });
    chosen
}

use crate::{app::events::Events, components::Components, event::Event};
use eframe::egui::{self, Modal};

#[derive(Default)]
pub struct Settings {
    open: bool,
}

impl Components for Settings {
    fn init(&mut self, _cc: &eframe::CreationContext<'_>) {}

    fn update(&mut self, events: &mut Events<Event>) {
        if let Some(opened) = events.pop(|e| match e {
            Event::ToggleSettingsWindow { opened } => Some(*opened),
            _ => None,
        }) {
            self.open = opened;
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame, events: &mut Events<Event>) {
        if self.open {
            let modal = Modal::new("Settings".into()).show(ui.ctx(), |ui| {
                ui.set_width(250.0);
                ui.heading("Parametres");

                ui.separator();

                egui::Sides::new().show(
                    ui,
                    |ui| ui.label("Theme : "),
                    egui::widgets::global_theme_preference_buttons,
                );

                ui.separator();

                egui::Sides::new().show(
                    ui,
                    |_ui| {},
                    |ui| {
                        if ui.button("close").clicked() {
                            ui.close();
                        }
                    },
                );
            });

            if modal.should_close() {
                events.push(Event::ToggleSettingsWindow { opened: false });
            }
        }
    }
}

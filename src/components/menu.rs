use crate::{app::events::Events, components::Components, event::Event};
use eframe::egui::{self};

#[derive(Default)]
pub struct Menu {}

impl Components for Menu {
    fn init(&mut self, _cc: &eframe::CreationContext<'_>) {}

    fn update(&mut self, events: &mut Events<Event>) {}

    fn show(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame, events: &mut Events<Event>) {
        egui::Panel::top("menu")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 24.0; // Espacement entre les items
                ui.horizontal(|ui| {
                    egui::Sides::new().show(
                        ui,
                        |ui| {
                            ui.label(
                                egui::RichText::new(egui_phosphor::regular::FILE_PDF).size(32.0),
                            );
                            ui.label(
                                egui::RichText::new("Analyser vos fiches de payes").size(25.0),
                            );
                        },
                        |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(egui_phosphor::regular::GEAR)
                                            .size(20.0),
                                    )
                                    .frame(false)
                                    .min_size(egui::vec2(0.0, 40.0)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                events.push(Event::ToggleSettingsWindow { opened: true });
                            }
                            // Bouton pour importer de nouvelles fiche de paie
                            if ui
                                .scope(|ui| {
                                    ui.spacing_mut().button_padding = egui::vec2(16.0, 10.0);
                                    ui.add(
                                        egui::Button::new(
                                            egui::RichText::new("Scanner une fiche").size(20.0),
                                        )
                                        .min_size(egui::vec2(0.0, 40.0)),
                                    )
                                })
                                .inner
                                .clicked()
                            {
                                events.push(Event::ImportPaystubs);
                            }
                        },
                    );
                });
            });
    }
}

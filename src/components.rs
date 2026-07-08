use eframe::{CreationContext, egui};

use crate::{app::events::Events, event::Event};

pub mod menu;
pub mod settings;

pub trait Components {
    fn init(&mut self, _cc: &CreationContext<'_>) {}
    fn update(&mut self, _events: &mut Events<Event>) {}
    fn show(
        &mut self,
        _ui: &mut egui::Ui,
        _frame: &mut eframe::Frame,
        _events: &mut Events<Event>,
    ) {
    }
}

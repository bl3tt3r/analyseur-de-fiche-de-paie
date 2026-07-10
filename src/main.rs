use crate::app::{App, analyse::claude_code_ready};
use eframe::egui;
use egui_commonmark::*;

pub mod app;
pub mod components;

pub struct AppWrapper {
    inner: App,
}

pub struct ErrorAppWrapper {
    error: &'static str,
    cache: CommonMarkCache,
}

fn main() -> eframe::Result {
    let mut app: App = App::load();
    eframe::run_native(
        app.name(),
        app.options(),
        Box::new(|cc| match claude_code_ready() {
            Ok(_) => {
                app.init(cc);
                Ok(Box::new(AppWrapper { inner: app }))
            }
            Err(error) => Ok(Box::new(ErrorAppWrapper {
                error,
                cache: CommonMarkCache::default(),
            })),
        }),
    )
}

impl eframe::App for ErrorAppWrapper {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::Area::new("error_message".into())
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    ui.set_max_width(500.0);
                    CommonMarkViewer::new().show(ui, &mut self.cache, self.error);
                });
        });
    }
}

impl eframe::App for AppWrapper {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.inner.tick(ui, frame);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.inner.save();
    }
}

use crate::app::App;
use eframe::egui;

pub mod app;
pub mod components;
pub mod event;

pub struct Wrapper {
    inner: App,
}

fn main() -> eframe::Result {
    let mut app: App = App::load();
    eframe::run_native(
        app.name(),
        app.options(),
        Box::new(|cc| {
            app.init(cc);
            Ok(Box::new(Wrapper { inner: app }))
        }),
    )
}

impl eframe::App for Wrapper {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.inner.tick(ui, frame);
    }
}

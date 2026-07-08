use eframe::{CreationContext, egui};
use tracing::level_filters::LevelFilter;

use crate::{
    app::events::Events,
    components::{Components, menu::Menu, settings::Settings},
    event::Event,
};

pub mod database;
pub mod events;
pub mod paystubs;

const DEFAULT_LOG_LEVEL: LevelFilter = tracing::level_filters::LevelFilter::INFO;

pub struct App {
    pub events: Events<Event>,
    pub components: Vec<Box<dyn Components>>,
}

impl App {
    pub fn load() -> App {
        // Demarrage du logger
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::builder()
                    .with_default_directive(DEFAULT_LOG_LEVEL.into())
                    .from_env_lossy(),
            )
            .init();
        // Création de l'App
        App {
            events: Events::default(),
            components: vec![Box::new(Menu::default()), Box::new(Settings::default())],
        }
    }

    pub fn name(&self) -> &'static str {
        "Fiche de paye"
    }

    pub fn options(&self) -> eframe::NativeOptions {
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 1000.0]),
            centered: true,
            ..Default::default()
        }
    }

    pub fn init(&mut self, cc: &CreationContext<'_>) {
        // Initialisation de la font phosphor
        let mut fonts: egui::FontDefinitions = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        for view in &mut self.components {
            view.init(cc);
        }
    }

    pub fn tick(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        for view in &mut self.components {
            view.update(&mut self.events);
        }
        for view in &mut self.components {
            view.show(ui, frame, &mut self.events);
        }
    }
}

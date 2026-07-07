use eframe::{CreationContext, egui};
use tracing::level_filters::LevelFilter;

use crate::{app::events::Events, event::Event};

pub mod database;
pub mod events;
pub mod paystubs;

const DEFAULT_LOG_LEVEL: LevelFilter = tracing::level_filters::LevelFilter::INFO;

pub struct App {
    pub events: Events<Event>,
}

impl App {
    pub fn load() -> App {
        // Initialisation du logger
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

    pub fn init(&self, _cc: &CreationContext<'_>) {}

    pub fn tick(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.label("test rudy");
    }
}

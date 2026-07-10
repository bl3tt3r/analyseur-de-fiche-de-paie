use crate::app::{
    analyse::Analyse,
    paystubs::{Paystub, PaystubState},
};
use eframe::egui::{self, Color32, Context, Popup, Vec2};
use egui_extras::{Column, TableBuilder};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
use time::Timestamp;
use tracing::{debug, info, warn};

use crate::{
    app::{event::Event, events::Events, store::Store},
    components::Components,
};

/// Nombre de fiches analysées en parallèle au maximum.
const MAX_CONCURRENT_ANALYSES: usize = 2;

/// Délai avant retry d'une fiche lors d'une erreur de quotas claude
const CLAUDE_QUOTAS_EXCEEDED_RETRY_DELAY_MS: i64 = 20 * 60 * 1000; // 20 minutes

#[derive(Default)]
pub struct Paystubs {
    search: String,
    analyse: Option<Analyse>,
    /// Fiches (par id) déjà envoyées au thread d'analyse, dont le résultat
    /// n'est pas encore revenu. Sert à ne pas les redéclencher en boucle :
    /// un `ProcessingError` en cours de retry reste dans cet état en base
    /// jusqu'à la fin de l'analyse, donc `get_next_paystub_to_analyse` le
    /// reverrait sans arrêt sans ce garde-fou.
    in_flight: HashSet<String>,
}

fn paystub_icon(paystub: &Paystub) -> &'static str {
    match paystub.state {
        PaystubState::Pending => egui_phosphor::regular::HOURGLASS,
        PaystubState::Processing => egui_phosphor::regular::FILE_MAGNIFYING_GLASS,
        PaystubState::ProcessingError { .. } => egui_phosphor::regular::WARNING_CIRCLE,
        PaystubState::Completed { .. } => egui_phosphor::regular::FILE_TEXT,
    }
}

fn paystub_status(paystub: &Paystub) -> (Color32, &'static str) {
    match paystub.state {
        PaystubState::Pending => (Color32::from_rgb(100, 100, 150), "En attente"),
        PaystubState::Processing => (Color32::from_rgb(100, 100, 220), "Analyse"),
        PaystubState::ProcessingError { .. } => (Color32::from_rgb(220, 100, 100), "Erreur"),
        PaystubState::Completed { .. } => (Color32::from_rgb(100, 220, 100), "Analysée"),
    }
}

fn get_next_paystub_to_analyse(
    store: &Store,
    in_flight: &HashSet<String>,
) -> Option<(String, Paystub)> {
    store
        .paystubs
        .iter()
        .find(|(id, paystub)| {
            if in_flight.contains(*id) {
                return false;
            }
            match &paystub.state {
                PaystubState::Pending => true,
                PaystubState::ProcessingError { error, .. }
                    if error.contains("You've hit your session limit") =>
                {
                    Timestamp::now().as_milliseconds() - paystub.since
                        > CLAUDE_QUOTAS_EXCEEDED_RETRY_DELAY_MS
                }
                PaystubState::ProcessingError { .. } => true,
                _ => false,
            }
        })
        .map(|(id, paystub)| (id.clone(), paystub.clone()))
}

/// Repasse en `ProcessingError` les fiches bloquées en `Processing` depuis
/// trop longtemps (voir `Paystub::is_stuck`) : sans ça, une fiche dont
/// l'analyse a été interrompue (ex: l'application a crashé) resterait
/// indéfiniment en "Analyse" sans jamais être retentée. Renvoie `true` si
/// au moins une fiche a été modifiée.
fn reap_stuck_paystubs(store: &mut Store, in_flight: &mut HashSet<String>) -> bool {
    let mut changed = false;
    for (id, paystub) in store.paystubs.iter_mut() {
        if paystub.is_stuck()
            && let Ok(timed_out) = paystub.to_timed_out()
        {
            warn!(
                id,
                file = %paystub.file,
                "fiche bloquée en Processing depuis trop longtemps, repassée en erreur"
            );
            *paystub = timed_out;
            in_flight.remove(id);
            changed = true;
        }
    }
    changed
}

/// Réduit un caractère accentué à sa forme sans accent (ex: 'é' -> 'e').
/// Volontairement limité aux caractères usuels en français : suffisant pour
/// `normalize_key`, où seule la cohérence (le même caractère d'origine
/// donne toujours le même résultat) compte, pas une translittération exacte.
fn fold_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'î' | 'ï' | 'Î' | 'Ï' => 'i',
        'ô' | 'ö' | 'õ' | 'Ô' | 'Ö' | 'Õ' => 'o',
        'ù' | 'û' | 'ü' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ÿ' | 'Ÿ' => 'y',
        'ñ' | 'Ñ' => 'n',
        'œ' | 'Œ' => 'o',
        'æ' | 'Æ' => 'a',
        other => other,
    }
}

/// Normalise une clé de `infos`/`datas` pour la comparaison : accents et
/// casse retirés, ne garde que les caractères alphanumériques. Sert à
/// repérer deux clés quasi identiques qui ne diffèrent que par un caractère
/// spécial ou un accent (ex: "Salaire de base" / "Salaire de base." /
/// "Salaire dé base"), pour éviter de dupliquer un label pour rien d'une
/// fiche à l'autre.
fn normalize_key(key: &str) -> String {
    key.chars()
        .map(fold_accent)
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Recale les clés d'`infos`/`datas` d'une fiche fraîchement analysée sur
/// celles déjà utilisées par les fiches précédentes du store, quand leurs
/// formes normalisées (voir `normalize_key`) correspondent. `infos` et
/// `datas` sont deux espaces de clés distincts, traités séparément.
fn reconcile_keys(
    store: &Store,
    infos: HashMap<String, String>,
    datas: HashMap<String, f32>,
) -> (HashMap<String, String>, HashMap<String, f32>) {
    let mut known_infos_keys: HashMap<String, String> = HashMap::new();
    let mut known_datas_keys: HashMap<String, String> = HashMap::new();

    for paystub in store.paystubs.values() {
        if let PaystubState::Completed {
            infos: existing_infos,
            datas: existing_datas,
            ..
        } = &paystub.state
        {
            for key in existing_infos.keys() {
                known_infos_keys
                    .entry(normalize_key(key))
                    .or_insert_with(|| key.clone());
            }
            for key in existing_datas.keys() {
                known_datas_keys
                    .entry(normalize_key(key))
                    .or_insert_with(|| key.clone());
            }
        }
    }

    let infos = infos
        .into_iter()
        .map(
            |(key, value)| match known_infos_keys.get(&normalize_key(&key)) {
                Some(canonical) if canonical != &key => {
                    debug!(
                        new_key = key,
                        canonical_key = canonical,
                        "clé infos recalée"
                    );
                    (canonical.clone(), value)
                }
                _ => (key, value),
            },
        )
        .collect();

    let datas = datas
        .into_iter()
        .map(
            |(key, value)| match known_datas_keys.get(&normalize_key(&key)) {
                Some(canonical) if canonical != &key => {
                    debug!(
                        new_key = key,
                        canonical_key = canonical,
                        "clé datas recalée"
                    );
                    (canonical.clone(), value)
                }
                _ => (key, value),
            },
        )
        .collect();

    (infos, datas)
}

impl Paystubs {
    /// Envoie au thread d'analyse toutes les fiches en attente, jusqu'à
    /// `MAX_CONCURRENT_ANALYSES` en vol. À appeler au démarrage puis à
    /// chaque frame, pour reprendre la main après chaque résultat reçu ou
    /// après l'import d'une nouvelle fiche (ailleurs, dans `Menu`).
    fn dispatch_next_paystubs(&mut self, store: &mut Store) {
        if reap_stuck_paystubs(store, &mut self.in_flight) {
            store.save();
        }

        let Some(analyse) = &self.analyse else {
            return;
        };

        while self.in_flight.len() < MAX_CONCURRENT_ANALYSES
            && let Some((id, next)) = get_next_paystub_to_analyse(store, &self.in_flight)
        {
            if let Some(stored) = store.paystubs.get_mut(&id)
                && let Ok(processing) = stored.to_processing()
            {
                *stored = processing;
            }
            info!(id, file = %next.file, "envoi d'une fiche pour analyse");
            self.in_flight.insert(id.clone());
            analyse.analyse(id, next);
        }
    }
}

impl Components for Paystubs {
    fn init(&mut self, cc: &eframe::CreationContext<'_>, store: &mut Store) {
        self.analyse = Some(Analyse::new(cc.egui_ctx.clone()));
        info!(
            paystubs = store.paystubs.len(),
            "composant Paystubs initialisé"
        );
        self.dispatch_next_paystubs(store);
    }

    fn update(&mut self, context: &Context, _events: &mut Events<Event>, store: &mut Store) {
        // Force un repaint au minimum toutes les 20 minutes
        // permet de laisser l'application ouverte tourner
        context.request_repaint_after(std::time::Duration::from_mins(20));

        let mut changed = false;
        if let Some(analyse) = &self.analyse {
            while let Some((id, mut result)) = analyse.try_recv() {
                if let PaystubState::Completed {
                    payment_date,
                    net_salary,
                    infos,
                    datas,
                } = result.state
                {
                    // Recale les clés sur celles déjà connues (accents/ponctuation
                    // près) avant de stocker, pour éviter qu'une fiche crée un
                    // nouveau label pour rien à cause d'une variation mineure.
                    let (infos, datas) = reconcile_keys(store, infos, datas);
                    info!(
                        id,
                        file = %result.file,
                        payment_date,
                        net_salary,
                        infos = infos.len(),
                        datas = datas.len(),
                        "résultat d'analyse reçu : fiche complétée"
                    );
                    result.state = PaystubState::Completed {
                        payment_date,
                        net_salary,
                        infos,
                        datas,
                    };
                } else if let PaystubState::ProcessingError { error, retry } = &result.state {
                    warn!(id, file = %result.file, retry, %error, "résultat d'analyse reçu : échec");
                } else {
                    debug!(id, file = %result.file, "résultat d'analyse reçu");
                }

                self.in_flight.remove(&id);
                store.paystubs.insert(id, result);
                changed = true;
            }
        }

        self.dispatch_next_paystubs(store);

        if changed {
            store.save();
        }
    }

    fn show(
        &mut self,
        ui: &mut eframe::egui::Ui,
        _frame: &mut eframe::Frame,
        _events: &mut Events<Event>,
        store: &Store,
    ) {
        egui::Panel::left("paystubs")
            .resizable(false)
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Fiches de paie");

                    egui::Frame::new()
                        .fill(ui.visuals().widgets.inactive.bg_fill) // gris qui contraste avec le fond
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("{}", store.paystubs.len(),))
                                    .size(12.0)
                                    .color(ui.visuals().weak_text_color()), // gris pour le texte
                            );
                        });
                });

                ui.add_space(20.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .min_size(Vec2::new(350.0, 0.0))
                        .prefix(
                            egui::RichText::new(egui_phosphor::regular::MAGNIFYING_GLASS)
                                .size(13.0),
                        )
                        .hint_text(egui::RichText::new("Rechercher..").size(13.0))
                        .font(egui::FontId::proportional(13.0))
                        .margin(egui::Margin::symmetric(10, 8)),
                );
                ui.add_space(20.0);

                egui::Panel::bottom("paystubs_footer")
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.add_space(20.0);

                        let (mut processing, mut completed, mut error, mut pending) = (0, 0, 0, 0);

                        for paystub in store.paystubs.values() {
                            match paystub.state {
                                PaystubState::Pending => pending += 1,
                                PaystubState::Processing => processing += 1,
                                PaystubState::ProcessingError { .. } => error += 1,
                                PaystubState::Completed { .. } => completed += 1,
                            }
                        }

                        ui.horizontal(|ui| {
                            egui::Frame::new()
                                .fill(ui.visuals().widgets.inactive.bg_fill) // gris qui contraste avec le fond
                                .corner_radius(egui::CornerRadius::same(8))
                                .inner_margin(egui::Margin::symmetric(8, 3))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "En attente{}: {}",
                                            if pending > 1 { "s" } else { "" },
                                            pending
                                        ))
                                        .size(11.0)
                                        .color(ui.visuals().weak_text_color()), // gris pour le texte
                                    );
                                });

                            egui::Frame::new()
                                .fill(ui.visuals().widgets.inactive.bg_fill) // gris qui contraste avec le fond
                                .corner_radius(egui::CornerRadius::same(8))
                                .inner_margin(egui::Margin::symmetric(8, 3))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Analyse{}: {}",
                                            if processing > 1 { "s" } else { "" },
                                            processing
                                        ))
                                        .size(11.0)
                                        .color(ui.visuals().weak_text_color()), // gris pour le texte
                                    );
                                });

                            egui::Frame::new()
                                .fill(ui.visuals().widgets.inactive.bg_fill) // gris qui contraste avec le fond
                                .corner_radius(egui::CornerRadius::same(8))
                                .inner_margin(egui::Margin::symmetric(8, 3))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Erreur{}: {}",
                                            if error > 1 { "s" } else { "" },
                                            error
                                        ))
                                        .size(11.0)
                                        .color(ui.visuals().weak_text_color()), // gris pour le texte
                                    );
                                });

                            egui::Frame::new()
                                .fill(ui.visuals().widgets.inactive.bg_fill) // gris qui contraste avec le fond
                                .corner_radius(egui::CornerRadius::same(8))
                                .inner_margin(egui::Margin::symmetric(8, 3))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Analysée{}: {}",
                                            if completed > 1 { "s" } else { "" },
                                            completed
                                        ))
                                        .size(11.0)
                                        .color(ui.visuals().weak_text_color()), // gris pour le texte
                                    );
                                });
                        });
                    });
                let search = &self.search;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (_id, paystub) in store
                            .paystubs
                            .iter()
                            .filter(|(_, item)| {
                                item.file.to_lowercase().contains(&search.to_lowercase())
                            })
                            .collect::<Vec<(&String, &Paystub)>>()
                        {
                            let response = ui
                                .scope_builder(
                                    egui::UiBuilder::new().sense(egui::Sense::click()),
                                    |ui| {
                                        ui.style_mut().interaction.selectable_labels = false;
                                        let hovered = ui.response().hovered();
                                        egui::Frame::group(ui.style())
                                            .fill(if hovered {
                                                ui.visuals().widgets.hovered.weak_bg_fill
                                            } else {
                                                Color32::TRANSPARENT
                                            })
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.set_width(300.0);

                                                    ui.label(
                                                        egui::RichText::new(paystub_icon(paystub))
                                                            .size(30.0),
                                                    );

                                                    ui.vertical(|ui| {
                                                        ui.label(
                                                            egui::RichText::new(
                                                                Path::new(&paystub.file)
                                                                    .file_stem()
                                                                    .and_then(|s| s.to_str())
                                                                    .unwrap_or(""),
                                                            )
                                                            .size(15.0),
                                                        );

                                                        let (color, status) =
                                                            paystub_status(paystub);
                                                        ui.horizontal(|ui| {
                                                            ui.colored_label(
                                                                color,
                                                                egui::RichText::new(status)
                                                                    .size(15.0),
                                                            );
                                                            if let PaystubState::ProcessingError {
                                                                retry,
                                                                ..
                                                            } = &paystub.state
                                                            {
                                                                egui::Frame::new()
                                                                    .fill(
                                                                        ui.visuals()
                                                                            .widgets
                                                                            .inactive
                                                                            .bg_fill,
                                                                    ) // gris qui contraste avec le fond
                                                                    .corner_radius(
                                                                        egui::CornerRadius::same(8),
                                                                    )
                                                                    .inner_margin(
                                                                        egui::Margin::symmetric(
                                                                            8, 3,
                                                                        ),
                                                                    )
                                                                    .show(ui, |ui| {
                                                                        ui.label(
                                                                        egui::RichText::new(
                                                                            format!(
                                                                                "{}",
                                                                                retry,
                                                                            ),
                                                                        )
                                                                        .size(12.0)
                                                                        .color(
                                                                            ui.visuals()
                                                                                .weak_text_color(),
                                                                        ), // gris pour le texte
                                                                    );
                                                                    });
                                                            }
                                                        });
                                                    });
                                                });
                                            });
                                    },
                                )
                                .response;

                            if matches!(
                                paystub.state,
                                PaystubState::Pending | PaystubState::Processing,
                            ) {
                                continue;
                            }

                            if let PaystubState::ProcessingError { error, .. } = &paystub.state
                                && error.is_empty()
                            {
                                continue;
                            }

                            Popup::menu(&response)
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                .show(|ui| {
                                    ui.set_width(300.0);
                                    let max_height =
                                        (ui.ctx().viewport_rect().height() * 0.6).min(300.0);
                                    ui.set_max_height(max_height);

                                    if let PaystubState::Completed { infos, .. } = &paystub.state {
                                        TableBuilder::new(ui)
                                            .striped(true)
                                            .max_scroll_height(max_height)
                                            // Sans ça, les cellules héritent du layout
                                            // "justified" du `Popup::menu` englobant, et le
                                            // texte qui wrap sur 2 lignes se retrouve étiré
                                            // (mots espacés pour remplir la largeur).
                                            .cell_layout(egui::Layout::left_to_right(
                                                egui::Align::Center,
                                            ))
                                            // Largeurs bornées (plutôt que `Column::auto()`,
                                            // qui dimensionne sur le contenu et peut faire
                                            // déborder la table au-delà des 300px) pour que
                                            // le retour à la ligne se déclenche au lieu de
                                            // pousser la table plus large que le popup.
                                            .column(Column::exact(110.0))
                                            .column(Column::remainder())
                                            .body(|mut body| {
                                                let mut infos: Vec<_> = infos.iter().collect();
                                                infos.sort_unstable_by_key(|(key, _)| *key);
                                                for (key, value) in infos {
                                                    body.row(34.0, |mut row| {
                                                        row.col(|ui| {
                                                            ui.add(egui::Label::new(key).wrap());
                                                        });
                                                        row.col(|ui| {
                                                            ui.add(egui::Label::new(value).wrap());
                                                        });
                                                    });
                                                }
                                            });
                                    } else if let PaystubState::ProcessingError { error, .. } =
                                        &paystub.state
                                    {
                                        egui::ScrollArea::vertical().auto_shrink(false).show(
                                            ui,
                                            |ui| {
                                                ui.label(error);
                                            },
                                        );
                                    }
                                });
                        }
                    });
            });
    }
}

#[cfg(test)]
mod normalize_key_tests {
    use super::normalize_key;

    #[test]
    fn similar_keys_normalize_identically() {
        let a = normalize_key("Remunération rbute.(1)");
        let b = normalize_key("Remunération rbute.(1)  ");
        let c = normalize_key("Remunération rbute(1)");

        assert_eq!(a, b);
        assert_eq!(a, c);
    }
}

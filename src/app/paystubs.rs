use crate::app::database::Database;
use bitcode::{Decode, Encode};
use std::collections::HashMap;
use time::Timestamp;
use uuid::Uuid;

type DateTime = i64;

/// Erreurs pouvant survenir sur un `Paystub` ou son stockage.
#[derive(Debug)]
pub enum PaystubError {
    /// Transition d'état demandée non autorisée depuis l'état courant.
    Transition {
        from: &'static str,
        to: &'static str,
    },
    /// Échec de lecture/écriture dans la `Database` sous-jacente ; contient
    /// le message d'erreur générique renvoyé par `Database`.
    Database(&'static str),
}

/// État d'une fiche de paie en cours de traitement.
///
/// Chaque variante porte les données propres à son état (ex: `error`/`retry`
/// uniquement pour `ProcessingError`). Les transitions entre variantes se
/// font via les méthodes `to_*` de `impl Paystub`, jamais en construisant
/// directement une variante depuis l'extérieur (hormis `Pending` via
/// `Paystub::pending`).
#[derive(Encode, Decode)]
pub enum Paystub {
    /// Fichier détecté, en attente de traitement.
    Pending { file: String, since: DateTime },
    /// Traitement du fichier en cours.
    Processing { file: String, since: DateTime },
    /// Le traitement a échoué ; `retry` compte le nombre de tentatives.
    ProcessingError {
        file: String,
        since: DateTime,
        error: String,
        retry: u8,
    },
    /// Traitement terminé avec succès ; `datas` contient les valeurs extraites.
    Completed {
        file: String,
        since: DateTime,
        datas: HashMap<String, f32>,
    },
}

impl Paystub {
    /// Crée une nouvelle fiche de paie à l'état `Pending`, avec l'horodatage courant.
    pub fn pending(file: String) -> Paystub {
        Paystub::Pending {
            file,
            since: Timestamp::now().as_milliseconds(),
        }
    }

    /// Passe la fiche à l'état `Processing`.
    ///
    /// Seule une fiche `Pending` peut effectuer cette transition ; toute
    /// autre variante renvoie `PaystubError::Transition`.
    pub fn to_processing(&self) -> Result<Paystub, PaystubError> {
        match self {
            Paystub::Pending { file, .. } => Ok(Paystub::Processing {
                file: file.to_string(),
                since: Timestamp::now().as_milliseconds(),
            }),
            Paystub::Processing { .. } => Err(PaystubError::Transition {
                from: "Processing",
                to: "Processing",
            }),
            Paystub::ProcessingError { .. } => Err(PaystubError::Transition {
                from: "ProcessingError",
                to: "Processing",
            }),
            Paystub::Completed { .. } => Err(PaystubError::Transition {
                from: "Completed",
                to: "Processing",
            }),
        }
    }

    /// Passe la fiche à l'état `ProcessingError`.
    ///
    /// Autorisé depuis `Processing` (première erreur, `retry = 1`) et depuis
    /// `ProcessingError` (nouvel essai, `retry` incrémenté). Toute autre
    /// variante renvoie `PaystubError::Transition`.
    pub fn to_processing_error(&self, error: &str) -> Result<Paystub, PaystubError> {
        match self {
            Paystub::Pending { .. } => Err(PaystubError::Transition {
                from: "Pending",
                to: "ProcessingError",
            }),
            Paystub::Processing { file, .. } => Ok(Paystub::ProcessingError {
                file: file.to_string(),
                since: Timestamp::now().as_milliseconds(),
                error: error.to_string(),
                retry: 1,
            }),
            Paystub::ProcessingError { file, retry, .. } => Ok(Paystub::ProcessingError {
                file: file.to_string(),
                since: Timestamp::now().as_milliseconds(),
                error: error.to_string(),
                retry: retry + 1,
            }),
            Paystub::Completed { .. } => Err(PaystubError::Transition {
                from: "Completed",
                to: "ProcessingError",
            }),
        }
    }

    /// Passe la fiche à l'état `Completed` avec les données extraites.
    ///
    /// Autorisé depuis `Processing` ou `ProcessingError` (un retry réussi
    /// aboutit directement à `Completed`). Toute autre variante renvoie
    /// `PaystubError::Transition`.
    pub fn to_completed(&self, datas: HashMap<String, f32>) -> Result<Paystub, PaystubError> {
        match self {
            Paystub::Pending { .. } => Err(PaystubError::Transition {
                from: "Pending",
                to: "Completed",
            }),
            Paystub::Processing { file, .. } => Ok(Paystub::Completed {
                file: file.to_string(),
                since: Timestamp::now().as_milliseconds(),
                datas,
            }),
            Paystub::ProcessingError { file, .. } => Ok(Paystub::Completed {
                file: file.to_string(),
                since: Timestamp::now().as_milliseconds(),
                datas,
            }),
            Paystub::Completed { .. } => Err(PaystubError::Transition {
                from: "Completed",
                to: "Completed",
            }),
        }
    }
}

/// Accès persistant aux `Paystub`, indexés par un identifiant `Uuid` v4 généré à l'ajout.
pub struct PaystubRepository {
    database: Database<Paystub>,
}

impl PaystubRepository {
    /// Ouvre (ou crée) la base de données `paystubs` sur le disque.
    pub fn load() -> Result<PaystubRepository, PaystubError> {
        match Database::load("paystubs") {
            Ok(database) => Ok(PaystubRepository { database }),
            Err(error) => Err(PaystubError::Database(error)),
        }
    }

    /// Enregistre une nouvelle fiche sous un id généré, et renvoie cet id.
    pub fn add(&mut self, paystub: Paystub) -> Result<String, PaystubError> {
        self.database
            .upsert(&Uuid::new_v4().to_string(), paystub)
            .map_err(PaystubError::Database)
    }

    /// Récupère la fiche associée à `id`, ou `None` si l'id est inconnu.
    pub fn get(&mut self, id: &str) -> Result<Option<Paystub>, PaystubError> {
        self.database.get(id).map_err(PaystubError::Database)
    }

    /// Récupère toutes les fiches, indexées par leur id.
    pub fn get_all(&mut self) -> Result<HashMap<String, Paystub>, PaystubError> {
        self.database.get_all().map_err(PaystubError::Database)
    }

    /// Supprime la fiche associée à `id` et la renvoie, ou `None` si l'id est inconnu.
    pub fn delete(&mut self, id: &str) -> Result<Option<Paystub>, PaystubError> {
        let paystub = self.get(id)?;
        self.database.remove(id).map_err(PaystubError::Database)?;
        Ok(paystub)
    }

    /// Remplace la fiche associée à `id` si elle existe ; renvoie `None` sans
    /// effet si `id` est inconnu.
    pub fn update(&mut self, id: &str, paystub: Paystub) -> Result<Option<String>, PaystubError> {
        if !self.database.contains(id).map_err(PaystubError::Database)? {
            return Ok(None);
        }
        self.database
            .upsert(id, paystub)
            .map(Some)
            .map_err(PaystubError::Database)
    }
}

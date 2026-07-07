use bitcode::{Decode, Encode};
use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use std::collections::HashMap;
use std::fs;
use std::marker::PhantomData;

const DIR: &str = "datas";
const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("default");

/// Base de données clé-valeur persistante, fondée sur `redb`, dédiée au stockage
/// de valeurs d'un seul type `T`.
pub struct Database<T> {
    name: &'static str,
    db: redb::Database,
    location: String,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Database<T>
where
    T: Encode + for<'a> Decode<'a>,
{
    /// Ouvre le fichier `datas/{name}.redb`, en le créant s'il n'existe pas encore.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use fiche_de_paie::app::database::Database;
    ///
    /// let employes: Database<String> = Database::load("employes")?;
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn load(name: &'static str) -> Result<Self, &'static str> {
        // Création ou ouverture de la base de données sur le disque
        fs::create_dir_all(DIR).map_err(|error| {
            let msg = "Création du dossier datas.";
            tracing::error!(caused = %error, database = name, msg);
            msg
        })?;
        let location = format!("{}/{}.redb", DIR, name);
        let db = redb::Database::create(&location).map_err(|error| {
            let msg = "Création de la base de données.";
            tracing::error!(caused = %error, database = name, msg);
            msg
        })?;

        // Création de la table si elle n'existe pas encore
        let transaction = db.begin_write().map_err(|error| {
            let msg = "Ouverture d'une transaction en écriture.";
            tracing::error!(caused = %error, database = name, msg);
            msg
        })?;
        transaction.open_table(TABLE).map_err(|error| {
            let msg = "Création de la table.";
            tracing::error!(caused = %error, database = name, msg);
            msg
        })?;
        transaction.commit().map_err(|error| {
            let msg = "Commit de la transaction.";
            tracing::error!(caused = %error, database = name, msg);
            msg
        })?;

        Ok(Self {
            name,
            db,
            location,
            _marker: PhantomData,
        })
    }

    /// Insère `value` sous la clé `key`, en remplaçant la valeur existante le cas échéant.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use fiche_de_paie::app::database::Database;
    ///
    /// let mut employes: Database<String> = Database::load("employes")?;
    /// employes.upsert("42", "Alice Dupont".to_string())?;
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn upsert(&self, key: &str, value: T) -> Result<String, &'static str> {
        let transaction = self.db.begin_write().map_err(|error| {
            let msg = "Ouverture d'une transaction en écriture.";
            tracing::error!(caused = %error, database = self.name,  key, msg);
            msg
        })?;
        {
            let mut table = transaction.open_table(TABLE).map_err(|error| {
                let msg = "Ouverture de la table.";
                tracing::error!(caused = %error, database = self.name,  key, msg);
                msg
            })?;

            let bytes = bitcode::encode(&value);
            table.insert(key, bytes.as_slice()).map_err(|error| {
                let msg = "Insertion de la valeur.";
                tracing::error!(caused = %error, database = self.name, key, msg);
                msg
            })?;
        }
        transaction.commit().map_err(|error| {
            let msg = "Commit de la transaction.";
            tracing::error!(caused = %error, database = self.name,  key, msg);
            msg
        })?;
        Ok(key.to_string())
    }

    /// Récupère la valeur associée à `key`, ou `None` si la clé est absente.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use fiche_de_paie::app::database::Database;
    ///
    /// let mut employes: Database<String> = Database::load("employes")?;
    /// if let Some(nom) = employes.get("42")? {
    ///     println!("{nom}");
    /// }
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn get(&self, key: &str) -> Result<Option<T>, &'static str> {
        let transaction = self.db.begin_read().map_err(|error| {
            let msg = "Ouverture d'une transaction en lecture.";
            tracing::error!(caused = %error, database = self.name,  key, msg);
            msg
        })?;
        let table = transaction.open_table(TABLE).map_err(|error| {
            let msg = "Ouverture de la table.";
            tracing::error!(caused = %error, database = self.name,  key, msg);
            msg
        })?;

        let value = table.get(key).map_err(|error| {
            let msg = "Lecture d'une valeur.";
            tracing::error!(caused = %error, database = self.name,  key, msg);
            msg
        })?;
        value
            .map(|value| bitcode::decode::<T>(value.value()))
            .transpose()
            .map_err(|error| {
                let msg = "Décodage d'une valeur.";
                tracing::error!(caused = %error, database = self.name, key, msg);
                msg
            })
    }

    /// Indique si `key` est présente, sans décoder la valeur associée.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use fiche_de_paie::app::database::Database;
    ///
    /// let employes: Database<String> = Database::load("employes")?;
    /// if employes.contains("42")? {
    ///     println!("42 existe déjà");
    /// }
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn contains(&self, key: &str) -> Result<bool, &'static str> {
        let transaction = self.db.begin_read().map_err(|error| {
            let msg = "Ouverture d'une transaction en lecture.";
            tracing::error!(caused = %error, database = self.name, key, msg);
            msg
        })?;
        let table = transaction.open_table(TABLE).map_err(|error| {
            let msg = "Ouverture de la table.";
            tracing::error!(caused = %error, database = self.name, key, msg);
            msg
        })?;

        table
            .get(key)
            .map(|value| value.is_some())
            .map_err(|error| {
                let msg = "Lecture d'une valeur.";
                tracing::error!(caused = %error, database = self.name, key, msg);
                msg
            })
    }

    /// Récupère toutes les paires clé/valeur de la table.
    ///
    /// La lecture est atomique : si une entrée est illisible ou corrompue,
    /// l'ensemble de l'opération échoue plutôt que de renvoyer un résultat partiel.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use fiche_de_paie::app::database::Database;
    ///
    /// let mut employes: Database<String> = Database::load("employes")?;
    /// for (id, nom) in employes.get_all()? {
    ///     println!("{id} -> {nom}");
    /// }
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn get_all(&self) -> Result<HashMap<String, T>, &'static str> {
        let transaction = self.db.begin_read().map_err(|error| {
            let msg = "Ouverture d'une transaction en lecture.";
            tracing::error!(caused = %error, database = self.name, msg);
            msg
        })?;
        let table = transaction.open_table(TABLE).map_err(|error| {
            let msg = "Ouverture de la table.";
            tracing::error!(caused = %error, database = self.name, msg);
            msg
        })?;
        table
            .iter()
            .map_err(|error| {
                let msg = "Lecture des valeurs.";
                tracing::error!(caused = %error, database = self.name, msg);
                msg
            })?
            .map(|entry| {
                let (key, value) = entry.map_err(|error| {
                    let msg = "Lecture d'une entrée.";
                    tracing::error!(caused = %error, database = self.name, msg);
                    msg
                })?;
                let key = key.value().to_string();
                bitcode::decode::<T>(value.value())
                    .map(|v| (key.clone(), v))
                    .map_err(|error| {
                        let msg = "Décodage d'une valeur.";
                        tracing::error!(caused = %error, database = self.name, key, msg);
                        msg
                    })
            })
            .collect()
    }

    /// Supprime le fichier `.redb` associé à cette base de données.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use fiche_de_paie::app::database::Database;
    ///
    /// let employes: Database<String> = Database::load("employes")?;
    /// employes.delete()?;
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn delete(self) -> Result<(), &'static str> {
        fs::remove_file(&self.location).map_err(|error| {
            let msg = "Suppression de la base de données.";
            tracing::error!(caused = %error, database = self.name, location = self.location, msg);
            msg
        })
    }

    /// Supprime la valeur associée à `key`, sans effet si la clé est absente.
    ///
    /// # Exemples
    ///
    /// ```no_run
    /// use fiche_de_paie::app::database::Database;
    ///
    /// let mut employes: Database<String> = Database::load("employes")?;
    /// employes.remove("42")?;
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn remove(&mut self, key: &str) -> Result<(), &'static str> {
        let transaction = self.db.begin_write().map_err(|error| {
            let msg = "Ouverture d'une transaction en écriture.";
            tracing::error!(caused = %error, database = self.name, key, msg);
            msg
        })?;
        {
            let mut table = transaction.open_table(TABLE).map_err(|error| {
                let msg = "Ouverture de la table.";
                tracing::error!(caused = %error, database = self.name, key, msg);
                msg
            })?;

            table.remove(key).map_err(|error| {
                let msg = "Suppression de la valeur.";
                tracing::error!(caused = %error, database = self.name, key, msg);
                msg
            })?;
        }
        transaction.commit().map_err(|error| {
            let msg = "Commit de la transaction.";
            tracing::error!(caused = %error, database = self.name, key, msg);
            msg
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn add_puis_get_renvoie_la_valeur() {
        let database: Database<String> = Database::load("test_add_puis_get").unwrap();
        let value = database
            .upsert("cle", "valeur".to_string())
            .and_then(|_| database.get("cle"));
        database.delete().unwrap();
        assert_eq!(value.unwrap(), Some("valeur".to_string()));
    }

    #[test_log::test]
    fn get_sur_cle_absente_renvoie_none() {
        let database: Database<String> = Database::load("test_get_cle_absente").unwrap();
        let value = database
            .upsert("autre_cle", "valeur".to_string())
            .and_then(|_| database.get("inconnue"));
        database.delete().unwrap();
        assert_eq!(value.unwrap(), None);
    }

    #[test_log::test]
    fn add_ecrase_la_valeur_existante() {
        let database: Database<String> = Database::load("test_add_ecrase").unwrap();
        let value = database
            .upsert("cle", "premiere".to_string())
            .and_then(|_| database.upsert("cle", "seconde".to_string()))
            .and_then(|_| database.get("cle"));
        database.delete().unwrap();
        assert_eq!(value.unwrap(), Some("seconde".to_string()));
    }

    #[test_log::test]
    fn remove_supprime_la_valeur() {
        let mut database: Database<String> = Database::load("test_remove").unwrap();
        let value = database
            .upsert("cle", "valeur".to_string())
            .and_then(|_| database.remove("cle"))
            .and_then(|_| database.get("cle"));
        database.delete().unwrap();
        assert_eq!(value.unwrap(), None);
    }

    #[test_log::test]
    fn remove_sur_cle_absente_ne_renvoie_pas_derreur() {
        let mut database: Database<String> = Database::load("test_remove_absente").unwrap();
        let value = database.remove("inconnue");
        database.delete().unwrap();
        value.unwrap();
    }

    #[test_log::test]
    fn get_all_renvoie_toutes_les_entrees() {
        let database: Database<String> = Database::load("test_get_all").unwrap();
        let all = database
            .upsert("a", "1".to_string())
            .and_then(|_| database.upsert("b", "2".to_string()))
            .and_then(|_| database.get_all());
        database.delete().unwrap();
        let all = all.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all.get("a"), Some(&"1".to_string()));
        assert_eq!(all.get("b"), Some(&"2".to_string()));
    }

    #[test_log::test]
    fn get_all_renvoie_une_map_vide() {
        let database: Database<String> = Database::load("test_get_all_empty").unwrap();
        let all = database.get_all();
        database.delete().unwrap();
        let all = all.unwrap();
        assert_eq!(all.len(), 0);
    }
}

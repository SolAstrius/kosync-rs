use redb::{Database as RedbDatabase, ReadableTable, TableDefinition};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};
use crate::models::{DocumentAnnotations, Progress};

// Table definitions
const USERS: TableDefinition<&str, &str> = TableDefinition::new("users");
const PROGRESS: TableDefinition<&str, &[u8]> = TableDefinition::new("progress");
const ANNOTATIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("annotations");

pub struct Database {
    db: RedbDatabase,
}

impl Database {
    pub fn open(path: &str) -> Result<Self> {
        let db = RedbDatabase::create(path)?;

        // Initialize tables
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(USERS)?;
            let _ = write_txn.open_table(PROGRESS)?;
            let _ = write_txn.open_table(ANNOTATIONS)?;
        }
        write_txn.commit()?;

        Ok(Self { db })
    }

    // === User operations ===

    pub fn create_user(&self, username: &str, password_hash: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let created = {
            let mut table = write_txn.open_table(USERS)?;
            if table.get(username)?.is_some() {
                false
            } else {
                table.insert(username, password_hash)?;
                true
            }
        };
        write_txn.commit()?;
        Ok(created)
    }

    pub fn verify_user(&self, username: &str, password_hash: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(USERS)?;
        match table.get(username)? {
            Some(stored) => Ok(stored.value() == password_hash),
            None => Ok(false),
        }
    }

    // === Progress operations (legacy KOSync) ===

    fn progress_key(username: &str, document: &str) -> String {
        format!("{}:{}", username, document)
    }

    pub fn get_progress(&self, username: &str, document: &str) -> Result<Progress> {
        let key = Self::progress_key(username, document);
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(PROGRESS)?;

        match table.get(key.as_str())? {
            Some(data) => {
                let progress: Progress = serde_json::from_slice(data.value())?;
                Ok(progress)
            }
            None => Ok(Progress::default()),
        }
    }

    pub fn set_progress(
        &self,
        username: &str,
        document: &str,
        progress: &str,
        percentage: f64,
        device: &str,
        device_id: Option<&str>,
    ) -> Result<i64> {
        let key = Self::progress_key(username, document);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let data = Progress {
            document: Some(document.to_string()),
            progress: Some(progress.to_string()),
            percentage: Some(percentage),
            device: Some(device.to_string()),
            device_id: device_id.map(String::from),
            timestamp: Some(timestamp),
        };

        let json = serde_json::to_vec(&data)?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PROGRESS)?;
            table.insert(key.as_str(), json.as_slice())?;
        }
        write_txn.commit()?;

        Ok(timestamp)
    }

    // === Annotations operations (extended API) ===

    fn annotations_key(username: &str, document: &str) -> String {
        format!("{}:{}", username, document)
    }

    pub fn get_annotations(&self, username: &str, document: &str) -> Result<DocumentAnnotations> {
        let key = Self::annotations_key(username, document);
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ANNOTATIONS)?;

        match table.get(key.as_str())? {
            Some(data) => {
                let annotations: DocumentAnnotations = serde_json::from_slice(data.value())?;
                Ok(annotations)
            }
            None => Ok(DocumentAnnotations::default()),
        }
    }

    pub fn set_annotations(
        &self,
        username: &str,
        document: &str,
        annotations: &DocumentAnnotations,
    ) -> Result<()> {
        let key = Self::annotations_key(username, document);
        let json = serde_json::to_vec(annotations)?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ANNOTATIONS)?;
            table.insert(key.as_str(), json.as_slice())?;
        }
        write_txn.commit()?;

        Ok(())
    }

    pub fn update_annotations(
        &self,
        username: &str,
        document: &str,
        new_annotations: Vec<crate::models::Annotation>,
        new_deleted: Vec<String>,
        base_version: Option<u64>,
    ) -> Result<(u64, i64)> {
        let key = Self::annotations_key(username, document);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let write_txn = self.db.begin_write()?;
        let (version, ts) = {
            let mut table = write_txn.open_table(ANNOTATIONS)?;

            // Get current state
            let current: DocumentAnnotations = match table.get(key.as_str())? {
                Some(data) => serde_json::from_slice(data.value())?,
                None => DocumentAnnotations::default(),
            };

            // Check version if provided (optimistic locking)
            if let Some(base) = base_version {
                if base != current.version && current.version > 0 {
                    return Err(AppError::VersionConflict);
                }
            }

            // Merge annotations
            let merged = merge_annotations(
                current.annotations,
                new_annotations,
                &current.deleted,
                &new_deleted,
            );

            // Merge deleted lists
            let mut all_deleted = current.deleted;
            for d in new_deleted {
                if !all_deleted.contains(&d) {
                    all_deleted.push(d);
                }
            }

            let new_doc = DocumentAnnotations {
                version: current.version + 1,
                annotations: merged,
                deleted: all_deleted,
                updated_at: timestamp,
            };

            let json = serde_json::to_vec(&new_doc)?;
            table.insert(key.as_str(), json.as_slice())?;

            (new_doc.version, timestamp)
        };
        write_txn.commit()?;

        Ok((version, ts))
    }
}

/// Merge annotations from two sources using timestamp-based conflict resolution
fn merge_annotations(
    server: Vec<crate::models::Annotation>,
    client: Vec<crate::models::Annotation>,
    server_deleted: &[String],
    client_deleted: &[String],
) -> Vec<crate::models::Annotation> {
    use std::collections::HashMap;

    // Index by position key
    fn position_key(a: &crate::models::Annotation) -> String {
        format!(
            "{}|{:?}|{:?}",
            serde_json::to_string(&a.page).unwrap_or_default(),
            a.pos0,
            a.pos1
        )
    }

    fn effective_time(a: &crate::models::Annotation) -> &str {
        a.datetime_updated.as_deref().unwrap_or(&a.datetime)
    }

    let mut merged: HashMap<String, crate::models::Annotation> = HashMap::new();

    // Add server annotations (skip if deleted by client)
    for anno in server {
        if !client_deleted.contains(&anno.datetime) {
            merged.insert(position_key(&anno), anno);
        }
    }

    // Merge client annotations
    for anno in client {
        if server_deleted.contains(&anno.datetime) {
            continue; // Skip if deleted on server
        }

        let key = position_key(&anno);
        if let Some(existing) = merged.get(&key) {
            // Keep newer one
            if effective_time(&anno) > effective_time(existing) {
                merged.insert(key, anno);
            }
        } else {
            merged.insert(key, anno);
        }
    }

    merged.into_values().collect()
}

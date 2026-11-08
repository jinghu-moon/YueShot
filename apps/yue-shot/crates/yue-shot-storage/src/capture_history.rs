//! YueShot-owned history metadata and immutable source references.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use yue_shot_model::CaptureSource;

use crate::{StorageError, artifact_store::CaptureArtifactStore};

const SCHEMA_VERSION: u32 = 2;
const APPLICATION_ID: u32 = 0x59534854; // YSHT
const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 100;
// Existing journals can contain a 1 MiB preview serialized as a JSON byte array
// (up to 4 MiB). Bound recovery without discarding those durable source references.
const MAX_PENDING_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cursor {
    created_at_unix_ms: i64,
    id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PendingCapture {
    version: u32,
    id: String,
    source: CaptureSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureHistoryEntry {
    pub id: String,
    pub created_at_unix_ms: i64,
    pub source: CaptureSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureHistoryPage {
    pub items: Vec<CaptureHistoryEntry>,
    pub next_cursor: Option<String>,
}

/// Owns YueShot history rows and source-reference counts.
#[derive(Debug, Clone)]
pub struct CaptureHistoryStore {
    root: PathBuf,
    history_root: PathBuf,
    database_path: PathBuf,
}

impl CaptureHistoryStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StorageError> {
        let root = root.as_ref().to_path_buf();
        let history_root = root.join("capture-history");
        fs::create_dir_all(history_root.join("data"))?;
        let store = Self {
            database_path: history_root.join("data/yue-shot-history.db"),
            root,
            history_root,
        };
        let mut connection = store.open_connection()?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let captures_exist: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='captures')",
            [],
            |row| row.get(0),
        )?;
        let unrelated: u32 = connection.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT IN ('captures','retired_sources','sqlite_sequence')",
            [],
            |row| row.get(0),
        )?;
        if unrelated != 0 {
            return Err(StorageError::Data(
                "database contains unrelated tables; original preserved".into(),
            ));
        }
        if version == 1 || (version == 0 && captures_exist) {
            validate_v1_shape(&connection)?;
            migrate_v1(&mut connection)?;
        } else if version == 0 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            create_current_schema(&transaction)?;
            transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
            transaction.commit()?;
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        drop(connection);
        store.recover_pending()?;
        store.recover_retired()?;
        Ok(store)
    }

    /// The single owner of durable originals; writers and deletion use this same root.
    pub fn artifact_root(&self) -> PathBuf {
        self.root.join("artifacts/capture")
    }

    fn open_connection(&self) -> Result<Connection, StorageError> {
        let connection = Connection::open_with_flags(
            &self.database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let application: u32 =
            connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        if version > SCHEMA_VERSION
            || (application != 0 && application != APPLICATION_ID)
            || (version != 0 && application != APPLICATION_ID)
        {
            return Err(StorageError::Data(format!(
                "unsupported capture database identity/schema ({application:#x}/{version}); original preserved"
            )));
        }
        // Reject future schemas before changing journaling or starting recovery writes.
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(connection)
    }
    fn connect(&self) -> Result<Connection, StorageError> {
        let connection = self.open_connection()?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version != SCHEMA_VERSION {
            return Err(StorageError::Data(
                "capture schema is not initialized; reopen the store".into(),
            ));
        }
        Ok(connection)
    }

    /// Durably register an immutable source.
    pub fn save_capture(
        &self,
        id: impl Into<String>,
        source: CaptureSource,
    ) -> Result<(), StorageError> {
        let id = id.into();
        validate_capture(&id, &source)?;
        let pending = PendingCapture {
            version: 1,
            id: id.clone(),
            source: source.clone(),
        };
        let journal = self.journal_path(&id);
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        fs::create_dir_all(self.history_root.join("pending-captures"))?;
        if journal.exists() {
            let existing = read_pending(&journal)?;
            if existing != pending {
                return Err(StorageError::Invalid(
                    "capture id has a different pending registration".into(),
                ));
            }
        } else {
            let temporary = journal.with_extension("tmp");
            let bytes = serde_json::to_vec(&pending)
                .map_err(|error| StorageError::Data(error.to_string()))?;
            if bytes.len() as u64 > MAX_PENDING_BYTES {
                return Err(StorageError::Invalid(
                    "pending capture record exceeds its budget".into(),
                ));
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            let result = file
                .write_all(&bytes)
                .and_then(|()| file.sync_all())
                .and_then(|()| fs::rename(&temporary, &journal));
            if let Err(error) = result {
                let _ = fs::remove_file(temporary);
                return Err(error.into());
            }
        }
        self.insert_pending(&transaction, &pending)?;
        transaction.commit()?;
        fs::remove_file(journal)?;
        Ok(())
    }

    fn insert_pending(
        &self,
        connection: &Connection,
        pending: &PendingCapture,
    ) -> Result<(), StorageError> {
        let existing: Option<String> = connection
            .query_row(
                "SELECT source_json FROM captures WHERE id = ?1",
                [&pending.id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(json) = existing {
            let existing: CaptureSource = serde_json::from_str(&json)
                .map_err(|error| StorageError::Data(error.to_string()))?;
            if existing != pending.source {
                return Err(StorageError::Invalid(
                    "capture id belongs to a different immutable source".into(),
                ));
            }
            return Ok(());
        }
        let source_json = serde_json::to_string(&pending.source)
            .map_err(|error| StorageError::Data(error.to_string()))?;
        connection.execute(
            "INSERT INTO captures(id, created_at_unix_ms, artifact_path, source_json) VALUES (?1, ?2, ?3, ?4)",
            params![pending.id, pending.source.metadata.captured_at_unix_ms, pending.source.artifact.absolute_path.to_string_lossy().as_ref(), source_json],
        )?;
        Ok(())
    }

    fn journal_path(&self, id: &str) -> PathBuf {
        self.history_root
            .join("pending-captures")
            .join(format!("{}.json", blake3::hash(id.as_bytes()).to_hex()))
    }

    fn recover_pending(&self) -> Result<(), StorageError> {
        let directory = self.history_root.join("pending-captures");
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let recovered = (|| {
                let pending = read_pending(&path)?;
                if self.journal_path(&pending.id) != path {
                    return Err(StorageError::Invalid(
                        "pending capture record identity is invalid".into(),
                    ));
                }
                validate_capture(&pending.id, &pending.source)?;
                let mut connection = self.connect()?;
                let transaction =
                    connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                self.insert_pending(&transaction, &pending)?;
                transaction.commit()?;
                fs::remove_file(&path)?;
                Ok::<(), StorageError>(())
            })();
            if let Err(error) = recovered {
                log::debug!(
                    "[yue-shot-storage] retained pending capture {}: {error}",
                    path.display()
                );
            }
        }
        Ok(())
    }

    pub fn history_page(
        &self,
        cursor: Option<String>,
        requested_limit: Option<u32>,
    ) -> Result<CaptureHistoryPage, StorageError> {
        let cursor = cursor
            .map(|value| {
                serde_json::from_str::<Cursor>(&value)
                    .map_err(|error| StorageError::Data(error.to_string()))
            })
            .transpose()?;
        let limit = requested_limit
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, created_at_unix_ms, source_json FROM captures
             WHERE (?1 IS NULL OR created_at_unix_ms < ?1 OR (created_at_unix_ms = ?1 AND id < ?2))
             ORDER BY created_at_unix_ms DESC, id DESC LIMIT ?3",
        )?;
        let mut rows = statement.query(params![
            cursor.as_ref().map(|value| value.created_at_unix_ms),
            cursor.as_ref().map(|value| value.id.as_str()).unwrap_or(""),
            i64::from(limit + 1)
        ])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let created_at_unix_ms: i64 = row.get(1)?;
            let source: CaptureSource = serde_json::from_str(&row.get::<_, String>(2)?)
                .map_err(|error| StorageError::Data(error.to_string()))?;
            items.push(CaptureHistoryEntry {
                id,
                created_at_unix_ms,
                source,
            });
        }
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor = if has_more {
            items
                .last()
                .map(|item| {
                    serde_json::to_string(&Cursor {
                        created_at_unix_ms: item.created_at_unix_ms,
                        id: item.id.clone(),
                    })
                    .map_err(|error| StorageError::Data(error.to_string()))
                })
                .transpose()?
        } else {
            None
        };
        Ok(CaptureHistoryPage { items, next_cursor })
    }

    /// Remove a reference and release an owned source when no rows still point to it.
    pub fn delete_capture(&self, id: &str) -> Result<Option<(CaptureSource, bool)>, StorageError> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let source_json: Option<String> = transaction
            .query_row(
                "SELECT source_json FROM captures WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(source_json) = source_json else {
            return Ok(None);
        };
        let source: CaptureSource = serde_json::from_str(&source_json)
            .map_err(|error| StorageError::Data(error.to_string()))?;
        transaction.execute("DELETE FROM captures WHERE id = ?1", [id])?;
        let referenced: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM captures WHERE artifact_path = ?1)",
            [source.artifact.absolute_path.to_string_lossy().as_ref()],
            |row| row.get::<_, i64>(0).map(|value| value != 0),
        )?;
        if !referenced {
            transaction.execute(
                "INSERT OR REPLACE INTO retired_sources(artifact_path, source_json) VALUES (?1,?2)",
                params![
                    source.artifact.absolute_path.to_string_lossy().as_ref(),
                    source_json
                ],
            )?;
        }
        // The durable tombstone and row deletion commit together. Files are only touched after
        // that commit, so a failing SQLite transaction cannot lose an image with a live row.
        transaction.commit()?;
        if !referenced {
            self.recover_retired()?;
        }
        Ok(Some((source, referenced)))
    }
    /// Collect only immutable originals explicitly retired by a committed history deletion.
    /// Unknown/orphan files and user exports are never inferred to be disposable.
    fn recover_retired(&self) -> Result<(), StorageError> {
        let mut connection = self.connect()?;
        loop {
            // Recheck references under the write lock; concurrent archive commits cannot race
            // the verification and removal of an explicitly retired original.
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let next: Option<(String, String)> = transaction
                .query_row(
                    "SELECT artifact_path,source_json FROM retired_sources LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((path, json)) = next else {
                return Ok(());
            };
            let referenced: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM captures WHERE artifact_path=?1)",
                [&path],
                |row| row.get(0),
            )?;
            if !referenced {
                let source: CaptureSource = serde_json::from_str(&json)
                    .map_err(|error| StorageError::Data(error.to_string()))?;
                if let Err(error) =
                    CaptureArtifactStore::new(self.artifact_root()).remove(&source.artifact)
                {
                    // Preserve the tombstone for retry; this never authorizes a wider delete.
                    log::debug!("[yue-shot-storage] retained retired artifact: {error}");
                    return Ok(());
                }
            }
            transaction.execute("DELETE FROM retired_sources WHERE artifact_path=?1", [path])?;
            transaction.commit()?;
        }
    }
}

fn validate_v1_shape(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(captures)")?;
    let columns: Vec<(String, String, i64, i64)> = statement
        .query_map([], |row| {
            Ok((row.get(1)?, row.get(2)?, row.get(3)?, row.get(5)?))
        })?
        .collect::<Result<_, _>>()?;
    let expected = vec![
        ("id".into(), "TEXT".into(), 0, 1),
        ("created_at_unix_ms".into(), "INTEGER".into(), 1, 0),
        ("artifact_path".into(), "TEXT".into(), 1, 0),
        ("source_json".into(), "TEXT".into(), 1, 0),
        ("thumbnail_png".into(), "BLOB".into(), 1, 0),
    ];
    if columns != expected {
        return Err(StorageError::Data(
            "unrecognized capture schema v1; original preserved".into(),
        ));
    }
    Ok(())
}

fn create_current_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS captures (
            id TEXT PRIMARY KEY,
            created_at_unix_ms INTEGER NOT NULL,
            artifact_path TEXT NOT NULL,
            source_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS captures_created_id ON captures(created_at_unix_ms DESC, id DESC);
        CREATE INDEX IF NOT EXISTS captures_artifact_path ON captures(artifact_path);
        CREATE TABLE IF NOT EXISTS retired_sources (artifact_path TEXT PRIMARY KEY, source_json TEXT NOT NULL);",
    )?;
    Ok(())
}

fn migrate_v1(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch("ALTER TABLE captures DROP COLUMN thumbnail_png;")?;
    create_current_schema(&transaction)?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
    transaction.commit()?;

    // The removed thumbnail BLOBs are disposable; reclaim their pages once during this schema change.
    connection.execute_batch("VACUUM")?;
    Ok(())
}

fn validate_capture(id: &str, source: &CaptureSource) -> Result<(), StorageError> {
    use yue_media::ImageDimensions;
    let metadata = &source.metadata;
    if id.is_empty()
        || id.len() > 256
        || metadata.width == 0
        || metadata.height == 0
        || source.artifact.dimensions
            != (ImageDimensions {
                width: metadata.width,
                height: metadata.height,
            })
    {
        return Err(StorageError::Invalid(
            "capture identity or geometry is invalid".into(),
        ));
    }
    if !matches!(
        source.artifact.mime.as_str(),
        "application/x-snapclip-pixels" | "image/png"
    ) {
        return Err(StorageError::Invalid(
            "unsupported capture source format".into(),
        ));
    }
    Ok(())
}

fn read_pending(path: &Path) -> Result<PendingCapture, StorageError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_PENDING_BYTES {
        return Err(StorageError::Invalid(
            "pending capture record exceeds its budget".into(),
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_PENDING_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PENDING_BYTES {
        return Err(StorageError::Invalid(
            "pending capture record exceeds its budget".into(),
        ));
    }
    let pending: PendingCapture =
        serde_json::from_slice(&bytes).map_err(|error| StorageError::Data(error.to_string()))?;
    if pending.version != 1 {
        return Err(StorageError::Invalid(
            "pending capture record version is unsupported".into(),
        ));
    }
    Ok(pending)
}

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use yue_media::{
    ImageDimensions, PixelFormat,
    pixels::{AlphaMode, PixelDescriptor, PixelEncoding, PixelSource},
};
use yue_shot_model::{ArtifactRef, CaptureMetadata, CaptureSource};
use yue_shot_storage::capture_history::CaptureHistoryStore;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "yue-shot-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source(path: PathBuf, session: &str, captured_at_unix_ms: i64) -> CaptureSource {
    let metadata = CaptureMetadata {
        session_id: session.into(),
        width: 8,
        height: 4,
        dpi: 96,
        source: PixelSource::new(
            PixelDescriptor::new(
                PixelFormat::Rgba16Float,
                PixelEncoding::ScRgb,
                AlphaMode::Straight,
            )
            .unwrap(),
            1,
        ),
        captured_at_unix_ms,
        monitor_device_name: None,
    };
    CaptureSource {
        metadata: metadata.clone(),
        artifact: ArtifactRef {
            absolute_path: path,
            mime: "application/x-snapclip-pixels".into(),
            dimensions: ImageDimensions {
                width: metadata.width,
                height: metadata.height,
            },
            byte_len: 1,
            content_fingerprint: "source-fingerprint".into(),
        },
    }
}

#[test]
fn capture_history_is_persistent_sorted_and_paged_without_pixel_cache() {
    let root = TempDir::new();
    let first = source(root.0.join("one.png"), "one", 10);
    let second = source(root.0.join("two.png"), "two", 20);
    let store = CaptureHistoryStore::open(&root.0).unwrap();
    store.save_capture("capture-one", first.clone()).unwrap();
    store.save_capture("capture-two", second.clone()).unwrap();

    let page = store.history_page(None, Some(1)).unwrap();
    assert_eq!(page.items[0].id, "capture-two");
    assert!(page.next_cursor.is_some());
    assert_eq!(page.items[0].source, second);
    let next = store
        .history_page(page.next_cursor.clone(), Some(1))
        .unwrap();
    assert_eq!(next.items.len(), 1);
    assert_eq!(next.items[0].id, "capture-one");
    assert!(next.next_cursor.is_none());

    let reopened = CaptureHistoryStore::open(&root.0).unwrap();
    assert_eq!(reopened.history_page(None, None).unwrap().items.len(), 2);
}

#[test]
fn duplicate_identity_is_idempotent_conflicts_are_rejected_and_shared_source_survives() {
    let root = TempDir::new();
    let first_source = source(root.0.join("same.png"), "same", 42);
    let store = CaptureHistoryStore::open(&root.0).unwrap();
    store
        .save_capture("capture-a", first_source.clone())
        .unwrap();
    let other = source(root.0.join("other.png"), "other", 43);
    assert!(store.save_capture("capture-a", other).is_err());
    store
        .save_capture("capture-b", first_source.clone())
        .unwrap();

    let (deleted, referenced) = store.delete_capture("capture-a").unwrap().unwrap();
    assert_eq!(deleted, first_source);
    assert!(referenced);
    assert!(store.delete_capture("capture-b").unwrap().is_some());
    assert!(store.history_page(None, None).unwrap().items.is_empty());
}

#[test]
fn deleting_shared_rows_releases_the_owned_artifact_only_with_the_last_reference() {
    let root = TempDir::new();
    let metadata = CaptureMetadata {
        session_id: "owned-source".into(),
        width: 8,
        height: 4,
        dpi: 96,
        source: PixelSource::new(PixelDescriptor::bgra8(), 1),
        captured_at_unix_ms: 42,
        monitor_device_name: None,
    };
    let artifact_store = yue_shot_storage::artifact_store::CaptureArtifactStore::new(
        root.0.join("artifacts/capture"),
    );
    let artifact = artifact_store
        .write(&yue_shot_model::CaptureOutput {
            bytes: b"owned artifact bytes".to_vec(),
            metadata: metadata.clone(),
        })
        .unwrap();
    let source = CaptureSource { metadata, artifact };
    let store = CaptureHistoryStore::open(&root.0).unwrap();
    store.save_capture("owned-a", source.clone()).unwrap();
    store.save_capture("owned-b", source.clone()).unwrap();

    let (_, still_referenced) = store.delete_capture("owned-a").unwrap().unwrap();
    assert!(still_referenced);
    assert!(source.artifact.absolute_path.exists());
    let (_, still_referenced) = store.delete_capture("owned-b").unwrap().unwrap();
    assert!(!still_referenced);
    assert!(!source.artifact.absolute_path.exists());
}

#[test]
fn an_invalid_pending_record_is_retained_without_disabling_capture_history() {
    let root = TempDir::new();
    let pending = root.0.join("capture-history/pending-captures");
    std::fs::create_dir_all(&pending).unwrap();
    let record = pending.join("damaged.json");
    std::fs::write(&record, b"not json").unwrap();

    let store = CaptureHistoryStore::open(&root.0).unwrap();
    assert!(
        record.exists(),
        "recovery evidence remains available for inspection"
    );
    assert!(store.history_page(None, None).unwrap().items.is_empty());
}

#[test]
fn future_database_schema_is_rejected_before_journaling_or_recovery_changes() {
    let root = TempDir::new();
    let store = CaptureHistoryStore::open(&root.0).unwrap();
    let path = root.0.join("capture-history/data/yue-shot-history.db");
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", 999).unwrap();
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
    }
    let original = std::fs::read(&path).unwrap();
    assert!(
        CaptureHistoryStore::open(&root.0)
            .unwrap_err()
            .to_string()
            .contains("unsupported")
    );
    assert!(
        store.history_page(None, None).is_err(),
        "an already-open handle must also reject future schemas"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn failed_delete_commit_preserves_the_row_and_its_owned_original() {
    let root = TempDir::new();
    let store = CaptureHistoryStore::open(&root.0).unwrap();
    let bytes = b"owned original";
    let path = store.artifact_root().join("original.pixels");
    std::fs::create_dir_all(store.artifact_root()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    let mut picture = source(path.clone(), "delete-commit", 1);
    picture.artifact.byte_len = bytes.len() as u64;
    picture.artifact.content_fingerprint = blake3::hash(bytes).to_hex().to_string();
    store.save_capture("kept", picture).unwrap();
    let database = root.0.join("capture-history/data/yue-shot-history.db");
    {
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_retire BEFORE INSERT ON retired_sources BEGIN SELECT RAISE(ABORT, 'injected retire failure'); END;").unwrap();
    }
    assert!(store.delete_capture("kept").is_err());
    assert_eq!(store.history_page(None, None).unwrap().items.len(), 1);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "a rolled-back history deletion must not delete its original"
    );
}

#[test]
fn committed_retirement_is_recovered_without_touching_an_external_export() {
    let root = TempDir::new();
    let store = CaptureHistoryStore::open(&root.0).unwrap();
    let bytes = b"retired original";
    let path = store.artifact_root().join("retired.pixels");
    std::fs::create_dir_all(store.artifact_root()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    let external = root.0.join("user-export.png");
    std::fs::write(&external, b"user export").unwrap();
    let mut picture = source(path.clone(), "retired", 1);
    picture.artifact.byte_len = bytes.len() as u64;
    picture.artifact.content_fingerprint = blake3::hash(bytes).to_hex().to_string();
    let database = root.0.join("capture-history/data/yue-shot-history.db");
    {
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .execute(
                "INSERT INTO retired_sources VALUES (?1,?2)",
                rusqlite::params![
                    path.to_string_lossy(),
                    serde_json::to_string(&picture).unwrap()
                ],
            )
            .unwrap();
    }
    drop(store);
    let _reopened = CaptureHistoryStore::open(&root.0).unwrap();
    assert!(!path.exists());
    assert_eq!(std::fs::read(external).unwrap(), b"user export");
}

fn create_v1_database(root: &TempDir, picture: &CaptureSource) -> (PathBuf, u64) {
    let path = root.0.join("capture-history/data/yue-shot-history.db");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut database = rusqlite::Connection::open(&path).unwrap();
    database
        .execute_batch(
            "PRAGMA application_id=1498630228;
        PRAGMA user_version=1;
        CREATE TABLE captures(id TEXT PRIMARY KEY, created_at_unix_ms INTEGER NOT NULL,
            artifact_path TEXT NOT NULL, source_json TEXT NOT NULL, thumbnail_png BLOB NOT NULL);
        CREATE INDEX captures_created_id ON captures(created_at_unix_ms DESC, id DESC);
        CREATE INDEX captures_artifact_path ON captures(artifact_path);
        CREATE TABLE retired_sources(artifact_path TEXT PRIMARY KEY, source_json TEXT NOT NULL);",
        )
        .unwrap();
    let transaction = database.transaction().unwrap();
    for ix in 0..32 {
        transaction
            .execute(
                "INSERT INTO captures VALUES (?1,?2,?3,?4,zeroblob(65536))",
                rusqlite::params![
                    format!("old-{ix:02}"),
                    ix,
                    picture.artifact.absolute_path.to_string_lossy(),
                    serde_json::to_string(picture).unwrap()
                ],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
    drop(database);
    let size = std::fs::metadata(&path).unwrap().len();
    (path, size)
}

#[test]
fn removing_unused_cache_preserves_history_and_originals_and_reclaims_disk() {
    let root = TempDir::new();
    std::fs::create_dir_all(&root.0).unwrap();
    let original = root.0.join("keep-original.pixels");
    std::fs::write(&original, b"unchanged original").unwrap();
    let picture = source(original.clone(), "v1", 42);
    let (path, before) = create_v1_database(&root, &picture);

    let store = CaptureHistoryStore::open(&root.0).unwrap();
    let page = store.history_page(None, Some(100)).unwrap();
    assert_eq!(page.items.len(), 32);
    assert!(page.items.iter().all(|row| row.source == picture));
    assert_eq!(std::fs::read(original).unwrap(), b"unchanged original");
    let database = rusqlite::Connection::open(&path).unwrap();
    let columns: Vec<String> = database
        .prepare("PRAGMA table_info(captures)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        columns,
        ["id", "created_at_unix_ms", "artifact_path", "source_json"]
    );
    assert_eq!(
        database
            .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        2
    );
    drop(database);
    assert!(
        std::fs::metadata(path).unwrap().len() < before / 4,
        "obsolete BLOB pages must be reclaimed"
    );
    assert_eq!(
        CaptureHistoryStore::open(&root.0)
            .unwrap()
            .history_page(None, Some(100))
            .unwrap()
            .items
            .len(),
        32
    );
}

#[test]
fn failed_schema_change_keeps_version_rows_and_source_references() {
    let root = TempDir::new();
    let picture = source(root.0.join("keep.pixels"), "v1-failure", 42);
    let (path, _) = create_v1_database(&root, &picture);
    {
        let database = rusqlite::Connection::open(&path).unwrap();
        // A view depending on the removed column makes SQLite reject the change.
        database
            .execute_batch("CREATE VIEW blocked AS SELECT thumbnail_png FROM captures;")
            .unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    assert!(CaptureHistoryStore::open(&root.0).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let database = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        database
            .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        1
    );
    assert_eq!(
        database
            .query_row("SELECT count(*) FROM captures", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        32
    );
}

#[test]
fn pending_registration_contains_only_metadata_and_recovers_after_failed_commit() {
    let root = TempDir::new();
    let store = CaptureHistoryStore::open(&root.0).unwrap();
    let picture = source(root.0.join("keep.pixels"), "pending", 42);
    let path = root.0.join("capture-history/data/yue-shot-history.db");
    {
        let database = rusqlite::Connection::open(&path).unwrap();
        database.execute_batch("CREATE TRIGGER fail_insert BEFORE INSERT ON captures BEGIN SELECT RAISE(ABORT, 'injected insert failure'); END;").unwrap();
    }
    assert!(store.save_capture("recover", picture.clone()).is_err());
    assert!(store.history_page(None, None).unwrap().items.is_empty());
    let pending_dir = root.0.join("capture-history/pending-captures");
    let journal = std::fs::read_dir(&pending_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(journal).unwrap()).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
    assert!(value.get("thumbnail").is_none());
    {
        let database = rusqlite::Connection::open(path).unwrap();
        database.execute_batch("DROP TRIGGER fail_insert;").unwrap();
    }
    let recovered = CaptureHistoryStore::open(&root.0).unwrap();
    assert_eq!(
        recovered.history_page(None, None).unwrap().items[0].source,
        picture
    );
    assert_eq!(std::fs::read_dir(pending_dir).unwrap().count(), 0);
}

#[test]
fn existing_pending_capture_recovers_source_without_retaining_its_disposable_preview() {
    let root = TempDir::new();
    let picture = source(root.0.join("pending-original.pixels"), "pending-v1", 42);
    let _ = create_v1_database(&root, &picture);
    let directory = root.0.join("capture-history/pending-captures");
    std::fs::create_dir_all(&directory).unwrap();
    let journal = directory.join(format!("{}.json", blake3::hash(b"unfinished").to_hex()));
    let value = serde_json::json!({
        "version": 1,
        "id": "unfinished",
        "source": picture,
        "thumbnail": vec![255_u8; 1024 * 1024]
    });
    std::fs::write(&journal, serde_json::to_vec(&value).unwrap()).unwrap();
    let store = CaptureHistoryStore::open(&root.0).unwrap();
    let rows = store.history_page(None, Some(100)).unwrap();
    assert_eq!(rows.items.len(), 33);
    assert!(
        rows.items
            .iter()
            .any(|row| row.id == "unfinished" && row.source == picture)
    );
    assert!(!journal.exists());
}

//! Installation regression fixture, using the real storage API without capture/UI.
use std::path::PathBuf;
use yue_media::pixels::{PixelDescriptor, PixelSource};
use yue_shot_model::{CaptureMetadata, CaptureOutput, CaptureSource};
use yue_shot_storage::{
    artifact_store::CaptureArtifactStore, capture_history::CaptureHistoryStore,
};

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 3, "seed|verify ABSOLUTE_ROOT ONE_PIXEL_PNG");
    let root = PathBuf::from(&args[1]);
    assert!(root.is_absolute());
    let png = std::fs::read(&args[2]).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let history = CaptureHistoryStore::open(&root).unwrap();
    let artifacts = CaptureArtifactStore::new(history.artifact_root());
    if args[0] == "seed" {
        assert!(history.history_page(None, None).unwrap().items.is_empty());
        let metadata = CaptureMetadata {
            session_id: "lifecycle-regression".into(),
            width: 1,
            height: 1,
            dpi: 96,
            source: PixelSource::new(PixelDescriptor::bgra8(), 1),
            captured_at_unix_ms: 1_700_000_000_000,
            monitor_device_name: None,
        };
        let artifact = artifacts
            .write(&CaptureOutput {
                bytes: png.clone(),
                metadata: metadata.clone(),
            })
            .unwrap();
        history
            .save_capture("lifecycle-regression", CaptureSource { metadata, artifact })
            .unwrap();
    } else {
        assert_eq!(args[0], "verify");
    }
    let page = history.history_page(None, None).unwrap();
    assert_eq!(page.items.len(), 1);
    let entry = &page.items[0];
    assert_eq!(entry.id, "lifecycle-regression");
    artifacts.verify_owned(&entry.source.artifact).unwrap();
    artifacts.verify(&entry.source.artifact).unwrap();
    assert_eq!(
        std::fs::read(&entry.source.artifact.absolute_path).unwrap(),
        png
    );
    assert_eq!(entry.source.metadata.width, 1);
    println!("LIFECYCLE_HISTORY_FIXTURE=PASS (real row, owned PNG, fingerprint, original bytes)");
}

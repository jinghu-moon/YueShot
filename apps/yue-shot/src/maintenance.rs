//! Offline, locked data backup/restore. No screenshot, registry, UI or network services.
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
const MANIFEST: &str = "yueshot-backup.json";
const RESTORING: &str = ".restore-in-progress.json";
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    root: PathBuf,
    files: Vec<Entry>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: PathBuf,
    bytes: u64,
    hash: String,
}
fn ordinary(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt as _;
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_attributes() & 0x400 != 0 {
        return Err("backup/restore refuses symlinks and reparse points".into());
    }
    Ok(())
}
fn digest(path: &Path) -> Result<(u64, String), String> {
    ordinary(path)?;
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = blake3::Hasher::new();
    let mut total = 0;
    let mut scratch = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut scratch).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&scratch[..n]);
        total += n as u64;
    }
    Ok((total, hash.finalize().to_hex().to_string()))
}
fn copy_new(from: &Path, to: &Path) -> Result<(), String> {
    ordinary(from)?;
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut input = fs::File::open(from).map_err(|e| e.to_string())?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())
}
fn external(root: &Path, destination: &Path) -> Result<PathBuf, String> {
    if !destination.is_absolute() {
        return Err("backup path must be absolute".into());
    }
    // Parent must exist: normalization must not hide traversal or junction escapes.
    let parent = destination
        .parent()
        .ok_or("backup path has no parent")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let result = parent.join(destination.file_name().ok_or("backup path has no name")?);
    if result.starts_with(root) || root.starts_with(&result) {
        return Err("backup must be outside the product data root".into());
    }
    Ok(result)
}
fn inventory(root: &Path, directory: &Path, files: &mut Vec<Entry>) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        ordinary(&path)?;
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        if relative.components().count() == 1
            && matches!(relative.to_str(), Some(".yueshot.lock" | "logs" | "cache"))
        {
            continue;
        }
        if path.is_dir() {
            inventory(root, &path, files)?;
        } else {
            if files.len() >= 100_000 {
                return Err("backup file count exceeds 100000".into());
            }
            let (bytes, hash) = digest(&path)?;
            files.push(Entry {
                path: relative.into(),
                bytes,
                hash,
            });
        }
    }
    Ok(())
}
pub(super) fn ensure_ready(root: &Path) -> Result<(), String> {
    if root.join(RESTORING).exists() {
        return Err("data restore is incomplete; resume --restore-from with the same backup before starting YueShot".into());
    }
    Ok(())
}
pub(super) fn backup(root: &Path, destination: &Path) -> Result<(), String> {
    ensure_ready(root)?;
    let destination = external(root, destination)?;
    fs::create_dir(&destination).map_err(|e| format!("create a new backup directory: {e}"))?;
    let mut files = Vec::new();
    inventory(root, root, &mut files)?;
    for entry in &files {
        let target = destination.join("files").join(&entry.path);
        copy_new(&root.join(&entry.path), &target)?;
        if digest(&target)? != (entry.bytes, entry.hash.clone()) {
            return Err("backup verification failed".into());
        }
    }
    let manifest = Manifest {
        version: 1,
        root: root.into(),
        files,
    };
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination.join(MANIFEST))
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub(super) fn restore(root: &Path, backup: &Path) -> Result<(), String> {
    let backup = backup.canonicalize().map_err(|e| e.to_string())?;
    external(root, &backup)?;
    ordinary(&backup)?;
    ordinary(&backup.join(MANIFEST))?;
    if fs::metadata(backup.join(MANIFEST))
        .map_err(|e| e.to_string())?
        .len()
        > 64 * 1024 * 1024
    {
        return Err("backup manifest is too large".into());
    }
    let bytes = fs::read(backup.join(MANIFEST)).map_err(|e| e.to_string())?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if manifest.version != 1 || manifest.root != root || manifest.files.len() > 100_000 {
        return Err("backup identity/version does not match this data root".into());
    }
    let mut paths = std::collections::HashSet::new();
    for entry in &manifest.files {
        if entry.path.as_os_str().is_empty()
            || !entry.path.components().all(|part| match part {
                Component::Normal(name) => name.to_str().is_some_and(|name| {
                    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
                    !name.contains(':')
                        && !name.ends_with(['.', ' '])
                        && !matches!(
                            base.as_str(),
                            "CON"
                                | "PRN"
                                | "AUX"
                                | "NUL"
                                | "COM1"
                                | "COM2"
                                | "COM3"
                                | "COM4"
                                | "COM5"
                                | "COM6"
                                | "COM7"
                                | "COM8"
                                | "COM9"
                                | "LPT1"
                                | "LPT2"
                                | "LPT3"
                                | "LPT4"
                                | "LPT5"
                                | "LPT6"
                                | "LPT7"
                                | "LPT8"
                                | "LPT9"
                        )
                }),
                _ => false,
            })
            || !paths.insert(entry.path.to_string_lossy().to_lowercase())
        {
            return Err("invalid or duplicate backup-relative path".into());
        }
        if entry.path.components().next().is_some_and(|part| {
            part.as_os_str().to_str().is_some_and(|name| {
                [".yueshot.lock", RESTORING, ".restore-work"]
                    .iter()
                    .any(|reserved| name.eq_ignore_ascii_case(reserved))
            })
        }) {
            return Err("backup contains a reserved path".into());
        }
        let source = backup.join("files").join(&entry.path);
        // Check every ancestor as well as the final file before reading.
        let mut ancestor = source.as_path();
        while ancestor != backup {
            ordinary(ancestor)?;
            ancestor = ancestor.parent().ok_or("invalid backup ancestry")?;
        }
        if digest(&source)? != (entry.bytes, entry.hash.clone()) {
            return Err("backup file checksum mismatch; original data untouched".into());
        }
    }
    let marker = root.join(RESTORING);
    let identity = blake3::hash(&bytes).to_hex().to_string();
    if marker.exists() {
        if fs::read_to_string(&marker).map_err(|e| e.to_string())? != identity {
            return Err("another restore is incomplete; use its original backup".into());
        }
    } else {
        for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_name().eq_ignore_ascii_case(".yueshot.lock") {
                return Err(
                    "restore requires an empty data root; move the existing data aside first"
                        .into(),
                );
            }
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
            .map_err(|e| e.to_string())?;
        file.write_all(identity.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
    }
    for entry in &manifest.files {
        let target = root.join(&entry.path);
        let mut ancestor = target.parent().ok_or("invalid destination")?;
        while ancestor != root {
            if ancestor.exists() {
                ordinary(ancestor)?;
            }
            ancestor = ancestor.parent().ok_or("invalid destination ancestry")?;
        }
        if target.exists() {
            if digest(&target)? != (entry.bytes, entry.hash.clone()) {
                return Err("restore destination changed; refusing to overwrite".into());
            }
            continue;
        }
        // A partial copy never gets the destination's final name. On retry, only the task's
        // own staged partial file is discarded and recopied from the verified backup.
        let temporary = root.join(".restore-work");
        if temporary.exists() {
            ordinary(&temporary)?;
            fs::remove_file(&temporary).map_err(|e| e.to_string())?;
        }
        copy_new(&backup.join("files").join(&entry.path), &temporary)?;
        if digest(&temporary)? != (entry.bytes, entry.hash.clone()) {
            return Err("restore verification failed".into());
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::rename(temporary, target).map_err(|e| e.to_string())?;
    }
    fs::remove_file(marker).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{backup, ensure_ready, restore};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yueshot-maintenance-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn backup_verifies_bytes_and_restore_refuses_existing_data_then_recovers_exactly() {
        let temp = Temp::new();
        let root = temp.0.join("data");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(root.join("settings.json"), b"configuration").unwrap();
        fs::create_dir(root.join("artifacts")).unwrap();
        fs::write(root.join("artifacts/source.bin"), b"original pixels").unwrap();
        fs::write(root.join(".yueshot.lock"), b"").unwrap();
        let snapshot = temp.0.join("backup");
        backup(&root, &snapshot).unwrap();
        assert!(backup(&root, &snapshot).is_err());
        assert!(restore(&root, &snapshot).is_err());
        fs::rename(root.join("settings.json"), temp.0.join("old-settings.json")).unwrap();
        fs::rename(root.join("artifacts"), temp.0.join("old-artifacts")).unwrap();
        restore(&root, &snapshot).unwrap();
        assert_eq!(
            fs::read(root.join("settings.json")).unwrap(),
            b"configuration"
        );
        assert_eq!(
            fs::read(root.join("artifacts/source.bin")).unwrap(),
            b"original pixels"
        );
        ensure_ready(&root).unwrap();
    }
    #[test]
    fn corruption_and_path_traversal_fail_before_touching_destination() {
        let temp = Temp::new();
        let root = temp.0.join("data");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(root.join("settings.json"), b"original").unwrap();
        let snapshot = temp.0.join("backup");
        backup(&root, &snapshot).unwrap();
        fs::remove_file(root.join("settings.json")).unwrap();
        fs::write(snapshot.join("files/settings.json"), b"tampered").unwrap();
        assert!(restore(&root, &snapshot).is_err());
        assert!(!root.join(".restore-in-progress.json").exists());
        assert!(backup(&root, &root.join("nested")).is_err());
        let manifest = snapshot.join("yueshot-backup.json");
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        json["files"][0]["path"] = "../escape".into();
        fs::write(&manifest, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(restore(&root, &snapshot).is_err());
        assert!(!temp.0.join("escape").exists());
        for path in [
            ".YUESHOT.LOCK",
            ".RESTORE-WORK",
            "settings.json:stream",
            "CON.txt",
            "dir./file",
        ] {
            json["files"][0]["path"] = path.into();
            fs::write(&manifest, serde_json::to_vec(&json).unwrap()).unwrap();
            let error = restore(&root, &snapshot).unwrap_err();
            assert!(error.contains("path"), "{path}: {error}");
            assert!(!root.join(".restore-in-progress.json").exists());
        }
    }
    #[test]
    fn interrupted_restore_is_blocked_from_startup_and_resumes_without_overwriting() {
        let temp = Temp::new();
        let root = temp.0.join("data");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(root.join("settings.json"), b"configuration").unwrap();
        let snapshot = temp.0.join("backup");
        backup(&root, &snapshot).unwrap();
        fs::remove_file(root.join("settings.json")).unwrap();
        let manifest = fs::read(snapshot.join("yueshot-backup.json")).unwrap();
        fs::write(
            root.join(".restore-in-progress.json"),
            blake3::hash(&manifest).to_hex().as_bytes(),
        )
        .unwrap();
        fs::write(root.join(".restore-work"), b"interrupted partial").unwrap();
        assert!(ensure_ready(&root).is_err());
        restore(&root, &snapshot).unwrap();
        ensure_ready(&root).unwrap();
        assert_eq!(
            fs::read(root.join("settings.json")).unwrap(),
            b"configuration"
        );
    }
}

//! Explicit offline export. Never copy raw/debug logs, configuration or image data.
use std::{
    collections::VecDeque,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};

pub(super) const BUILD_ID: Option<&str> = option_env!("YUESHOT_BUILD_ID");
const FILE_LIMIT: u64 = 4 * 1024 * 1024;
const LINE_LIMIT: usize = 8 * 1024;
const RECORD_LIMIT: usize = 8192;
const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Collection {
    records: VecDeque<serde_json::Value>,
    malformed_records: u64,
    oversized_records: u64,
    incomplete_records: u64,
    discarded_old_records: u64,
    present_log_files: u32,
    panic: Option<serde_json::Value>,
}

fn hex_id(value: &serde_json::Value) -> Option<&str> {
    value.as_str().filter(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn choice(value: &serde_json::Value, choices: &'static [&'static str]) -> Option<&'static str> {
    choices.iter().copied().find(|s| value.as_str() == Some(s))
}
fn version(value: &serde_json::Value) -> Option<[u32; 3]> {
    let mut parts = value.as_str()?.split('.');
    let result = [
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ];
    parts.next().is_none().then_some(result)
}
fn metadata(value: &serde_json::Value) -> Option<serde_json::Value> {
    if value["product"] != "YueShot" {
        return None;
    }
    Some(serde_json::json!({
        "version": version(&value["version"]),
        "channel": choice(&value["channel"], &["dev", "beta", "stable"]),
        "buildId": hex_id(&value["buildId"]),
        "timestampMs": value["timestampMs"].as_u64(),
        "line": value["line"].as_u64().and_then(|n| u32::try_from(n).ok()),
    }))
}
fn record(bytes: &[u8]) -> Option<serde_json::Value> {
    let input: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let mut output = metadata(&input)?;
    output["level"] = choice(
        &input["level"],
        &["ERROR", "WARN", "INFO", "DEBUG", "TRACE"],
    )?
    .into();
    output["stage"] = choice(
        &input["stage"],
        &["yueshot.event", "rust.panic", "product.operation_failed"],
    )
    .unwrap_or("other")
    .into();
    // Only fixed, known event names survive; even debug messages and source/module names are omitted.
    output["event"] = choice(
        &input["message"],
        &[
            "startup.logging_ready",
            "shutdown.begin",
            "shutdown.services_stopped",
        ],
    )
    .into();
    Some(output)
}

impl Collection {
    fn scan(&mut self, mut reader: impl BufRead) -> std::io::Result<()> {
        let mut line = Vec::with_capacity(LINE_LIMIT);
        let mut oversized = false;
        loop {
            let buffer = reader.fill_buf()?;
            if buffer.is_empty() {
                if !line.is_empty() || oversized {
                    self.incomplete_records += 1;
                }
                return Ok(());
            }
            let end = buffer.iter().position(|&b| b == b'\n');
            let count = end.map_or(buffer.len(), |n| n + 1);
            if !oversized && line.len() + count <= LINE_LIMIT {
                line.extend_from_slice(&buffer[..count]);
            } else {
                oversized = true;
                line.clear();
            }
            reader.consume(count);
            if end.is_some() {
                if oversized {
                    self.oversized_records += 1;
                } else if let Some(record) = record(&line) {
                    if self.records.len() == RECORD_LIMIT {
                        self.records.pop_front();
                        self.discarded_old_records += 1;
                    }
                    self.records.push_back(record);
                } else {
                    self.malformed_records += 1;
                }
                line.clear();
                oversized = false;
            }
        }
    }
}

fn ordinary(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt as _;
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_attributes() & 0x400 != 0 {
        return Err("diagnostics refuses reparse points".into());
    }
    Ok(())
}
fn input(path: &Path, limit: u64) -> Result<Option<std::io::Take<fs::File>>, String> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("open diagnostics input: {e}")),
    };
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if metadata.file_attributes() & 0x400 != 0 || !metadata.is_file() {
        return Err("diagnostics input must be an ordinary file".into());
    }
    if metadata.len() > limit {
        return Err("diagnostics input exceeds its size budget".into());
    }
    Ok(Some(file.take(limit)))
}

fn destination(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("invalid diagnostics filename")?;
    // A restricted JSON filename also excludes NTFS streams, devices and trailing-dot aliases.
    if !path.is_absolute()
        || !name.ends_with(".json")
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || matches!(
            name.split('.')
                .next()
                .unwrap_or("")
                .to_ascii_uppercase()
                .as_str(),
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
    {
        return Err("diagnostics requires an absolute path with a plain .json filename".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let parent = path
        .parent()
        .ok_or("missing diagnostics parent")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    // Windows paths are case insensitive; compare canonical components with case folding.
    let root_components: Vec<_> = root
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let parent_components: Vec<_> = parent
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    if parent_components.starts_with(&root_components) {
        return Err("diagnostics destination must be outside the product data root".into());
    }
    Ok(parent.join(name))
}

pub(super) fn export(root: &Path, path: &Path, channel: &str) -> Result<(), String> {
    let path = destination(root, path)?;
    let mut collection = Collection::default();
    let logs = root.join("logs");
    match fs::symlink_metadata(&logs) {
        Ok(_) => {
            ordinary(&logs)?;
            for name in [
                "yueshot.3.jsonl",
                "yueshot.2.jsonl",
                "yueshot.1.jsonl",
                "yueshot.jsonl",
            ] {
                if let Some(file) = input(&logs.join(name), FILE_LIMIT)? {
                    collection
                        .scan(BufReader::new(file))
                        .map_err(|e| e.to_string())?;
                    collection.present_log_files += 1;
                }
            }
            if let Some(mut file) = input(&logs.join("last-panic.json"), LINE_LIMIT as u64)? {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                let panic = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
                collection.panic = panic.and_then(|value| {
                    let mut result = metadata(&value)?;
                    if value["stage"] != "rust.panic" {
                        return None;
                    }
                    result["stage"] = "rust.panic".into();
                    result["siteHash"] = hex_id(&value["siteHash"]).into();
                    Some(result)
                });
                if collection.panic.is_none() {
                    collection.malformed_records += 1;
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({
        "format": 1, "product": "YueShot", "exporterVersion": env!("CARGO_PKG_VERSION"),
        "exporterBuildId": BUILD_ID, "channel": channel,
        "policy": "metadata-only; no raw messages, source paths, settings, images, clipboard, dumps or upload",
        "nativeCrashCollection": "disabled", "collection": collection,
    })).map_err(|e| e.to_string())?;
    if bytes.len() > OUTPUT_LIMIT {
        return Err("diagnostics output exceeds its size budget".into());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("create new diagnostics JSON: {e}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> serde_json::Value {
        serde_json::json!({"product":"YueShot", "version":"0.1.1", "channel":"dev", "level":"INFO", "stage":"yueshot.event", "timestampMs":42})
    }
    #[test]
    fn export_drops_arbitrary_strings_even_in_metadata_and_debug_records() {
        let mut value = valid();
        for key in [
            "version",
            "channel",
            "stage",
            "source",
            "message",
            "buildId",
            "line",
            "timestampMs",
        ] {
            value[key] = "C:\\secret\\clipboard.png 正文".into();
        }
        let cleaned = record(&serde_json::to_vec(&value).unwrap()).unwrap();
        let text = cleaned.to_string();
        assert!(!text.contains("secret") && !text.contains("正文"));
        assert_eq!(cleaned["stage"], "other");
        assert!(cleaned["buildId"].is_null());
        assert!(cleaned["timestampMs"].is_null());
        value["level"] = "secret".into();
        assert!(record(&serde_json::to_vec(&value).unwrap()).is_none());
    }
    #[test]
    fn bounded_scan_recovers_after_corrupt_oversized_and_incomplete_lines() {
        let mut bytes = vec![b'x'; LINE_LIMIT * 3];
        bytes.extend_from_slice(b"\ninvalid\n");
        bytes.extend(serde_json::to_vec(&valid()).unwrap());
        bytes.push(b'\n');
        bytes.extend_from_slice(b"{truncated");
        let mut collected = Collection::default();
        collected
            .scan(BufReader::with_capacity(31, bytes.as_slice()))
            .unwrap();
        assert_eq!(collected.records.len(), 1);
        assert_eq!(collected.oversized_records, 1);
        assert_eq!(collected.malformed_records, 1);
        assert_eq!(collected.incomplete_records, 1);
    }
    #[test]
    fn record_budget_keeps_the_newest_records() {
        let mut bytes = Vec::new();
        for timestamp in 0..RECORD_LIMIT + 2 {
            let mut value = valid();
            value["timestampMs"] = timestamp.into();
            bytes.extend(serde_json::to_vec(&value).unwrap());
            bytes.push(b'\n');
        }
        let mut collected = Collection::default();
        collected.scan(bytes.as_slice()).unwrap();
        assert_eq!(collected.records.len(), RECORD_LIMIT);
        assert_eq!(collected.discarded_old_records, 2);
        assert_eq!(collected.records.front().unwrap()["timestampMs"], 2);
    }

    #[test]
    fn maximum_metadata_fits_the_export_budget() {
        let mut value = valid();
        value["timestampMs"] = u64::MAX.into();
        value["line"] = u32::MAX.into();
        value["version"] = "4294967295.4294967295.4294967295".into();
        value["buildId"] = "f".repeat(64).into();
        value["message"] = "shutdown.services_stopped".into();
        value["stage"] = "product.operation_failed".into();
        value["channel"] = "stable".into();
        value["level"] = "ERROR".into();
        let record = record(&serde_json::to_vec(&value).unwrap()).unwrap();
        let collection = Collection {
            records: std::iter::repeat_n(record, RECORD_LIMIT).collect(),
            ..Default::default()
        };
        // Leave room for the exporter envelope and a panic marker too.
        assert!(serde_json::to_vec_pretty(&collection).unwrap().len() + LINE_LIMIT < OUTPUT_LIMIT);
    }
}

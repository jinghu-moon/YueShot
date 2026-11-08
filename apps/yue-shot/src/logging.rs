//! Bounded local diagnostics. Normal logs omit message bodies, paths and image/clipboard data.
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{OnceLock, mpsc},
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};
const LIMIT: u64 = 4 * 1024 * 1024;
const BACKUPS: usize = 3;
struct Logger {
    sender: mpsc::SyncSender<Option<Vec<u8>>>,
    debug: bool,
    channel: &'static str,
}
static LOGGER: OnceLock<Logger> = OnceLock::new();
impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level()
            <= if self.debug {
                log::Level::Debug
            } else {
                log::Level::Info
            }
    }
    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let message =
            (self.debug || record.target() == "yueshot.event").then(|| record.args().to_string());
        let value = serde_json::json!({"timestampMs":timestamp,"product":"YueShot","version":env!("CARGO_PKG_VERSION"),"buildId":super::diagnostics::BUILD_ID,"channel":self.channel,"level":record.level().as_str(),"stage":record.target(),"source":record.module_path(),"line":record.line(),"message":message});
        if let Ok(mut bytes) = serde_json::to_vec(&value) {
            bytes.push(b'\n');
            let _ = self.sender.try_send(Some(bytes));
        }
    }
    fn flush(&self) {}
}
struct Rolling {
    directory: PathBuf,
    file: Option<fs::File>,
    bytes: u64,
}
impl Rolling {
    fn new(directory: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&directory)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("yueshot.jsonl"))?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            directory,
            file: Some(file),
            bytes,
        })
    }
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if bytes.len() as u64 > LIMIT {
            return Ok(());
        }
        if self.bytes + bytes.len() as u64 > LIMIT {
            if let Some(file) = self.file.take() {
                file.sync_all()?;
                drop(file);
            }
            let oldest = self.directory.join(format!("yueshot.{BACKUPS}.jsonl"));
            match fs::remove_file(oldest) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            for index in (1..BACKUPS).rev() {
                let from = self.directory.join(format!("yueshot.{index}.jsonl"));
                if from.exists() {
                    fs::rename(
                        from,
                        self.directory.join(format!("yueshot.{}.jsonl", index + 1)),
                    )?;
                }
            }
            fs::rename(
                self.directory.join("yueshot.jsonl"),
                self.directory.join("yueshot.1.jsonl"),
            )?;
            self.file = Some(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.directory.join("yueshot.jsonl"))?,
            );
            self.bytes = 0;
        }
        if self.file.is_none() {
            let file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.directory.join("yueshot.jsonl"))?;
            self.bytes = file.metadata()?.len();
            self.file = Some(file);
        }
        self.file
            .as_mut()
            .expect("opened log file")
            .write_all(bytes)?;
        self.bytes += bytes.len() as u64;
        Ok(())
    }
}
pub(super) struct Guard {
    sender: mpsc::SyncSender<Option<Vec<u8>>>,
    worker: Option<JoinHandle<()>>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.sender.send(None);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub(super) fn init(root: &std::path::Path, channel: &'static str) -> Result<Guard, String> {
    let mut rolling =
        Rolling::new(root.join("logs")).map_err(|e| format!("initialize diagnostics: {e}"))?;
    let debug = std::env::var("YUESHOT_LOG").is_ok_and(|value| value == "debug");
    let (sender, receiver) = mpsc::sync_channel(256);
    let logger = LOGGER.get_or_init(|| Logger {
        sender: sender.clone(),
        debug,
        channel,
    });
    log::set_logger(logger).map_err(|e| e.to_string())?;
    log::set_max_level(if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    });
    let worker = thread::Builder::new()
        .name("yueshot-log".into())
        .spawn(move || {
            while let Ok(Some(bytes)) = receiver.recv() {
                if rolling.write(&bytes).is_err() {
                    break;
                }
            }
            if let Some(file) = rolling.file {
                let _ = file.sync_all();
            }
        })
        .map_err(|e| e.to_string())?;
    let panic_directory = root.join("logs");
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        if let Some(location) = panic.location() {
            log::error!(target:"rust.panic","panic at {}:{}",location.file(),location.line());
        } else {
            log::error!(target:"rust.panic","panic without source location");
        }
        // A release build aborts after this hook, so the crash marker must be synchronous.
        let _ = write_panic_marker(
            &panic_directory,
            channel,
            panic.location().map(|v| (v.file(), v.line())),
        );
        previous(panic);
    }));
    log::info!(target:"yueshot.event","startup.logging_ready");
    Ok(Guard {
        sender,
        worker: Some(worker),
    })
}
fn write_panic_marker(
    directory: &std::path::Path,
    channel: &str,
    site: Option<(&str, u32)>,
) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "product":"YueShot", "version":env!("CARGO_PKG_VERSION"),
        "channel":channel, "stage":"rust.panic", "line":site.map(|v| v.1),
        "timestampMs":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "buildId":super::diagnostics::BUILD_ID,
        "siteHash":site.map(|v| blake3::hash(v.0.as_bytes()).to_hex().to_string())
    }))?;
    let mut file = fs::File::create(directory.join("last-panic.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()
}
#[cfg(test)]
mod tests {
    use super::{BACKUPS, LIMIT, Logger, Rolling};
    use log::Log as _;
    #[test]
    fn rotation_has_a_fixed_disk_budget() {
        let root = std::env::temp_dir().join(format!("yueshot-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut rolling = Rolling::new(root.clone()).unwrap();
        let bytes = vec![b'x'; LIMIT as usize / 2];
        for _ in 0..12 {
            rolling.write(&bytes).unwrap();
        }
        drop(rolling);
        super::write_panic_marker(&root, "dev", Some((r"C:\private\source.rs", 42))).unwrap();
        let panic: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("last-panic.json")).unwrap()).unwrap();
        assert_eq!(panic["line"], 42);
        assert!(panic.get("message").is_none());
        assert_eq!(
            panic["siteHash"],
            blake3::hash(br"C:\private\source.rs").to_hex().as_str()
        );
        assert!(!panic.to_string().contains("private"));
        let files: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|value| value == "jsonl"))
            .collect();
        assert_eq!(files.len(), BACKUPS + 1);
        for path in files {
            assert!(std::fs::metadata(path).unwrap().len() <= LIMIT);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn normal_log_records_omit_sensitive_messages_and_the_queue_is_bounded() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let logger = Logger {
            sender,
            debug: false,
            channel: "dev",
        };
        let record = log::Record::builder()
            .args(format_args!("secret clipboard at C:\\private\\capture.png"))
            .level(log::Level::Error)
            .target("capture.failed")
            .build();
        logger.log(&record);
        logger.log(&record);
        let bytes = receiver.try_recv().unwrap().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["message"].is_null());
        assert_eq!(json["stage"], "capture.failed");
        assert!(!String::from_utf8(bytes).unwrap().contains("secret"));
        assert!(receiver.try_recv().is_err());
    }
}

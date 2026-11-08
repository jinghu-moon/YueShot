//! Exercise the built product entry without initializing a desktop, GPU or global hotkeys.
#![cfg(windows)]
use std::{
    fs::OpenOptions,
    os::windows::fs::OpenOptionsExt,
    path::PathBuf,
    process::{Command, Output},
};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new(label: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
            "yueshot-cli-{}-{label}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        )))
    }
    fn run(&self, command: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_YueShot"))
            .arg(command)
            .arg("--data-dir")
            .arg(&self.0)
            .output()
            .unwrap()
    }
    fn lock(&self) -> std::fs::File {
        std::fs::create_dir_all(&self.0).unwrap();
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .share_mode(3)
            .open(self.0.join(".yueshot.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        lock
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn smoke_refuses_an_active_writer_before_opening_configuration_or_storage() {
    let sandbox = Sandbox::new("smoke-lock");
    let lock = sandbox.lock();
    let output = sandbox.run("--smoke-test");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("active writer"));
    assert_eq!(
        std::fs::read_dir(&sandbox.0).unwrap().count(),
        1,
        "only the lock file may exist"
    );
    drop(lock);
    let output = sandbox.run("--smoke-test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("no UI/GPU/hotkeys"));
}

#[test]
fn missing_instance_endpoint_fails_without_starting_another_writer() {
    let sandbox = Sandbox::new("no-endpoint");
    let _lock = sandbox.lock();
    let output = sandbox.run("--background");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("不启动第二个写入者"));
    assert_eq!(std::fs::read_dir(&sandbox.0).unwrap().count(), 1);
}

#[test]
fn help_and_version_do_not_create_or_lock_a_data_directory() {
    let sandbox = Sandbox::new("help");
    for command in ["--help", "--version", "--product-info"] {
        assert!(sandbox.run(command).status.success());
        assert!(!sandbox.0.exists());
    }
    let _lock = sandbox.lock();
    for command in ["--help", "--version", "--product-info"] {
        assert!(sandbox.run(command).status.success());
    }
}

#[test]
fn product_info_is_available_without_localappdata_or_data_side_effects() {
    let output = Command::new(env!("CARGO_BIN_EXE_YueShot"))
        .arg("--product-info")
        .env_remove("LOCALAPPDATA")
        .output()
        .unwrap();
    assert!(output.status.success());
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(info["product"], "YueShot");
    let channel = info["channel"].as_str().unwrap();
    assert_eq!(
        info["installationId"],
        format!("com.seeyuer.yueshot.{channel}.x64")
    );
    assert_eq!(info["dataRelative"], format!("Yue/YueShot/{channel}"));
}

#[test]
fn offline_maintenance_refuses_a_writer_and_restores_the_verified_database() {
    let sandbox = Sandbox::new("maintenance");
    let snapshot = sandbox.0.with_extension("backup");
    let previous = sandbox.0.with_extension("previous");
    let invoke = |command: &str| {
        Command::new(env!("CARGO_BIN_EXE_YueShot"))
            .args([command])
            .arg(&snapshot)
            .arg("--data-dir")
            .arg(&sandbox.0)
            .output()
            .unwrap()
    };
    let lock = sandbox.lock();
    for command in ["--backup-to", "--restore-from"] {
        let output = invoke(command);
        assert!(!output.status.success());
        assert!(!snapshot.exists());
        assert_eq!(std::fs::read_dir(&sandbox.0).unwrap().count(), 1);
    }
    drop(lock);
    assert!(sandbox.run("--smoke-test").status.success());
    std::fs::write(sandbox.0.join("maintenance-fixture.bin"), b"owned data").unwrap();
    let output = invoke("--backup-to");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!invoke("--restore-from").status.success());
    std::fs::rename(&sandbox.0, &previous).unwrap();
    let output = invoke("--restore-from");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(sandbox.0.join("maintenance-fixture.bin")).unwrap(),
        b"owned data"
    );
    assert!(sandbox.run("--smoke-test").status.success());
    std::fs::remove_dir_all(snapshot).unwrap();
    std::fs::remove_dir_all(previous).unwrap();
}

#[test]
fn diagnostics_are_offline_locked_redacted_and_never_overwrite() {
    let sandbox = Sandbox::new("diagnostics");
    let destination = sandbox.0.with_extension("json");
    let invoke = |path: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_YueShot"))
            .arg("--export-diagnostics-to")
            .arg(path)
            .arg("--data-dir")
            .arg(&sandbox.0)
            .output()
            .unwrap()
    };
    let lock = sandbox.lock();
    assert!(!invoke(&destination).status.success());
    assert!(!destination.exists());
    assert_eq!(std::fs::read_dir(&sandbox.0).unwrap().count(), 1);
    drop(lock);
    let logs = sandbox.0.join("logs");
    std::fs::create_dir(&logs).unwrap();
    let record = serde_json::json!({"product":"YueShot", "version":"0.1.1", "channel":"dev", "level":"ERROR", "stage":"rust.panic", "timestampMs":42, "message":"secret clipboard C:\\private\\image.png", "source":"private source path"});
    std::fs::write(logs.join("yueshot.jsonl"), format!("{record}\ninvalid\n")).unwrap();
    let build_id = "a".repeat(64);
    let panic = serde_json::json!({"product":"YueShot", "version":"0.1.1", "channel":"dev", "stage":"rust.panic", "line":42, "buildId":build_id, "siteHash":"b".repeat(64), "message":"secret"});
    std::fs::write(logs.join("last-panic.json"), panic.to_string()).unwrap();
    std::fs::write(sandbox.0.join("settings.json"), "secret").unwrap();
    std::fs::write(sandbox.0.join("image.png"), "secret").unwrap();
    assert!(!invoke(&sandbox.0.join("inside.json")).status.success());
    let output = invoke(&destination);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let original = std::fs::read(&destination).unwrap();
    let text = String::from_utf8(original.clone()).unwrap();
    assert!(!text.contains("secret") && !text.contains("private") && !text.contains("image.png"));
    let report: serde_json::Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(report["collection"]["records"].as_array().unwrap().len(), 1);
    assert_eq!(report["collection"]["malformedRecords"], 1);
    assert_eq!(report["collection"]["panic"]["buildId"], build_id);
    assert_eq!(report["collection"]["panic"]["line"], 42);
    assert!(!sandbox.0.join("yueshot.sqlite3").exists());
    assert_eq!(std::fs::read_dir(&sandbox.0).unwrap().count(), 4);
    assert!(!invoke(&destination).status.success());
    assert_eq!(std::fs::read(&destination).unwrap(), original);
    std::fs::remove_file(destination).unwrap();
    // Oversize inputs are rejected before an output file is created.
    let file = OpenOptions::new()
        .write(true)
        .open(logs.join("yueshot.jsonl"))
        .unwrap();
    file.set_len(4 * 1024 * 1024 + 1).unwrap();
    drop(file);
    assert!(!invoke(&sandbox.0.with_extension("json")).status.success());
    assert!(!sandbox.0.with_extension("json").exists());
}

use super::*;
use base64::Engine as _;
use blake2::Digest as _;
use ring::signature::KeyPair as _;
use std::time::Instant;

const CHILD_ROOT: &str = "YUESHOT_NATIVE_UPDATE_TEST_ROOT";

#[test]
fn registry_lookup_refuses_unknown_channels_and_does_not_guess_installation_paths() {
    assert!(registered_executable("invalid").is_err());
    assert!(registered_executable("beta/../../stable").is_err());
    // When a channel exists, read its real per-user registration without mutating it.
    for channel in ["dev", "beta", "stable"] {
        if let Ok(executable) = registered_executable(channel) {
            assert!(executable.is_absolute());
            assert_eq!(executable.file_name().unwrap(), "YueShot.exe");
            assert!(executable.is_file());
        }
    }
}

// These entry points exist only in the Rust test executable, never in YueShot.exe.
#[test]
#[ignore = "subprocess entry point invoked by the native update regression"]
fn helper_process() {
    let root = PathBuf::from(std::env::var_os(CHILD_ROOT).unwrap());
    let key = fs::read_to_string(root.join("test-public-key.txt")).unwrap();
    let result = apply_authorized(
        &root.join("stage/handoff.json"),
        "dev",
        &[key],
        &root.join("installed/YueShot.exe").canonicalize().unwrap(),
    );
    // Installer failures are written to the receipt and are expected in the failure scenario.
    if result.is_err() && !root.join("stage/result.json").exists() {
        panic!("helper did not reach installation: {result:?}");
    }
}

#[test]
#[ignore = "subprocess entry point invoked by the native update regression"]
fn parent_process() {
    let root = PathBuf::from(std::env::var_os(CHILD_ROOT).unwrap());
    let event_name = format!("Local\\YueShot-Native-Update-Test-{}", std::process::id());
    let event = handle(unsafe {
        CreateEventW(
            null(),
            1,
            0,
            wide(std::ffi::OsStr::new(&event_name)).as_ptr(),
        )
    })
    .unwrap();
    let job = Job {
        parent: std::process::id(),
        executable: std::env::current_exe().unwrap().canonicalize().unwrap(),
        data: root.join("data"),
        channel: "dev".into(),
        ready_event: event_name.clone(),
    };
    write_new(
        &root.join("stage/handoff.json"),
        &serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    let mut helper = Command::new(root.join("stage/updater-host.exe"))
        .args([
            "--exact",
            "update::handoff::tests::helper_process",
            "--ignored",
            "--nocapture",
        ])
        .stdout(File::create(root.join("helper.log")).unwrap())
        .stderr(Stdio::inherit())
        .creation_flags(NO_WINDOW)
        .spawn()
        .unwrap();
    assert_eq!(
        unsafe { WaitForSingleObject(event.0, 15_000) },
        WAIT_OBJECT_0
    );
    assert!(!root.join("data/installer-started").exists());
    Prepared(format!("{event_name}-commit")).commit().unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        !root.join("data/installer-started").exists(),
        "installer ran before parent exit"
    );
    assert!(
        helper.try_wait().unwrap().is_none(),
        "helper must still be waiting for its parent"
    );
    // Return through the normal test harness shutdown; do not kill the parent or helper.
}

#[test]
fn native_helper_waits_for_exit_checks_payload_and_records_success_or_failure() {
    let mut next = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    next.patch += 1;
    next.pre = semver::Prerelease::EMPTY;
    next.build = semver::BuildMetadata::EMPTY;
    let next = next.to_string();
    let base = unique_directory(&std::env::temp_dir().join("yueshot-native-update")).unwrap();
    for fail in [false, true] {
        let root = base.join(if fail { "failure" } else { "success" });
        for part in ["installed", "stage", "data"] {
            fs::create_dir_all(root.join(part)).unwrap();
        }
        let original = std::env::current_exe().unwrap();
        fs::copy(&original, root.join("installed/YueShot.exe")).unwrap();
        fs::copy(&original, root.join("stage/updater-host.exe")).unwrap();
        write_new(
            &root.join("data/user-settings"),
            b"retain exact user settings bytes",
        )
        .unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/update/handoff/fixture.rs");
        for (output, mode) in [
            ("payload.exe", "payload"),
            ("stage/installer.exe", "installer"),
        ] {
            let status = Command::new("rustc")
                .arg(&source)
                .args(["--edition=2024", "-C", "target-feature=+crt-static"])
                .arg("-o")
                .arg(root.join(output))
                .env("YUESHOT_UPDATE_FIXTURE_MODE", mode)
                .env("YUESHOT_UPDATE_FIXTURE_ROOT", &root)
                .env("YUESHOT_UPDATE_FIXTURE_FAIL", if fail { "1" } else { "0" })
                .env("YUESHOT_UPDATE_FIXTURE_VERSION", &next)
                .status()
                .unwrap();
            assert!(status.success());
        }
        let installer = root.join("stage/installer.exe");
        let installer_hash = file_hash(&mut File::open(&installer).unwrap()).unwrap();
        let executable_hash = file_hash(&mut File::open(root.join("payload.exe")).unwrap())
            .unwrap()
            .0;
        let manifest = Manifest {
            format: 1,
            product: "YueShot".into(),
            channel: "dev".into(),
            architecture: "x64".into(),
            version: next.clone(),
            expires_unix: now().unwrap() + 600,
            installer_url: format!("{REPOSITORY}yueshot-v0.1.3/test.exe"),
            installer_size: installer_hash.1,
            installer_sha256: installer_hash.0,
            executable_sha256: executable_hash,
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[91; 32]).unwrap();
        let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let mut public = b"Edtest-key".to_vec();
        public.extend_from_slice(key.public_key().as_ref());
        write_new(
            &root.join("test-public-key.txt"),
            encode(&public).as_bytes(),
        )
        .unwrap();
        let digest = blake2::Blake2b512::digest(&bytes);
        let signed = key.sign(&digest);
        let comment = "timestamp:1900000000\tfile:latest.json\tprehashed";
        let mut packet = b"EDtest-key".to_vec();
        packet.extend_from_slice(signed.as_ref());
        let mut global = signed.as_ref().to_vec();
        global.extend_from_slice(comment.as_bytes());
        let signature = format!(
            "untrusted comment: test only\n{}\ntrusted comment: {comment}\n{}\n",
            encode(&packet),
            encode(key.sign(&global).as_ref())
        );
        write_new(&root.join("stage/latest.json"), &bytes).unwrap();
        write_new(
            &root.join("stage/latest.json.minisig"),
            signature.as_bytes(),
        )
        .unwrap();
        let parent = Command::new(root.join("installed/YueShot.exe"))
            .args([
                "--exact",
                "update::handoff::tests::parent_process",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD_ROOT, &root)
            .creation_flags(NO_WINDOW)
            .output()
            .unwrap();
        assert!(
            parent.status.success(),
            "{}",
            String::from_utf8_lossy(&parent.stderr)
        );
        let start = Instant::now();
        let receipt_path = root.join("stage/result.json");
        while !receipt_path.exists() && start.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(&receipt_path).unwrap_or_else(|error| {
                panic!(
                    "{error}; helper log: {:?}",
                    fs::read_to_string(root.join("helper.log"))
                )
            }))
            .unwrap();
        assert_eq!(receipt["status"], if fail { "failed" } else { "installed" });
        assert_eq!(
            fs::read(root.join("data/user-settings")).unwrap(),
            b"retain exact user settings bytes"
        );
        assert!(root.join("data/installer-started").is_file());
        if fail {
            assert_eq!(
                file_hash(&mut File::open(root.join("installed/YueShot.exe")).unwrap())
                    .unwrap()
                    .0,
                file_hash(&mut File::open(&original).unwrap()).unwrap().0
            );
        } else {
            while !root.join("data/restarted").exists() && start.elapsed() < Duration::from_secs(20)
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            assert!(
                root.join("data/restarted").is_file(),
                "updated executable was not restarted"
            );
        }
    }
    // Retain the isolated fixture and receipts as native process evidence. No real installation changed.
    println!("native update fixture evidence: {}", base.display());
}

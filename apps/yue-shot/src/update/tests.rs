use super::*;

fn fixtures() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures.json")).unwrap()
}
fn authenticated(fixture: &serde_json::Value) -> Result<Offer> {
    authenticate(
        fixture["manifest"].as_str().unwrap().as_bytes(),
        fixture["signature"].as_str().unwrap(),
        &[fixtures()["public_key"].as_str().unwrap().to_owned()],
        "stable",
        "0.1.2",
        2_000_000_000,
    )
}

#[test]
fn signed_identity_channel_architecture_version_url_size_and_expiry_are_enforced() {
    for case in fixtures()["cases"].as_array().unwrap() {
        let result = authenticated(case);
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}: {result:?}",
            case["name"]
        );
    }
}

#[test]
fn unsigned_bytes_changed_after_signing_and_wrong_trust_roots_are_rejected() {
    let fixtures = fixtures();
    let case = &fixtures["cases"][0];
    let mut bytes = case["manifest"].as_str().unwrap().as_bytes().to_vec();
    bytes.push(b' ');
    let signature = case["signature"].as_str().unwrap();
    let key = fixtures["public_key"].as_str().unwrap().to_owned();
    assert!(authenticate(&bytes, signature, &[key], "stable", "0.1.2", 2_000_000_000).is_err());
    assert!(
        authenticate(
            case["manifest"].as_str().unwrap().as_bytes(),
            signature,
            &[],
            "stable",
            "0.1.2",
            2_000_000_000
        )
        .is_err()
    );
    assert!(authenticate(b"{}", "", &[], "stable", "0.1.2", 0).is_err());
}

#[test]
fn downloaded_bytes_are_checked_and_locked_against_replacement() {
    let directory = unique_directory(&std::env::temp_dir().join("yueshot-update-unit")).unwrap();
    let path = directory.join("installer.exe");
    write_new(&path, b"test installer bytes").unwrap();
    let hash = hex(&Sha256::digest(b"test installer bytes"));
    assert!(verify_file(&path, &hash, Some(1)).is_err());
    assert!(verify_file(&path, &"0".repeat(64), None).is_err());
    {
        let _guard = verified_file(&path, &hash, Some(20)).unwrap();
        assert!(fs::write(&path, b"replacement").is_err());
        assert!(fs::remove_file(&path).is_err());
    }
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn unconfigured_channels_fail_closed_and_plain_http_is_rejected() {
    // Production trust roots must be explicitly provisioned; a fixture key must never enter them.
    let fixture_key = fixtures()["public_key"].as_str().unwrap().to_owned();
    assert!(!KEYS.contains(&fixture_key));
    assert!(trusted_keys("invalid").is_err());
    assert!(http::get("http://127.0.0.1/no-network-request", 10).is_err());
    assert!(http::get("https://user:password@github.com/", 10).is_err());
}

#[test]
fn check_download_and_authentication_form_one_flow_with_latest_version_handling() {
    let fixtures = fixtures();
    let case = &fixtures["cases"][0];
    let keys = [fixtures["public_key"].as_str().unwrap().to_owned()];
    for (current, available) in [("0.1.2", true), ("0.1.3", false), ("0.1.4", false)] {
        let mut requests = Vec::new();
        let result = check_with("stable", &keys, current, 2_000_000_000, |url, _| {
            requests.push(url.to_owned());
            Ok(if url.ends_with(".minisig") {
                case["signature"].as_str().unwrap()
            } else {
                case["manifest"].as_str().unwrap()
            }
            .as_bytes()
            .to_vec())
        })
        .unwrap();
        assert_eq!(result.is_some(), available);
        assert_eq!(
            requests,
            [
                format!("{REPOSITORY}yueshot-updater-stable/latest.json"),
                format!("{REPOSITORY}yueshot-updater-stable/latest.json.minisig")
            ]
        );
    }
    assert!(
        check_with("stable", &keys, "0.1.2", 2_000_000_000, |_, _| Err(
            "connection failed".into()
        ))
        .is_err()
    );
}

#[test]
fn interrupted_and_corrupt_downloads_never_leave_an_installable_payload() {
    let root = unique_directory(&std::env::temp_dir().join("yueshot-update-download")).unwrap();
    let fixture = &fixtures()["cases"][0];
    let mut offer = authenticated(fixture).unwrap();
    offer.manifest.installer_sha256 = hex(&Sha256::digest(b"test installer bytes"));
    for interrupted in [false, true] {
        let result = stage_with(offer.clone(), &root, |_, _, file| {
            file.write_all(b"corrupt download").unwrap();
            if interrupted {
                Err("connection interrupted".into())
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read_dir(root.join("updates")).unwrap().count(), 0);
    }
    let staged = stage_with(offer, &root, |_, size, file| {
        assert_eq!(size, 20);
        file.write_all(b"test installer bytes")
            .map_err(|error| error.to_string())
    })
    .unwrap();
    assert!(staged.directory.join("installer.exe").is_file());
    assert!(!staged.directory.join("installer.partial").exists());
    assert_eq!(
        fs::read(staged.directory.join("latest.json")).unwrap(),
        fixture["manifest"].as_str().unwrap().as_bytes()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_requires_an_isolated_helper_and_cannot_be_used_as_an_arbitrary_installer_launcher() {
    assert!(handoff::apply(Path::new("relative.json")).is_err());
    let directory = unique_directory(&std::env::temp_dir().join("yueshot-update-apply")).unwrap();
    write_new(
        &directory.join("handoff.json"),
        b"{\"installer\":\"malicious.exe\"}",
    )
    .unwrap();
    assert!(handoff::apply(&directory.join("handoff.json")).is_err());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "explicit network qualification against the public YueShot repository"]
fn https_transport_uses_windows_networking_and_reads_the_public_repository() {
    let bytes = http::get("https://github.com/jinghu-moon/YueShot", 1024 * 1024).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("YueShot"));
}

#[test]
#[ignore = "explicit live download on the guarded low-space VHD; tools/test-yueshot-low-space.ps1 -UpdateDownload"]
fn authenticated_download_on_full_volume_is_cleaned_and_reclaimed_space_is_usable() {
    let root = PathBuf::from(
        std::env::var_os("YUESHOT_UPDATE_SPACE_ROOT").expect("use the guarded VHD test script"),
    );
    let phase = std::env::var("YUESHOT_UPDATE_SPACE_PHASE").expect("missing qualification phase");
    assert!(root.is_absolute());
    assert_eq!(root.file_name().unwrap(), "update-space-probe");
    assert_eq!(
        fs::read(root.parent().unwrap().join(".yueshot-low-space-owned")).unwrap(),
        b"YueShot guarded 128 MiB VHD qualification"
    );
    assert!(root.is_dir());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    assert!(matches!(phase.as_str(), "full" | "reclaimed"));
    // Authenticate the actual public feed with the same embedded trust root.
    // A lower explicit comparison version only obtains the offer; no install runs.
    let offer = check_with(
        "beta",
        &trusted_keys("beta").unwrap(),
        "0.0.0",
        now().unwrap(),
        http::get,
    )
    .unwrap()
    .expect("no authenticated public beta offer");
    let manifest_sha256 = hex(&Sha256::digest(&offer.bytes));
    let version = offer.version().to_owned();
    let installer_sha256 = offer.manifest.installer_sha256.clone();
    let staged = download(offer, &root);
    if phase == "full" {
        let error = staged.expect_err("download unexpectedly fit on the full test volume");
        assert!(
            error.contains("os error 112"),
            "not disk exhaustion: {error}"
        );
        assert_eq!(fs::read_dir(root.join("updates")).unwrap().count(), 0);
        println!("EXPECTED_DISK_FULL={error}");
    } else {
        let staged = staged.expect("download did not recover after space was reclaimed");
        assert!(staged.directory.join("installer.exe").is_file());
        assert!(!staged.directory.join("installer.partial").exists());
        verify_file(
            &staged.directory.join("installer.exe"),
            &staged.offer.manifest.installer_sha256,
            Some(staged.offer.manifest.installer_size),
        )
        .unwrap();
        fs::remove_dir_all(staged.directory).unwrap();
        assert_eq!(fs::read_dir(root.join("updates")).unwrap().count(), 0);
    }
    // Leave the next phase an empty test-owned data root.
    fs::remove_dir(root.join("updates")).unwrap();
    println!(
        "SPACE_QUALIFICATION={}",
        serde_json::json!({
            "phase": phase,
            "version": version,
            "manifestSha256": manifest_sha256,
            "installerSha256": installer_sha256,
            "status": "passed"
        })
    );
}

//! Compiled only by the updater's native process test; never linked into YueShot.
use std::{fs, path::PathBuf};
fn main() {
    let root = PathBuf::from(env!("YUESHOT_UPDATE_FIXTURE_ROOT"));
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if env!("YUESHOT_UPDATE_FIXTURE_MODE") == "installer" {
        assert_eq!(args[0], "/S");
        assert_eq!(args[1], "/NS");
        assert!(args[2].to_string_lossy().starts_with("/D="));
        fs::write(root.join("data/installer-started"), b"started after parent exit").unwrap();
        if env!("YUESHOT_UPDATE_FIXTURE_FAIL") == "1" { std::process::exit(14); }
        fs::copy(root.join("payload.exe"), root.join("installed/YueShot.exe")).unwrap();
    } else if args.first().is_some_and(|arg| arg == "--product-info") {
        println!("{{\"product\":\"YueShot\",\"version\":\"{}\",\"channel\":\"dev\",\"architecture\":\"x64\"}}", env!("YUESHOT_UPDATE_FIXTURE_VERSION"));
    } else {
        assert_eq!(args[0], "--show");
        assert_eq!(args[1], "--data-dir");
        assert_eq!(PathBuf::from(&args[2]), root.join("data"));
        fs::write(root.join("data/restarted"), b"new payload launched with same data directory").unwrap();
    }
}

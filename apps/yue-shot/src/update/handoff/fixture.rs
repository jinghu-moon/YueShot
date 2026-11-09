//! Compiled only by the updater's native process test; never linked into YueShot.
use std::{fs, path::PathBuf};
#[link(name = "kernel32")]
unsafe extern "system" { fn GetCommandLineW() -> *const u16; }
fn command_line() -> String {
    unsafe {
        let pointer = GetCommandLineW();
        let mut length = 0;
        while *pointer.add(length) != 0 { length += 1; }
        String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length))
    }
}
fn main() {
    let root = PathBuf::from(env!("YUESHOT_UPDATE_FIXTURE_ROOT"));
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if env!("YUESHOT_UPDATE_FIXTURE_MODE") == "installer" {
        assert_eq!(args[0], "/S");
        assert_eq!(args[1], "/NS");
        assert!(args[2].to_string_lossy().starts_with("/D="));
        // NSIS consumes the raw, unquoted tail, including spaces; CRT argv parsing differs.
        assert!(command_line().ends_with(&format!(" /D={}", root.join("installed").canonicalize().unwrap().display())),
            "NSIS /D must be the final, unquoted command-line tail");
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

use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-env-changed=YUESHOT_CHANNEL");
    println!("cargo:rerun-if-env-changed=YUESHOT_BUILD_ID");
    if let Ok(id) = env::var("YUESHOT_BUILD_ID") {
        assert!(
            id.len() == 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "YUESHOT_BUILD_ID must be a lowercase SHA-256 of SOURCE-MANIFEST.json"
        );
    }
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    println!("cargo:rerun-if-changed=packaging/YueShot.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numeric = [
        "CARGO_PKG_VERSION_MAJOR",
        "CARGO_PKG_VERSION_MINOR",
        "CARGO_PKG_VERSION_PATCH",
    ]
    .map(|key| env::var(key).unwrap())
    .join(",");
    let channel = env::var("YUESHOT_CHANNEL").unwrap_or_else(|_| {
        if env::var("DEBUG").as_deref() == Ok("true") {
            "dev".into()
        } else {
            "beta".into()
        }
    });
    assert!(
        ["dev", "beta", "stable"].contains(&channel.as_str()),
        "invalid build channel"
    );
    let resource_path = |name: &str| {
        root.join("packaging")
            .join(name)
            .display()
            .to_string()
            .replace('\\', "\\\\")
    };
    let rc = format!(
        r#"
#include <windows.h>
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric},0
PRODUCTVERSION {numeric},0
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904b0"
  BEGIN
   VALUE "CompanyName", "Yue\0"
   VALUE "FileDescription", "YueShot ({channel})\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "InternalName", "YueShot\0"
   VALUE "OriginalFilename", "YueShot.exe\0"
   VALUE "ProductName", "YueShot\0"
   VALUE "ProductVersion", "{version}\0"
   VALUE "LegalCopyright", "AGPL-3.0-only; see LICENSE\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x409, 1200
 END
END
"#,
        icon = resource_path("YueShot.ico"),
    );
    let source = out.join("YueShot.rc");
    let compiled = out.join("YueShot.res");
    fs::write(&source, rc).expect("write Windows resources");
    let mut candidates = Vec::new();
    if let Some(sdk) = env::var_os("WindowsSdkDir") {
        candidates.push(PathBuf::from(sdk).join("bin"));
    }
    if let Some(programs) = env::var_os("ProgramFiles(x86)") {
        candidates.push(PathBuf::from(programs).join("Windows Kits/10/bin"));
    }
    let sdk = candidates.into_iter().find_map(|base| {
        let mut versions: Vec<_> = fs::read_dir(base)
            .ok()?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("10."))
            .map(|entry| entry.path())
            .collect();
        versions.sort_by_key(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .split('.')
                .map(|part| part.parse::<u32>().unwrap_or(0))
                .collect::<Vec<_>>()
        });
        versions
            .into_iter()
            .rev()
            .find(|version| version.join("x64/rc.exe").is_file())
    });
    let (compiler, includes) = if let Some(version) = sdk {
        (
            version.join("x64/rc.exe"),
            Some(
                version
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("Include")
                    .join(version.file_name().unwrap()),
            ),
        )
    } else {
        (PathBuf::from("rc.exe"), None)
    };
    let mut command = Command::new(compiler);
    command.arg("/nologo").arg("/fo").arg(&compiled);
    if let Some(include) = includes {
        for folder in ["um", "shared", "ucrt", "winrt", "cppwinrt"] {
            let path = include.join(folder);
            if path.is_dir() {
                command.arg("/I").arg(path);
            }
        }
    }
    let output = command
        .arg(source)
        .output()
        .expect("Windows SDK rc.exe is required to build YueShot");
    assert!(
        output.status.success(),
        "resource compilation failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("cargo:rustc-link-arg-bin=YueShot={}", compiled.display());
}

//! Product identity and launch parsing run before GPUI, GPU or global hotkeys.
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Channel {
    Dev,
    Beta,
    Stable,
}
impl Channel {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Beta => "beta",
            Self::Stable => "stable",
        }
    }
    pub(crate) fn build() -> Result<Self, String> {
        match option_env!("YUESHOT_CHANNEL").unwrap_or(if cfg!(debug_assertions) {
            "dev"
        } else {
            "beta"
        }) {
            "dev" => Ok(Self::Dev),
            "beta" => Ok(Self::Beta),
            "stable" => Ok(Self::Stable),
            _ => Err("invalid build channel".into()),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Launch {
    #[default]
    Show,
    Capture,
    Background,
    Smoke,
    Help,
    Version,
    RemoveStartup,
    Backup,
    Restore,
    ExportDiagnostics,
    ProductInfo,
    UninstallCleanup,
}
#[derive(Debug, Default)]
pub(crate) struct StartupOptions {
    pub launch: Launch,
    pub data_dir: Option<PathBuf>,
    pub maintenance_path: Option<PathBuf>,
}
impl StartupOptions {
    pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut result = Self::default();
        let mut args = args.into_iter();
        let mut command = false;
        while let Some(arg) = args.next() {
            if arg == "--data-dir" {
                if result.data_dir.is_some() {
                    return Err("--data-dir cannot be repeated".into());
                }
                result.data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir requires an absolute path")?,
                ));
                continue;
            }
            let launch = match arg.to_str() {
                Some("--show") => Launch::Show,
                Some("--capture") => Launch::Capture,
                Some("--background") => Launch::Background,
                Some("--smoke-test") => Launch::Smoke,
                Some("--help" | "-h") => Launch::Help,
                Some("--version" | "-V") => Launch::Version,
                Some("--product-info") => Launch::ProductInfo,
                Some("--uninstall-cleanup") => Launch::UninstallCleanup,
                Some("--remove-startup") => Launch::RemoveStartup,
                Some("--backup-to" | "--restore-from" | "--export-diagnostics-to") => {
                    let path = PathBuf::from(
                        args.next()
                            .ok_or("maintenance command requires an absolute destination")?,
                    );
                    if !path.is_absolute() {
                        return Err("maintenance destination must be absolute".into());
                    }
                    result.maintenance_path = Some(path);
                    match arg.to_str() {
                        Some("--backup-to") => Launch::Backup,
                        Some("--restore-from") => Launch::Restore,
                        _ => Launch::ExportDiagnostics,
                    }
                }
                _ => return Err("unknown argument; use --help".into()),
            };
            if command {
                return Err("choose one launch command".into());
            }
            command = true;
            result.launch = launch;
        }
        if result
            .data_dir
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err("--data-dir must be absolute".into());
        }
        if matches!(
            result.launch,
            Launch::Smoke | Launch::Backup | Launch::Restore | Launch::ExportDiagnostics
        ) && result.data_dir.is_none()
        {
            return Err("maintenance command requires explicit --data-dir".into());
        }
        Ok(result)
    }
}
#[derive(Clone, Debug)]
pub(crate) struct ProductPaths {
    pub root: PathBuf,
    pub channel: Channel,
    pub instance_id: String,
}
impl ProductPaths {
    pub(crate) fn resolve(
        options: &StartupOptions,
        channel: Channel,
        local: Option<&Path>,
    ) -> Result<Self, String> {
        let root = match &options.data_dir {
            Some(root) => root.clone(),
            None => local
                .ok_or("LOCALAPPDATA is unavailable; specify --data-dir")?
                .join(crate::product::data_relative(channel)),
        };
        if !root.is_absolute() {
            return Err("product data root must be absolute".into());
        }
        std::fs::create_dir_all(&root)
            .map_err(|e| format!("create product data directory: {e}"))?;
        let root = root
            .canonicalize()
            .map_err(|e| format!("resolve product data directory: {e}"))?;
        let instance_id = crate::product::instance_id(channel, &root);
        Ok(Self {
            root,
            channel,
            instance_id,
        })
    }
    pub(crate) fn aumid(&self) -> String {
        crate::product::aumid(self.channel)
    }
}
pub(crate) const HELP: &str = "YueShot\nUsage: YueShot.exe [--show | --capture | --background | --smoke-test | --remove-startup | --backup-to ABSOLUTE_DIR | --restore-from ABSOLUTE_DIR] [--data-dir ABSOLUTE_PATH]\n  --smoke-test requires an isolated --data-dir; starts no UI, GPU or hotkeys\n  --remove-startup  Remove this channel/data root's per-user login startup; no UI/GPU/hotkeys\n  --backup-to ABSOLUTE_DIR  Offline verified backup; requires explicit --data-dir\n  --restore-from ABSOLUTE_DIR  Restore to an empty matching data root; requires --data-dir\n  --export-diagnostics-to ABSOLUTE_JSON  Offline metadata-only export; requires --data-dir; quit YueShot first; no overwrite/upload\n  --product-info  Print channel installation identity; no data access\n  --uninstall-cleanup  Remove only startup entries owned by this executable/channel\n  --help, -h     Show help\n  --version, -V  Show version and build channel";
#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<StartupOptions, String> {
        StartupOptions::parse(args.iter().map(OsString::from))
    }
    #[test]
    fn launch_parser_preserves_windows_paths_and_rejects_ambiguous_commands() {
        assert_eq!(
            parse(&["--remove-startup"]).unwrap().launch,
            Launch::RemoveStartup
        );
        let options = parse(&["--capture", "--data-dir", r"D:\test space\悦"]).unwrap();
        assert_eq!(options.launch, Launch::Capture);
        assert_eq!(
            options.data_dir.unwrap(),
            PathBuf::from(r"D:\test space\悦")
        );
        for args in [
            &["--capture", "--show"][..],
            &["--unknown"],
            &["--data-dir"],
            &["--data-dir", "relative"],
            &["--smoke-test"],
            &["--export-diagnostics-to"],
            &[
                "--export-diagnostics-to",
                "relative.json",
                "--data-dir",
                r"D:\data",
            ],
            &["--export-diagnostics-to", r"D:\diagnostics.json"],
            &["--export-diagnostics-to", r"D:\diagnostics.json", "--show"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
        assert_eq!(
            parse(&[
                "--export-diagnostics-to",
                r"D:\diagnostics.json",
                "--data-dir",
                r"D:\data"
            ])
            .unwrap()
            .launch,
            Launch::ExportDiagnostics
        );
    }
    #[test]
    fn channels_and_canonical_data_roots_define_independent_instances() {
        let dir = std::env::temp_dir().join(format!("yueshot-identity-{}", std::process::id()));
        let options = StartupOptions {
            data_dir: Some(dir.clone()),
            ..Default::default()
        };
        let dev = ProductPaths::resolve(&options, Channel::Dev, None).unwrap();
        let beta = ProductPaths::resolve(&options, Channel::Beta, None).unwrap();
        assert_ne!(dev.instance_id, beta.instance_id);
        assert_ne!(dev.aumid(), beta.aumid());
        let alias = StartupOptions {
            data_dir: Some(dir.join(".")),
            ..Default::default()
        };
        assert_eq!(
            dev.instance_id,
            ProductPaths::resolve(&alias, Channel::Dev, None)
                .unwrap()
                .instance_id
        );
        assert!(ProductPaths::resolve(&StartupOptions::default(), Channel::Dev, None).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn default_product_roots_are_channel_isolated_and_never_use_a_temp_fallback() {
        let local = std::env::temp_dir().join(format!(
            "yueshot-localappdata-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let options = StartupOptions::default();
        let dev = ProductPaths::resolve(&options, Channel::Dev, Some(&local)).unwrap();
        let beta = ProductPaths::resolve(&options, Channel::Beta, Some(&local)).unwrap();
        let stable = ProductPaths::resolve(&options, Channel::Stable, Some(&local)).unwrap();

        assert_eq!(
            dev.root,
            local.join("Yue/YueShot/dev").canonicalize().unwrap()
        );
        assert_eq!(
            beta.root,
            local.join("Yue/YueShot/beta").canonicalize().unwrap()
        );
        assert_eq!(
            stable.root,
            local.join("Yue/YueShot/stable").canonicalize().unwrap()
        );
        assert_ne!(dev.root, beta.root);
        assert_ne!(beta.root, stable.root);
        assert_eq!(dev.aumid(), "com.seeyuer.YueShot.dev");
        assert_eq!(beta.aumid(), "com.seeyuer.YueShot.beta");
        assert_eq!(stable.aumid(), "com.seeyuer.YueShot.stable");
        assert!(
            ProductPaths::resolve(&options, Channel::Beta, None)
                .unwrap_err()
                .contains("LOCALAPPDATA is unavailable")
        );

        std::fs::remove_dir_all(local).unwrap();
    }

    #[test]
    fn persistent_path_creation_failure_is_returned_without_fallback() {
        let local = std::env::temp_dir().join(format!(
            "yueshot-invalid-localappdata-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&local).unwrap();
        let blocker = local.join("not-a-directory");
        std::fs::write(&blocker, b"fixture").unwrap();
        let error =
            ProductPaths::resolve(&StartupOptions::default(), Channel::Beta, Some(&blocker))
                .unwrap_err();
        assert!(
            error.starts_with("create product data directory:"),
            "{error}"
        );
        assert!(!local.join("Yue/YueShot/beta").exists());
        std::fs::remove_dir_all(local).unwrap();
    }
}

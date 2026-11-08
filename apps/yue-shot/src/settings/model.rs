//! The settings model and its file on disk.
//!
//! Design rules this follows (Design Guides, "Forms and settings"): one named file in the
//! app's data directory, defaults that match the product's current behaviour, and a load
//! path that uses defaults for a missing file and preserves invalid files for recovery.
//!
//! Only settings with a *real* consumer are modelled. There is deliberately no
//! `deep_select_visible_wrappers`: the plan named it, but no behaviour in the walker
//! corresponds to it yet, and a switch that changes nothing is worse than no switch.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yue_shot_capture::keymap::{Keymap, KeymapOverrides};
use yue_shot_capture::preferences::OverlayPreferences;
use yue_shot_capture::save_format::{SaveFormat, SavePreferences};
use yue_shot_capture::units::{LabelDisplay, LengthUnit};
use yue_shot_capture::window_detection::{DEFAULT_ADOPT_TEXT_RUNS, DetectionOptions};

/// The user's preferences.
///
/// `#[serde(default)]` on the struct is what makes an older file (missing a field) and a
/// current one with omitted optional fields load: missing fields take defaults.
/// The file envelope independently rejects unsupported schema versions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Keep the application and its global shortcuts alive after the main window closes.
    pub close_to_tray: bool,
    pub shortcuts: crate::shortcuts::Shortcuts,
    /// Whether a bare text run may be published as the snap target (docs/21 §5.19).
    pub deep_select_text_runs: bool,
    /// Blur the screen outside the selection while picking.
    pub mask_blur: bool,
    /// Show the shortcut guide in the bottom-left corner while picking.
    pub shortcut_guide: bool,
    /// The unit the size/coordinate label is measured in (`px` or `dp`).
    pub length_unit: LengthUnit,
    /// Whether that label shows the selection's size.
    pub show_size: bool,
    /// Whether that label also shows the selection's origin.
    pub show_coordinates: bool,
    /// Whether that label prints the unit it is in (`1024×768 px`).
    pub show_unit: bool,
    /// The overlay's key bindings, as changes from the defaults.
    pub keymap: KeymapOverrides,
    /// What a save should produce (`docs/34` §15). Defaults to PNG, the format that can hold
    /// anything; an older file without these fields loads as those defaults.
    pub save_format: SaveFormat,
}

impl Default for Settings {
    fn default() -> Self {
        let overlay = OverlayPreferences::default();
        Self {
            close_to_tray: true,
            shortcuts: Default::default(),
            deep_select_text_runs: DEFAULT_ADOPT_TEXT_RUNS,
            mask_blur: overlay.mask_blur,
            shortcut_guide: overlay.shortcut_guide,
            length_unit: overlay.label.length_unit,
            show_size: overlay.label.show_size,
            show_coordinates: overlay.label.show_coordinates,
            show_unit: overlay.label.show_unit,
            keymap: KeymapOverrides::default(),
            save_format: overlay.save.format,
        }
    }
}

impl Settings {
    fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.save_format,
            SaveFormat::Png | SaveFormat::Jpeg | SaveFormat::Webp
        ) {
            return Err("默认保存格式必须是 PNG、JPEG 或 WebP；原配置未修改".into());
        }
        let keymap = Keymap::new(&self.keymap).map_err(|error| {
            format!("invalid shortcut configuration: {error}; original preserved")
        })?;
        self.shortcuts.validate(&keymap)?;
        Ok(())
    }

    /// The detection options these settings imply (docs/23 T4.4.1).
    pub fn detection_options(&self) -> DetectionOptions {
        DetectionOptions {
            adopt_text_runs: self.deep_select_text_runs,
        }
    }

    /// The overlay preferences these settings imply.
    ///
    /// Bindings that cannot be applied (a hand-edited file naming an unknown action, two actions
    /// on one chord) are reported and replaced by the default keymap as a whole, for the same
    /// reason a damaged file loads as defaults: a half-applied keymap is a set of keys nobody
    /// chose, and refusing to capture over it is worse.
    pub fn overlay_preferences(&self) -> OverlayPreferences {
        let keymap = Keymap::new(&self.keymap).unwrap_or_else(|error| {
            eprintln!(
                "[yue-shot] key bindings in the settings are invalid ({error}); using defaults"
            );
            Keymap::default()
        });
        OverlayPreferences {
            mask_blur: self.mask_blur,
            shortcut_guide: self.shortcut_guide,
            label: LabelDisplay {
                length_unit: self.length_unit,
                show_size: self.show_size,
                show_coordinates: self.show_coordinates,
                show_unit: self.show_unit,
            },
            keymap: Arc::new(keymap),
            save: SavePreferences {
                format: self.save_format,
                ..SavePreferences::default()
            },
        }
    }

    /// Adopt the display settings a capture ended with (docs/23 §14.53/§14.54); `true` when they differ.
    ///
    /// The badge's menu and the custom-region panel edit the same four values this file stores. The
    /// merge is here, on the value, so the rule can be tested without a window; the view is what
    /// writes the result out.
    pub fn adopt_label(&mut self, label: LabelDisplay) -> bool {
        if self.length_unit == label.length_unit
            && self.show_size == label.show_size
            && self.show_coordinates == label.show_coordinates
            && self.show_unit == label.show_unit
        {
            return false;
        }
        self.length_unit = label.length_unit;
        self.show_size = label.show_size;
        self.show_coordinates = label.show_coordinates;
        self.show_unit = label.show_unit;
        true
    }
}

const SETTINGS_SCHEMA: u32 = 1;
fn schema_version() -> u32 {
    SETTINGS_SCHEMA
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsFile {
    #[serde(default = "schema_version")]
    schema_version: u32,
    #[serde(flatten)]
    settings: Settings,
}
fn next_file_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Reads and writes `<app data>/settings.json`.
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(app_data: impl AsRef<Path>) -> Self {
        Self {
            path: app_data.as_ref().join("settings.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Missing files use defaults. Invalid and future files remain untouched and refuse writes.
    pub fn load(&self) -> Result<Settings, String> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Settings::default());
            }
            Err(error) => return Err(format!("read settings failed: {error}")),
        };
        let file: SettingsFile = serde_json::from_slice(&bytes).map_err(|error| {
            let backup = self.backup_corrupt(&bytes);
            format!("settings are damaged ({error}); original preserved; backup: {backup}")
        })?;
        if file.schema_version != SETTINGS_SCHEMA {
            return Err(format!(
                "unsupported settings schema {}; expected {}; original preserved",
                file.schema_version, SETTINGS_SCHEMA
            ));
        }
        file.settings.validate()?;
        Ok(file.settings)
    }

    fn backup_corrupt(&self, bytes: &[u8]) -> String {
        use std::io::Write as _;
        let path = self.path.with_file_name(format!(
            "settings.corrupt-{}-{}.json",
            std::process::id(),
            next_file_id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut file| {
                file.write_all(bytes)?;
                file.sync_all()
            }) {
            Ok(()) => path.display().to_string(),
            Err(error) => format!("backup failed ({error})"),
        }
    }

    /// Flush a uniquely created sibling then atomically replace the destination.
    pub fn save(&self, settings: &Settings) -> Result<(), String> {
        use std::io::Write as _;
        settings.validate()?;
        // This is also enforced on every save, even if a newer build changes the file
        // after the view opened. Never silently downgrade its schema or destroy bad input.
        self.load()?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create settings directory failed: {error}"))?;
        }
        let bytes = serde_json::to_vec_pretty(&SettingsFile {
            schema_version: SETTINGS_SCHEMA,
            settings: settings.clone(),
        })
        .map_err(|error| format!("serialize settings failed: {error}"))?;
        let temporary =
            self.path
                .with_extension(format!("{}-{}.tmp", std::process::id(), next_file_id()));
        let write = (|| -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            replace_file(&temporary, &self.path)
        })();
        if write.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        write.map_err(|error| format!("commit settings failed: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use yue_shot_capture::keymap::{Keymap, KeymapOverrides};
    use yue_shot_capture::preferences::OverlayPreferences;
    use yue_shot_capture::save_format::SaveFormat;
    use yue_shot_capture::units::{LabelDisplay, LengthUnit};

    use super::{Settings, SettingsStore};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "yue-shot-settings-{name}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn a_missing_file_is_first_run_and_keeps_todays_behaviour() {
        let dir = root("missing");
        let store = SettingsStore::new(&dir);
        let settings = store.load().unwrap();
        assert_eq!(settings, Settings::default());
        // The default is the product's current behaviour, not a new choice.
        assert_eq!(
            settings.detection_options(),
            yue_shot_capture::window_detection::DetectionOptions::default()
        );
        assert_eq!(
            settings.overlay_preferences(),
            OverlayPreferences::default()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let dir = root("round-trip");
        let store = SettingsStore::new(&dir);
        let mut keymap = KeymapOverrides::default();
        keymap.set_visible("level", false);
        let wanted = Settings {
            deep_select_text_runs: false,
            mask_blur: false,
            shortcut_guide: false,
            length_unit: LengthUnit::Dp,
            show_size: true,
            show_coordinates: false,
            show_unit: false,
            keymap,
            ..Settings::default()
        };
        store.save(&wanted).expect("save");
        assert_eq!(store.load().unwrap(), wanted);
        // The file spells the readout's unit the way the page does, so a hand-edited file needs no
        // translation table, and a field the user never touched keeps its default (above).
        let text = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(text.contains("\"lengthUnit\": \"dp\""), "{text}");
        assert!(text.contains("\"showCoordinates\": false"), "{text}");
        assert!(text.contains("\"showUnit\": false"), "{text}");
        // And the bridges to capture see the same values.
        assert!(!store.load().unwrap().detection_options().adopt_text_runs);
        let overlay = store.load().unwrap().overlay_preferences();
        assert!(!overlay.mask_blur && !overlay.shortcut_guide);
        assert_eq!(overlay.label.length_unit, LengthUnit::Dp);
        assert!(!overlay.label.show_coordinates);
        assert!(!overlay.label.show_unit);
        assert_ne!(*overlay.keymap, Keymap::default());
        // No temporary file is left behind.
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_file_is_preserved_and_backed_up_without_being_overwritten() {
        let dir = root("damaged");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), "{ not json").unwrap();
        let store = SettingsStore::new(&dir);
        let before = std::fs::read(store.path()).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&Settings::default()).is_err());
        assert_eq!(std::fs::read(store.path()).unwrap(), before);
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("settings.corrupt-")
            })
            .collect();
        assert!(!backups.is_empty());
        for path in backups {
            assert_eq!(std::fs::read(path).unwrap(), before);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_from_another_version_still_loads() {
        let dir = root("forward-compatible");
        std::fs::create_dir_all(&dir).unwrap();
        // A field this build does not know, and the field it does.
        std::fs::write(
            dir.join("settings.json"),
            r#"{"deepSelectTextRuns": false, "somethingNew": 7}"#,
        )
        .unwrap();
        let store = SettingsStore::new(&dir);
        assert_eq!(
            store.load().unwrap(),
            Settings {
                deep_select_text_runs: false,
                ..Settings::default()
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_older_file_without_a_save_block_keeps_png_at_ninety_two() {
        // Backward compatibility is the struct-level `#[serde(default)]`: a file written before the
        // save block existed must load, not reset, and it must not invent a format.
        let dir = root("no-save-block");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{"maskBlur": false}"#).unwrap();
        let settings = SettingsStore::new(&dir).load().unwrap();
        assert!(!settings.mask_blur);
        assert_eq!(settings.save_format, SaveFormat::Png);
        let overlay = settings.overlay_preferences();
        assert_eq!(overlay.save.format, SaveFormat::Png);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_save_block_round_trips_through_the_file() {
        let dir = root("save-round-trip");
        std::fs::create_dir_all(&dir).unwrap();
        let wanted = Settings {
            save_format: SaveFormat::Jpeg,
            ..Settings::default()
        };
        let store = SettingsStore::new(&dir);
        store.save(&wanted).expect("save");
        assert_eq!(store.load().unwrap(), wanted);
        let text = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        // Stored by the name the file spells, so a hand edit is readable.
        assert!(text.contains("\"saveFormat\": \"jpeg\""), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_key_bindings_are_rejected_and_the_original_is_preserved() {
        let dir = root("bad-keymap");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("settings.json"),
            r#"{"maskBlur": false, "keymap": {"chords": {"no-such-action": ["X"]}}}"#,
        )
        .unwrap();
        let store = SettingsStore::new(&dir);
        let before = std::fs::read(store.path()).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&Settings::default()).is_err());
        assert_eq!(std::fs::read(store.path()).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untouched_bindings_are_not_written_out() {
        let text = serde_json::to_string(&Settings::default()).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.keymap, KeymapOverrides::default());
        assert!(back.keymap.is_empty());
    }

    #[test]
    fn adopting_the_overlay_label_keeps_the_rest_of_the_settings() {
        let mut settings = Settings {
            deep_select_text_runs: false,
            mask_blur: false,
            shortcut_guide: false,
            ..Settings::default()
        };
        let from_the_menu = LabelDisplay {
            length_unit: LengthUnit::Dp,
            show_size: false,
            show_coordinates: false,
            show_unit: false,
        };
        assert!(settings.adopt_label(from_the_menu));
        assert_eq!(settings.length_unit, LengthUnit::Dp);
        assert!(!settings.show_size && !settings.show_coordinates && !settings.show_unit);
        // Everything the badge's menu says nothing about stays exactly as it was.
        assert!(!settings.deep_select_text_runs && !settings.mask_blur && !settings.shortcut_guide);
        // The same values again are not a change, so nothing is written and nothing is announced.
        assert!(!settings.adopt_label(from_the_menu));
    }
    #[test]
    fn future_schema_cannot_be_loaded_or_downgraded_by_save() {
        let dir = root("future-schema");
        std::fs::create_dir_all(&dir).unwrap();
        let store = SettingsStore::new(&dir);
        let bytes = br#"{"schemaVersion":999,"maskBlur":false}"#;
        std::fs::write(store.path(), bytes).unwrap();
        assert!(
            store
                .load()
                .unwrap_err()
                .contains("unsupported settings schema 999")
        );
        assert!(store.save(&Settings::default()).is_err());
        assert_eq!(std::fs::read(store.path()).unwrap(), bytes);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn replacement_is_durable_and_invalid_values_leave_the_last_good_file() {
        let dir = root("replace-valid");
        let store = SettingsStore::new(&dir);
        store.save(&Settings::default()).unwrap();
        let changed = Settings {
            mask_blur: false,
            ..Settings::default()
        };
        store.save(&changed).unwrap();
        assert_eq!(store.load().unwrap(), changed);
        let before = std::fs::read(store.path()).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&before).unwrap()["schemaVersion"],
            1
        );
        for format in [SaveFormat::Bmp, SaveFormat::Avif] {
            assert!(
                store
                    .save(&Settings {
                        save_format: format,
                        ..changed.clone()
                    })
                    .is_err()
            );
            assert_eq!(std::fs::read(store.path()).unwrap(), before);
        }
        assert!(!std::fs::read_dir(&dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "tmp")
        }));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

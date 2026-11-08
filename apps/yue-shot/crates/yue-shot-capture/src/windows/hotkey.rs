//! Session-scoped global keys for a running scroll capture. Product-wide hotkeys belong to the app.
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND},
    UI::Input::KeyboardAndMouse::{
        MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_ESCAPE, VK_RETURN,
    },
};
pub const ESCAPE_VIRTUAL_KEY: u32 = VK_ESCAPE as u32;
pub const RETURN_VIRTUAL_KEY: u32 = VK_RETURN as u32;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hotkey {
    id: i32,
    modifiers: u32,
    virtual_key: u32,
    label: &'static str,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyError {
    /// Another application already owns the combination.
    Conflict { label: &'static str },
    /// Registration failed for any other reason.
    Failed { label: &'static str, code: u32 },
}

impl HotkeyError {
    pub fn message(self) -> String {
        match self {
            Self::Conflict { label } => {
                format!("{label} is already registered by another application (Win32 error 1409)")
            }
            Self::Failed { label, code } => {
                format!("RegisterHotKey({label}) failed with Win32 error {code}")
            }
        }
    }

    pub fn is_conflict(self) -> bool {
        matches!(self, Self::Conflict { .. })
    }
}

const ERROR_HOTKEY_ALREADY_REGISTERED: u32 = 1409;

/// Registrations owned by one scroll session, including bindings changed in settings.
/// Dropping this owner releases every successful registration, including partial failure.
pub(crate) struct ScrollHotkeys {
    window: HWND,
    registered: Vec<(Hotkey, crate::keymap::Action)>,
}
impl ScrollHotkeys {
    pub(crate) fn register(window: HWND, keymap: &crate::keymap::Keymap) -> Result<Self, String> {
        let mut scope = Self {
            window,
            registered: Vec::new(),
        };
        for (hotkey, action) in scroll_bindings(keymap)? {
            register_one(window, hotkey)
                .map_err(|error| format!("{}: {}", keymap.key_text(action), error.message()))?;
            scope.registered.push((hotkey, action));
        }
        Ok(scope)
    }
    pub(crate) fn action(&self, id: i32) -> Option<crate::keymap::Action> {
        self.registered
            .iter()
            .find(|(key, _)| key.id == id)
            .map(|(_, action)| *action)
    }
}
impl Drop for ScrollHotkeys {
    fn drop(&mut self) {
        for (key, _) in &self.registered {
            unregister_one(self.window, *key);
        }
    }
}
fn scroll_bindings(
    keymap: &crate::keymap::Keymap,
) -> Result<Vec<(Hotkey, crate::keymap::Action)>, String> {
    use crate::keymap::{Action, Key, Trigger};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_SHIFT};
    let mut keys = Vec::new();
    for action in [Action::ScrollFinish, Action::ScrollCancel] {
        for chord in keymap.chords(action) {
            let Trigger::Key(key) = chord.trigger else {
                return Err("长截图热键必须为键盘组合".into());
            };
            if chord.hold.is_some() {
                return Err("长截图热键不支持额外按住按键".into());
            }
            let virtual_key = (1..=255)
                .find(|vk| Key::from_virtual_key(*vk) == Some(key))
                .ok_or_else(|| format!("无法注册长截图热键 {chord}"))?;
            let id = 0x5400 + i32::try_from(keys.len()).map_err(|e| e.to_string())?;
            if id > 0xbfff {
                return Err("长截图热键数量超出上限".into());
            }
            keys.push((
                Hotkey {
                    id,
                    modifiers: MOD_NOREPEAT
                        | if chord.ctrl { MOD_CONTROL } else { 0 }
                        | if chord.shift { MOD_SHIFT } else { 0 }
                        | if chord.alt { MOD_ALT } else { 0 },
                    virtual_key,
                    label: action.spec().label,
                },
                action,
            ));
        }
    }
    Ok(keys)
}

fn register_one(window: HWND, hotkey: Hotkey) -> Result<(), HotkeyError> {
    if unsafe { RegisterHotKey(window, hotkey.id, hotkey.modifiers, hotkey.virtual_key) } != 0 {
        return Ok(());
    }
    let code = unsafe { GetLastError() };
    Err(if code == ERROR_HOTKEY_ALREADY_REGISTERED || code == 0 {
        HotkeyError::Conflict {
            label: hotkey.label,
        }
    } else {
        HotkeyError::Failed {
            label: hotkey.label,
            code,
        }
    })
}

fn unregister_one(window: HWND, hotkey: Hotkey) {
    unsafe {
        UnregisterHotKey(window, hotkey.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ctrl_s_and_escape_are_scoped_and_f9_is_free() {
        use crate::keymap::{Action, Keymap};
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_CONTROL, VK_F9};
        let bindings = super::scroll_bindings(&Keymap::default()).unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].1, Action::ScrollFinish);
        assert_eq!(bindings[0].0.modifiers, MOD_CONTROL | MOD_NOREPEAT);
        assert_eq!(bindings[0].0.virtual_key, b'S' as u32);
        assert_eq!(bindings[1].1, Action::ScrollCancel);
        assert_eq!(bindings[1].0.virtual_key, VK_ESCAPE as u32);
        assert_ne!(bindings[0].0.id, bindings[1].0.id);
        assert!(
            bindings
                .iter()
                .all(|(key, _)| key.virtual_key != VK_F9 as u32)
        );
    }

    #[test]
    fn scoped_save_registration_tracks_settings_including_all_alternative_chords() {
        use crate::keymap::{Action, Chord, Keymap, KeymapOverrides};
        let mut overrides = KeymapOverrides::default();
        overrides.set_chords(
            Action::ScrollFinish,
            &[
                Chord::parse("Ctrl+Shift+S").unwrap(),
                Chord::parse("Alt+Enter").unwrap(),
            ],
        );
        let map = Keymap::new(&overrides).unwrap();
        let bindings = super::scroll_bindings(&map).unwrap();
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[0].0.virtual_key, b'S' as u32);
        assert_eq!(bindings[1].0.virtual_key, VK_RETURN as u32);
        assert_eq!(bindings[0].1, Action::ScrollFinish);
        assert_eq!(bindings[1].1, Action::ScrollFinish);
        assert_ne!(bindings[0].0.modifiers, bindings[1].0.modifiers);
    }
}

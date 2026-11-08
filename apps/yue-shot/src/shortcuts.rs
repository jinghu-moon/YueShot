//! Product-global bindings use physical Windows virtual keys. Overlay keymaps stay session scoped.
use crate::commands::Command;
use yue_shot_capture::keymap::{Action, Chord, Key, Keymap, Trigger};
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Shortcuts {
    #[serde(rename = "capture.region")]
    pub capture: String,
    #[serde(rename = "capture.scroll")]
    pub scroll: String,
    #[serde(rename = "pin.latest")]
    pub pin: String,
}
impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            capture: "F5".into(),
            scroll: "F7".into(),
            pin: "F6".into(),
        }
    }
}
impl Shortcuts {
    pub(crate) fn rows(&self) -> [(Command, &str); 3] {
        [
            (Command::Capture, &self.capture),
            (Command::Scroll, &self.scroll),
            (Command::Pin, &self.pin),
        ]
    }
    pub(crate) fn validate(&self, keymap: &Keymap) -> Result<(), String> {
        let mut chords = std::collections::HashSet::new();
        for (command, value) in self.rows() {
            let chord = parse(value)
                .map_err(|error| format!("{}：{error}", command.action_id().unwrap_or("global")))?;
            if !chords.insert(chord) {
                return Err(format!("全局快捷键重复：{value}"));
            }
            for action in Action::ALL {
                if keymap.chords(action).contains(&chord) {
                    return Err(format!(
                        "全局快捷键 {value} 与截图会话操作“{}”冲突",
                        action.spec().label
                    ));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn hotkeys(&self) -> Result<Vec<yue_shell::Hotkey>, String> {
        self.rows()
            .into_iter()
            .map(|(command, value)| {
                let chord = parse(value)?;
                let Trigger::Key(key) = chord.trigger else {
                    unreachable!()
                };
                let vk = (1..=255)
                    .find(|vk| Key::from_virtual_key(*vk) == Some(key))
                    .ok_or_else(|| format!("Windows 不支持该快捷键：{value}"))?;
                Ok(yue_shell::Hotkey::new(
                    command.id(),
                    if chord.ctrl { 2 } else { 0 }
                        | if chord.shift { 4 } else { 0 }
                        | if chord.alt { 1 } else { 0 },
                    vk,
                ))
            })
            .collect()
    }
    pub(crate) fn menu(&self) -> Vec<yue_shell::MenuItem> {
        Command::ALL
            .into_iter()
            .map(|command| {
                let label = match self
                    .rows()
                    .into_iter()
                    .find(|(action, _)| *action == command)
                {
                    Some((_, key)) => format!("{}\t{key}", command.label()),
                    None => command.label().into(),
                };
                yue_shell::MenuItem::new(command.id(), label)
            })
            .collect()
    }
}
pub(crate) fn failure_message(command: u32, error: u32, keys: &Shortcuts) -> String {
    let action = Command::from_id(command);
    let key = keys
        .rows()
        .into_iter()
        .find(|(action, _)| action.id() == command)
        .map(|(_, key)| key)
        .unwrap_or("未知组合");
    format!(
        "{}快捷键 {key} 注册失败（Windows 错误 {error}）；释放占用后将自动重试，也可从托盘执行",
        action.map(Command::label).unwrap_or("全局")
    )
}

fn parse(value: &str) -> Result<Chord, String> {
    let chord = Chord::parse(value).map_err(|error| error.to_string())?;
    let Trigger::Key(key) = chord.trigger else {
        return Err("全局快捷键必须为键盘组合".into());
    };
    if chord.hold.is_some() || (!chord.ctrl && !chord.alt && !matches!(key, Key::F(_))) {
        return Err("全局快捷键需包含 Ctrl/Alt，或使用功能键 F1–F24".into());
    }
    // Reserved system combinations cannot be made reliable by RegisterHotKey.
    if (chord.alt && !chord.ctrl && matches!(key, Key::F(4) | Key::Tab))
        || (chord.ctrl && !chord.alt && key == Key::Escape)
    {
        return Err("该组合由 Windows 保留".into());
    }
    Ok(chord)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_name_real_actions_and_do_not_claim_f3_or_f9() {
        let keys = Shortcuts::default();
        keys.validate(&Keymap::default()).unwrap();
        assert_eq!(
            keys.rows()
                .map(|(action, key)| (action.action_id().unwrap(), key)),
            [
                ("capture.region", "F5"),
                ("capture.scroll", "F7"),
                ("pin.latest", "F6")
            ]
        );
        assert_eq!(keys.hotkeys().unwrap().len(), 3);
        assert_eq!(keys.menu().len(), 10);
    }
    #[test]
    fn global_keys_reject_duplicates_typing_system_and_overlay_conflicts() {
        for value in [
            "A",
            "Shift+A",
            "Ctrl+S",
            "Esc",
            "Ctrl+Escape",
            "Alt+F4",
            "Wheel",
            "Z+Wheel",
        ] {
            let keys = Shortcuts {
                capture: value.into(),
                ..Default::default()
            };
            assert!(keys.validate(&Keymap::default()).is_err(), "{value}");
        }
        assert!(
            Shortcuts {
                capture: "F6".into(),
                ..Default::default()
            }
            .validate(&Keymap::default())
            .is_err()
        );
        Shortcuts {
            capture: "Ctrl+Shift+P".into(),
            ..Default::default()
        }
        .validate(&Keymap::default())
        .unwrap();
    }
    #[test]
    fn the_second_hotkey_is_registered_with_the_same_discipline() {
        let keys = Shortcuts::default().hotkeys().unwrap();
        assert_eq!(
            keys.iter()
                .map(|key| (key.command(), key.modifiers(), key.virtual_key()))
                .collect::<Vec<_>>(),
            vec![(2, 0, 0x74), (10, 0, 0x76), (3, 0, 0x75)]
        );
        assert_eq!(Command::from_id(10), Some(Command::Scroll));
    }
    #[test]
    fn escape_and_enter_are_not_global_hotkeys() {
        for key in Shortcuts::default().hotkeys().unwrap() {
            assert!(![0x1b, 0x0d, 0x78].contains(&key.virtual_key()));
        }
        let map = Keymap::default();
        assert_eq!(map.key_text(Action::Confirm), "Enter");
        assert_eq!(map.key_text(Action::Cancel), "Esc");
    }
    #[test]
    fn each_conflict_message_names_its_own_key() {
        let keys = Shortcuts {
            capture: "Ctrl+Shift+P".into(),
            ..Default::default()
        };
        for (action, key) in keys.rows() {
            let message = super::failure_message(action.id(), 1409, &keys);
            assert!(message.contains(key));
            assert!(message.contains(action.label()));
            assert!(message.contains("1409"));
        }
    }
}

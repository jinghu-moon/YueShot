//! Stable product commands shared by startup, tray, hotkeys and window controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Show,
    Capture,
    Scroll,
    Pin,
    Settings,
    ShowPins,
    HidePins,
    ClosePins,
    UndoPin,
    Quit,
}
impl Command {
    pub(super) const ALL: [Self; 10] = [
        Self::Show,
        Self::Capture,
        Self::Scroll,
        Self::Pin,
        Self::Settings,
        Self::ShowPins,
        Self::HidePins,
        Self::ClosePins,
        Self::UndoPin,
        Self::Quit,
    ];
    pub(super) fn id(self) -> u32 {
        match self {
            Self::Show => 1,
            Self::Capture => 2,
            Self::Pin => 3,
            Self::Settings => 4,
            Self::ShowPins => 5,
            Self::HidePins => 6,
            Self::ClosePins => 7,
            Self::UndoPin => 8,
            Self::Quit => 9,
            Self::Scroll => 10,
        }
    }
    pub(super) fn from_id(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|command| command.id() == id)
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Show => "打开 YueShot…",
            Self::Capture => "开始截图",
            Self::Scroll => "开始长截图",
            Self::Pin => "贴出最近截图",
            Self::Settings => "设置…",
            Self::ShowPins => "显示全部贴图",
            Self::HidePins => "隐藏全部贴图",
            Self::ClosePins => "关闭全部贴图",
            Self::UndoPin => "撤销关闭贴图",
            Self::Quit => "退出 YueShot",
        }
    }
    pub(crate) fn action_id(self) -> Option<&'static str> {
        match self {
            Self::Capture => Some("capture.region"),
            Self::Scroll => Some("capture.scroll"),
            Self::Pin => Some("pin.latest"),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn product_commands_have_unique_roundtripping_ids() {
        let mut ids = std::collections::HashSet::new();
        for command in Command::ALL {
            assert!(ids.insert(command.id()));
            assert_eq!(Command::from_id(command.id()), Some(command));
            assert!(!command.label().is_empty());
        }
        assert_eq!(Command::from_id(0), None);
        assert_eq!(Command::from_id(999), None);
    }
}

use crate::{YueTheme, theme::keys, tokens::rems};

/// Preferred width; viewport constraints always take precedence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DialogSize {
    Small,
    #[default]
    Medium,
    Large,
    ExtraLarge,
}
impl DialogSize {
    pub(super) fn width(self, theme: YueTheme) -> gpui_kit::Rems {
        rems(
            theme,
            match self {
                Self::Small => keys::DIALOG_WIDTH_SM,
                Self::Medium => keys::DIALOG_WIDTH_MD,
                Self::Large => keys::DIALOG_WIDTH_LG,
                Self::ExtraLarge => keys::DIALOG_WIDTH_XL,
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DialogFooter {
    #[default]
    Standard,
    ConfirmOnly,
    None,
}

/// Optional emphasis; it does not change the accessibility role or dismissal policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogStatus {
    Info,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogCloseReason {
    Confirm,
    CancelButton,
    CloseButton,
    Escape,
    OverlayPress,
    Programmatic,
}

/// The result of the host's confirmation callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogDecision {
    Close,
    KeepOpen,
    Pending,
}

/// Terminal result of one asynchronous submission. Business errors stay with the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogOutcome {
    Close,
    KeepOpen,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogChange {
    pub(super) open: bool,
    pub(super) reason: Option<DialogCloseReason>,
}
impl DialogChange {
    pub fn is_open(&self) -> bool {
        self.open
    }
    /// Opening has no close reason.
    pub fn reason(&self) -> Option<DialogCloseReason> {
        self.reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum YueDialogEvent {
    OpenChanged(DialogChange),
    Opened,
    Closed(DialogChange),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogOpenError {
    MissingRoot,
    NotMounted,
    WrongWindow,
    Occupied,
    InvalidParent,
}
impl std::fmt::Display for DialogOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingRoot => {
                "initialize Yue and create the window's Base Root before opening a dialog"
            }
            Self::NotMounted => "keep the dialog declaration mounted before opening it",
            Self::WrongWindow => "a dialog state belongs to one mounted window",
            Self::Occupied => {
                "another modal occupies this window; use open_child for an explicit child decision"
            }
            Self::InvalidParent => "the parent must be this window's open topmost dialog",
        })
    }
}
impl std::error::Error for DialogOpenError {}

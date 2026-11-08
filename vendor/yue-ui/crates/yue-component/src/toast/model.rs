use crate::YueIcon;
use gpui_kit::{App, SharedString, Window};
use std::{rc::Rc, time::Duration};

/// A business key, scoped to a single window. Tickets also carry a generation.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ToastId(pub(super) SharedString);
impl ToastId {
    pub fn new(key: impl Into<SharedString>) -> Self {
        Self(key.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToastKind {
    #[default]
    Normal,
    Success,
    Info,
    Warning,
    Error,
    Loading,
}
impl ToastKind {
    pub(super) fn icon(self) -> Option<YueIcon> {
        match self {
            Self::Normal => None,
            Self::Success => Some(YueIcon::Success),
            Self::Info => Some(YueIcon::Info),
            Self::Warning => Some(YueIcon::Warning),
            Self::Error => Some(YueIcon::Error),
            Self::Loading => Some(YueIcon::Loading),
        }
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Normal => "通知",
            Self::Success => "成功",
            Self::Info => "信息",
            Self::Warning => "警告",
            Self::Error => "错误",
            Self::Loading => "正在处理",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToastAppearance {
    #[default]
    Standard,
    Rich,
    Inverse,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToastAnnouncement {
    #[default]
    Polite,
    Assertive,
    Silent,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastDecision {
    Close,
    KeepOpen,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastDismissReason {
    Timeout,
    CloseButton,
    Keyboard,
    Action,
    Programmatic,
    ScopeDisposed,
    WindowClosed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastError {
    Unavailable,
    WrongWindow,
    InvalidTitle,
    InvalidAction,
    InvalidOptions,
    Capacity,
    Occupied,
    Missing,
    Stale,
    AlreadyClosing,
}
impl std::fmt::Display for ToastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Toast: {self:?}")
    }
}
impl std::error::Error for ToastError {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToastPosition {
    TopLeft,
    Top,
    TopRight,
    Right,
    #[default]
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}
impl ToastPosition {
    pub const ALL: [Self; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Bottom,
        Self::BottomLeft,
        Self::Left,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "左上",
            Self::Top => "上",
            Self::TopRight => "右上",
            Self::Right => "右",
            Self::BottomRight => "右下",
            Self::Bottom => "下",
            Self::BottomLeft => "左下",
            Self::Left => "左",
        }
    }
    pub(super) fn bottom(self) -> bool {
        matches!(self, Self::Bottom | Self::BottomLeft | Self::BottomRight)
    }
    pub(super) fn side(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

type ActionHandler = Rc<dyn Fn(&super::ToastTicket, &mut Window, &mut App) -> ToastDecision>;
/// A card has exactly one optional business action, separate from its close control.
#[derive(Clone)]
pub struct ToastAction {
    pub(super) label: SharedString,
    pub(super) handler: ActionHandler,
}
impl ToastAction {
    pub fn new(
        label: impl Into<SharedString>,
        handler: impl Fn(&super::ToastTicket, &mut Window, &mut App) -> ToastDecision + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            handler: Rc::new(handler),
        }
    }
}

/// Immutable presentation data. Replacing it never implicitly retains an old action.
#[derive(Clone)]
pub struct ToastSpec {
    pub(super) id: Option<ToastId>,
    pub(super) title: SharedString,
    pub(super) description: Option<SharedString>,
    pub(super) kind: ToastKind,
    pub(super) icon: Option<Option<YueIcon>>,
    pub(super) action: Option<ToastAction>,
    pub(super) timeout: Option<Option<Duration>>,
    pub(super) dismissible: Option<bool>,
    pub(super) progress: bool,
    pub(super) appearance: ToastAppearance,
    pub(super) announcement: ToastAnnouncement,
    pub(super) announcement_text: Option<SharedString>,
}
impl ToastSpec {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            id: None,
            title: title.into(),
            description: None,
            kind: ToastKind::Normal,
            icon: None,
            action: None,
            timeout: None,
            dismissible: None,
            progress: false,
            appearance: ToastAppearance::Standard,
            announcement: ToastAnnouncement::Polite,
            announcement_text: None,
        }
    }
    pub fn id(mut self, key: impl Into<SharedString>) -> Self {
        self.id = Some(ToastId::new(key));
        self
    }
    pub fn kind(mut self, kind: ToastKind) -> Self {
        self.kind = kind;
        self
    }
    pub fn description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }
    pub fn icon(mut self, icon: Option<YueIcon>) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn action(mut self, action: ToastAction) -> Self {
        self.action = Some(action);
        self
    }
    /// `None` explicitly means persistent; absence of this builder uses the kind default.
    pub fn timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = Some(timeout);
        self
    }
    pub fn dismissible(mut self, value: bool) -> Self {
        self.dismissible = Some(value);
        self
    }
    pub fn progress(mut self, value: bool) -> Self {
        self.progress = value;
        self
    }
    pub fn appearance(mut self, value: ToastAppearance) -> Self {
        self.appearance = value;
        self
    }
    pub fn announcement(mut self, value: ToastAnnouncement) -> Self {
        self.announcement = value;
        self
    }
    /// Override the complete spoken text, including a localized status prefix.
    pub fn announcement_text(mut self, text: impl Into<SharedString>) -> Self {
        self.announcement_text = Some(text.into());
        self
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn toast_kind(&self) -> ToastKind {
        self.kind
    }
    pub(super) fn can_close(&self) -> bool {
        self.dismissible.unwrap_or(self.kind != ToastKind::Loading)
    }
    pub(super) fn glyph(&self) -> Option<YueIcon> {
        self.icon.unwrap_or_else(|| self.kind.icon())
    }
    pub(super) fn lifetime(&self) -> Option<Duration> {
        self.timeout.unwrap_or_else(|| {
            if self.kind == ToastKind::Loading || self.action.is_some() {
                None
            } else {
                Some(Duration::from_secs(
                    if matches!(self.kind, ToastKind::Warning | ToastKind::Error) {
                        8
                    } else {
                        4
                    },
                ))
            }
        })
    }
    pub(super) fn validate(&self) -> Result<(), ToastError> {
        if self
            .announcement_text
            .as_ref()
            .is_some_and(|t| t.trim().is_empty())
        {
            return Err(ToastError::InvalidOptions);
        }
        if self.title.trim().is_empty() {
            return Err(ToastError::InvalidTitle);
        }
        if self
            .action
            .as_ref()
            .is_some_and(|a| a.label.trim().is_empty())
        {
            return Err(ToastError::InvalidAction);
        }
        if self.timeout.flatten().is_some_and(|t| t.is_zero()) {
            return Err(ToastError::InvalidOptions);
        }
        Ok(())
    }
    pub(super) fn spoken(&self) -> SharedString {
        if let Some(text) = &self.announcement_text {
            return text.clone();
        }
        format!(
            "{}：{}{}",
            self.kind.label(),
            self.title,
            self.description
                .as_ref()
                .map_or(String::new(), |d| format!("。{d}"))
        )
        .into()
    }
}

/// Extra occupied space inside the window, in logical pixels (not screen coordinates).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToastInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}
#[derive(Clone, Debug)]
pub struct ToasterOptions {
    pub(super) position: ToastPosition,
    pub(super) capacity: usize,
    pub(super) visible: usize,
    pub(super) insets: ToastInsets,
    pub(super) close_label: SharedString,
    pub(super) region_label: SharedString,
}
impl Default for ToasterOptions {
    fn default() -> Self {
        Self {
            position: ToastPosition::default(),
            capacity: 32,
            visible: 3,
            insets: ToastInsets::default(),
            close_label: "关闭通知".into(),
            region_label: "通知".into(),
        }
    }
}
impl ToasterOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn position(mut self, p: ToastPosition) -> Self {
        self.position = p;
        self
    }
    pub fn capacity(mut self, n: usize) -> Self {
        self.capacity = n;
        self
    }
    pub fn visible_toasts(mut self, n: usize) -> Self {
        self.visible = n;
        self
    }
    pub fn insets(mut self, insets: ToastInsets) -> Self {
        self.insets = insets;
        self
    }
    pub fn close_label(mut self, label: impl Into<SharedString>) -> Self {
        self.close_label = label.into();
        self
    }
    pub fn region_label(mut self, label: impl Into<SharedString>) -> Self {
        self.region_label = label.into();
        self
    }
    pub fn toast_position(&self) -> ToastPosition {
        self.position
    }
    pub fn max_toasts(&self) -> usize {
        self.capacity
    }
    pub fn visible_count(&self) -> usize {
        self.visible
    }
    pub fn safe_insets(&self) -> ToastInsets {
        self.insets
    }
    pub fn close_accessible_label(&self) -> &str {
        &self.close_label
    }
    pub fn region_accessible_label(&self) -> &str {
        &self.region_label
    }
    pub(super) fn validate(&self) -> bool {
        self.capacity > 0
            && self.visible > 0
            && !self.close_label.trim().is_empty()
            && !self.region_label.trim().is_empty()
            && [
                self.insets.top,
                self.insets.right,
                self.insets.bottom,
                self.insets.left,
            ]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.)
    }
}

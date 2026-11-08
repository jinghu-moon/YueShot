use crate::{Icon, IconDefinition};
use gpui_kit::{EntityId, SharedString};
use std::{collections::HashSet, fmt};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectMode {
    #[default]
    Single,
    Multiple,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectValue {
    Single(Option<SharedString>),
    Multiple(Vec<SharedString>),
}
impl SelectValue {
    pub fn keys(&self) -> &[SharedString] {
        match self {
            Self::Single(key) => key.as_slice(),
            Self::Multiple(keys) => keys,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.keys().is_empty()
    }
    pub fn contains(&self, key: &str) -> bool {
        self.keys().iter().any(|k| k == key)
    }
    pub(super) fn empty(mode: SelectMode) -> Self {
        match mode {
            SelectMode::Single => Self::Single(None),
            SelectMode::Multiple => Self::Multiple(vec![]),
        }
    }
    pub(super) fn validate(
        &self,
        mode: SelectMode,
        limit: Option<usize>,
    ) -> Result<(), SelectError> {
        if !matches!(
            (self, mode),
            (Self::Single(_), SelectMode::Single) | (Self::Multiple(_), SelectMode::Multiple)
        ) {
            return Err(SelectError::ModeMismatch);
        }
        let mut keys = HashSet::new();
        for k in self.keys() {
            validate_text(k)?;
            if !keys.insert(k) {
                return Err(SelectError::DuplicateKey(k.clone()));
            }
        }
        if limit.is_some_and(|limit| self.keys().len() > limit) {
            return Err(SelectError::SelectionLimit);
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct SelectOption {
    pub(super) key: SharedString,
    pub(super) label: SharedString,
    pub(super) disabled: bool,
    pub(super) icon: Option<Icon>,
    pub(super) description: Option<SharedString>,
    pub(super) keywords: Vec<SharedString>,
    pub(super) meta: Option<SharedString>,
}
impl SelectOption {
    pub fn new(key: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            disabled: false,
            icon: None,
            description: None,
            keywords: vec![],
            meta: None,
        }
    }
    pub fn key(&self) -> &SharedString {
        &self.key
    }
    pub fn label(&self) -> &SharedString {
        &self.label
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
    pub fn description(&self) -> Option<&SharedString> {
        self.description.as_ref()
    }
    pub fn meta(&self) -> Option<&SharedString> {
        self.meta.as_ref()
    }
    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }
    pub fn leading_icon(mut self, icon: impl IconDefinition) -> Self {
        self.icon = Some(Icon::new(icon));
        self
    }
    pub fn with_description(mut self, value: impl Into<SharedString>) -> Self {
        self.description = Some(value.into());
        self
    }
    pub fn keywords(mut self, words: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.keywords = words.into_iter().map(Into::into).collect();
        self
    }
    pub fn with_meta(mut self, value: impl Into<SharedString>) -> Self {
        self.meta = Some(value.into());
        self
    }
}
#[derive(Clone)]
pub struct SelectGroup {
    pub(super) key: SharedString,
    pub(super) label: SharedString,
    pub(super) options: Vec<SelectOption>,
}
impl SelectGroup {
    pub fn new(
        key: impl Into<SharedString>,
        label: impl Into<SharedString>,
        options: impl IntoIterator<Item = SelectOption>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            options: options.into_iter().collect(),
        }
    }
    pub fn key(&self) -> &SharedString {
        &self.key
    }
    pub fn label(&self) -> &SharedString {
        &self.label
    }
    pub fn options(&self) -> &[SelectOption] {
        &self.options
    }
}
#[derive(Clone)]
pub enum SelectEntry {
    Option(SelectOption),
    Group(SelectGroup),
}
impl From<SelectOption> for SelectEntry {
    fn from(value: SelectOption) -> Self {
        Self::Option(value)
    }
}
impl From<SelectGroup> for SelectEntry {
    fn from(value: SelectGroup) -> Self {
        Self::Group(value)
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectSearch {
    #[default]
    None,
    Local,
    Remote,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectSize {
    Small,
    #[default]
    Medium,
    Large,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectVariant {
    #[default]
    Outline,
    Text,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SelectPanelWidth {
    #[default]
    MatchTrigger,
    Content,
    Fixed(gpui_kit::Pixels),
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectTagLayout {
    Wrap,
    Auto,
    #[default]
    Collapse,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectRowLayout {
    #[default]
    SingleLine,
    Description,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectVirtualization {
    #[default]
    Auto,
    Always,
    Never,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectChangeSource {
    Pointer,
    Keyboard,
    TagRemove,
    Clear,
    Create,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectCloseReason {
    Trigger,
    Confirm,
    Escape,
    Tab,
    OutsidePress,
    Lifecycle,
}
#[derive(Clone)]
pub struct SelectChange {
    pub(super) before: SelectValue,
    pub(super) after: SelectValue,
    pub(super) source: SelectChangeSource,
    pub(super) items: Vec<SelectOption>,
}
impl SelectChange {
    pub fn before(&self) -> &SelectValue {
        &self.before
    }
    pub fn after(&self) -> &SelectValue {
        &self.after
    }
    pub fn source(&self) -> SelectChangeSource {
        self.source
    }
    pub fn items(&self) -> &[SelectOption] {
        &self.items
    }
}
#[derive(Clone, Debug)]
pub struct SelectOpenChange {
    pub(super) open: bool,
    pub(super) reason: SelectCloseReason,
}
impl SelectOpenChange {
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn reason(&self) -> SelectCloseReason {
        self.reason
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RequestKind {
    Search,
    More,
    Create,
}
/// A capability belonging to one state, query session and operation. Constructed
/// only by the component; cancellation alone is not sufficient race protection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectRequestTicket {
    pub(super) owner: EntityId,
    pub(super) session: u64,
    pub(super) generation: u64,
    pub(super) serial: u64,
    pub(super) selection: u64,
    pub(super) kind: RequestKind,
}
#[derive(Clone)]
pub struct SelectRequest {
    pub(super) query: SharedString,
    pub(super) ticket: SelectRequestTicket,
    pub(super) cursor: Option<SharedString>,
}
impl SelectRequest {
    pub fn query(&self) -> &SharedString {
        &self.query
    }
    pub fn ticket(&self) -> &SelectRequestTicket {
        &self.ticket
    }
    pub fn cursor(&self) -> Option<&SharedString> {
        self.cursor.as_ref()
    }
}
#[derive(Clone)]
pub enum SelectEvent {
    Change(SelectChange),
    OpenChange(SelectOpenChange),
    SearchRequested(SelectRequest),
    LoadMoreRequested(SelectRequest),
    CreateRequested(SelectRequest),
}
#[derive(Clone)]
pub struct SelectPage {
    pub(super) entries: Vec<SelectEntry>,
    pub(super) next_cursor: Option<SharedString>,
}
impl SelectPage {
    pub fn new(entries: impl IntoIterator<Item = impl Into<SelectEntry>>) -> Self {
        Self {
            entries: entries.into_iter().map(Into::into).collect(),
            next_cursor: None,
        }
    }
    pub fn next_cursor(mut self, cursor: impl Into<SharedString>) -> Self {
        self.next_cursor = Some(cursor.into());
        self
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectApply {
    Applied,
    Stale,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectError {
    EmptyText,
    DuplicateKey(SharedString),
    DuplicateGroup(SharedString),
    ModeMismatch,
    SelectionLimit,
    InvalidLimit,
    InvalidGeometry,
    InvalidRequest,
    DisabledOption,
    NoProgress,
}
impl fmt::Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SelectError {}
fn validate_text(text: &str) -> Result<(), SelectError> {
    if text.trim().is_empty() {
        Err(SelectError::EmptyText)
    } else {
        Ok(())
    }
}
pub(super) fn validate_entries(entries: &[SelectEntry]) -> Result<(), SelectError> {
    let mut keys = HashSet::new();
    let mut groups = HashSet::new();
    for entry in entries {
        let options = match entry {
            SelectEntry::Option(option) => std::slice::from_ref(option),
            SelectEntry::Group(group) => {
                validate_text(&group.key)?;
                validate_text(&group.label)?;
                if !groups.insert(&group.key) {
                    return Err(SelectError::DuplicateGroup(group.key.clone()));
                }
                &group.options
            }
        };
        for option in options {
            validate_text(&option.key)?;
            validate_text(&option.label)?;
            if !keys.insert(&option.key) {
                return Err(SelectError::DuplicateKey(option.key.clone()));
            }
        }
    }
    Ok(())
}
pub(super) fn options_of(entries: &[SelectEntry]) -> impl Iterator<Item = &SelectOption> {
    entries.iter().flat_map(|entry| match entry {
        SelectEntry::Option(option) => std::slice::from_ref(option),
        SelectEntry::Group(group) => group.options.as_slice(),
    })
}
#[derive(Clone)]
pub(super) enum Row {
    Group(SharedString, SharedString),
    Option(SelectOption),
    Create(SharedString),
}
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(super) enum ActiveKey {
    Option(SharedString),
    Create(SharedString),
}
impl Row {
    pub(super) fn virtual_key(&self) -> gpui_kit::ElementId {
        match self {
            Self::Group(key, _) => (gpui_kit::ElementId::from("group"), key.clone()).into(),
            Self::Option(option) => ActiveKey::Option(option.key.clone()).virtual_key(),
            Self::Create(query) => ActiveKey::Create(query.clone()).virtual_key(),
        }
    }
    pub(super) fn selectable(&self) -> bool {
        match self {
            Self::Option(option) => !option.disabled,
            Self::Create(_) => true,
            Self::Group(..) => false,
        }
    }
    pub(super) fn active_key(&self) -> Option<ActiveKey> {
        match self {
            Self::Option(option) => Some(ActiveKey::Option(option.key.clone())),
            Self::Create(query) => Some(ActiveKey::Create(query.clone())),
            Self::Group(..) => None,
        }
    }
}

impl ActiveKey {
    pub(super) fn virtual_key(&self) -> gpui_kit::ElementId {
        match self {
            Self::Option(key) => (gpui_kit::ElementId::from("option"), key.clone()).into(),
            Self::Create(query) => (gpui_kit::ElementId::from("create"), query.clone()).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_are_identity_not_labels() {
        assert!(
            validate_entries(&[
                SelectOption::new("a", "Same").into(),
                SelectOption::new("b", "Same").into()
            ])
            .is_ok()
        );
    }
    #[test]
    fn duplicate_across_groups_is_rejected() {
        assert!(matches!(
            validate_entries(&[
                SelectOption::new("a", "A").into(),
                SelectGroup::new("g", "Group", [SelectOption::new("a", "B")]).into()
            ]),
            Err(SelectError::DuplicateKey(_))
        ));
    }
    #[test]
    fn invalid_value_is_atomic_input() {
        assert_eq!(
            SelectValue::Single(None).validate(SelectMode::Multiple, None),
            Err(SelectError::ModeMismatch)
        );
        assert_eq!(
            SelectValue::Multiple(vec!["a".into(), "b".into()])
                .validate(SelectMode::Multiple, Some(1)),
            Err(SelectError::SelectionLimit)
        );
    }
    #[test]
    fn duplicate_value_rejected() {
        assert!(matches!(
            SelectValue::Multiple(vec!["a".into(), "a".into()])
                .validate(SelectMode::Multiple, None),
            Err(SelectError::DuplicateKey(_))
        ));
    }
    #[test]
    fn blank_option_and_group_rejected() {
        assert_eq!(
            validate_entries(&[SelectOption::new("a", " ").into()]),
            Err(SelectError::EmptyText)
        );
        assert_eq!(
            validate_entries(&[SelectGroup::new(" ", "Group", []).into()]),
            Err(SelectError::EmptyText)
        );
    }
}

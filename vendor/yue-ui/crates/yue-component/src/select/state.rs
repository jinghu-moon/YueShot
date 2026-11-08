use super::{Presentation, model::*};
use crate::popup::{PopupAnchor, PopupId};
use gpui_kit::base::{
    input::InputState,
    virtualization::{VirtualizerSizing, VirtualizerSource, VirtualizerState},
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EntityInputHandler as _, EventEmitter, FocusHandle,
    Focusable as _, Render, SharedString, Subscription, Task, Window,
};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

type Filter = Rc<dyn Fn(&SelectOption, &str) -> bool>;
pub(super) struct ListGeometry {
    pub(super) rows: Rc<Vec<Row>>,
    pub(super) option_height: gpui_kit::Pixels,
    pub(super) group_height: gpui_kit::Pixels,
    pub(super) total: gpui_kit::Pixels,
    pub(super) option_count: usize,
    pub(super) has_icons: bool,
}
pub(super) struct SelectVirtualSource {
    pub(super) rows: Rc<Vec<Row>>,
    pub(super) positions: Rc<HashMap<gpui_kit::ElementId, usize>>,
}
impl VirtualizerSource for SelectVirtualSource {
    fn len(&self) -> usize {
        self.rows.len()
    }
    fn key_at(&self, index: usize) -> gpui_kit::ElementId {
        self.rows[index].virtual_key()
    }
    fn index_of(&self, key: &gpui_kit::ElementId) -> Option<usize> {
        self.positions.get(key).copied()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectResultState {
    Ready,
    Loading,
    Failed(SharedString),
}

/// Retained selection controller. Search text belongs exclusively to the
/// existing InputState; this entity also renders the composed control so
/// notifications work without requiring consumers to register an observer.
pub struct YueSelectState {
    pub(super) mode: SelectMode,
    pub(super) value: SelectValue,
    pub(super) entries: Vec<SelectEntry>,
    cache: HashMap<SharedString, SelectOption>,
    pub(super) search: SelectSearch,
    pub(super) input: Option<Entity<InputState>>,
    subscriptions: Vec<Subscription>,
    observed_query: SharedString,
    committed_query: SharedString,
    composition_active: bool,
    pub(super) rows: Rc<Vec<Row>>,
    pub(super) active: Option<ActiveKey>,
    pub(super) positions: Rc<HashMap<gpui_kit::ElementId, usize>>,
    pub(super) hovered: Option<ActiveKey>,
    pub(super) open: bool,
    pub(super) collapsed_open: bool,
    pub(super) field_hover: bool,
    pub(super) keyboard_navigation: bool,
    pub(super) hidden_keys: Vec<SharedString>,
    pub(super) measured_width: gpui_kit::Pixels,
    pub(super) disabled: bool,
    pub(super) readonly: bool,
    pub(super) loading: bool,
    pub(super) max_selected: Option<usize>,
    creatable: bool,
    filter: Option<Filter>,
    pub(super) result: SelectResultState,
    pub(super) more_result: SelectResultState,
    pub(super) create_result: SelectResultState,
    next_cursor: Option<SharedString>,
    consumed_cursors: HashSet<SharedString>,
    pub(super) page_size: usize,
    pub(super) slot_heights: [gpui_kit::Pixels; 2],
    pub(super) width_cache: Option<(Rc<Vec<Row>>, gpui_kit::Pixels, String, gpui_kit::Pixels)>,
    pub(super) list_cache: Option<Rc<ListGeometry>>,
    pub(super) focus: FocusHandle,
    pub(super) list_focus: FocusHandle,
    pub(super) collapsed_focus: FocusHandle,
    pub(super) tag_focus: HashMap<SharedString, FocusHandle>,
    pub(super) anchor: PopupAnchor,
    pub(super) collapse_anchor: PopupAnchor,
    pub(super) parent: Option<PopupId>,
    pub(super) scroll: gpui_kit::ScrollHandle,
    pub(super) virtualizer: Entity<VirtualizerState>,
    pub(super) pending_reveal: Option<gpui_kit::ElementId>,
    pub(super) virtualized: bool,
    session: u64,
    generation: u64,
    serial: u64,
    selection_revision: u64,
    pending_search: Option<SelectRequestTicket>,
    pending_more: Option<SelectRequestTicket>,
    pending_create: Option<SelectRequestTicket>,
    debounce: Option<Task<()>>,
    debounce_duration: Duration,
    pub(super) typeahead: SharedString,
    pub(super) typeahead_at: Option<std::time::Instant>,
    pub(super) presentation: Option<Presentation>,
}
impl EventEmitter<SelectEvent> for YueSelectState {}
impl YueSelectState {
    pub fn new(mode: SelectMode, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let virtualizer = cx.new(|_| {
            VirtualizerState::new(
                gpui_kit::Axis::Vertical,
                Rc::new(SelectVirtualSource {
                    rows: Rc::new(Vec::new()),
                    positions: Rc::new(HashMap::new()),
                }),
                VirtualizerSizing::known(Rc::<[gpui_kit::Pixels]>::from([])),
            )
            .expect("valid empty Select source")
        });
        let scroll = virtualizer.read(cx).native_scroll_handle().unwrap().clone();
        Self {
            mode,
            value: SelectValue::empty(mode),
            entries: vec![],
            cache: HashMap::new(),
            search: SelectSearch::None,
            input: None,
            subscriptions: vec![],
            observed_query: "".into(),
            committed_query: "".into(),
            composition_active: false,
            rows: Rc::new(vec![]),
            active: None,
            positions: Rc::new(HashMap::new()),
            hovered: None,
            open: false,
            collapsed_open: false,
            field_hover: false,
            keyboard_navigation: false,
            hidden_keys: vec![],
            measured_width: gpui_kit::px(0.),
            disabled: false,
            readonly: false,
            loading: false,
            max_selected: None,
            creatable: false,
            filter: None,
            result: SelectResultState::Ready,
            more_result: SelectResultState::Ready,
            create_result: SelectResultState::Ready,
            next_cursor: None,
            consumed_cursors: HashSet::new(),
            page_size: 1,
            slot_heights: [gpui_kit::px(0.); 2],
            width_cache: None,
            list_cache: None,
            focus: cx.focus_handle(),
            list_focus: cx.focus_handle().tab_stop(false),
            collapsed_focus: cx.focus_handle().tab_stop(false),
            tag_focus: HashMap::new(),
            anchor: PopupAnchor::default(),
            collapse_anchor: PopupAnchor::default(),
            parent: None,
            scroll,
            virtualizer,
            pending_reveal: None,
            virtualized: false,
            session: 0,
            generation: 0,
            serial: 0,
            selection_revision: 0,
            pending_search: None,
            pending_more: None,
            pending_create: None,
            debounce: None,
            debounce_duration: Duration::from_millis(200),
            typeahead: "".into(),
            typeahead_at: None,
            presentation: None,
        }
    }
    pub fn mode(&self) -> SelectMode {
        self.mode
    }
    pub fn value(&self) -> &SelectValue {
        &self.value
    }
    pub fn options(&self) -> &[SelectEntry] {
        &self.entries
    }
    pub fn query(&self, cx: &App) -> SharedString {
        self.input
            .as_ref()
            .map_or_else(SharedString::default, |input| input.read(cx).value())
    }
    pub fn query_input(&self) -> Option<&Entity<InputState>> {
        self.input.as_ref()
    }
    pub fn active_key(&self) -> Option<&SharedString> {
        match &self.active {
            Some(ActiveKey::Option(key)) => Some(key),
            _ => None,
        }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
    pub fn is_readonly(&self) -> bool {
        self.readonly
    }
    pub fn result_state(&self) -> &SelectResultState {
        &self.result
    }
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }
    pub fn selected_items(&self) -> Vec<SelectOption> {
        self.value
            .keys()
            .iter()
            .map(|key| {
                self.cache.get(key).cloned().unwrap_or_else(|| {
                    SelectOption::new(key.clone(), key.clone()).with_description("选中项尚未解析")
                })
            })
            .collect()
    }
    pub fn set_value(
        &mut self,
        value: SelectValue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), SelectError> {
        value.validate(self.mode, self.max_selected)?;
        if self.value == value {
            return Ok(());
        }
        self.value = value;
        self.selection_revision += 1;
        self.end_session(window, cx);
        self.refresh_cache(cx);
        self.rebuild();
        cx.notify();
        Ok(())
    }
    pub fn set_options(
        &mut self,
        entries: impl IntoIterator<Item = impl Into<SelectEntry>>,
        cx: &mut Context<Self>,
    ) -> Result<(), SelectError> {
        let entries = entries.into_iter().map(Into::into).collect::<Vec<_>>();
        validate_entries(&entries)?;
        self.entries = entries;
        self.invalidate();
        self.result = SelectResultState::Ready;
        self.next_cursor = None;
        self.refresh_cache(cx);
        self.rebuild();
        cx.notify();
        Ok(())
    }
    /// Hydrate only currently selected identities, without changing their keys.
    pub fn set_selected_items(
        &mut self,
        items: impl IntoIterator<Item = SelectOption>,
        cx: &mut Context<Self>,
    ) -> Result<(), SelectError> {
        let items = items.into_iter().collect::<Vec<_>>();
        validate_entries(
            &items
                .iter()
                .cloned()
                .map(SelectEntry::Option)
                .collect::<Vec<_>>(),
        )?;
        for item in items {
            if self.value.contains(&item.key) {
                self.cache.insert(item.key.clone(), item);
            }
        }
        self.refresh_tag_focus(cx);
        cx.notify();
        Ok(())
    }
    pub fn set_max_selected(
        &mut self,
        limit: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Result<(), SelectError> {
        if limit == Some(0) {
            return Err(SelectError::InvalidLimit);
        }
        if self.mode == SelectMode::Single && limit.is_some() {
            return Err(SelectError::ModeMismatch);
        }
        self.value.validate(self.mode, limit)?;
        self.max_selected = limit;
        self.rebuild();
        cx.notify();
        Ok(())
    }
    pub fn set_disabled(&mut self, value: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled != value {
            self.disabled = value;
            if value {
                self.end_session(window, cx);
            }
            cx.notify();
        }
    }
    pub fn set_readonly(&mut self, value: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly != value {
            self.readonly = value;
            if value {
                self.end_session(window, cx);
            }
            cx.notify();
        }
    }
    pub fn set_loading(&mut self, value: bool, cx: &mut Context<Self>) {
        if self.loading != value {
            self.loading = value;
            cx.notify();
        }
    }
    pub fn set_search_debounce(&mut self, duration: Duration) {
        self.debounce_duration = duration;
    }
    pub fn set_creatable(&mut self, value: bool, cx: &mut Context<Self>) {
        self.creatable = value;
        self.rebuild();
        cx.notify();
    }
    pub fn set_filter(
        &mut self,
        filter: impl Fn(&SelectOption, &str) -> bool + 'static,
        cx: &mut Context<Self>,
    ) {
        self.filter = Some(Rc::new(filter));
        self.rebuild();
        cx.notify();
    }
    pub fn set_search_mode(
        &mut self,
        search: SelectSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search == search {
            return;
        }
        self.end_session(window, cx);
        self.search = search;
        if search != SelectSearch::None && self.input.is_none() {
            let input = cx.new(|cx| InputState::new(window, cx));
            let focus = input.focus_handle(cx);
            let _ = focus.tab_stop(false);
            self.subscriptions
                .push(cx.observe_in(&input, window, |state, input, window, cx| {
                    state.query_changed(&input, window, cx);
                }));
            self.input = Some(input);
        }
        self.rebuild();
        cx.notify();
    }
    /// Silent external synchronization. User opening uses the same invariant
    /// path but emits OpenChange and transfers focus through Base Select.
    pub fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.change_open(open, SelectCloseReason::Trigger, false, window, cx);
    }
    pub(super) fn change_open(
        &mut self,
        open: bool,
        reason: SelectCloseReason,
        emit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if open && !self.editable() {
            return false;
        }
        if self.open == open {
            if !open && self.collapsed_open {
                self.end_session(window, cx);
                cx.notify();
            }
            return open;
        }
        if open {
            self.parent = self.anchor.handle().bounds().and_then(|bounds| {
                crate::popup::PopupRuntime::for_window(window, cx).parent_at(bounds.center())
            });
            self.collapsed_open = false;
            self.open = true;
            self.session += 1;
            self.hovered = None;
            self.rebuild();
            self.choose_initial();
            if self.search == SelectSearch::Remote {
                self.start_search(cx);
            }
        } else {
            if matches!(
                reason,
                SelectCloseReason::OutsidePress | SelectCloseReason::Lifecycle
            ) && self.owns_focus(window, cx)
            {
                window.blur(cx);
            }
            self.end_session(window, cx);
        }
        if emit {
            cx.emit(SelectEvent::OpenChange(SelectOpenChange {
                open: self.open,
                reason,
            }));
        }
        cx.notify();
        self.open
    }
    pub(super) fn close(
        &mut self,
        reason: SelectCloseReason,
        restore: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_open(false, reason, true, window, cx);
        if restore && !self.disabled {
            self.focus.focus(window, cx);
        }
    }
    pub(super) fn editable(&self) -> bool {
        !self.disabled && !self.readonly
    }
    pub(super) fn content_focus(&self, cx: &App) -> FocusHandle {
        if self.search != SelectSearch::None {
            self.input.as_ref().expect("search input").focus_handle(cx)
        } else {
            self.list_focus.clone()
        }
    }
    pub(super) fn composing(&self, window: &mut Window, cx: &mut App) -> bool {
        self.input.as_ref().is_some_and(|input| {
            input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        })
    }
    fn query_changed(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = input.read(cx).value();
        if self.composing(window, cx) {
            if !self.composition_active {
                self.invalidate();
                self.composition_active = true;
            }
            return;
        }
        let ended_composition = std::mem::replace(&mut self.composition_active, false);
        if query == self.observed_query && !ended_composition {
            return;
        }
        self.observed_query = query.clone();
        self.invalidate();
        self.committed_query = query.trim().into();
        self.active = None;
        self.rebuild();
        if self.open && self.search == SelectSearch::Remote {
            self.result = SelectResultState::Loading;
            self.rows = Rc::new(vec![]);
            self.positions = Rc::new(HashMap::new());
            self.active = None;
            let generation = self.generation;
            let delay = self.debounce_duration;
            self.debounce = Some(cx.spawn_in(window, async move |state, cx| {
                cx.background_executor().timer(delay).await;
                let _ = state.update_in(cx, |state, _, cx| {
                    if state.generation == generation && state.open && state.editable() {
                        state.start_search(cx);
                    }
                });
            }));
        }
        cx.notify();
    }
    fn invalidate(&mut self) {
        self.generation += 1;
        self.pending_search = None;
        self.pending_more = None;
        self.pending_create = None;
        self.debounce = None;
        self.more_result = SelectResultState::Ready;
        self.create_result = SelectResultState::Ready;
    }
    fn end_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.owns_focus(window, cx) {
            if self.disabled {
                window.blur(cx);
            } else {
                self.focus.focus(window, cx);
            }
        }
        self.open = false;
        self.collapsed_open = false;
        self.session += 1;
        self.invalidate();
        self.result = SelectResultState::Ready;
        self.active = None;
        self.hovered = None;
        self.typeahead = "".into();
        self.typeahead_at = None;
        self.observed_query = "".into();
        self.composition_active = false;
        self.committed_query = "".into();
        if let Some(input) = &self.input {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.rebuild();
    }
    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
            || self.list_focus.contains_focused(window, cx)
            || self.collapsed_focus.contains_focused(window, cx)
            || self
                .input
                .as_ref()
                .is_some_and(|input| input.focus_handle(cx).contains_focused(window, cx))
            || self
                .tag_focus
                .values()
                .any(|handle| handle.contains_focused(window, cx))
            || self.presentation.as_ref().is_some_and(|p| {
                p.header_focus
                    .iter()
                    .chain(&p.footer_focus)
                    .any(|handle| handle.contains_focused(window, cx))
            })
    }
    fn refresh_cache(&mut self, cx: &mut Context<Self>) {
        self.cache.retain(|key, _| self.value.contains(key));
        for option in options_of(&self.entries) {
            if self.value.contains(&option.key) {
                self.cache.insert(option.key.clone(), option.clone());
            }
        }
        self.refresh_tag_focus(cx);
    }
    fn refresh_tag_focus(&mut self, cx: &mut Context<Self>) {
        self.tag_focus.retain(|key, _| self.value.contains(key));
        for key in self.value.keys() {
            self.tag_focus
                .entry(key.clone())
                .or_insert_with(|| cx.focus_handle());
        }
    }
    pub(super) fn removable(&self, key: &str) -> bool {
        self.editable()
            && self.value.contains(key)
            && !self.cache.get(key).is_some_and(|option| option.disabled)
    }
    pub(super) fn at_limit(&self) -> bool {
        self.max_selected
            .is_some_and(|limit| self.value.keys().len() >= limit)
    }
    pub(super) fn eligible(&self, row: &Row) -> bool {
        row.selectable()
            && match row {
                Row::Option(option) => self.value.contains(&option.key) || !self.at_limit(),
                Row::Create(_) => {
                    !self.at_limit() && !matches!(self.create_result, SelectResultState::Loading)
                }
                _ => false,
            }
    }
    pub(super) fn rebuild(&mut self) {
        let previous_active = self.active.clone();
        let previous_position = self
            .active
            .as_ref()
            .and_then(|key| self.positions.get(&key.virtual_key()))
            .copied();
        let query = self.committed_query.to_lowercase();
        let filter = self.filter.clone();
        let search = self.search;
        let matches = |option: &SelectOption| {
            search != SelectSearch::Local
                || query.is_empty()
                || filter.as_ref().map_or_else(
                    || {
                        option.label.to_lowercase().contains(&query)
                            || option
                                .keywords
                                .iter()
                                .any(|word| word.to_lowercase().contains(&query))
                    },
                    |filter| filter(option, &self.committed_query),
                )
        };
        let mut rows = vec![];
        if self.search != SelectSearch::Remote
            || !matches!(
                self.result,
                SelectResultState::Loading | SelectResultState::Failed(_)
            )
        {
            for entry in &self.entries {
                match entry {
                    SelectEntry::Option(option) => {
                        if matches(option) {
                            rows.push(Row::Option(option.clone()));
                        }
                    }
                    SelectEntry::Group(group) => {
                        let options = group
                            .options
                            .iter()
                            .filter(|option| matches(option))
                            .cloned()
                            .collect::<Vec<_>>();
                        if !options.is_empty() {
                            rows.push(Row::Group(group.key.clone(), group.label.clone()));
                            rows.extend(options.into_iter().map(Row::Option));
                        }
                    }
                }
            }
            if self.creatable
                && self.search != SelectSearch::None
                && !query.is_empty()
                && !self.at_limit()
                && !options_of(&self.entries).any(|option| option.label.to_lowercase() == query)
            {
                rows.push(Row::Create(self.committed_query.clone()));
            }
        }
        self.positions = Rc::new(
            rows.iter()
                .enumerate()
                .map(|(ix, row)| (row.virtual_key(), ix))
                .collect(),
        );
        if self.active.as_ref().is_none_or(|key| {
            !self
                .positions
                .get(&key.virtual_key())
                .is_some_and(|ix| self.eligible(&rows[*ix]))
        }) {
            self.active = rows
                .iter()
                .find(|row| self.eligible(row))
                .and_then(Row::active_key);
        }
        self.rows = Rc::new(rows);
        let current_position = self
            .active
            .as_ref()
            .and_then(|key| self.positions.get(&key.virtual_key()))
            .copied();
        if self.open && (self.active != previous_active || current_position != previous_position) {
            self.scroll_active();
        }
    }
    fn choose_initial(&mut self) {
        self.active=self.rows.iter().find(|row|matches!(row,Row::Option(option) if !option.disabled&&self.value.contains(&option.key))).or_else(||self.rows.iter().find(|row|self.eligible(row))).and_then(Row::active_key);
        self.scroll_active();
    }
    pub(super) fn scroll_active(&mut self) {
        if let Some(ix) = self
            .active
            .as_ref()
            .and_then(|key| self.positions.get(&key.virtual_key()))
        {
            self.pending_reveal = self.active.as_ref().map(ActiveKey::virtual_key);
            if !self.virtualized {
                self.scroll.scroll_to_item(*ix);
            }
        }
    }
    pub(super) fn move_active(&mut self, movement: super::behavior::Move, cx: &mut Context<Self>) {
        let eligible = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| self.eligible(row))
            .map(|(ix, _)| ix)
            .collect::<Vec<_>>();
        let current = self
            .active
            .as_ref()
            .and_then(|key| self.positions.get(&key.virtual_key()))
            .and_then(|ix| eligible.iter().position(|row| row == ix));
        if let Some(slot) = super::behavior::move_slot(current, eligible.len(), movement) {
            self.active = self.rows[eligible[slot]].active_key();
            self.hovered = None;
            self.scroll_active();
            cx.notify();
        }
    }
    pub(super) fn activate(
        &mut self,
        key: ActiveKey,
        source: SelectChangeSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editable() || !self.open || self.composing(window, cx) {
            return;
        }
        let Some(row) = self
            .positions
            .get(&key.virtual_key())
            .map(|ix| self.rows[*ix].clone())
        else {
            return;
        };
        if !self.eligible(&row) {
            return;
        }
        match row {
            Row::Option(option) => {
                let value = match &self.value {
                    SelectValue::Single(_) => SelectValue::Single(Some(option.key.clone())),
                    SelectValue::Multiple(keys) => {
                        let mut keys = keys.clone();
                        if keys.contains(&option.key) {
                            keys.retain(|key| key != &option.key);
                        } else {
                            keys.push(option.key.clone());
                        }
                        SelectValue::Multiple(keys)
                    }
                };
                self.commit(value, source, cx);
                if self.mode == SelectMode::Single {
                    self.close(SelectCloseReason::Confirm, true, window, cx);
                } else {
                    self.list_focus.focus(window, cx);
                }
            }
            Row::Create(_) => self.request_create(cx),
            Row::Group(..) => {}
        }
    }
    fn commit(&mut self, value: SelectValue, source: SelectChangeSource, cx: &mut Context<Self>) {
        if value == self.value {
            return;
        }
        let before = std::mem::replace(&mut self.value, value);
        self.selection_revision += 1;
        self.pending_create = None;
        self.create_result = SelectResultState::Ready;
        self.refresh_cache(cx);
        self.rebuild();
        cx.emit(SelectEvent::Change(SelectChange {
            before,
            after: self.value.clone(),
            source,
            items: self.selected_items(),
        }));
        cx.notify();
    }
    pub(super) fn remove(
        &mut self,
        key: &str,
        source: SelectChangeSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.removable(key) {
            return;
        }
        let next = self
            .value
            .keys()
            .iter()
            .position(|item| item == key)
            .unwrap_or(0);
        let value = match &self.value {
            SelectValue::Single(_) => SelectValue::Single(None),
            SelectValue::Multiple(keys) => SelectValue::Multiple(
                keys.iter()
                    .filter(|item| item.as_str() != key)
                    .cloned()
                    .collect(),
            ),
        };
        self.commit(value, source, cx);
        if self.collapsed_open {
            self.collapsed_focus.focus(window, cx);
            return;
        }
        let target = self
            .value
            .keys()
            .iter()
            .skip(next)
            .chain(self.value.keys().iter().take(next).rev())
            .find(|key| self.removable(key) && !self.hidden_keys.contains(key));
        if let Some(handle) = target.and_then(|key| self.tag_focus.get(key)) {
            handle.focus(window, cx);
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }
    pub(super) fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let value = match &self.value {
            SelectValue::Single(Some(key)) if !self.removable(key) => self.value.clone(),
            SelectValue::Single(_) => SelectValue::Single(None),
            SelectValue::Multiple(keys) => SelectValue::Multiple(
                keys.iter()
                    .filter(|key| !self.removable(key))
                    .cloned()
                    .collect(),
            ),
        };
        self.commit(value, SelectChangeSource::Clear, cx);
        self.focus.focus(window, cx);
    }
    fn ticket(&mut self, kind: RequestKind, cx: &Context<Self>) -> SelectRequestTicket {
        self.serial += 1;
        SelectRequestTicket {
            owner: cx.entity_id(),
            session: self.session,
            generation: self.generation,
            serial: self.serial,
            selection: self.selection_revision,
            kind,
        }
    }
    fn valid(&self, ticket: &SelectRequestTicket, cx: &Context<Self>) -> bool {
        self.open
            && self.editable()
            && ticket.owner == cx.entity_id()
            && ticket.session == self.session
            && ticket.generation == self.generation
            && match ticket.kind {
                RequestKind::Search => self.pending_search.as_ref() == Some(ticket),
                RequestKind::More => self.pending_more.as_ref() == Some(ticket),
                RequestKind::Create => {
                    self.pending_create.as_ref() == Some(ticket)
                        && ticket.selection == self.selection_revision
                }
            }
    }
    fn start_search(&mut self, cx: &mut Context<Self>) {
        let ticket = self.ticket(RequestKind::Search, cx);
        self.pending_search = Some(ticket.clone());
        self.pending_more = None;
        self.next_cursor = None;
        self.consumed_cursors.clear();
        self.result = SelectResultState::Loading;
        self.rebuild();
        cx.emit(SelectEvent::SearchRequested(SelectRequest {
            query: self.committed_query.clone(),
            ticket,
            cursor: None,
        }));
        cx.notify();
    }
    pub fn request_more(&mut self, cx: &mut Context<Self>) {
        if !self.open
            || !self.editable()
            || self.search != SelectSearch::Remote
            || self.pending_search.is_some()
            || self.pending_more.is_some()
            || matches!(self.more_result, SelectResultState::Failed(_))
        {
            return;
        }
        let Some(cursor) = self.next_cursor.clone() else {
            return;
        };
        let ticket = self.ticket(RequestKind::More, cx);
        self.pending_more = Some(ticket.clone());
        self.more_result = SelectResultState::Loading;
        cx.emit(SelectEvent::LoadMoreRequested(SelectRequest {
            query: self.committed_query.clone(),
            ticket,
            cursor: Some(cursor),
        }));
        cx.notify();
    }
    fn request_create(&mut self, cx: &mut Context<Self>) {
        if self.pending_create.is_some() {
            return;
        }
        let ticket = self.ticket(RequestKind::Create, cx);
        self.pending_create = Some(ticket.clone());
        self.create_result = SelectResultState::Loading;
        cx.emit(SelectEvent::CreateRequested(SelectRequest {
            query: self.committed_query.clone(),
            ticket,
            cursor: None,
        }));
        cx.notify();
    }
    pub fn retry_request(&mut self, cx: &mut Context<Self>) {
        if !self.open || !self.editable() {
            return;
        }
        if matches!(self.result, SelectResultState::Failed(_)) {
            self.start_search(cx);
        } else if matches!(self.more_result, SelectResultState::Failed(_)) {
            self.more_result = SelectResultState::Ready;
            self.request_more(cx);
        } else if matches!(self.create_result, SelectResultState::Failed(_)) {
            self.request_create(cx);
        }
    }
    pub fn apply_page(
        &mut self,
        ticket: &SelectRequestTicket,
        page: SelectPage,
        cx: &mut Context<Self>,
    ) -> Result<SelectApply, SelectError> {
        if !self.valid(ticket, cx) {
            return Ok(SelectApply::Stale);
        }
        if ticket.kind == RequestKind::Create {
            return Err(SelectError::InvalidRequest);
        }
        let mut entries = if ticket.kind == RequestKind::More {
            self.entries.clone()
        } else {
            vec![]
        };
        if ticket.kind == RequestKind::More
            && (page.entries.is_empty()
                || page.next_cursor == self.next_cursor
                || page
                    .next_cursor
                    .as_ref()
                    .is_some_and(|cursor| self.consumed_cursors.contains(cursor)))
        {
            self.pending_more = None;
            self.more_result = SelectResultState::Failed("加载未取得进展，请重试".into());
            cx.notify();
            return Err(SelectError::NoProgress);
        }
        entries.extend(page.entries);
        if let Err(error) = validate_entries(&entries) {
            self.reject_request(ticket, format!("选项数据无效：{error}"), cx);
            return Err(error);
        }
        if ticket.kind == RequestKind::More
            && let Some(cursor) = &self.next_cursor
        {
            self.consumed_cursors.insert(cursor.clone());
        }
        self.entries = entries;
        self.next_cursor = page.next_cursor;
        self.pending_search = None;
        self.pending_more = None;
        self.result = SelectResultState::Ready;
        self.more_result = SelectResultState::Ready;
        self.refresh_cache(cx);
        self.rebuild();
        cx.notify();
        Ok(SelectApply::Applied)
    }
    pub fn reject_request(
        &mut self,
        ticket: &SelectRequestTicket,
        message: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> SelectApply {
        if !self.valid(ticket, cx) {
            return SelectApply::Stale;
        }
        let message = message.into();
        match ticket.kind {
            RequestKind::Search => {
                self.pending_search = None;
                self.result = SelectResultState::Failed(message);
            }
            RequestKind::More => {
                self.pending_more = None;
                self.more_result = SelectResultState::Failed(message);
            }
            RequestKind::Create => {
                self.pending_create = None;
                self.create_result = SelectResultState::Failed(message);
            }
        }
        self.rebuild();
        cx.notify();
        SelectApply::Applied
    }
    pub fn resolve_create(
        &mut self,
        ticket: &SelectRequestTicket,
        option: SelectOption,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<SelectApply, SelectError> {
        if !self.valid(ticket, cx) {
            return Ok(SelectApply::Stale);
        }
        if ticket.kind != RequestKind::Create {
            return Err(SelectError::InvalidRequest);
        }
        if self.at_limit() {
            self.reject_request(ticket, "已达到选择数量上限", cx);
            return Err(SelectError::SelectionLimit);
        }
        if option.disabled {
            self.reject_request(ticket, "新建选项不可为禁用状态", cx);
            return Err(SelectError::DisabledOption);
        }
        if self.value.contains(&option.key) {
            let error = SelectError::DuplicateKey(option.key);
            self.reject_request(ticket, format!("新建选项无效：{error}"), cx);
            return Err(error);
        }
        let mut entries = self.entries.clone();
        entries.push(option.clone().into());
        if let Err(error) = validate_entries(&entries) {
            self.reject_request(ticket, format!("新建选项无效：{error}"), cx);
            return Err(error);
        }
        self.entries = entries;
        let value = match &self.value {
            SelectValue::Single(_) => SelectValue::Single(Some(option.key)),
            SelectValue::Multiple(keys) => {
                let mut keys = keys.clone();
                keys.push(option.key);
                SelectValue::Multiple(keys)
            }
        };
        self.commit(value, SelectChangeSource::Create, cx);
        if self.mode == SelectMode::Single {
            self.close(SelectCloseReason::Confirm, true, window, cx);
        }
        Ok(SelectApply::Applied)
    }
    pub(super) fn has_more(&self) -> bool {
        self.next_cursor.is_some()
    }
}
impl Render for YueSelectState {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        super::element::render(self, window, cx)
    }
}

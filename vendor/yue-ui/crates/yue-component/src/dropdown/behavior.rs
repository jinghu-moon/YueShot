//! Token-free navigation, typeahead, dwell, state and channel identity.
use super::{ROW_ENTER_WINDOW, SUBMENU_DWELL, TYPEAHEAD_WINDOW, YueMenuItem};
use gpui_kit::{Modifiers, SharedString};
use std::time::{Duration, Instant};
// A move the arrow keys ask for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MenuMove {
    /// One row along the menu's own axis.
    Line(i32),
    /// One column across a panel that runs in more than one column.
    Column(i32),
    First,
    Last,
}

// Which move a key asks for, or `None` when the key belongs to something else.
//
// Left and right are not moves here: they open and close the submenu, and the
// panel dispatches those itself.
pub(super) fn move_for(key: &str) -> Option<MenuMove> {
    match key {
        "down" => Some(MenuMove::Line(1)),
        "up" => Some(MenuMove::Line(-1)),
        "home" => Some(MenuMove::First),
        "end" => Some(MenuMove::Last),
        _ => None,
    }
}

// The character a keystroke types, when it types one at all.
//
// A named key — `escape`, `home`, `space` — is longer than one character, so the
// length test is the reference's own `e.key.length === 1`, and the modifiers it
// excludes are the ones that make a keystroke a binding rather than a letter. A
// shifted letter arrives as the lowercase character with `shift` set, so capitals
// typeahead the same way their row is spelled.
pub(super) fn typeable(key: &str, modifiers: &Modifiers) -> Option<char> {
    if modifiers.control || modifiers.alt || modifiers.platform {
        return None;
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => Some(ch),
        _ => None,
    }
}

// The row a move lands on, out of `len` roving rows.
//
// A single column wraps at both ends, the way a native menu does; a multi-column
// panel steps by whole rows and stops at the edge. `None` — the panel has just
// opened and nothing is highlighted yet — enters at the first row going down and
// at the last going up.
pub(super) fn move_index(
    len: usize,
    current: Option<usize>,
    move_: MenuMove,
    columns: usize,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    match (current, move_) {
        (None, MenuMove::Line(dy)) => Some(if dy < 0 { last } else { 0 }),
        (None, MenuMove::Column(dx)) => Some(if dx < 0 { last } else { 0 }),
        (None, MenuMove::First) | (Some(_), MenuMove::First) => Some(0),
        (Some(_), MenuMove::Last) | (None, MenuMove::Last) => Some(last),
        (Some(current), MenuMove::Line(dy)) => {
            if columns > 1 {
                Some((current as i32 + dy * columns as i32).clamp(0, last as i32) as usize)
            } else {
                Some((current as i32 + dy + len as i32) as usize % len)
            }
        }
        (Some(current), MenuMove::Column(dx)) => {
            if columns > 1 {
                Some((current as i32 + dx).clamp(0, last as i32) as usize)
            } else {
                Some(current)
            }
        }
    }
}

// Where a typed letter takes the highlight, and what the prefix buffer holds
// afterwards.
//
// A multi-letter prefix that finds nothing falls back to its last letter, but a
// key that finds nothing at all leaves the highlight where it was: the panel
// must not swallow a keystroke it did not use.
pub(super) fn typeahead(
    labels: &[SharedString],
    current: Option<usize>,
    previous: &str,
    previous_age: Option<Duration>,
    key: char,
) -> (Option<usize>, SharedString) {
    let single = key.to_lowercase().collect::<String>();
    let mut query = match previous_age {
        Some(age) if age <= TYPEAHEAD_WINDOW => previous.to_lowercase(),
        _ => String::new(),
    };
    query.push_str(&single);
    if let Some(index) = find_ahead(labels, current, &query) {
        return (Some(index), SharedString::from(query));
    }
    if query != single {
        // The reference replaces its buffer with the last letter before it retries,
        // so the next key extends the letter that survived rather than the prefix
        // that failed — whether or not that retry found a row.
        let index = find_ahead(labels, current, &single);
        return (index, SharedString::from(single));
    }
    (None, SharedString::from(query))
}

// The first row after `current`, wrapping, whose text starts with `query`.
fn find_ahead(labels: &[SharedString], current: Option<usize>, query: &str) -> Option<usize> {
    let len = labels.len();
    if len == 0 || query.is_empty() {
        return None;
    }
    // The reference walks `for (s = 1; s <= len; s++) l[(i + s) % len]`: one past
    // the highlighted row, so a repeated letter steps between its matches, and `i`
    // is -1 when nothing is highlighted, which starts the walk at the first row.
    let base = current.map_or(-1, |index| index as i32);
    (1..=len as i32)
        .map(|ahead| (base + ahead).rem_euclid(len as i32) as usize)
        .find(|index| labels[*index].to_lowercase().starts_with(query))
}

// Whether a hovered submenu row has held the pointer long enough to open.
pub(super) fn dwell_elapsed(held: Duration) -> bool {
    held >= SUBMENU_DWELL
}

// What one panel keeps where its own rows are concerned.
//
// Every index is a position in that panel's `items`, so a heading the pointer
// rests on still has a row, even though the keyboard skips it.
#[derive(Default)]
pub(super) struct PanelMotion {
    /// The highlighted row — the one Enter runs and the one
    /// `aria-activedescendant` names. `None` until the panel has opened or a key
    /// has moved.
    pub(super) active: Option<usize>,
    /// The row the pointer is over, which may be one the keyboard skips.
    pub(super) hovered: Option<usize>,
    /// When the pointer landed on `hovered`: a submenu opens only once the
    /// pointer has held still on it for `SUBMENU_DWELL`.
    pub(super) hovered_at: Option<Instant>,
    /// The row being pressed.
    pub(super) pressed: Option<usize>,
    /// The typeahead prefix and when it last moved.
    pub(super) prefix: SharedString,
    pub(super) typed_at: Option<Instant>,
    /// When this panel opened, for as long as its rows still stagger in.
    pub(super) opened_at: Option<Instant>,
    /// Bumped on every open edge of *this* panel, so its rows' enter keyframes
    /// restart their playback instead of continuing the last open's finished one.
    pub(super) generation: usize,
    /// Whether this open has already given its rows a frame of unlit tint.
    pub(super) tint_seeded: bool,
}

impl PanelMotion {
    /// Records the edge into the open state: the highlight, the dwell and the
    /// prefix all start again.
    pub(super) fn note_open(&mut self, now: Instant) {
        self.generation = self.generation.wrapping_add(1);
        self.active = None;
        self.hovered = None;
        self.hovered_at = None;
        self.pressed = None;
        self.prefix = SharedString::default();
        self.typed_at = None;
        self.opened_at = Some(now);
        self.tint_seeded = false;
    }

    /// Whether this panel has just opened, which is the only time its rows
    /// stagger in. Past the window, a row that rebuilds must not re-run its
    /// entrance because the pointer moved.
    pub(super) fn is_entering(&self, now: Instant) -> bool {
        self.opened_at
            .is_some_and(|at| now.saturating_duration_since(at) < ROW_ENTER_WINDOW)
    }

    /// How long the pointer has rested on the hovered row, for a submenu dwell.
    pub(super) fn dwell(&self, now: Instant) -> Duration {
        self.hovered_at
            .map_or(Duration::ZERO, |at| now.saturating_duration_since(at))
    }

    /// Types one character into the prefix buffer and returns the row it lands
    /// on, if any. A key that matches nothing leaves the highlight alone: the
    /// reference does not swallow a keystroke it could not use.
    pub(super) fn type_into(
        &mut self,
        labels: &[SharedString],
        current: Option<usize>,
        now: Instant,
        key: char,
    ) -> Option<usize> {
        let age = self.typed_at.map(|at| now.saturating_duration_since(at));
        let (index, prefix) = typeahead(labels, current, &self.prefix, age, key);
        self.prefix = prefix;
        self.typed_at = Some(now);
        index
    }
}

// What one host keeps for the panel it shows and the one submenu off its rows.
//
// This is element state on the host, so it survives a frame of repaint but is
// dropped with the host: a menu scrolled out of a list takes its highlight and
// its armed submenu with it.
#[derive(Default)]
pub(super) struct MenuMotion {
    pub(super) root: PanelMotion,
    pub(super) submenu: PanelMotion,
    /// The root row whose submenu panel is showing. One at a time, and the only
    /// source of a submenu's visibility: a submenu holds no open state of its
    /// own, so it cannot disagree with the panel that owns it.
    pub(super) armed: Option<usize>,
    /// Whether an open panel was rendered last frame — how the edge into the
    /// open state is found without a second state machine.
    pub(super) was_open: bool,
}

impl MenuMotion {
    /// The motion of one of this host's two panels.
    pub(super) fn panel(&self, path: PanelPath) -> &PanelMotion {
        match path {
            PanelPath::Root => &self.root,
            PanelPath::Submenu(_) => &self.submenu,
        }
    }

    pub(super) fn panel_mut(&mut self, path: PanelPath) -> &mut PanelMotion {
        match path {
            PanelPath::Root => &mut self.root,
            PanelPath::Submenu(_) => &mut self.submenu,
        }
    }

    /// Records the edge into the open state: both panels start clean, the rows get
    /// a fresh generation to key their entrance on, and the root highlights the
    /// row the reference's `focusFirst` would — the panel opens with a mark on it.
    pub(super) fn note_open(&mut self, now: Instant, first: Option<usize>) {
        self.armed = None;
        self.root.note_open(now);
        self.submenu.note_open(now);
        self.root.active = first;
    }

    /// Arms or disarms the submenu off root row `index`, restarting that panel's
    /// entrance clock and highlighting `first` — the submenu's own first
    /// navigable row, because the reference's `openSub` calls `focusFirst`.
    pub(super) fn arm(&mut self, index: Option<usize>, first: Option<usize>, now: Instant) {
        if self.armed == index {
            return;
        }
        self.armed = index;
        if index.is_some() {
            self.submenu.note_open(now);
            self.submenu.active = first;
            // One mark at a time: the row the submenu came from gives its own up,
            // and `close_submenu` is what hands it back.
            self.root.active = None;
        }
    }
}

// The rows the arrow keys stop at, and their labels, in menu order.
//
// A heading, a hairline, a loading line and a disabled command are all outside
// this set: they are drawn, but neither the keyboard nor typeahead lands on them.
pub(super) fn roving_of(items: &[YueMenuItem]) -> (Vec<usize>, Vec<SharedString>) {
    let roving: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.is_roving())
        .map(|(index, _)| index)
        .collect();
    let labels = roving
        .iter()
        .filter_map(|index| items.get(*index))
        .map(|item| item.label.clone())
        .collect();
    (roving, labels)
}

// The row a panel highlights the moment it opens, in `items` coordinates.
//
// A scan rather than a call to `roving_of`, because the pointer dwell asks for
// it every frame and only the first hit matters.
pub(super) fn first_roving(items: &[YueMenuItem]) -> Option<usize> {
    items.iter().position(YueMenuItem::is_roving)
}

// The highlighted row's place in the roving set, which is what the arrows move
// through — and `None` when the highlight sits on a row the keyboard skips.
pub(super) fn slot_of(roving: &[usize], active: Option<usize>) -> Option<usize> {
    active.and_then(|index| roving.iter().position(|slot| *slot == index))
}

// What the panel of this host is: the root panel, or one of its submenus.
//
// Panels are the same code with different identities, so every channel — the
// presence ramp, the motion state, the anchor — is keyed by this rather than by
// a name that could collide between a panel and its own submenu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PanelPath {
    Root,
    Submenu(usize),
}

impl PanelPath {
    pub(super) fn channel(self, name: &'static str) -> String {
        match self {
            PanelPath::Root => format!("yue-dropdown-{name}"),
            PanelPath::Submenu(index) => format!("yue-dropdown-submenu-{index}-{name}"),
        }
    }
}

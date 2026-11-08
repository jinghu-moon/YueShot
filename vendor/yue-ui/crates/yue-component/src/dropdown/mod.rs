//! Yue dropdown: a click-triggered command panel and the rows that live in it.
//!
//! A dropdown is the desktop answer to a group of related commands: the trigger
//! stays one tab stop, the panel takes focus when it opens, arrow keys walk the
//! available rows, and Escape hands focus back to the trigger. The open, dismiss
//! and focus lifecycle is GPUI Base's (`PopoverState`), so a Yue panel and a Base
//! popover cannot disagree about what is open. What lives here is Yue's own: the
//! row model, the placement and navigation arithmetic, the panel's drawing and
//! its motion.
//!
//! Rows are built by the caller every frame and hold no state of their own: a
//! checked row reflects the caller's model, and activating a row runs its handler
//! and — unless the row asked to `.keep_open()` — dismisses the panel.

mod element;
mod host;

use std::rc::Rc;

use std::time::{Duration, Instant};

use behavior::*;

use panel::*;

use placement::*;

use gpui_kit::base::{
    Align, Easing, Keyframe, Keyframes, Placement, PopoverState, SignedDuration, Stagger,
    StaggerOrigin, TestSupportExt as _, Timing, Transition, animate_keyframes, transition,
};

use gpui_kit::prelude::FluentBuilder as _;

use gpui_kit::{
    Anchor, Animation, AnimationExt as _, AnyElement, App, Bounds, ClickEvent, ElementId, Entity,
    EntityId, Focusable, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyboardButton, KeyboardClickEvent, MouseButton, ParentElement as _, Pixels, Role,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Toggled, Window,
    canvas, div, px,
};

use yue_component_icons::{Icon, IconDefinition};

use crate::icon::YueIcon;

use crate::popup::{
    ArrowSpec, PopupAnchor, PopupAnchorHandle, PopupDismissRoute, PopupHost, PopupId,
    PopupPlacement, PopupPointerMode, PopupRuntime, PopupSpec,
};

use crate::tokens::{cubic_bezier, duration, easing, mix_rgba, number, pixels, rems, rgba, shadow};

use crate::{YueButton, YueTheme};

mod behavior;

mod panel;

mod placement;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "headless-tests"))]
mod interaction_tests;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

type OpenHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

const KEY_CONTEXT: &str = "YueDropdownMenu";

pub(crate) fn init(cx: &mut App) {
    // GPUI resolves key bindings before raw key listeners. Mask ancestor commands
    // for keys owned by the menu so Escape/Enter/arrows reach its keyboard ladder.
    cx.bind_keys(
        [
            "escape",
            "tab",
            "shift-tab",
            "enter",
            "space",
            "up",
            "down",
            "left",
            "right",
            "home",
            "end",
        ]
        .map(|key| gpui_kit::KeyBinding::new(key, gpui_kit::NoAction, Some(KEY_CONTEXT))),
    );
}

/// Distance kept between the panel and the window edge, the reference's `MG`.
const WINDOW_MARGIN: Pixels = px(8.);

/// The caret is held this far from a panel corner when the trigger centre would
/// put it closer than that — the reference's `PAD`.
const CARET_EDGE_MARGIN: f32 = 16.;

/// A submenu keeps 8px from its parent panel, closer than the 12px a top-level
/// trigger gets: the two panels still read as one tree, not two unrelated menus.
const SUBMENU_GAP: Pixels = px(8.);

/// A context menu sits on the pointer itself, so it needs no air around it.
const CONTEXT_GAP: Pixels = px(0.);

/// How long the pointer has to rest on a submenu row before it opens — the
/// reference's `setTimeout(go, 80)`.
const SUBMENU_DWELL: Duration = Duration::from_millis(80);

/// Each row enters this much later than the one before it, the reference's
/// `animation-delay: calc(var(--i,0)*20ms)`.
const ROW_STAGGER: Duration = Duration::from_millis(20);

/// A panel that has been open this long stops staggering: late rows have had
/// time to run, and the reference drops its `.enter` class at 520ms.
const ROW_ENTER_WINDOW: Duration = Duration::from_millis(520);

/// A typeahead prefix restarts after this much silence — the reference's
/// `now - tbt > 600`.
const TYPEAHEAD_WINDOW: Duration = Duration::from_millis(600);

/// The caret's box: an 8px square rotated 45°, the reference's `.menu::before`.
const CARET_BOX: f32 = 8.;

/// The square's corners — and so the caret's tip and its two shoulder points —
/// sit half a diagonal from its centre.
const CARET_RADIUS: f32 = CARET_BOX / 2.0 * std::f32::consts::SQRT_2;

/// Where the panel sits relative to its trigger, before the viewport is asked.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DropdownPlacement {
    /// Below the trigger, above when that does not fit.
    #[default]
    Auto,
    Above,
    Below,
    Leading,
    Trailing,
}

/// How the panel lines up with its trigger along the cross axis.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DropdownAlign {
    /// With the trigger's leading edge — what a desktop menu expects.
    #[default]
    Start,
    Center,
    End,
}

/// Which end of a row carries its check mark or radio dot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DropdownIndicatorSide {
    /// The leading end, before the label, whenever the menu has a checkable row —
    /// and no slot at all when none of its rows can be marked.
    #[default]
    Auto,
    Leading,
    Trailing,
}

impl DropdownPlacement {
    /// The side Base's positioner is asked for.
    ///
    /// Base treats "no preference" as "above first", so `Auto` asks for the lower
    /// side explicitly to get the order the reference uses.
    fn side(self) -> Placement {
        match self {
            DropdownPlacement::Auto | DropdownPlacement::Below => Placement::Bottom,
            DropdownPlacement::Above => Placement::Top,
            DropdownPlacement::Leading => Placement::Left,
            DropdownPlacement::Trailing => Placement::Right,
        }
    }
}

impl DropdownAlign {
    fn align(self) -> Align {
        match self {
            DropdownAlign::Start => Align::Start,
            DropdownAlign::Center => Align::Center,
            DropdownAlign::End => Align::End,
        }
    }
}

/// What opened this panel, which decides its gap, its caret and its edge margin.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum MenuKind {
    /// A button trigger: the panel floats one gap below it and points at it.
    #[default]
    Trigger,
    /// A row's own panel, placed to the side of that row.
    Submenu,
    /// The panel of a right-click area, placed on the pointer.
    Context,
}

impl MenuKind {
    /// Only a trigger panel points back at its trigger: a submenu is joined to
    /// its parent by the row that opened it, and a context menu has no trigger.
    fn wants_caret(self) -> bool {
        matches!(self, MenuKind::Trigger)
    }
}

/// The kind of a row, which decides whether the keyboard stops there.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum MenuRowKind {
    Info,
    #[default]
    Command,
    Submenu,
    Heading,
    Separator,
    Loading,
}

/// The mark a row carries, if any.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum MenuMark {
    #[default]
    None,
    Check(bool),
    Radio(bool),
}

/// One row of a dropdown panel.
///
/// Identity comes from `.id()`, falling back to the label, so re-ordering a menu
/// does not strand a highlight on the row that moved.
#[derive(Clone)]
pub struct YueMenuItem {
    kind: MenuRowKind,
    id: Option<SharedString>,
    label: SharedString,
    description: Option<SharedString>,
    mark: MenuMark,
    icon: Option<Icon>,
    avatar: Option<SharedString>,
    hotkey: Option<SharedString>,
    danger: bool,
    disabled: bool,
    keep_open: bool,
    rule: bool,
    children: Vec<Self>,
    on_click: Option<ClickHandler>,
}

impl YueMenuItem {
    pub(crate) fn with_icon(mut self, icon: Option<Icon>) -> Self {
        self.icon = icon;
        self
    }
    /// A non-actionable row for compound controls that expose a read-only list.
    pub(crate) fn info(label: impl Into<SharedString>) -> Self {
        Self::of(MenuRowKind::Info, label)
    }

    pub(crate) fn description(mut self, description: Option<SharedString>) -> Self {
        self.description = description;
        self
    }
    /// A command row.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self::of(MenuRowKind::Command, label)
    }

    /// A group caption. The keyboard does not stop here.
    pub fn heading(label: impl Into<SharedString>) -> Self {
        Self::of(MenuRowKind::Heading, label)
    }

    /// A hairline between two semantically different groups of commands.
    pub fn separator() -> Self {
        Self::of(MenuRowKind::Separator, "")
    }

    /// A row that says its content is still arriving.
    pub fn loading(label: impl Into<SharedString>) -> Self {
        Self::of(MenuRowKind::Loading, label)
    }

    /// A row that opens one further panel.
    ///
    /// One level only: a child that itself carries a submenu is drawn as a plain
    /// command, because a deeper tree belongs to a dialog or a select.
    pub fn submenu(label: impl Into<SharedString>, items: impl IntoIterator<Item = Self>) -> Self {
        Self {
            children: items.into_iter().collect(),
            ..Self::of(MenuRowKind::Submenu, label)
        }
    }

    fn of(kind: MenuRowKind, label: impl Into<SharedString>) -> Self {
        Self {
            kind,
            id: None,
            label: label.into(),
            description: None,
            mark: MenuMark::None,
            icon: None,
            avatar: None,
            hotkey: None,
            danger: false,
            disabled: false,
            keep_open: false,
            rule: false,
            children: Vec::new(),
            on_click: None,
        }
    }

    pub fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn icon(mut self, icon: impl IconDefinition) -> Self {
        self.icon = Some(Icon::new(icon));
        self
    }

    /// A circle of initials before the label, for the rows that name a person.
    pub fn avatar(mut self, initials: impl Into<SharedString>) -> Self {
        self.avatar = Some(initials.into());
        self
    }

    /// The binding that also runs this command. It says nothing the application
    /// has not actually bound, and it is not a substitute for one.
    pub fn hotkey(mut self, hint: impl Into<SharedString>) -> Self {
        self.hotkey = Some(hint.into());
        self
    }

    /// A tick at the indicator end, driven by the caller's model.
    pub fn checked(mut self, checked: bool) -> Self {
        self.mark = MenuMark::Check(checked);
        self
    }

    /// A filled dot at the indicator end: this row is the current value of a set.
    pub fn radio(mut self, selected: bool) -> Self {
        self.mark = MenuMark::Radio(selected);
        self
    }

    /// A destructive command, drawn with the danger colour rather than the
    /// menu's own, and named with a verb.
    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    /// Visible, but neither pointer nor keyboard reaches it. Say why next to it:
    /// low contrast alone does not explain a disabled command.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Stay open after activating this row — what a toggle inside a menu wants.
    pub fn keep_open(mut self, keep_open: bool) -> Self {
        self.keep_open = keep_open;
        self
    }

    /// A caption followed by a hairline, the reference's ruled `.mt`.
    pub fn rule(mut self, rule: bool) -> Self {
        self.rule = rule;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// The row's stable name for its focus handle and motion channels.
    fn key(&self, index: usize) -> SharedString {
        self.id
            .clone()
            .or_else(|| (!self.label.is_empty()).then(|| self.label.clone()))
            .unwrap_or_else(|| SharedString::from(format!("row-{index}")))
    }

    /// Whether the arrow keys stop here: a separator, a caption, a loading row and
    /// a disabled row are all transparent.
    fn is_roving(&self) -> bool {
        !self.disabled && matches!(self.kind, MenuRowKind::Command | MenuRowKind::Submenu)
    }

    fn is_checkable(&self) -> bool {
        !matches!(self.mark, MenuMark::None)
    }
}

/// The choices a panel and its submenu have to agree on: a submenu inherits
/// them, so one menu cannot mix two densities or two indicator sides.
#[derive(Clone)]
struct MenuShape {
    theme: YueTheme,
    placement: DropdownPlacement,
    align: DropdownAlign,
    columns: usize,
    indicator_side: DropdownIndicatorSide,
    max_height: Option<Pixels>,
    panel_width: Option<Pixels>,
    surface_bounds: Option<Rc<dyn Fn(Bounds<Pixels>)>>,
}

impl MenuShape {
    fn new(theme: YueTheme) -> Self {
        Self {
            theme,
            placement: DropdownPlacement::default(),
            align: DropdownAlign::default(),
            columns: 1,
            indicator_side: DropdownIndicatorSide::default(),
            max_height: None,
            panel_width: None,
            surface_bounds: None,
        }
    }

    /// The panel's two width edges against the viewport this frame: the width the
    /// caller asked for (`.panel_width(...)`, else `--menu-width`) and the
    /// reference's ceiling `min(320px, 90vw)`. Whether the first is an exact width
    /// or only a floor is the panel's business; either way it is clamped against the
    /// second, so no row can spill past the surface that holds it.
    fn bounds(&self, viewport: gpui_kit::Size<Pixels>) -> (Pixels, Pixels) {
        let width = self
            .panel_width
            .unwrap_or(px(pixels(self.theme, crate::theme::keys::MENU_WIDTH)));
        let widest = px(320.).min(viewport.width * 0.9);
        (width, widest)
    }
}

/// A cubic-Bézier token read as Base's easing policy.
fn curve(theme: YueTheme, name: impl crate::theme::TokenSource) -> Easing {
    let (x1, y1, x2, y2) = cubic_bezier(theme, name);
    Easing::CubicBezier { x1, y1, x2, y2 }
}

/// What the menu opens from: a Yue button, or anything the caller can draw.
type TriggerBuilder = Box<dyn FnOnce(bool, &mut Window, &mut App) -> AnyElement>;

/// The two shapes a trigger comes in, which decide where the toggle is bound.
enum TriggerSpec {
    Action(ActionTriggerBuilder),
    /// A Yue button, which the host arms itself. GPUI maps Enter and Space to the
    /// click listeners of the element that holds the focus, so the toggle has to
    /// sit on the button rather than on the wrapper around it: bound outside, a
    /// keyboard user can reach the trigger by Tab and then do nothing with it.
    ///
    /// The button rides a `Box` so that a menu with no button at all does not carry
    /// a couple of hundred bytes per frame.
    Button(Box<YueButton>),
    /// Anything else. The host cannot reach inside an arbitrary element, so the
    /// toggle binds to the wrapper and the trigger answers the pointer.
    Custom(TriggerBuilder),
}

type ActionTriggerBuilder =
    Box<dyn FnOnce(bool, ClickHandler, &mut Window, &mut App) -> AnyElement>;

/// A command menu: one trigger, and the panel of commands it opens.
///
/// The trigger keeps the menu's single tab stop and the panel takes focus while it
/// is open, so a disabled menu is a disabled trigger: gpui's button then swallows
/// both the pointer and the keyboard activation, and there is no second flag that
/// could disagree with the first.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueButton, YueDropdownMenu, YueMenuItem};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let menu = YueDropdownMenu::new("actions", theme, YueButton::new("trigger", "操作"))
///     .items([YueMenuItem::new("复制").on_click(|_, _, _| {})]);
/// ```
#[derive(IntoElement)]
pub struct YueDropdownMenu {
    state: Option<Entity<PopoverState>>,
    id: ElementId,
    shape: MenuShape,
    name: Option<SharedString>,
    trigger: TriggerSpec,
    items: Vec<YueMenuItem>,
    default_open: bool,
    on_open_change: Option<OpenHandler>,
    use_default_theme: bool,
}

impl YueDropdownMenu {
    pub(crate) fn with_state(mut self, state: &Entity<PopoverState>) -> Self {
        self.state = Some(state.clone());
        self
    }

    /// Give a composed Base button the same toggle as the built-in Yue trigger.
    pub(crate) fn with_action_trigger(
        id: impl Into<ElementId>,
        theme: YueTheme,
        trigger: impl FnOnce(bool, ClickHandler, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self::build(id, theme, TriggerSpec::Action(Box::new(trigger)))
    }
    pub(crate) fn on_surface_bounds(mut self, callback: impl Fn(Bounds<Pixels>) + 'static) -> Self {
        self.shape.surface_bounds = Some(Rc::new(callback));
        self
    }

    /// Follow the application theme for the panel on each render. The supplied
    /// trigger owns its theme; use `YueButton::new` for an inherited trigger.
    pub fn use_default_theme(mut self) -> Self {
        self.use_default_theme = true;
        self
    }
    /// Override the panel theme locally.
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.shape.theme = theme;
        self.use_default_theme = false;
        self
    }

    /// A menu whose trigger is a Yue button: the host gives it the expanded state
    /// and the activation that opens it, so the caller's button carries no handler.
    pub fn new(id: impl Into<ElementId>, theme: YueTheme, trigger: YueButton) -> Self {
        Self::build(id, theme, TriggerSpec::Button(Box::new(trigger)))
    }

    /// A menu whose trigger is anything else — a label, an avatar, a compound row.
    /// The flag says whether the panel is open, which is what a chevron turns on.
    pub fn with_trigger(
        id: impl Into<ElementId>,
        theme: YueTheme,
        trigger: impl FnOnce(bool, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self::build(id, theme, TriggerSpec::Custom(Box::new(trigger)))
    }

    fn build(id: impl Into<ElementId>, theme: YueTheme, trigger: TriggerSpec) -> Self {
        Self {
            state: None,
            id: id.into(),
            shape: MenuShape::new(theme),
            use_default_theme: false,
            name: None,
            trigger,
            items: Vec::new(),
            default_open: false,
            on_open_change: None,
        }
    }

    pub fn items(mut self, items: impl IntoIterator<Item = YueMenuItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    /// What the panel is called. The reference names a panel after the trigger that
    /// opened it; GPUI has no `aria-labelledby`, so the name comes from here. A
    /// submenu is named by its own row and ignores this.
    pub fn label(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Which side the panel tries first. `Auto` asks below and lets the viewport
    /// answer; a fixed placement is a request the collision rules can still refuse.
    pub fn placement(mut self, placement: DropdownPlacement) -> Self {
        self.shape.placement = placement;
        self
    }

    /// How the panel lines up with the trigger along the cross axis.
    pub fn align(mut self, align: DropdownAlign) -> Self {
        self.shape.align = align;
        self
    }

    /// Which end of a row carries its tick or dot. `Auto` reserves no slot at all
    /// when the menu has nothing to mark.
    pub fn indicator_side(mut self, side: DropdownIndicatorSide) -> Self {
        self.shape.indicator_side = side;
        self
    }

    /// Columns for a bounded, comparable set — swatches, icon tiles. Arrow keys
    /// step across a row and down a column; a list of unlike commands stays at one.
    pub fn columns(mut self, columns: usize) -> Self {
        self.shape.columns = columns.max(1);
        self
    }

    /// Where the panel stops growing and starts scrolling. Defaults to half the
    /// viewport, capped, so a long menu never becomes the whole window.
    pub fn max_height(mut self, max_height: Pixels) -> Self {
        self.shape.max_height = Some(max_height);
        self
    }

    /// A fixed panel width, which is what lets a long label truncate to an
    /// ellipsis instead of pushing the panel wider.
    pub fn panel_width(mut self, panel_width: Pixels) -> Self {
        self.shape.panel_width = Some(panel_width);
        self
    }

    /// Open on the first frame, for a menu that is a page's standing panel rather
    /// than a response to a click.
    pub fn default_open(mut self, default_open: bool) -> Self {
        self.default_open = default_open;
        self
    }

    pub fn on_open_change(
        mut self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(handler));
        self
    }
}

/// The commands for the object under the pointer, opened by a right click.
///
/// The whole region is the trigger surface and the panel's corner sits on the
/// click itself: a caret would land under the cursor, on the spot the user just
/// aimed at, and a gap would point somewhere else. `placement`, `align` and
/// `indicator_side` mean nothing for a panel with no trigger to line up with, so
/// this host does not offer them.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueContextMenu, YueMenuItem};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let menu = YueContextMenu::new("context", theme).items([YueMenuItem::new("复制")]);
/// ```
#[derive(IntoElement)]
pub struct YueContextMenu {
    id: ElementId,
    shape: MenuShape,
    name: Option<SharedString>,
    items: Vec<YueMenuItem>,
    children: Vec<AnyElement>,
    on_open_change: Option<OpenHandler>,
    use_default_theme: bool,
}

impl YueContextMenu {
    /// Follow the application theme on each render.
    pub fn use_default_theme(mut self) -> Self {
        self.use_default_theme = true;
        self
    }
    /// Override the panel theme locally.
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.shape.theme = theme;
        self.use_default_theme = false;
        self
    }

    pub fn new(id: impl Into<ElementId>, theme: YueTheme) -> Self {
        Self {
            id: id.into(),
            shape: MenuShape::new(theme),
            use_default_theme: false,
            name: None,
            items: Vec::new(),
            children: Vec::new(),
            on_open_change: None,
        }
    }

    pub fn items(mut self, items: impl IntoIterator<Item = YueMenuItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    /// What the panel is called. A right-click panel has no trigger to borrow a
    /// name from, so a menu of commands that repeat across a page says what they
    /// belong to — `"文件"` for the panel that opened over a file.
    pub fn label(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children
            .extend(children.into_iter().map(IntoElement::into_any_element));
        self
    }

    pub fn max_height(mut self, max_height: Pixels) -> Self {
        self.shape.max_height = Some(max_height);
        self
    }

    pub fn panel_width(mut self, panel_width: Pixels) -> Self {
        self.shape.panel_width = Some(panel_width);
        self
    }

    pub fn on_open_change(
        mut self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(handler));
        self
    }
}

/// A primary action and the menu of its secondary actions, side by side.
///
/// The halves stay two click targets with two accessible names: the label runs the
/// default command, the arrow opens the rest. The panel floats from the arrow
/// alone, so its caret points at the half that opened it rather than at the pair.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueButton, YueDropdownSplitButton};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let control = YueDropdownSplitButton::new("save-options", theme, YueButton::new("save", "保存"), "更多保存方式");
/// ```
#[derive(IntoElement)]
pub struct YueDropdownSplitButton {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    arrow_label: SharedString,
    shape: MenuShape,
    name: Option<SharedString>,
    items: Vec<YueMenuItem>,
    action: YueButton,
    disabled: bool,
    default_open: bool,
    on_open_change: Option<OpenHandler>,
}

impl YueDropdownSplitButton {
    /// Resolve the application default at render time, including later theme changes.
    pub fn use_default_theme(mut self) -> Self {
        self.use_default_theme = true;
        self
    }
    /// Override the application default for this component instance.
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.use_default_theme = false;
        self
    }

    /// `action` is the left half with its own handler; `arrow_label` is the right
    /// half's accessible name, since it shows nothing but a chevron.
    pub fn new(
        id: impl Into<ElementId>,
        theme: YueTheme,
        action: YueButton,
        arrow_label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            arrow_label: arrow_label.into(),
            shape: MenuShape::new(theme),
            name: None,
            items: Vec::new(),
            action,
            disabled: false,
            default_open: false,
            on_open_change: None,
        }
    }

    pub fn items(mut self, items: impl IntoIterator<Item = YueMenuItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    /// What the panel of secondary actions is called.
    pub fn label(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn align(mut self, align: DropdownAlign) -> Self {
        self.shape.align = align;
        self
    }

    pub fn max_height(mut self, max_height: Pixels) -> Self {
        self.shape.max_height = Some(max_height);
        self
    }

    pub fn panel_width(mut self, panel_width: Pixels) -> Self {
        self.shape.panel_width = Some(panel_width);
        self
    }

    /// Neither half works: not the default command, not the disclosure.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn default_open(mut self, default_open: bool) -> Self {
        self.default_open = default_open;
        self
    }

    pub fn on_open_change(
        mut self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(handler));
        self
    }
}

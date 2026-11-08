//! Frame-local menu host: retained handles, anchors and surface assembly.

use super::*;

use std::rc::Rc;

use gpui_kit::base::PopoverState;

use gpui_kit::prelude::FluentBuilder as _;

use gpui_kit::{
    AnyElement, App, Bounds, ClickEvent, ElementId, Entity, EntityId, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, Point, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use crate::popup::PopupAnchor;

pub(super) struct MenuHost {
    id: ElementId,
    kind: MenuKind,
    shape: Rc<MenuShape>,
    /// What the panel is called: the trigger's own name for a menu opened from a
    /// button, which the reference puts on the panel as `aria-labelledby`. A
    /// submenu takes the label of the row that opened it instead.
    name: Option<SharedString>,
    state: Entity<PopoverState>,
    motion: Entity<MenuMotion>,
    anchor: PopupAnchor,
    rows: Entity<Vec<PopupAnchor>>,
    scroll: ScrollHandle,
    parent_view: EntityId,
    /// Whether the menu is open, which is the trigger's expanded state.
    open: bool,
}

/// What the caller says about the panel its host will show: which edge it asks
/// for, what it is called, how far it may grow, and whether it opens on its own.
pub(super) struct HostSpec {
    pub(super) state: Option<Entity<PopoverState>>,
    pub(super) kind: MenuKind,
    pub(super) shape: MenuShape,
    pub(super) name: Option<SharedString>,
    pub(super) default_open: bool,
    pub(super) on_open_change: Option<OpenHandler>,
}

impl MenuHost {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }
    pub(super) fn new(
        id: ElementId,
        spec: HostSpec,
        items: &[YueMenuItem],
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let HostSpec {
            state: provided_state,
            kind,
            shape,
            name,
            default_open,
            on_open_change,
        } = spec;
        let shape = Rc::new(shape);
        let state = provided_state.unwrap_or_else(|| {
            window.use_keyed_state((id.clone(), "yue-dropdown-state"), cx, |_, cx| {
                PopoverState::new(default_open, cx)
            })
        });
        state.update(cx, |state, _| state.set_on_open_change(on_open_change));
        let motion = window.use_keyed_state((id.clone(), "yue-dropdown-motion"), cx, |_, _| {
            MenuMotion::default()
        });
        let anchor = window
            .use_keyed_state((id.clone(), "yue-dropdown-anchor"), cx, |_, _| {
                PopupAnchor::default()
            })
            .read(cx)
            .clone();
        let rows = window.use_keyed_state((id.clone(), "yue-dropdown-rows"), cx, |_, _| {
            Vec::<PopupAnchor>::new()
        });
        rows.update(cx, |rows, _| {
            rows.resize_with(items.len(), PopupAnchor::default);
        });
        let scroll = window
            .use_keyed_state((id.clone(), "yue-dropdown-scroll"), cx, |_, _| {
                ScrollHandle::new()
            })
            .read(cx)
            .clone();
        let open = state.read(cx).is_open();
        // The edge into the open state is found here rather than in Base's open
        // callback, because what restarts at that edge is the rows' entrance, the
        // dwell clock and the highlight — all of them the panel's own motion. A
        // render that is open without being an edge, a repaint while the panel
        // stays up, must not re-stagger rows the pointer merely moved over.
        let now = cx.background_executor().now();
        let first = first_roving(items);
        motion.update(cx, |motion, _| {
            if open && !motion.was_open {
                motion.note_open(now, first);
            }
            motion.was_open = open;
        });
        Self {
            id,
            kind,
            shape,
            name,
            state,
            motion,
            anchor,
            rows,
            scroll,
            parent_view: window.current_view(),
            open,
        }
    }

    /// The root panel for `items`, floating from whatever the anchor measured last.
    fn panel(&self, items: Vec<YueMenuItem>) -> MenuPanel {
        let items = Rc::new(items);
        let (roving, labels) = roving_of(&items);
        MenuPanel {
            host: self.id.clone(),
            path: PanelPath::Root,
            kind: self.kind,
            shape: self.shape.clone(),
            name: self.name.clone(),
            state: self.state.clone(),
            motion: self.motion.clone(),
            parent_view: self.parent_view,
            anchor: self.anchor.handle(),
            rows: self.rows.clone(),
            items,
            roving: Rc::new(roving),
            labels: Rc::new(labels),
            scroll: self.scroll.clone(),
        }
    }

    /// Click the trigger: open, or close the panel the trigger itself opened.
    ///
    /// Stopping propagation is what keeps a menu inside a clickable card, a list
    /// row or a toolbar button from also activating the thing underneath it.
    pub(super) fn toggle(&self) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let state = self.state.clone();
        let parent_view = self.parent_view;
        move |_, window, cx| {
            cx.stop_propagation();
            state.update(cx, |state, cx| state.toggle_open(window, cx));
            cx.notify(parent_view);
        }
    }

    /// Read by Popup's side source later in the same frame's prepaint.
    fn capture(&self) -> AnyElement {
        self.anchor.capture()
    }

    /// The host's shell: its trigger, its recorded bounds, and its panel.
    ///
    /// The toggle lives on the trigger's own wrapper rather than on the shell,
    /// because a deferred panel keeps its place in the element tree: bound to the
    /// shell, a click on the panel's padding would bubble up and read as a second
    /// click on the trigger — closing a menu whose row had asked to stay open.
    ///
    /// `toggled` says the trigger already carries the toggle on itself, which is
    /// what a Yue button does so that Enter and Space reach it. Binding it twice
    /// would open and close the menu in one click.
    pub(super) fn assemble(
        &self,
        trigger: AnyElement,
        toggled: bool,
        items: Vec<YueMenuItem>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let open = div()
            .id((self.id.clone(), "yue-dropdown-open"))
            .flex()
            .when(!toggled, |open| open.on_click(self.toggle()))
            .child(trigger);
        let shell = div()
            .id((self.id.clone(), "yue-dropdown-shell"))
            .relative()
            .flex()
            .child(open)
            .child(self.capture());
        let surface = self.panel(items).popup(self.open, window, cx);
        shell
            .when_some(surface, |shell, surface| shell.child(surface))
            .into_any_element()
    }

    /// The same shell for a region that opens on the pointer instead of on a
    /// trigger. Such a region records no bounds of its own: the click is the
    /// anchor, and the region's own rectangle is only what the pointer may land on.
    pub(super) fn region(
        &self,
        body: Vec<AnyElement>,
        items: Vec<YueMenuItem>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let anchor = self.anchor.clone();
        let state = self.state.clone();
        let parent_view = self.parent_view;
        let shell = div()
            .id((self.id.clone(), "yue-dropdown-region"))
            .relative()
            .flex()
            .flex_col()
            .children(body)
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                anchor.set_point(
                    event.position,
                    Bounds::new(Point::default(), window.viewport_size()),
                );
                // Right-clicking again while the panel is up moves it rather than
                // toggling it away: `show` leaves the open state alone, and the
                // panel is placed from the anchor written just above.
                state.update(cx, |state, cx| state.show(window, cx));
                cx.notify(parent_view);
            });
        let surface = self.panel(items).popup(self.open, window, cx);
        shell
            .when_some(surface, |shell, surface| shell.child(surface))
            .into_any_element()
    }
}

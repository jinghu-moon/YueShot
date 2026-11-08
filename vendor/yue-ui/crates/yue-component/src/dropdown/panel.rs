use super::*;
use gpui_kit::base::A11yDisabledExt as _;

// What a row needs from its panel in order to draw itself.
#[derive(Clone, Copy)]
struct RowContext {
    index: usize,
    /// Whether the panel is open enough to take input: an exiting panel draws its
    /// rows but binds nothing, the reference's `pointer-events: none`.
    interactive: bool,
    active: bool,
    hovered: bool,
    pressed: bool,
    armed: bool,
    entering: bool,
    generation: usize,
    column: MenuColumn,
}

// One panel — the host's own, or the single submenu off its rows.
//
// Every panel is the same code with a different identity: the rows it holds, the
// anchor it floats by, the ramp it rides. None of them owns the open state — the
// host's `PopoverState` does, and a submenu is present exactly while the root
// says the row that opened it is armed.
#[derive(Clone)]
pub(super) struct MenuPanel {
    pub(super) host: ElementId,
    pub(super) path: PanelPath,
    pub(super) kind: MenuKind,
    pub(super) shape: Rc<MenuShape>,
    /// The panel's accessible name — the trigger's for a root panel, the row's for
    /// its submenu. GPUI has no `aria-labelledby`, so the reference's label is
    /// carried onto the panel itself.
    pub(super) name: Option<SharedString>,
    pub(super) items: Rc<Vec<YueMenuItem>>,
    pub(super) state: Entity<PopoverState>,
    pub(super) motion: Entity<MenuMotion>,
    pub(super) parent_view: EntityId,
    /// A live source read after this frame's trigger/row prepaint.
    pub(super) anchor: PopupAnchorHandle,
    /// Every row's bounds, so a submenu can float off the row that opened it.
    pub(super) rows: Entity<Vec<PopupAnchor>>,
    /// The rows the arrow keys stop at, as positions in `items`.
    pub(super) roving: Rc<Vec<usize>>,
    /// Those rows' labels, in the same order, for typeahead.
    pub(super) labels: Rc<Vec<SharedString>>,
    pub(super) scroll: ScrollHandle,
}

impl MenuPanel {
    /// The panel's floating surface, or nothing while it is absent.
    pub(super) fn popup(
        self,
        present: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let theme = self.shape.theme;
        let placement = match self.kind {
            MenuKind::Context => PopupPlacement::Point(Anchor::TopLeft),
            MenuKind::Submenu => PopupPlacement::Side(Placement::Right, Align::Start),
            MenuKind::Trigger => {
                PopupPlacement::Side(self.shape.placement.side(), self.shape.align.align())
            }
        };
        let id = self.popup_id();
        let mut spec = PopupSpec::new(id, self.anchor.clone(), present, placement)
            .surface_gap(gap_for(self.kind, theme))
            .margin(WINDOW_MARGIN)
            .pointer_mode(PopupPointerMode::Block)
            .dismiss_route(PopupDismissRoute::Owner)
            .motion(crate::motion::dropdown_popup(theme));
        if self.path != PanelPath::Root {
            spec = spec.parent(self.root_popup_id());
        }
        if self.kind == MenuKind::Trigger {
            spec = spec.exclude(self.anchor.clone());
        }
        if self.kind.wants_caret() {
            spec = spec.arrow(
                ArrowSpec::new(px(CARET_RADIUS * 2.), px(CARET_RADIUS))
                    .edge_inset(px(CARET_EDGE_MARGIN))
                    .corner_radii([px(pixels(theme, crate::theme::keys::POPOVER_BORDER_RADIUS)); 4])
                    .border_width(px(pixels(theme, crate::theme::keys::POPOVER_BORDER_WIDTH)))
                    .style(
                        rgba(theme, crate::theme::keys::POPOVER_BACKGROUND),
                        rgba(theme, crate::theme::keys::POPOVER_BORDER_COLOR),
                    ),
            );
        }
        let state = self.state.clone();
        let parent_view = self.parent_view;
        spec = spec.on_dismiss(move |_, window, cx| {
            state.update(cx, |state, cx| state.dismiss(window, cx));
            cx.notify(parent_view);
        });
        if let Some(observer) = self.shape.surface_bounds.clone() {
            spec = spec.on_position(move |position| observer(position.bounds));
        }
        let mut outsets: gpui_kit::Edges<Pixels> = gpui_kit::Edges::default();
        for shadow in shadow(theme, crate::theme::keys::POPOVER_SHADOW) {
            let radius = (shadow.blur_radius + shadow.spread_radius).max(px(0.));
            outsets.left = outsets.left.max(radius - shadow.offset.x);
            outsets.right = outsets.right.max(radius + shadow.offset.x);
            outsets.top = outsets.top.max(radius - shadow.offset.y);
            outsets.bottom = outsets.bottom.max(radius + shadow.offset.y);
        }
        spec = spec.visual_outsets(outsets);
        let host = PopupHost::new(spec);
        let available = host.content_size(window);
        host.mount(
            move |presentation, _, window, cx| {
                self.into_panel(presentation.is_interactive(), available, window, cx)
            },
            window,
            cx,
        )
    }

    fn popup_id(&self) -> PopupId {
        PopupId::new(
            self.parent_view,
            (self.host.clone(), self.path.channel("popup")),
        )
    }

    fn root_popup_id(&self) -> PopupId {
        PopupId::new(
            self.parent_view,
            (self.host.clone(), PanelPath::Root.channel("popup")),
        )
    }

    /// The panel's own element: its rows, its caret, and the keyboard and mouse
    /// rules of the whole menu tree when this is the root.
    fn into_panel(
        self,
        present: bool,
        available: gpui_kit::Size<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.shape.theme;
        let now = cx.background_executor().now();
        // The dwell and the pointer decide which submenu is showing, before any
        // row is drawn, so the row it belongs to reads as armed in this frame.
        // Only the root resolves it: the indices are its own rows'.
        let armed = if self.path == PanelPath::Root {
            self.resolve_dwell(now, window, cx)
        } else {
            None
        };
        let (active, hovered, pressed, entering, generation) = {
            let motion = self.motion.read(cx);
            let panel = motion.panel(self.path);
            (
                panel.active,
                panel.hovered,
                panel.pressed,
                panel.is_entering(now),
                panel.generation,
            )
        };
        // A row the panel opens on is lit from its very first sample, and Base
        // hands a fresh channel whatever it is first shown — so the reference's
        // highlight fade is missing exactly where the pointer already rests. Give
        // every row one frame of unlit tint before this open draws its rows.
        if entering
            && self.motion.update(cx, |motion, _| {
                let panel = motion.panel_mut(self.path);
                let fresh = !panel.tint_seeded;
                panel.tint_seeded = true;
                fresh
            })
        {
            for (index, item) in self.items.iter().enumerate() {
                let row_id = self.row_key(item, index);
                transition(
                    (row_id_string(&row_id), "yue-dropdown-tint".to_string()),
                    0.0_f32,
                    Transition::new(Duration::ZERO),
                    window,
                    cx,
                );
            }
        }
        let column = menu_column(&self.items, self.shape.indicator_side);
        let rows: Vec<AnyElement> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                self.row(
                    item,
                    RowContext {
                        index,
                        interactive: present,
                        active: active == Some(index),
                        hovered: hovered == Some(index),
                        pressed: pressed == Some(index),
                        armed: armed == Some(index),
                        entering,
                        generation,
                        column,
                    },
                    window,
                    cx,
                )
            })
            .collect();

        // One level, and the root owns its submenu's surface: a panel inside a
        // scrollable list would be clipped by it, so the tree hangs off the
        // panel rather than off the row.
        let mut submenu_panels = Vec::new();
        let mut surfaces_of_submenus = Vec::new();
        if self.path == PanelPath::Root {
            for (index, item) in self.items.iter().enumerate() {
                if item.kind != MenuRowKind::Submenu {
                    continue;
                }
                let submenu = self.submenu(index, item, window, cx);
                if let Some(surface) =
                    submenu
                        .clone()
                        .popup(present && armed == Some(index), window, cx)
                {
                    surfaces_of_submenus.push(surface);
                }
                submenu_panels.push((index, submenu));
            }
        }

        let viewport = window.viewport_size();
        let (requested, ceiling) = self.shape.bounds(viewport);
        // A caller-given `panel_width` and a grid both ask for a definite width, the
        // reference's `width:min(var(--mw), …)`; the `--menu-width` token default is
        // only where a content-sized panel stops shrinking.
        let fixed = self.shape.panel_width.is_some() || self.shape.columns > 1;
        let ceiling = ceiling.min(available.width);
        let width = requested.min(ceiling);
        let chrome = px(2.
            * (pixels(theme, crate::theme::keys::POPOVER_PADDING)
                + pixels(theme, crate::theme::keys::POPOVER_BORDER_WIDTH)));
        let max_height = self
            .shape
            .max_height
            .unwrap_or((viewport.height * 0.5).min(px(450.)))
            .min((available.height - chrome).max(px(0.)));
        let scroll = self.scroll.clone();
        let mut content = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(pixels(theme, crate::theme::keys::SPACE_2)))
            .when(!present, |content| content.top(scroll.offset().y))
            .children(rows);
        if self.shape.columns > 1 {
            content = content
                .grid()
                .grid_cols(self.shape.columns as u16)
                .gap(px(pixels(theme, crate::theme::keys::SPACE_4)));
        }
        let list = div()
            .id((self.host.clone(), self.path.channel("items")))
            .w_full()
            .max_h(max_height)
            .when(present, |list| {
                list.overflow_y_scroll().track_scroll(&scroll)
            })
            .when(!present, |list| list.overflow_hidden())
            .child(content);

        let mut panel = div()
            .id((self.host.clone(), self.path.channel("panel")))
            // The panel is the focusable container: its name, its role and whether
            // it currently holds the keyboard are all worth seeing from the tree.
            .test_support()
            .when(present, |panel| {
                panel
                    .role(Role::Menu)
                    .when_some(self.name.clone(), |panel, name| panel.aria_label(name))
            })
            .relative()
            .flex()
            .flex_col()
            // Both edges land on the panel, whose box is what the rows are painted
            // on: putting the ceiling anywhere else lets a wide row outgrow the
            // surface behind it.
            .when(fixed, |panel| panel.w(width))
            .when(!fixed, |panel| panel.min_w(width))
            .max_w(ceiling)
            .p(rems(theme, crate::theme::keys::POPOVER_PADDING))
            .bg(rgba(theme, crate::theme::keys::POPOVER_BACKGROUND))
            .border(rems(theme, crate::theme::keys::POPOVER_BORDER_WIDTH))
            .border_color(rgba(theme, crate::theme::keys::POPOVER_BORDER_COLOR))
            .rounded(rems(theme, crate::theme::keys::POPOVER_BORDER_RADIUS))
            .shadow(shadow(theme, crate::theme::keys::POPOVER_SHADOW))
            .text_color(rgba(theme, crate::theme::keys::POPOVER_COLOR))
            .text_size(rems(theme, crate::theme::keys::POPOVER_FONT_SIZE))
            .child(list)
            .children(surfaces_of_submenus);
        if self.path == PanelPath::Root {
            panel = self.bind_root(panel, present, submenu_panels, window, cx);
        }
        panel.into_any_element()
    }

    /// The panel of the submenu off root row `index`, sharing its host's state,
    /// its surface list and its row recorder.
    fn submenu(
        &self,
        index: usize,
        item: &YueMenuItem,
        window: &mut Window,
        cx: &mut App,
    ) -> MenuPanel {
        let path = PanelPath::Submenu(index);
        let items: Vec<YueMenuItem> = item
            .children
            .iter()
            // One level only: a child that carries its own submenu is drawn as
            // the plain command it also is, rather than opening a third panel.
            .map(|child| {
                if child.kind == MenuRowKind::Submenu {
                    YueMenuItem {
                        kind: MenuRowKind::Command,
                        children: Vec::new(),
                        ..child.clone()
                    }
                } else {
                    child.clone()
                }
            })
            .collect();
        let (roving, labels) = roving_of(&items);
        let anchor = self.rows.read(cx)[index].handle();
        let scroll = window
            .use_keyed_state((self.host.clone(), path.channel("scroll")), cx, |_, _| {
                ScrollHandle::new()
            })
            .read(cx)
            .clone();
        MenuPanel {
            path,
            kind: MenuKind::Submenu,
            name: Some(item.label.clone()),
            items: Rc::new(items),
            anchor,
            roving: Rc::new(roving),
            labels: Rc::new(labels),
            scroll,
            ..self.clone()
        }
    }

    /// Which submenu is showing, once the pointer has rested on its row.
    fn resolve_dwell(&self, now: Instant, window: &mut Window, cx: &mut App) -> Option<usize> {
        let items = self.items.clone();
        let (armed, waiting) = self.motion.update(cx, |motion, _| {
            let Some(hovered) = motion.root.hovered else {
                // The pointer left the rows without arriving at another one, which
                // is not a demand to close: the reference drops a submenu when a
                // row takes it, never when the pointer wanders off, and that is
                // what lets the pointer cross the gap into an open submenu.
                return (motion.armed, false);
            };
            let Some(item) = items.get(hovered) else {
                return (motion.armed, false);
            };
            if item.kind != MenuRowKind::Submenu {
                motion.arm(None, None, now);
                return (motion.armed, false);
            }
            if !dwell_elapsed(motion.root.dwell(now)) {
                return (motion.armed, true);
            }
            motion.arm(Some(hovered), first_roving(&item.children), now);
            (motion.armed, false)
        });
        // The clock is what turns a rest into an open, so a dwell that has not
        // paid yet has to ask for the frame that pays it.
        if waiting {
            window.request_animation_frame();
        }
        armed
    }

    /// Focus, keyboard and the outside press, on the panel that owns them.
    ///
    /// Only the root binds these: a deferred panel is a descendant of the node
    /// that holds focus, so its key events reach the root's handler, which routes
    /// them to the panel that is showing — the same shape as the reference's one
    /// keydown ladder.
    fn bind_root<E>(
        &self,
        panel: E,
        present: bool,
        submenus: Vec<(usize, MenuPanel)>,
        window: &mut Window,
        cx: &mut App,
    ) -> E
    where
        E: gpui_kit::StatefulInteractiveElement,
    {
        if !present {
            return panel;
        }
        let focus_handle = self.state.read(cx).focus_handle(cx);
        let panel = panel.key_context(KEY_CONTEXT).track_focus(&focus_handle);
        let root = self.clone();
        let submenus = Rc::new(submenus);
        let panel = panel.on_key_down(move |event: &KeyDownEvent, window, cx| {
            root.handle_key(event, &submenus, window, cx);
        });
        let runtime = PopupRuntime::for_window(window, cx);
        let popup_id = self.popup_id();
        let state = self.state.clone();
        let motion = self.motion.clone();
        let parent_view = self.parent_view;
        panel.on_mouse_down_out(move |event, window, cx| {
            if runtime.owner_press_suppressed()
                || runtime.contains_or_excluded(&popup_id, event.position)
            {
                return;
            }
            motion.update(cx, |motion, _| motion.armed = None);
            runtime.mark_owner_press_routed();
            state.update(cx, |state, cx| state.dismiss(window, cx));
            cx.notify(parent_view);
        })
    }
}

// The colour of a row's own text.
//
// A destructive command takes the action's danger colour rather than the menu's:
// the reference paints `.mi.danger` with `--danger-fg` and nothing else about the
// row changes.
fn row_color(item: &YueMenuItem, theme: YueTheme) -> gpui_kit::Rgba {
    if item.disabled {
        rgba(theme, crate::theme::keys::DISABLED_CONTENT)
    } else if item.danger {
        rgba(theme, crate::theme::keys::ACTION_DANGER)
    } else {
        rgba(theme, crate::theme::keys::MENU_ITEM_COLOR)
    }
}

// The colour of a row's companion marks — its icon, its chevron, its shortcut.
fn row_glyph_color(item: &YueMenuItem, theme: YueTheme) -> gpui_kit::Rgba {
    if item.disabled {
        rgba(theme, crate::theme::keys::DISABLED_CONTENT)
    } else {
        rgba(theme, crate::theme::keys::TEXT_SECONDARY)
    }
}

// The role a row reports, which is what tells assistive technology whether the
// row carries a state or runs a command.
fn row_role(item: &YueMenuItem) -> Role {
    match item.mark {
        MenuMark::Check(_) => Role::MenuItemCheckBox,
        MenuMark::Radio(_) => Role::MenuItemRadio,
        MenuMark::None => Role::MenuItem,
    }
}

impl MenuPanel {
    /// The id a row's ramps key off: the host, which panel of the tree this row
    /// is in, and the row's own name.
    fn row_key(&self, item: &YueMenuItem, index: usize) -> ElementId {
        SharedString::from(format!(
            "{}-{}-{}",
            row_id_string(&self.host),
            self.path.channel("row"),
            item.key(index),
        ))
        .into()
    }

    /// One row of this panel, in whatever shape its kind asks for.
    fn row(
        &self,
        item: &YueMenuItem,
        ctx: RowContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.shape.theme;
        let row_id = self.row_key(item, ctx.index);
        let highlight = ctx.active || ctx.hovered;
        let (enter, rise) = row_entrance(&row_id, ctx, theme, window, cx);
        let tint = row_tint(&row_id, ctx, highlight, theme, window, cx);
        let shell = div()
            .id(row_id.clone())
            .when_some(item.description.clone(), |row, description| {
                row.aria_description(description)
            })
            // Rows carry their own name and role, so the tree — and a headless test
            // — can see which row a screen reader would land on.
            .test_support()
            .w_full()
            .min_h(px(pixels(theme, crate::theme::keys::SIZE_32)))
            .flex()
            .items_center()
            .rounded(rems(theme, crate::theme::keys::RADIUS_SM))
            .bg(mix_rgba(
                rgba(theme, crate::theme::keys::POPOVER_BACKGROUND),
                rgba(theme, crate::theme::keys::MENU_ITEM_COLOR),
                tint,
            ))
            .text_color(row_color(item, theme))
            // The rows' entrance is a 4px fall from the panel's top edge, which is
            // the reference's `translateY(-4px)`. A relative offset is what gpui
            // gives for free: its own `Style::position` defaults to relative, so
            // `top` shifts the row without disturbing its siblings' layout.
            .opacity(enter)
            .top(rise);

        match item.kind {
            MenuRowKind::Info => shell
                .when(ctx.interactive, |row| {
                    row.role(Role::Caption).aria_label(item.label.clone())
                })
                .px(rems(theme, crate::theme::keys::SPACE_8))
                .py(rems(theme, crate::theme::keys::SPACE_4))
                .gap(rems(theme, crate::theme::keys::SPACE_8))
                .text_size(rems(theme, crate::theme::keys::FONT_SIZE_MD))
                .text_color(rgba(
                    theme,
                    if item.disabled {
                        crate::theme::keys::TEXT_DISABLED
                    } else {
                        crate::theme::keys::TEXT_PRIMARY
                    },
                ))
                .when_some(item.icon, |row, icon| {
                    row.child(icon.size(rems(theme, crate::theme::keys::ICON_SIZE_SM)))
                })
                .child(item.label.clone())
                .a11y_disabled(ctx.interactive && item.disabled)
                .into_any_element(),
            MenuRowKind::Separator => shell
                // A hairline is not a row: the menu does not stop here, and gpui
                // has no separator role to ask for.
                .h(px(1.))
                .min_h(px(1.))
                .mx(px(pixels(theme, crate::theme::keys::SPACE_4)))
                .my(px(pixels(theme, crate::theme::keys::SPACE_2)))
                .bg(rgba(theme, crate::theme::keys::POPOVER_BORDER_COLOR))
                .rounded(px(1.))
                .into_any_element(),
            MenuRowKind::Heading => shell
                .when(ctx.interactive, |row| row.role(Role::Caption))
                .min_h(px(pixels(theme, crate::theme::keys::SIZE_24)))
                .px(px(pixels(theme, crate::theme::keys::SPACE_8)))
                .py(px(pixels(theme, crate::theme::keys::SPACE_4)))
                .text_size(rems(theme, crate::theme::keys::FONT_SIZE_SM))
                .font_weight(FontWeight(number(
                    theme,
                    crate::theme::keys::WEIGHT_SEMIBOLD,
                )))
                .text_color(rgba(theme, crate::theme::keys::TEXT_SECONDARY))
                .child(item.label.clone())
                .when(item.rule, |this| {
                    // The reference's ruled caption puts its hairline after the
                    // text, not in place of a row.
                    this.child(
                        div()
                            .flex_1()
                            .ml(px(pixels(theme, crate::theme::keys::SPACE_8)))
                            .h(px(1.))
                            .bg(rgba(theme, crate::theme::keys::POPOVER_BORDER_COLOR)),
                    )
                })
                .into_any_element(),
            MenuRowKind::Loading => shell
                .when(ctx.interactive, |row| row.role(Role::Caption))
                .px(px(pixels(theme, crate::theme::keys::SPACE_8)))
                .text_size(rems(theme, crate::theme::keys::FONT_SIZE_SM))
                .text_color(rgba(theme, crate::theme::keys::TEXT_SECONDARY))
                .child(
                    Icon::new(YueIcon::Loading)
                        .size(rems(theme, crate::theme::keys::SIZE_16))
                        .color_value(rgba(theme, crate::theme::keys::ACCENT_TEXT))
                        .with_animation(
                            (row_id.clone(), "yue-dropdown-loading"),
                            Animation::new(duration(theme, crate::theme::keys::SPINNER_DURATION))
                                .repeat(),
                            |icon, delta| icon.rotation(delta),
                        ),
                )
                .child(item.label.clone())
                .into_any_element(),
            MenuRowKind::Command | MenuRowKind::Submenu => {
                let submenu = item.kind == MenuRowKind::Submenu;
                let mut row = shell
                    .when(ctx.interactive, |row| {
                        row.role(row_role(item))
                            .aria_label(item.label.clone())
                            .aria_selected(ctx.active)
                    })
                    .text_color(row_color(item, theme));
                if ctx.interactive && ctx.active {
                    // The panel is what holds focus, so the highlighted row is
                    // announced through the container rather than by moving real
                    // focus to every row.
                    row = row.aria_active_descendant();
                }
                if let Some(hotkey) = item.hotkey.clone().filter(|_| ctx.interactive) {
                    row = row.aria_keyshortcuts(hotkey);
                }
                match if ctx.interactive {
                    item.mark
                } else {
                    MenuMark::None
                } {
                    MenuMark::Check(checked) => row = row.aria_toggled(Toggled::from(checked)),
                    MenuMark::Radio(selected) => row = row.aria_selected(selected),
                    MenuMark::None => {}
                }
                if ctx.interactive && submenu {
                    row = row.aria_expanded(ctx.armed);
                }
                if self.kind != MenuKind::Submenu {
                    // Only the root's rows are recorded: they are the anchors a
                    // submenu floats by.
                    row = row.child(self.rows.read(cx)[ctx.index].capture());
                }
                row.child(self.row_content(item, &ctx, submenu, window, cx))
                    .when(ctx.interactive && !item.disabled, |row| {
                        self.bind_row(row, ctx.index)
                    })
                    // The reference's `.dd .mi:disabled{cursor:not-allowed}`. A row
                    // with no handler still gets the cursor, because gpui registers a
                    // hitbox for any box that names a `mouse_cursor`.
                    .when(ctx.interactive && item.disabled, |row| {
                        row.cursor_not_allowed()
                    })
                    .a11y_disabled(ctx.interactive && item.disabled)
                    .into_any_element()
            }
        }
    }

    /// Everything between the row's two edges: the mark slot, the icon, the
    /// initials, the label, the shortcut, the chevron.
    fn row_content(
        &self,
        item: &YueMenuItem,
        ctx: &RowContext,
        submenu: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.shape.theme;
        let space = px(pixels(theme, crate::theme::keys::SPACE_8));
        let glyph = row_glyph_color(item, theme);
        let mut content = div().w_full().flex().items_center().gap(space).px(space);
        if ctx.column == MenuColumn::Leading {
            content = content.child(self.indicator(item, ctx, theme, window, cx));
        }
        if let Some(icon) = item.icon {
            content = content.child(
                div().flex_shrink_0().child(
                    icon.size(rems(theme, crate::theme::keys::SIZE_16))
                        .color_value(glyph),
                ),
            );
        }
        if let Some(initials) = item.avatar.clone() {
            content = content.child(
                div()
                    .flex_shrink_0()
                    .size(px(pixels(theme, crate::theme::keys::SIZE_20)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(rems(theme, crate::theme::keys::RADIUS_FULL))
                    .bg(rgba(theme, crate::theme::keys::ACCENT_SUBTLE))
                    .text_color(rgba(theme, crate::theme::keys::ACCENT_SUBTLE_TEXT))
                    .text_size(rems(theme, crate::theme::keys::FONT_SIZE_SM))
                    .font_weight(FontWeight(number(theme, crate::theme::keys::WEIGHT_MEDIUM)))
                    .child(initials),
            );
        }
        content = content.child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(item.label.clone()),
        );
        if ctx.column == MenuColumn::Trailing {
            content = content.child(self.indicator(item, ctx, theme, window, cx));
        }
        if let Some(hotkey) = item.hotkey.clone().filter(|_| ctx.interactive) {
            // The hint is the row's trailing edge, so the pixels it lands on are
            // worth seeing from the tree: they are what says the mark column, when
            // there is one, sits before it rather than after.
            let hint_id: ElementId = SharedString::from(format!(
                "{}-{}-{}-hint",
                row_id_string(&self.host),
                self.path.channel("row"),
                item.key(ctx.index),
            ))
            .into();
            content = content.child(
                div()
                    .id(hint_id)
                    .test_support()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_size(rems(theme, crate::theme::keys::FONT_SIZE_SM))
                    .text_color(glyph)
                    .child(hotkey),
            );
        }
        if submenu {
            content = content.child(
                div().flex_shrink_0().child(
                    Icon::new(YueIcon::ChevronRight)
                        .size(rems(theme, crate::theme::keys::SIZE_16))
                        .color_value(glyph),
                ),
            );
        }
        content.into_any_element()
    }

    /// The row's check tick or radio dot, in the slot the menu reserves for it.
    fn indicator(
        &self,
        item: &YueMenuItem,
        ctx: &RowContext,
        theme: YueTheme,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let box_size = px(pixels(theme, crate::theme::keys::SIZE_16));
        let slot = div().flex_shrink_0().size(box_size).relative();
        match item.mark {
            MenuMark::None => slot.into_any_element(),
            MenuMark::Check(checked) => {
                // The tick is written on the checkbox's two clocks: fast and
                // easing out as it is erased, slower — after a beat — as it is
                // inked.
                let policy = if checked {
                    Transition::new(duration(theme, crate::theme::keys::POPOVER_DURATION_ENTER))
                        .delay(duration(theme, crate::theme::keys::POPOVER_DURATION_EXIT))
                        .ease(easing(theme, crate::theme::keys::MOTION_EASE_ENTER))
                } else {
                    Transition::new(duration(theme, crate::theme::keys::POPOVER_DURATION_EXIT))
                        .ease(easing(theme, crate::theme::keys::MOTION_EASE_EXIT))
                };
                let ink = transition(
                    (self.row_channel(ctx.index), "yue-dropdown-check"),
                    f32::from(checked),
                    policy,
                    window,
                    cx,
                )
                .clamp(0.0, 1.0);
                slot.child(
                    canvas(
                        move |_, _, _| (),
                        move |bounds, _, window, _| {
                            crate::icon::paint_mark(
                                window,
                                bounds,
                                &crate::icon::CHECK_LINE,
                                ink,
                                f32::from(bounds.size.width) / crate::icon::MARK_BOX,
                                2.0,
                                gpui_kit::Hsla::from(rgba(theme, crate::theme::keys::ACCENT_TEXT)),
                            )
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
                .into_any_element()
            }
            MenuMark::Radio(selected) => {
                // The ring's colour and the dot's size ride one ramp, so the dot
                // cannot arrive before the ring has finished turning accent.
                let progress = transition(
                    (self.row_channel(ctx.index), "yue-dropdown-radio"),
                    f32::from(selected),
                    Transition::new(duration(theme, crate::theme::keys::POPOVER_DURATION_ENTER))
                        .ease(easing(theme, crate::theme::keys::MOTION_EASE_ENTER)),
                    window,
                    cx,
                )
                .clamp(0.0, 1.0);
                slot.child(
                    div()
                        .absolute()
                        .inset(px(1.))
                        .border(px(pixels(theme, crate::theme::keys::POPOVER_BORDER_WIDTH)))
                        .border_color(mix_rgba(
                            rgba(theme, crate::theme::keys::BORDER_CONTROL),
                            rgba(theme, crate::theme::keys::ACCENT_TEXT),
                            progress,
                        ))
                        .rounded(rems(theme, crate::theme::keys::RADIUS_FULL)),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .size(px(8.0 * progress))
                                .rounded(rems(theme, crate::theme::keys::RADIUS_FULL))
                                .bg(rgba(theme, crate::theme::keys::ACCENT_TEXT)),
                        ),
                )
                .into_any_element()
            }
        }
    }

    /// The row's identity in the motion channels, so two rows never share a ramp.
    fn row_channel(&self, index: usize) -> String {
        format!("{}-{}", self.path.channel("row"), index)
    }

    /// Pointer rules of a live row: the highlight follows the pointer, a press
    /// reads as pressed, and a click runs the row.
    ///
    /// Generic over the row's own element because a headless build wraps it in the
    /// observation layer: the row this binds is the same row either way.
    fn bind_row<E>(&self, row: E, index: usize) -> E
    where
        E: gpui_kit::StatefulInteractiveElement + gpui_kit::Styled,
    {
        let path = self.path;
        let parent_view = self.parent_view;
        let click = self.clone();
        let down = self.clone();
        let up = self.clone();
        let out = self.clone();
        let hover = self.clone();
        row.cursor_pointer()
            .on_click(move |event, window, cx| click.activate(index, event, window, cx))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                down.press(Some(index), cx);
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| up.press(None, cx))
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| out.press(None, cx))
            .on_hover(move |entered, _, cx| {
                let now = cx.background_executor().now();
                hover.motion.update(cx, |motion, _| {
                    let panel = motion.panel_mut(path);
                    if *entered {
                        // The pointer marks the row it rests on, as the reference's
                        // mouseover handler does: there is one highlight, whichever
                        // way it arrived, and a row the pointer rests on reads the
                        // same as one an arrow key landed on.
                        panel.hovered = Some(index);
                        panel.hovered_at = Some(now);
                        panel.active = Some(index);
                    } else if panel.hovered == Some(index) {
                        panel.hovered = None;
                        panel.hovered_at = None;
                    }
                });
                cx.notify(parent_view);
            })
    }

    /// Run a row: its handler, then what its own kind says the menu does next.
    ///
    /// A submenu row opens its panel and stays, a row that asked to keep open
    /// stays, and anything else dismisses the menu — which is the path that hands
    /// focus back to the trigger.
    fn activate(&self, index: usize, event: &ClickEvent, window: &mut Window, cx: &mut App) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        let handler = item.on_click.clone();
        let submenu = item.kind == MenuRowKind::Submenu;
        let keep_open = item.keep_open || submenu;
        let first = submenu.then(|| first_roving(&item.children)).flatten();
        let now = cx.background_executor().now();
        let path = self.path;
        if let Some(handler) = &handler {
            handler(event, window, cx);
        }
        self.motion.update(cx, |motion, _| {
            motion.panel_mut(path).pressed = None;
            if submenu {
                motion.arm(Some(index), first, now);
            } else {
                motion.armed = None;
            }
        });
        if !keep_open {
            let state = self.state.clone();
            state.update(cx, |state, cx| state.dismiss(window, cx));
        }
        cx.notify(self.parent_view);
    }

    /// Which row reads as pressed — the reference's active state, released as much
    /// by a pointer that leaves the row as by one that lets go inside it.
    fn press(&self, index: Option<usize>, cx: &mut App) {
        let path = self.path;
        self.motion
            .update(cx, |motion, _| motion.panel_mut(path).pressed = index);
        cx.notify(self.parent_view);
    }

    /// Move the highlight through the rows the keyboard stops at and bring the row
    /// it lands on back into the panel: the reference's step, plus a nearest-edge
    /// scroll for the row it lands on.
    fn step(&self, move_: MenuMove, cx: &mut App) {
        let columns = self.shape.columns;
        let roving = self.roving.clone();
        let path = self.path;
        let landing = self.motion.update(cx, |motion, _| {
            let panel = motion.panel_mut(path);
            let current = slot_of(&roving, panel.active);
            let landing = move_index(roving.len(), current, move_, columns)
                .and_then(|position| roving.get(position).copied());
            panel.active = landing;
            landing
        });
        self.reveal(landing, cx);
    }

    /// Type one character into this panel's prefix buffer, and mark the row it
    /// finds — or leave the highlight alone when it finds nothing, which is also
    /// the answer the panel gives the window: an unused keystroke is not swallowed.
    fn type_char(&self, ch: char, cx: &mut App) -> bool {
        let now = cx.background_executor().now();
        let labels = self.labels.clone();
        let roving = self.roving.clone();
        let path = self.path;
        let landing = self.motion.update(cx, |motion, _| {
            let panel = motion.panel_mut(path);
            let current = slot_of(&roving, panel.active);
            let found = panel
                .type_into(&labels, current, now, ch)
                .and_then(|position| roving.get(position).copied());
            if let Some(index) = found {
                panel.active = Some(index);
            }
            found
        });
        self.reveal(landing, cx);
        landing.is_some()
    }

    /// Scroll `index` into this panel's own viewport and redraw the host for it.
    fn reveal(&self, index: Option<usize>, cx: &mut App) {
        if let Some(index) = index {
            self.scroll.scroll_to_item(index);
        }
        cx.notify(self.parent_view);
    }

    /// Whether the highlighted row is one that carries a submenu — the row the
    /// right arrow opens, and the only kind a panel below the root cannot have.
    fn highlighted_submenu(&self, active: Option<usize>) -> Option<usize> {
        active.filter(|index| {
            matches!(self.items.get(*index), Some(item) if item.kind == MenuRowKind::Submenu)
        })
    }

    /// Open the submenu off root row `index` without waiting out the dwell, which
    /// is what the right arrow and a click on the row both do.
    fn arm_row(&self, index: usize, cx: &mut App) {
        let now = cx.background_executor().now();
        let first = self
            .items
            .get(index)
            .and_then(|item| first_roving(&item.children));
        self.motion
            .update(cx, |motion, _| motion.arm(Some(index), first, now));
        cx.notify(self.parent_view);
    }

    /// Close the showing submenu and put the highlight back on the row that opened
    /// it, which is what the reference's submenu close ends with.
    fn close_submenu(&self, cx: &mut App) {
        let armed = self.motion.read(cx).armed;
        self.motion.update(cx, |motion, _| {
            motion.armed = None;
            motion.root.active = armed;
        });
        self.reveal(armed, cx);
    }

    /// The menu's whole keyboard ladder, in the reference's order.
    ///
    /// The keys are read where the focus is — the root panel — and applied to the
    /// panel the user is looking at, so a down arrow inside a submenu moves inside
    /// it. A key the menu has no rule for is left to the window.
    fn handle_key(
        &self,
        event: &KeyDownEvent,
        submenus: &[(usize, MenuPanel)],
        window: &mut Window,
        cx: &mut App,
    ) {
        let armed = self.motion.read(cx).armed;
        let showing = armed
            .and_then(|index| {
                submenus
                    .iter()
                    .find(|(slot, _)| *slot == index)
                    .map(|(_, submenu)| submenu.clone())
            })
            .unwrap_or_else(|| self.clone());
        let active = {
            let motion = self.motion.read(cx);
            motion.panel(showing.path).active
        };
        let key = event.keystroke.key.as_str();
        let consumed = match key {
            "escape" => {
                if armed.is_some() {
                    self.close_submenu(cx);
                } else {
                    let state = self.state.clone();
                    state.update(cx, |state, cx| state.dismiss(window, cx));
                }
                true
            }
            // Tab leaves the menu with the focus it came in on: the panel does not
            // keep the keyboard hostage, and a Yue menu has no text field to reach.
            "tab" => {
                let state = self.state.clone();
                state.update(cx, |state, cx| state.dismiss(window, cx));
                true
            }
            "enter" | "space" => {
                let Some(index) = active else {
                    return;
                };
                let click = ClickEvent::Keyboard(KeyboardClickEvent {
                    button: if key == "space" {
                        KeyboardButton::Space
                    } else {
                        KeyboardButton::Enter
                    },
                    bounds: Bounds::default(),
                });
                showing.activate(index, &click, window, cx);
                true
            }
            // One level only, so the right arrow has exactly one thing to do in the
            // root; a column move is for a panel that runs in more than one.
            "right" => {
                if let Some(index) = showing.highlighted_submenu(active) {
                    self.arm_row(index, cx);
                    true
                } else if self.shape.columns > 1 {
                    showing.step(MenuMove::Column(1), cx);
                    true
                } else {
                    false
                }
            }
            "left" => {
                if armed.is_some() {
                    self.close_submenu(cx);
                    true
                } else if self.shape.columns > 1 {
                    showing.step(MenuMove::Column(-1), cx);
                    true
                } else {
                    false
                }
            }
            other => match move_for(other) {
                Some(move_) => {
                    showing.step(move_, cx);
                    true
                }
                None => match typeable(other, &event.keystroke.modifiers) {
                    Some(ch) => showing.type_char(ch, cx),
                    None => false,
                },
            },
        };
        if consumed {
            cx.stop_propagation();
        }
    }
}

// A row's entrance: an opacity ramp and the 4px rise that comes with it.
fn row_entrance(
    row_id: &ElementId,
    ctx: RowContext,
    theme: YueTheme,
    window: &mut Window,
    cx: &mut App,
) -> (f32, Pixels) {
    if !ctx.entering {
        return (1.0, px(0.));
    }
    let delay = Stagger::new(ROW_STAGGER, StaggerOrigin::First).delay(ctx.index, ctx.index + 1);
    // Two endpoint frames: the row's own progress is what stands in for the
    // opacity it fades in through and the 4px it falls from, so the timing below
    // carries both the stagger delay and the reference's enter curve.
    let keyframes = Keyframes::try_new([Keyframe::new(0.0, 0.0_f32), Keyframe::new(1.0, 1.0)])
        .expect("a row entrance is a pair of endpoints, which the frame rules allow");
    let timing = Timing::new(duration(theme, crate::theme::keys::POPOVER_DURATION_ENTER))
        .delay(SignedDuration::positive(delay))
        .ease(curve(theme, crate::theme::keys::MOTION_EASE_ENTER));
    // The generation in the channel is what makes the next open replay the
    // entrance instead of resuming this one, already finished.
    let channel = format!("{}-enter-{}", row_id_string(row_id), ctx.generation);
    let progress = animate_keyframes(channel, &keyframes, timing, window, cx).value;
    (progress, px(4.0 * (progress - 1.0)))
}

// A row's highlight, on one channel: the hover and the keyboard tint share the
// reference's `--hover`, so a row that is both reads as one state, not two.
fn row_tint(
    row_id: &ElementId,
    ctx: RowContext,
    highlight: bool,
    theme: YueTheme,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    let target = if ctx.pressed {
        number(theme, crate::theme::keys::OPACITY_PRESSED)
    } else if highlight {
        number(theme, crate::theme::keys::OPACITY_HOVER)
    } else {
        0.0
    };
    transition(
        (row_id_string(row_id), "yue-dropdown-tint".to_string()),
        target,
        // Fluent's balanced easy-ease keeps the onset gentle and settles the
        // color smoothly; menu feedback is independent of panel dismissal.
        Transition::new(duration(theme, crate::theme::keys::MENU_ITEM_DURATION))
            .ease(easing(theme, crate::theme::keys::MENU_ITEM_EASE)),
        window,
        cx,
    )
}

// An `ElementId` as a channel prefix, so a row's ramps key off the row itself.
fn row_id_string(id: &ElementId) -> String {
    format!("{id:?}")
}

// The keyed state a menu host owns: Base's open lifecycle, Yue's row motion, and
// the bounds the panel floats from.
//
// The hosts differ only in what they anchor to and what opens them, so the state
// and the panel construction live here once.

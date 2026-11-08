//! breadcrumb element composition.

use super::*;

use crate::{
    DropdownAlign, DropdownPlacement, YueDropdownMenu, YueMenuItem,
    theme::keys,
    tokens::{rems, rgba},
};

use action::TextAction;

use gpui_kit::base::{PopoverState, TestSupportExt as _};

use gpui_kit::{
    App, AppContext as _, ElementId, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Role, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _,
};

use model::{PathPlan, plan};

use std::{collections::HashMap, ops::Range, rc::Rc};

struct Runtime {
    items: Vec<YueBreadcrumbItem>,
    hidden: Range<usize>,
    collapse: Option<BreadcrumbCollapse>,
    generation: u64,
    handler: Option<NavigateHandler>,
    current_clickable: bool,
    focus: HashMap<SharedString, FocusHandle>,
    overflow: FocusHandle,
    scroll: ScrollHandle,
    last_focused: Option<SharedString>,
    last_rem: gpui_kit::Pixels,
    overflow_focused: bool,
    geometry: Rc<std::cell::Cell<gpui_kit::Pixels>>,
    menu: Entity<PopoverState>,
}

fn dispatch(
    runtime: &Entity<Runtime>,
    generation: u64,
    value: &SharedString,
    source: BreadcrumbNavigateSource,
    window: &mut Window,
    cx: &mut App,
) {
    let state = runtime.read(cx);
    if state.generation != generation {
        return;
    }
    let Some(ix) = state
        .items
        .iter()
        .position(|item| &item.value == value && !item.disabled)
    else {
        return;
    };
    let allowed = match source {
        BreadcrumbNavigateSource::Item => {
            !state.hidden.contains(&ix) && (ix + 1 < state.items.len() || state.current_clickable)
        }
        BreadcrumbNavigateSource::OverflowMenu => {
            state.hidden.contains(&ix) && state.menu.read(cx).is_open()
        }
    };
    let handler = allowed.then(|| state.handler.clone()).flatten();
    if let Some(handler) = handler {
        handler(
            &BreadcrumbNavigate {
                value: value.clone(),
                source,
            },
            window,
            cx,
        );
    }
}

impl RenderOnce for YueBreadcrumb {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = if self.inherited {
            crate::current_theme(cx)
        } else {
            self.theme
        };
        let PathPlan { visible, hidden } = plan(&self.items, self.collapse);
        let runtime =
            window.use_keyed_state((self.id.clone(), "breadcrumb-runtime"), cx, |window, cx| {
                let menu = cx.new(|cx| PopoverState::new(false, cx));
                Runtime {
                    items: vec![],
                    hidden: 0..0,
                    collapse: None,
                    generation: 0,
                    handler: None,
                    current_clickable: false,
                    focus: HashMap::new(),
                    overflow: cx.focus_handle(),
                    scroll: ScrollHandle::new(),
                    last_focused: None,
                    last_rem: window.rem_size(),
                    overflow_focused: false,
                    geometry: Rc::new(std::cell::Cell::new(gpui_kit::px(0.))),
                    menu,
                }
            });
        let changed = {
            let old = runtime.read(cx);
            old.collapse != self.collapse
                || old
                    .items
                    .iter()
                    .map(|i| &i.value)
                    .ne(self.items.iter().map(|i| &i.value))
        };
        let menu = runtime.read(cx).menu.clone();
        let old_focused = runtime
            .read(cx)
            .focus
            .iter()
            .find(|(_, focus)| focus.is_focused(window))
            .map(|(v, _)| v.clone());
        let overflow_was_focused = runtime.read(cx).overflow.is_focused(window);
        let menu_was_open = menu.read(cx).is_open();
        if changed && menu_was_open {
            menu.update(cx, |s, cx| s.sync_open(false, window, cx));
        }
        let (generation, scroll, overflow_focus) = runtime.update(cx, |rt, cx| {
            if changed {
                rt.generation += 1;
            }
            rt.items = self.items.clone();
            rt.hidden = hidden.clone();
            rt.collapse = self.collapse;
            rt.current_clickable = self.current_clickable;
            rt.handler = self.handler.clone();
            rt.focus
                .retain(|key, _| self.items.iter().any(|i| &i.value == key));
            for item in &self.items {
                rt.focus
                    .entry(item.value.clone())
                    .or_insert_with(|| cx.focus_handle());
            }
            (rt.generation, rt.scroll.clone(), rt.overflow.clone())
        });
        let focused_is_valid = old_focused.as_ref().is_none_or(|value| {
            visible.iter().flatten().any(|&ix| {
                self.items[ix].value == *value
                    && !self.items[ix].disabled
                    && self.handler.is_some()
                    && (ix + 1 < self.items.len() || self.current_clickable)
            })
        });
        let restore_removed = !focused_is_valid
            || (hidden.is_empty()
                && (overflow_was_focused || (menu_was_open && overflow_focus.is_focused(window))));
        if restore_removed {
            if old_focused
                .as_ref()
                .is_some_and(|value| hidden.clone().any(|ix| self.items[ix].value == *value))
            {
                window.focus(&overflow_focus, cx);
            } else if let Some(ix) = visible.iter().flatten().copied().rev().find(|&ix| {
                !self.items[ix].disabled
                    && self.handler.is_some()
                    && (ix + 1 < self.items.len() || self.current_clickable)
            }) {
                let focus = runtime.read(cx).focus[&self.items[ix].value].clone();
                window.focus(&focus, cx);
            } else if let Some(previous) = window.focused(cx) {
                window.on_next_frame(move |w, cx| {
                    if previous.is_focused(w) {
                        w.focus_next(cx);
                    }
                });
            }
        }
        if self.items.is_empty() {
            return div().id(self.id).into_any_element();
        }
        let current = self.items.last().expect("nonempty");
        let focused = runtime
            .read(cx)
            .focus
            .iter()
            .find(|(_, f)| f.is_focused(window))
            .map(|(v, _)| v.clone());
        let focused_changed = runtime.read(cx).last_focused != focused;
        let overflow_focused = overflow_focus.is_focused(window);
        let rem_changed = runtime.read(cx).last_rem != window.rem_size();
        let overflow_changed = runtime.read(cx).overflow_focused != overflow_focused;
        runtime.update(cx, |rt, _| {
            rt.last_focused = focused.clone();
            rt.last_rem = window.rem_size();
            rt.overflow_focused = overflow_focused;
        });
        let target = focused.as_ref().unwrap_or(&current.value);
        let target_slot = if overflow_focused {
            visible.iter().position(Option::is_none)
        } else {
            visible
                .iter()
                .position(|ix| ix.map(|ix| &self.items[ix].value) == Some(target))
        };
        if (changed || focused_changed || rem_changed || overflow_changed)
            && let Some(ix) = target_slot
        {
            scroll.scroll_to_item(ix);
        }
        let max_width = self
            .max_width
            .unwrap_or_else(|| rems(theme, keys::BREADCRUMB_ITEM_MAX_WIDTH).into());
        let mut list = div()
            .id("breadcrumb-list")
            .test_support()
            .role(Role::List)
            .relative()
            .flex()
            .items_center()
            .min_w_0()
            .max_w_full()
            .overflow_x_scroll()
            .track_scroll(&scroll);
        let outside = rems(theme, keys::BREADCRUMB_FOCUS_RING_WIDTH)
            + rems(theme, keys::BREADCRUMB_FOCUS_RING_OFFSET);
        list = list.p(outside);
        let slots = visible.len();
        for (slot, ix) in visible.into_iter().enumerate() {
            let key = ix
                .map(|ix| ElementId::from((ElementId::from("item"), self.items[ix].value.clone())))
                .unwrap_or_else(|| ElementId::from("breadcrumb-overflow"));
            let mut cell = div()
                .id(key)
                .test_support()
                .role(Role::ListItem)
                .flex()
                .items_center()
                .min_w_0();
            if let Some(ix) = ix {
                let item = &self.items[ix];
                let is_current = ix + 1 == self.items.len();
                let value = item.value.clone();
                let handler: Option<ClickHandler> = (self.handler.is_some()
                    && (!is_current || self.current_clickable))
                    .then(|| {
                        let runtime = runtime.clone();
                        let value = value.clone();
                        Rc::new(
                            move |_: &gpui_kit::ClickEvent, w: &mut Window, cx: &mut App| {
                                dispatch(
                                    &runtime,
                                    generation,
                                    &value,
                                    BreadcrumbNavigateSource::Item,
                                    w,
                                    cx,
                                )
                            },
                        ) as ClickHandler
                    });
                let description = if is_current {
                    Some(match &item.description {
                        Some(d) => format!("{}; {}", self.current_description, d).into(),
                        None => self.current_description.clone(),
                    })
                } else {
                    item.description.clone()
                };
                cell = cell
                    .min_w(rems(theme, keys::BREADCRUMB_ITEM_MIN_WIDTH))
                    .when(is_current, |d| d.flex_shrink_0().max_w_full())
                    .child(TextAction {
                        theme,
                        label: item.label.clone(),
                        description,
                        icon: item.icon,
                        focus: runtime.read(cx).focus[&value].clone(),
                        disabled: item.disabled,
                        current: is_current,
                        expanded: None,
                        handler,
                        max_width,
                        overflow: None,
                    });
            } else {
                let mut items = Vec::new();
                for ix in hidden.clone() {
                    let item = &self.items[ix];
                    let mut row = if self.handler.is_some() {
                        YueMenuItem::new(item.label.clone())
                    } else {
                        YueMenuItem::info(item.label.clone())
                    };
                    row = row
                        .id(item.value.clone())
                        .disabled(item.disabled)
                        .description(item.description.clone())
                        .with_icon(item.icon);
                    if self.handler.is_some() {
                        let runtime = runtime.clone();
                        let value = item.value.clone();
                        row = row.on_click(move |_, w, cx| {
                            dispatch(
                                &runtime,
                                generation,
                                &value,
                                BreadcrumbNavigateSource::OverflowMenu,
                                w,
                                cx,
                            )
                        });
                    }
                    items.push(row);
                }
                let ellipsis = self.ellipsis.0.clone();
                let name = self.overflow_name.clone();
                let focus = overflow_focus.clone();
                cell = cell.flex_shrink_0().child(
                    YueDropdownMenu::with_action_trigger(
                        "breadcrumb-menu",
                        theme,
                        move |open, toggle, _, _| {
                            TextAction {
                                theme,
                                label: name,
                                description: None,
                                icon: None,
                                focus,
                                disabled: false,
                                current: false,
                                expanded: Some(open),
                                handler: Some(toggle),
                                max_width,
                                overflow: Some(ellipsis),
                            }
                            .into_any_element()
                        },
                    )
                    .with_state(&menu)
                    .label(self.overflow_name.clone())
                    .placement(DropdownPlacement::Below)
                    .align(DropdownAlign::Start)
                    .items(items),
                );
            }
            if slot + 1 < slots {
                cell = cell.child(
                    div()
                        .flex_shrink_0()
                        .mx(rems(theme, keys::BREADCRUMB_SEPARATOR_GAP))
                        .child(decoration_element(
                            self.separator.0.clone(),
                            theme,
                            rgba(theme, keys::BREADCRUMB_COLOR),
                        )),
                );
            }
            list = list.child(cell);
        }
        let geometry = runtime.read(cx).geometry.clone();
        let observed_scroll = scroll.clone();
        list = list.child(
            gpui_kit::canvas(
                move |bounds, window, _| {
                    if geometry.replace(bounds.size.width) != bounds.size.width
                        && let Some(ix) = target_slot
                    {
                        observed_scroll.scroll_to_item(ix);
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
        self.layout
            .apply(
                div()
                    .id(self.id)
                    .test_support()
                    .role(Role::Navigation)
                    .aria_label(self.name)
                    .flex()
                    .max_w_full()
                    .min_w_0()
                    .child(list),
            )
            .into_any_element()
    }
}

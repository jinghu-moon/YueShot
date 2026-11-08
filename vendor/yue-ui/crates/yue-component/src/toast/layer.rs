use super::{
    card::{self, Body, Card, length},
    state::Visual,
    *,
};
use crate::{MotionEase, YueTheme, theme::keys, tokens};
use gpui_kit::base::{
    A11yLive, ElementExt as _, Root, RootPlugin, Spring, ToastMotion, ToastStack,
    ToastTransitionStatus, Transition, spring_at, transition_at,
};
use gpui_kit::{
    Anchor, App, AvailableSpace, Bounds, Context, DispatchPhase, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _,
    Pixels, Render, Role, StatefulInteractiveElement as _, Styled as _, Window, canvas, div, point,
    prelude::FluentBuilder as _, px, size,
};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn init(cx: &mut App) {
    Root::register_plugin(cx, YueToasterState::new);
    cx.bind_keys([
        gpui_kit::KeyBinding::new("escape", CollapseToasts, Some("YueToast")),
        gpui_kit::KeyBinding::new("delete", DismissFocusedToast, Some("YueToast")),
        gpui_kit::KeyBinding::new("down", NextToast, Some("YueToast")),
        gpui_kit::KeyBinding::new("up", PreviousToast, Some("YueToast")),
    ]);
}
impl RootPlugin for YueToasterState {
    fn prepare(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.contains_focused(w, cx)
            && gpui_kit::base::active_modal_scope(w, cx).is_none()
            && let Some(focus) = w.focused(cx)
        {
            self.previous_focus = Some(focus.downgrade());
        }
        self.sync(w, cx);
        if self.waiting.iter().any(|r| r.measured.is_some()) {
            self.promote(w, cx);
        }
    }
    fn decorate(
        &self,
        surface: gpui_kit::AnyElement,
        _: &Root,
        _: &mut Window,
        _: &mut App,
    ) -> impl IntoElement {
        let owner = self.owner.clone();
        div()
            .id("yue-toast-root-actions")
            .size_full()
            .on_action(move |_: &FocusToasts, w, cx| {
                let handled = owner
                    .update(cx, |s, cx| s.focus_notifications(w, cx))
                    .unwrap_or(false);
                if !handled {
                    cx.propagate();
                }
            })
            .child(surface)
    }
}

#[derive(Clone, Copy)]
struct Geometry {
    safe: Bounds<Pixels>,
    width: Pixels,
    x: Pixels,
    anchor: Pixels,
    bottom: bool,
    side: bool,
}
fn geometry(options: &ToasterOptions, t: YueTheme, w: &Window, grow_up: bool) -> Geometry {
    let viewport = w.viewport_size();
    let margin = length(
        t,
        if viewport.width
            < length(t, keys::TOAST_WIDTH, w) + length(t, keys::TOAST_VIEWPORT_OFFSET, w) * 2.
        {
            keys::TOAST_VIEWPORT_OFFSET_COMPACT
        } else {
            keys::TOAST_VIEWPORT_OFFSET
        },
        w,
    );
    // A tiny window must still have a nonnegative, usable constraint.
    let x = (margin + px(options.insets.left)).min(viewport.width / 2.);
    let y = (margin + px(options.insets.top)).min(viewport.height / 2.);
    let width = (viewport.width - x - margin - px(options.insets.right)).max(px(1.));
    let height = (viewport.height - y - margin - px(options.insets.bottom)).max(px(1.));
    let safe = Bounds::new(point(x, y), size(width, height));
    let card_width = length(t, keys::TOAST_WIDTH, w).min(width);
    let x = match options.position {
        ToastPosition::Top | ToastPosition::Bottom => x + (width - card_width) / 2.,
        ToastPosition::TopRight | ToastPosition::Right | ToastPosition::BottomRight => {
            x + width - card_width
        }
        _ => x,
    };
    let side = options.position.side();
    let bottom = options.position.bottom() || (side && grow_up);
    let anchor = if side {
        safe.origin.y + height / 2.
    } else if bottom {
        safe.bottom()
    } else {
        safe.top()
    };
    Geometry {
        safe,
        width: card_width,
        x,
        anchor,
        bottom,
        side,
    }
}

pub(super) fn prefer_grow_up(
    options: &ToasterOptions,
    height: Pixels,
    w: &Window,
    cx: &App,
) -> bool {
    let theme = crate::current_theme(cx);
    let safe = geometry(options, theme, w, false).safe;
    height / 2. + length(theme, keys::TOAST_PEEK, w) * options.visible.saturating_sub(1) as f32
        > safe.size.height / 2.
}

fn motion(t: YueTheme, w: &Window, visible: usize) -> ToastMotion {
    ToastMotion {
        duration: tokens::duration(t, keys::TOAST_LAYOUT_RESPONSE),
        exit_duration: tokens::duration(t, keys::TOAST_DURATION_EXIT),
        collapsed_peek: length(t, keys::TOAST_PEEK, w),
        expanded_gap: length(t, keys::TOAST_GAP, w),
        collapsed_scale_step: tokens::number(t, keys::TOAST_COLLAPSED_SCALE_STEP),
        collapsed_visible: visible,
    }
}

impl Render for YueToasterState {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = crate::current_theme(cx);
        let now = self.now(cx);
        let reduced = cx.reduce_motion();
        let running = self.animating();
        let mut root = div().absolute().inset_0().overflow_hidden();
        if self.hidden() {
            for (_, record, _) in self.manager.iter() {
                record.visual.borrow_mut().unmount();
            }
            return root;
        }
        let g = geometry(&self.options, t, w, self.grow_up);
        let signature = (
            g.width,
            f32::from(w.rem_size()),
            t.mode(),
            t.accent(),
            t.neutral(),
        );
        let remeasure = self.signature != Some(signature);
        self.signature = Some(signature);
        let mut pending = vec![];
        for r in self.waiting.iter().chain(
            self.manager
                .iter()
                .filter(|(_, _, s)| *s != ToastTransitionStatus::Ending)
                .map(|(_, r, _)| r),
        ) {
            if remeasure || r.measured.is_none() || r.display_revision != r.revision {
                let element = card::body(
                    Body {
                        spec: &r.spec,
                        record: r,
                        theme: t,
                        width: g.width,
                        ticket: None,
                        controls: false,
                        keyboard: false,
                        close_label: self.options.close_label.clone(),
                        now,
                        reduced,
                    },
                    w,
                );
                pending.push((
                    r.generation,
                    r.revision,
                    div().w(g.width).child(element).into_any_element(),
                ));
            }
        }
        if !pending.is_empty() {
            let weak = cx.weak_entity();
            let width = g.width;
            let rem = f32::from(w.rem_size());
            root = root.child(
                canvas(
                    move |_, w, cx| {
                        let mut measured = Vec::with_capacity(pending.len());
                        for (generation, revision, mut element) in pending {
                            let natural = element.layout_as_root(
                                size(AvailableSpace::Definite(width), AvailableSpace::MinContent),
                                w,
                                cx,
                            );
                            if natural.height > px(0.) {
                                measured.push((generation, revision, natural.height));
                            }
                        }
                        let _ = weak.update(cx, |s, cx| {
                            let changed_at = s.now(cx);
                            for (generation, revision, height) in measured {
                                if let Some(r) =
                                    s.record_mut(generation).filter(|r| r.revision == revision)
                                {
                                    if r.display_revision != revision {
                                        let (old, alpha) = card::content_sample(
                                            r,
                                            changed_at,
                                            tokens::duration(t, keys::TOAST_DURATION_CONTENT),
                                            reduced,
                                        );
                                        r.previous = Some(old.clone());
                                        r.previous_alpha = alpha;
                                        r.display = r.spec.clone();
                                        r.display_revision = revision;
                                        r.changed_at = changed_at;
                                    }
                                    r.measured = Some((height, width, rem));
                                    s.stack.set_measurement(
                                        ("yue-toast-item", generation),
                                        height,
                                        width,
                                    );
                                }
                            }
                            s.promote(w, cx);
                            cx.notify();
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_0(),
            );
        }

        let mut active = self
            .manager
            .iter()
            .filter(|(_, _, s)| *s != ToastTransitionStatus::Ending)
            .collect::<Vec<_>>();
        let skip = active.len().saturating_sub(self.options.visible);
        for (_, record, _) in active.drain(..skip) {
            // These lifecycle records keep running, but have no mounted card.
            // A later timeout must not resurrect their last visible geometry.
            record.visual.borrow_mut().unmount();
        }
        let front_height = active
            .last()
            .and_then(|(_, r, _)| r.measured)
            .map_or(px(0.), |m| m.0);
        let available = if g.side {
            (g.safe.size.height / 2. + front_height / 2.).min(g.safe.size.height)
        } else {
            g.safe.size.height
        };
        let natural_extent = active
            .iter()
            .filter_map(|(_, r, _)| r.measured.map(|m| m.0))
            .fold(px(0.), |a, h| a + h)
            + length(t, keys::TOAST_GAP, w) * active.len().saturating_sub(1) as f32;
        let scroll = (self.expanded && natural_extent > available) || front_height > available;
        let scroll_key = active.last().map(|(id, _, _)| (**id, self.expanded));
        let initial_scroll = scroll && self.scroll_session != scroll_key;
        self.scroll_session = if scroll { scroll_key } else { None };
        if initial_scroll {
            if g.bottom {
                self.scroll.scroll_to_bottom();
            } else {
                self.scroll.set_offset(point(px(0.), px(0.)));
            }
        }
        let side_center = if scroll && g.side {
            spring_at(
                ("yue-toast-region", "scroll-center"),
                front_height / 2.,
                Spring::new(tokens::duration(t, keys::TOAST_LAYOUT_RESPONSE)).with_epsilon(0.1),
                now,
                running,
                w,
                cx,
            )
        } else {
            px(0.)
        };
        let anchor = if scroll && g.side {
            if g.bottom {
                (g.anchor + side_center).min(g.safe.bottom())
            } else {
                (g.anchor - side_center).max(g.safe.top())
            }
        } else {
            g.anchor
        };
        let available = if scroll && g.side {
            if g.bottom {
                anchor - g.safe.top()
            } else {
                g.safe.bottom() - anchor
            }
        } else {
            available
        };
        let focus_bounds = active.iter().find_map(|(_, r, _)| {
            if r.action_focus.is_focused(w) {
                Some(r.action_bounds.clone())
            } else if r.close_focus.is_focused(w) {
                Some(r.close_bounds.clone())
            } else {
                None
            }
        });
        let focused_card = active
            .iter()
            .find(|(_, r, _)| r.focus.is_focused(w))
            .map(|(_, r, _)| r.visual.clone());
        let initial_card = if initial_scroll && front_height > available {
            active.last().map(|(_, r, _)| r.visual.clone())
        } else {
            None
        };
        let scroll_handle = self.scroll.clone();
        let stack_motion = motion(t, w, self.options.visible);
        let mut stack = ToastStack::new("yue-toast-stack", self.stack.clone())
            .motion(stack_motion)
            .placement(if g.bottom {
                Anchor::BottomRight
            } else {
                Anchor::TopRight
            })
            .front_shell(true)
            .center_front(g.side && !scroll)
            .measure_items(false)
            .expanded(self.expanded)
            .sample_time(now, running)
            .w_full();
        let policy =
            Spring::new(tokens::duration(t, keys::TOAST_LAYOUT_RESPONSE)).with_epsilon(0.1);
        let region_bounds = Rc::new(RefCell::new(None::<Bounds<Pixels>>));
        let mut active_visuals = Vec::new();
        for (index, (generation, r, phase)) in active.iter().enumerate() {
            let foreground = index + 1 == active.len();
            let Some((natural, _, _)) = r.measured else {
                continue;
            };
            self.stack
                .set_measurement(("yue-toast-item", **generation), natural, g.width);
            let shell = spring_at(
                (
                    gpui_kit::ElementId::NamedInteger("yue-toast-height".into(), **generation),
                    "shell",
                ),
                if foreground || self.expanded {
                    natural
                } else {
                    front_height
                },
                policy,
                now,
                running,
                w,
                cx,
            );
            let content = transition_at(
                (
                    gpui_kit::ElementId::NamedInteger("yue-toast-body".into(), **generation),
                    "fade",
                ),
                if foreground || self.expanded {
                    1_f32
                } else {
                    0.
                },
                Transition::new(tokens::duration(t, keys::TOAST_DURATION_CONTENT))
                    .ease(MotionEase::Fade.easing(t)),
                now,
                running,
                w,
                cx,
            )
            .value;
            let elapsed = self
                .manager
                .transition_elapsed_at(generation, now)
                .unwrap_or_default();
            let enter = tokens::duration(t, keys::TOAST_DURATION_ENTER);
            let p = if reduced || *phase == ToastTransitionStatus::Present {
                1.
            } else {
                (elapsed.as_secs_f32() / enter.as_secs_f32()).clamp(0., 1.)
            };
            let ease = MotionEase::Enter.easing(t)(p);
            let mut opacity = if reduced || *phase == ToastTransitionStatus::Present {
                1.
            } else {
                (elapsed.as_secs_f32()
                    / tokens::duration(t, keys::TOAST_DURATION_CONTENT).as_secs_f32())
                .clamp(0., 1.)
            };
            let offset = entry_offset(self.options.position, g, natural, w, t) * (1. - ease);
            let (mut dx, mut dy) = if g.side {
                (offset, px(0.))
            } else {
                (px(0.), offset)
            };
            if !reduced
                && *phase == ToastTransitionStatus::Starting
                && let Some(entry) = r.entry
                && let Some(from) = entry.bounds
            {
                let target_y = if g.side && !scroll {
                    g.anchor - natural / 2.
                } else if g.bottom {
                    anchor - natural
                } else {
                    anchor
                };
                dx = (from.origin.x - g.x) * (1. - ease);
                dy = (from.origin.y - target_y) * (1. - ease);
                opacity = entry.opacity + (1. - entry.opacity) * opacity;
            }
            let controls = (foreground || self.expanded) && p > 0.;
            let reading = self
                .manager
                .reading_time_at(generation, now, self.is_paused());
            let progress = reading.map(|(remaining, total)| {
                remaining.as_secs_f32() / total.as_secs_f32().max(f32::EPSILON)
            });
            let element = card::render(
                Card {
                    record: r,
                    spec: &r.display,
                    theme: t,
                    width: g.width,
                    height: shell,
                    ticket: self.ticket(r, cx),
                    phase: *phase,
                    content,
                    controls,
                    keyboard: self.focus.contains_focused(w, cx),
                    progress,
                    close_label: self.options.close_label.clone(),
                    now,
                    reduced,
                    running,
                },
                w,
                cx,
            );
            let painted = element;
            *r.painted.borrow_mut() = Some(painted.body);
            let element = painted.element;
            let visual = r.visual.clone();
            let observe = visual.clone();
            let bounds = visual.borrow().bounds;
            *visual.borrow_mut() = Visual {
                bounds,
                opacity,
                content: content * painted.body_alpha,
                foreground,
                colors: Some(painted.colors),
                sampled_at: Some(now),
            };
            let region = region_bounds.clone();
            let item = div()
                .id(("yue-toast-tab-group", **generation))
                .tab_group()
                .tab_index(if g.bottom {
                    index
                } else {
                    active.len() - 1 - index
                } as isize)
                .tab_stop(false)
                .relative()
                .left(dx)
                .top(dy)
                .opacity(opacity)
                .on_prepaint(move |bounds, _, _| {
                    observe.borrow_mut().bounds = Some(bounds);
                    let mut region = region.borrow_mut();
                    *region = Some(region.map_or(bounds, |old| crate::popup::union(old, bounds)));
                })
                .child(element);
            stack = stack.item(("yue-toast-item", **generation), item);
            active_visuals.push(visual);
            let content_changing = now.saturating_duration_since(r.changed_at)
                < tokens::duration(t, keys::TOAST_DURATION_CONTENT)
                && r.previous.is_some();
            let animated_progress = r.display.progress && reading.is_some() && !self.is_paused();
            let animated_spinner =
                r.display.kind == ToastKind::Loading && (foreground || self.expanded);
            if !reduced
                && running
                && (*phase == ToastTransitionStatus::Starting
                    || content_changing
                    || animated_progress
                    || animated_spinner)
            {
                w.request_animation_frame();
            }
        }
        let mut region_element = None;
        if !active.is_empty() {
            let weak = cx.weak_entity();
            let region = div()
                .id("yue-toast-region")
                .role(Role::Region)
                .aria_label(self.options.region_label.clone())
                .track_focus(&self.focus)
                .tab_group()
                .tab_stop(false)
                .key_context("YueToast")
                .on_action(cx.listener(|s, _: &CollapseToasts, w, cx| {
                    if !s.focus.contains_focused(w, cx) || s.hidden() {
                        cx.propagate();
                        return;
                    }
                    s.collapse_notifications(w, cx);
                }))
                .on_action(cx.listener(|s, _: &DismissFocusedToast, w, cx| {
                    let target = s
                        .manager
                        .iter()
                        .rev()
                        .find(|(_, r, phase)| {
                            *phase != ToastTransitionStatus::Ending
                                && r.spec.can_close()
                                && r.focus.contains_focused(w, cx)
                        })
                        .map(|(g, _, _)| *g);
                    if let Some(g) = target {
                        s.close_generation(g, ToastDismissReason::Keyboard, w, cx);
                    } else {
                        cx.propagate();
                    }
                }))
                .on_action(cx.listener(|s, _: &NextToast, w, cx| {
                    if !s.navigate(1, w, cx) {
                        cx.propagate();
                    }
                }))
                .on_action(cx.listener(|s, _: &PreviousToast, w, cx| {
                    if !s.navigate(-1, w, cx) {
                        cx.propagate();
                    }
                }))
                .absolute()
                .left(g.x)
                .w(g.width)
                .when(g.bottom, |d| d.bottom(w.viewport_size().height - anchor))
                .when(!g.bottom, |d| d.top(anchor))
                .when(scroll, |d| {
                    d.max_h(available)
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                })
                .child(stack)
                .child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, w, _| {
                            if scroll {
                                let target = focus_bounds
                                    .as_ref()
                                    .and_then(|b| b.get())
                                    .or_else(|| {
                                        focused_card.as_ref().and_then(|v| v.borrow().bounds)
                                    })
                                    .or_else(|| {
                                        initial_card.as_ref().and_then(|v| v.borrow().bounds)
                                    });
                                if let Some(target) = target {
                                    let viewport = scroll_handle.bounds();
                                    let dy = if target.top() < viewport.top()
                                        || target.size.height > viewport.size.height
                                    {
                                        viewport.top() - target.top()
                                    } else if target.bottom() > viewport.bottom() {
                                        viewport.bottom() - target.bottom()
                                    } else {
                                        px(0.)
                                    };
                                    if dy.abs() > px(0.1) {
                                        let offset = scroll_handle.offset();
                                        scroll_handle.set_offset(point(offset.x, offset.y + dy));
                                        w.request_animation_frame();
                                    }
                                }
                            }
                            let move_weak = weak.clone();
                            let move_bounds = region_bounds.clone();
                            w.on_mouse_event(move |event: &MouseMoveEvent, phase, w, cx| {
                                if phase != DispatchPhase::Capture {
                                    return;
                                }
                                let hit = move_bounds.borrow().is_some_and(|b| {
                                    b.intersect(&g.safe).contains(&event.position)
                                });
                                let _ = move_weak.update(cx, |s, cx| s.hover(hit, w, cx));
                            });
                            let down_weak = weak.clone();
                            w.on_mouse_event(move |event: &MouseDownEvent, phase, w, cx| {
                                if phase != DispatchPhase::Capture
                                    || event.button != MouseButton::Left
                                {
                                    return;
                                }
                                let hit = active_visuals.iter().any(|v| {
                                    v.borrow().bounds.is_some_and(|b| {
                                        b.intersect(&g.safe).contains(&event.position)
                                    })
                                });
                                if hit {
                                    let _ = down_weak.update(cx, |s, cx| s.press(true, w, cx));
                                }
                            });
                            w.on_mouse_event(move |event: &MouseUpEvent, phase, w, cx| {
                                if phase == DispatchPhase::Capture
                                    && event.button == MouseButton::Left
                                {
                                    let _ = weak.update(cx, |s, cx| s.press(false, w, cx));
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                );
            region_element = Some(region.into_any_element());
        }
        let mut back_exits = vec![];
        let mut front_exits = vec![];
        // Endings are independent snapshots: excluded from active count, offsets and measurements.
        for (generation, r, phase) in self
            .manager
            .iter()
            .filter(|(_, _, s)| *s == ToastTransitionStatus::Ending)
        {
            if r.superseded {
                continue;
            }
            let Some(exit) = r.exit else {
                continue;
            };
            let Some(bounds) = exit.bounds else {
                continue;
            };
            let elapsed = self
                .manager
                .transition_elapsed_at(generation, now)
                .unwrap_or_default();
            let p = if reduced {
                1.
            } else {
                (elapsed.as_secs_f32()
                    / tokens::duration(t, keys::TOAST_DURATION_EXIT).as_secs_f32())
                .clamp(0., 1.)
            };
            let p = MotionEase::Exit.easing(t)(p);
            let distance = length(
                t,
                if exit.foreground {
                    keys::SPACE_24
                } else {
                    keys::SPACE_8
                },
                w,
            ) * p;
            let (dx, dy) = match self.options.position {
                ToastPosition::Left => (-distance, px(0.)),
                ToastPosition::Right => (distance, px(0.)),
                p if p.bottom() => (px(0.), distance),
                _ => (px(0.), -distance),
            };
            let card = card::render(
                Card {
                    record: r,
                    spec: r.exit_spec.as_ref().unwrap_or(&r.display),
                    theme: t,
                    width: bounds.size.width,
                    height: bounds.size.height,
                    ticket: self.ticket(r, cx),
                    phase,
                    content: exit.content,
                    controls: false,
                    keyboard: false,
                    progress: None,
                    close_label: self.options.close_label.clone(),
                    now,
                    reduced,
                    running,
                },
                w,
                cx,
            );
            let ending = div()
                .absolute()
                .left(bounds.origin.x + dx)
                .top(bounds.origin.y + dy)
                .w(bounds.size.width)
                .h(bounds.size.height)
                .opacity(exit.opacity * (1. - p))
                .child(card.element)
                .into_any_element();
            if exit.foreground {
                front_exits.push(ending);
            } else {
                back_exits.push(ending);
            }
            if running && !reduced && p < 1. {
                w.request_animation_frame();
            }
        }
        root = root
            .children(back_exits)
            .children(region_element)
            .children(front_exits);
        for (_, r, phase) in self.manager.iter() {
            if phase == ToastTransitionStatus::Ending || !live_visible(r) {
                continue;
            }
            root = root.child(live_node(r));
        }
        root
    }
}

fn entry_offset(
    position: ToastPosition,
    g: Geometry,
    height: Pixels,
    w: &Window,
    t: YueTheme,
) -> Pixels {
    let shadows = tokens::shadow(t, keys::TOAST_SHADOW);
    // Conservative extent from the same parsed shadow, including its offset/spread.
    let q = shadows
        .iter()
        .map(|s| s.blur_radius * 1.5 + s.spread_radius + s.offset.x.abs().max(s.offset.y.abs()))
        .fold(px(0.), Pixels::max);
    let inset = if g.side {
        if position == ToastPosition::Left {
            g.safe.left()
        } else {
            w.viewport_size().width - g.safe.right()
        }
    } else if position.bottom() {
        w.viewport_size().height - g.safe.bottom()
    } else {
        g.safe.top()
    };
    let short = length(t, keys::SPACE_24, w);
    let extra = if inset > length(t, keys::TOAST_VIEWPORT_OFFSET, w) + px(0.1) {
        short
    } else if g.side {
        g.width + inset + q
    } else {
        height + inset + q
    };
    if matches!(
        position,
        ToastPosition::Top | ToastPosition::TopLeft | ToastPosition::TopRight | ToastPosition::Left
    ) {
        -extra
    } else {
        extra
    }
}

/// Delivered events stay mounted with stable native identity. Modal restoration never reinserts them.
pub(super) fn live_visible(r: &super::state::Record) -> bool {
    r.spec.announcement != ToastAnnouncement::Silent
        && !(r.delivered.get() && r.announcement_suppressed)
        && (r.display_revision == r.revision || r.delivered.get())
}
pub(super) fn live_node(r: &super::state::Record) -> A11yLive<gpui_kit::Stateful<gpui_kit::Div>> {
    let delivered = r.delivered.clone();
    let spoken = r.spec.spoken();
    let live = if r.spec.announcement == ToastAnnouncement::Assertive {
        gpui_kit::accesskit::Live::Assertive
    } else {
        gpui_kit::accesskit::Live::Polite
    };
    A11yLive::new(
        div()
            .id(gpui_kit::ElementId::NamedInteger(
                format!("yue-toast-live-{}-{}", r.generation, r.semantic_revision).into(),
                0,
            ))
            .role(if live == gpui_kit::accesskit::Live::Assertive {
                Role::Alert
            } else {
                Role::Status
            })
            .aria_label(spoken.clone())
            .aria_value(spoken)
            .absolute()
            .size_0()
            .overflow_hidden(),
        live,
    )
    .atomic(true)
    .on_written(move || delivered.set(true))
}

impl YueToasterState {
    fn navigate(&mut self, direction: isize, w: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.hidden() || !self.focus.contains_focused(w, cx) {
            return false;
        }
        let mut items = self
            .manager
            .iter()
            .rev()
            .filter(|(_, _, s)| *s != ToastTransitionStatus::Ending)
            .take(self.options.visible)
            .map(|(_, r, _)| {
                (
                    r.visual.borrow().bounds.map_or(px(0.), |b| b.center().y),
                    r.focus.clone(),
                    r.focus.contains_focused(w, cx),
                )
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let index = items.iter().position(|(_, _, focused)| *focused).map_or(
            if direction > 0 {
                0
            } else {
                items.len() as isize - 1
            },
            |ix| ix as isize + direction,
        );
        if index < 0 {
            return false;
        }
        if let Some((_, focus, _)) = items.get(index as usize) {
            focus.focus(w, cx);
            cx.notify();
            true
        } else {
            false
        }
    }
}

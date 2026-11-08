use super::*;
use crate::{Accent, ThemeMode, YueDropdownMenu, YueInput, YueMenuItem, YueTooltip};
use crate::{
    YueButton,
    theme::keys,
    tokens::{duration, rgba, shadow},
};
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::base::Root;
use gpui_kit::{
    AppContext as _, Context, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Render,
    Subscription, TestAppContext, VisualTestContext, point, size,
};
use gpui_kit::{div, px};
use std::time::Duration;

fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}
struct Demo {
    dialog: Entity<YueDialogState>,
    child: Entity<YueDialogState>,
    input: Entity<gpui_kit::base::input::InputState>,
    outside: FocusHandle,
    inner: FocusHandle,
    events: Vec<YueDialogEvent>,
    _events: Subscription,
    mounted: bool,
    alert: bool,
    escape: bool,
    overlay: bool,
    veto: bool,
    disabled: bool,
    pending: bool,
    ticket: Option<DialogTicket>,
    confirms: usize,
    guards: usize,
    background: usize,
    long: bool,
    textarea: bool,
    note: Entity<gpui_kit::base::input::TextareaState>,
    no_controls: bool,
    draggable: bool,
    explicit_focus: bool,
}
impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let guard = weak.clone();
        let state = self.dialog.clone();
        let mut root = div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                YueButton::text("open", theme(), "Open")
                    .track_focus(&self.outside)
                    .on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| s.open(w, cx)).unwrap();
                    }),
            )
            .child(
                YueButton::text("background", theme(), "Background")
                    .on_click(cx.listener(|s, _, _, _| s.background += 1)),
            );
        for ix in 0usize..110 {
            root = root.child(YueButton::text(
                ("background", ix),
                theme(),
                format!("Background {ix}"),
            ));
        }
        if !self.mounted {
            return root;
        }
        let mut dialog = if self.alert {
            YueDialog::alert("dialog", theme(), &self.dialog)
        } else {
            YueDialog::new("dialog", theme(), &self.dialog)
        }
        .title("Edit project")
        .description("Save the project settings.")
        .close_on_escape(self.escape)
        .close_on_overlay_click(self.overlay)
        .confirm_disabled(self.disabled)
        .draggable(self.draggable)
        .on_confirm(move |request, _, cx| {
            weak.update(cx, |s, _| {
                s.confirms += 1;
                s.ticket = Some(request.ticket());
                if s.pending {
                    DialogDecision::Pending
                } else {
                    DialogDecision::Close
                }
            })
            .unwrap_or(DialogDecision::KeepOpen)
        })
        .on_close_request(move |_, _, cx| {
            guard
                .update(cx, |s, _| {
                    s.guards += 1;
                    !s.veto
                })
                .unwrap_or(false)
        });
        if self.explicit_focus {
            dialog = dialog.initial_focus(&self.inner);
        }
        if self.no_controls {
            dialog = dialog.closable(false).footer(DialogFooter::None);
        } else {
            let child = self.child.clone();
            let parent = self.dialog.clone();
            dialog = dialog
                .child(YueButton::text("inner", theme(), "Inner").track_focus(&self.inner))
                .child(YueInput::new("name", theme(), &self.input).accessibility_label("Name"))
                .child(
                    YueDropdownMenu::new(
                        "choices",
                        theme(),
                        YueButton::text("choices-trigger", theme(), "Choices"),
                    )
                    .label("Project choices")
                    .items([YueMenuItem::new("First choice")]),
                )
                .child(
                    YueTooltip::new("static-help", theme(), "Modal explanation")
                        .show_delay(Duration::ZERO)
                        .trigger(
                            div()
                                .id("static-target")
                                .test_support()
                                .w_full()
                                .h_8()
                                .child("Help"),
                        ),
                )
                .child(YueButton::text("child-open", theme(), "Child").on_click(
                    move |_, w, cx| {
                        child
                            .update(cx, |s, cx| s.open_child(&parent, w, cx))
                            .unwrap();
                    },
                ));
        }
        if self.long {
            dialog = dialog.children((0..100).map(|n| {
                div()
                    .id(format!("paragraph-{n}"))
                    .test_support()
                    .child(format!("Paragraph {n}"))
            }));
        }
        if self.textarea {
            dialog = dialog.child(
                crate::YueTextarea::new("note", theme(), &self.note).accessibility_label("Note"),
            );
        }
        root.child(dialog).child(
            YueDialog::alert("child", theme(), &self.child)
                .title("Delete project?")
                .status(DialogStatus::Danger)
                .confirm_label("Delete"),
        )
    }
}
fn setup(cx: &mut TestAppContext) -> (Entity<Demo>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::popup::init(cx);
        crate::tooltip::init(cx);
        super::init(cx);
        crate::set_theme(cx, theme());
        cx.set_reduce_motion(true);
    });
    let mut saved = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let view = cx.new(|cx| {
            let dialog = cx.new(YueDialogState::new);
            let events = cx.subscribe(&dialog, |s: &mut Demo, _, e, _| s.events.push(e.clone()));
            Demo {
                dialog,
                child: cx.new(YueDialogState::new),
                input: cx.new(|cx| gpui_kit::base::input::InputState::new(w, cx)),
                outside: cx.focus_handle(),
                inner: cx.focus_handle(),
                events: vec![],
                _events: events,
                mounted: true,
                alert: false,
                escape: true,
                overlay: true,
                veto: false,
                disabled: false,
                pending: false,
                ticket: None,
                confirms: 0,
                guards: 0,
                background: 0,
                long: false,
                textarea: false,
                note: cx.new(|cx| gpui_kit::base::input::TextareaState::new(w, cx)),
                no_controls: false,
                draggable: false,
                explicit_focus: false,
            }
        });
        saved = Some(view.clone());
        Root::new(view, w, cx)
    });
    cx.update(|w, _| w.activate_window());
    draw(cx);
    (saved.unwrap(), cx)
}
fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}
fn tick(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    draw(cx);
}
fn node(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.label() == Some(label))
            .unwrap_or_else(|| panic!("missing {label}"))
    })
}
fn shown(cx: &mut VisualTestContext, label: &str) -> bool {
    cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some(label)))
}
fn click(cx: &mut VisualTestContext, label: &str) {
    let p = node(cx, label).bounds().center();
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
}
fn edit(view: &Entity<Demo>, cx: &mut VisualTestContext, f: impl FnOnce(&mut Demo)) {
    view.update(cx, |s, cx| {
        f(s);
        cx.notify();
    });
    draw(cx);
}
fn state(view: &Entity<Demo>, cx: &mut VisualTestContext) -> Entity<YueDialogState> {
    view.read_with(cx, |s, _| s.dialog.clone())
}
fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = gpui_kit::Keystroke::parse(key).expect("fixture key");
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
    draw(cx);
}

#[gpui_kit::test]
fn declaration_is_absent_until_open_and_restores_focus_once(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    assert!(!shown(cx, "Edit project"));
    click(cx, "Open");
    assert!(shown(cx, "Edit project"));
    let dialog = state(&v, cx);
    assert!(dialog.read_with(cx, |s, _| s.is_open()));
    press(cx, "escape");
    draw(cx);
    assert!(!shown(cx, "Edit project"));
    assert!(cx.update(|w, cx| v.read(cx).outside.is_focused(w)));
    assert_eq!(
        v.read_with(cx, |s, _| s.events.clone()),
        vec![
            YueDialogEvent::OpenChanged(DialogChange {
                open: true,
                reason: None
            }),
            YueDialogEvent::Opened,
            YueDialogEvent::OpenChanged(DialogChange {
                open: false,
                reason: Some(DialogCloseReason::Escape)
            }),
            YueDialogEvent::Closed(DialogChange {
                open: false,
                reason: Some(DialogCloseReason::Escape)
            }),
        ]
    );
}
#[gpui_kit::test]
fn guard_callbacks_can_update_host_and_all_close_routes_share_it(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| s.veto = true);
    click(cx, "Open");
    click(cx, "Cancel");
    press(cx, "escape");
    click(cx, "Close");
    click(cx, "OK");
    assert!(shown(cx, "Edit project"));
    assert_eq!(v.read_with(cx, |s, _| s.guards), 4);
    edit(&v, cx, |s| s.veto = false);
    click(cx, "Cancel");
    assert!(!shown(cx, "Edit project"));
}
#[gpui_kit::test]
fn enter_policy_is_independent_of_escape_and_alert_requires_explicit_action(
    cx: &mut TestAppContext,
) {
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| s.escape = false);
    click(cx, "Open");
    press(cx, "escape");
    assert!(shown(cx, "Edit project"));
    press(cx, "enter");
    assert!(!shown(cx, "Edit project"));
    edit(&v, cx, |s| {
        s.alert = true;
        s.escape = true;
    });
    click(cx, "Open");
    let d = state(&v, cx);
    cx.update(|w, cx| {
        let focus = d.read(cx).focus.clone();
        focus.focus(w, cx);
    });
    press(cx, "enter");
    assert!(shown(cx, "Edit project"));
    click(cx, "OK");
    assert!(!shown(cx, "Edit project"));
}
#[gpui_kit::test]
fn pending_rejects_every_dismissal_and_ticket_consumes_once(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| s.pending = true);
    click(cx, "Open");
    click(cx, "OK");
    let d = state(&v, cx);
    assert!(d.read_with(cx, |s, _| s.is_pending()));
    click(cx, "Cancel");
    click(cx, "Close");
    press(cx, "escape");
    press(cx, "enter");
    cx.update(|w, cx| {
        assert!(!d.update(cx, |s, cx| s.close(w, cx)));
    });
    assert_eq!(v.read_with(cx, |s, _| s.confirms), 1);
    let ticket = v.read_with(cx, |s, _| s.ticket.clone().unwrap());
    cx.update(|w, cx| {
        assert!(ticket.complete(DialogOutcome::KeepOpen, w, cx));
        assert!(!ticket.complete(DialogOutcome::Close, w, cx));
    });
    draw(cx);
    assert!(shown(cx, "Edit project"));
    click(cx, "OK");
    let next = v.read_with(cx, |s, _| s.ticket.clone().unwrap());
    cx.update(|w, cx| {
        assert!(next.complete(DialogOutcome::Close, w, cx));
    });
    draw(cx);
    assert!(!shown(cx, "Edit project"));
    click(cx, "Open");
    cx.update(|w, cx| assert!(!next.complete(DialogOutcome::Close, w, cx)));
    assert!(shown(cx, "Edit project"));
}
#[gpui_kit::test]
fn disabled_confirm_and_repeated_submission_do_not_invoke_host(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| s.disabled = true);
    click(cx, "Open");
    click(cx, "OK");
    press(cx, "enter");
    assert_eq!(v.read_with(cx, |s, _| s.confirms), 0);
    click(cx, "Cancel");
    assert!(!shown(cx, "Edit project"));
}
#[gpui_kit::test]
fn exit_retains_barrier_and_reverse_does_not_emit_stale_closed(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    click(cx, "Open");
    tick(cx, 250);
    let start = node(cx, "Edit project").bounds();
    click(cx, "Cancel");
    tick(cx, 60);
    assert!(shown(cx, "Edit project"));
    let moving = node(cx, "Edit project").bounds();
    assert!(moving.top() > start.top());
    let d = state(&v, cx);
    cx.update(|w, cx| d.update(cx, |s, cx| s.open(w, cx)).unwrap());
    draw(cx);
    let reversed = node(cx, "Edit project").bounds();
    assert!((moving.top() - reversed.top()).abs() < px(0.01));
    tick(cx, 250);
    assert!(shown(cx, "Edit project"));
    assert_eq!(
        v.read_with(cx, |s, _| s
            .events
            .iter()
            .filter(|e| matches!(e, YueDialogEvent::Closed(_)))
            .count()),
        0
    );
    click(cx, "Cancel");
    tick(cx, 150);
    draw(cx);
    assert!(!shown(cx, "Edit project"));
}
#[gpui_kit::test]
fn explicit_child_is_topmost_and_unrelated_open_is_rejected(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    click(cx, "Open");
    let child = v.read_with(cx, |s, _| s.child.clone());
    cx.update(|w, cx| {
        assert_eq!(
            child.update(cx, |s, cx| s.open(w, cx)),
            Err(DialogOpenError::Occupied)
        )
    });
    click(cx, "Child");
    assert!(shown(cx, "Delete project?"));
    press(cx, "escape");
    assert!(!shown(cx, "Delete project?"));
    assert!(shown(cx, "Edit project"));
    draw(cx); // Retiring the child schedules the resumed parent's frame.
    press(cx, "escape");
    assert!(!shown(cx, "Edit project"));
}
#[gpui_kit::test]
fn popup_is_above_modal_and_escape_closes_one_surface(cx: &mut TestAppContext) {
    let (_, cx) = setup(cx);
    click(cx, "Open");
    click(cx, "Choices");
    assert!(shown(cx, "First choice"));
    press(cx, "escape");
    assert!(!shown(cx, "First choice"));
    assert!(shown(cx, "Edit project"));
    press(cx, "escape");
    assert!(!shown(cx, "Edit project"));
}

#[gpui_kit::test]
fn popup_outside_press_does_not_become_a_backdrop_dismissal(cx: &mut TestAppContext) {
    let (_, cx) = setup(cx);
    click(cx, "Open");
    click(cx, "Choices");
    cx.simulate_click(point(px(790.), px(590.)), Modifiers::default());
    draw(cx);
    assert!(!shown(cx, "First choice"));
    assert!(shown(cx, "Edit project"));
    cx.simulate_click(point(px(790.), px(590.)), Modifiers::default());
    draw(cx);
    assert!(!shown(cx, "Edit project"));
}

#[gpui_kit::test]
fn nested_dialog_uses_one_continuous_scrim_during_enter_and_exit(cx: &mut TestAppContext) {
    let (_, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    click(cx, "Open");
    tick(cx, 250);
    click(cx, "Child");
    let scrims = |cx: &mut VisualTestContext| {
        cx.update(|w, _| {
            let viewport = gpui_kit::Bounds::new(Default::default(), w.viewport_size())
                .scale(w.scale_factor());
            w.painted_quads()
                .iter()
                .filter(|q| {
                    q.bounds == viewport
                        && q.background
                            .as_solid()
                            .is_some_and(|c| c.l == 0. && c.a > 0.4)
                })
                .count()
        })
    };
    assert_eq!(scrims(cx), 1);
    tick(cx, 125);
    assert_eq!(scrims(cx), 1);
    tick(cx, 125);
    press(cx, "escape");
    tick(cx, 75);
    assert_eq!(scrims(cx), 1);
    tick(cx, 75);
    draw(cx);
    assert_eq!(scrims(cx), 1);
    assert!(shown(cx, "Edit project"));
}
#[gpui_kit::test]
fn modal_accepts_a_static_tooltip_without_a_focus_handle(cx: &mut TestAppContext) {
    let (_, cx) = setup(cx);
    click(cx, "Open");
    let target = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&"static-target".into()))
            .unwrap()
            .bounds()
    });
    cx.simulate_mouse_move(target.center(), None, Modifiers::default());
    draw(cx);
    tick(cx, 1);
    assert!(shown(cx, "Modal explanation"));
    press(cx, "escape");
    assert!(!shown(cx, "Modal explanation"));
    assert!(shown(cx, "Edit project"));
}
#[gpui_kit::test]
fn complete_backdrop_click_is_required_and_never_activates_background(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    click(cx, "Open");
    let inside = node(cx, "Edit project").bounds().center();
    let outside = point(px(790.), px(590.));
    cx.simulate_event(MouseDownEvent {
        position: inside,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position: outside,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    draw(cx);
    assert!(shown(cx, "Edit project"));
    cx.simulate_click(outside, Modifiers::default());
    draw(cx);
    assert!(!shown(cx, "Edit project"));
    assert_eq!(v.read_with(cx, |s, _| s.background), 0);
}
#[gpui_kit::test]
fn more_than_a_hundred_background_controls_do_not_escape_tab_scope(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    click(cx, "Open");
    let d = state(&v, cx);
    for _ in 0..125 {
        press(cx, "tab");
        assert!(cx.update(|w, cx| d.read(cx).focus.contains_focused(w, cx)));
    }
    edit(&v, cx, |s| s.no_controls = true);
    draw(cx);
    for _ in 0..3 {
        press(cx, "tab");
        assert!(cx.update(|w, cx| d.read(cx).focus.contains_focused(w, cx)));
    }
}
#[gpui_kit::test]
fn viewport_and_rem_constraints_keep_long_surface_inside_window(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| s.long = true);
    cx.simulate_resize(size(px(420.), px(340.)));
    click(cx, "Open");
    for rem in [16., 24.] {
        cx.update(|w, _| w.set_rem_size(px(rem)));
        draw(cx);
        let b = node(cx, "Edit project").bounds();
        let margin = px(rem);
        assert!(b.top() >= margin && b.left() >= margin);
        assert!(
            b.bottom() <= px(340.) - margin && b.right() <= px(420.) - margin,
            "{b:?}"
        );
    }
}

fn part(cx: &mut VisualTestContext, id: &'static str) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&id.into()))
            .unwrap_or_else(|| panic!("missing part {id}"))
            .bounds()
    })
}

fn wheel(cx: &mut VisualTestContext, position: gpui_kit::Point<gpui_kit::Pixels>, delta: f32) {
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position,
        delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(delta))),
        touch_phase: gpui_kit::TouchPhase::Moved,
        ..Default::default()
    });
    draw(cx);
}

#[gpui_kit::test]
fn scrolling_long_body_keeps_header_footer_and_drag_geometry_fixed(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| {
        s.long = true;
        s.draggable = true;
    });
    click(cx, "Open");
    let surface = node(cx, "Edit project").bounds();
    let header = part(cx, "dialog-header");
    let footer = part(cx, "dialog-footer");
    let paragraph = part(cx, "paragraph-0");
    let position = part(cx, "dialog-body").center();
    wheel(cx, position, -300.);
    assert!(part(cx, "paragraph-0").top() < paragraph.top());
    assert_eq!(part(cx, "dialog-header"), header);
    assert_eq!(part(cx, "dialog-footer"), footer);
    assert_eq!(node(cx, "Edit project").bounds(), surface);
}

#[gpui_kit::test]
fn tiny_viewport_scrolls_shell_without_moving_surface_or_drag_bounds(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| {
        s.long = true;
        s.draggable = true;
    });
    cx.simulate_resize(size(px(420.), px(160.)));
    click(cx, "Open");
    let surface = node(cx, "Edit project").bounds();
    wheel(cx, surface.center(), -10000.);
    assert_eq!(node(cx, "Edit project").bounds(), surface);
    assert!(surface.contains(&node(cx, "OK").bounds().center()));
}

#[gpui_kit::test]
fn unmount_retires_scope_and_invalidates_pending_work(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| s.pending = true);
    click(cx, "Open");
    click(cx, "OK");
    let ticket = v.read_with(cx, |s, _| s.ticket.clone().unwrap());
    edit(&v, cx, |s| s.mounted = false);
    draw(cx);
    draw(cx);
    assert!(cx.update(|w, cx| gpui_kit::base::active_modal_scope(w, cx).is_none()));
    cx.update(|w, cx| assert!(!ticket.complete(DialogOutcome::Close, w, cx)));
}

#[gpui_kit::test]
fn editable_initial_focus_and_textarea_enter_preserve_input_priority(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    let (v, cx) = setup(cx);
    click(cx, "Open");
    assert!(cx.update(|w, cx| v.read(cx).input.read(cx).focus_handle(cx).is_focused(w)));
    press(cx, "enter");
    assert_eq!(v.read_with(cx, |s, _| s.confirms), 1);
    assert!(!shown(cx, "Edit project"));
    edit(&v, cx, |s| s.textarea = true);
    click(cx, "Open");
    click(cx, "Note");
    press(cx, "enter");
    assert!(shown(cx, "Edit project"));
    assert_eq!(
        v.read_with(cx, |s, cx| s.note.read(cx).value().to_string()),
        "\n"
    );
    assert_eq!(v.read_with(cx, |s, _| s.confirms), 1);
}

#[gpui_kit::test]
fn explicit_initial_focus_is_applied_after_render_once_per_session(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| s.explicit_focus = true);
    click(cx, "Open");
    assert!(cx.update(|w, cx| v.read(cx).inner.is_focused(w)));
    click(cx, "Name");
    edit(&v, cx, |s| s.long = true);
    assert!(cx.update(|w, cx| v.read(cx).input.read(cx).focus_handle(cx).is_focused(w)));
}

#[gpui_kit::test]
fn button_keyboard_activation_runs_one_command(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    click(cx, "Open");
    let d = state(&v, cx);
    cx.update(|w, cx| {
        let focus = d.read(cx).cancel_focus.clone();
        focus.focus(w, cx);
    });
    press(cx, "space");
    assert!(!shown(cx, "Edit project"));
    assert_eq!(v.read_with(cx, |s, _| s.confirms), 0);
    assert_eq!(v.read_with(cx, |s, _| s.guards), 1);
}

#[gpui_kit::test]
fn dragging_follows_pointer_clamps_and_new_session_recenters(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    edit(&v, cx, |s| s.draggable = true);
    click(cx, "Open");
    let original = node(cx, "Edit project").bounds();
    let start = original.origin + point(px(32.), px(32.));
    cx.simulate_event(MouseDownEvent {
        position: start,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_mouse_move(
        start + point(px(1.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    assert_eq!(
        node(cx, "Edit project").bounds(),
        original,
        "stay below drag threshold"
    );
    let end = point(px(10000.), px(10000.));
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    let moved = node(cx, "Edit project").bounds();
    assert_ne!(moved.origin, original.origin);
    let viewport = cx.update(|w, _| w.viewport_size());
    assert!(
        moved.right() <= viewport.width - px(16.) && moved.bottom() <= viewport.height - px(16.),
        "moved={moved:?}, viewport={viewport:?}, offset={:?}",
        state(&v, cx).read_with(cx, |s, _| s.drag.offset())
    );
    cx.simulate_event(MouseUpEvent {
        position: end,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    draw(cx);
    cx.simulate_resize(size(px(460.), px(420.)));
    draw(cx);
    draw(cx);
    let constrained = node(cx, "Edit project").bounds();
    assert!(constrained.left() >= px(16.) && constrained.top() >= px(16.));
    assert!(constrained.right() <= px(444.) && constrained.bottom() <= px(404.));
    click(cx, "Cancel");
    click(cx, "Open");
    let reopened = node(cx, "Edit project").bounds();
    assert!((reopened.center().x - px(230.)).abs() <= px(0.5));
}

#[gpui_kit::test]
fn guard_may_navigate_focus_without_close_completion_overriding_it(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    click(cx, "Open");
    let outside = v.read_with(cx, |s, _| s.outside.clone());
    let d = state(&v, cx);
    cx.update(|w, cx| {
        d.update(cx, |s, _| {
            s.behavior.guard = Some(Rc::new(move |_, w, cx| {
                outside.focus(w, cx);
                true
            }))
        });
        d.update(cx, |s, cx| {
            s.close(w, cx);
        });
    });
    draw(cx);
    assert!(cx.update(|w, cx| v.read(cx).outside.is_focused(w)));
}

#[gpui_kit::test]
fn removing_parent_declaration_retires_children_and_their_focus(cx: &mut TestAppContext) {
    let (v, cx) = setup(cx);
    click(cx, "Open");
    click(cx, "Child");
    edit(&v, cx, |s| s.mounted = false);
    draw(cx);
    draw(cx);
    assert!(cx.update(|w, cx| gpui_kit::base::active_modal_scope(w, cx).is_none()));
    assert!(!v.read_with(cx, |s, cx| s.child.read(cx).is_open()));
    assert!(cx.update(|w, cx| {
        w.focused(cx)
            .is_none_or(|f| Root::is_focus_available(&f, w))
    }));
}

#[test]
fn dialog_tokens_cover_all_profiles_and_the_scrim_retains_alpha() {
    let luminance = |color: u32| {
        let linear = |channel: u32| {
            let c = channel as f64 / 255.;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear((color >> 16) & 255)
            + 0.7152 * linear((color >> 8) & 255)
            + 0.0722 * linear(color & 255)
    };
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            for tone in crate::NeutralTone::ALL {
                let t = YueTheme::new(mode, accent).with_neutral(tone);
                for size in [
                    DialogSize::Small,
                    DialogSize::Medium,
                    DialogSize::Large,
                    DialogSize::ExtraLarge,
                ] {
                    assert!(size.width(t).to_pixels(px(16.)) > px(0.));
                }
                assert_eq!(rgba(t, keys::DIALOG_SCRIM_COLOR).a, 0.5);
                assert_eq!(
                    duration(t, keys::DIALOG_DURATION_ENTER),
                    Duration::from_millis(250)
                );
                assert_eq!(
                    duration(t, keys::DIALOG_DURATION_EXIT),
                    Duration::from_millis(150)
                );
                assert!(!shadow(t, keys::DIALOG_SHADOW).is_empty());
                let a = luminance(t.token(keys::DIALOG_COLOR).unwrap().color().unwrap());
                let b = luminance(t.token(keys::DIALOG_BACKGROUND).unwrap().color().unwrap());
                assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5);
            }
        }
    }
}

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{InteractiveElement as _, Styled as _};

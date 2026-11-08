use super::*;
use crate::{Accent, ThemeMode, YueButton, YueTheme};
use gpui_kit::base::Root;
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use std::time::Duration;

struct Demo {
    outside: FocusHandle,
    dialog: Entity<crate::YueDialogState>,
}
impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(
                YueButton::text(
                    "outside",
                    YueTheme::new(ThemeMode::Light, Accent::Azure),
                    "Outside",
                )
                .track_focus(&self.outside),
            )
            .child(
                crate::YueDialog::new("toast-modal", crate::current_theme(cx), &self.dialog)
                    .title("Modal"),
            )
    }
}
fn setup(cx: &mut TestAppContext) -> (Entity<YueToasterState>, &mut VisualTestContext) {
    cx.update(|cx| crate::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap());
    let (_, cx) = cx.add_window_view(|w, cx| {
        let view = cx.new(|cx| Demo {
            outside: cx.focus_handle(),
            dialog: cx.new(crate::YueDialogState::new),
        });
        Root::new(view, w, cx)
    });
    cx.update(|w, _| w.activate_window());
    draw(cx);
    let state = cx.update(|w, cx| YueToaster::for_window(w, cx).unwrap());
    (state, cx)
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
fn push(
    state: &Entity<YueToasterState>,
    cx: &mut VisualTestContext,
    spec: ToastSpec,
) -> ToastTicket {
    let ticket = cx.update(|w, cx| state.update(cx, |s, cx| s.push(spec, w, cx)).unwrap());
    draw(cx);
    draw(cx);
    ticket
}
fn snapshots(state: &Entity<YueToasterState>, cx: &mut VisualTestContext) -> Vec<ToastSnapshot> {
    cx.update(|_, cx| state.read(cx).snapshots(cx))
}

#[test]
fn defaults_and_validation() {
    assert_eq!(
        ToastSpec::new("Saved").lifetime(),
        Some(Duration::from_secs(4))
    );
    assert_eq!(
        ToastSpec::new("Error").kind(ToastKind::Error).lifetime(),
        Some(Duration::from_secs(8))
    );
    assert_eq!(
        ToastSpec::new("Loading")
            .kind(ToastKind::Loading)
            .lifetime(),
        None
    );
    assert_eq!(
        ToastSpec::new("Undo")
            .action(ToastAction::new("Undo", |_, _, _| ToastDecision::Close))
            .lifetime(),
        None
    );
    assert_eq!(
        ToastSpec::new("  ").validate(),
        Err(ToastError::InvalidTitle)
    );
    assert_eq!(
        ToastSpec::new("Saved")
            .timeout(Some(Duration::ZERO))
            .validate(),
        Err(ToastError::InvalidOptions)
    );
}
#[gpui_kit::test]
fn initial_measurement_then_presence_then_independent_expiry(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let a = push(
        &state,
        cx,
        ToastSpec::new("Saved A").timeout(Some(Duration::from_secs(1))),
    );
    assert_eq!(snapshots(&state, cx)[0].phase, ToastPhase::Starting);
    tick(cx, 300);
    assert_eq!(snapshots(&state, cx)[0].phase, ToastPhase::Present);
    let _b = push(
        &state,
        cx,
        ToastSpec::new("Loading B").kind(ToastKind::Loading),
    );
    tick(cx, 300);
    tick(cx, 700);
    assert_eq!(
        snapshots(&state, cx)
            .iter()
            .find(|s| s.generation == a.generation())
            .unwrap()
            .phase,
        ToastPhase::Ending
    );
    tick(cx, 200);
    let remaining = snapshots(&state, cx);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].phase, ToastPhase::Present);
    assert!(remaining[0].bounds.unwrap().size.height > px(0.));
}
#[gpui_kit::test]
fn update_preserves_order_and_exact_reading_budget(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let ticket = push(&state, cx, ToastSpec::new("A").id("a"));
    tick(cx, 300);
    tick(cx, 2000);
    let before = snapshots(&state, cx)[0].remaining.unwrap();
    cx.update(|w, cx| ticket.update(ToastSpec::new("A updated"), w, cx).unwrap());
    draw(cx);
    draw(cx);
    assert_eq!(snapshots(&state, cx)[0].remaining.unwrap(), before);
    tick(cx, 2000);
    assert_eq!(snapshots(&state, cx)[0].phase, ToastPhase::Ending);
    cx.update(|w, cx| {
        assert_eq!(
            ticket.update(ToastSpec::new("Too late"), w, cx),
            Err(ToastError::AlreadyClosing)
        )
    });
}
#[gpui_kit::test]
fn operation_ticket_cannot_overwrite_retry_or_new_generation(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let first = push(
        &state,
        cx,
        ToastSpec::new("Loading").id("job").kind(ToastKind::Loading),
    );
    let retry = cx.update(|w, cx| first.begin_operation(w, cx).unwrap());
    cx.update(|w, cx| {
        assert_eq!(
            first.update(ToastSpec::new("Old result"), w, cx),
            Err(ToastError::Stale)
        )
    });
    cx.update(|w, cx| {
        retry
            .update(ToastSpec::new("Done").kind(ToastKind::Success), w, cx)
            .unwrap()
    });
    draw(cx);
    draw(cx);
    tick(cx, 300);
    assert!(snapshots(&state, cx)[0].remaining.is_some());
    cx.update(|w, cx| retry.dismiss(w, cx).unwrap());
    let newer = push(&state, cx, ToastSpec::new("New job").id("job"));
    assert_ne!(newer.generation(), first.generation());
    cx.update(|w, cx| {
        assert_eq!(
            retry.update(ToastSpec::new("Stale result"), w, cx),
            Err(ToastError::AlreadyClosing)
        )
    });
}
#[gpui_kit::test]
fn capacity_counts_waiting_and_exits_and_configuration_requires_empty(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|w, cx| {
        state
            .update(cx, |s, cx| {
                s.configure(ToasterOptions::new().capacity(1), w, cx)
            })
            .unwrap()
    });
    let ticket = push(&state, cx, ToastSpec::new("A"));
    cx.update(|w, cx| {
        state.update(cx, |s, cx| {
            assert_eq!(
                s.push(ToastSpec::new("B"), w, cx).err(),
                Some(ToastError::Capacity)
            )
        });
        ticket.dismiss(w, cx).unwrap();
        state.update(cx, |s, cx| {
            assert_eq!(
                s.configure(ToasterOptions::new(), w, cx),
                Err(ToastError::Occupied)
            );
            assert_eq!(
                s.push(ToastSpec::new("C"), w, cx).err(),
                Some(ToastError::Capacity)
            );
        });
    });
    tick(cx, 200);
    cx.update(|w, cx| {
        state
            .update(cx, |s, cx| {
                s.configure(ToasterOptions::new().position(ToastPosition::Left), w, cx)
            })
            .unwrap()
    });
}
#[gpui_kit::test]
fn folded_tall_card_uses_front_shell_and_ending_snapshot_is_fixed(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let a = push(
        &state,
        cx,
        ToastSpec::new("Tall")
            .description("Line one\nLine two\nLine three\nLine four")
            .timeout(None),
    );
    tick(cx, 300);
    let b = push(&state, cx, ToastSpec::new("Short").timeout(None));
    tick(cx, 1000);
    let folded = snapshots(&state, cx);
    let old = folded
        .iter()
        .find(|s| s.generation == a.generation())
        .unwrap()
        .bounds
        .unwrap();
    let front = folded
        .iter()
        .find(|s| s.generation == b.generation())
        .unwrap()
        .bounds
        .unwrap();
    assert!((old.size.height - front.size.height).abs() < px(0.2));
    cx.update(|w, cx| a.dismiss(w, cx).unwrap());
    let _c = push(&state, cx, ToastSpec::new("Newest").timeout(None));
    let frozen = state.read_with(cx, |s, _| {
        s.manager
            .get(&a.generation())
            .unwrap()
            .exit
            .unwrap()
            .bounds
            .unwrap()
    });
    assert_eq!(frozen, old);
}
#[gpui_kit::test]
fn reduced_motion_settles_presence_without_delayed_unmount(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let ticket = push(&state, cx, ToastSpec::new("Saved"));
    assert_eq!(snapshots(&state, cx)[0].phase, ToastPhase::Present);
    cx.update(|w, cx| ticket.dismiss(w, cx).unwrap());
    draw(cx);
    assert!(snapshots(&state, cx).is_empty());
}
#[gpui_kit::test]
fn diagnostic_pause_and_step_use_one_local_clock(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|w, cx| state.update(cx, |s, cx| s.set_playback(ToastPlayback::Paused, w, cx)));
    let _ticket = push(&state, cx, ToastSpec::new("Paused"));
    tick(cx, 2000);
    assert_eq!(snapshots(&state, cx)[0].phase, ToastPhase::Starting);
    cx.update(|w, cx| state.update(cx, |s, cx| s.step(Duration::from_millis(300), w, cx)));
    draw(cx);
    assert_eq!(snapshots(&state, cx)[0].phase, ToastPhase::Present);
    cx.update(|w, cx| state.update(cx, |s, cx| s.step(Duration::from_secs(4), w, cx)));
    draw(cx);
    assert_eq!(snapshots(&state, cx)[0].phase, ToastPhase::Ending);
}

fn demo(cx: &mut VisualTestContext) -> Entity<Demo> {
    cx.update(|w, cx| {
        Root::read(w, cx)
            .view()
            .clone()
            .downcast::<Demo>()
            .ok()
            .unwrap()
    })
}
fn key(cx: &mut VisualTestContext, key: &str) {
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: gpui_kit::Keystroke::parse(key).unwrap(),
        is_held: false,
        prefer_character_input: false,
    });
    draw(cx);
}

#[gpui_kit::test]
fn modal_hides_existing_and_waits_for_barrier_before_presenting_new(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let a = push(&state, cx, ToastSpec::new("Before modal").progress(true));
    tick(cx, 300);
    tick(cx, 1000);
    let before = snapshots(&state, cx)[0].remaining.unwrap();
    let dialog = demo(cx).read_with(cx, |d, _| d.dialog.clone());
    cx.update(|w, cx| dialog.update(cx, |d, cx| d.open(w, cx)).unwrap());
    draw(cx);
    let b = push(&state, cx, ToastSpec::new("During modal"));
    tick(cx, 2000);
    assert!(state.read_with(cx, |s, _| s.hidden()));
    let values = snapshots(&state, cx);
    assert_eq!(
        values
            .iter()
            .find(|s| s.generation == a.generation())
            .unwrap()
            .remaining,
        Some(before)
    );
    assert_eq!(
        values
            .iter()
            .find(|s| s.generation == b.generation())
            .unwrap()
            .phase,
        ToastPhase::Waiting
    );
    cx.update(|w, cx| {
        dialog.update(cx, |d, cx| d.close(w, cx));
    });
    draw(cx);
    assert!(state.read_with(cx, |s, _| s.hidden()));
    tick(cx, 150);
    draw(cx);
    draw(cx);
    assert!(!state.read_with(cx, |s, _| s.hidden()));
    assert_ne!(
        snapshots(&state, cx)
            .iter()
            .find(|s| s.generation == b.generation())
            .unwrap()
            .phase,
        ToastPhase::Waiting
    );
}

#[gpui_kit::test]
fn hover_focus_pressed_are_a_union_and_leave_has_grace(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let _ticket = push(&state, cx, ToastSpec::new("Paused while interacting"));
    tick(cx, 300);
    let bounds = snapshots(&state, cx)[0].bounds.unwrap();
    cx.simulate_mouse_move(bounds.center(), None, gpui_kit::Modifiers::default());
    draw(cx);
    assert!(state.read_with(cx, |s, _| s.is_expanded()));
    let remaining = snapshots(&state, cx)[0].remaining;
    cx.update(|w, cx| {
        state.update(cx, |s, cx| {
            s.focus_notifications(w, cx);
            s.press(true, w, cx);
        });
    });
    cx.simulate_mouse_move(
        gpui_kit::point(px(0.), px(0.)),
        None,
        gpui_kit::Modifiers::default(),
    );
    tick(cx, 1000);
    assert!(state.read_with(cx, |s, _| s.is_paused()));
    assert_eq!(snapshots(&state, cx)[0].remaining, remaining);
    cx.update(|w, cx| {
        state.update(cx, |s, cx| {
            s.press(false, w, cx);
            s.restore_focus(w, cx);
        })
    });
    draw(cx);
    assert!(!state.read_with(cx, |s, _| s.is_paused()));
    tick(cx, 99);
    assert!(state.read_with(cx, |s, _| s.is_expanded()));
    tick(cx, 1);
    assert!(!state.read_with(cx, |s, _| s.is_expanded()));
}

#[gpui_kit::test]
fn eight_positions_keep_front_anchor_during_expansion(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.simulate_resize(gpui_kit::size(px(800.), px(600.)));
    cx.update(|_, cx| cx.set_reduce_motion(true));
    for position in ToastPosition::ALL {
        cx.update(|w, cx| {
            state
                .update(cx, |s, cx| {
                    s.configure(ToasterOptions::new().position(position), w, cx)
                })
                .unwrap()
        });
        let _old = push(
            &state,
            cx,
            ToastSpec::new("Old")
                .description("A taller message\nwith several lines")
                .timeout(None),
        );
        let ticket = push(&state, cx, ToastSpec::new("Front").timeout(None));
        let front = snapshots(&state, cx)
            .into_iter()
            .find(|s| s.generation == ticket.generation())
            .unwrap()
            .bounds
            .unwrap();
        assert!(
            front.left() >= px(24.) && front.right() <= px(776.),
            "{position:?}: {front:?}"
        );
        match position {
            p if p.side() => assert!((front.center().y - px(300.)).abs() < px(0.2)),
            p if p.bottom() => assert!((front.bottom() - px(576.)).abs() < px(0.2)),
            _ => assert!((front.top() - px(24.)).abs() < px(0.2)),
        }
        cx.update(|w, cx| state.update(cx, |s, cx| s.hover(true, w, cx)));
        draw(cx);
        let expanded = snapshots(&state, cx)
            .into_iter()
            .find(|s| s.generation == ticket.generation())
            .unwrap()
            .bounds
            .unwrap();
        assert_eq!(
            front, expanded,
            "front must not move on {position:?} expansion"
        );
        cx.update(|w, cx| state.update(cx, |s, cx| s.dismiss_all(w, cx)));
        draw(cx);
    }
}

#[gpui_kit::test]
fn hidden_actions_and_exits_leave_no_button_or_tab_target(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let action = ToastAction::new("Undo unique", |_, _, _| ToastDecision::Close);
    let a = push(&state, cx, ToastSpec::new("Old actionable").action(action));
    let _b = push(&state, cx, ToastSpec::new("Foreground").timeout(None));
    draw(cx);
    assert!(!cx.update(|w, _| {
        gpui_base::test_support::snapshots(w)
            .iter()
            .any(|n| n.label() == Some("Undo unique"))
    }));
    cx.update(|w, cx| state.update(cx, |s, cx| s.hover(true, w, cx)));
    draw(cx);
    assert!(cx.update(|w, _| {
        gpui_base::test_support::snapshots(w)
            .iter()
            .any(|n| n.label() == Some("Undo unique"))
    }));
    cx.update(|w, cx| a.dismiss(w, cx).unwrap());
    draw(cx);
    assert!(!cx.update(|w, _| {
        gpui_base::test_support::snapshots(w)
            .iter()
            .any(|n| n.label() == Some("Undo unique"))
    }));
}

#[gpui_kit::test]
fn focus_action_delete_escape_and_removed_action_restore_valid_focus(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let outside = demo(cx).read_with(cx, |d, _| d.outside.clone());
    cx.update(|w, cx| outside.focus(w, cx));
    draw(cx);
    let _ticket = push(&state, cx, ToastSpec::new("Keyboard toast").timeout(None));
    cx.update(|w, cx| w.dispatch_action(Box::new(FocusToasts), cx));
    draw(cx);
    assert!(cx.update(|w, cx| state.read(cx).focus.contains_focused(w, cx)));
    key(cx, "escape");
    assert!(cx.update(|w, _| outside.is_focused(w)));
    cx.update(|w, cx| w.dispatch_action(Box::new(FocusToasts), cx));
    draw(cx);
    key(cx, "delete");
    assert!(state.read_with(cx, |s, _| s.is_empty()));
    assert!(cx.update(|w, _| outside.is_focused(w)));
}

#[gpui_kit::test]
fn narrow_window_keeps_close_and_action_in_card_and_updates_measurement(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.simulate_resize(gpui_kit::size(px(280.), px(500.)));
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let ticket = push(
        &state,
        cx,
        ToastSpec::new("A long path /workspace/resources/a_very_long_file_name.md")
            .description("Contents can wrap on a narrow viewport.")
            .action(ToastAction::new("Undo", |_, _, _| ToastDecision::KeepOpen)),
    );
    let bounds = snapshots(&state, cx)[0].bounds.unwrap();
    cx.update(|w, _| {
        for n in gpui_base::test_support::snapshots(w)
            .into_iter()
            .filter(|n| matches!(n.label(), Some("Undo" | "关闭通知")))
        {
            assert!(bounds.contains(&n.bounds().origin));
            assert!(n.bounds().right() <= bounds.right());
            assert!(n.bounds().bottom() <= bounds.bottom());
        }
    });
    cx.simulate_resize(gpui_kit::size(px(800.), px(600.)));
    draw(cx);
    draw(cx);
    assert!(snapshots(&state, cx)[0].bounds.unwrap().size.height < bounds.size.height);
    cx.update(|w, cx| ticket.dismiss(w, cx).unwrap());
    draw(cx);
}

#[gpui_kit::test]
fn update_mid_fade_starts_at_current_alpha_and_exit_freezes_content(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let ticket = push(&state, cx, ToastSpec::new("Original").timeout(None));
    tick(cx, 300);
    cx.update(|w, cx| {
        ticket
            .update(ToastSpec::new("Second").timeout(None), w, cx)
            .unwrap()
    });
    draw(cx);
    draw(cx);
    tick(cx, 30);
    let alpha = state.read_with(cx, |s, cx| {
        let r = s.manager.get(&ticket.generation()).unwrap();
        super::card::content_sample(r, s.now(cx), Duration::from_millis(150), false).1
    });
    cx.update(|w, cx| {
        ticket
            .update(ToastSpec::new("Third").timeout(None), w, cx)
            .unwrap()
    });
    draw(cx);
    draw(cx);
    assert!(
        (state.read_with(cx, |s, _| s
            .manager
            .get(&ticket.generation())
            .unwrap()
            .previous_alpha)
            - alpha)
            .abs()
            < 0.001
    );
    cx.update(|w, cx| ticket.dismiss(w, cx).unwrap());
    draw(cx);
    let frozen = state.read_with(cx, |s, _| {
        s.manager
            .get(&ticket.generation())
            .unwrap()
            .exit_spec
            .as_ref()
            .unwrap()
            .title()
            .to_owned()
    });
    tick(cx, 100);
    assert_eq!(
        state.read_with(cx, |s, _| s
            .manager
            .get(&ticket.generation())
            .unwrap()
            .exit_spec
            .as_ref()
            .unwrap()
            .title()
            .to_owned()),
        frozen
    );
}

#[gpui_kit::test]
fn idle_and_diagnostic_pause_do_not_request_perpetual_frames(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let _ticket = push(&state, cx, ToastSpec::new("Idle").timeout(None));
    tick(cx, 1500);
    draw(cx);
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
    cx.update(|w, cx| state.update(cx, |s, cx| s.set_playback(ToastPlayback::Paused, w, cx)));
    let _loading = push(
        &state,
        cx,
        ToastSpec::new("Loading paused").kind(ToastKind::Loading),
    );
    draw(cx);
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
}

#[gpui_kit::test]
fn explicit_same_key_push_continues_the_exit_sample_with_new_generation(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let old = push(
        &state,
        cx,
        ToastSpec::new("Original").id("job").timeout(None),
    );
    tick(cx, 300);
    cx.update(|w, cx| old.dismiss(w, cx).unwrap());
    draw(cx);
    tick(cx, 50);
    let new = push(&state, cx, ToastSpec::new("Retry").id("job").timeout(None));
    assert_ne!(new.generation(), old.generation());
    assert!(state.read_with(cx, |s, _| {
        s.manager
            .get(&new.generation())
            .unwrap()
            .entry
            .unwrap()
            .bounds
            .is_some()
    }));
    assert!(state.read_with(cx, |s, _| {
        s.manager.get(&old.generation()).unwrap().superseded
    }));
}

#[gpui_kit::test]
fn oversized_card_scrolls_the_region_and_keeps_focused_action_visible(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    cx.simulate_resize(gpui_kit::size(px(420.), px(180.)));
    let ticket = push(
        &state,
        cx,
        ToastSpec::new("Tall message")
            .description((0..18).map(|i| format!("Line {i}\n")).collect::<String>())
            .action(ToastAction::new("Undo tall", |_, _, _| {
                ToastDecision::KeepOpen
            })),
    );
    cx.update(|w, cx| {
        let focus = state
            .read(cx)
            .manager
            .get(&ticket.generation())
            .unwrap()
            .action_focus
            .clone();
        focus.focus(w, cx);
    });
    draw(cx);
    draw(cx);
    draw(cx);
    let (target, viewport) = state.read_with(cx, |s, _| {
        (
            s.manager
                .get(&ticket.generation())
                .unwrap()
                .action_bounds
                .get()
                .unwrap(),
            s.scroll.bounds(),
        )
    });
    assert!(
        target.top() >= viewport.top() - px(0.2),
        "{target:?} vs {viewport:?}"
    );
    assert!(
        target.bottom() <= viewport.bottom() + px(0.2),
        "{target:?} vs {viewport:?}"
    );
}

#[gpui_kit::test]
fn live_events_have_stable_distinct_native_keys_and_modal_does_not_reannounce(
    cx: &mut TestAppContext,
) {
    use gpui_kit::Element as _;
    let (state, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let a = push(&state, cx, ToastSpec::new("Same text"));
    let b = push(&state, cx, ToastSpec::new("Same text"));
    let (key_a, key_b) = state.read_with(cx, |s, _| {
        let mut keys = vec![];
        for g in [a.generation(), b.generation()] {
            let r = s.manager.get(&g).unwrap();
            let node = super::layer::live_node(r);
            keys.push(node.id().unwrap());
            let mut native = gpui_kit::accesskit::Node::new(node.a11y_role().unwrap());
            node.write_a11y_info(&mut native);
            assert_eq!(native.label(), native.value());
            assert_eq!(native.live(), Some(gpui_kit::accesskit::Live::Polite));
            assert!(native.is_live_atomic());
            assert!(r.delivered.get());
            assert!(super::layer::live_visible(r));
        }
        (keys.remove(0), keys.remove(0))
    });
    assert_ne!(key_a, key_b);
    cx.update(|w, cx| {
        a.update(
            ToastSpec::new("Same text").appearance(ToastAppearance::Inverse),
            w,
            cx,
        )
        .unwrap()
    });
    draw(cx);
    draw(cx);
    assert_eq!(
        state.read_with(cx, |s, _| super::layer::live_node(
            s.manager.get(&a.generation()).unwrap()
        )
        .id()
        .unwrap()),
        key_a
    );
    let dialog = demo(cx).read_with(cx, |s, _| s.dialog.clone());
    cx.update(|w, cx| dialog.update(cx, |s, cx| s.open(w, cx)).unwrap());
    draw(cx);
    cx.update(|w, cx| {
        dialog.update(cx, |s, cx| s.close(w, cx));
    });
    draw(cx);
    assert!(!state.read_with(cx, |s, _| super::layer::live_visible(
        s.manager.get(&a.generation()).unwrap()
    )));
    cx.update(|w, cx| {
        a.update(
            ToastSpec::new("A meaningful revision").announcement_text("Success: settings saved"),
            w,
            cx,
        )
        .unwrap()
    });
    draw(cx);
    draw(cx);
    assert!(state.read_with(cx, |s, _| super::layer::live_visible(
        s.manager.get(&a.generation()).unwrap()
    )));
}

#[gpui_kit::test]
fn ordinary_tab_does_not_gain_toast_stops_until_notification_focus(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let outside = demo(cx).read_with(cx, |s, _| s.outside.clone());
    cx.update(|w, cx| outside.focus(w, cx));
    draw(cx);
    let _ticket = push(
        &state,
        cx,
        ToastSpec::new("Ready").action(ToastAction::new("Undo", |_, _, _| ToastDecision::KeepOpen)),
    );
    key(cx, "tab");
    assert!(cx.update(|w, _| outside.is_focused(w)));
    cx.update(|w, cx| w.dispatch_action(Box::new(FocusToasts), cx));
    draw(cx);
    key(cx, "tab");
    assert!(cx.update(|w, cx| state.read(cx).focus.contains_focused(w, cx)));
    key(cx, "escape");
    assert!(cx.update(|w, _| outside.is_focused(w)));
}

#[gpui_kit::test]
fn native_action_runs_once_and_loading_can_only_be_programmatically_dismissed(
    cx: &mut TestAppContext,
) {
    use std::{cell::Cell, rc::Rc};
    let (state, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    let _action = push(
        &state,
        cx,
        ToastSpec::new("Undoable").action(ToastAction::new("Undo once", move |_, _, _| {
            called.set(called.get() + 1);
            ToastDecision::Close
        })),
    );
    let target = cx.update(|w, _| {
        gpui_base::test_support::snapshots(w)
            .into_iter()
            .find(|n| n.label() == Some("Undo once"))
            .unwrap()
            .bounds()
            .center()
    });
    cx.simulate_click(target, gpui_kit::Modifiers::default());
    draw(cx);
    assert_eq!(calls.get(), 1);
    assert!(state.read_with(cx, |s, _| s.is_empty()));
    let loading = push(
        &state,
        cx,
        ToastSpec::new("Loading").kind(ToastKind::Loading),
    );
    cx.update(|w, cx| {
        assert!(
            !state
                .update(cx, |s, cx| s.dismiss(
                    &loading,
                    ToastDismissReason::CloseButton,
                    w,
                    cx
                ))
                .unwrap()
        );
        assert!(loading.dismiss(w, cx).unwrap());
    });
    draw(cx);
}

#[gpui_kit::test]
fn tickets_reject_a_different_window(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let ticket = push(&state, cx, ToastSpec::new("Window one"));
    let other = cx.update(|_, cx| {
        gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| Demo {
                outside: cx.focus_handle(),
                dialog: cx.new(crate::YueDialogState::new),
            })
        })
        .unwrap()
        .0
    });
    cx.update(|_, cx| {
        other
            .update(cx, |_, w, cx| {
                assert_eq!(
                    ticket.update(ToastSpec::new("Wrong window"), w, cx),
                    Err(ToastError::WrongWindow)
                )
            })
            .unwrap()
    });
}

#[gpui_kit::test]
fn tab_follows_visual_card_order_and_returns_to_application(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let outside = demo(cx).read_with(cx, |s, _| s.outside.clone());
    for position in [ToastPosition::TopRight, ToastPosition::BottomRight] {
        cx.update(|w, cx| {
            state
                .update(cx, |s, cx| {
                    s.configure(ToasterOptions::new().position(position), w, cx)
                })
                .unwrap();
            outside.focus(w, cx);
        });
        draw(cx);
        let old = push(
            &state,
            cx,
            ToastSpec::new("Old").action(ToastAction::new("Old action", |_, _, _| {
                ToastDecision::KeepOpen
            })),
        );
        let front = push(
            &state,
            cx,
            ToastSpec::new("Front").action(ToastAction::new("Front action", |_, _, _| {
                ToastDecision::KeepOpen
            })),
        );
        cx.update(|w, cx| {
            state.update(cx, |s, cx| s.focus_notifications(w, cx));
        });
        draw(cx);
        // Begin from the visually topmost card. Bottom placements put the older card above.
        let top = if position.bottom() { &old } else { &front };
        let bottom = if position.bottom() { &front } else { &old };
        cx.update(|w, cx| {
            let focus = state
                .read(cx)
                .manager
                .get(&top.generation())
                .unwrap()
                .focus
                .clone();
            focus.focus(w, cx);
        });
        draw(cx);
        for ticket in [top, bottom] {
            key(cx, "tab");
            assert!(cx.update(|w, cx| {
                state
                    .read(cx)
                    .manager
                    .get(&ticket.generation())
                    .unwrap()
                    .action_focus
                    .is_focused(w)
            }));
            key(cx, "tab");
            assert!(cx.update(|w, cx| {
                state
                    .read(cx)
                    .manager
                    .get(&ticket.generation())
                    .unwrap()
                    .close_focus
                    .is_focused(w)
            }));
        }
        key(cx, "shift-tab");
        assert!(cx.update(|w, cx| {
            state
                .read(cx)
                .manager
                .get(&bottom.generation())
                .unwrap()
                .action_focus
                .is_focused(w)
        }));
        key(cx, "tab");
        key(cx, "tab");
        assert!(cx.update(|w, _| outside.is_focused(w)));
        cx.update(|w, cx| state.update(cx, |s, cx| s.dismiss_all(w, cx)));
        draw(cx);
    }
}

#[gpui_kit::test]
fn new_card_retains_real_focus_when_old_card_leaves_visible_budget(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|w, cx| {
        cx.set_reduce_motion(true);
        state
            .update(cx, |s, cx| {
                s.configure(ToasterOptions::new().visible_toasts(1), w, cx)
            })
            .unwrap();
    });
    let _old = push(&state, cx, ToastSpec::new("Old").timeout(None));
    cx.update(|w, cx| {
        state.update(cx, |s, cx| s.focus_notifications(w, cx));
    });
    draw(cx);
    let front = push(&state, cx, ToastSpec::new("New").timeout(None));
    assert!(cx.update(|w, cx| {
        state
            .read(cx)
            .manager
            .get(&front.generation())
            .unwrap()
            .focus
            .is_focused(w)
    }));
    assert!(state.read_with(cx, |s, _| s.is_expanded() && s.is_paused()));
}

#[gpui_kit::test]
fn side_growth_is_chosen_once_for_a_nonempty_session(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|w, cx| {
        cx.set_reduce_motion(true);
        state
            .update(cx, |s, cx| {
                s.configure(ToasterOptions::new().position(ToastPosition::Left), w, cx)
            })
            .unwrap();
    });
    cx.simulate_resize(gpui_kit::size(px(800.), px(120.)));
    let _first = push(
        &state,
        cx,
        ToastSpec::new("Taller")
            .description("First\nSecond\nThird")
            .timeout(None),
    );
    assert!(state.read_with(cx, |s, _| s.grow_up));
    cx.simulate_resize(gpui_kit::size(px(800.), px(600.)));
    let _next = push(&state, cx, ToastSpec::new("Small").timeout(None));
    assert!(
        state.read_with(cx, |s, _| s.grow_up),
        "resize and hover must not flip a nonempty session"
    );
    cx.update(|w, cx| state.update(cx, |s, cx| s.dismiss_all(w, cx)));
    draw(cx);
    let _fresh = push(&state, cx, ToastSpec::new("Fresh session").timeout(None));
    assert!(!state.read_with(cx, |s, _| s.grow_up));
}

#[gpui_kit::test]
fn lifecycle_events_are_exact_and_window_close_releases_retained_state(cx: &mut TestAppContext) {
    struct Log {
        events: Vec<ToastEvent>,
        _subscription: gpui_kit::Subscription,
    }
    let (state, cx) = setup(cx);
    let log = cx.update(|_, cx| {
        cx.new(|cx| Log {
            events: vec![],
            _subscription: cx.subscribe(&state, |s: &mut Log, _, event, _| {
                s.events.push(event.clone())
            }),
        })
    });
    let expired = push(
        &state,
        cx,
        ToastSpec::new("Expires").timeout(Some(Duration::from_secs(1))),
    );
    tick(cx, 300);
    tick(cx, 1000);
    tick(cx, 200);
    log.read_with(cx, |s, _| {
        assert_eq!(s.events.len(), 3);
        assert!(matches!(&s.events[0], ToastEvent::Presented { generation, .. } if *generation == expired.generation()));
        assert!(matches!(&s.events[1], ToastEvent::Dismissed { reason: ToastDismissReason::Timeout, .. }));
        assert!(matches!(&s.events[2], ToastEvent::Removed { reason: ToastDismissReason::Timeout, .. }));
    });
    let manual = push(&state, cx, ToastSpec::new("Dismiss once").timeout(None));
    tick(cx, 300);
    cx.update(|w, cx| {
        assert!(manual.dismiss(w, cx).unwrap());
        assert!(!manual.dismiss(w, cx).unwrap());
    });
    tick(cx, 200);
    assert_eq!(log.read_with(cx, |s, _| s.events.len()), 6);
    let _live = push(
        &state,
        cx,
        ToastSpec::new("Window-owned loading").kind(ToastKind::Loading),
    );
    // Retaining the state/ticket must not keep timers or records after its window closes.
    cx.update(|w, _| w.remove_window());
    cx.run_until_parked();
    assert!(state.read_with(cx, |s, cx| s.is_empty() && s.snapshots(cx).is_empty()));
    log.read_with(cx, |s, _| {
        assert!(matches!(
            &s.events[s.events.len() - 2],
            ToastEvent::Dismissed {
                reason: ToastDismissReason::WindowClosed,
                ..
            }
        ));
        assert!(matches!(
            &s.events[s.events.len() - 1],
            ToastEvent::Removed {
                reason: ToastDismissReason::WindowClosed,
                ..
            }
        ));
    });
}

#[gpui_kit::test]
fn countdown_paints_the_card_silhouette_through_a_bottom_strip(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let ticket = push(&state, cx, ToastSpec::new("Countdown").progress(true));
    tick(cx, 300);
    tick(cx, 1000);
    let snapshot = snapshots(&state, cx)
        .into_iter()
        .find(|s| s.generation == ticket.generation())
        .unwrap();
    let card = snapshot.bounds.unwrap();
    cx.update(|w, cx| {
        let theme = crate::current_theme(cx);
        let accent: gpui_kit::Hsla =
            crate::tokens::rgba(theme, crate::theme::keys::ACCENT_SOLID).into();
        let scale = w.scale_factor();
        let progress = w
            .painted_quads()
            .into_iter()
            .find(|q| q.background.as_solid() == Some(accent))
            .expect("countdown fill submitted to scene");
        assert!(
            progress.corner_radii.bottom_left > gpui_kit::ScaledPixels(0.),
            "a rectangular strip cannot follow the card's rounded corners"
        );
        assert!(
            progress.bounds.size.height.as_f32() > f32::from(card.size.height) * scale * 0.8,
            "paint the complete rounded silhouette, then mask its bottom strip"
        );
        assert!(progress.content_mask.bounds.size.height < progress.bounds.size.height);
        assert!(progress.content_mask.bounds.size.width < progress.bounds.size.width);
    });
}

#[gpui_kit::test]
fn expiry_of_an_unmounted_toast_cannot_repaint_its_stale_card(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    let old = push(
        &state,
        cx,
        ToastSpec::new("Hidden error")
            .kind(ToastKind::Error)
            .appearance(ToastAppearance::Rich)
            .timeout(Some(Duration::from_secs(1))),
    );
    tick(cx, 300);
    for title in ["Back", "Middle", "Front"] {
        push(&state, cx, ToastSpec::new(title).timeout(None));
        tick(cx, 50);
    }
    let error_background: gpui_kit::Hsla = cx.update(|_, cx| {
        crate::tokens::rgba(crate::current_theme(cx), crate::theme::keys::ERROR_BG).into()
    });
    tick(cx, 950);
    assert_eq!(
        snapshots(&state, cx)
            .iter()
            .find(|s| s.generation == old.generation())
            .unwrap()
            .phase,
        ToastPhase::Ending
    );
    cx.update(|w, _| {
        assert!(
            !w.painted_quads()
                .iter()
                .any(|q| q
                    .background
                    .as_solid()
                    .is_some_and(|c| (c.h - error_background.h).abs() < 0.00001
                        && (c.s - error_background.s).abs() < 0.00001
                        && (c.l - error_background.l).abs() < 0.00001
                        && c.a > 0.001)),
            "an expired toast outside the visible budget must not reappear above the stack"
        )
    });
    let shown = snapshots(&state, cx)
        .into_iter()
        .filter(|s| s.generation != old.generation())
        .map(|s| (s.generation, s.bounds))
        .collect::<Vec<_>>();
    tick(cx, 200);
    assert_eq!(
        snapshots(&state, cx)
            .into_iter()
            .map(|s| (s.generation, s.bounds))
            .collect::<Vec<_>>(),
        shown,
        "removing an invisible lifecycle record cannot move visible cards"
    );
}

#[gpui_kit::test]
fn countdown_masks_share_card_radius_at_every_theme_appearance_and_scale(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for scale in [1., 1.5, 2.] {
            cx.simulate_scale_factor_change(scale);
            for rem in [16., 20.] {
                cx.update(|w, cx| {
                    crate::set_theme(cx, YueTheme::new(mode, Accent::Azure));
                    w.set_rem_size(px(rem));
                });
                for appearance in [
                    ToastAppearance::Standard,
                    ToastAppearance::Rich,
                    ToastAppearance::Inverse,
                ] {
                    let ticket = push(
                        &state,
                        cx,
                        ToastSpec::new("Rounded countdown")
                            .kind(ToastKind::Info)
                            .appearance(appearance)
                            .progress(true)
                            .timeout(Some(Duration::from_secs(4))),
                    );
                    for elapsed in [0, 1000, 2900] {
                        tick(cx, elapsed);
                        let snapshot = snapshots(&state, cx).pop().unwrap();
                        let fraction = snapshot.remaining.unwrap().as_secs_f32()
                            / snapshot.total.unwrap().as_secs_f32();
                        let card = snapshot.bounds.unwrap();
                        cx.update(|w, cx| {
                            let theme = crate::current_theme(cx);
                            let expected: gpui_kit::Hsla = crate::tokens::rgba(
                                theme,
                                if appearance == ToastAppearance::Inverse {
                                    crate::theme::keys::BOX_COLOR_TOAST
                                } else {
                                    crate::theme::keys::INFO
                                },
                            )
                            .into();
                            let fill = w
                                .painted_quads()
                                .into_iter()
                                .find(|q| {
                                    q.background.as_solid().is_some_and(|c| {
                                        (c.h - expected.h).abs() < 0.00001
                                            && (c.s - expected.s).abs() < 0.00001
                                            && (c.l - expected.l).abs() < 0.00001
                                            && c.a > 0.999
                                    }) && q.corner_radii.bottom_left > gpui_kit::ScaledPixels(0.)
                                        && q.content_mask.bounds.size.height < q.bounds.size.height
                                })
                                .expect("rounded countdown quad");
                            let radius = super::card::length(
                                theme,
                                crate::theme::keys::TOAST_BORDER_RADIUS,
                                w,
                            ) - px(1.);
                            assert!(
                                (fill.corner_radii.bottom_left.as_f32()
                                    - f32::from(radius) * scale)
                                    .abs()
                                    < 0.01
                            );
                            assert_eq!(
                                fill.corner_radii.bottom_left,
                                fill.corner_radii.bottom_right
                            );
                            assert!(
                                (fill.bounds.left().as_f32()
                                    - f32::from(card.left() + px(1.)) * scale)
                                    .abs()
                                    <= 1.
                            );
                            assert!(
                                (fill.bounds.right().as_f32()
                                    - f32::from(card.right() - px(1.)) * scale)
                                    .abs()
                                    <= 1.
                            );
                            assert!(
                                (fill.content_mask.bounds.size.width.as_f32()
                                    - fill.bounds.size.width.as_f32() * fraction)
                                    .abs()
                                    <= 1.
                            );
                            assert!(
                                (fill.content_mask.bounds.bottom().as_f32()
                                    - fill.bounds.bottom().as_f32())
                                .abs()
                                    <= 1.
                            );
                            let height = super::card::length(
                                theme,
                                crate::theme::keys::TOAST_PROGRESS_HEIGHT,
                                w,
                            );
                            assert!(
                                (fill.content_mask.bounds.size.height.as_f32()
                                    - f32::from(height) * scale)
                                    .abs()
                                    <= 1.
                            );
                        });
                    }
                    cx.update(|w, cx| ticket.dismiss(w, cx).unwrap());
                    draw(cx);
                    assert!(snapshots(&state, cx).is_empty());
                }
            }
        }
    }
}

#[gpui_kit::test]
fn dismissing_hidden_cards_during_rapid_pushes_preserves_visible_samples(cx: &mut TestAppContext) {
    let (state, cx) = setup(cx);
    for position in ToastPosition::ALL {
        cx.update(|w, cx| {
            cx.set_reduce_motion(false);
            state
                .update(cx, |s, cx| {
                    s.configure(ToasterOptions::new().position(position), w, cx)
                })
                .unwrap();
        });
        let hidden = push(&state, cx, ToastSpec::new("Old visible card").timeout(None));
        tick(cx, 300);
        for title in ["Back", "Middle", "Front"] {
            push(&state, cx, ToastSpec::new(title).timeout(None));
            tick(cx, 16);
        }
        let before = snapshots(&state, cx)
            .into_iter()
            .filter(|s| s.generation != hidden.generation())
            .map(|s| (s.generation, s.bounds))
            .collect::<Vec<_>>();
        assert!(
            snapshots(&state, cx)
                .iter()
                .find(|s| s.generation == hidden.generation())
                .unwrap()
                .bounds
                .is_none(),
            "leaving the visible budget invalidates the mounted geometry"
        );
        cx.update(|w, cx| hidden.dismiss(w, cx).unwrap());
        draw(cx);
        assert!(
            state.read_with(cx, |s, _| s
                .manager
                .get(&hidden.generation())
                .unwrap()
                .exit
                .unwrap()
                .bounds
                .is_none()),
            "an unmounted record has no exit surface"
        );
        assert_eq!(
            snapshots(&state, cx)
                .into_iter()
                .filter(|s| s.generation != hidden.generation())
                .map(|s| (s.generation, s.bounds))
                .collect::<Vec<_>>(),
            before,
            "same-time dismissal cannot restart visible springs or change card geometry"
        );
        cx.update(|w, cx| {
            cx.set_reduce_motion(true);
            state.update(cx, |s, cx| s.dismiss_all(w, cx));
        });
        draw(cx);
        assert!(snapshots(&state, cx).is_empty());
    }
}

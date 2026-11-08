use super::super::runtime::{PopupDismissReason, PopupValidity};
use super::super::*;
use gpui_kit::base::{Align, Placement, PresencePhase, Root, TestSupportExt as _, Transition};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, Modifiers, ParentElement as _, Render, Role, StatefulInteractiveElement as _,
    StyleRefinement, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

type Sample = Rc<RefCell<Option<PopupSnapshotHandle>>>;
struct Probe {
    anchor: PopupAnchor,
    child_anchor: PopupAnchor,
    blocker: PopupAnchor,
    sample: Sample,
    child_sample: Sample,
    focus: FocusHandle,
    x: f32,
    y: f32,
    open: bool,
    anchor_visible: bool,
    nested: bool,
    pointer: PopupPointerMode,
    route: PopupDismissRoute,
    placement: PopupPlacement,
    arrow: bool,
    tip_gap: bool,
    gap: f32,
    popup_width: f32,
    popup_height: f32,
    move_budget: Option<f32>,
    protection: bool,
    fallback: bool,
    scroll: gpui_kit::ScrollHandle,
    scrolling: bool,
    near_right_edge: bool,
    motion: bool,
    entrance_distance: f32,
    custom_candidates: bool,
    clicks: Rc<Cell<usize>>,
    hover: Rc<Cell<bool>>,
    factories: Rc<Cell<usize>>,
    dismissals: Vec<PopupDismissReason>,
    owner_escape: usize,
}
impl Probe {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            anchor: PopupAnchor::default(),
            child_anchor: PopupAnchor::default(),
            blocker: PopupAnchor::default(),
            sample: Sample::default(),
            child_sample: Sample::default(),
            focus: cx.focus_handle(),
            x: 100.,
            y: 100.,
            open: true,
            anchor_visible: true,
            nested: false,
            pointer: PopupPointerMode::PassThrough,
            route: PopupDismissRoute::None,
            placement: PopupPlacement::Side(Placement::Bottom, Align::Start),
            arrow: false,
            tip_gap: false,
            gap: 8.,
            popup_width: 120.,
            popup_height: 40.,
            move_budget: None,
            protection: false,
            fallback: false,
            scroll: gpui_kit::ScrollHandle::new(),
            scrolling: false,
            near_right_edge: false,
            motion: false,
            entrance_distance: 0.,
            custom_candidates: false,
            clicks: Rc::new(Cell::new(0)),
            hover: Rc::new(Cell::new(false)),
            factories: Rc::new(Cell::new(0)),
            dismissals: vec![],
            owner_escape: 0,
        }
    }
    fn snapshot(&self) -> super::super::runtime::PopupSnapshot {
        self.sample.borrow().as_ref().expect("mounted host").read()
    }
}
impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let runtime = PopupRuntime::for_window(window, cx);
        let id = PopupId::new(cx.entity_id(), "popup");
        let weak = cx.weak_entity();
        let mut spec = PopupSpec::new(id.clone(), self.anchor.handle(), self.open, self.placement)
            .surface_gap(px(self.gap))
            .margin(px(4.))
            .pointer_mode(self.pointer)
            .dismiss_route(self.route)
            .dismiss_on(true, true)
            .exclude(self.anchor.handle())
            .on_dismiss(move |request, _, cx| {
                let _ = weak.update(cx, |probe, cx| {
                    probe.dismissals.push(request.reason());
                    probe.open = false;
                    cx.notify();
                });
            });
        if let Some(budget) = self.move_budget {
            spec = spec.translate(Transition::new(Duration::from_millis(150)), px(budget));
        }
        if self.protection {
            spec = spec.protected_regions([(self.blocker.handle(), px(0.))]);
        }
        if self.fallback {
            spec = spec.fallback([(Placement::Top, Align::Start)]);
        }
        if self.tip_gap {
            spec = spec.tip_gap(px(self.gap));
        }
        if self.arrow {
            spec = spec.arrow(
                ArrowSpec::new(px(12.), px(6.))
                    .border_width(px(1.))
                    .style(gpui_kit::rgb(0xffffff), gpui_kit::rgb(0x222222)),
            );
        }
        if self.motion {
            spec = spec.motion(
                PopupMotion::new(
                    Transition::new(Duration::from_millis(100)),
                    Transition::new(Duration::from_millis(100)),
                )
                .entrance_distance(px(self.entrance_distance)),
            );
        }
        if self.custom_candidates {
            let gap = px(self.gap + if self.tip_gap && self.arrow { 6. } else { 0. });
            spec = spec.candidates_with(move |_, _, _| {
                vec![PopupCandidate::new(Placement::Bottom, Align::Start, gap)]
            });
        }
        let sample = self.sample.clone();
        let child_sample = self.child_sample.clone();
        let child_anchor = self.child_anchor.clone();
        let nested = self.nested;
        let owner = cx.entity_id();
        let factories = self.factories.clone();
        let width = self.popup_width;
        let height = self.popup_height;
        let surface = PopupHost::new(spec).mount(
            move |presentation, snapshot, window, cx| {
                factories.set(factories.get() + 1);
                *sample.borrow_mut() = Some(snapshot);
                let child = if nested {
                    PopupHost::new(
                        PopupSpec::new(
                            PopupId::new(owner, "child"),
                            child_anchor.handle(),
                            presentation.is_interactive(),
                            PopupPlacement::Side(Placement::Right, Align::Start),
                        )
                        .parent(id)
                        .surface_gap(px(8.))
                        .margin(px(4.))
                        .pointer_mode(PopupPointerMode::Block)
                        .dismiss_route(PopupDismissRoute::Owner),
                    )
                    .mount(
                        move |_, snapshot, _, _| {
                            *child_sample.borrow_mut() = Some(snapshot);
                            div()
                                .id("child-surface")
                                .test_support()
                                .role(Role::Menu)
                                .aria_label("child surface")
                                .w(px(60.))
                                .h(px(30.))
                                .into_any_element()
                        },
                        window,
                        cx,
                    )
                } else {
                    None
                };
                div()
                    .id("surface")
                    .test_support()
                    .when(presentation.is_interactive(), |surface| {
                        surface.role(Role::Tooltip).aria_label("surface")
                    })
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .child(child_anchor.capture())
                    .children(child)
                    .into_any_element()
            },
            window,
            cx,
        );
        let clicks = self.clicks.clone();
        let hover = self.hover.clone();
        let x = if self.near_right_edge {
            f32::from(window.viewport_size().width) - 100.
        } else {
            self.x
        };
        let anchor = div()
            .id("anchor")
            .test_support()
            .role(Role::Button)
            .aria_label("anchor")
            .absolute()
            .left(px(x))
            .top(px(self.y))
            .w(px(80.))
            .h(px(24.))
            .child(self.anchor.capture());
        let anchor = if self.scrolling {
            div()
                .id("scroll")
                .absolute()
                .left_0()
                .top_0()
                .w(px(400.))
                .h(px(200.))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(div().relative().w_full().h(px(600.)).child(anchor))
                .into_any_element()
        } else {
            anchor.into_any_element()
        };
        div()
            .size_full()
            .key_context("PopupProbe")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|probe, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    probe.owner_escape += 1;
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .id("under")
                    .test_support()
                    .role(Role::Button)
                    .aria_label("under")
                    .absolute()
                    .left(px(100.))
                    .top(px(132.))
                    .w(px(120.))
                    .h(px(40.))
                    .on_hover(move |active, _, _| hover.set(*active))
                    .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
            )
            .when(self.anchor_visible, |body| body.child(anchor))
            .child(
                div()
                    .absolute()
                    .left(px(240.))
                    .top(px(125.))
                    .w(px(20.))
                    .h(px(50.))
                    .child(self.blocker.capture()),
            )
            .children(surface)
            .child(gpui_kit::canvas(
                |_, _, _| (),
                move |_, _, _, _| {
                    let _ = &runtime;
                },
            ))
    }
}

struct CachedProbe(Entity<Probe>);
impl Render for CachedProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            self.0
                .clone()
                .cached(StyleRefinement::default().size_full()),
        )
    }
}
fn open(
    cx: &mut TestAppContext,
    configure: impl FnOnce(&mut Probe),
) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        super::super::init(cx);
    });
    let entity = Rc::new(RefCell::new(None));
    let save = entity.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let probe = cx.new(|cx| {
            let mut probe = Probe::new(cx);
            configure(&mut probe);
            probe
        });
        *save.borrow_mut() = Some(probe.clone());
        let cached = cx.new(|_| CachedProbe(probe));
        Root::new(cached, window, cx)
    });
    let entity = entity.borrow_mut().take().unwrap();
    cx.update(|window, cx| {
        window.activate_window();
        let focus = entity.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    draw(cx);
    (entity, cx)
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
fn tick(cx: &mut VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}
fn change(view: &Entity<Probe>, cx: &mut VisualTestContext, edit: impl FnOnce(&mut Probe)) {
    cx.update(|_, cx| {
        view.update(cx, |probe, cx| {
            edit(probe);
            cx.notify();
        })
    });
    draw(cx);
}
fn snapshot(
    view: &Entity<Probe>,
    cx: &mut VisualTestContext,
) -> super::super::runtime::PopupSnapshot {
    cx.update(|_, cx| view.read(cx).snapshot())
}

#[gpui_kit::test]
fn first_frame_and_one_relayout_use_the_current_side_anchor(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |_| {});
    let first = snapshot(&view, cx);
    let initial = first.position().unwrap();
    assert_eq!(initial.bounds.origin, point(px(100.), px(132.)));
    assert_eq!(
        first.anchor_bounds().unwrap().origin,
        point(px(100.), px(100.))
    );
    change(&view, cx, |probe| {
        probe.x = 160.;
        probe.y = 170.;
    });
    let next = snapshot(&view, cx);
    assert_eq!(
        next.position().unwrap().bounds.origin,
        point(px(160.), px(202.))
    );
    assert!(next.revision() > first.revision());
    assert_eq!(
        next.anchor_bounds().unwrap().origin,
        point(px(160.), px(170.))
    );
}

#[gpui_kit::test]
fn pass_through_preserves_hover_and_click_while_block_only_covers_surface(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |_| {});
    let at = point(px(150.), px(150.));
    // Assert during input dispatch: the test platform reports (0, 0) on a
    // subsequent platform draw unless a physical pointer device updates it.
    cx.update(|window, cx| {
        window.simulate_mouse_move(at, cx);
        assert!(view.read(cx).hover.get());
    });
    cx.simulate_click(at, Modifiers::default());
    draw(cx);
    assert_eq!(cx.update(|_, cx| view.read(cx).clicks.get()), 1);
    change(&view, cx, |probe| probe.pointer = PopupPointerMode::Block);
    cx.update(|window, cx| {
        window.simulate_mouse_move(at, cx);
        assert!(!view.read(cx).hover.get());
    });
    cx.simulate_click(at, Modifiers::default());
    draw(cx);
    assert_eq!(cx.update(|_, cx| view.read(cx).clicks.get()), 1);
}

#[gpui_kit::test]
fn all_twelve_placements_and_point_share_base_resolution(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.x = 300.;
        probe.y = 250.;
    });
    for side in [
        Placement::Top,
        Placement::Bottom,
        Placement::Left,
        Placement::Right,
    ] {
        for align in [Align::Start, Align::Center, Align::End] {
            change(&view, cx, |probe| {
                probe.placement = PopupPlacement::Side(side, align)
            });
            let position = snapshot(&view, cx).position().unwrap();
            assert_eq!(position.placement, Some(side));
            let cross = match align {
                Align::Start => 0.,
                Align::Center => -20.,
                Align::End => -40.,
            };
            let vertical_cross = cross * 0.4;
            let expected = match side {
                Placement::Top => point(px(300. + cross), px(202.)),
                Placement::Bottom => point(px(300. + cross), px(282.)),
                Placement::Left => point(px(172.), px(250. + vertical_cross)),
                Placement::Right => point(px(388.), px(250. + vertical_cross)),
            };
            assert_eq!(position.bounds.origin, expected);
        }
    }
    change(&view, cx, |probe| {
        probe.placement = PopupPlacement::Point(gpui_kit::Anchor::TopLeft)
    });
    let point = snapshot(&view, cx).position().unwrap();
    assert_eq!(point.placement, None);
    assert_eq!(point.bounds.origin, gpui_kit::point(px(300.), px(250.)));
}

#[gpui_kit::test]
fn arrow_envelope_distinguishes_tip_gap_and_surface_gap(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| probe.arrow = true);
    let surface = snapshot(&view, cx);
    assert_eq!(surface.position().unwrap().bounds.top(), px(132.));
    let tip_stroke = 0.5 * 2.0_f32.sqrt();
    assert_eq!(
        surface.visual_bounds().unwrap().top(),
        px(126. - tip_stroke)
    );
    change(&view, cx, |probe| probe.tip_gap = true);
    let tip = snapshot(&view, cx);
    assert_eq!(tip.position().unwrap().bounds.top(), px(138.));
    assert_eq!(tip.visual_bounds().unwrap().top(), px(132. - tip_stroke));
    change(&view, cx, |probe| probe.popup_width = 10.);
    assert_eq!(
        snapshot(&view, cx).validity(),
        PopupValidity::ArrowDoesNotFit
    );
}

#[gpui_kit::test]
fn protected_endpoints_and_protected_sweeps_fail_before_hit_testing(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.protection = true;
        probe.move_budget = Some(200.);
    });
    change(&view, cx, |probe| probe.x = 280.);
    assert_eq!(snapshot(&view, cx).validity(), PopupValidity::UnsafeMotion);
    // Repeating the same rejected target cannot erase the last displayed origin.
    change(&view, cx, |_| {});
    assert_eq!(snapshot(&view, cx).validity(), PopupValidity::UnsafeMotion);
    change(&view, cx, |probe| {
        probe.x = 250.;
        probe.move_budget = None;
    });
    assert_eq!(snapshot(&view, cx).validity(), PopupValidity::Protected);
}

#[gpui_kit::test]
fn missing_anchor_requests_once_and_cannot_reopen_from_an_old_true(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| probe.anchor_visible = false);
    for _ in 0..3 {
        draw(cx);
    }
    assert_eq!(
        cx.update(|_, cx| view.read(cx).dismissals.clone()),
        [PopupDismissReason::AnchorInvalid]
    );
    assert!(snapshot(&view, cx).position().is_none());
}

#[gpui_kit::test]
fn stale_registration_drop_cannot_remove_its_replacement(cx: &mut TestAppContext) {
    use super::super::runtime::RegistrationSpec;
    let (view, cx) = open(cx, |_| {});
    let runtime = cx.update(|window, cx| PopupRuntime::for_window(window, cx));
    let id = PopupId::new(view.entity_id(), "generation-test");
    let spec = || RegistrationSpec {
        modal: None,
        id: id.clone(),
        parent: None,
        present: true,
        route: PopupDismissRoute::None,
        escape: false,
        outside: false,
        exclude: vec![],
        scope: None,
        handler: None,
    };
    let first = runtime.register(spec());
    let second = runtime.register(spec());
    assert!(second.generation() > first.generation());
    let anchor = cx.update(|_, cx| view.read(cx).anchor.handle().snapshot().unwrap());
    let position = snapshot(&view, cx).position().unwrap();
    second.snapshot().publish(
        PresencePhase::Present,
        anchor,
        position,
        position.bounds,
        None,
    );
    drop(first);
    assert!(runtime.contains_or_excluded(&id, position.bounds.center()));
    drop(second);
    assert!(!runtime.contains_or_excluded(&id, position.bounds.center()));
}

#[gpui_kit::test]
fn real_scroll_moves_anchor_surface_and_arrow_in_the_same_frame(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.scrolling = true;
        probe.arrow = true;
    });
    change(&view, cx, |probe| {
        probe.scroll.set_offset(point(px(0.), px(-60.)))
    });
    let sample = snapshot(&view, cx);
    assert_eq!(sample.anchor_bounds().unwrap().top(), px(40.));
    assert_eq!(sample.position().unwrap().bounds.top(), px(72.));
    assert_eq!(sample.arrow().unwrap().outline()[1].y, px(66.));
    cx.update(|window, _| {
        let actual = gpui_base::test_support::snapshots(window)
            .into_iter()
            .find(|node| node.label() == Some("surface"))
            .unwrap();
        assert_eq!(actual.bounds(), sample.position().unwrap().bounds);
    });
}

#[gpui_kit::test]
fn viewport_resize_keeps_current_anchor_and_complete_envelope_inside_window(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, |probe| {
        probe.near_right_edge = true;
        probe.arrow = true;
    });
    cx.simulate_resize(gpui_kit::size(px(640.), px(480.)));
    draw(cx);
    let sample = snapshot(&view, cx);
    assert_eq!(sample.anchor_bounds().unwrap().left(), px(540.));
    assert!(sample.visual_bounds().unwrap().right() <= px(636.));
    cx.update(|window, _| {
        let actual = gpui_base::test_support::snapshots(window)
            .into_iter()
            .find(|node| node.label() == Some("surface"))
            .unwrap();
        assert_eq!(actual.bounds(), sample.position().unwrap().bounds);
    });
}

#[gpui_kit::test]
fn bounded_fallback_avoids_a_protected_target_without_rebuilding_content(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.x = 250.;
        probe.protection = true;
        probe.fallback = true;
    });
    assert_eq!(
        snapshot(&view, cx).position().unwrap().placement,
        Some(Placement::Top)
    );
    let count = cx.update(|_, cx| view.read(cx).factories.get());
    // Reusing a cached subtree preserves both the registration and the content.
    for _ in 0..2 {
        cx.update(|window, cx| {
            window
                .root::<Root>()
                .unwrap()
                .unwrap()
                .update(cx, |_, cx| cx.notify())
        });
        draw(cx);
    }
    assert_eq!(cx.update(|_, cx| view.read(cx).factories.get()), count);
}

#[gpui_kit::test]
fn a_blocking_surface_hits_at_its_sampled_position_during_translation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.pointer = PopupPointerMode::Block;
        probe.move_budget = Some(160.);
    });
    change(&view, cx, |probe| probe.x = 260.);
    tick(cx, 30);
    let sample = snapshot(&view, cx).position().unwrap().bounds;
    assert!(sample.left() < px(200.) && sample.right() > px(200.));
    cx.update(|window, _| {
        let actual = gpui_base::test_support::snapshots(window)
            .into_iter()
            .find(|node| node.label() == Some("surface"))
            .unwrap();
        assert_eq!(actual.bounds(), sample);
    });
    cx.simulate_click(point(px(200.), px(150.)), Modifiers::default());
    assert_eq!(cx.update(|_, cx| view.read(cx).clicks.get()), 0);
}

#[gpui_kit::test]
fn retained_registry_is_window_local_and_stable_frames_stop_scheduling(cx: &mut TestAppContext) {
    let mut first_context = cx.clone();
    let (first, first_window) = open(&mut first_context, |_| {});
    let first_runtime = first_window.update(|window, cx| PopupRuntime::for_window(window, cx));
    let first_sample = snapshot(&first, first_window).position().unwrap();
    let (second, second_window) = open(cx, |_| {});
    let second_runtime = second_window.update(|window, cx| PopupRuntime::for_window(window, cx));
    assert!(!second_runtime.contains_or_excluded(
        &PopupId::new(first.entity_id(), "popup"),
        first_sample.bounds.center()
    ));
    assert!(!first_runtime.contains_or_excluded(
        &PopupId::new(second.entity_id(), "popup"),
        first_sample.bounds.center()
    ));
    for _ in 0..2 {
        tick(second_window, 1);
    }
    assert_eq!(
        second_window.update(|window, cx| window.simulate_next_frame(cx)),
        0
    );
}

#[gpui_kit::test]
fn dropdown_style_four_pixel_motion_is_preserved_on_both_presence_legs(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.motion = true;
        probe.entrance_distance = 4.;
    });
    assert_eq!(
        snapshot(&view, cx).position().unwrap().bounds.top(),
        px(128.)
    );
    tick(cx, 100);
    assert_eq!(
        snapshot(&view, cx).position().unwrap().bounds.top(),
        px(132.)
    );
    change(&view, cx, |probe| probe.open = false);
    tick(cx, 50);
    let exit = snapshot(&view, cx);
    assert_eq!(exit.phase(), PresencePhase::Exiting);
    assert!(
        exit.position().unwrap().bounds.top() >= px(128.)
            && exit.position().unwrap().bounds.top() < px(132.)
    );
    cx.update(|window, _| {
        assert!(
            !gpui_base::test_support::snapshots(window)
                .iter()
                .any(|node| node.role() == Some(Role::Tooltip))
        )
    });
}

#[gpui_kit::test]
fn custom_candidates_keep_presence_motion_and_tip_gap(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.motion = true;
        probe.entrance_distance = 4.;
        probe.custom_candidates = true;
        probe.arrow = true;
        probe.tip_gap = true;
    });
    let start = snapshot(&view, cx).position().unwrap().bounds;
    tick(cx, 100);
    let rest = snapshot(&view, cx).position().unwrap().bounds;
    assert_eq!(rest.top() - start.top(), px(4.));
    assert_eq!(rest.top(), px(138.), "24px trigger + 8px gap + 6px arrow");
    change(&view, cx, |probe| probe.open = false);
    tick(cx, 50);
    let exit = snapshot(&view, cx);
    assert_eq!(exit.phase(), PresencePhase::Exiting);
    assert!(exit.position().unwrap().bounds.top() < rest.top());
    assert!(exit.position().unwrap().bounds.top() >= start.top());
}

#[gpui_kit::test]
fn resize_refuses_translation_but_reduced_motion_adopts_a_distant_new_size(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, |probe| probe.move_budget = Some(160.));
    change(&view, cx, |probe| probe.popup_width = 160.);
    assert_eq!(snapshot(&view, cx).validity(), PopupValidity::UnsafeMotion);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    change(&view, cx, |probe| {
        probe.x = 500.;
        probe.popup_width = 180.;
    });
    let sample = snapshot(&view, cx).position().unwrap();
    assert_eq!(sample.bounds.left(), px(500.));
    assert_eq!(sample.bounds.size.width, px(180.));
}

#[gpui_kit::test]
fn offset_participates_in_flip_and_oversize_is_not_painted(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.y = 540.;
        probe.gap = 50.;
    });
    cx.update(|window, cx| {
        view.update(cx, |probe, cx| {
            probe.y = f32::from(window.viewport_size().height) - 74.;
            cx.notify();
        })
    });
    draw(cx);
    assert_eq!(
        snapshot(&view, cx).position().unwrap().placement,
        Some(Placement::Top)
    );
    change(&view, cx, |probe| probe.popup_width = 100_000.);
    assert_eq!(snapshot(&view, cx).validity(), PopupValidity::NoSpace);
    assert!(snapshot(&view, cx).position().is_none());
}

#[gpui_kit::test]
fn nested_deferred_survives_cached_reuse_and_moves_in_one_frame(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| probe.nested = true);
    for _ in 0..3 {
        cx.update(|window, cx| {
            let root = window.root::<Root>().unwrap().unwrap();
            root.update(cx, |_, cx| cx.notify());
        });
        draw(cx);
        let child = cx.update(|_, cx| view.read(cx).child_sample.borrow().as_ref().unwrap().read());
        assert_eq!(
            child.position().unwrap().bounds.origin,
            point(px(228.), px(132.))
        );
    }
    change(&view, cx, |probe| probe.x += 40.);
    let child = cx.update(|_, cx| view.read(cx).child_sample.borrow().as_ref().unwrap().read());
    assert_eq!(
        child.position().unwrap().bounds.origin,
        point(px(268.), px(132.))
    );
    let runtime = cx.update(|window, cx| PopupRuntime::for_window(window, cx));
    let id = PopupId::new(view.entity_id(), "popup");
    assert!(runtime.contains_or_excluded(&id, child.position().unwrap().bounds.center()));
    change(&view, cx, |probe| probe.open = false);
    assert!(!runtime.contains_or_excluded(&id, child.position().unwrap().bounds.center()));
}

#[gpui_kit::test]
fn runtime_escape_closes_only_its_top_surface_and_owner_route_stays_with_owner(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, |probe| probe.route = PopupDismissRoute::Runtime);
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|_, cx| {
        let probe = view.read(cx);
        assert_eq!(probe.dismissals, [PopupDismissReason::Escape]);
        assert_eq!(probe.owner_escape, 0);
    });
    change(&view, cx, |probe| {
        probe.open = true;
        probe.route = PopupDismissRoute::Owner;
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|_, cx| {
        let probe = view.read(cx);
        assert_eq!(probe.dismissals.len(), 1);
        assert_eq!(probe.owner_escape, 1);
    });
}

#[gpui_kit::test]
fn runtime_outside_press_excludes_trigger_and_preserves_the_clicked_button(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, |probe| probe.route = PopupDismissRoute::Runtime);
    cx.simulate_click(point(px(120.), px(110.)), Modifiers::default());
    draw(cx);
    assert!(cx.update(|_, cx| view.read(cx).dismissals.is_empty()));
    cx.simulate_click(point(px(10.), px(10.)), Modifiers::default());
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).dismissals, [PopupDismissReason::OutsidePress]));
}

#[gpui_kit::test]
fn zero_progress_closed_surface_does_not_build_content_or_block_pointer(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.open = false;
        probe.pointer = PopupPointerMode::Block;
        probe.motion = true;
    });
    assert_eq!(cx.update(|_, cx| view.read(cx).factories.get()), 0);
    cx.simulate_click(point(px(150.), px(150.)), Modifiers::default());
    draw(cx);
    assert_eq!(cx.update(|_, cx| view.read(cx).clicks.get()), 1);
}

#[gpui_kit::test]
fn translation_uses_current_geometry_and_refuses_far_or_protected_sweeps(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| probe.move_budget = Some(160.));
    let start = snapshot(&view, cx).position().unwrap().bounds;
    change(&view, cx, |probe| probe.x = 180.);
    assert_eq!(snapshot(&view, cx).position().unwrap().bounds, start);
    tick(cx, 75);
    let middle = snapshot(&view, cx).position().unwrap().bounds;
    assert!(middle.left() > start.left() && middle.left() < px(180.));
    tick(cx, 75);
    assert_eq!(
        snapshot(&view, cx).position().unwrap().bounds.left(),
        px(180.)
    );
    change(&view, cx, |probe| probe.x = 500.);
    assert_eq!(snapshot(&view, cx).validity(), PopupValidity::UnsafeMotion);
    assert!(snapshot(&view, cx).position().is_none());
}

#[gpui_kit::test]
fn reduced_motion_uses_target_and_exit_completes_without_interaction(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, |probe| {
        probe.motion = true;
        probe.pointer = PopupPointerMode::Block;
        probe.move_budget = Some(160.);
    });
    tick(cx, 100);
    change(&view, cx, |probe| probe.open = false);
    cx.simulate_click(point(px(150.), px(150.)), Modifiers::default());
    draw(cx);
    assert_eq!(cx.update(|_, cx| view.read(cx).clicks.get()), 1);
    tick(cx, 100);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    change(&view, cx, |probe| {
        probe.open = true;
        probe.x = 180.;
    });
    let sample = snapshot(&view, cx);
    assert_eq!(sample.phase(), PresencePhase::Present);
    assert_eq!(sample.position().unwrap().bounds.left(), px(180.));
}

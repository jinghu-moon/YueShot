use super::*;
use gpui_kit::{FocusHandle, Focusable as _};

struct FocusCards {
    state: Entity<VirtualizerState>,
    input: Entity<yue_component::InputState>,
    fallback: FocusHandle,
    seen: Rc<std::cell::RefCell<Vec<ElementId>>>,
}
impl FocusCards {
    fn new(mode: usize, w: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| yue_component::InputState::new(w, cx));
        let focus = input.read(cx).focus_handle(cx);
        let fallback = cx.focus_handle();
        let state = cx.new(|_| {
            VirtualizerState::new(
                Axis::Vertical,
                source(0, 100),
                match mode {
                    0 => VirtualizerSizing::fixed(px(48.)),
                    1 => VirtualizerSizing::known(vec![px(48.); 100]),
                    _ => VirtualizerSizing::measured(px(48.)),
                },
            )
            .unwrap()
        });
        state.update(cx, |s, _| {
            s.set_focus_handle(0usize.into(), Some(focus)).unwrap();
            s.set_focus_fallback(Some(fallback.clone()));
        });
        Self {
            state,
            input,
            fallback,
            seen: Rc::new(std::cell::RefCell::new(Vec::new())),
        }
    }
}
impl Render for FocusCards {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = self.input.clone();
        let seen = self.seen.clone();
        let theme = yue_component::current_theme(cx);
        div()
            .id("focus-container")
            .track_focus(&self.fallback)
            .child(
                VirtualList::new("focus-cards", &self.state, move |item, _, _| {
                    seen.borrow_mut().push(item.key().clone());
                    if *item.key() == ElementId::from(0usize) {
                        div()
                            .h(px(48.))
                            .child(
                                yue_component::YueInput::new("row-input", theme, &input)
                                    .accessibility_label("Retained editor"),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .h(px(48.))
                            .child(format!("Card {:?}", item.key()))
                            .into_any_element()
                    }
                })
                .w(px(240.))
                .h(px(96.)),
            )
    }
}

#[gpui_kit::test]
fn editor_is_retained_without_intervening_rows_and_deleted_focus_is_repaired(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        yue_component::init(
            cx,
            yue_component::YueTheme::new(
                yue_component::ThemeMode::Light,
                yue_component::Accent::Azure,
            ),
        )
        .unwrap();
    });
    for mode in 0..3 {
        let (view, cx) = cx.add_window_view(move |w, cx| FocusCards::new(mode, w, cx));
        cx.update(|w, cx| {
            view.read(cx).input.clone().update(cx, |input, cx| {
                input.set_value("retained domain value", w, cx);
                input.focus(w, cx);
            })
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let (state, seen) = view.read_with(cx, |v, _| (v.state.clone(), v.seen.clone()));
        state.update(cx, |s, cx| {
            s.scroll_to_key(
                99usize.into(),
                VirtualizerScrollOptions::default().align(VirtualizerAlign::End),
            )
            .unwrap();
            cx.notify();
        });
        seen.borrow_mut().clear();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert!(seen.borrow().contains(&ElementId::from(0usize)));
        assert!(
            seen.borrow().len() <= 6,
            "must not retain all intervening rows: {:?}",
            seen.borrow()
        );
        assert!(cx.update(|w, cx| view.read(cx).input.read(cx).focus_handle(cx).is_focused(w)));
        let input_bounds = cx.update(|w, _| {
            snapshots(w)
                .into_iter()
                .find(|n| n.label() == Some("Retained editor"))
                .unwrap()
                .bounds()
        });
        assert!(
            input_bounds.bottom() <= px(0.),
            "off-screen focus must not cover the last visible row"
        );
        state.update(cx, |s, cx| {
            s.replace_source(
                source(1, 99),
                match mode {
                    0 => VirtualizerSizing::fixed(px(48.)),
                    1 => VirtualizerSizing::known(vec![px(48.); 99]),
                    _ => VirtualizerSizing::measured(px(48.)),
                },
                VirtualizerChange::Splice {
                    old: 0..1,
                    inserted: 0,
                },
            )
            .unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert!(cx.update(|w, cx| view.read(cx).fallback.is_focused(w)));
        assert_eq!(
            view.read_with(cx, |v, cx| v.input.read(cx).value().to_string()),
            "retained domain value"
        );
    }
}

#[test]
fn stale_content_and_nearest_for_an_oversized_item_have_explicit_results() {
    let mut state = fixed(100);
    let old = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
    state
        .set_layout(VirtualizerLayout::default().revision(2))
        .unwrap();
    assert_eq!(
        state.invalidate_items_from_snapshot(&old, 0..1),
        Err(VirtualizerError::StaleSnapshot)
    );
    let mut state = VirtualizerState::new(
        Axis::Vertical,
        source(0, 10),
        VirtualizerSizing::known(vec![px(300.); 10]),
    )
    .unwrap();
    let v = viewport(&state, 1, 50.);
    state.commit(&v).unwrap();
    state
        .scroll_to_key(0usize.into(), VirtualizerScrollOptions::default())
        .unwrap();
    let (_, request) = state.prepare(&v, Instant::now()).unwrap();
    assert_eq!(request.unwrap().offset(), 50.);
}

#[gpui_kit::test]
fn dynamic_attached_follow_tail_splice_and_target_settlement_use_native_geometry(
    cx: &mut TestAppContext,
) {
    let (host, cx) = cx.add_window_view(|_, cx| SharedHost::new(true, false, cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let rows = host.read_with(cx, |h, _| h.rows.clone());
    rows.update(cx, |s, _| s.set_follow_tail(true).unwrap());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(rows.read_with(cx, |s, _| s.is_following_tail()));
    let before = rows.read_with(cx, |s, _| s.viewport().unwrap().offset());
    rows.update(cx, |s, _| {
        s.replace_source(
            source(0, 10_010),
            VirtualizerSizing::measured(px(20.)),
            VirtualizerChange::Splice {
                old: 10_000..10_000,
                inserted: 10,
            },
        )
        .unwrap()
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(rows.read_with(cx, |s, _| s.is_following_tail()));
    assert!(rows.read_with(cx, |s, _| s.viewport().unwrap().offset()) > before);
    rows.update(cx, |s, _| {
        s.scroll_to_key(
            2usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let snapshot = rows.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert!(snapshot.is_estimated());
    assert!(
        !rows.read_with(cx, |s, _| s.has_scroll_command()),
        "target can settle while most off-screen rows still use hints"
    );
}

#[gpui_kit::test]
fn deletion_checks_current_focus_even_before_the_next_virtual_frame(cx: &mut TestAppContext) {
    cx.update(|cx| {
        yue_component::init(
            cx,
            yue_component::YueTheme::new(
                yue_component::ThemeMode::Light,
                yue_component::Accent::Azure,
            ),
        )
        .unwrap()
    });
    let (view, cx) = cx.add_window_view(|w, cx| FocusCards::new(0, w, cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.update(|w, cx| {
        view.read(cx)
            .input
            .clone()
            .update(cx, |input, cx| input.focus(w, cx))
    });
    let state = view.read_with(cx, |v, _| v.state.clone());
    state.update(cx, |s, cx| {
        s.replace_source(
            source(1, 99),
            VirtualizerSizing::fixed(px(48.)),
            VirtualizerChange::Splice {
                old: 0..1,
                inserted: 0,
            },
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(cx.update(|w, cx| view.read(cx).fallback.is_focused(w)));
}

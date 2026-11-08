use super::*;

#[gpui_kit::test]
fn single_keyboard_skips_disabled_and_restores_trigger(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| state.read(cx).focus_handle().clone().focus(window, cx));
    draw(cx);
    key(cx, "down");
    assert_eq!(
        state.read_with(cx, |s, _| s.active_key().cloned()),
        Some("a".into())
    );
    key(cx, "down");
    key(cx, "enter");
    assert_eq!(
        state.read_with(cx, |s, _| s.value().clone()),
        SelectValue::Single(Some("c".into()))
    );
    assert_eq!(change_count(&view, cx), 1);
    cx.update(|window, cx| {
        assert!(!state.read(cx).is_open());
        assert!(state.read(cx).focus_handle().is_focused(window));
    });
}

#[gpui_kit::test]
fn disabled_after_focus_and_readonly_do_not_open(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.read(cx).focus_handle().clone().focus(window, cx);
        state.update(cx, |s, cx| s.set_disabled(true, window, cx));
    });
    draw(cx);
    key(cx, "enter");
    key(cx, "down");
    assert!(!state.read_with(cx, |s, _| s.is_open()));
    assert_eq!(change_count(&view, cx), 0);
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_disabled(false, window, cx);
            s.set_readonly(true, window, cx);
            s.focus_handle().focus(window, cx);
        })
    });
    draw(cx);
    key(cx, "enter");
    assert!(!state.read_with(cx, |s, _| s.is_open()));
}

#[gpui_kit::test]
fn form_reads_typed_selection_and_focuses_first_failure(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let (form, state) = view.read_with(cx, |v, _| (v.form.clone(), v.state.clone()));
    cx.update(|window, cx| {
        assert!(!form.update(cx, |form, cx| form.submit(window, cx)));
        assert!(state.read(cx).focus_handle().is_focused(window));
        state.update(cx, |s, cx| {
            s.set_value(SelectValue::Single(Some("a".into())), window, cx)
                .unwrap()
        });
        assert!(form.read(cx).is_valid(cx));
        assert!(form.read(cx).error("fruit", cx).is_none());
    });
}

#[gpui_kit::test]
fn external_close_and_disable_do_not_leave_hidden_input_focused(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Local, window, cx)
        })
    });
    draw(cx);
    click(cx, "展开选项");
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            assert!(s.content_focus(cx).is_focused(window));
            s.set_open(false, window, cx);
            assert!(s.focus.is_focused(window));
        })
    });
    draw(cx);
    click(cx, "展开选项");
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            let query = s.content_focus(cx);
            s.set_disabled(true, window, cx);
            assert!(!query.is_focused(window));
            assert!(!s.focus.is_focused(window));
        })
    });
}

#[gpui_kit::test]
fn ime_preedit_does_not_filter_or_select_and_unmark_commits_query(cx: &mut TestAppContext) {
    use gpui_kit::EntityInputHandler as _;
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Local, window, cx)
        })
    });
    draw(cx);
    click(cx, "展开选项");
    let input = state.read_with(cx, |s, _| s.query_input().unwrap().clone());
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "cher", None, window, cx)
        })
    });
    draw(cx);
    assert_eq!(state.read_with(cx, |s, _| s.rows.len()), 3);
    key(cx, "enter");
    assert_eq!(change_count(&view, cx), 0);
    cx.update(|window, cx| input.update(cx, |input, cx| input.unmark_text(window, cx)));
    draw(cx);
    assert_eq!(state.read_with(cx, |s, _| s.rows.len()), 1);
    key(cx, "enter");
    assert_eq!(change_count(&view, cx), 0, "first Enter enters results");
    key(cx, "enter");
    assert_eq!(change_count(&view, cx), 1);
}

#[gpui_kit::test]
fn query_tab_leaves_entire_field_and_shift_tab_returns_to_previous(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Multiple);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_value(
                SelectValue::Multiple(vec!["a".into(), "c".into()]),
                window,
                cx,
            )
            .unwrap();
            s.set_search_mode(SelectSearch::Local, window, cx);
        })
    });
    draw(cx);
    click(cx, "展开选项");
    key(cx, "tab");
    assert_eq!(find(cx, "After").focused(), Some(true));
    assert!(!state.read_with(cx, |s, _| s.is_open()));
    click(cx, "展开选项");
    key(cx, "shift-tab");
    assert_eq!(find(cx, "Before").focused(), Some(true));
    assert_eq!(state.read_with(cx, |s, cx| s.query(cx)), "");
    assert_eq!(change_count(&view, cx), 0);
}

#[gpui_kit::test]
fn footer_tab_order_and_exit_policy_use_registered_handles(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let footer_focus = cx.update(|_, cx| cx.focus_handle().tab_stop(false));
    let factory_focus = footer_focus.clone();
    view.update(cx, |v, cx| {
        v.decorate = Some(Rc::new(move |s| {
            let focus = factory_focus.clone();
            s.panel_footer_focus([focus.clone()])
                .panel_footer(move |interactive, _, _| {
                    crate::YueButton::new("footer-command", "Footer command")
                        .track_focus(&focus)
                        .tab_stop(false)
                        .disabled(!interactive)
                })
        }));
        cx.notify();
    });
    draw(cx);
    click(cx, "展开选项");
    key(cx, "tab");
    cx.update(|window, _| assert!(footer_focus.is_focused(window)));
    key(cx, "shift-tab");
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| assert!(state.read(cx).list_focus.is_focused(window)));
    key(cx, "tab");
    key(cx, "tab");
    assert_eq!(find(cx, "After").focused(), Some(true));
}

#[gpui_kit::test]
fn caller_focus_tab_policy_and_same_value_echo_preserve_live_query(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let caller = cx.update(|_, cx| cx.focus_handle());
    let handle = caller.clone();
    view.update(cx, |v, cx| {
        v.decorate = Some(Rc::new(move |s| {
            s.track_focus(&handle).tab_index(4).tab_stop(false)
        }));
        cx.notify();
    });
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Local, window, cx);
            s.set_value(SelectValue::Single(Some("a".into())), window, cx)
                .unwrap();
        })
    });
    draw(cx);
    cx.update(|window, cx| {
        caller.focus(window, cx);
        let policy = window.focused(cx).unwrap();
        assert!(!policy.tab_stop);
        assert_eq!(policy.tab_index, 4);
    });
    click(cx, "展开选项");
    cx.simulate_input("cher");
    draw(cx);
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_value(s.value().clone(), window, cx).unwrap();
            assert!(s.is_open());
            assert_eq!(s.query(cx), "cher");
            assert!(s.content_focus(cx).is_focused(window));
        })
    });
    key(cx, "escape");
    cx.update(|window, _| assert!(caller.is_focused(window)));
    assert_eq!(change_count(&view, cx), 0);
}

#[gpui_kit::test]
fn modal_select_uses_same_scope_and_escape_closes_only_top_surface(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::init(
            cx,
            crate::YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure),
        )
        .unwrap()
    });
    let mut entities = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let dialog = cx.new(crate::YueDialogState::new);
        let select = cx.new(|cx| {
            let mut state = YueSelectState::new(SelectMode::Single, window, cx);
            state
                .set_options([SelectOption::new("a", "Apple")], cx)
                .unwrap();
            state
        });
        entities = Some((dialog.clone(), select.clone()));
        let view = cx.new(|_| ModalProbe { dialog, select });
        gpui_kit::base::Root::new(view, window, cx)
    });
    let (dialog, select) = entities.unwrap();
    draw(cx);
    cx.update(|window, cx| dialog.update(cx, |s, cx| s.open(window, cx).unwrap()));
    draw(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    draw(cx);
    click(cx, "展开选项");
    assert_eq!(find(cx, "Apple").role(), Some(Role::ListBoxOption));
    key(cx, "escape");
    assert!(!select.read_with(cx, |s, _| s.is_open()));
    assert!(dialog.read_with(cx, |s, _| s.is_open()));
    cx.update(|window, cx| {
        select.update(cx, |state, cx| {
            state
                .set_value(SelectValue::Single(Some("a".into())), window, cx)
                .unwrap();
        })
    });
    draw(cx);
    let frame = find(cx, "Modal fruit").bounds();
    assert_complete_focus_ring(cx, frame);
    key(cx, "escape");
    assert!(!dialog.read_with(cx, |s, _| s.is_open()));
}

#[gpui_kit::test]
fn typeahead_cycles_repeated_prefix_and_pointer_release_outside_does_not_commit(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_options(
                [
                    SelectOption::new("a", "Apple"),
                    SelectOption::new("b", "Apricot"),
                    SelectOption::new("c", "Avocado").disabled(true),
                ],
                cx,
            )
            .unwrap();
            s.focus.focus(window, cx);
        })
    });
    draw(cx);
    key(cx, "a");
    assert_eq!(
        state.read_with(cx, |s, _| s.active_key().cloned()),
        Some("a".into())
    );
    key(cx, "a");
    assert_eq!(
        state.read_with(cx, |s, _| s.active_key().cloned()),
        Some("b".into())
    );
    key(cx, "a");
    assert_eq!(
        state.read_with(cx, |s, _| s.active_key().cloned()),
        Some("a".into())
    );
    let position = find(cx, "Apple").bounds().center();
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    let outside = find(cx, "Before").bounds().center();
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: outside,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    draw(cx);
    assert_eq!(change_count(&view, cx), 0);
}

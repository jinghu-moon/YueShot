use super::*;

#[gpui_kit::test]
fn popup_fades_and_moves_on_both_dropdown_motion_legs(cx: &mut TestAppContext) {
    let (_, cx) = open(cx, SelectMode::Single);
    click(cx, "展开选项");
    let start = select_panel(cx).unwrap().bounds();
    tick(cx, 50);
    let middle = select_panel(cx).unwrap().bounds();
    let alpha = panel_alpha(cx);
    tick(cx, 150);
    let rest = select_panel(cx).unwrap().bounds();
    assert!((f32::from(rest.top() - start.top()) - 4.).abs() < 0.01);
    assert!(start.top() < middle.top() && middle.top() < rest.top());
    let theme = crate::YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure);
    let ease_enter = crate::tokens::easing(theme, "--motion-ease-enter");
    assert!(
        (alpha - ease_enter(0.25)).abs() < 0.01,
        "enter alpha: {alpha}"
    );
    assert!((panel_alpha(cx) - 1.).abs() < 0.001);

    key(cx, "escape");
    assert_eq!(select_panel(cx).unwrap().bounds(), rest);
    tick(cx, 50);
    let exiting = select_panel(cx).unwrap().bounds();
    let ease_exit = crate::tokens::easing(theme, "--motion-ease-exit");
    assert!((panel_alpha(cx) - (1. - ease_exit(0.5))).abs() < 0.01);
    assert!(start.top() < exiting.top() && exiting.top() < rest.top());
    cx.update(|window, _| {
        assert!(
            !snapshots(window)
                .iter()
                .any(|node| node.role() == Some(Role::ListBoxOption))
        );
    });
    tick(cx, 50);
    assert!(select_panel(cx).is_none());

    // Reduced motion settles both geometry and opacity immediately.
    cx.update(|_, cx| cx.set_reduce_motion(true));
    click(cx, "展开选项");
    assert_eq!(select_panel(cx).unwrap().bounds(), rest);
    assert!((panel_alpha(cx) - 1.).abs() < 0.001);
    key(cx, "escape");
    assert!(select_panel(cx).is_none());
}

#[gpui_kit::test]
fn pointer_selection_clear_and_no_change_on_external_echo(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    click(cx, "展开选项");
    click(cx, "Apple");
    assert_eq!(change_count(&view, cx), 1);
    cx.update(|window, cx| {
        state.update(cx, |state, cx| {
            state.set_value(state.value().clone(), window, cx).unwrap()
        })
    });
    draw(cx);
    assert_eq!(change_count(&view, cx), 1);
    click(cx, "清空选择");
    assert!(state.read_with(cx, |s, _| s.value().is_empty()));
    assert_eq!(change_count(&view, cx), 2);
}

#[gpui_kit::test]
fn multi_toggle_stays_open_and_locked_clear_is_preserved(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Multiple);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_value(SelectValue::Multiple(vec!["b".into()]), window, cx)
                .unwrap()
        })
    });
    draw(cx);
    click(cx, "展开选项");
    click(cx, "Apple");
    assert!(state.read_with(cx, |s, _| s.is_open()));
    assert_eq!(change_count(&view, cx), 1);
    key(cx, "escape");
    click(cx, "清除可移除的选择");
    assert_eq!(
        state.read_with(cx, |s, _| s.value().clone()),
        SelectValue::Multiple(vec!["b".into()])
    );
    assert_eq!(change_count(&view, cx), 2);
}

#[gpui_kit::test]
fn query_is_existing_yue_input_and_clear_filters_once(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Local, window, cx)
        })
    });
    draw(cx);
    click(cx, "展开选项");
    assert_eq!(find(cx, "搜索选项").role(), Some(Role::SearchInput));
    cx.simulate_input("cher");
    draw(cx);
    assert_eq!(state.read_with(cx, |s, cx| s.query(cx)), "cher");
    assert_eq!(find(cx, "Fruit选项").description(), Some("共 1 个选项"));
    assert_eq!(
        state.read_with(cx, |s, _| s
            .rows
            .iter()
            .filter(|row| matches!(row, model::Row::Option(_)))
            .count()),
        1
    );
    click(cx, "Clear input");
    assert_eq!(state.read_with(cx, |s, cx| s.query(cx)), "");
    assert_eq!(state.read_with(cx, |s, _| s.rows.len()), 3);
    assert_eq!(find(cx, "Fruit选项").description(), Some("共 3 个选项"));
    assert_eq!(change_count(&view, cx), 0);
}

#[gpui_kit::test]
fn collapsed_tags_remove_without_opening_main_popup_and_escape_restores_count(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, SelectMode::Multiple);
    let state = view.read_with(cx, |v, _| v.state.clone());
    view.update(cx, |v, cx| {
        v.decorate = Some(Rc::new(|s| s.collapse_count(0)));
        cx.notify();
    });
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_value(
                SelectValue::Multiple(vec!["a".into(), "b".into(), "c".into()]),
                window,
                cx,
            )
            .unwrap();
        })
    });
    draw(cx);
    click(cx, "查看其余 3 个选择");
    click(cx, "移除 Apple");
    assert!(!state.read_with(cx, |s, _| s.is_open()));
    assert_eq!(change_count(&view, cx), 1);
    draw(cx);
    key(cx, "escape");
    assert_eq!(find(cx, "查看其余 2 个选择").focused(), Some(true));
    assert!(!state.read_with(cx, |s, _| s.collapsed_open));
}

#[gpui_kit::test]
fn outside_click_keeps_external_focus_and_escape_reports_cancel(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    click(cx, "展开选项");
    key(cx, "down");
    key(cx, "escape");
    assert_eq!(change_count(&view, cx), 0);
    view.read_with(cx, |v, _| assert!(v.events.iter().any(|e| matches!(e, SelectEvent::OpenChange(change) if !change.is_open() && change.reason() == SelectCloseReason::Escape))));
    draw(cx);
    click(cx, "展开选项");
    click(cx, "Before");
    assert_eq!(find(cx, "Before").focused(), Some(true));
    assert!(!view.read_with(cx, |v, cx| v.state.read(cx).is_open()));
}

#[gpui_kit::test]
fn selection_limit_is_atomic_and_custom_filter_keeps_group_identity(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Multiple);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_options(
                [SelectGroup::new(
                    "one",
                    "Group",
                    [
                        SelectOption::new("a", "Same"),
                        SelectOption::new("c", "Same"),
                    ],
                )],
                cx,
            )
            .unwrap();
            s.set_value(
                SelectValue::Multiple(vec!["a".into(), "c".into()]),
                window,
                cx,
            )
            .unwrap();
            assert_eq!(
                s.set_max_selected(Some(1), cx),
                Err(SelectError::SelectionLimit)
            );
            assert_eq!(s.value().keys().len(), 2);
            s.set_search_mode(SelectSearch::Local, window, cx);
            s.set_filter(|option, query| option.key().as_str() == query, cx);
        })
    });
    draw(cx);
    click(cx, "展开选项");
    cx.simulate_input("c");
    draw(cx);
    state.read_with(cx, |s, _| {
        assert_eq!(s.rows.len(), 2);
        assert!(matches!(&s.rows[0], model::Row::Group(key, _) if key == "one"));
        assert_eq!(s.active_key(), Some(&SharedString::from("c")));
    });
}

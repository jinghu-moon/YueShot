use super::*;

#[gpui_kit::test]
fn remote_tickets_reject_other_owner_close_reopen_and_old_error(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Remote, window, cx);
            s.set_open(true, window, cx);
        })
    });
    draw(cx);
    let ticket = view.read_with(cx, |v, _| {
        v.events
            .iter()
            .find_map(|e| {
                if let SelectEvent::SearchRequested(r) = e {
                    Some(r.ticket().clone())
                } else {
                    None
                }
            })
            .unwrap()
    });
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_open(false, window, cx);
            assert_eq!(
                s.result_state(),
                &SelectResultState::Ready,
                "closing cancels loading visuals as well as tickets"
            );
            s.set_open(true, window, cx);
            assert_eq!(
                s.apply_page(
                    &ticket,
                    SelectPage::new([SelectOption::new("old", "Old")]),
                    cx
                )
                .unwrap(),
                SelectApply::Stale
            );
            assert_eq!(
                s.reject_request(&ticket, "late error", cx),
                SelectApply::Stale
            );
        })
    });
}

#[gpui_kit::test]
fn invalid_remote_page_is_atomic_and_retryable(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Remote, window, cx);
            s.set_open(true, window, cx);
        })
    });
    draw(cx);
    let ticket = last_request(&view, cx);
    cx.update(|_, cx| {
        state.update(cx, |s, cx| {
            assert!(matches!(
                s.apply_page(
                    &ticket,
                    SelectPage::new([
                        SelectOption::new("d", "D"),
                        SelectOption::new("d", "Duplicate")
                    ]),
                    cx
                ),
                Err(SelectError::DuplicateKey(_))
            ));
            assert_eq!(s.options().len(), 3);
            assert!(matches!(s.result_state(), SelectResultState::Failed(_)));
            s.retry_request(cx);
            assert!(matches!(s.result_state(), SelectResultState::Loading));
            assert_eq!(s.reject_request(&ticket, "late", cx), SelectApply::Stale);
        })
    });
}

#[gpui_kit::test]
fn remote_pagination_rejects_no_progress_and_duplicate_keys(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Remote, window, cx);
            s.set_open(true, window, cx);
        })
    });
    draw(cx);
    let ticket = last_request(&view, cx);
    cx.update(|_, cx| {
        state.update(cx, |s, cx| {
            s.apply_page(
                &ticket,
                SelectPage::new([SelectOption::new("x", "X")]).next_cursor("next"),
                cx,
            )
            .unwrap();
            s.request_more(cx);
        })
    });
    draw(cx);
    let more = last_request(&view, cx);
    cx.update(|_, cx| {
        state.update(cx, |s, cx| {
            assert_eq!(
                s.apply_page(
                    &more,
                    SelectPage::new([SelectOption::new("y", "Y")]).next_cursor("next"),
                    cx
                ),
                Err(SelectError::NoProgress)
            );
            assert_eq!(s.options().len(), 1);
            s.retry_request(cx);
        })
    });
    draw(cx);
    let retry = last_request(&view, cx);
    cx.update(|_, cx| {
        state.update(cx, |s, cx| {
            assert!(matches!(
                s.apply_page(
                    &retry,
                    SelectPage::new([SelectOption::new("x", "Duplicate")]),
                    cx
                ),
                Err(SelectError::DuplicateKey(_))
            ));
            assert!(matches!(s.more_result, SelectResultState::Failed(_)));
            assert_eq!(s.options().len(), 1);
        })
    });
}

#[gpui_kit::test]
fn unknown_selection_hydrates_without_changing_identity_or_emitting(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Multiple);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_value(SelectValue::Multiple(vec!["missing".into()]), window, cx)
                .unwrap();
            assert_eq!(s.selected_items()[0].label(), "missing");
            s.set_selected_items(
                [SelectOption::new("missing", "Hydrated").disabled(true)],
                cx,
            )
            .unwrap();
            s.set_options([SelectOption::new("other", "Other")], cx)
                .unwrap();
            assert_eq!(s.selected_items()[0].label(), "Hydrated");
            s.clear(window, cx);
            assert_eq!(s.value().keys(), &[SharedString::from("missing")]);
        })
    });
    assert_eq!(change_count(&view, cx), 0);
}

#[gpui_kit::test]
fn unmount_cancels_session_even_when_state_is_retained(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Remote, window, cx);
            s.set_open(true, window, cx);
        })
    });
    draw(cx);
    let ticket = last_request(&view, cx);
    view.update(cx, |v, cx| {
        v.visible = false;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    draw(cx);
    cx.update(|_, cx| {
        state.update(cx, |s, cx| {
            assert!(!s.is_open());
            assert_eq!(
                s.apply_page(
                    &ticket,
                    SelectPage::new([SelectOption::new("late", "Late")]),
                    cx
                )
                .unwrap(),
                SelectApply::Stale
            );
        })
    });
}

#[gpui_kit::test]
fn create_requests_validate_and_stale_responses_cannot_replace_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Multiple);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Local, window, cx);
            s.set_creatable(true, cx);
        })
    });
    draw(cx);
    click(cx, "展开选项");
    cx.simulate_input("New fruit");
    draw(cx);
    key(cx, "enter");
    key(cx, "enter");
    let ticket = last_request(&view, cx);
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            assert_eq!(
                s.resolve_create(
                    &ticket,
                    SelectOption::new("new", "New fruit").disabled(true),
                    window,
                    cx
                ),
                Err(SelectError::DisabledOption)
            );
            assert!(matches!(s.create_result, SelectResultState::Failed(_)));
            s.retry_request(cx);
        })
    });
    draw(cx);
    let retry = last_request(&view, cx);
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.resolve_create(&retry, SelectOption::new("new", "New fruit"), window, cx)
                .unwrap();
            assert!(s.value().contains("new"));
            assert!(s.is_open());
        })
    });
    draw(cx);
    assert_eq!(change_count(&view, cx), 1);
    key(cx, "ctrl-f");
    click(cx, "Clear input");
    cx.simulate_input("Another");
    draw(cx);
    key(cx, "enter");
    key(cx, "enter");
    let pending = last_request(&view, cx);
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_value(SelectValue::Multiple(vec!["a".into()]), window, cx)
                .unwrap();
            assert_eq!(
                s.resolve_create(&pending, SelectOption::new("late", "Late"), window, cx)
                    .unwrap(),
                SelectApply::Stale
            );
        })
    });
    assert_eq!(change_count(&view, cx), 1);
}

#[gpui_kit::test]
fn remote_query_generation_debounces_and_rejects_other_owner(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Remote, window, cx)
        })
    });
    draw(cx);
    click(cx, "展开选项");
    let initial = last_request(&view, cx);
    cx.simulate_input("a");
    draw(cx);
    cx.simulate_input("b");
    draw(cx);
    cx.update(|window, cx| {
        let other = cx.new(|cx| YueSelectState::new(SelectMode::Single, window, cx));
        other.update(cx, |s, cx| {
            s.set_search_mode(SelectSearch::Remote, window, cx);
            s.set_open(true, window, cx);
            assert_eq!(
                s.apply_page(
                    &initial,
                    SelectPage::new([SelectOption::new("wrong", "Wrong owner")]),
                    cx
                )
                .unwrap(),
                SelectApply::Stale
            );
        });
        state.update(cx, |s, cx| {
            assert_eq!(
                s.apply_page(
                    &initial,
                    SelectPage::new([SelectOption::new("old", "Old query")]),
                    cx
                )
                .unwrap(),
                SelectApply::Stale
            )
        });
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(220));
    draw(cx);
    view.read_with(cx, |v, _| {
        let searches = v
            .events
            .iter()
            .filter_map(|event| {
                if let SelectEvent::SearchRequested(request) = event {
                    Some(request.query().as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(searches, ["", "ab"]);
    });
}

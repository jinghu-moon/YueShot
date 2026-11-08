use super::*;

#[gpui_kit::test]
fn a_right_edge_menu_keeps_its_trigger_alignment_without_reserving_shadow_space(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, SelectMode::Single);
    view.update(cx, |view, cx| {
        view.near_right_edge = true;
        view.width = px(120.);
        cx.notify();
    });
    draw(cx);
    let trigger = find(cx, "Fruit").bounds();
    click(cx, "展开选项");
    tick(cx, 250);
    let panel = select_panel(cx).unwrap().bounds();
    assert!(
        (panel.left() - trigger.left()).abs() < px(1.),
        "{panel:?} vs {trigger:?}"
    );
    assert!((panel.right() - trigger.right()).abs() < px(1.));
}

#[gpui_kit::test]
fn plain_options_start_at_the_row_padding_and_mixed_icons_keep_one_text_column(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    for virtualization in [SelectVirtualization::Never, SelectVirtualization::Always] {
        for size in [SelectSize::Small, SelectSize::Medium, SelectSize::Large] {
            for with_icon in [false, true] {
                view.update(cx, |v, cx| {
                    v.size = size;
                    v.width = px(104.);
                    v.decorate = Some(Rc::new(move |s| s.virtualization(virtualization)));
                    cx.notify();
                });
                cx.update(|_, cx| {
                    state.update(cx, |s, cx| {
                        let mut png = SelectOption::new("png", ".PNG");
                        if with_icon {
                            png = png.leading_icon(crate::outline::Photo);
                        }
                        s.set_options([png, SelectOption::new("jpg", ".JPG")], cx)
                            .unwrap();
                    });
                });
                draw(cx);
                click(cx, "展开选项");
                tick(cx, 250);
                for (key, label) in [("png", ".PNG"), ("jpg", ".JPG")] {
                    let row = find(cx, label).bounds();
                    cx.update(|window, cx| {
                        let id: gpui_kit::ElementId = (gpui_kit::ElementId::from("fruit"), format!("option:{key}")).into();
                        let text_id: gpui_kit::ElementId = (id, "text").into();
                        let text = snapshots(window).into_iter().find(|n| n.path().last() == Some(&text_id)).unwrap().bounds();
                        let theme = crate::current_theme(cx);
                        let padding = super::super::layout::metric(theme, &format!("--input-padding-inline-{}", size.suffix()), window);
                        let icon_space = if with_icon {
                            super::super::layout::metric(theme, &format!("--input-icon-size-{}", size.suffix()), window)
                                + super::super::layout::metric(theme, "--input-gap", window)
                        } else { px(0.) };
                        assert!((text.left() - row.left() - padding - icon_space).abs() < px(0.01), "{size:?}/{virtualization:?}/icons={with_icon}: text={text:?}, row={row:?}");
                    });
                }
                key(cx, "escape");
                tick(cx, 250);
            }
        }
    }
}

#[gpui_kit::test]
fn focus_ring_surrounds_the_whole_trigger_without_changing_layout(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |state, cx| {
            state
                .set_value(SelectValue::Single(Some("a".into())), window, cx)
                .unwrap();
        })
    });
    for (size, width) in [
        (SelectSize::Small, 180.),
        (SelectSize::Medium, 240.),
        (SelectSize::Large, 320.),
    ] {
        view.update(cx, |v, cx| {
            v.size = size;
            v.width = px(width);
            cx.notify();
        });
        cx.update(|window, cx| window.blur(cx));
        draw(cx);
        let before = find(cx, "Fruit").bounds();
        cx.update(|window, cx| state.read(cx).focus_handle().clone().focus(window, cx));
        draw(cx);
        assert_eq!(find(cx, "Fruit").bounds(), before);
        assert_complete_focus_ring(cx, before);
    }
}

#[gpui_kit::test]
fn width_and_sizes_stay_independent_of_parent(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    for (size, height) in [
        (SelectSize::Small, 28.),
        (SelectSize::Medium, 32.),
        (SelectSize::Large, 40.),
    ] {
        view.update(cx, |v, cx| {
            v.size = size;
            cx.notify();
        });
        draw(cx);
        let frame = find(cx, "Fruit");
        assert!((f32::from(frame.bounds().size.width) - 240.).abs() < 1.);
        assert!((f32::from(frame.bounds().size.height) - height).abs() < 1.);
    }
}

#[gpui_kit::test]
fn filtering_and_replacing_scrolled_options_keeps_active_row_visible(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_options(
                (0..10_000).map(|i| SelectOption::new(format!("k{i}"), format!("Item {i}"))),
                cx,
            )
            .unwrap();
            s.set_search_mode(SelectSearch::Local, window, cx);
        })
    });
    draw(cx);
    click(cx, "展开选项");
    key(cx, "down");
    key(cx, "end");
    draw(cx);
    find(cx, "Item 9999");
    click(cx, "搜索选项");
    cx.simulate_input("Item 1");
    draw(cx);
    draw(cx);
    assert_eq!(
        state.read_with(cx, |s, _| s.active_key().cloned()),
        Some("k1".into())
    );
    let first = find(cx, "Item 1");
    let viewport = find(cx, "Fruit选项");
    assert!(viewport.bounds().contains(&first.bounds().center()));

    // Keep the active identity but move it to the end of a replacement model.
    cx.update(|_, cx| {
        state.update(cx, |s, cx| {
            s.set_options(
                (100..1200)
                    .map(|i| SelectOption::new(format!("k{i}"), format!("Item {i}")))
                    .chain([SelectOption::new("k1", "Item 1")]),
                cx,
            )
            .unwrap();
        })
    });
    draw(cx);
    draw(cx);
    let moved = find(cx, "Item 1");
    let viewport = find(cx, "Fruit选项");
    assert!(viewport.bounds().contains(&moved.bounds().center()));
}

#[gpui_kit::test]
fn ten_thousand_options_virtualize_and_end_selects_last_model_key(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    cx.update(|_, cx| {
        state.update(cx, |s, cx| {
            s.set_options(
                (0..10_000).map(|i| SelectOption::new(format!("k{i}"), format!("Item {i}"))),
                cx,
            )
            .unwrap();
        })
    });
    draw(cx);
    click(cx, "展开选项");
    cx.update(|window, _| {
        let count = snapshots(window)
            .iter()
            .filter(|s| s.role() == Some(Role::ListBoxOption))
            .count();
        assert!(count > 0 && count < 40, "bounded mounted rows: {count}");
    });
    key(cx, "end");
    draw(cx);
    assert_eq!(
        state.read_with(cx, |s, _| s.active_key().cloned()),
        Some("k9999".into())
    );
    let last = find(cx, "Item 9999");
    assert!(last.bounds().size.height > px(0.));
    key(cx, "enter");
    assert_eq!(
        state.read_with(cx, |s, _| s.value().clone()),
        SelectValue::Single(Some("k9999".into()))
    );
}

#[gpui_kit::test]
fn page_keys_use_visible_rows_and_never_mode_mounts_all_options(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    view.update(cx, |v, cx| {
        v.decorate = Some(Rc::new(|s| {
            s.max_panel_height(px(96.))
                .virtualization(SelectVirtualization::Never)
        }));
        cx.notify();
    });
    cx.update(|_, cx| {
        state.update(cx, |s, cx| {
            s.set_options(
                (0..25).map(|i| SelectOption::new(format!("k{i}"), format!("Item {i}"))),
                cx,
            )
            .unwrap();
        })
    });
    draw(cx);
    click(cx, "展开选项");
    draw(cx);
    assert_eq!(state.read_with(cx, |s, _| s.page_size), 3);
    key(cx, "pagedown");
    assert_eq!(
        state.read_with(cx, |s, _| s.active_key().cloned()),
        Some("k3".into())
    );
    cx.update(|window, _| {
        assert_eq!(
            snapshots(window)
                .iter()
                .filter(|s| s.role() == Some(Role::ListBoxOption))
                .count(),
            25
        )
    });
    key(cx, "pageup");
    assert_eq!(
        state.read_with(cx, |s, _| s.active_key().cloned()),
        Some("k0".into())
    );
}

#[gpui_kit::test]
fn custom_option_content_is_vertically_centered_in_each_row(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    for virtualization in [SelectVirtualization::Never, SelectVirtualization::Always] {
        for row_layout in [SelectRowLayout::SingleLine, SelectRowLayout::Description] {
            for size in [SelectSize::Small, SelectSize::Medium, SelectSize::Large] {
                view.update(cx, |v, cx| {
                    v.size = size;
                    v.decorate = Some(Rc::new(move |select| {
                        select
                            .row_layout(row_layout)
                            .virtualization(virtualization)
                            .render_option(|row, _, _| {
                                crate::YueTag::new(
                                    format!("custom:{}", row.option().key()),
                                    format!("Tag {}", row.option().label()),
                                )
                                .size(match row.size() {
                                    SelectSize::Small => crate::TagSize::Small,
                                    SelectSize::Medium => crate::TagSize::Medium,
                                    SelectSize::Large => crate::TagSize::Large,
                                })
                            })
                    }));
                    cx.notify();
                });
                draw(cx);
                click(cx, "展开选项");
                tick(cx, 250);
                for label in ["Apple", "Banana", "Cherry"] {
                    let row = find(cx, label).bounds();
                    let tag = find(cx, &format!("Tag {label}")).bounds();
                    assert!(
                        f32::from(row.center().y - tag.center().y).abs() < 0.01,
                        "{size:?} / {virtualization:?} / {row_layout:?}: custom tag {tag:?} centered in {row:?}"
                    );
                    assert!(tag.top() >= row.top() && tag.bottom() <= row.bottom());
                }
                let before = state.read_with(cx, |state, _| state.value().clone());
                click(cx, "Tag Banana");
                assert_eq!(
                    state.read_with(cx, |state, _| state.value().clone()),
                    before
                );
                assert!(state.read_with(cx, |state, _| state.is_open()));
                click(cx, "Tag Cherry");
                assert_eq!(
                    state.read_with(cx, |state, _| state.value().clone()),
                    SelectValue::Single(Some("c".into()))
                );
                assert!(!state.read_with(cx, |state, _| state.is_open()));
                tick(cx, 150);
            }
        }
    }
}

#[gpui_kit::test]
fn all_themes_preserve_roles_and_layout_cache_on_hover(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, SelectMode::Single);
    let state = view.read_with(cx, |v, _| v.state.clone());
    for mode in [crate::ThemeMode::Light, crate::ThemeMode::Dark] {
        for accent in [crate::Accent::Azure, crate::Accent::Neutral] {
            cx.update(|_, cx| crate::set_theme(cx, crate::YueTheme::new(mode, accent)));
            draw(cx);
            click(cx, "展开选项");
            assert_eq!(find(cx, "Fruit").role(), Some(Role::ComboBox));
            assert_eq!(find(cx, "Banana").disabled(), Some(true));
            let geometry = state.read_with(cx, |s, _| s.list_cache.clone().unwrap());
            draw(cx);
            assert!(state.read_with(cx, |s, _| Rc::ptr_eq(
                &geometry,
                s.list_cache.as_ref().unwrap()
            )));
            key(cx, "escape");
            draw(cx);
        }
    }
}

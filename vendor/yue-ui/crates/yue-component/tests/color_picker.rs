//! Public-contract interaction tests. GPU color pixels, OS clipboard and Windows
//! screen-reader output still require native-window checks.
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, Modifiers,
    ParentElement as _, Render, Role, SharedString, Styled as _, Subscription, TestAppContext,
    VisualTestContext, Window, div, px,
};
use yue_component::{
    Accent, ColorFormat, ColorPickerEvent, ColorPickerSize, ColorPickerTrigger, ThemeMode,
    YueButton, YueColorPicker, YueColorPickerPanel, YueColorPickerState, YueSwatchGroup,
    YueSwatchOption, YueTheme,
};
struct Probe {
    state: Entity<YueColorPickerState>,
    events: Vec<ColorPickerEvent>,
    disabled: bool,
    panel: bool,
    size: ColorPickerSize,
    compact: bool,
    background: SharedString,
    accept: bool,
    background_width: gpui_kit::Pixels,
    width: Option<gpui_kit::DefiniteLength>,
    panel_width: Option<gpui_kit::Pixels>,
    selection_changes: usize,
    form_submits: usize,
    before: FocusHandle,
    after: FocusHandle,
    _subscription: Subscription,
}
fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}
impl Probe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| {
            YueColorPickerState::new(window, cx).default_value(gpui_kit::rgba(0x3366CC80))
        });
        let sub = cx.subscribe(&state, |this, _, event, cx| {
            this.events.push(*event);
            cx.notify();
        });
        Self {
            state,
            events: vec![],
            disabled: false,
            panel: false,
            size: ColorPickerSize::Medium,
            compact: false,
            background: "white".into(),
            accept: true,
            background_width: px(160.),
            width: None,
            panel_width: None,
            selection_changes: 0,
            form_submits: 0,
            before: cx.focus_handle(),
            after: cx.focus_handle(),
            _subscription: sub,
        }
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = if self.panel {
            let mut panel = YueColorPickerPanel::new("picker", theme(), &self.state)
                .label("填充")
                .presets([gpui_kit::rgb(0xFF0000), gpui_kit::rgba(0x3366CC80)])
                .disabled(self.disabled);
            if let Some(width) = self.panel_width {
                panel = panel.width(width);
            }
            panel.into_any_element()
        } else {
            let mut picker = YueColorPicker::new("picker", theme(), &self.state)
                .label("填充")
                .size(self.size)
                .trigger(if self.compact {
                    ColorPickerTrigger::Swatch
                } else {
                    ColorPickerTrigger::Field
                })
                .presets([gpui_kit::rgb(0xFF0000), gpui_kit::rgba(0x3366CC80)])
                .disabled(self.disabled);
            if let Some(width) = self.width {
                picker = picker.width(width);
            }
            picker.into_any_element()
        };
        div()
            .on_action(
                cx.listener(|this, _: &gpui_kit::base::input::Enter, _, cx| {
                    this.form_submits += 1;
                    cx.notify();
                }),
            )
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .p_4()
            .size_full()
            .child(YueButton::text("before", theme(), "前一个").track_focus(&self.before))
            .child(div().w(px(400.)).child(editor))
            .child(YueButton::text("after", theme(), "后一个").track_focus(&self.after))
            .child(
                YueSwatchGroup::new("background", theme(), self.background.clone())
                    .label("背景选项")
                    .width(self.background_width)
                    .options([
                        YueSwatchOption::solid("white", "白色", gpui_kit::rgb(0xFFFFFF)),
                        YueSwatchOption::solid("red", "红色", gpui_kit::rgb(0xFF0000)),
                        YueSwatchOption::solid("disabled", "不可选", gpui_kit::rgb(0))
                            .disabled(true),
                        YueSwatchOption::checkerboard(
                            "checks",
                            "棋盘格",
                            gpui_kit::rgb(0xFFFFFF),
                            gpui_kit::rgb(0x888888),
                        ),
                    ])
                    .custom_color("custom", "自定义颜色", &self.state)
                    .on_change(cx.listener(|this, value: &SharedString, _, cx| {
                        this.selection_changes += 1;
                        if this.accept {
                            this.background = value.clone();
                            cx.notify();
                        }
                    })),
            )
            .child(YueButton::text("tail", theme(), "色块组之后"))
    }
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
}
fn open(cx: &mut TestAppContext) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let mut probe = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Probe::new(window, cx));
        probe = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    draw(cx);
    (probe.unwrap(), cx)
}
fn find(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|w, _| {
        let found = snapshots(w)
            .into_iter()
            .filter(|n| n.label() == Some(label))
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "one node for {label}: {found:?}");
        found[0].clone()
    })
}
fn click(cx: &mut VisualTestContext, label: &str) {
    let p = find(cx, label).bounds().center();
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
}
fn key(cx: &mut VisualTestContext, k: &str) {
    let k = gpui_kit::Keystroke::parse(k).unwrap();
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: k.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke: k });
    draw(cx);
}
fn is_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|w, _| {
        snapshots(w)
            .iter()
            .any(|n| n.role() == Some(Role::Dialog) && n.label() == Some("填充颜色编辑器"))
    })
}
#[gpui_kit::test]
fn default_width_is_not_the_parent_width_and_open_is_a_dialog(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    assert!((f32::from(find(cx, "填充").bounds().size.width) - 160.).abs() < 1.);
    click(cx, "填充");
    assert!(is_open(cx));
    assert_eq!(find(cx, "饱和度和明度").role(), Some(Role::Group));
    assert_eq!(find(cx, "透明度").role(), Some(Role::Slider));
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
}
#[gpui_kit::test]
fn external_sync_and_format_switch_do_not_emit_color_events(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let state = v.read_with(cx, |v, _| v.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.set_value(gpui_kit::rgba(0xFFAA0080), window, cx);
            s.set_format(ColorFormat::Hsl, window, cx);
        })
    });
    draw(cx);
    click(cx, "填充");
    assert_eq!(find(cx, "透明度 A").value(), Some("50.2"));
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
}
#[gpui_kit::test]
fn pointer_preset_emits_change_then_commit_and_adds_one_history(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    click(cx, "#FF0000");
    assert!(is_open(cx));
    v.read_with(cx, |v, cx| {
        assert_eq!(
            v.events,
            vec![
                ColorPickerEvent::Change(gpui_kit::rgb(0xFF0000)),
                ColorPickerEvent::Commit(gpui_kit::rgb(0xFF0000))
            ]
        );
        assert_eq!(v.state.read(cx).recent(), &[gpui_kit::rgb(0xFF0000)]);
    });
}
#[gpui_kit::test]
fn escape_closes_and_restores_trigger_without_rolling_back(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "填充");
    key(cx, "escape");
    assert!(!is_open(cx));
    assert_eq!(find(cx, "填充").focused(), Some(true));
}
#[gpui_kit::test]
fn disabling_an_open_focused_editor_blocks_keys_and_closes(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    v.update(cx, |v, cx| {
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    key(cx, "right");
    key(cx, "space");
    assert!(!is_open(cx));
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
}
#[gpui_kit::test]
fn actual_tab_crosses_inputs_without_inserting_indentation(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "填充");
    assert_eq!(find(cx, "饱和度和明度").focused(), Some(true));
    key(cx, "tab");
    key(cx, "tab");
    key(cx, "tab");
    assert_eq!(find(cx, "颜色格式：HEX").focused(), Some(true));
    key(cx, "tab");
    assert_eq!(find(cx, "HEX").value(), Some("#3366CC80"));
    key(cx, "tab");
    assert_eq!(find(cx, "透明度 A").focused(), Some(true));
    key(cx, "shift-tab");
    assert_eq!(find(cx, "HEX").focused(), Some(true));
}
#[gpui_kit::test]
fn keyboard_color_edit_is_one_commit_and_respects_modified_keys(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    key(cx, "ctrl-right");
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
    key(cx, "left");
    v.read_with(cx, |v, cx| {
        assert_eq!(v.events.len(), 2);
        assert!(matches!(v.events[0], ColorPickerEvent::Change(_)));
        assert!(matches!(v.events[1], ColorPickerEvent::Commit(_)));
        assert_eq!(v.state.read(cx).recent().len(), 1);
    });
}
#[gpui_kit::test]
fn swatch_arrows_move_focus_without_selecting_and_skip_disabled(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "红色");
    assert_eq!(v.read_with(cx, |v, _| v.background.to_string()), "red");
    key(cx, "right");
    assert_eq!(find(cx, "棋盘格").focused(), Some(true));
    assert_eq!(v.read_with(cx, |v, _| v.background.to_string()), "red");
    key(cx, "enter");
    assert_eq!(v.read_with(cx, |v, _| v.background.to_string()), "checks");
}
#[gpui_kit::test]
fn embedded_panel_has_no_dialog_and_keeps_external_keyboard_flow(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.panel = true;
        cx.notify();
    });
    draw(cx);
    assert!(!is_open(cx));
    assert_eq!(find(cx, "填充调色面板").role(), Some(Role::Group));
}

fn replace_text(cx: &mut VisualTestContext, label: &str, text: &str) {
    click(cx, label);
    key(cx, "ctrl-a");
    cx.simulate_input(text);
    draw(cx);
}
#[gpui_kit::test]
fn alpha_and_hex_edit_each_other_without_losing_rgb(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    replace_text(cx, "透明度 A", "0");
    key(cx, "enter");
    assert_eq!(find(cx, "HEX").value(), Some("#3366CC00"));
    replace_text(cx, "透明度 A", "100");
    key(cx, "enter");
    assert_eq!(find(cx, "HEX").value(), Some("#3366CC"));
    replace_text(cx, "HEX", "#ABC8");
    key(cx, "enter");
    assert_eq!(find(cx, "透明度 A").value(), Some("53.33"));
    replace_text(cx, "HEX", "#aabbcc");
    key(cx, "enter");
    assert_eq!(find(cx, "透明度 A").value(), Some("100"));
    v.read_with(cx, |v, cx| {
        assert_eq!(v.state.read(cx).value(), gpui_kit::rgb(0xAABBCC))
    });
}
#[gpui_kit::test]
fn invalid_draft_never_changes_color_or_copies_and_escape_discards_it(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    replace_text(cx, "HEX", "#zzzzzz");
    assert_eq!(find(cx, "复制颜色").disabled(), Some(true));
    key(cx, "enter");
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
    key(cx, "escape");
    click(cx, "填充");
    assert_eq!(find(cx, "HEX").value(), Some("#3366CC80"));
    assert_ne!(find(cx, "复制颜色").disabled(), Some(true));
}
#[gpui_kit::test]
fn nested_format_menu_escape_and_selection_keep_parent_open(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    click(cx, "颜色格式：HEX");
    assert_eq!(find(cx, "颜色格式").role(), Some(Role::Menu));
    key(cx, "escape");
    assert!(is_open(cx));
    assert_eq!(find(cx, "颜色格式：HEX").focused(), Some(true));
    click(cx, "颜色格式：HEX");
    click(cx, "RGB");
    assert!(is_open(cx));
    assert_eq!(find(cx, "红 R").value(), Some("51"));
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
    key(cx, "escape");
    assert!(!is_open(cx));
}
#[gpui_kit::test]
fn swatch_controlled_rejection_does_not_change_selection(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.accept = false;
        cx.notify();
    });
    draw(cx);
    click(cx, "红色");
    key(cx, "enter");
    assert_eq!(v.read_with(cx, |v, _| v.background.to_string()), "white");
    assert_eq!(find(cx, "白色").selected(), Some(true));
}
#[gpui_kit::test]
fn custom_same_color_confirmation_uses_current_selection_not_first_render(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "自定义颜色");
    click(cx, "HEX");
    key(cx, "enter");
    assert_eq!(v.read_with(cx, |v, _| v.background.to_string()), "custom");
    key(cx, "escape");
    click(cx, "红色");
    click(cx, "自定义颜色");
    click(cx, "HEX");
    key(cx, "enter");
    assert_eq!(v.read_with(cx, |v, _| v.background.to_string()), "custom");
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
}
#[gpui_kit::test]
fn tab_exits_popup_and_returns_to_host_in_both_directions(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "填充");
    key(cx, "shift-tab");
    assert!(!is_open(cx));
    assert_eq!(find(cx, "填充").focused(), Some(true));
    click(cx, "填充");
    for _ in 0..8 {
        key(cx, "tab");
    }
    assert!(!is_open(cx));
    assert_eq!(find(cx, "后一个").focused(), Some(true));
}
#[gpui_kit::test]
fn outside_click_preserves_the_external_focus(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "填充");
    click(cx, "前一个");
    assert!(!is_open(cx));
    assert_eq!(find(cx, "前一个").focused(), Some(true));
}
#[gpui_kit::test]
fn sv_drag_clamps_outside_and_release_is_one_commit(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    let bounds = find(cx, "饱和度和明度").bounds();
    let start = bounds.center();
    let end = gpui_kit::point(bounds.right() + px(20.), bounds.top() - px(20.));
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: start,
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    cx.simulate_mouse_move(end, Some(gpui_kit::MouseButton::Left), Modifiers::default());
    draw(cx);
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: end,
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    v.read_with(cx, |v, cx| {
        assert_eq!(
            v.events
                .iter()
                .filter(|e| matches!(e, ColorPickerEvent::Commit(_)))
                .count(),
            1
        );
        assert_eq!(v.state.read(cx).recent().len(), 1);
        assert_eq!(v.state.read(cx).value().a, 128. / 255.);
    });
    let count = v.read_with(cx, |v, _| v.events.len());
    cx.simulate_mouse_move(start, None, Modifiers::default());
    draw(cx);
    assert_eq!(v.read_with(cx, |v, _| v.events.len()), count);
}

#[gpui_kit::test]
fn sliders_are_focusable_independent_and_keep_alpha_or_rgb(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    key(cx, "tab");
    assert_eq!(find(cx, "色相").focused(), Some(true));
    let hue = find(cx, "色相").bounds();
    let alpha = find(cx, "透明度").bounds();
    assert!(hue.bottom() <= alpha.top());
    key(cx, "home");
    assert_eq!(find(cx, "HEX").value(), Some("#CC333380"));
    key(cx, "tab");
    assert_eq!(find(cx, "透明度").focused(), Some(true));
    key(cx, "end");
    assert_eq!(find(cx, "HEX").value(), Some("#CC3333"));
    key(cx, "home");
    assert_eq!(find(cx, "HEX").value(), Some("#CC333300"));
    v.read_with(cx, |v, cx| {
        assert_eq!(v.state.read(cx).value().r, 204. / 255.)
    });
}

fn slider_thumb(cx: &mut VisualTestContext, alpha: bool) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    let id = gpui_kit::ElementId::from(if alpha {
        "color-picker-alpha-thumb"
    } else {
        "color-picker-hue-thumb"
    });
    cx.update(|window, _| {
        let found: Vec<_> = snapshots(window)
            .into_iter()
            .filter(|node| node.path().last() == Some(&id))
            .collect();
        assert_eq!(found.len(), 1, "one rendered thumb for {id:?}");
        found[0].bounds()
    })
}

#[gpui_kit::test]
fn slider_press_grows_from_its_center_without_changing_the_track(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    click(cx, "填充");
    for alpha in [false, true] {
        let label = if alpha { "透明度" } else { "色相" };
        let track = find(cx, label).bounds();
        let idle = slider_thumb(cx, alpha);
        assert_eq!(idle.size, gpui_kit::size(px(16.), px(16.)));
        cx.simulate_event(gpui_kit::MouseDownEvent {
            position: idle.center(),
            button: gpui_kit::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        draw(cx);
        let pressed = slider_thumb(cx, alpha);
        let physical_pixel = cx.update(|window, _| 1. / window.scale_factor());
        assert!((f32::from(pressed.size.width) - 18.4).abs() <= physical_pixel);
        assert_eq!(pressed.center().x, idle.center().x);
        assert_eq!(pressed.center().y, idle.center().y);
        assert_eq!(find(cx, label).bounds(), track);
        // Release outside the track must restore the same resting geometry.
        cx.simulate_event(gpui_kit::MouseUpEvent {
            position: gpui_kit::point(track.right() + px(40.), track.bottom() + px(40.)),
            button: gpui_kit::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        draw(cx);
        assert_eq!(slider_thumb(cx, alpha).size, idle.size);
        assert_eq!(find(cx, label).bounds(), track);
    }
}

#[gpui_kit::test]
fn external_color_replacement_resets_a_held_slider_thumb(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    click(cx, "填充");
    let thumb = slider_thumb(cx, false);
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: thumb.center(),
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    assert!(slider_thumb(cx, false).size.width > thumb.size.width);
    let state = view.read_with(cx, |view, _| view.state.clone());
    cx.update(|window, cx| {
        state.update(cx, |state, cx| {
            state.set_value(gpui_kit::rgb(0x00FF00), window, cx)
        });
    });
    draw(cx);
    assert_eq!(slider_thumb(cx, false).size, thumb.size);
    assert_eq!(slider_thumb(cx, true).size, thumb.size);
}

#[gpui_kit::test]
fn slider_thumb_animates_but_its_value_follows_the_pointer_immediately(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    click(cx, "填充");
    let idle = slider_thumb(cx, false);
    let track = find(cx, "色相").bounds();
    let destination = gpui_kit::point(track.left() + track.size.width * 0.25, track.center().y);
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: destination,
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    let first = slider_thumb(cx, false);
    assert_eq!(first.size, idle.size);
    assert!((f32::from(first.center().x - destination.x)).abs() <= 0.5);
    let state = view.read_with(cx, |view, _| view.state.clone());
    let color = state.read_with(cx, |state, _| state.value());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    draw(cx);
    let held = slider_thumb(cx, false);
    assert!(held.size.width > idle.size.width);
    assert_eq!(held.center(), first.center());
    assert_eq!(state.read_with(cx, |state, _| state.value()), color);
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: destination,
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    assert_eq!(slider_thumb(cx, false).size, held.size);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    draw(cx);
    assert_eq!(slider_thumb(cx, false).size, idle.size);
}
#[gpui_kit::test]
fn rgb_and_hsl_fields_share_a_row_at_default_panel_width(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "填充");
    click(cx, "颜色格式：HEX");
    click(cx, "RGB");
    let r = find(cx, "红 R").bounds();
    let a = find(cx, "透明度 A").bounds();
    assert_eq!(r.top(), a.top());
    assert!((r.size.width - a.size.width).abs() < px(1.));
    replace_text(cx, "红 R", "255");
    key(cx, "enter");
    assert_eq!(find(cx, "透明度 A").value(), Some("50.2"));
    click(cx, "颜色格式：RGB");
    click(cx, "HSL");
    assert_eq!(
        find(cx, "色相 H").bounds().top(),
        find(cx, "透明度 A").bounds().top()
    );
    replace_text(cx, "色相 H", "240");
    key(cx, "enter");
    replace_text(cx, "饱和度 S", "100");
    key(cx, "enter");
    replace_text(cx, "亮度 L", "100");
    key(cx, "enter");
    replace_text(cx, "亮度 L", "50");
    key(cx, "enter");
    click(cx, "颜色格式：HSL");
    click(cx, "HEX");
    assert_eq!(find(cx, "HEX").value(), Some("#0000FF80"));
}
#[gpui_kit::test]
fn color_panel_tab_exits_after_moving_within_a_swatch_group(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "填充");
    for _ in 0..7 {
        key(cx, "tab");
    }
    assert_eq!(find(cx, "#3366CC80").focused(), Some(true));
    key(cx, "left");
    assert_eq!(find(cx, "#FF0000").focused(), Some(true));
    key(cx, "tab");
    assert!(!is_open(cx));
    assert_eq!(find(cx, "后一个").focused(), Some(true));
}
#[gpui_kit::test]
fn external_value_interrupts_an_active_drag_without_commit(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    let pos = find(cx, "饱和度和明度").bounds().center();
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: pos,
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    let state = v.read_with(cx, |v, _| v.state.clone());
    cx.update(|w, cx| state.update(cx, |s, cx| s.set_value(gpui_kit::rgb(0x00FF00), w, cx)));
    draw(cx);
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: pos,
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    v.read_with(cx, |v, cx| {
        assert_eq!(v.state.read(cx).value(), gpui_kit::rgb(0x00FF00));
        assert!(
            !v.events
                .iter()
                .any(|e| matches!(e, ColorPickerEvent::Commit(_)))
        );
        assert!(v.state.read(cx).recent().is_empty());
    });
}
#[gpui_kit::test]
fn recent_import_is_bounded_unique_alpha_sensitive_and_instance_local(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let state = v.read_with(cx, |v, _| v.state.clone());
    cx.update(|w, cx| {
        let colors = [
            gpui_kit::rgba(0x3366CC00),
            gpui_kit::rgba(0x3366CC80),
            gpui_kit::rgba(0x3366CC00),
            gpui_kit::rgb(1),
            gpui_kit::rgb(2),
            gpui_kit::rgb(3),
            gpui_kit::rgb(4),
            gpui_kit::rgb(5),
            gpui_kit::rgb(6),
            gpui_kit::rgb(7),
        ];
        state.update(cx, |s, cx| s.set_recent(colors, cx));
        assert_eq!(state.read(cx).recent().len(), 8);
        assert_eq!(&state.read(cx).recent()[..2], &colors[..2]);
        let another = cx.new(|cx| YueColorPickerState::new(w, cx));
        assert!(another.read(cx).recent().is_empty());
    });
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
}
#[gpui_kit::test]
fn copying_submits_canonical_text_and_close_clears_feedback(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    click(cx, "复制颜色");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("#3366CC80")
    );
    assert_eq!(find(cx, "复制请求已提交").role(), Some(Role::Button));
    key(cx, "escape");
    click(cx, "填充");
    assert_eq!(find(cx, "复制颜色").role(), Some(Role::Button));
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
}
#[gpui_kit::test]
fn sizes_and_rem_scale_the_whole_trigger(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    for (size, height) in [
        (ColorPickerSize::Small, 28.),
        (ColorPickerSize::Medium, 32.),
        (ColorPickerSize::Large, 40.),
    ] {
        v.update(cx, |v, cx| {
            v.size = size;
            cx.notify();
        });
        draw(cx);
        assert_eq!(find(cx, "填充").bounds().size.height, px(height));
        cx.update(|w, _| w.set_rem_size(px(24.)));
        draw(cx);
        assert_eq!(find(cx, "填充").bounds().size.height, px(height * 1.5));
        cx.update(|w, _| w.set_rem_size(px(16.)));
    }
    v.update(cx, |v, cx| {
        v.compact = true;
        cx.notify();
    });
    draw(cx);
    let bounds = find(cx, "填充").bounds();
    assert_eq!(bounds.size.width, bounds.size.height);
}

#[gpui_kit::test]
fn swatch_vertical_navigation_uses_the_visual_grid_including_disabled_cells(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.background_width = px(96.);
        cx.notify();
    });
    draw(cx);
    click(cx, "白色");
    key(cx, "down");
    assert_eq!(find(cx, "棋盘格").focused(), Some(true));
    key(cx, "up");
    assert_eq!(find(cx, "白色").focused(), Some(true));
    assert_eq!(v.read_with(cx, |v, _| v.background.to_string()), "white");
}

#[gpui_kit::test]
fn caller_width_resolves_against_the_host_and_not_the_trigger(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.width = Some(px(220.).into());
        cx.notify();
    });
    draw(cx);
    assert_eq!(find(cx, "填充").bounds().size.width, px(220.));
    v.update(cx, |v, cx| {
        v.width = Some(gpui_kit::relative(0.5));
        cx.notify();
    });
    draw(cx);
    assert_eq!(find(cx, "填充").bounds().size.width, px(200.));
    click(cx, "填充");
    assert_eq!(find(cx, "填充颜色编辑器").bounds().size.width, px(300.));
}

#[gpui_kit::test]
fn narrow_inline_panel_wraps_channels_without_clipping_inputs(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.panel = true;
        v.panel_width = Some(px(260.));
        cx.notify();
    });
    draw(cx);
    click(cx, "颜色格式：HEX");
    click(cx, "RGB");
    let panel = find(cx, "填充调色面板").bounds();
    assert_eq!(panel.size.width, px(260.));
    let r = find(cx, "红 R").bounds();
    let a = find(cx, "透明度 A").bounds();
    assert!(a.top() > r.top(), "four channels must wrap at 260px");
    for label in ["红 R", "绿 G", "蓝 B", "透明度 A"] {
        let input = find(cx, label).bounds();
        assert!(input.left() >= panel.left() && input.right() <= panel.right());
        assert!(input.size.width >= px(62.));
    }
    replace_text(cx, "透明度 A", "0");
    key(cx, "enter");
    v.read_with(cx, |v, cx| {
        assert_eq!(v.state.read(cx).value(), gpui_kit::rgba(0x3366CC00))
    });
}

#[gpui_kit::test]
fn rgb_and_hsl_tab_paths_cross_every_field_and_leave_no_indent(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "填充");
    for (format, fields) in [
        ("RGB", ["红 R", "绿 G", "蓝 B", "透明度 A"]),
        ("HSL", ["色相 H", "饱和度 S", "亮度 L", "透明度 A"]),
    ] {
        let old = if format == "RGB" { "HEX" } else { "RGB" };
        click(cx, &format!("颜色格式：{old}"));
        click(cx, format);
        for label in fields {
            key(cx, "tab");
            assert_eq!(find(cx, label).focused(), Some(true));
            assert!(!find(cx, label).value().unwrap().contains('\t'));
        }
        for label in fields[..3].iter().rev() {
            key(cx, "shift-tab");
            assert_eq!(find(cx, label).focused(), Some(true));
        }
    }
}

#[gpui_kit::test]
fn swatch_tab_leaves_the_group_after_arrow_focus_moves(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "红色");
    key(cx, "right");
    assert_eq!(find(cx, "棋盘格").focused(), Some(true));
    key(cx, "tab");
    assert_eq!(find(cx, "色块组之后").focused(), Some(true));
}

#[gpui_kit::test]
fn custom_confirmation_requests_selection_once_and_not_on_open(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "自定义颜色");
    assert_eq!(v.read_with(cx, |v, _| v.selection_changes), 0);
    click(cx, "HEX");
    key(cx, "enter");
    assert_eq!(
        v.read_with(cx, |v, _| v.selection_changes),
        1,
        "one request from one Enter"
    );
    key(cx, "enter");
    assert_eq!(
        v.read_with(cx, |v, _| (v.background.clone(), v.selection_changes)),
        ("custom".into(), 1)
    );
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
    assert_eq!(v.read_with(cx, |v, _| v.form_submits), 0);
}

#[gpui_kit::test]
fn closing_during_sv_drag_removes_editor_semantics_and_cancels_capture(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    let pos = find(cx, "饱和度和明度").bounds().center();
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: pos,
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    key(cx, "escape");
    assert!(!is_open(cx));
    cx.update(|w, _| {
        assert!(!snapshots(w).iter().any(|n| matches!(
            n.role(),
            Some(Role::Slider | Role::TextInput)
        )
            && n.path().contains(&gpui_kit::ElementId::from("picker"))))
    });
    let count = v.read_with(cx, |v, _| v.events.len());
    cx.simulate_mouse_move(
        pos + gpui_kit::point(px(40.), px(20.)),
        Some(gpui_kit::MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    assert_eq!(v.read_with(cx, |v, _| v.events.len()), count);
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: pos,
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    click(cx, "填充");
    assert!(is_open(cx));
}

#[gpui_kit::test]
fn hue_pointer_drag_commits_once_and_external_sync_cancels_the_old_gesture(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx);
    click(cx, "填充");
    let b = find(cx, "色相").bounds();
    let at = |ratio| gpui_kit::point(b.left() + b.size.width * ratio, b.center().y);
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: at(0.2),
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    cx.simulate_mouse_move(
        at(0.4),
        Some(gpui_kit::MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: at(0.4),
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    assert_eq!(
        v.read_with(cx, |v, _| v
            .events
            .iter()
            .filter(|e| matches!(e, ColorPickerEvent::Commit(_)))
            .count()),
        1
    );
    let state = v.read_with(cx, |v, _| v.state.clone());
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: at(0.2),
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    cx.simulate_mouse_move(
        at(0.4),
        Some(gpui_kit::MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    cx.update(|w, cx| state.update(cx, |s, cx| s.set_value(gpui_kit::rgb(0x00FF00), w, cx)));
    draw(cx);
    let count = v.read_with(cx, |v, _| v.events.len());
    cx.simulate_mouse_move(
        at(0.8),
        Some(gpui_kit::MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: at(0.8),
        button: gpui_kit::MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
    assert_eq!(
        state.read_with(cx, |s, _| s.value()),
        gpui_kit::rgb(0x00FF00)
    );
    assert_eq!(v.read_with(cx, |v, _| v.events.len()), count);
    cx.simulate_click(at(0.6), Modifiers::default());
    draw(cx);
    assert_ne!(
        state.read_with(cx, |s, _| s.value()),
        gpui_kit::rgb(0x00FF00)
    );
}

//! Native event paths, camera ownership, revisions and focus isolation.
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Context, Entity, Image, ImageFormat, IntoElement, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, ParentElement as _, Point, Render, Role, ScrollDelta,
    ScrollWheelEvent, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};
use yue_component::{
    Accent, DialogFooter, ImagePreviewDocument, ImagePreviewError, ImagePreviewEvent,
    ImagePreviewFit, ImagePreviewImage, ImagePreviewLoad, ImagePreviewMode, ImagePreviewSnapshot,
    ImagePreviewToolbar, InputState, NeutralTone, ThemeMode, YueButton, YueDialog, YueDialogState,
    YueImagePreview, YueImagePreviewState, YueInput, YueTheme,
};

struct Probe {
    state: Entity<YueImagePreviewState>,
    image: ImagePreviewImage,
    show: bool,
    disabled: bool,
    events: Rc<RefCell<Vec<ImagePreviewEvent>>>,
    _subscription: gpui_kit::Subscription,
    input: Entity<InputState>,
    toolbar: ImagePreviewToolbar,
    toolbar_visible: bool,
    animated: bool,
    preview_size: [f32; 2],
    large: Entity<YueImagePreviewState>,
    dialog: Entity<YueDialogState>,
    expand_enabled: bool,
    expansions: usize,
}
impl Probe {
    fn new(w: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| YueImagePreviewState::new("test-preview", w, cx));
        let image = Image::from_bytes(
            ImageFormat::Png,
            include_bytes!(
                "../../../examples/windows-showcase/assets/image-preview/transparent.png"
            )
            .to_vec(),
        )
        .to_image_data(cx.svg_renderer())
        .unwrap();
        let image = ImagePreviewImage::new(image, "透明测试图片").unwrap();
        state.update(cx, |s, cx| {
            s.set_document(
                document(&image, 1, ImagePreviewLoad::Ready(image.clone())),
                w,
                cx,
            )
            .unwrap()
        });
        let events = Rc::new(RefCell::new(vec![]));
        let out = events.clone();
        let subscription = cx.subscribe(&state, move |_, _, event, cx| {
            out.borrow_mut().push(event.clone());
            cx.notify();
        });
        Self {
            state,
            image,
            show: true,
            disabled: false,
            events,
            _subscription: subscription,
            input: cx.new(|cx| InputState::new(w, cx)),
            toolbar: ImagePreviewToolbar::Floating,
            toolbar_visible: true,
            animated: false,
            preview_size: [600., 380.],
            large: cx.new(|cx| YueImagePreviewState::new("large-preview", w, cx)),
            dialog: cx.new(YueDialogState::new),
            expand_enabled: true,
            expansions: 0,
        }
    }
}
fn document(
    image: &ImagePreviewImage,
    revision: u64,
    result: ImagePreviewLoad,
) -> ImagePreviewDocument {
    ImagePreviewDocument::new("fixture", revision)
        .with_source(ImagePreviewLoad::Ready(image.clone()))
        .with_result(result)
        .full_frame(true)
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut preview = YueImagePreview::new(&self.state)
            .animated(self.animated)
            .disabled(self.disabled)
            .toolbar(self.toolbar)
            .show_toolbar(self.toolbar_visible);
        if self.expand_enabled {
            preview = preview.on_expand(cx.listener(|p, view, w, cx| {
                p.large
                    .update(cx, |s, cx| s.restore_view(view, w, cx).unwrap());
                p.expansions += 1;
                p.dialog.update(cx, |s, cx| s.open(w, cx).unwrap());
                cx.notify();
            }));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .p_4()
            .gap_3()
            .child(YueButton::new("before-preview", "之前"))
            .child(if self.show {
                div()
                    .w(px(self.preview_size[0]))
                    .h(px(self.preview_size[1]))
                    .child(preview)
                    .into_any_element()
            } else {
                div().h(px(380.)).into_any_element()
            })
            .child(
                YueInput::new(
                    "other-input",
                    YueTheme::new(ThemeMode::Light, Accent::Azure),
                    &self.input,
                )
                .accessibility_label("旁边的输入框"),
            )
            .child(YueButton::new("after-preview", "之后"))
            .child(
                YueDialog::new(
                    "large-preview-dialog",
                    yue_component::current_theme(cx),
                    &self.dialog,
                )
                .title("放大预览")
                .close_label("关闭放大预览")
                .footer(DialogFooter::None)
                .width(gpui_kit::rems(44.))
                .child(
                    div()
                        .h(px(440.))
                        .child(YueImagePreview::new(&self.large).animated(false)),
                ),
            )
    }
}
fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.refresh();
        w.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}
fn tick(cx: &mut VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    draw(cx);
}
fn open(cx: &mut TestAppContext) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(|cx| {
        yue_component::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap()
    });
    let mut view = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let p = cx.new(|cx| Probe::new(w, cx));
        view = Some(p.clone());
        gpui_kit::base::Root::new(p, w, cx)
    });
    cx.update(|w, _| w.activate_window());
    cx.simulate_scale_factor_change(1.);
    draw(cx);
    draw(cx);
    (view.unwrap(), cx)
}
fn node(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.label() == Some(label))
            .unwrap_or_else(|| panic!("missing {label}"))
    })
}
fn snap(view: &Entity<Probe>, cx: &VisualTestContext) -> ImagePreviewSnapshot {
    view.read_with(cx, |p, cx| p.state.read(cx).snapshot())
}
fn click(cx: &mut VisualTestContext, label: &str) {
    let target = cx
        .update(|w, _| {
            snapshots(w)
                .into_iter()
                .find(|n| n.label() == Some(label) && n.role() == Some(Role::Button))
        })
        .unwrap_or_else(|| node(cx, label));
    let p = target.bounds().center();
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
}
fn toolbar_opacity(cx: &mut VisualTestContext) -> f32 {
    cx.update(|w, _| {
        w.painted_primitives()
            .into_iter()
            .find_map(|p| match p {
                gpui_kit::Primitive::BackdropBlur(b) if b.quad.bounds.size.width.0 > 80. => {
                    Some(b.opacity)
                }
                _ => None,
            })
            .expect("floating toolbar must paint one glass surface")
    })
}
fn command_has_outline(cx: &mut VisualTestContext, label: &str) -> bool {
    let bounds = node(cx, label).bounds();
    cx.update(|w, _| {
        let scale = w.scale_factor();
        w.painted_primitives().iter().any(|p| match p {
            gpui_kit::Primitive::Quad(q) => {
                q.border_color.a > 0.
                    && q.border_widths.left.0 > 0.
                    && (q.bounds.origin.x.0 / scale - f32::from(bounds.left())).abs() < 2.
                    && (q.bounds.origin.y.0 / scale - f32::from(bounds.top())).abs() < 2.
                    && (q.bounds.size.width.0 / scale - f32::from(bounds.size.width)).abs() < 4.
                    && (q.bounds.size.height.0 / scale - f32::from(bounds.size.height)).abs() < 4.
            }
            _ => false,
        })
    })
}
fn down(cx: &mut VisualTestContext, p: Point<gpui_kit::Pixels>) {
    cx.simulate_event(MouseDownEvent {
        position: p,
        button: MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
}
fn up(cx: &mut VisualTestContext, p: Point<gpui_kit::Pixels>) {
    cx.simulate_event(MouseUpEvent {
        position: p,
        button: MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
}
fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = gpui_kit::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
    draw(cx);
}
#[gpui_kit::test]
fn checker_paints_only_complete_squares_when_the_viewport_changes(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let color: gpui_kit::Background = theme
        .token("--image-preview-checker-dark-b")
        .unwrap()
        .rgba()
        .unwrap()
        .into();
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        for size in [[600., 380.], [437.5, 301.25], [680., 440.]] {
            view.update(cx, |v, cx| {
                v.preview_size = size;
                cx.notify();
            });
            draw(cx);
            cx.update(|w, _| {
                let tiles: Vec<_> = w
                    .painted_quads()
                    .into_iter()
                    .filter(|q| q.background == color)
                    .collect();
                assert!(tiles.len() > 10, "production checker must paint its tiles");
                let side = tiles[0].bounds.size.width.as_f32();
                for tile in tiles {
                    assert!(
                        (tile.bounds.size.width.as_f32() - side).abs() < 0.001,
                        "scale={scale}, size={size:?}, expected side={side}, tile={tile:?}"
                    );
                    assert!((tile.bounds.size.height.as_f32() - side).abs() < 0.001);
                    assert_eq!(
                        tile.bounds.intersect(&tile.content_mask.bounds),
                        tile.bounds,
                        "no clipped edge/corner tile"
                    );
                }
            });
        }
    }
}
#[gpui_kit::test]
fn comparison_grip_paints_a_visible_icon_inside_the_circle(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "对比");
    let grip = node(cx, "对比分割线").bounds();
    cx.update(|w, _| {
        let bounds = grip.scale(w.scale_factor());
        assert!(
            w.painted_primitives().iter().any(|p| {
                matches!(p, gpui_kit::Primitive::MonochromeSprite(s)
                if s.color.a > 0. && s.bounds.size.width.as_f32() > 0.
                    && bounds.intersect(&s.bounds) == s.bounds)
            }),
            "grip must contain a painted, visible icon"
        );
    });
}
#[gpui_kit::test]
fn toolbar_zoom_and_actual_size_use_production_geometry(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let first = snap(&view, cx);
    assert!(first.zoom() > 0. && first.zoom() < 1.);
    assert!(node(cx, "图片预览").bounds().size.height > px(100.));
    click(cx, "放大");
    assert!((snap(&view, cx).zoom() - first.zoom() * 1.25).abs() < 0.001);
    click(cx, "回到100%");
    assert_eq!(snap(&view, cx).zoom(), 1.);
    click(cx, "适应窗口");
    assert!((snap(&view, cx).zoom() - first.zoom()).abs() < 0.001);
}

#[gpui_kit::test]
fn toolbar_fit_width_uses_the_command_and_keyboard_stays_local(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "适应宽度");
    assert_eq!(snap(&view, cx).fit(), ImagePreviewFit::FitWidth);
    assert!(command_has_outline(cx, "适应宽度"));
    assert!(!command_has_outline(cx, "适应窗口"));
    assert!(node(cx, "适应宽度").description().unwrap().contains("当前"));
    click(cx, "图片预览");
    cx.simulate_keystrokes("0 w");
    draw(cx);
    assert_eq!(snap(&view, cx).fit(), ImagePreviewFit::FitWidth);
    click(cx, "旁边的输入框");
    cx.simulate_keystrokes("w");
    draw(cx);
    assert_eq!(
        view.read_with(cx, |p, cx| p.input.read(cx).value().to_string()),
        "w"
    );
}

#[gpui_kit::test]
fn fullscreen_button_and_keyboard_shortcuts_toggle_the_window(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    click(cx, "全屏");
    assert!(cx.update(|w, _| w.is_fullscreen()));
    press(cx, "escape");
    assert!(!cx.update(|w, _| w.is_fullscreen()));
    click(cx, "图片预览");
    press(cx, "f");
    assert!(cx.update(|w, _| w.is_fullscreen()));
    click(cx, "退出全屏");
    assert!(!cx.update(|w, _| w.is_fullscreen()));
}

#[gpui_kit::test]
fn gallery_filmstrip_reveals_at_the_bottom_and_selects_documents(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |p, cx| {
        p.toolbar = ImagePreviewToolbar::Footer;
        cx.notify();
    });
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            p.state.update(cx, |s, cx| {
                s.set_gallery(
                    vec![
                        document(&p.image, 1, ImagePreviewLoad::Ready(p.image.clone())),
                        ImagePreviewDocument::new("second", 1)
                            .with_source(ImagePreviewLoad::Ready(p.image.clone())),
                    ],
                    0,
                    w,
                    cx,
                )
                .unwrap();
            });
        });
    });
    draw(cx);
    assert!(cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("图片标题栏"))));
    assert!(cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("下一张"))));
    let viewport = node(cx, "图片预览").bounds();
    let side = node(cx, "下一张图片").bounds();
    assert!((f32::from(side.center().y - viewport.center().y)).abs() <= 1.5);
    click(cx, "下一张图片");
    assert_eq!(snap(&view, cx).document_id().unwrap().as_ref(), "second");
    click(cx, "上一张图片");
    click(cx, "下一张");
    assert_eq!(snap(&view, cx).document_id().unwrap().as_ref(), "second");
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            p.state
                .update(cx, |s, cx| s.select_gallery_item(0, w, cx).unwrap());
        });
    });
    draw(cx);
    let bounds = node(cx, "图片预览").bounds();
    cx.simulate_mouse_move(
        point(bounds.left() + px(20.), bounds.bottom() - px(8.)),
        None,
        Modifiers::default(),
    );
    draw(cx);
    assert!(cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("图片胶片条"))));
    assert!(cx.update(|w, _| {
        snapshots(w)
            .iter()
            .any(|n| n.label() == Some("第 2 张：second"))
    }));
    click(cx, "第 2 张：second");
    assert_eq!(snap(&view, cx).document_id().unwrap().as_ref(), "second");
    cx.simulate_mouse_move(
        point(bounds.left() + px(20.), bounds.top() + px(20.)),
        None,
        Modifiers::default(),
    );
    draw(cx);
    tick(cx, 999);
    assert!(cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("图片胶片条"))));
    tick(cx, 1);
    assert!(!cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("图片胶片条"))));
}

#[gpui_kit::test]
fn footer_layout_has_a_titlebar_and_a_dedicated_bottom_toolbar(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |p, cx| {
        p.toolbar = ImagePreviewToolbar::Footer;
        cx.notify();
    });
    draw(cx);
    assert!(cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("图片标题栏"))));
    assert!(cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("图片工具栏"))));
    let title = node(cx, "图片标题栏").bounds();
    let viewport = node(cx, "图片预览").bounds();
    let toolbar = node(cx, "图片工具栏").bounds();
    let mode = node(cx, "预览模式").bounds();
    assert_eq!(title.bottom(), viewport.top());
    assert_eq!(viewport.bottom(), toolbar.top());
    assert_eq!(title.left(), viewport.left());
    assert_eq!(toolbar.left(), viewport.left());
    assert_eq!(title.right(), viewport.right());
    assert_eq!(toolbar.right(), viewport.right());
    assert!(mode.top() >= title.top() && mode.bottom() <= title.bottom());
    for label in [
        "背景",
        "显示",
        "图片缩放倍率",
        "缩放倍率预设",
        "适应窗口",
        "适应宽度",
        "键盘操作",
    ] {
        let bounds = node(cx, label).bounds();
        assert!(
            bounds.top() >= toolbar.top() && bounds.bottom() <= toolbar.bottom(),
            "{label} belongs to the footer band"
        );
        assert!(
            bounds.left() >= toolbar.left() && bounds.right() <= toolbar.right(),
            "{label} must fit inside the shell"
        );
    }
}

#[gpui_kit::test]
fn footer_zoom_slider_cannot_overlap_adjacent_buttons(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |p, cx| {
        p.toolbar = ImagePreviewToolbar::Footer;
        cx.notify();
    });
    for scale in [1., 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        for width in [600., 1100.] {
            view.update(cx, |p, cx| {
                p.preview_size[0] = width;
                cx.notify();
            });
            draw(cx);
            let center = node(cx, "图片预览").bounds().center();
            cx.update(|w, cx| {
                view.update(cx, |p, cx| {
                    p.state.update(cx, |s, cx| {
                        // The thumb reaches both ends of the rail; inspecting only
                        // the initial middle position misses the overflow.
                        s.zoom_at(8., [center.x.into(), center.y.into()], false, w, cx)
                            .unwrap();
                    });
                });
            });
            draw(cx);
            let minus = node(cx, "缩小").bounds();
            let plus = node(cx, "放大").bounds();
            let thumb = node(cx, "图片缩放倍率").bounds();
            let slider_id = view.read_with(cx, |p, cx| {
                gpui_kit::ElementId::NamedChild(
                    std::sync::Arc::new(p.state.read(cx).id().clone()),
                    "zoom-slider".into(),
                )
            });
            let slider = cx.update(|w, _| {
                snapshots(w)
                    .into_iter()
                    .find(|n| n.path().last() == Some(&slider_id))
                    .expect("zoom slider frame")
                    .bounds()
            });
            assert!(
                minus.right() < slider.left(),
                "scale={scale}, width={width}: minus/slider gap"
            );
            assert!(
                slider.right() < plus.left(),
                "scale={scale}, width={width}: slider/plus gap"
            );
            assert!(
                thumb.right() <= slider.right(),
                "scale={scale}, width={width}: thumb overflows slider: {thumb:?}, frame: {slider:?}"
            );
        }
    }
}

#[gpui_kit::test]
fn footer_controls_and_information_preserve_viewport_geometry(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    view.update(cx, |p, cx| {
        p.toolbar = ImagePreviewToolbar::Footer;
        cx.notify();
    });
    draw(cx);
    let viewport = node(cx, "图片预览").bounds();
    click(cx, "缩放倍率预设");
    click(cx, "200%");
    tick(cx, 500);
    assert!((snap(&view, cx).zoom() - 2.).abs() < 0.001);
    click(cx, "适应窗口");
    assert_eq!(snap(&view, cx).fit(), ImagePreviewFit::FitAll);
    let slider = node(cx, "图片缩放倍率").bounds();
    let target = slider.center() + point(px(40.), px(0.));
    down(cx, slider.center());
    cx.simulate_mouse_move(target, Some(MouseButton::Left), Modifiers::default());
    up(cx, target);
    assert!(
        snap(&view, cx).zoom() > 2.,
        "slider pointer input changes the image camera"
    );
    click(cx, "背景");
    click(cx, "纯白");
    assert_eq!(
        snap(&view, cx).background(),
        yue_component::ImagePreviewBackground::White
    );
    click(cx, "图片信息");
    draw(cx);
    assert!(node(cx, "图片信息面板").bounds().intersects(&viewport));
    assert_eq!(node(cx, "图片预览").bounds(), viewport);
    press(cx, "escape");
    tick(cx, 500);
    assert!(!cx.update(|w, _| {
        snapshots(w)
            .iter()
            .any(|n| n.label() == Some("图片信息面板"))
    }));
    click(cx, "显示");
    click(cx, "放大预览…");
    assert_eq!(view.read_with(cx, |p, _| p.expansions), 1);
}

#[gpui_kit::test]
fn more_menu_switches_background_and_centers_only_comparison(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let before = snap(&view, cx);
    click(cx, "更多预览操作");
    assert_eq!(node(cx, "深色棋盘格").selected(), Some(true));
    assert!(!cx.update(|w, _| {
        snapshots(w)
            .iter()
            .any(|n| n.label() == Some("对比归中（50%）"))
    }));
    click(cx, "纯白");
    assert_eq!(
        snap(&view, cx).background(),
        yue_component::ImagePreviewBackground::White
    );
    assert_eq!(snap(&view, cx).zoom(), before.zoom());
    assert_eq!(snap(&view, cx).center(), before.center());
    click(cx, "对比");
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            p.state.update(cx, |s, cx| s.set_split(0.2, w, cx).unwrap())
        })
    });
    let before = snap(&view, cx);
    click(cx, "更多预览操作");
    click(cx, "对比归中（50%）");
    let after = snap(&view, cx);
    assert_eq!(after.split(), 0.5);
    assert_eq!(after.zoom(), before.zoom());
    assert_eq!(after.center(), before.center());
}

#[gpui_kit::test]
fn pixel_grid_menu_enforces_threshold_and_paints_native_nearest_sprites(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    click(cx, "更多预览操作");
    click(cx, "像素网格（≥400%）");
    assert!(!snap(&view, cx).is_pixel_grid_enabled());
    press(cx, "escape");
    draw(cx);
    let center = node(cx, "图片预览").bounds().center();
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            p.state.update(cx, |s, cx| {
                s.zoom_at(4., [center.x.into(), center.y.into()], false, w, cx)
                    .unwrap()
            })
        })
    });
    draw(cx);
    click(cx, "更多预览操作");
    click(cx, "像素网格（≥400%）");
    assert!(snap(&view, cx).is_pixel_grid_enabled());
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let color: gpui_kit::Background = theme
        .token("--image-preview-pixel-grid-color")
        .unwrap()
        .rgba()
        .unwrap()
        .into();
    let background: gpui_kit::Background = theme
        .token("--image-preview-checker-dark-a")
        .unwrap()
        .rgba()
        .unwrap()
        .into();
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        draw(cx);
        cx.update(|w, _| {
            let primitives = w.painted_primitives();
            let viewport = primitives
                .iter()
                .find_map(|p| match p {
                    gpui_kit::Primitive::Quad(q)
                        if q.background == background && q.corner_radii.top_left.0 == 0. =>
                    {
                        Some(q)
                    }
                    _ => None,
                })
                .unwrap();
            assert!(
                primitives.iter().any(
                    |p| matches!(p, gpui_kit::Primitive::PolychromeSprite(s) if s.pad & 1 != 0)
                )
            );
            let lines: Vec<_> = primitives
                .iter()
                .filter_map(|p| match p {
                    gpui_kit::Primitive::Quad(q) if q.background == color => Some(q),
                    _ => None,
                })
                .collect();
            assert!(!lines.is_empty());
            assert!(
                lines.len() < 2048,
                "only visible pixel boundaries are drawn"
            );
            for line in lines {
                assert!(line.bounds.size.width.0 == 1. || line.bounds.size.height.0 == 1.);
                let visible = line.bounds.intersect(&line.content_mask.bounds);
                let radius = viewport.corner_radii.top_left.0;
                for x in [visible.left().0, visible.right().0] {
                    for y in [visible.top().0, visible.bottom().0] {
                        let dx = (viewport.bounds.left().0 + radius - x)
                            .max(x - viewport.bounds.right().0 + radius)
                            .max(0.);
                        let dy = (viewport.bounds.top().0 + radius - y)
                            .max(y - viewport.bounds.bottom().0 + radius)
                            .max(0.);
                        assert!(
                            dx * dx + dy * dy <= radius * radius + 0.01,
                            "grid line must stay inside rounded viewport: {visible:?}"
                        );
                    }
                }
            }
        });
    }
    assert!(view.read_with(cx, |p, _| {
        p.events
            .borrow()
            .iter()
            .any(|e| matches!(e, ImagePreviewEvent::PixelGridChanged(_, _)))
    }));
    click(cx, "对比");
    cx.update(|w, _| {
        let mut images: Vec<_> = w
            .painted_primitives()
            .into_iter()
            .filter_map(|p| match p {
                gpui_kit::Primitive::PolychromeSprite(s) if s.pad & 1 != 0 => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(images.len(), 2);
        images.sort_by(|a, b| {
            a.content_mask
                .bounds
                .left()
                .0
                .total_cmp(&b.content_mask.bounds.left().0)
        });
        assert_eq!(images[0].corner_radii.top_left.0, 0.);
        assert_eq!(images[0].corner_radii.top_right.0, 0.);
        assert_eq!(images[1].corner_radii.top_left.0, 0.);
        assert_eq!(images[1].corner_radii.top_right.0, 0.);
        assert_eq!(
            images[0].bounds, images[1].bounds,
            "comparison shares full-image sampling geometry"
        );
    });
    click(cx, "适应窗口");
    assert!(
        snap(&view, cx).is_pixel_grid_enabled(),
        "remember the preference below threshold"
    );
    cx.update(|w, _| {
        assert!(
            !w.painted_primitives()
                .iter()
                .any(|p| matches!(p, gpui_kit::Primitive::PolychromeSprite(s) if s.pad & 1 != 0))
        )
    });
}

#[gpui_kit::test]
fn expand_opens_independent_dialog_with_camera_and_retained_result(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    click(cx, "对比");
    click(cx, "回到100%");
    click(cx, "放大");
    let p = node(cx, "图片预览").bounds().center() - point(px(90.), px(30.));
    down(cx, p);
    let to = p + point(px(30.), px(15.));
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    up(cx, to);
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            let doc = document(&p.image, 2, ImagePreviewLoad::Loading);
            p.state
                .update(cx, |s, cx| s.set_document(doc, w, cx).unwrap());
        })
    });
    draw(cx);
    let before = snap(&view, cx);
    assert_eq!(before.result_revision(), Some(1));
    click(cx, "放大预览");
    assert!(view.read_with(cx, |p, cx| p.dialog.read(cx).is_open()));
    assert_eq!(view.read_with(cx, |p, _| p.expansions), 1);
    assert_eq!(
        view.read_with(cx, |p, cx| p.large.read(cx).snapshot()),
        before
    );
    view.update(cx, |p, cx| {
        p.large.update(cx, |s, cx| {
            s.set_background(yue_component::ImagePreviewBackground::White, cx)
        })
    });
    draw(cx);
    assert_eq!(
        snap(&view, cx),
        before,
        "larger view edits remain independent"
    );
    let more = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| {
                n.label() == Some("背景")
                    && n.role() == Some(Role::Button)
                    && n.path().contains(&"large-preview-dialog".into())
            })
            .expect("dialog's own more button")
            .bounds()
            .center()
    });
    cx.simulate_click(more, Modifiers::default());
    draw(cx);
    assert!(cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("深色棋盘格"))));
    press(cx, "escape");
    assert!(
        view.read_with(cx, |p, cx| p.dialog.read(cx).is_open()),
        "Escape closes the child menu first"
    );
    assert!(!cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("深色棋盘格"))));
    // Returning focus may show the trigger's help tooltip. Give the viewport
    // focus before checking the preview's own unhandled Escape path.
    let viewport = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| {
                n.label() == Some("图片预览") && n.path().contains(&"large-preview-dialog".into())
            })
            .unwrap()
            .bounds()
    });
    cx.simulate_click(
        viewport.origin + point(px(40.), px(40.)),
        Modifiers::default(),
    );
    draw(cx);
    press(cx, "escape");
    draw(cx);
    assert!(!view.read_with(cx, |p, cx| p.dialog.read(cx).is_open()));
    assert_eq!(
        cx.update(|w, _| snapshots(w)
            .into_iter()
            .find(|n| n.label() == Some("放大预览") && n.role() == Some(Role::Button))
            .unwrap()
            .focused()),
        Some(true)
    );
}
#[gpui_kit::test]
fn expand_refits_automatic_views_preserves_alignment_and_requires_a_host(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            let doc =
                document(&p.image, 2, ImagePreviewLoad::Ready(p.image.clone())).full_frame(false);
            p.state
                .update(cx, |s, cx| s.set_document(doc, w, cx).unwrap());
        })
    });
    draw(cx);
    let before = snap(&view, cx);
    click(cx, "放大预览");
    draw(cx);
    let large = view.read_with(cx, |p, cx| p.large.read(cx).snapshot());
    assert_eq!(large.fit(), ImagePreviewFit::FitAll);
    assert!(
        large.zoom() > before.zoom(),
        "FitAll uses the larger dialog viewport"
    );
    assert_eq!(large.center(), [0.5, 0.5]);
    assert!(
        !view.read_with(cx, |p, cx| p
            .large
            .read(cx)
            .is_mode_available(ImagePreviewMode::Compare)),
        "opening a dialog cannot invent full-frame correspondence"
    );
    assert_eq!(snap(&view, cx), before);
    press(cx, "escape");
    draw(cx);
    view.update(cx, |p, cx| {
        p.expand_enabled = false;
        cx.notify();
    });
    draw(cx);
    assert!(
        !cx.update(|w, _| snapshots(w)
            .iter()
            .any(|n| n.label() == Some("放大预览") && n.role() == Some(Role::Button))),
        "no inert expand button without a host callback"
    );
}
#[gpui_kit::test]
fn toolbar_exposes_fit_and_actual_size_as_distinct_current_commands(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    assert!(command_has_outline(cx, "适应窗口"));
    assert!(!command_has_outline(cx, "回到100%"));
    assert!(node(cx, "适应窗口").description().unwrap().contains("当前"));
    assert!(!node(cx, "回到100%").description().unwrap().contains("当前"));
    let width = node(cx, "回到100%").bounds().size.width;
    click(cx, "回到100%");
    assert!(command_has_outline(cx, "回到100%"));
    assert!(!command_has_outline(cx, "适应窗口"));
    assert!(node(cx, "回到100%").description().unwrap().contains("当前"));
    assert!(!node(cx, "适应窗口").description().unwrap().contains("当前"));
    assert_eq!(node(cx, "回到100%").bounds().size.width, width);
    click(cx, "放大");
    assert!(!node(cx, "回到100%").description().unwrap().contains("当前"));
    assert!(!node(cx, "适应窗口").description().unwrap().contains("当前"));
    view.update(cx, |p, cx| {
        p.preview_size = [720., 560.];
        cx.notify();
    });
    draw(cx);
    click(cx, "适应窗口");
    assert_eq!(snap(&view, cx).zoom(), 1.);
    assert!(node(cx, "适应窗口").description().unwrap().contains("当前"));
    assert!(!node(cx, "回到100%").description().unwrap().contains("当前"));
}
#[gpui_kit::test]
fn floating_toolbar_dims_only_while_panning_and_restores_on_hover_and_release(
    cx: &mut TestAppContext,
) {
    let (_, cx) = open(cx);
    let start = node(cx, "图片预览").bounds().center() - point(px(90.), px(30.));
    down(cx, start);
    assert_eq!(toolbar_opacity(cx), 1., "a press alone must not dim chrome");
    let to = start + point(px(36.), px(24.));
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 0.5);
    let zoom = node(cx, "放大").bounds().center();
    cx.simulate_mouse_move(zoom, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(
        toolbar_opacity(cx),
        1.,
        "hover must restore during pointer capture"
    );
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 0.5);
    up(cx, to);
    assert_eq!(toolbar_opacity(cx), 1.);
}
#[gpui_kit::test]
fn toolbar_focus_and_comparison_drag_keep_chrome_clear(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    let p = node(cx, "图片预览").bounds().center() - point(px(90.), px(30.));
    down(cx, p);
    cx.simulate_mouse_move(
        p + point(px(36.), px(24.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 0.5);
    cx.simulate_keystrokes("tab");
    draw(cx);
    assert!(
        ["缩小", "回到100%", "放大", "适应窗口"]
            .iter()
            .any(|name| node(cx, name).focused() == Some(true))
    );
    assert_eq!(toolbar_opacity(cx), 1.);
    up(cx, p);
    click(cx, "对比");
    let grip = node(cx, "对比分割线").bounds().center();
    down(cx, grip);
    let to = grip + point(px(30.), px(0.));
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 1.);
    up(cx, to);
}
#[gpui_kit::test]
fn toolbar_fade_reverses_continuously_and_stops_requesting_frames(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    view.update(cx, |p, cx| {
        p.animated = true;
        cx.notify();
    });
    draw(cx);
    let p = node(cx, "图片预览").bounds().center() - point(px(90.), px(30.));
    down(cx, p);
    let to = p + point(px(36.), px(24.));
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 1.);
    cx.executor().advance_clock(Duration::from_millis(50));
    draw(cx);
    let halfway = toolbar_opacity(cx);
    assert!(
        (halfway - 0.75).abs() < 0.001,
        "linear fade midpoint: {halfway}"
    );
    let zoom = node(cx, "放大").bounds().center();
    cx.simulate_mouse_move(zoom, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert!(
        (toolbar_opacity(cx) - halfway).abs() < 0.001,
        "hover reverses without jumping"
    );
    cx.executor().advance_clock(Duration::from_millis(25));
    draw(cx);
    let restoring = toolbar_opacity(cx);
    assert!(restoring > halfway && restoring < 1.);
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert!((toolbar_opacity(cx) - restoring).abs() < 0.001);
    up(cx, to);
    cx.executor().advance_clock(Duration::from_millis(500));
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 1.);
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
    });
    draw(cx);
    assert_eq!(
        cx.update(|w, cx| w.simulate_next_frame(cx)),
        0,
        "settled toolbar must stop frame demand"
    );
}
#[gpui_kit::test]
fn reduced_motion_and_disabled_preview_settle_toolbar_immediately(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |p, cx| {
        p.animated = true;
        cx.notify();
    });
    cx.update(|_, cx| cx.set_reduce_motion(true));
    draw(cx);
    let p = node(cx, "图片预览").bounds().center() - point(px(90.), px(30.));
    down(cx, p);
    cx.simulate_mouse_move(
        p + point(px(36.), px(24.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 0.5);
    cx.simulate_keystrokes("tab");
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 1.);
    up(cx, p);
    down(cx, p);
    cx.simulate_mouse_move(
        p + point(px(36.), px(24.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 0.5);
    view.update(cx, |p, cx| {
        p.disabled = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(toolbar_opacity(cx), 1.);
    assert!(cx.update(|w, _| w.captured_hitbox().is_none()));
}
#[gpui_kit::test]
fn floating_toolbar_visibility_preserves_viewport_and_fitted_camera(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |p, cx| {
        p.preview_size = [600., p.image.pixel_size()[1] as f32 * 0.375 + 48.];
        cx.notify();
    });
    for scale in [1., 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        draw(cx);
        click(cx, "适应窗口");
        let viewport = node(cx, "图片预览").bounds();
        let before = snap(&view, cx);
        assert!(
            before.zoom() < 1.,
            "fixture must constrain the fitted image"
        );
        let zoom = node(cx, "放大").bounds();
        assert_eq!(viewport.intersect(&zoom), zoom);
        let events_before = view.read_with(cx, |p, _| p.events.borrow().len());
        for visible in [false, true] {
            view.update(cx, |p, cx| {
                p.toolbar_visible = visible;
                cx.notify();
            });
            draw(cx);
            assert_eq!(node(cx, "图片预览").bounds(), viewport);
            assert_eq!(snap(&view, cx), before, "toolbar visibility changed FitAll");
            assert_eq!(
                view.read_with(cx, |p, _| p.events.borrow().len()),
                events_before,
                "toolbar visibility must not emit camera changes"
            );
        }
    }
}
#[gpui_kit::test]
fn floating_toolbar_visibility_preserves_zoomed_and_panned_camera(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "回到100%");
    click(cx, "放大");
    let viewport = node(cx, "图片预览").bounds();
    let p = viewport.center() - point(px(90.), px(30.));
    down(cx, p);
    let to = p + point(px(36.), px(24.));
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    up(cx, to);
    let before = snap(&view, cx);
    assert_eq!(before.fit(), ImagePreviewFit::Manual);
    assert_ne!(before.center(), [0.5, 0.5]);
    for visible in [false, true] {
        view.update(cx, |p, cx| {
            p.toolbar_visible = visible;
            cx.notify();
        });
        draw(cx);
        assert_eq!(node(cx, "图片预览").bounds(), viewport);
        assert_eq!(snap(&view, cx), before);
    }
}
#[gpui_kit::test]
fn mode_and_split_keyboard_keep_one_camera_and_restore_middle(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let first = snap(&view, cx);
    click(cx, "对比");
    assert_eq!(snap(&view, cx).mode(), ImagePreviewMode::Compare);
    let grip = node(cx, "对比分割线");
    assert_eq!(grip.role(), Some(Role::Slider));
    click(cx, "对比分割线");
    cx.simulate_keystrokes("right");
    draw(cx);
    assert!((snap(&view, cx).split() - 0.52).abs() < 0.001);
    click(cx, "处理后");
    click(cx, "对比");
    assert!((snap(&view, cx).split() - 0.52).abs() < 0.001);
    assert_eq!(snap(&view, cx).center(), first.center());
    assert_eq!(snap(&view, cx).zoom(), first.zoom());
}
#[gpui_kit::test]
fn dragging_split_does_not_pan_and_releases_capture(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "对比");
    let before = snap(&view, cx);
    let grip = node(cx, "对比分割线").bounds().center();
    down(cx, grip);
    let p = grip + point(px(90.), px(30.));
    cx.simulate_mouse_move(p, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert!(snap(&view, cx).split() > before.split());
    assert_eq!(snap(&view, cx).center(), before.center());
    up(cx, p);
    assert!(cx.update(|w, _| w.captured_hitbox().is_none()));
}
#[gpui_kit::test]
fn fitted_image_can_be_dragged_in_every_mode_and_double_click_resets(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for label in ["原图", "对比", "处理后"] {
        click(cx, label);
        click(cx, "适应窗口");
        let before = snap(&view, cx);
        assert_eq!(before.fit(), ImagePreviewFit::FitAll);
        // Away from the comparison divider and floating toolbar, on the image.
        let b = node(cx, "图片预览").bounds();
        let p = b.center() - point(px(90.), px(30.));
        down(cx, p);
        let delta = point(px(36.), px(24.));
        cx.simulate_mouse_move(p + delta, Some(MouseButton::Left), Modifiers::default());
        draw(cx);
        let after = snap(&view, cx);
        assert_ne!(
            after.center()[0],
            before.center()[0],
            "{label}: horizontal pan"
        );
        assert_ne!(
            after.center()[1],
            before.center()[1],
            "{label}: vertical pan"
        );
        assert_eq!(after.fit(), ImagePreviewFit::Manual);
        assert_eq!(after.zoom(), before.zoom());
        assert_eq!(after.split(), before.split());
        up(cx, p + delta);
        assert!(cx.update(|w, _| w.captured_hitbox().is_none()));
        cx.simulate_mouse_move(p, None, Modifiers::default());
        draw(cx);
        assert_eq!(
            snap(&view, cx),
            after,
            "released image must stop following pointer"
        );
        cx.simulate_event(MouseDownEvent {
            position: p,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        up(cx, p);
        assert_eq!(snap(&view, cx).fit(), ImagePreviewFit::FitAll);
        assert_eq!(snap(&view, cx).center(), before.center());
    }
}
#[gpui_kit::test]
fn image_drag_can_continue_outside_viewport(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "回到100%");
    click(cx, "放大");
    let before = snap(&view, cx);
    let p = node(cx, "图片预览").bounds().center();
    down(cx, p);
    let out = p + point(px(-800.), px(-500.));
    cx.simulate_mouse_move(out, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    up(cx, out);
    assert_ne!(snap(&view, cx).center(), before.center());
    assert!(cx.update(|w, _| w.captured_hitbox().is_none()));
}
#[gpui_kit::test]
fn wheel_changes_zoom_and_does_not_move_other_regions(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = snap(&view, cx);
    let b = node(cx, "图片预览").bounds();
    let p = b.origin + point(px(80.), px(100.));
    cx.simulate_mouse_move(p, None, Modifiers::default());
    cx.simulate_event(ScrollWheelEvent {
        position: p,
        delta: ScrollDelta::Pixels(point(px(0.), px(-80.))),
        ..Default::default()
    });
    draw(cx);
    assert!(snap(&view, cx).zoom() > before.zoom());
    let after = snap(&view, cx);
    let p = node(cx, "之后").bounds().center();
    cx.simulate_mouse_move(p, None, Modifiers::default());
    cx.simulate_event(ScrollWheelEvent {
        position: p,
        delta: ScrollDelta::Lines(point(0., -3.)),
        ..Default::default()
    });
    draw(cx);
    assert_eq!(snap(&view, cx), after);
}
#[gpui_kit::test]
fn disabled_view_rejects_buttons_wheel_and_drag(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |p, cx| {
        p.disabled = true;
        cx.notify();
    });
    draw(cx);
    let before = snap(&view, cx);
    click(cx, "放大");
    let p = node(cx, "图片预览").bounds().center();
    down(cx, p);
    cx.simulate_mouse_move(
        p + point(px(60.), px(50.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    up(cx, p);
    assert_eq!(snap(&view, cx), before);
    assert!(view.read_with(cx, |p, _| p.events.borrow().is_empty()));
}
#[gpui_kit::test]
fn update_failure_retains_view_and_previous_revision(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "对比");
    click(cx, "回到100%");
    let before = snap(&view, cx);
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            let d = document(&p.image, 2, ImagePreviewLoad::Loading);
            p.state
                .update(cx, |s, cx| s.set_document(d, w, cx).unwrap());
        })
    });
    draw(cx);
    let loading = snap(&view, cx);
    assert_eq!(loading.zoom(), before.zoom());
    assert_eq!(loading.result_revision(), Some(1));
    assert!(node(cx, "正在更新 · 当前为上一版").role() == Some(Role::Status));
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            let d = document(&p.image, 2, ImagePreviewLoad::Failed("失败".into()));
            p.state
                .update(cx, |s, cx| s.set_document(d, w, cx).unwrap());
        })
    });
    draw(cx);
    assert_eq!(snap(&view, cx).result_revision(), Some(1));
    assert_eq!(snap(&view, cx).center(), before.center());
    click(cx, "重试");
    assert!(
        view.read_with(cx, |p, _| p.events.borrow().iter().any(|e| matches!(
            e,
            ImagePreviewEvent::RetryRequested {
                revision: 2,
                result: true,
                ..
            }
        )))
    );
}
#[gpui_kit::test]
fn stale_document_is_rejected_atomically_and_setters_do_not_echo(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            let d = document(&p.image, 3, ImagePreviewLoad::Ready(p.image.clone()));
            p.state
                .update(cx, |s, cx| s.set_document(d, w, cx).unwrap());
            let stale = document(&p.image, 2, ImagePreviewLoad::Empty);
            assert_eq!(
                p.state.update(cx, |s, cx| s.set_document(stale, w, cx)),
                Err(ImagePreviewError::StaleRevision)
            );
            p.state.update(cx, |s, cx| s.actual_size(w, cx).unwrap());
        })
    });
    draw(cx);
    assert_eq!(snap(&view, cx).revision(), 3);
    assert_eq!(snap(&view, cx).result_revision(), Some(3));
    assert!(view.read_with(cx, |p, _| p.events.borrow().is_empty()));
}
#[gpui_kit::test]
fn unmount_during_drag_releases_pointer_capture(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let p = node(cx, "图片预览").bounds().center();
    down(cx, p);
    assert!(cx.update(|w, _| w.captured_hitbox().is_some()));
    view.update(cx, |p, cx| {
        p.show = false;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!(cx.update(|w, _| w.captured_hitbox().is_none()));
}
#[gpui_kit::test]
fn dpi_change_preserves_physical_zoom_and_footer_toolbar_does_not_overlap(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "回到100%");
    cx.simulate_scale_factor_change(1.5);
    draw(cx);
    assert_eq!(snap(&view, cx).zoom(), 1.);
    view.update(cx, |p, cx| {
        p.toolbar = ImagePreviewToolbar::Footer;
        cx.notify();
    });
    draw(cx);
    let preview = node(cx, "图片预览").bounds();
    let zoom = node(cx, "放大").bounds();
    assert!(zoom.top() >= preview.bottom());
}

#[gpui_kit::test]
fn keyboard_shortcuts_only_apply_when_the_viewport_itself_has_focus(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let p = node(cx, "图片预览").bounds().center();
    cx.simulate_click(p, Modifiers::default());
    cx.simulate_keystrokes("1");
    draw(cx);
    assert_eq!(snap(&view, cx).zoom(), 1.);
    click(cx, "之后");
    let before = snap(&view, cx);
    cx.simulate_keystrokes("0");
    draw(cx);
    assert_eq!(snap(&view, cx), before);
    click(cx, "旁边的输入框");
    cx.simulate_keystrokes("0");
    draw(cx);
    assert_eq!(snap(&view, cx), before);
    assert_eq!(
        view.read_with(cx, |p, cx| p.input.read(cx).value().to_string()),
        "0"
    );
}

#[gpui_kit::test]
fn identical_document_does_not_interrupt_active_drag(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "回到100%");
    click(cx, "放大");
    let p = node(cx, "图片预览").bounds().center();
    down(cx, p);
    let capture = cx.update(|w, _| w.captured_hitbox());
    assert!(capture.is_some());
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            let doc = document(&p.image, 1, ImagePreviewLoad::Ready(p.image.clone()));
            p.state
                .update(cx, |s, cx| s.set_document(doc, w, cx).unwrap());
        })
    });
    assert_eq!(cx.update(|w, _| w.captured_hitbox()), capture);
    let before = snap(&view, cx);
    cx.simulate_mouse_move(
        p + point(px(-60.), px(-50.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    draw(cx);
    assert_ne!(snap(&view, cx).center(), before.center());
    up(cx, p);
}

#[gpui_kit::test]
fn new_document_resets_view_while_updates_keep_the_manual_camera(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "对比");
    click(cx, "回到100%");
    assert_eq!(snap(&view, cx).fit(), ImagePreviewFit::Manual);
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            let doc = ImagePreviewDocument::new("second-image", 1)
                .with_source(ImagePreviewLoad::Ready(p.image.clone()));
            p.state
                .update(cx, |s, cx| s.set_document(doc, w, cx).unwrap());
        })
    });
    draw(cx);
    let next = snap(&view, cx);
    assert_eq!(next.mode(), ImagePreviewMode::Source);
    assert_eq!(next.fit(), ImagePreviewFit::FitAll);
    assert_eq!(next.center(), [0.5, 0.5]);
    assert!(next.zoom() < 1.);
    assert_eq!(next.result_revision(), None);
}

#[gpui_kit::test]
fn half_resolution_result_converts_zoom_without_changing_view_center(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            let decode = |bytes: &[u8], label| {
                ImagePreviewImage::new(
                    Image::from_bytes(ImageFormat::Jpeg, bytes.to_vec())
                        .to_image_data(cx.svg_renderer())
                        .unwrap(),
                    label,
                )
                .unwrap()
            };
            let source = decode(
                include_bytes!(
                    "../../../examples/windows-showcase/assets/image-preview/desktop-q70.jpg"
                ),
                "原图",
            );
            let result = decode(
                include_bytes!(
                    "../../../examples/windows-showcase/assets/image-preview/desktop-half-q70.jpg"
                ),
                "半尺寸",
            );
            let doc = ImagePreviewDocument::new("half-resolution", 1)
                .with_source(ImagePreviewLoad::Ready(source))
                .with_result(ImagePreviewLoad::Ready(result))
                .full_frame(true);
            p.state
                .update(cx, |s, cx| s.set_document(doc, w, cx).unwrap());
        })
    });
    draw(cx);
    click(cx, "回到100%");
    let source = snap(&view, cx);
    assert_eq!(source.reference_size(), Some([3840, 2088]));
    click(cx, "对比");
    let result = snap(&view, cx);
    assert_eq!(result.reference_size(), Some([1920, 1044]));
    assert_eq!(result.zoom(), source.zoom() * 2.);
    assert_eq!(result.center(), source.center());
    click(cx, "原图");
    assert_eq!(snap(&view, cx).zoom(), source.zoom());
}

#[gpui_kit::test]
fn leaving_compare_transfers_removed_grip_focus_to_viewport(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "对比");
    click(cx, "对比分割线");
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            p.state.update(cx, |s, cx| {
                s.set_mode(ImagePreviewMode::Result, w, cx).unwrap()
            });
        })
    });
    draw(cx);
    assert_eq!(node(cx, "图片预览").focused(), Some(true));
    cx.simulate_keystrokes("1");
    draw(cx);
    assert_eq!(snap(&view, cx).zoom(), 1.);
}

#[gpui_kit::test]
fn newly_declared_alignment_cannot_relabel_an_unaligned_previous_result(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|w, cx| {
        view.update(cx, |p, cx| {
            let unaligned =
                document(&p.image, 2, ImagePreviewLoad::Ready(p.image.clone())).full_frame(false);
            p.state.update(cx, |s, cx| {
                s.set_document(unaligned, w, cx).unwrap();
                assert!(!s.is_mode_available(ImagePreviewMode::Compare));
                s.set_mode(ImagePreviewMode::Result, w, cx).unwrap();
                let updating = document(&p.image, 3, ImagePreviewLoad::Loading).full_frame(true);
                s.set_document(updating, w, cx).unwrap();
                assert!(!s.is_mode_available(ImagePreviewMode::Compare));
                assert!(!s.is_mode_available(ImagePreviewMode::Result));
            });
        })
    });
    draw(cx);
    assert_eq!(snap(&view, cx).mode(), ImagePreviewMode::Source);
    assert_eq!(snap(&view, cx).result_revision(), None);
}

#[gpui_kit::test]
fn floating_toolbar_and_compare_grip_reuse_core_blur_footer_stays_solid(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let filters = |cx: &mut VisualTestContext| {
        cx.update(|w, _| {
            w.painted_primitives()
                .into_iter()
                .filter_map(|p| {
                    if let gpui_kit::Primitive::BackdropBlur(b) = p {
                        Some(b)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
    };
    let initial = filters(cx);
    assert_eq!(
        initial.len(),
        1,
        "Floating toolbar owns one filter, not one per button"
    );
    assert_eq!(initial[0].sigma, gpui_kit::ScaledPixels(16.));
    click(cx, "对比");
    let compare = filters(cx);
    assert_eq!(compare.len(), 2);
    let grip = node(cx, "对比分割线").bounds();
    assert!(
        compare.iter().any(|b| {
            b.quad.bounds.size.width.0 == f32::from(grip.size.width)
                && b.quad.corner_radii.top_left.0 == f32::from(grip.size.width) / 2.
        }),
        "Circular handle uses its actual rounded output geometry"
    );
    view.update(cx, |p, cx| {
        p.toolbar = ImagePreviewToolbar::Footer;
        cx.notify();
    });
    draw(cx);
    assert_eq!(
        filters(cx).len(),
        1,
        "Only the comparison grip filters the image with a footer toolbar"
    );
    click(cx, "原图");
    assert!(filters(cx).is_empty());
}

#[test]
fn glass_tokens_keep_readable_foreground_over_the_brightest_background() {
    fn luminance(v: gpui_kit::Rgba) -> f32 {
        let linear = |c: f32| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(v.r) + 0.7152 * linear(v.g) + 0.0722 * linear(v.b)
    }
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            for tone in NeutralTone::ALL {
                let theme = YueTheme::new(mode, accent).with_neutral(tone);
                let tint = theme
                    .token("--image-preview-glass-background")
                    .unwrap()
                    .rgba()
                    .unwrap();
                let foreground = theme
                    .token("--image-preview-glass-color")
                    .unwrap()
                    .rgba()
                    .unwrap();
                let white_image = gpui_kit::Rgba {
                    r: tint.r * tint.a + 1. - tint.a,
                    g: tint.g * tint.a + 1. - tint.a,
                    b: tint.b * tint.a + 1. - tint.a,
                    a: 1.,
                };
                let contrast = (luminance(foreground) + 0.05) / (luminance(white_image) + 0.05);
                assert!(contrast >= 4.5, "{mode:?}/{accent:?}/{tone:?}: {contrast}");
            }
        }
    }
}

#[gpui_kit::test]
fn actual_size_snaps_static_raster_origin_to_device_pixels_at_every_dpi(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        view.update(cx, |p, cx| {
            p.preview_size = [601.5, 381.25];
            cx.notify();
        });
        draw(cx);
        click(cx, "回到100%");
        let pos = node(cx, "图片预览").bounds().center() - point(px(90.), px(30.));
        down(cx, pos);
        let to = pos + point(px(0.25), px(0.75));
        cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        up(cx, to);
        let reference = snap(&view, cx).reference_size().unwrap();
        cx.update(|w, _| {
            let images: Vec<_> = w
                .painted_primitives()
                .into_iter()
                .filter_map(|p| match p {
                    gpui_kit::Primitive::PolychromeSprite(s) => Some(s),
                    _ => None,
                })
                .collect();
            assert_eq!(images.len(), 1);
            assert_eq!(
                images[0].bounds.size.width.0, reference[0] as f32,
                "100% must retain full-image geometry even when clipped"
            );
            assert_eq!(images[0].bounds.size.height.0, reference[1] as f32);
            for value in [images[0].bounds.origin.x.0, images[0].bounds.origin.y.0] {
                assert!(
                    (value - value.round()).abs() < 0.001,
                    "DPI {scale}: fractional raster origin {value}"
                );
            }
        });
    }
}

#[gpui_kit::test]
fn proxy_uses_original_dimensions_preserves_camera_on_upgrade_and_suppresses_pixel_grid(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let full = ImagePreviewImage::from_rgba8(1200, 800, vec![255; 1200 * 800 * 4], "full").unwrap();
    let proxy = yue_component::ImagePreviewPixels::from_rgba8(1200, 800, vec![255; 1200 * 800 * 4])
        .unwrap()
        .into_preview(
            &yue_component::ImagePreviewLimits::default().texture_budget(300, 300 * 300 * 4),
            "proxy",
        )
        .unwrap();
    let state = view.read_with(cx, |p, _| p.state.clone());
    cx.update(|w, cx| {
        state.update(cx, |s, cx| {
            s.set_document(
                ImagePreviewDocument::new("proxy-upgrade", 1)
                    .with_source(ImagePreviewLoad::Ready(proxy)),
                w,
                cx,
            )
            .unwrap();
        })
    });
    draw(cx);
    cx.update(|w, cx| {
        state.update(cx, |s, cx| {
            s.zoom_at(4., [200., 160.], false, w, cx).unwrap();
            s.set_pixel_grid_enabled(true, cx);
        })
    });
    draw(cx);
    let before = snap(&view, cx);
    assert_eq!(before.reference_size(), Some([1200, 800]));
    assert_eq!(before.texture_size(), Some([300, 200]));
    assert!(before.is_proxy());
    assert!(!before.is_pixel_grid_available());
    cx.update(|w, _| {
        assert!(
            !w.painted_primitives()
                .iter()
                .any(|p| matches!(p, gpui_kit::Primitive::PolychromeSprite(s) if s.pad & 1 != 0))
        )
    });
    cx.update(|w, cx| {
        state.update(cx, |s, cx| {
            s.set_document(
                ImagePreviewDocument::new("proxy-upgrade", 2)
                    .with_source(ImagePreviewLoad::Ready(full)),
                w,
                cx,
            )
            .unwrap();
        })
    });
    draw(cx);
    let after = snap(&view, cx);
    assert_eq!(before.zoom(), after.zoom());
    assert_eq!(before.center(), after.center());
    assert!(!after.is_proxy());
    assert!(after.is_pixel_grid_available());
    cx.update(|w, _| {
        assert!(
            w.painted_primitives()
                .iter()
                .any(|p| matches!(p, gpui_kit::Primitive::PolychromeSprite(s) if s.pad & 1 != 0))
        )
    });
}

#[gpui_kit::test]
fn compare_with_only_source_proxy_suppresses_grid_and_nearest_until_result_only(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let full = ImagePreviewImage::from_rgba8(1200, 800, vec![255; 1200 * 800 * 4], "full").unwrap();
    let proxy = yue_component::ImagePreviewPixels::from_rgba8(1200, 800, vec![255; 1200 * 800 * 4])
        .unwrap()
        .into_preview(
            &yue_component::ImagePreviewLimits::default().texture_budget(300, 300 * 300 * 4),
            "source proxy",
        )
        .unwrap();
    let state = view.read_with(cx, |p, _| p.state.clone());
    cx.update(|w, cx| {
        state.update(cx, |s, cx| {
            s.set_document(
                ImagePreviewDocument::new("mixed-proxy", 1)
                    .with_source(ImagePreviewLoad::Ready(proxy))
                    .with_result(ImagePreviewLoad::Ready(full))
                    .full_frame(true),
                w,
                cx,
            )
            .unwrap();
            s.set_pixel_grid_enabled(true, cx);
            s.set_sampling(yue_component::ImagePreviewSampling::Nearest, cx);
        });
    });
    draw(cx);
    for mode in [ImagePreviewMode::Compare, ImagePreviewMode::Result] {
        cx.update(|w, cx| {
            state.update(cx, |s, cx| {
                s.set_mode(mode, w, cx).unwrap();
                s.zoom_at(4., [200., 160.], false, w, cx).unwrap();
            });
        });
        draw(cx);
        let comparing = mode == ImagePreviewMode::Compare;
        let current = snap(&view, cx);
        assert_eq!(current.texture_size(), Some([1200, 800]));
        assert_eq!(current.is_proxy(), comparing);
        assert_eq!(current.is_pixel_grid_available(), !comparing);
        if comparing {
            node(cx, "代理预览；100% 按原始尺寸，细节已缩减");
        }
        cx.update(|w, _| {
            let images: Vec<_> = w
                .painted_primitives()
                .into_iter()
                .filter_map(|p| match p {
                    gpui_kit::Primitive::PolychromeSprite(s) => Some(s),
                    _ => None,
                })
                .collect();
            assert_eq!(images.len(), if comparing { 2 } else { 1 });
            assert!(
                images
                    .iter()
                    .all(|s| s.pad == if comparing { 2 } else { 1 })
            );
        });
    }
}

#[gpui_kit::test]
fn sampling_menu_switches_native_path_and_reports_exactly_one_event(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "回到100%");
    click(cx, "更多预览操作");
    click(cx, "像素显示");
    assert_eq!(
        snap(&view, cx).sampling(),
        yue_component::ImagePreviewSampling::Nearest
    );
    assert_eq!(
        view.read_with(cx, |p, _| p
            .events
            .borrow()
            .iter()
            .filter(|e| matches!(e, ImagePreviewEvent::SamplingChanged(_, _)))
            .count()),
        1
    );
    cx.update(|w, _| {
        assert!(
            w.painted_primitives()
                .iter()
                .any(|p| matches!(p, gpui_kit::Primitive::PolychromeSprite(s) if s.pad == 1))
        )
    });
    click(cx, "更多预览操作");
    click(cx, "平滑显示");
    cx.update(|w, _| {
        assert!(
            w.painted_primitives()
                .iter()
                .any(|p| matches!(p, gpui_kit::Primitive::PolychromeSprite(s) if s.pad == 2))
        )
    });
    let image = view.read_with(cx, |p, _| p.image.image().clone());
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        draw(cx);
        click(cx, "放大");
        cx.update(|w, _| {
            assert!(
                w.has_image_atlas_entry(&image),
                "reuse original resource at DPI {scale}"
            )
        });
    }
}

struct PaintOnlyPreview {
    image: ImagePreviewImage,
    sampling: yue_component::ImagePreviewSampling,
}
impl Render for PaintOnlyPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let image = self.image.clone();
        let sampling = self.sampling;
        div().size_full().child(
            gpui_kit::canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    image
                        .paint(bounds, bounds, px(6.), sampling, window)
                        .unwrap();
                },
            )
            .w(px(321.))
            .h(px(123.)),
        )
    }
}

#[gpui_kit::test]
fn paint_only_preview_shares_encoding_geometry_and_sampling_without_chrome(
    cx: &mut TestAppContext,
) {
    use yue_component::{
        ImagePreviewColorSpace, ImagePreviewLimits, ImagePreviewPixels, ImagePreviewSampling,
    };
    let bytes: Vec<u8> = [-0.25_f32, 12.5, 0.5, 0.5]
        .into_iter()
        .flat_map(|v| half::f16::from_f32(v).to_le_bytes())
        .collect();
    let image =
        ImagePreviewPixels::from_rgba16f(1, 1, 8, bytes.clone(), ImagePreviewColorSpace::ScRgb)
            .unwrap()
            .into_preview(&ImagePreviewLimits::default(), "HDR surface")
            .unwrap();
    let original = image.image().clone();
    let (view, cx) = cx.add_window_view(|_, _| PaintOnlyPreview {
        image,
        sampling: ImagePreviewSampling::Smooth,
    });
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        draw(cx);
        cx.update(|window, _| {
            let sprites: Vec<_> = window
                .painted_primitives()
                .into_iter()
                .filter_map(|p| match p {
                    gpui_kit::Primitive::PolychromeSprite(s) => Some(s),
                    _ => None,
                })
                .collect();
            assert_eq!(
                sprites.len(),
                1,
                "no footer, toolbar, checker or second image"
            );
            let sprite = &sprites[0];
            // GPUI resolves element layout to physical pixels before canvas paint.
            assert_eq!(sprite.bounds.size.width.0, (321. * scale - 0.5).ceil());
            assert_eq!(sprite.bounds.size.height.0, (123. * scale - 0.5).ceil());
            assert_eq!(sprite.corner_radii.top_left.0, 6. * scale);
            assert_eq!(sprite.pad & 15, 10, "precise linear + absolute scRGB");
            assert!(window.has_image_atlas_entry(&original));
        });
    }
    view.update(cx, |v, cx| {
        v.sampling = ImagePreviewSampling::Nearest;
        cx.notify();
    });
    draw(cx);
    cx.update(|window, _| {
        assert!(
            window
                .painted_primitives()
                .iter()
                .any(|p| matches!(p, gpui_kit::Primitive::PolychromeSprite(s) if s.pad & 15 == 9))
        );
    });
    assert_eq!(original.as_bytes(0).unwrap(), bytes);
}

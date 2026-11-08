use super::*;
use gpui_base::test_support::snapshots;
use gpui_kit::{
    AppContext as _, Context, Render, TestAppContext, WindowBounds, WindowOptions, point, size,
};
use gpui_kit::{Modifiers, MouseDownEvent, MouseUpEvent, PlatformInput};

gpui_kit::actions!(compact_menu_test, [ParentEscape]);

struct CompactMenu {
    open: bool,
    parent_escape: bool,
}
impl Render for CompactMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::current_theme(cx);
        div()
            .key_context("CompactMenuParent")
            .on_action(cx.listener(|this, _: &ParentEscape, _, cx| {
                this.parent_escape = true;
                cx.notify();
            }))
            .size_full()
            .flex()
            .justify_end()
            .items_start()
            .p_2()
            .child(
                YueDropdownMenu::new(
                    "compact-menu",
                    theme,
                    YueButton::new("compact-trigger", "更多"),
                )
                .items((0..12).map(|i| YueMenuItem::new(format!("命令 {i}"))))
                .on_open_change(cx.listener(|this, open, _, cx| {
                    this.open = *open;
                    cx.notify();
                })),
            )
    }
}

#[gpui_kit::test]
fn compact_window_menu_reserves_chrome_and_shadow_before_measuring_rows(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::init(
            cx,
            YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure),
        )
        .unwrap();
        cx.bind_keys([gpui_kit::KeyBinding::new(
            "escape",
            ParentEscape,
            Some("CompactMenuParent"),
        )]);
    });
    for height in [240., 144.] {
        let (window, view) = cx.update(|cx| {
            let mut view = None;
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            point(px(0.), px(0.)),
                            size(px(448.), px(height)),
                        ))),
                        ..Default::default()
                    },
                    |window, cx| {
                        let child = cx.new(|_| CompactMenu {
                            open: false,
                            parent_escape: false,
                        });
                        view = Some(child.clone());
                        cx.new(|cx| gpui_kit::base::Root::new(child, window, cx))
                    },
                )
                .unwrap();
            (window, view.unwrap())
        });
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            let position = snapshots(window)
                .into_iter()
                .find(|n| n.label() == Some("更多") && n.role() == Some(Role::Button))
                .unwrap()
                .bounds()
                .center();
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                }),
                cx,
            );
            window.draw(cx).clear(cx);
            assert!(view.read(cx).open);
            let panel = snapshots(window)
                .into_iter()
                .find(|n| n.role() == Some(Role::Menu))
                .expect("menu must be rendered in compact window");
            assert!(panel.bounds().top() >= px(8.));
            assert!(panel.bounds().bottom() <= px(height - 8.));
        })
        .unwrap();
        cx.simulate_keystrokes(window.into(), "escape");
        cx.update_window(window.into(), |_, window, cx| {
            assert!(!view.read(cx).open);
            assert!(
                !view.read(cx).parent_escape,
                "menu Escape must not invoke an ancestor command"
            );
            window.remove_window();
        })
        .unwrap();
        cx.run_until_parked();
    }
}

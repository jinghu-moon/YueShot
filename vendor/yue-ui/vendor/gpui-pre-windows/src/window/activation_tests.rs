//! Activation policy contracts; the native probe opts into a short-lived GPUI popup.
use super::*;
use std::prelude::v1::test;

#[test]
fn ordinary_windows_remain_activatable_by_default() {
    assert!(WindowOptions::default().is_activatable);
    assert!(!WindowOptions::default().keep_physical_size_on_dpi_change);
}

#[test]
#[ignore = "requires Windows GPU/window services; creates hidden GPUI windows, no input injection"]
fn native_physical_size_survives_dpi_changes() {
    use gpui::{AppContext as _, Application, EmptyView};
    let failures = Rc::new(RefCell::new(Vec::<String>::new()));
    let observed = failures.clone();
    Application::with_platform(Rc::new(crate::WindowsPlatform::new(false).unwrap())).run(move |cx| {
        let mut cases = Vec::new();
        for physical in [false, true] {
            let handle = cx.open_window(WindowOptions {
                titlebar: None,
                kind: WindowKind::PopUp,
                focus: false,
                show: false,
                is_activatable: false,
                is_resizable: false,
                keep_physical_size_on_dpi_change: physical,
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(20.), px(20.)), size(px(64.), px(48.)),
                ))),
                ..Default::default()
            }, |_, cx| cx.new(|_| EmptyView)).unwrap();
            let rwh::RawWindowHandle::Win32(raw) = handle.update(cx, |_, w, _|
                rwh::HasWindowHandle::window_handle(w).unwrap().as_raw()
            ).unwrap() else { panic!("native Win32 window required") };
            let hwnd = HWND(raw.hwnd.get() as *mut _);
            cases.push((physical, handle, hwnd));
        }
        cx.spawn(async move |cx| {
            // Window placement is deferred by the backend. Deliver native
            // messages outside App::update, where callbacks can access the App.
            cx.background_executor().timer(Duration::from_millis(80)).await;
            for (physical, handle, hwnd) in cases {
            unsafe {
                SetWindowLongPtrW(hwnd, GWL_STYLE, WS_POPUP.0 as isize);
                SetWindowPos(hwnd, None, 20, 20, 64, 48,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_FRAMECHANGED).unwrap();
            }
            for dpi in [96_u32, 120, 144, 192, 96] {
                let suggested = RECT { left: -20, top: 20, right: 300, bottom: 220 };
                unsafe { SendMessageW(hwnd, WM_DPICHANGED,
                    Some(WPARAM((dpi | dpi << 16) as usize)),
                    Some(LPARAM((&suggested as *const RECT) as isize))); }
                let mut client = RECT::default();
                unsafe { GetClientRect(hwnd, &mut client).unwrap(); }
                let expected = if physical { (64, 48) } else { (320, 200) };
                let logical = handle.update(cx, |_, w, _| (w.viewport_size(), w.scale_factor())).unwrap();
                if (client.right, client.bottom) != expected
                    || (logical.1 - dpi as f32 / 96.).abs() > 0.01
                    || (f32::from(logical.0.width) * logical.1 - expected.0 as f32).abs() > 0.01
                    || (f32::from(logical.0.height) * logical.1 - expected.1 as f32).abs() > 0.01
                {
                    observed.borrow_mut().push(format!("physical={physical} dpi={dpi}: client={client:?}, logical={logical:?}, expected={expected:?}"));
                }
            }
            handle.update(cx, |_, w, _| w.remove_window()).unwrap();
            }
            cx.update(|cx| cx.quit());
        }).detach();
    });
    assert!(failures.borrow().is_empty(), "{:?}", failures.borrow());
}

#[test]
#[ignore = "requires an interactive Windows desktop; briefly shows GPUI popups without activating them"]
fn native_nonactivating_window_never_steals_focus() {
    use gpui::{AppContext as _, Application, EmptyView};
    fn native_handle(window: &Window) -> HWND {
        let rwh::RawWindowHandle::Win32(raw) = rwh::HasWindowHandle::window_handle(window)
            .unwrap()
            .as_raw()
        else {
            panic!("expected a native Windows window");
        };
        HWND(raw.hwnd.get() as *mut _)
    }
    let foreground = unsafe { GetForegroundWindow() };
    let failures = Rc::new(RefCell::new(Vec::<String>::new()));
    let observed = failures.clone();
    Application::with_platform(Rc::new(crate::WindowsPlatform::new(false).unwrap())).run(
        move |cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(8.), px(8.)),
                ))),
                titlebar: None,
                kind: WindowKind::PopUp,
                is_activatable: false,
                // The persistent policy must override even an explicitly requested initial focus.
                focus: true,
                show: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            };
            let popup = cx
                .open_window(options, |_, cx| cx.new(|_| EmptyView))
                .unwrap();
            let handle = popup
                .update(cx, |_, window, _| native_handle(window))
                .unwrap();
            let check = |condition: bool, message: &str| {
                if !condition {
                    observed.borrow_mut().push(message.into());
                }
            };
            let style = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } as u32;
            check(
                style & WS_EX_NOACTIVATE.0 != 0,
                "creation must set WS_EX_NOACTIVATE",
            );
            check(
                style & WS_EX_TOOLWINDOW.0 != 0,
                "a nonactivating popup must be a tool window",
            );
            check(
                style & WS_EX_APPWINDOW.0 == 0,
                "a pin must not get an application taskbar button",
            );
            let result = unsafe {
                SendMessageW(
                    handle,
                    WM_MOUSEACTIVATE,
                    Some(WPARAM(0)),
                    Some(LPARAM(HTCLIENT as isize)),
                )
            };
            check(
                result.0 == MA_NOACTIVATE as isize,
                "the production message handler must reject mouse activation",
            );
            check(
                unsafe { GetForegroundWindow() } == foreground,
                "hidden creation changed the foreground window",
            );
            check(
                !unsafe { IsWindowVisible(handle).as_bool() },
                "show=false must preserve hidden creation",
            );
            // Initial show and later explicit show are separate native code paths.
            let initially_shown = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            point(px(0.), px(0.)),
                            size(px(8.), px(8.)),
                        ))),
                        kind: WindowKind::PopUp,
                        titlebar: None,
                        is_activatable: false,
                        focus: true,
                        show: true,
                        is_resizable: false,
                        is_minimizable: false,
                        ..Default::default()
                    },
                    |_, cx| cx.new(|_| EmptyView),
                )
                .unwrap();
            let shown_handle = initially_shown
                .update(cx, |_, window, _| native_handle(window))
                .unwrap();
            check(
                unsafe { IsWindowVisible(shown_handle).as_bool() },
                "show=true must display the nonactivating window",
            );
            check(
                unsafe { GetForegroundWindow() } == foreground,
                "initial show stole focus",
            );

            // Negative control through the same production handler: the default
            // policy must still request activation when clicked. Sending this
            // query alone does not perform an actual click or change focus.
            let ordinary = cx
                .open_window(
                    WindowOptions {
                        focus: false,
                        show: false,
                        ..Default::default()
                    },
                    |_, cx| cx.new(|_| EmptyView),
                )
                .unwrap();
            let ordinary_handle = ordinary
                .update(cx, |_, window, _| native_handle(window))
                .unwrap();
            let normal_result = unsafe {
                SendMessageW(
                    ordinary_handle,
                    WM_MOUSEACTIVATE,
                    Some(WPARAM(0)),
                    Some(LPARAM(HTCLIENT as isize)),
                )
            };
            check(
                normal_result.0 == MA_ACTIVATE as isize,
                "ordinary windows lost mouse activation",
            );
            ordinary
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            popup
                .update(cx, |_, window, _| window.activate_window())
                .unwrap();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                cx.update(|cx| {
                    if unsafe { GetForegroundWindow() } != foreground {
                        observed
                            .borrow_mut()
                            .push("explicit activate_window stole focus".into());
                    }
                    if !unsafe { IsWindowVisible(handle).as_bool() } {
                        observed
                            .borrow_mut()
                            .push("activate_window must still show the nonactivating popup".into());
                    }
                    popup
                        .update(cx, |_, window, _| window.remove_window())
                        .unwrap();
                    initially_shown
                        .update(cx, |_, window, _| window.remove_window())
                        .unwrap();
                    cx.quit();
                });
            })
            .detach();
        },
    );
    assert!(failures.borrow().is_empty(), "{:?}", failures.borrow());
}

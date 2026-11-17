//! The only GPUI/Win32 boundary for pin chrome, placement and explicit file operations.
use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    UI::{
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::*,
    },
};

pub(crate) fn origin(window: &Window) -> Option<(i32, i32)> {
    let mut rect = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    (unsafe { GetWindowRect(hwnd(window)?, &mut rect) } != 0).then_some((rect.left, rect.top))
}

pub(crate) fn round_screenshot(window: &Window, dimensions: (i32, i32), radius: i32) {
    use windows_sys::Win32::Graphics::Gdi::{DeleteObject, SetWindowRgn};
    let Some(hwnd) = hwnd(window) else { return };
    unsafe {
        if radius <= 0 {
            SetWindowRgn(hwnd, std::ptr::null_mut(), 0);
            return;
        }
        let region = screenshot_region(dimensions, radius);
        if !region.is_null() && SetWindowRgn(hwnd, region, 0) == 0 {
            DeleteObject(region);
        }
        // On success Windows owns and releases the HRGN.
    }
}

// Preserve every pixel that can receive partial curve coverage. A centre-only
// binary clip would cut the antialiased outer hairline again.
fn screenshot_region(
    dimensions: (i32, i32),
    radius: i32,
) -> windows_sys::Win32::Graphics::Gdi::HRGN {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateRectRgn, ExtCreateRegion, RDH_RECTANGLES, RGNDATAHEADER,
    };
    unsafe {
        let (width, height) = dimensions;
        let radius = radius.min(width / 2).min(height / 2);
        if radius <= 0 {
            return CreateRectRgn(0, 0, width, height);
        }
        let mut rows = Vec::<RECT>::with_capacity((2 * radius + 1) as usize);
        let cut = |y: i32| {
            yue_geometry::selection_shape::boundary_inset(f64::from(y + 1), f64::from(radius))
                .floor()
                .max(0.) as i32
        };
        for y in 0..radius {
            rows.push(RECT {
                left: cut(y),
                top: y,
                right: width - cut(y),
                bottom: y + 1,
            });
        }
        if height > 2 * radius {
            rows.push(RECT {
                left: 0,
                top: radius,
                right: width,
                bottom: height - radius,
            });
        }
        for y in (0..radius).rev() {
            rows.push(RECT {
                left: cut(y),
                top: height - y - 1,
                right: width - cut(y),
                bottom: height - y,
            });
        }
        let header_size = std::mem::size_of::<RGNDATAHEADER>();
        let row_size = std::mem::size_of_val(rows.as_slice());
        let bytes = header_size + row_size;
        let header = RGNDATAHEADER {
            dwSize: header_size as u32,
            iType: RDH_RECTANGLES,
            nCount: rows.len() as u32,
            nRgnSize: row_size as u32,
            rcBound: RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
        };
        let mut data = vec![header; bytes.div_ceil(header_size)];
        std::ptr::copy_nonoverlapping(
            rows.as_ptr().cast::<u8>(),
            data.as_mut_ptr().cast::<u8>().add(header_size),
            row_size,
        );
        ExtCreateRegion(std::ptr::null(), bytes as u32, data.as_ptr().cast())
    }
}

pub(crate) fn wheel_delta(event: &gpui_kit::ScrollWheelEvent) -> i32 {
    let mut lines = 3_u32;
    unsafe {
        SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            (&mut lines as *mut u32).cast(),
            0,
        );
    }
    match event.delta {
        // GPUI's Windows backend multiplies notches by the OS scroll setting.
        // Undo that preference: image zoom is one tenth per physical notch.
        gpui_kit::ScrollDelta::Lines(delta) => {
            (delta.y * 120. / lines.max(1) as f32).round() as i32
        }
        gpui_kit::ScrollDelta::Pixels(delta) => f32::from(delta.y).round() as i32,
    }
}

pub(crate) fn image_menu(hwnd: usize, hdr: bool, text: bool) -> u32 {
    unsafe {
        release_hover_capture(hwnd as HWND);
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return 0;
        }
        for (id, label) in [
            (1, "复制图片（SDR）"),
            (2, "复制 HDR 图片（PNG）"),
            (3, "查看文字备选…"),
            (4, "关闭"),
        ] {
            if (id == 2 && !hdr) || (id == 3 && !text) {
                continue;
            }
            let wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
            AppendMenuW(menu, MF_STRING, id, wide.as_ptr());
        }
        let cursor = cursor_position();
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            cursor.0,
            cursor.1,
            0,
            hwnd as HWND,
            std::ptr::null(),
        ) as u32;
        DestroyMenu(menu);
        command
    }
}

pub(crate) fn hover_capture(window: &Window, hovered: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, SetCapture};
    if let Some(hwnd) = hwnd(window) {
        unsafe {
            if hovered && GetCapture().is_null() {
                SetCapture(hwnd);
            } else if !hovered {
                release_hover_capture(hwnd);
            }
        }
    }
}
unsafe fn release_hover_capture(hwnd: HWND) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture};
    unsafe {
        if GetCapture() == hwnd {
            ReleaseCapture();
        }
    }
}

pub(crate) fn hwnd(window: &Window) -> Option<HWND> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
        _ => None,
    }
}
pub(crate) fn cursor_position() -> (i32, i32) {
    let mut cursor = POINT { x: 0, y: 0 };
    unsafe {
        GetCursorPos(&mut cursor);
    }
    (cursor.x, cursor.y)
}
pub(crate) fn start_move(window: &mut Window) {
    // A headless window has no native surface to move. Keep the pointer routing
    // contract testable without invoking the test backend's unsupported syscall.
    if hwnd(window).is_some() {
        window.start_window_move();
    }
}
pub(crate) fn work_area(point: (i32, i32)) -> ((i32, i32, i32, i32), f32) {
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: point.0,
                y: point.1,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let mut x = 96;
        let mut y = 96;
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            let r = info.rcWork;
            ((r.left, r.top, r.right, r.bottom), x as f32 / 96.)
        } else {
            ((0, 0, 1920, 1080), 1.)
        }
    }
}
pub(crate) fn clamp_position(
    cursor: (i32, i32),
    size: (i32, i32),
    area: (i32, i32, i32, i32),
) -> (i32, i32) {
    let (l, t, r, b) = area;
    let (w, h) = size;
    let x = if cursor.0.saturating_add(16).saturating_add(w) <= r {
        cursor.0.saturating_add(16)
    } else {
        cursor.0.saturating_sub(w).saturating_sub(16)
    };
    let y = if cursor.1.saturating_add(16).saturating_add(h) <= b {
        cursor.1.saturating_add(16)
    } else {
        cursor.1.saturating_sub(h).saturating_sub(16)
    };
    (
        x.clamp(l, r.saturating_sub(w).max(l)),
        y.clamp(t, b.saturating_sub(h).max(t)),
    )
}
pub(crate) fn configure(window: &Window, cursor: (i32, i32), logical: (f32, f32), topmost: bool) {
    let Some(hwnd) = hwnd(window) else {
        return;
    };
    let (area, scale) = work_area(cursor);
    let w = (logical.0 * scale)
        .round()
        .min((area.2 - area.0) as f32)
        .max(1.) as i32;
    let h = (logical.1 * scale)
        .round()
        .min((area.3 - area.1) as f32)
        .max(1.) as i32;
    let position = clamp_position(cursor, (w, h), area);
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            (ex | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize),
        );
        SetWindowPos(
            hwnd,
            if topmost {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            },
            position.0,
            position.1,
            w,
            h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
        );
        let corner: u32 = 2;
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd,
            33,
            (&corner as *const u32).cast(),
            4,
        );
    }
}
pub(crate) fn topmost(window: &Window, on: bool) {
    if let Some(hwnd) = hwnd(window) {
        unsafe {
            SetWindowPos(
                hwnd,
                if on { HWND_TOPMOST } else { HWND_NOTOPMOST },
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}
pub(crate) fn visible(window: &Window, on: bool) {
    if let Some(hwnd) = hwnd(window) {
        unsafe {
            ShowWindow(hwnd, if on { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }
}
pub(crate) fn raise(window: &Window) {
    if let Some(hwnd) = hwnd(window) {
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}
pub(crate) fn open(path: &str, reveal: bool) -> Result<(), String> {
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let verb = wide("open");
    let (file, args) = if reveal {
        (wide("explorer.exe"), wide(&format!("/select,\"{}\"", path)))
    } else {
        (wide(path), vec![0])
    };
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            args.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err("无法打开该项目，请检查文件或系统关联程序".into())
    }
}

#[cfg(test)]
mod tests {
    use super::clamp_position;

    #[test]
    fn screenshot_region_preserves_partial_hairline_pixels_and_drops_empty_corners() {
        use windows_sys::Win32::Graphics::Gdi::{DeleteObject, PtInRegion};
        for (width, height, radius) in [
            (3, 3, 1),
            (66, 34, 9),
            (130, 66, 17),
            (642, 322, 81),
            (2582, 2582, 1282),
        ] {
            let region = super::screenshot_region((width, height), radius);
            assert!(!region.is_null());
            for y in 0..radius {
                for x in 0..radius {
                    // Independent analytic test: any interior area of this pixel
                    // must survive the conservative binary HWND region.
                    let dx = (f64::from(radius - x - 1) / f64::from(radius)).max(0.);
                    let dy = (f64::from(radius - y - 1) / f64::from(radius)).max(0.);
                    let expected = dx.powi(4) + dy.powi(4) < 1.;
                    for (x, y) in [
                        (x, y),
                        (width - 1 - x, y),
                        (x, height - 1 - y),
                        (width - 1 - x, height - 1 - y),
                    ] {
                        assert_eq!(
                            unsafe { PtInRegion(region, x, y) } != 0,
                            expected,
                            "r={radius} at ({x},{y})"
                        );
                    }
                }
            }
            unsafe {
                DeleteObject(region);
            }
        }
    }

    #[test]
    fn placement_flips_and_clamps_on_negative_monitors() {
        assert_eq!(
            clamp_position((-20, -20), (400, 300), (-1920, -1080, 0, 0)),
            (-436, -336)
        );
        assert_eq!(
            clamp_position((-1919, -1079), (400, 300), (-1920, -1080, 0, 0)),
            (-1903, -1063)
        );
        assert_eq!(
            clamp_position((10, 10), (4000, 3000), (-1920, -1080, 0, 0)),
            (-1920, -1080)
        );
    }
}

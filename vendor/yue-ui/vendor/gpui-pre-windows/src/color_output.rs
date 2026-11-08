//! Event-owned Windows Advanced Color discovery. No polling or pixel I/O.
use anyhow::{Context as _, Result};
use gpui::{DisplayColorKind, WindowColorOutput};
use gpui_util::ResultExt;
use std::{
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::Display::{AdvancedColorKind, DisplayInformation},
    System::{DispatcherQueue, DispatcherQueueController},
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Graphics::{
            Dxgi::*,
            Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow},
        },
        System::WinRT::*,
        UI::WindowsAndMessaging::*,
    },
    core::{GUID, HRESULT, IInspectable_Vtbl, Interface},
};

// windows 0.62.2 omits this Win11 interop interface. Narrow ABI copied from
// Microsoft's windows.graphics.display.interop.h (IInspectable base, both slots).
// https://github.com/microsoft/win32metadata/blob/main/generation/WinSDK/RecompiledIdlHeaders/um/windows.graphics.display.interop.h
windows_core::imp::define_interface!(
    DisplayInformationInterop,
    DisplayInformationInteropVtbl,
    0x7449121c_382b_4705_8da7_a795ba482013
);
#[repr(C)]
pub struct DisplayInformationInteropVtbl {
    base: IInspectable_Vtbl,
    get_for_window: unsafe extern "system" fn(
        *mut std::ffi::c_void,
        HWND,
        *const GUID,
        *mut *mut std::ffi::c_void,
    ) -> HRESULT,
    get_for_monitor: unsafe extern "system" fn(
        *mut std::ffi::c_void,
        windows::Win32::Graphics::Gdi::HMONITOR,
        *const GUID,
        *mut *mut std::ffi::c_void,
    ) -> HRESULT,
}

pub(crate) struct DisplayQueueOwner(Option<DispatcherQueueController>);
impl DisplayQueueOwner {
    pub(crate) fn new() -> Result<Self> {
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }
            .context("Initializing main-thread WinRT")?;
        if DispatcherQueue::GetForCurrentThread().is_ok() {
            return Ok(Self(None));
        }
        let owner = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_NONE,
            })
        }
        .context("Creating main-thread display DispatcherQueue");
        let owner = match owner {
            Ok(owner) => owner,
            Err(error) => {
                unsafe {
                    RoUninitialize();
                }
                return Err(error);
            }
        };
        Ok(Self(Some(owner)))
    }
}
impl Drop for DisplayQueueOwner {
    fn drop(&mut self) {
        let _uninitialize = gpui_util::defer(|| unsafe { RoUninitialize() });
        // This current-thread queue needs its message pump to drain before COM
        // teardown. Never shut down a queue that another host created.
        let Some(owner) = self.0.take() else { return };
        let Some(action) = owner.ShutdownQueueAsync().log_err() else {
            return;
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut quit = None;
        while action.Status().is_ok_and(|status| status.0 == 0)
            && std::time::Instant::now() < deadline
        {
            unsafe {
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    if message.message == WM_QUIT {
                        quit = Some(message.wParam.0 as i32);
                        continue;
                    }
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                let _ = MsgWaitForMultipleObjectsEx(None, 10, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            }
        }
        if action.Status().is_ok_and(|status| status.0 == 0) {
            log::warn!("Display DispatcherQueue did not finish shutdown within 2 seconds");
        } else {
            action.GetResults().log_err();
        }
        if let Some(code) = quit {
            unsafe {
                PostQuitMessage(code);
            }
        }
    }
}

pub(crate) struct DisplayWatcher {
    information: DisplayInformation,
    token: i64,
    alive: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
}
impl DisplayWatcher {
    pub(crate) fn new(hwnd: HWND) -> Result<Self> {
        // Runtime activation/QI is the OS version gate; no unguarded GetForWindow
        // import that would prevent an older Windows process from starting.
        let interop = windows::core::factory::<DisplayInformation, DisplayInformationInterop>()?;
        let mut raw = std::ptr::null_mut();
        let information: DisplayInformation = unsafe {
            (interop.vtable().get_for_window)(
                interop.as_raw(),
                hwnd,
                &DisplayInformation::IID,
                &mut raw,
            )
            .ok()?;
            Interface::from_raw(raw)
        };
        let alive = Arc::new(AtomicBool::new(true));
        let pending = Arc::new(AtomicBool::new(false));
        let (active, queued) = (alive.clone(), pending.clone());
        let handle = hwnd.0 as isize;
        let handler = TypedEventHandler::new(move |_, _| {
            if active.load(Ordering::Acquire) && !queued.swap(true, Ordering::AcqRel) {
                let result = unsafe {
                    PostMessageW(
                        Some(HWND(handle as _)),
                        crate::events::WM_GPUI_COLOR_CHANGED,
                        WPARAM(0),
                        LPARAM(0),
                    )
                };
                if result.is_err() {
                    queued.store(false, Ordering::Release);
                }
            }
            Ok(())
        });
        let token = information.AdvancedColorInfoChanged(&handler)?;
        Ok(Self {
            information,
            token,
            alive,
            pending,
        })
    }
    pub(crate) fn snapshot(&self, hwnd: HWND) -> Result<WindowColorOutput> {
        self.pending.store(false, Ordering::Release);
        let info = self.information.GetAdvancedColorInfo()?;
        let display_kind = match info.CurrentAdvancedColorKind()? {
            AdvancedColorKind::StandardDynamicRange => DisplayColorKind::Sdr,
            AdvancedColorKind::WideColorGamut => DisplayColorKind::WideColor,
            AdvancedColorKind::HighDynamicRange => DisplayColorKind::Hdr,
            _ => DisplayColorKind::Unknown,
        };
        let mut peak = info
            .MaxLuminanceInNits()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.);
        // Fresh factory + same HMONITOR, including a monitor on another adapter.
        // A renderer's GetContainingOutput alone is not enough on hybrid GPUs.
        if peak.is_none() {
            peak = dxgi_peak(hwnd);
        }
        Ok(WindowColorOutput {
            display_id: Some(crate::WindowsDisplay::display_id_for_monitor(unsafe {
                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
            })),
            display_kind,
            hdr_supported: info
                .IsAdvancedColorKindAvailable(AdvancedColorKind::HighDynamicRange)
                .ok(),
            sdr_white_nits: info
                .SdrWhiteLevelInNits()
                .ok()
                .filter(|v| v.is_finite() && *v > 0.),
            max_luminance_nits: peak,
            ..Default::default()
        })
    }
}
impl Drop for DisplayWatcher {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        self.information
            .RemoveAdvancedColorInfoChanged(self.token)
            .log_err();
    }
}

fn dxgi_peak(hwnd: HWND) -> Option<f32> {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            let mut j = 0;
            while let Ok(output) = adapter.EnumOutputs(j) {
                if let Ok(output) = output.cast::<IDXGIOutput6>() {
                    if let Ok(desc) = output.GetDesc1() {
                        if desc.Monitor == monitor {
                            return (desc.MaxLuminance.is_finite() && desc.MaxLuminance > 0.)
                                .then_some(desc.MaxLuminance);
                        }
                    }
                }
                j += 1;
            }
            i += 1;
        }
    }
    None
}

/// Pure policy gate. A missing absolute scale must not produce a guessed HDR.
pub(crate) fn requested_linear_mode(snapshot: &WindowColorOutput) -> Result<gpui::WindowColorMode> {
    match snapshot.display_kind {
        DisplayColorKind::Hdr => {
            anyhow::ensure!(
                snapshot
                    .sdr_white_nits
                    .is_some_and(|v| v.is_finite() && v > 0.),
                "HDR SDR white is unknown"
            );
            anyhow::ensure!(
                snapshot
                    .max_luminance_nits
                    .is_some_and(|v| v.is_finite() && v > 0.),
                "HDR peak is unknown"
            );
            Ok(gpui::WindowColorMode::HdrScRgb)
        }
        DisplayColorKind::Sdr | DisplayColorKind::WideColor => Ok(gpui::WindowColorMode::LinearSdr),
        DisplayColorKind::Unknown => anyhow::bail!("Windows Advanced Color state is unavailable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::test;
    #[test]
    fn color_policy_requires_actual_state_and_absolute_luminance() {
        let mut s = WindowColorOutput {
            hdr_supported: Some(true),
            ..Default::default()
        };
        assert!(requested_linear_mode(&s).is_err());
        s.display_kind = DisplayColorKind::Sdr;
        assert_eq!(
            requested_linear_mode(&s).unwrap(),
            gpui::WindowColorMode::LinearSdr
        );
        s.display_kind = DisplayColorKind::WideColor;
        assert_eq!(
            requested_linear_mode(&s).unwrap(),
            gpui::WindowColorMode::LinearSdr
        );
        s.display_kind = DisplayColorKind::Hdr;
        assert!(requested_linear_mode(&s).is_err());
        s.sdr_white_nits = Some(203.);
        assert!(requested_linear_mode(&s).is_err());
        s.max_luminance_nits = Some(400.);
        assert_eq!(
            requested_linear_mode(&s).unwrap(),
            gpui::WindowColorMode::HdrScRgb
        );
        s.sdr_white_nits = Some(f32::NAN);
        assert!(requested_linear_mode(&s).is_err());
    }
}

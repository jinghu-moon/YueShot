//! Immutable monitor color snapshots. This adapter never changes display settings
//! or initializes COM on the WGC worker.

use windows::Win32::{
    Devices::Display::*,
    Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
    Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFOEXW},
};
use windows::{
    Win32::Graphics::Dxgi::Common::{
        DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
        DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
    },
    Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput6},
    core::Interface,
};
use yue_media::pixels::{AlphaMode, PixelDescriptor, PixelEncoding};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColorTarget {
    adapter: u64,
    id: u32,
    // Advanced Color includes wide-gamut SDR. Never call this flag "HDR".
    advanced: Option<bool>,
    white_level: Option<u32>,
}
impl ColorTarget {
    fn new(adapter: u64, id: u32, advanced: Option<bool>, white_level: Option<u32>) -> Self {
        Self {
            adapter,
            id,
            advanced,
            white_level: white_level.filter(|v| *v > 0),
        }
    }
}

/// A value frozen at acquisition, separate from output state at display time.
/// Equality includes target identity, color state and the absolute white scale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorSnapshot {
    monitor: isize,
    targets: Vec<ColorTarget>,
    desktop_color_space: Option<i32>,
    max_luminance_millinits: Option<u32>,
    max_full_frame_luminance_millinits: Option<u32>,
}
impl ColorSnapshot {
    fn new(monitor: isize, mut targets: Vec<ColorTarget>) -> Self {
        targets.sort_by_key(|t| (t.adapter, t.id));
        Self {
            monitor,
            targets,
            desktop_color_space: None,
            max_luminance_millinits: None,
            max_full_frame_luminance_millinits: None,
        }
    }

    pub(crate) fn unknown(monitor: isize) -> Self {
        Self::new(monitor, Vec::new())
    }

    /// Only a complete, confirmed SDR source permits eight-bit acquisition or
    /// an automatic BitBlt fallback. Unknown capabilities are not SDR.
    pub fn allows_sdr_fallback(&self) -> bool {
        self.hdr_active() == Some(false)
            && !self.targets.is_empty()
            && self.targets.iter().all(|t| t.advanced == Some(false))
    }

    pub fn capture_descriptor(&self) -> PixelDescriptor {
        if self.allows_sdr_fallback() {
            PixelDescriptor::bgra8()
        } else {
            PixelDescriptor::new(
                yue_media::PixelFormat::Rgba16Float,
                PixelEncoding::ScRgb,
                AlphaMode::Straight,
            )
            .expect("the WGC scRGB descriptor is valid")
        }
    }

    /// Confirm the desktop's output mode independently of the Advanced Color
    /// flag, which can also mean wide-gamut SDR. Unknown is never HDR evidence.
    pub fn hdr_active(&self) -> Option<bool> {
        match self.desktop_color_space {
            Some(space)
                if space == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020.0
                    || space == DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709.0 =>
            {
                Some(true)
            }
            Some(space) if space == DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709.0 => Some(false),
            _ => None,
        }
    }

    /// Microsoft DISPLAYCONFIG_SDR_WHITE_LEVEL: 1000 means 80 nit. A missing
    /// value or disagreeing cloned outputs cannot establish one output white.
    pub fn sdr_white_nits(&self) -> Option<f32> {
        let first = self.targets.first()?.white_level?;
        self.targets
            .iter()
            .all(|t| t.white_level == Some(first))
            .then_some(first as f32 * 80.0 / 1000.0)
    }

    /// DXGI output peak for small highlights. This describes the display, not
    /// the captured image, and must not be used as an SDR export source peak.
    pub fn max_luminance_nits(&self) -> Option<f32> {
        self.max_luminance_millinits
            .map(|value| value as f32 / 1000.0)
    }

    /// DXGI full-frame peak, distinct from the small-highlight peak.
    pub fn max_full_frame_luminance_nits(&self) -> Option<f32> {
        self.max_full_frame_luminance_millinits
            .map(|value| value as f32 / 1000.0)
    }

    pub fn query(monitor: isize) -> Result<Self, String> {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !unsafe { GetMonitorInfoW(HMONITOR(monitor as _), &mut info.monitorInfo) }.as_bool() {
            return Err("GetMonitorInfoW failed while resolving display color".into());
        }
        let name = utf16_name(&info.szDevice);
        let paths = active_paths()?;
        let mut targets = Vec::new();
        for path in paths {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(),
                    path.sourceInfo.adapterId,
                    path.sourceInfo.id,
                ),
                ..Default::default()
            };
            let status = unsafe { DisplayConfigGetDeviceInfo(&mut source.header) };
            if status != 0 {
                // An unidentified active source could be this monitor. Refuse a
                // falsely complete snapshot rather than skipping it.
                return Err(format!("DisplayConfig source name failed: {status}"));
            }
            if utf16_name(&source.viewGdiDeviceName) != name {
                continue;
            }
            let mut color = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                    std::mem::size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>(),
                    path.targetInfo.adapterId,
                    path.targetInfo.id,
                ),
                ..Default::default()
            };
            let advanced = (unsafe { DisplayConfigGetDeviceInfo(&mut color.header) } == 0)
                .then_some(unsafe { color.Anonymous.value } & 2 != 0);
            let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                    std::mem::size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>(),
                    path.targetInfo.adapterId,
                    path.targetInfo.id,
                ),
                ..Default::default()
            };
            let white_level = (unsafe { DisplayConfigGetDeviceInfo(&mut white.header) } == 0)
                .then_some(white.SDRWhiteLevel);
            let adapter = path.targetInfo.adapterId;
            let adapter = (u64::from(adapter.HighPart as u32) << 32) | u64::from(adapter.LowPart);
            targets.push(ColorTarget::new(
                adapter,
                path.targetInfo.id,
                advanced,
                white_level,
            ));
        }
        if targets.is_empty() {
            return Err("the capture monitor has no active display path".into());
        }
        let mut snapshot = Self::new(monitor, targets);
        if let Some(output) = desktop_output(monitor) {
            snapshot.desktop_color_space = Some(output.ColorSpace.0);
            snapshot.max_luminance_millinits = luminance_millinits(output.MaxLuminance);
            snapshot.max_full_frame_luminance_millinits =
                luminance_millinits(output.MaxFullFrameLuminance);
        }
        Ok(snapshot)
    }
}

fn luminance_millinits(value: f32) -> Option<u32> {
    (value.is_finite() && value > 0.0 && value <= 10_000.0).then(|| (value * 1000.0).round() as u32)
}

fn desktop_output(monitor: isize) -> Option<windows::Win32::Graphics::Dxgi::DXGI_OUTPUT_DESC1> {
    // A fresh factory observes current HDR state, including a display on a
    // different adapter. Failure remains unknown; it does not authorize SDR.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    for adapter_ix in 0..64 {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(adapter_ix) }) else {
            break;
        };
        for output_ix in 0..256 {
            let Ok(output) = (unsafe { adapter.EnumOutputs(output_ix) }) else {
                break;
            };
            let Some(description) = output
                .cast::<IDXGIOutput6>()
                .ok()
                .and_then(|output| unsafe { output.GetDesc1() }.ok())
            else {
                continue;
            };
            if description.Monitor.0 as isize == monitor {
                return Some(description);
            }
        }
    }
    None
}

fn header(
    kind: DISPLAYCONFIG_DEVICE_INFO_TYPE,
    size: usize,
    adapter: windows::Win32::Foundation::LUID,
    id: u32,
) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: kind,
        size: size as u32,
        adapterId: adapter,
        id,
    }
}

fn utf16_name(name: &[u16]) -> &[u16] {
    &name[..name.iter().position(|v| *v == 0).unwrap_or(name.len())]
}

fn active_paths() -> Result<Vec<DISPLAYCONFIG_PATH_INFO>, String> {
    // Topology can change between buffer sizing and the actual query. Retry a
    // bounded number of times; never keep a partly populated path array.
    for _ in 0..3 {
        let (mut paths, mut modes) = (0, 0);
        let status =
            unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths, &mut modes) };
        if status != ERROR_SUCCESS {
            return Err(format!("GetDisplayConfigBufferSizes: {}", status.0));
        }
        if paths > 4096 || modes > 16384 {
            return Err("display topology exceeds the query budget".into());
        }
        let mut path_values = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
        let mut mode_values = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
        let status = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut paths,
                path_values.as_mut_ptr(),
                &mut modes,
                mode_values.as_mut_ptr(),
                None,
            )
        };
        if status == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if status != ERROR_SUCCESS {
            return Err(format!("QueryDisplayConfig: {}", status.0));
        }
        path_values.truncate(paths as usize);
        return Ok(path_values);
    }
    Err("display topology kept changing during the color query".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use yue_media::pixels::PixelEncoding;

    #[test]
    #[ignore = "reads current display capabilities only; requires a live desktop, does not capture or change settings"]
    fn query_live_monitor_color_without_com_or_capture() {
        let monitor = crate::windows::monitor::captured_monitor_at_cursor().unwrap();
        let snapshot = ColorSnapshot::query(monitor.handle).unwrap();
        assert!(!snapshot.targets.is_empty());
        log::debug!("monitor color snapshot: {snapshot:?}");
        let again = ColorSnapshot::query(monitor.handle).unwrap();
        assert_eq!(
            snapshot, again,
            "display state changed during the read-only probe"
        );
    }

    fn target(id: u32, advanced: Option<bool>, white: Option<u32>) -> ColorTarget {
        ColorTarget::new(7, id, advanced, white)
    }

    fn with_luminance(
        mut snapshot: ColorSnapshot,
        peak: Option<f32>,
        full: Option<f32>,
    ) -> ColorSnapshot {
        snapshot.max_luminance_millinits = peak.and_then(luminance_millinits);
        snapshot.max_full_frame_luminance_millinits = full.and_then(luminance_millinits);
        snapshot
    }

    fn confirmed_sdr(mut snapshot: ColorSnapshot) -> ColorSnapshot {
        snapshot.desktop_color_space = Some(DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709.0);
        snapshot
    }

    #[test]
    fn advanced_color_alone_does_not_prove_hdr_and_output_changes_invalidate_a_snapshot() {
        let mut snapshot = ColorSnapshot::new(42, vec![target(1, Some(true), Some(1000))]);
        assert_eq!(snapshot.hdr_active(), None);
        let unknown = snapshot.clone();
        snapshot.desktop_color_space = Some(DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709.0);
        assert_eq!(snapshot.hdr_active(), Some(false));
        assert_ne!(snapshot, unknown);
        let sdr = snapshot.clone();
        snapshot.desktop_color_space = Some(DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709.0);
        assert_eq!(snapshot.hdr_active(), Some(true));
        assert_ne!(snapshot, sdr);
        snapshot.desktop_color_space = Some(DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020.0);
        assert_eq!(snapshot.hdr_active(), Some(true));
        assert_ne!(snapshot, sdr);
        assert_eq!(
            snapshot.capture_descriptor().encoding(),
            PixelEncoding::ScRgb
        );
        // Conflicting capability snapshots must choose precision, never BitBlt.
        snapshot.targets[0].advanced = Some(false);
        assert!(!snapshot.allows_sdr_fallback());
        assert_eq!(
            snapshot.capture_descriptor().encoding(),
            PixelEncoding::ScRgb
        );
        snapshot.desktop_color_space = Some(-1);
        assert_eq!(snapshot.hdr_active(), None);
    }

    #[test]
    fn only_confirmed_sdr_can_use_eight_bits() {
        let sdr = confirmed_sdr(ColorSnapshot::new(42, vec![target(1, Some(false), None)]));
        assert_eq!(sdr.capture_descriptor(), PixelDescriptor::bgra8());
        for targets in [
            vec![],
            vec![target(1, None, None)],
            vec![target(1, Some(true), Some(2000))],
            vec![
                target(1, Some(false), None),
                target(2, Some(true), Some(1000)),
            ],
        ] {
            let snapshot = confirmed_sdr(ColorSnapshot::new(42, targets));
            assert_eq!(
                snapshot.capture_descriptor().encoding(),
                PixelEncoding::ScRgb
            );
            assert!(!snapshot.allows_sdr_fallback());
        }
        assert!(sdr.allows_sdr_fallback());
        let unknown = ColorSnapshot::new(42, vec![target(1, Some(false), None)]);
        assert!(!unknown.allows_sdr_fallback());
        assert_eq!(
            unknown.capture_descriptor().encoding(),
            PixelEncoding::ScRgb
        );
    }

    #[test]
    fn white_is_verified_and_clone_targets_must_agree() {
        let snapshot = |targets| confirmed_sdr(ColorSnapshot::new(42, targets));
        assert_eq!(
            snapshot(vec![target(1, Some(true), Some(1000))]).sdr_white_nits(),
            Some(80.0)
        );
        assert_eq!(
            snapshot(vec![target(1, Some(true), Some(2500))]).sdr_white_nits(),
            Some(200.0)
        );
        for targets in [
            vec![],
            vec![target(1, None, None)],
            vec![target(1, Some(true), Some(0))],
            vec![
                target(1, Some(true), Some(1000)),
                target(2, Some(true), None),
            ],
            vec![
                target(1, Some(true), Some(1000)),
                target(2, Some(true), Some(2000)),
            ],
        ] {
            assert_eq!(snapshot(targets).sdr_white_nits(), None);
        }
    }

    #[test]
    fn topology_order_is_not_a_change_but_white_identity_and_state_are() {
        let first = confirmed_sdr(ColorSnapshot::new(
            42,
            vec![
                target(1, Some(true), Some(1000)),
                target(2, Some(true), Some(1000)),
            ],
        ));
        assert_eq!(
            first,
            confirmed_sdr(ColorSnapshot::new(
                42,
                vec![
                    target(2, Some(true), Some(1000)),
                    target(1, Some(true), Some(1000))
                ]
            ))
        );
        for changed in [
            confirmed_sdr(ColorSnapshot::new(43, first.targets.clone())),
            confirmed_sdr(ColorSnapshot::new(
                42,
                vec![
                    target(1, Some(true), Some(2000)),
                    target(2, Some(true), Some(1000)),
                ],
            )),
            confirmed_sdr(ColorSnapshot::new(
                42,
                vec![target(1, Some(false), Some(1000))],
            )),
            confirmed_sdr(ColorSnapshot::new(
                42,
                vec![target(3, Some(true), Some(1000))],
            )),
        ] {
            assert_ne!(first, changed);
        }
    }

    #[test]
    fn white_and_display_peak_combinations_are_frozen_without_changing_source_policy() {
        let snapshot = |white, peak, full| {
            with_luminance(
                confirmed_sdr(ColorSnapshot::new(
                    42,
                    vec![target(1, Some(true), Some(white))],
                )),
                peak,
                full,
            )
        };
        let baseline = snapshot(1000, Some(400.0), Some(250.0));
        assert_eq!(baseline.sdr_white_nits(), Some(80.0));
        assert_eq!(baseline.max_luminance_nits(), Some(400.0));
        assert_eq!(baseline.max_full_frame_luminance_nits(), Some(250.0));

        let matrix = [
            snapshot(1000, Some(700.0), Some(300.0)),
            snapshot(2000, Some(400.0), Some(250.0)),
            snapshot(2000, Some(700.0), Some(300.0)),
        ];
        for changed in matrix {
            assert_ne!(
                baseline, changed,
                "white/peak change must invalidate acquisition snapshot"
            );
        }
        let unknown = snapshot(1000, None, Some(f32::NAN));
        assert_eq!(unknown.max_luminance_nits(), None);
        assert_eq!(unknown.max_full_frame_luminance_nits(), None);
        assert_ne!(baseline, unknown);
    }
}

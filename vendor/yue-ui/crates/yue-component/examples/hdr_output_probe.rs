//! Read-only system evidence. Does not open a window or enable/disable HDR.
//! A successful probe is not proof that an application swap chain presents HDR.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::mem::size_of;
    use windows::Win32::{Devices::Display::*, Graphics::Dxgi::*};
    use windows::core::Interface;
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut adapter_index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
            let adapter_desc = adapter.GetDesc1()?;
            let mut output_index = 0;
            while let Ok(output) = adapter.EnumOutputs(output_index) {
                let desc = output.cast::<IDXGIOutput6>()?.GetDesc1()?;
                println!(
                    "adapter={} output={} name={} desktop={:?} colorspace={:?} bits={} min_nits={} max_nits={} full_frame_nits={}",
                    String::from_utf16_lossy(&adapter_desc.Description).trim_end_matches('\0'),
                    output_index,
                    String::from_utf16_lossy(&desc.DeviceName).trim_end_matches('\0'),
                    desc.DesktopCoordinates,
                    desc.ColorSpace,
                    desc.BitsPerColor,
                    desc.MinLuminance,
                    desc.MaxLuminance,
                    desc.MaxFullFrameLuminance
                );
                output_index += 1;
            }
            adapter_index += 1;
        }
        // Query an internally coherent active path list; hot-plug may change
        // required buffer sizes between these two calls, so retry that case.
        let mut complete = false;
        for _ in 0..3 {
            let (mut path_count, mut mode_count) = (0, 0);
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
                .ok()?;
            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
            let result = QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            );
            if result == windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            result.ok()?;
            for path in &paths[..path_count as usize] {
                let header = |kind, size| DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: kind,
                    size,
                    adapterId: path.targetInfo.adapterId,
                    id: path.targetInfo.id,
                };
                let mut color = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
                    header: header(
                        DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                        size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32,
                    ),
                    ..Default::default()
                };
                let status = DisplayConfigGetDeviceInfo(&mut color.header);
                let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
                    header: header(
                        DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                        size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32,
                    ),
                    ..Default::default()
                };
                let white_status = DisplayConfigGetDeviceInfo(&mut white.header);
                println!(
                    "target={} advanced_color_status={} flags={:#x} bits={} white_status={} white_nits={:?}",
                    path.targetInfo.id,
                    status,
                    color.Anonymous.value,
                    color.bitsPerColorChannel,
                    white_status,
                    (white_status == 0).then_some(white.SDRWhiteLevel as f64 * 80.0 / 1000.0)
                );
            }
            complete = true;
            break;
        }
        if !complete {
            return Err("display topology changed during all probe attempts".into());
        }
    }
    println!("No swap chain was created; PRESENT support and SetColorSpace1 were not tested.");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("HDR display probe is Windows-only");
}

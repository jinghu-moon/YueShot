//! Production HLSL on WARP. The separately ignored native test creates only a
//! hidden HWND and never changes the display configuration.
use super::*;
use half::f16;
use std::prelude::v1::test;
use windows::Win32::Foundation::HMODULE;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/yue-component/tests/fixtures/hdr/sdr-vectors.rs"
));

#[test]
fn sdr_gpu_matches_the_same_independent_vectors_as_cpu_export() {
    for &(rgb, _, expected) in SDR_MAPPING_VECTORS {
        let rgb = rgb.map(|v| v as f32);
        let pixel = [rgb[0], rgb[1], rgb[2], 1.];
        let output = render([pixel; 2], false, 1., 100., 2, true, [0.; 4]);
        for (actual, linear) in output[3][..3].iter().zip(expected) {
            let encoded = if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1. / 2.4) - 0.055
            };
            // Final 8-bit quantization + F32 PQ/Hermite arithmetic; FP16 input
            // quantization is already part of the independent oracle.
            assert!(
                (f64::from(*actual) - encoded).abs() <= 0.006,
                "{rgb:?}: {output:?} != {expected:?}"
            );
        }
    }
}

fn texture(
    device: &ID3D11Device,
    width: u32,
    usage: D3D11_USAGE,
    format: DXGI_FORMAT,
) -> ID3D11Texture2D {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: 2,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: usage,
        BindFlags: if usage == D3D11_USAGE_STAGING {
            0
        } else {
            (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32
        },
        CPUAccessFlags: if usage == D3D11_USAGE_STAGING {
            D3D11_CPU_ACCESS_READ.0 as u32
        } else {
            0
        },
        ..Default::default()
    };
    let mut output = None;
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut output))
            .unwrap();
    }
    output.unwrap()
}

fn render(
    input: [[f32; 4]; 2],
    hdr: bool,
    sdr_white: f32,
    peak: f32,
    encoding: u32,
    nearest: bool,
    background: [f32; 4],
) -> Vec<[f32; 4]> {
    render_mode(
        input,
        if hdr { 1 } else { 0 },
        sdr_white,
        peak,
        encoding,
        nearest,
        background,
    )
}

fn render_mode(
    input: [[f32; 4]; 2],
    mode: u32,
    sdr_white: f32,
    peak: f32,
    encoding: u32,
    nearest: bool,
    background: [f32; 4],
) -> Vec<[f32; 4]> {
    let hdr = mode != 0;
    let mut device = None;
    let mut context = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_WARP,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .unwrap();
    }
    let device = device.unwrap();
    let context = context.unwrap();
    set_rasterizer_state(&device, &context).unwrap();
    let target = texture(
        &device,
        8,
        D3D11_USAGE_DEFAULT,
        if hdr {
            DXGI_FORMAT_R16G16B16A16_FLOAT
        } else {
            DXGI_FORMAT_B8G8R8A8_UNORM
        },
    );
    let mut rtv = None;
    let globals = DirectXGlobalElements::new(&device).unwrap();
    update_buffer(
        &context,
        globals.global_params_buffer.as_ref().unwrap(),
        &[GlobalParams {
            viewport_size: [8., 2.],
            output_hdr: mode,
            sdr_white,
            output_peak: peak,
            ..Default::default()
        }],
    )
    .unwrap();
    update_buffer(
        &context,
        globals.batch_params_buffer.as_ref().unwrap(),
        &[BatchParams::default()],
    )
    .unwrap();
    let bytes: Vec<u8> = input
        .iter()
        .cycle()
        .take(4)
        .flatten()
        .flat_map(|v| f16::from_f32(*v).to_bits().to_le_bytes())
        .collect();
    let atlas = DirectXAtlas::new(&device, &context);
    let tile = atlas
        .get_or_insert_with(
            AtlasKey::Image(RenderImageParams {
                image_id: ImageId(1),
                frame_index: 0,
                format: RenderImageFormat::Rgba16FloatScRgb,
            }),
            &mut || {
                Ok(Some((
                    size(2.into(), 2.into()),
                    std::borrow::Cow::Borrowed(bytes.as_slice()),
                )))
            },
        )
        .unwrap()
        .unwrap();
    let srv = atlas.get_texture_view(tile.texture_id).unwrap();
    unsafe {
        device
            .CreateRenderTargetView(&target, None, Some(&mut rtv))
            .unwrap();
        context.OMSetRenderTargets(Some(&[rtv.clone()]), None);
        context.ClearRenderTargetView(rtv.as_ref().unwrap(), &background);
        context.RSSetViewports(Some(&[D3D11_VIEWPORT {
            Width: 8.,
            Height: 2.,
            MinDepth: 0.,
            MaxDepth: 1.,
            ..Default::default()
        }]));
        let buffers = [
            globals.global_params_buffer.clone(),
            globals.batch_params_buffer.clone(),
        ];
        context.VSSetConstantBuffers(0, Some(&buffers));
        context.PSSetConstantBuffers(0, Some(&buffers));
    }
    let max_rgb = input
        .iter()
        .flat_map(|p| p[..3].iter())
        .copied()
        .fold(0_f32, f32::max);
    let flags = if nearest { 1 } else { 2 }
        | (encoding << 2)
        | ((f16::from_f32(max_rgb).to_bits() as u32) << 16);
    let mut pipeline = PipelineState::<PolychromeSprite>::new(
        &device,
        "hdr-test",
        ShaderModule::PolychromeSprite,
        1,
        create_blend_state(&device).unwrap(),
    )
    .unwrap();
    let bounds = Bounds::new(
        point(ScaledPixels(0.), ScaledPixels(0.)),
        size(ScaledPixels(8.), ScaledPixels(2.)),
    );
    pipeline
        .update_buffer(
            &device,
            &context,
            &[PolychromeSprite {
                order: 0,
                pad: flags,
                grayscale: false.into(),
                opacity: 1.,
                bounds,
                content_mask: ContentMask { bounds },
                corner_radii: Corners::all(ScaledPixels(0.)),
                tile,
            }],
        )
        .unwrap();
    pipeline
        .draw_with_texture(&context, &srv, &[globals.sampler], 1)
        .unwrap();
    unsafe {
        context.OMSetRenderTargets(None, None);
    }
    let readback = read_render_target(
        &device,
        &context,
        &target,
        match mode {
            0 => RenderImageFormat::Bgra8Srgb,
            2 => RenderImageFormat::Rgba16FloatLinearSrgb,
            _ => RenderImageFormat::Rgba16FloatScRgb,
        },
    )
    .unwrap();
    assert_eq!(readback.size(), size(8.into(), 2.into()));
    assert_eq!(readback.as_bytes().len(), if hdr { 128 } else { 64 });
    assert!(
        read_render_target(
            &device,
            &context,
            &target,
            if hdr {
                RenderImageFormat::Bgra8Srgb
            } else {
                RenderImageFormat::Rgba16FloatScRgb
            }
        )
        .is_err()
    );
    if hdr {
        readback
            .as_bytes()
            .as_chunks::<8>()
            .0
            .iter()
            .map(|p| {
                std::array::from_fn(|c| {
                    f16::from_bits(u16::from_le_bytes([p[c * 2], p[c * 2 + 1]])).to_f32()
                })
            })
            .collect()
    } else {
        readback
            .as_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[2], p[1], p[0], p[3]].map(|v| f32::from(v) / 255.))
            .collect()
    }
}

/// Requires an interactive Windows session, but does not show/activate a window,
/// present a frame or change HDR. Injected snapshots below test resource changes,
/// not the physical monitor's HDR presentation.
#[test]
#[ignore = "opt-in hidden native HWND/swapchain integration; requires a Windows desktop"]
fn native_hidden_color_output_resize_and_sdr_readback_restore_resources() {
    use windows::{Win32::UI::WindowsAndMessaging::*, core::w};
    let _queue = crate::color_output::DisplayQueueOwner::new().unwrap();
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("STATIC"),
            w!("Yue HDR hidden test"),
            WS_POPUP,
            0,
            0,
            64,
            32,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    };
    let _destroy = gpui_util::defer(|| unsafe { DestroyWindow(hwnd).unwrap() });
    assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
    let watcher = crate::color_output::DisplayWatcher::new(hwnd).unwrap();
    let actual = watcher.snapshot(hwnd).unwrap();
    println!("Actual Windows color snapshot: {actual:?}");
    assert_ne!(actual.display_kind, gpui::DisplayColorKind::Unknown);
    assert!(actual.display_id.is_some());
    let devices = crate::directx_devices::DirectXDevices::new().unwrap();
    let mut renderer = DirectXRenderer::new(
        hwnd,
        &devices,
        false,
        WindowColorPolicy::AutomaticHdr,
        actual,
    )
    .unwrap();
    renderer.resize(size(8.into(), 2.into())).unwrap();
    let mut scene = Scene::default();
    scene.insert_primitive(Quad {
        bounds: UiHarness::bounds(),
        content_mask: ContentMask {
            bounds: UiHarness::bounds(),
        },
        background: solid_background(hsla(0., 0., 0.5, 0.5)),
        ..Default::default()
    });
    scene.finish();
    let background = WindowBackgroundAppearance::Transparent;
    let mut snapshot = WindowColorOutput {
        display_kind: gpui::DisplayColorKind::Sdr,
        sdr_white_nits: Some(80.),
        max_luminance_nits: Some(400.),
        ..Default::default()
    };
    for kind in [
        gpui::DisplayColorKind::Sdr,
        gpui::DisplayColorKind::Hdr,
        gpui::DisplayColorKind::WideColor,
        gpui::DisplayColorKind::Unknown,
        gpui::DisplayColorKind::Hdr,
    ] {
        snapshot.display_kind = kind;
        renderer.refresh_output(snapshot.clone()).unwrap();
        let output = renderer.color_output();
        assert_eq!(
            output.mode,
            match kind {
                gpui::DisplayColorKind::Unknown => WindowColorMode::Sdr,
                gpui::DisplayColorKind::Hdr => WindowColorMode::HdrScRgb,
                _ => WindowColorMode::LinearSdr,
            }
        );
        assert!(!renderer.refresh_output(snapshot.clone()).unwrap());
        let native = renderer.render_to_native_image(&scene, background).unwrap();
        assert_eq!(
            native.format(),
            match output.mode {
                WindowColorMode::Sdr => RenderImageFormat::Bgra8Srgb,
                WindowColorMode::LinearSdr => RenderImageFormat::Rgba16FloatLinearSrgb,
                WindowColorMode::HdrScRgb => RenderImageFormat::Rgba16FloatScRgb,
            }
        );
        let sdr = renderer.render_to_image(&scene, background).unwrap();
        for pixel in sdr.pixels() {
            for channel in &pixel.0[..3] {
                assert!(channel.abs_diff(128) <= 2);
            }
            assert!(pixel.0[3].abs_diff(128) <= 1);
        }
        assert_eq!(renderer.color_output(), output);
        let after = renderer.render_to_native_image(&scene, background).unwrap();
        assert_eq!(after.as_bytes(), native.as_bytes());
        renderer.resize(size(16.into(), 4.into())).unwrap();
        assert_eq!(
            renderer
                .render_to_native_image(&scene, background)
                .unwrap()
                .size(),
            size(16.into(), 4.into())
        );
        renderer.resize(size(8.into(), 2.into())).unwrap();
    }
    // UI white changes are a shader-parameter generation, not another atlas upload.
    snapshot.sdr_white_nits = Some(203.);
    let generation = renderer.color_output().generation;
    assert!(renderer.refresh_output(snapshot).unwrap());
    assert!(renderer.color_output().generation > generation);
    let native = renderer.render_to_native_image(&scene, background).unwrap();
    let red = f16::from_bits(u16::from_le_bytes(
        native.as_bytes()[..2].try_into().unwrap(),
    ))
    .to_f32();
    close(red, 0.21404114 * (203. / 80.) * 0.5);
    // Recreate renderer resources through the production device-recovery route.
    renderer.handle_device_lost(&devices).unwrap();
    renderer.skip_draws = false;
    let recovered = renderer.render_to_native_image(&scene, background).unwrap();
    assert_eq!(recovered.as_bytes(), native.as_bytes());
    drop(renderer);
    drop(watcher);
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= expected.abs() * 0.004 + 0.002,
        "{actual} != {expected}"
    );
}

#[test]
fn scrgb_keeps_highlights_negative_gamut_and_absolute_white() {
    for white in [1., 2.5375, 4.] {
        let output = render(
            [[-0.25, 12.5, 2.5375, 1.]; 2],
            true,
            white,
            2000.,
            2,
            true,
            [0.; 4],
        );
        for pixel in output {
            for (v, e) in pixel.into_iter().zip([-0.25, 12.5, 2.5375, 1.]) {
                close(v, e);
            }
        }
    }
}

#[test]
fn linear_sdr_white_scales_once_and_sdr_output_stays_relative() {
    let hdr = render(
        [[1., 0.5, 0.25, 1.]; 2],
        true,
        2.5375,
        1000.,
        1,
        true,
        [0.; 4],
    );
    let sdr = render([[1., 0.5, 0.25, 1.]; 2], false, 4., 100., 1, true, [0.; 4]);
    close(hdr[3][0], 2.5375);
    close(hdr[3][1], 1.26875);
    for (value, expected) in sdr[3].into_iter().zip([1., 0.7353569, 0.5370987, 1.]) {
        close(value, expected);
    }
}

#[test]
fn fp16_sdr_target_maps_hdr_without_srgb_encoding_or_absolute_white() {
    let mapped = render_mode([[12.5, 12.5, 12.5, 1.]; 2], 2, 4., 100., 2, true, [0.; 4]);
    for p in mapped {
        for value in p {
            close(value, 1.);
        }
    }
    let relative = render_mode([[1., 0.5, 0.25, 1.]; 2], 2, 4., 100., 1, true, [0.; 4]);
    for p in relative {
        for (v, e) in p.into_iter().zip([1., 0.5, 0.25, 1.]) {
            close(v, e);
        }
    }
    let encoded = render_mode([[0.5, 0.5, 0.5, 1.]; 2], 2, 4., 100., 0, true, [0.; 4]);
    for p in encoded {
        close(p[0], 0.21404114);
        close(p[3], 1.);
    }
}

#[test]
fn hdr_to_sdr_is_bounded_neutral_and_maps_peak_to_white() {
    for gray in [1., 2.5375, 12.5, 125.] {
        let output = render(
            [[gray, gray, gray, 1.]; 2],
            false,
            1.,
            100.,
            2,
            true,
            [0.; 4],
        );
        for pixel in output {
            assert!((0.0..=1.001).contains(&pixel[0]));
            close(pixel[0], pixel[1]);
            close(pixel[1], pixel[2]);
            // 80 nit is 0.8 linear relative to a 100-nit SDR white.
            close(pixel[0], if gray == 1. { 0.90633175 } else { 1. });
        }
    }
}

#[test]
fn linear_sampling_associates_alpha_and_preserves_hdr_energy() {
    let result = render(
        [[100., 0., 0., 0.], [0., 0., 12.5, 1.]],
        true,
        2.5375,
        10000.,
        2,
        false,
        [0.; 4],
    );
    for pixel in &result {
        close(pixel[0], 0.);
        close(pixel[1], 0.);
    }
    assert!(result.iter().any(|p| p[2] > 1. && p[2] < 12.));
    assert!(result.iter().any(|p| p[3] > 0. && p[3] < 1.));
    close(result[7][2], 12.5);
}

#[test]
fn float_target_composites_coverage_without_accumulating_alpha_above_one() {
    let output = render(
        [[1., 2., 3., 0.5]; 2],
        true,
        2.5375,
        2000.,
        2,
        true,
        [0.25, 0.5, 0.75, 0.25],
    );
    for pixel in output {
        for (actual, expected) in pixel.into_iter().zip([0.625, 1.25, 1.875, 0.625]) {
            close(actual, expected);
        }
    }
}

// Exercise the same UI pipelines as the live renderer on a real FP16 target.
struct UiHarness {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    target: ID3D11Texture2D,
    globals: DirectXGlobalElements,
}

impl UiHarness {
    fn new(mode: u32, white: f32) -> Self {
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .unwrap();
        }
        let device = device.unwrap();
        let context = context.unwrap();
        set_rasterizer_state(&device, &context).unwrap();
        let target = texture(
            &device,
            8,
            D3D11_USAGE_DEFAULT,
            DXGI_FORMAT_R16G16B16A16_FLOAT,
        );
        let globals = DirectXGlobalElements::new(&device).unwrap();
        update_buffer(
            &context,
            globals.global_params_buffer.as_ref().unwrap(),
            &[GlobalParams {
                viewport_size: [8., 2.],
                output_hdr: mode,
                sdr_white: white,
                output_peak: 2000.,
                ..Default::default()
            }],
        )
        .unwrap();
        update_buffer(
            &context,
            globals.batch_params_buffer.as_ref().unwrap(),
            &[BatchParams::default()],
        )
        .unwrap();
        let this = Self {
            device,
            context,
            target,
            globals,
        };
        this.bind();
        this
    }
    fn bind(&self) {
        let mut rtv = None;
        unsafe {
            self.device
                .CreateRenderTargetView(&self.target, None, Some(&mut rtv))
                .unwrap();
            self.context.OMSetRenderTargets(Some(&[rtv.clone()]), None);
            self.context
                .ClearRenderTargetView(rtv.as_ref().unwrap(), &[0.; 4]);
            self.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: 8.,
                Height: 2.,
                MinDepth: 0.,
                MaxDepth: 1.,
                ..Default::default()
            }]));
            let buffers = [
                self.globals.global_params_buffer.clone(),
                self.globals.batch_params_buffer.clone(),
            ];
            self.context.VSSetConstantBuffers(0, Some(&buffers));
            self.context.PSSetConstantBuffers(0, Some(&buffers));
        }
    }
    fn draw<T>(&self, module: ShaderModule, data: &[T], associated: bool) {
        let blend = if associated {
            create_blend_state_for_path_rasterization(&self.device)
        } else {
            create_blend_state(&self.device)
        }
        .unwrap();
        let mut pipeline =
            PipelineState::<T>::new(&self.device, "hdr-ui-test", module, data.len(), blend)
                .unwrap();
        pipeline
            .update_buffer(&self.device, &self.context, data)
            .unwrap();
        pipeline
            .draw(
                &self.context,
                if associated {
                    D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST
                } else {
                    D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP
                },
                if associated { data.len() as u32 } else { 4 },
                1,
            )
            .unwrap();
    }
    fn read(&self) -> Vec<[f32; 4]> {
        unsafe {
            self.context.OMSetRenderTargets(None, None);
        }
        read_render_target(
            &self.device,
            &self.context,
            &self.target,
            RenderImageFormat::Rgba16FloatScRgb,
        )
        .unwrap()
        .as_bytes()
        .as_chunks::<8>()
        .0
        .iter()
        .map(|p| {
            std::array::from_fn(|c| {
                f16::from_bits(u16::from_le_bytes([p[c * 2], p[c * 2 + 1]])).to_f32()
            })
        })
        .collect()
    }
    fn bounds() -> Bounds<ScaledPixels> {
        Bounds::new(
            point(ScaledPixels(0.), ScaledPixels(0.)),
            size(ScaledPixels(8.), ScaledPixels(2.)),
        )
    }
}

#[test]
fn ui_quad_border_and_underline_convert_srgb_before_linear_blending() {
    let expected = 0.21404114 * 2.5375;
    let bounds = UiHarness::bounds();
    let gray = hsla(0., 0., 0.5, 0.5);
    let h = UiHarness::new(1, 2.5375);
    h.draw(
        ShaderModule::Quad,
        &[Quad {
            bounds,
            content_mask: ContentMask { bounds },
            background: solid_background(gray),
            ..Default::default()
        }],
        false,
    );
    for pixel in h.read() {
        close(pixel[0], expected * 0.5);
        close(pixel[3], 0.5);
    }
    h.bind();
    h.draw(
        ShaderModule::Quad,
        &[Quad {
            bounds,
            content_mask: ContentMask { bounds },
            border_color: gray,
            border_widths: Edges::all(ScaledPixels(1.)),
            ..Default::default()
        }],
        false,
    );
    for pixel in h.read() {
        close(pixel[0], expected * 0.5);
        close(pixel[3], 0.5);
    }
    h.bind();
    h.draw(
        ShaderModule::Underline,
        &[Underline {
            order: 0,
            pad: 0,
            bounds,
            content_mask: ContentMask { bounds },
            color: gray,
            thickness: ScaledPixels(1.),
            wavy: false.into(),
        }],
        false,
    );
    for pixel in h.read() {
        close(pixel[0], expected * 0.5);
        close(pixel[3], 0.5);
    }
}

#[test]
fn path_rasterization_is_linear_associated_and_not_white_scaled_twice() {
    let h = UiHarness::new(1, 2.5375);
    let bounds = UiHarness::bounds();
    let (resolved, srv) =
        create_path_intermediate_texture(&h.device, 8, 2, DXGI_FORMAT_R16G16B16A16_FLOAT).unwrap();
    let (msaa, view) = create_path_intermediate_msaa_texture_and_view(
        &h.device,
        8,
        2,
        DXGI_FORMAT_R16G16B16A16_FLOAT,
    )
    .unwrap();
    unsafe {
        h.context
            .ClearRenderTargetView(view.as_ref().unwrap(), &[0.; 4]);
        h.context.OMSetRenderTargets(Some(&[view]), None);
    }
    // Full interior triangle coverage: nonzero x derivative in st.
    let vertices = [[-1., -1.], [20., -1.], [-1., 20.]].map(|[x, y]| PathRasterizationSprite {
        xy_position: point(ScaledPixels(x), ScaledPixels(y)),
        st_position: point(x, 0.),
        color: solid_background(hsla(0., 0., 0.5, 0.5)),
        bounds,
    });
    h.draw(ShaderModule::PathRasterization, &vertices, true);
    unsafe {
        h.context.OMSetRenderTargets(None, None);
        h.context
            .ResolveSubresource(&resolved, 0, &msaa, 0, DXGI_FORMAT_R16G16B16A16_FLOAT);
    }
    h.bind();
    let mut composite = PipelineState::new(
        &h.device,
        "hdr-path-composite",
        ShaderModule::PathSprite,
        1,
        create_blend_state_for_path_sprite(&h.device).unwrap(),
    )
    .unwrap();
    composite
        .update_buffer(&h.device, &h.context, &[PathSprite { bounds }])
        .unwrap();
    composite
        .draw_with_texture(
            &h.context,
            &[srv],
            std::slice::from_ref(&h.globals.sampler),
            1,
        )
        .unwrap();
    for pixel in h.read() {
        close(pixel[0], 0.21404114 * 2.5375 * 0.5);
        close(pixel[3], 0.5);
    }
}

#[test]
fn encoded_sdr_sprites_decode_each_texel_before_linear_filtering() {
    let output = render(
        [[0., 0., 0., 1.], [1., 1., 1., 1.]],
        true,
        2.5375,
        2000.,
        0,
        false,
        [0.; 4],
    );
    // Source texel centers at output x=1.5 and 5.5; x=3.5 has 3/8 white.
    close(output[3][0], 0.375 * 2.5375);
    close(output[3][1], 0.375 * 2.5375);
}

#[test]
fn relative_linear_sdr_ui_ignores_absolute_hdr_white_and_gradient_dither() {
    let h = UiHarness::new(2, 4.);
    let bounds = UiHarness::bounds();
    h.draw(
        ShaderModule::Quad,
        &[Quad {
            bounds,
            content_mask: ContentMask { bounds },
            background: linear_gradient(
                90.,
                linear_color_stop(hsla(0., 0., 0.5, 0.5), 0.),
                linear_color_stop(hsla(0., 0., 0.5, 0.5), 1.),
            ),
            ..Default::default()
        }],
        false,
    );
    // All pixels must agree: no encoded 8-bit dither in a half-float scene.
    for p in h.read() {
        close(p[0], 0.21404114 * 0.5);
        close(p[3], 0.5);
    }
    h.bind();
    h.draw(
        ShaderModule::Shadow,
        &[Shadow {
            order: 0,
            blur_radius: ScaledPixels(0.),
            bounds,
            corner_radii: Corners::all(ScaledPixels(0.)),
            content_mask: ContentMask { bounds },
            color: hsla(0., 0., 0.5, 0.5),
            element_bounds: bounds,
            element_corner_radii: Corners::all(ScaledPixels(0.)),
            inset: 0,
            pad: 0,
        }],
        false,
    );
    for p in h.read() {
        close(p[0], 0.21404114 * 0.5);
        close(p[3], 0.5);
    }
}

#[test]
fn hdr_text_uses_grayscale_coverage_linear_color_and_preserves_target_alpha() {
    for subpixel in [false, true] {
        let h = UiHarness::new(1, 2.5375);
        let source = texture(
            &h.device,
            8,
            D3D11_USAGE_DEFAULT,
            DXGI_FORMAT_R16G16B16A16_FLOAT,
        );
        let data: Vec<u8> = [0.2, 0.5, 0.8, 1.]
            .into_iter()
            .cycle()
            .take(8 * 2 * 4)
            .flat_map(|v| f16::from_f32(v).to_bits().to_le_bytes())
            .collect();
        let mut srv = None;
        unsafe {
            h.context
                .UpdateSubresource(&source, 0, None, data.as_ptr().cast(), 8 * 8, 0);
            h.device
                .CreateShaderResourceView(&source, None, Some(&mut srv))
                .unwrap();
        }
        let bounds = UiHarness::bounds();
        let sprite = MonochromeSprite {
            order: 0,
            pad: 0,
            bounds,
            content_mask: ContentMask { bounds },
            color: hsla(0., 0., 0.5, 0.5),
            tile: AtlasTile {
                texture_id: AtlasTextureId {
                    index: 0,
                    kind: if subpixel {
                        AtlasTextureKind::Subpixel
                    } else {
                        AtlasTextureKind::Monochrome
                    },
                },
                tile_id: TileId(0),
                padding: 0,
                bounds: Bounds::new(point(0.into(), 0.into()), size(8.into(), 2.into())),
            },
            transformation: TransformationMatrix::unit(),
        };
        let mut pipeline = PipelineState::new(
            &h.device,
            "hdr-glyph",
            if subpixel {
                ShaderModule::SubpixelSprite
            } else {
                ShaderModule::MonochromeSprite
            },
            1,
            if subpixel {
                create_blend_state_for_subpixel_rendering(&h.device, true)
            } else {
                create_blend_state(&h.device)
            }
            .unwrap(),
        )
        .unwrap();
        pipeline
            .update_buffer(&h.device, &h.context, &[sprite])
            .unwrap();
        pipeline
            .draw_with_texture(
                &h.context,
                &[srv],
                std::slice::from_ref(&h.globals.sampler),
                1,
            )
            .unwrap();
        let alpha = 0.5 * if subpixel { 0.5 } else { 0.2 };
        for p in h.read() {
            for c in &p[..3] {
                close(*c, 0.21404114 * 2.5375 * alpha);
            }
            close(p[3], alpha);
        }
    }
}

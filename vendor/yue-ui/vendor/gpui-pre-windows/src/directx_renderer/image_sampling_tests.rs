//! Production sprite shaders on WARP, without a window or swap chain.
use super::*;
use std::prelude::v1::test;
use windows::Win32::Foundation::HMODULE;

fn texture(device: &ID3D11Device, width: u32, height: u32, usage: D3D11_USAGE) -> ID3D11Texture2D {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: RENDER_TARGET_FORMAT,
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

struct SamplingCase {
    nearest: bool,
    precise: bool,
    origin: [f32; 2],
    size: [f32; 2],
    clip: [f32; 4],
    corners: Corners<ScaledPixels>,
    opacity: f32,
    alpha: u8,
    transparent_first: bool,
    grayscale: bool,
}
impl Default for SamplingCase {
    fn default() -> Self {
        Self {
            nearest: true,
            precise: false,
            origin: [0., 0.],
            size: [8., 8.],
            clip: [0., 0., 8., 8.],
            corners: Corners::all(ScaledPixels(0.)),
            opacity: 1.,
            alpha: 255,
            transparent_first: false,
            grayscale: false,
        }
    }
}
fn render(nearest: bool, origin: [f32; 2], size: [f32; 2], clip: [f32; 4]) -> Vec<u8> {
    render_case(SamplingCase {
        nearest,
        origin,
        size,
        clip,
        ..Default::default()
    })
}
fn render_case(case: SamplingCase) -> Vec<u8> {
    let SamplingCase {
        nearest,
        precise,
        origin,
        size: size_,
        clip,
        corners,
        opacity,
        alpha,
        transparent_first,
        grayscale,
    } = case;
    let (mut device, mut context) = (None, None);
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
    let source = texture(&device, 4, 4, D3D11_USAGE_DEFAULT);
    let target = texture(&device, 8, 8, D3D11_USAGE_DEFAULT);
    let mut data = [[255u8, 0, 255, 255]; 16]; // atlas neighbours must never bleed
    data[5] = [0, 0, 255, 255];
    data[6] = [0, 255, 0, 255];
    data[9] = [255, 0, 0, 255];
    data[10] = [255, 255, 255, 255];
    for i in [5, 6, 9, 10] {
        data[i][3] = alpha;
    }
    if transparent_first {
        data[5][3] = 0;
    }
    let (mut srv, mut rtv) = (None, None);
    let globals = DirectXGlobalElements::new(&device).unwrap();
    update_buffer(
        &context,
        globals.global_params_buffer.as_ref().unwrap(),
        &[GlobalParams {
            viewport_size: [8., 8.],
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
    unsafe {
        context.UpdateSubresource(&source, 0, None, data.as_ptr().cast(), 16, 0);
        device
            .CreateShaderResourceView(&source, None, Some(&mut srv))
            .unwrap();
        device
            .CreateRenderTargetView(&target, None, Some(&mut rtv))
            .unwrap();
        context.OMSetRenderTargets(Some(&[rtv.clone()]), None);
        context.ClearRenderTargetView(rtv.as_ref().unwrap(), &[0.; 4]);
        context.RSSetViewports(Some(&[D3D11_VIEWPORT {
            Width: 8.,
            Height: 8.,
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
    let mut pipeline = PipelineState::<PolychromeSprite>::new(
        &device,
        "image-sampling-test",
        ShaderModule::PolychromeSprite,
        1,
        create_blend_state(&device).unwrap(),
    )
    .unwrap();
    pipeline
        .update_buffer(
            &device,
            &context,
            &[PolychromeSprite {
                order: 0,
                pad: if nearest {
                    1
                } else if precise {
                    2
                } else {
                    0
                },
                grayscale: grayscale.into(),
                opacity,
                bounds: Bounds::new(
                    point(ScaledPixels(origin[0]), ScaledPixels(origin[1])),
                    gpui::size(ScaledPixels(size_[0]), ScaledPixels(size_[1])),
                ),
                content_mask: ContentMask {
                    bounds: Bounds::new(
                        point(ScaledPixels(clip[0]), ScaledPixels(clip[1])),
                        gpui::size(ScaledPixels(clip[2]), ScaledPixels(clip[3])),
                    ),
                },
                corner_radii: corners,
                tile: AtlasTile {
                    texture_id: AtlasTextureId {
                        index: 0,
                        kind: AtlasTextureKind::Polychrome,
                    },
                    tile_id: TileId(0),
                    padding: 0,
                    bounds: Bounds::new(
                        point(DevicePixels(1), DevicePixels(1)),
                        gpui::size(DevicePixels(2), DevicePixels(2)),
                    ),
                },
            }],
        )
        .unwrap();
    pipeline
        .draw_with_texture(&context, &[srv], &[globals.sampler], 1)
        .unwrap();
    let staging = texture(&device, 8, 8, D3D11_USAGE_STAGING);
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    let mut result = vec![0; 8 * 8 * 4];
    unsafe {
        context.OMSetRenderTargets(None, None);
        context.CopyResource(&staging, &target);
        context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .unwrap();
        for y in 0..8 {
            std::ptr::copy_nonoverlapping(
                (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                result.as_mut_ptr().add(y * 32),
                32,
            );
        }
        context.Unmap(&staging, 0);
    }
    result
}

#[test]
fn nearest_sprite_samples_exact_texels_and_keeps_linear_default() {
    let nearest = render(true, [0., 0.], [8., 8.], [0., 0., 8., 8.]);
    let colors = [
        [0, 0, 255, 255],
        [0, 255, 0, 255],
        [255, 0, 0, 255],
        [255, 255, 255, 255],
    ];
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(
                &nearest[(y * 8 + x) * 4..(y * 8 + x + 1) * 4],
                &colors[(y / 4) * 2 + x / 4]
            );
        }
    }
    let linear = render(false, [0., 0.], [8., 8.], [0., 0., 8., 8.]);
    assert_ne!(linear, nearest, "ordinary images retain interpolation");
}

#[test]
fn nearest_sprite_fractional_pan_preserves_full_uvs_and_clip() {
    let pixels = render(true, [-1.25, -0.5], [12., 12.], [1., 1., 6., 6.]);
    let colors = [
        [0, 0, 255, 255],
        [0, 255, 0, 255],
        [255, 0, 0, 255],
        [255, 255, 255, 255],
    ];
    for y in 0..8 {
        for x in 0..8 {
            let expected = if (1..7).contains(&x) && (1..7).contains(&y) {
                colors[usize::from(y >= 5) * 2 + usize::from(x >= 5)]
            } else {
                [0; 4]
            };
            assert_eq!(
                &pixels[(y * 8 + x) * 4..(y * 8 + x + 1) * 4],
                &expected,
                "pixel {x},{y}"
            );
        }
    }
}

#[test]
fn nearest_sprite_retains_rounding_alpha_opacity_and_grayscale() {
    let pixels = render_case(SamplingCase {
        corners: Corners::all(ScaledPixels(3.)),
        ..Default::default()
    });
    assert_eq!(&pixels[..4], &[0; 4], "rounded exterior stays empty");
    assert_eq!(&pixels[(3 * 8 + 3) * 4..(3 * 8 + 4) * 4], &[0, 0, 255, 255]);
    let pixels = render_case(SamplingCase {
        alpha: 128,
        opacity: 0.5,
        grayscale: true,
        ..Default::default()
    });
    let pixel = &pixels[(3 * 8 + 3) * 4..(3 * 8 + 4) * 4];
    assert!(
        (i32::from(pixel[3]) - 64).abs() <= 1,
        "source alpha and element opacity multiply: {pixel:?}"
    );
    assert_eq!(pixel[0], pixel[1]);
    assert_eq!(pixel[1], pixel[2]);
    assert!(pixel[0] > 0, "grayscale must retain visible image content");
    let pixels = render_case(SamplingCase {
        clip: [0., 0., 4., 8.],
        corners: Corners {
            top_left: ScaledPixels(3.),
            bottom_left: ScaledPixels(3.),
            ..Corners::default()
        },
        ..Default::default()
    });
    assert_eq!(
        pixels[(3 * 4) + 3],
        255,
        "comparison inner edge must not acquire a rounded cutout"
    );
    assert_eq!(&pixels[4 * 4..5 * 4], &[0; 4]);
}

#[test]
fn precise_linear_actual_size_is_exact_and_magnified_edges_do_not_bleed_atlas_neighbors() {
    let pixels = render_case(SamplingCase {
        nearest: false,
        precise: true,
        origin: [3., 3.],
        size: [2., 2.],
        ..Default::default()
    });
    let colors = [
        [0, 0, 255, 255],
        [0, 255, 0, 255],
        [255, 0, 0, 255],
        [255, 255, 255, 255],
    ];
    for (ix, (x, y)) in [(3, 3), (4, 3), (3, 4), (4, 4)].into_iter().enumerate() {
        assert_eq!(&pixels[(y * 8 + x) * 4..(y * 8 + x + 1) * 4], &colors[ix]);
    }
    let pixels = render_case(SamplingCase {
        nearest: false,
        precise: true,
        ..Default::default()
    });
    for (ix, (x, y)) in [(0, 0), (7, 0), (0, 7), (7, 7)].into_iter().enumerate() {
        assert_eq!(
            &pixels[(y * 8 + x) * 4..(y * 8 + x + 1) * 4],
            &colors[ix],
            "atlas neighbor at {x},{y}"
        );
    }
    assert_ne!(
        &pixels[(3 * 8 + 3) * 4..(3 * 8 + 4) * 4],
        &colors[0],
        "linear interpolation must still occur between source texels"
    );
}

#[test]
fn precise_linear_clipping_does_not_recompute_uvs_or_double_premultiply_alpha() {
    let case = || SamplingCase {
        nearest: false,
        precise: true,
        origin: [-1.25, -0.5],
        size: [12., 12.],
        alpha: 128,
        opacity: 0.5,
        ..Default::default()
    };
    let whole = render_case(case());
    let clipped = render_case(SamplingCase {
        clip: [1., 1., 6., 6.],
        ..case()
    });
    for y in 0..8 {
        for x in 0..8 {
            let ix = (y * 8 + x) * 4;
            let expected = if (1..7).contains(&x) && (1..7).contains(&y) {
                &whole[ix..ix + 4]
            } else {
                &[0; 4]
            };
            assert_eq!(&clipped[ix..ix + 4], expected, "pixel {x},{y}");
        }
    }
    let alpha = i32::from(clipped[(2 * 8 + 2) * 4 + 3]);
    assert!((alpha - 64).abs() <= 1);
    assert!(
        (i32::from(clipped[(8 + 1) * 4 + 2]) - 64).abs() <= 1,
        "straight source alpha is multiplied once by blending"
    );
}

#[test]
fn precise_linear_interpolation_does_not_tint_edges_with_invisible_rgb() {
    let pixels = render_case(SamplingCase {
        nearest: false,
        precise: true,
        transparent_first: true,
        ..Default::default()
    });
    // Top row is transparent red beside opaque green. At x=3, green contributes
    // 3/8 coverage; no invisible red may survive the final transparent blend.
    let pixel = &pixels[3 * 4..4 * 4];
    assert_eq!(pixel[0], 0);
    assert_eq!(
        pixel[2], 0,
        "hidden red must not contaminate green: {pixel:?}"
    );
    assert!((i32::from(pixel[1]) - 96).abs() <= 1);
    assert!((i32::from(pixel[3]) - 96).abs() <= 1);
}

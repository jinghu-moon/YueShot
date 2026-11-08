use super::*;
use std::prelude::v1::test;
use windows::Win32::Foundation::HMODULE;

#[test]
fn float_capture_preserves_hdr_and_reallocates_when_target_format_changes() {
    let mut h = Harness::new(80, 64);
    let filter = blur([0., 0., 80., 64.], 4.);
    h.upload(|_, _| [80, 100, 120, 255]);
    h.draw(&filter);
    let allocations = h.pipeline.stats.texture_allocations;
    h.target = Texture::new(&h.device, [80, 64], DXGI_FORMAT_R16G16B16A16_FLOAT, true).unwrap();
    let data: Vec<u8> = (0..64)
        .flat_map(|_| {
            (0..80).flat_map(|x| {
                [-0.25, if x < 40 { 0. } else { 8. }, 12.5, 0.5]
                    .into_iter()
                    .flat_map(|v| half::f16::from_f32(v).to_bits().to_le_bytes())
            })
        })
        .collect();
    unsafe {
        h.context
            .UpdateSubresource(&h.target.texture, 0, None, data.as_ptr().cast(), 80 * 8, 0);
    }
    h.draw(&filter);
    let result = read_render_target(
        &h.device,
        &h.context,
        &h.target.texture,
        RenderImageFormat::Rgba16FloatScRgb,
    )
    .unwrap();
    let pixel = |x: usize, y: usize| -> [f32; 4] {
        let p = &result.as_bytes()[(y * 80 + x) * 8..][..8];
        std::array::from_fn(|c| {
            half::f16::from_bits(u16::from_le_bytes([p[c * 2], p[c * 2 + 1]])).to_f32()
        })
    };
    for x in [0, 20, 39, 40, 60, 79] {
        let p = pixel(x, 32);
        assert!((p[0] + 0.25).abs() < 0.003, "{p:?}");
        assert!((p[2] - 12.5).abs() < 0.04, "{p:?}");
        assert!((p[3] - 0.5).abs() < 0.003, "{p:?}");
    }
    assert!(pixel(39, 32)[1] > 1. && pixel(39, 32)[1] < 4.);
    assert!(pixel(40, 32)[1] > 4. && pixel(40, 32)[1] < 7.);
    assert_eq!(h.pipeline.stats.texture_allocations, allocations + 3);
    assert!(h.pipeline.stats.peak_scratch_bytes <= MAX_SCRATCH_BYTES);
}

fn blur(rect: [f32; 4], sigma: f32) -> BackdropBlur {
    let bounds = Bounds::new(
        point(ScaledPixels(rect[0]), ScaledPixels(rect[1])),
        size(ScaledPixels(rect[2]), ScaledPixels(rect[3])),
    );
    BackdropBlur {
        quad: Quad {
            bounds,
            content_mask: ContentMask { bounds },
            ..Default::default()
        },
        sigma: ScaledPixels(sigma),
        opacity: 1.,
    }
}

struct Harness {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    target: Texture,
    background: Option<Texture>,
    pipeline: BackdropPipeline,
    pipeline_create_us: f64,
}
impl Harness {
    fn new(width: u32, height: u32) -> Self {
        Self::with_driver(width, height, D3D_DRIVER_TYPE_WARP)
    }
    fn with_driver(width: u32, height: u32, driver: D3D_DRIVER_TYPE) -> Self {
        let mut device = None;
        let mut context = None;
        // WARP correctness or hardware profiling, without HWND or swap chain.
        unsafe {
            D3D11CreateDevice(
                None,
                driver,
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
        let target = Texture::new(&device, [width, height], RENDER_TARGET_FORMAT, true).unwrap();
        let start = std::time::Instant::now();
        let pipeline = BackdropPipeline::new(&device)
            .expect("Production HLSL must compile and create real GPU shaders");
        let pipeline_create_us = start.elapsed().as_secs_f64() * 1e6;
        Self {
            device,
            context,
            target,
            background: None,
            pipeline,
            pipeline_create_us,
        }
    }
    fn upload(&self, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let [width, height] = self.target.capacity;
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                data.extend(pixel(x, y));
            }
        }
        unsafe {
            self.context.UpdateSubresource(
                &self.target.texture,
                0,
                None,
                data.as_ptr().cast(),
                width * 4,
                0,
            );
        }
        data
    }
    fn draw(&mut self, blur: &BackdropBlur) {
        let [width, height] = self.target.capacity;
        self.pipeline
            .draw(
                &self.device,
                &self.context,
                &self.target.texture,
                &self.target.rtv,
                D3D11_VIEWPORT {
                    Width: width as f32,
                    Height: height as f32,
                    MinDepth: 0.,
                    MaxDepth: 1.,
                    ..Default::default()
                },
                blur,
            )
            .unwrap();
    }
    fn save_background(&mut self) {
        let texture = Texture::new(
            &self.device,
            self.target.capacity,
            RENDER_TARGET_FORMAT,
            false,
        )
        .unwrap();
        unsafe {
            self.context
                .CopyResource(&texture.texture, &self.target.texture);
        }
        self.background = Some(texture);
    }
    fn reset_background(&self) {
        if let Some(background) = &self.background {
            unsafe {
                self.context.OMSetRenderTargets(None, None);
                self.context
                    .CopyResource(&self.target.texture, &background.texture);
            }
        }
    }
    fn read(&self) -> Vec<u8> {
        let [width, height] = self.target.capacity;
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
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&mut staging))
                .unwrap();
        }
        let staging = staging.unwrap();
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        let mut result = vec![0; (width * height * 4) as usize];
        unsafe {
            self.context.OMSetRenderTargets(None, None);
            self.context.CopyResource(&staging, &self.target.texture);
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .unwrap();
            for y in 0..height as usize {
                std::ptr::copy_nonoverlapping(
                    (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                    result.as_mut_ptr().add(y * width as usize * 4),
                    width as usize * 4,
                );
            }
            self.context.Unmap(&staging, 0);
        }
        result
    }
}

#[derive(Default)]
struct FrameTiming {
    cpu_us: f64,
    cpu_stages: [f64; 6],
    gpu_stages: [f64; 5],
}
fn profile_frame(h: &mut Harness, regions: &[BackdropBlur]) -> Option<FrameTiming> {
    // Reset before timestamps so repeated frames do not gradually blur the
    // benchmark input into a different workload. This is fixture work only.
    h.reset_background();
    let start = std::time::Instant::now();
    for region in regions {
        h.draw(region);
    }
    h.pipeline.finish_profile_frame(&h.context);
    let cpu_us = start.elapsed().as_secs_f64() * 1e6;
    let profile = h.pipeline.profiler.as_mut().unwrap();
    // Waiting is explicit and outside both CPU submission and GPU intervals.
    // This drain is benchmark-only; the live renderer never blocks for queries.
    profile.drain(&h.context).unwrap();
    let samples = std::mem::take(&mut profile.completed);
    if samples.len() != regions.len() {
        return None;
    } // disjoint samples are invalid
    let mut frame = FrameTiming {
        cpu_us,
        ..Default::default()
    };
    for sample in samples {
        for (sum, value) in frame.cpu_stages.iter_mut().zip(sample.cpu_us) {
            *sum += value;
        }
        for (sum, value) in frame.gpu_stages.iter_mut().zip(sample.gpu_us) {
            *sum += value;
        }
    }
    Some(frame)
}

fn report(name: &str, phase: &str, frames: &[FrameTiming], stats: &Stats, pipeline_us: f64) {
    assert!(!frames.is_empty(), "Need valid non-disjoint GPU samples");
    let quantile = |mut values: Vec<f64>, fraction: f64| {
        values.sort_by(f64::total_cmp);
        values[((values.len() - 1) as f64 * fraction).round() as usize]
    };
    let cpu: Vec<_> = frames.iter().map(|f| f.cpu_us).collect();
    let gpu: Vec<_> = frames.iter().map(|f| f.gpu_stages.iter().sum()).collect();
    let mut cpu_stages = [0.; 6];
    let mut gpu_stages = [0.; 5];
    for f in frames {
        for (sum, v) in cpu_stages.iter_mut().zip(f.cpu_stages) {
            *sum += v / frames.len() as f64;
        }
        for (sum, v) in gpu_stages.iter_mut().zip(f.gpu_stages) {
            *sum += v / frames.len() as f64;
        }
    }
    println!(
        "PROFILE_ROW {name},{phase},n={},cpu_p50_us={:.3},cpu_p95_us={:.3},gpu_p50_us={:.3},gpu_p95_us={:.3},pipeline_create_us={pipeline_us:.3},allocations={},peak_scratch_bytes={},copied_px_per_frame={},work_px_per_frame={}",
        frames.len(),
        quantile(cpu.clone(), 0.5),
        quantile(cpu, 0.95),
        quantile(gpu.clone(), 0.5),
        quantile(gpu, 0.95),
        stats.texture_allocations,
        stats.peak_scratch_bytes,
        stats.copied_pixels / frames.len() as u64,
        stats.work_pixels / frames.len() as u64
    );
    if matches!(phase, "envelope" | "optimized" | "reference") {
        return;
    }
    println!(
        "PROFILE_CPU {name},{phase},prepare/copy/downsample/horizontal/vertical/composite_us={cpu_stages:.3?}"
    );
    println!(
        "PROFILE_GPU {name},{phase},copy/downsample/horizontal/vertical/composite_us={gpu_stages:.3?}"
    );
}

fn add_stats(total: &mut Stats, frame: &Stats) {
    total.regions += frame.regions;
    total.copied_pixels += frame.copied_pixels;
    total.work_pixels += frame.work_pixels;
    total.texture_allocations += frame.texture_allocations;
    total.kernel_rebuilds += frame.kernel_rebuilds;
    total.initialization_clears += frame.initialization_clears;
    total.skipped_regions += frame.skipped_regions;
    total.peak_scratch_bytes = total.peak_scratch_bytes.max(frame.peak_scratch_bytes);
}

/// A single timestamp pair around all regions, with the production profiler
/// disabled. Contrasts stage instrumentation against a minimally timed path.
struct EnvelopeTimer {
    disjoint: ID3D11Query,
    begin: ID3D11Query,
    end: ID3D11Query,
}
impl EnvelopeTimer {
    fn new(device: &ID3D11Device) -> Self {
        Self {
            disjoint: profile::query(device, D3D11_QUERY_TIMESTAMP_DISJOINT).unwrap(),
            begin: profile::query(device, D3D11_QUERY_TIMESTAMP).unwrap(),
            end: profile::query(device, D3D11_QUERY_TIMESTAMP).unwrap(),
        }
    }
    fn frame(&self, h: &mut Harness, regions: &[BackdropBlur]) -> Option<FrameTiming> {
        assert!(h.pipeline.profiler.is_none());
        h.reset_background();
        unsafe {
            h.context.Begin(&self.disjoint);
            h.context.End(&self.begin);
        }
        let start = std::time::Instant::now();
        for region in regions {
            h.draw(region);
        }
        let cpu_us = start.elapsed().as_secs_f64() * 1e6;
        unsafe {
            h.context.End(&self.end);
            h.context.End(&self.disjoint);
            h.context.Flush();
        }
        let wait = std::time::Instant::now();
        loop {
            let disjoint =
                profile::ready::<D3D11_QUERY_DATA_TIMESTAMP_DISJOINT>(&h.context, &self.disjoint)
                    .unwrap();
            if let Some(disjoint) = disjoint {
                if disjoint.Disjoint.as_bool() || disjoint.Frequency == 0 {
                    return None;
                }
                if let (Some(begin), Some(end)) = (
                    profile::ready::<u64>(&h.context, &self.begin).unwrap(),
                    profile::ready::<u64>(&h.context, &self.end).unwrap(),
                ) {
                    return Some(FrameTiming {
                        cpu_us,
                        gpu_stages: [
                            (end - begin) as f64 * 1e6 / disjoint.Frequency as f64,
                            0.,
                            0.,
                            0.,
                            0.,
                        ],
                        ..Default::default()
                    });
                }
            }
            assert!(wait.elapsed().as_secs() < 10, "GPU envelope query timeout");
            std::thread::yield_now();
        }
    }
}

/// An opt-in hardware measurement, not a correctness test or frame-rate gate.
/// Run `tools/test-gpui-backdrop.ps1 -Profile -Release`. No HWND or application.
#[test]
#[ignore = "Opt-in hardware GPU timestamp benchmark; use -Profile -Release"]
fn profile_hardware() {
    assert!(
        !cfg!(debug_assertions),
        "Measure Release precompiled shaders, not debug shader compilation"
    );
    let cases: Vec<(&str, Vec<BackdropBlur>)> = vec![
        ("toolbar_100", vec![blur([360., 250., 240., 40.], 16.)]),
        ("toolbar_200", vec![blur([720., 500., 480., 80.], 32.)]),
        ("grip_100", vec![blur([1000., 500., 32., 32.], 16.)]),
        (
            "toolbar_grip_100",
            vec![
                blur([360., 250., 240., 40.], 16.),
                blur([1000., 500., 32., 32.], 16.),
            ],
        ),
        (
            "toolbar_grip_200",
            vec![
                blur([720., 500., 480., 80.], 32.),
                blur([2000., 1000., 64., 64.], 32.),
            ],
        ),
        ("large_panel", vec![blur([360., 250., 1200., 800.], 16.)]),
        (
            "sixteen_toolbars",
            (0..16)
                .map(|i| {
                    blur(
                        [
                            100. + (i % 8) as f32 * 360.,
                            100. + (i / 8) as f32 * 300.,
                            240.,
                            40.,
                        ],
                        16.,
                    )
                })
                .collect(),
        ),
    ];
    for (index, (name, regions)) in cases.iter().enumerate() {
        let mut h = Harness::with_driver(3840, 2160, D3D_DRIVER_TYPE_HARDWARE);
        if index == 0 {
            let dxgi: IDXGIDevice = h.device.cast().unwrap();
            let desc = unsafe { dxgi.GetAdapter().unwrap().GetDesc().unwrap() };
            let end = desc
                .Description
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(desc.Description.len());
            println!(
                "PROFILE_ADAPTER name={:?},vendor={:#x},device={:#x},feature={:?},viewport=3840x2160,release=true",
                String::from_utf16_lossy(&desc.Description[..end]),
                desc.VendorId,
                desc.DeviceId,
                unsafe { h.device.GetFeatureLevel() }
            );
        }
        h.upload(|x, y| [(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255]);
        h.save_background();
        h.pipeline.profiler = Some(profile::Profiler::new(&h.device, 1, 32, false).unwrap());
        if let Some(cold) = profile_frame(&mut h, regions) {
            report(
                name,
                "cold",
                &[cold],
                &h.pipeline.stats,
                h.pipeline_create_us,
            );
        } else {
            println!("PROFILE_COLD_INVALID {name},disjoint_timestamp");
        }
        for _ in 0..32 {
            let _ = profile_frame(&mut h, regions);
        }
        h.pipeline.stats = Stats::default();
        let frames: Vec<_> = (0..128)
            .filter_map(|_| profile_frame(&mut h, regions))
            .collect();
        assert!(
            frames.len() >= 100,
            "Too many invalid/disjoint GPU samples for {name}"
        );
        assert_eq!(
            h.pipeline.stats.texture_allocations, 0,
            "Steady frames must reuse textures"
        );
        assert_eq!(
            h.pipeline.stats.kernel_rebuilds, 0,
            "Steady frames must reuse kernels"
        );
        assert_eq!(
            h.pipeline.stats.initialization_clears, 0,
            "No per-frame initialization clears"
        );
        report(
            name,
            "steady",
            &frames,
            &h.pipeline.stats,
            h.pipeline_create_us,
        );
        h.pipeline.profiler = None;
        let timer = EnvelopeTimer::new(&h.device);
        for _ in 0..32 {
            let _ = timer.frame(&mut h, regions);
        }
        h.pipeline.stats = Stats::default();
        let baseline: Vec<_> = (0..128)
            .filter_map(|_| timer.frame(&mut h, regions))
            .collect();
        assert!(
            baseline.len() >= 100,
            "Too many invalid GPU envelope samples"
        );
        assert_eq!(h.pipeline.stats.texture_allocations, 0);
        assert_eq!(h.pipeline.stats.kernel_rebuilds, 0);
        // Only PROFILE_ROW is meaningful here: the envelope has no stage data.
        report(
            name,
            "envelope",
            &baseline,
            &h.pipeline.stats,
            h.pipeline_create_us,
        );
        let mut measured = [Vec::new(), Vec::new()];
        let mut totals = [Stats::default(), Stats::default()];
        // Same device, images and scratch, with alternating AB/BA order.
        for frame in 0..160 {
            for variant in if frame % 2 == 0 { [0, 1] } else { [1, 0] } {
                h.pipeline.full_work_reference = variant == 1;
                h.pipeline.stats = Stats::default();
                if let Some(sample) = timer.frame(&mut h, regions) {
                    if frame >= 32 {
                        measured[variant].push(sample);
                        add_stats(&mut totals[variant], &h.pipeline.stats);
                    }
                }
            }
        }
        for (variant, phase) in ["optimized", "reference"].into_iter().enumerate() {
            assert!(measured[variant].len() >= 100);
            assert_eq!(totals[variant].texture_allocations, 0);
            assert_eq!(totals[variant].kernel_rebuilds, 0);
            assert_eq!(totals[variant].initialization_clears, 0);
            report(
                name,
                phase,
                &measured[variant],
                &totals[variant],
                h.pipeline_create_us,
            );
        }
    }
}
fn pixel(data: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    data[(y * width + x) * 4..(y * width + x + 1) * 4]
        .try_into()
        .unwrap()
}

#[test]
fn gaussian_pair_weights_are_normalized_and_bounded() {
    for sigma in [0.01, 0.5, 1., 2., 4., 8.] {
        let k = Kernel::new(sigma);
        let sum = k.center + 2. * k.pairs.iter().map(|p| p[1]).sum::<f32>();
        assert!((sum - 1.).abs() < 0.00001);
        assert!(k.count <= 12);
        assert!(k.pairs.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    }
}

#[test]
fn profiling_is_frame_scoped_bounded_and_reuses_completed_slots() {
    let mut h = Harness::new(64, 64);
    h.upload(|_, _| [80, 80, 80, 255]);
    h.pipeline.profiler = Some(profile::Profiler::new(&h.device, 1, 3, false).unwrap());
    let region = blur([12., 12., 32., 32.], 2.);
    for _ in 0..4 {
        h.draw(&region);
    }
    h.pipeline.finish_profile_frame(&h.context);
    let profiler = h.pipeline.profiler.as_mut().unwrap();
    profiler.drain(&h.context).unwrap();
    assert_eq!(
        profiler.completed.len(),
        3,
        "A busy ring skips rather than waits or grows"
    );
    for sample in profiler.completed.drain(..) {
        assert!(
            sample
                .gpu_us
                .iter()
                .all(|value| value.is_finite() && *value >= 0.)
        );
        assert!(sample.cpu_total_us >= sample.cpu_us[0]);
    }
    let frames = profile_frame(&mut h, &[region, region]);
    assert!(
        frames.is_some(),
        "Completed queries are reusable in the next frame"
    );
}

#[test]
fn planning_is_local_clipped_dpi_bounded_and_noop_safe() {
    let b = blur([360., 250., 240., 40.], 16.);
    let plan = Plan::new(&b, 3840, 2160).unwrap();
    assert_eq!(plan.copy, [312, 202, 336, 136]);
    assert_eq!(plan.work, [168, 68]);
    assert_eq!(plan.filtered, [22, 22, 124, 24]);
    assert_eq!(
        scratch_bytes(capacity([336, 136], [0; 2]), capacity([168, 68], [0; 2]), 4),
        688128
    );
    assert!(plan.copy[2] * plan.copy[3] < 3840 * 2160 / 100);
    for dpi in [1., 1.25, 1.5, 2., 3., 4.] {
        let p = Plan::new(
            &blur([360. * dpi, 250. * dpi, 240. * dpi, 40. * dpi], 16. * dpi),
            (3840. * dpi) as u32,
            (2160. * dpi) as u32,
        )
        .unwrap();
        assert!(p.sigma.into_iter().all(|v| v <= 8.));
        assert!(
            p.work[0] <= 170 && p.work[1] <= 70,
            "DPI should not square filter work"
        );
    }
    let mut invalid = b;
    for sigma in [0., -1., f32::NAN, f32::INFINITY] {
        invalid.sigma = ScaledPixels(sigma);
        assert!(Plan::new(&invalid, 3840, 2160).is_none());
    }
    assert!(Plan::new(&blur([4000., 0., 40., 40.], 16.), 3840, 2160).is_none());
}

#[test]
fn warp_shader_matches_gaussian_impulse_without_touching_outside_roi() {
    let mut h = Harness::new(64, 64);
    let original = h.upload(|x, _| if x == 32 { [255; 4] } else { [0, 0, 0, 255] });
    h.draw(&blur([12., 12., 40., 40.], 2.));
    let result = h.read();
    let weights: Vec<_> = (-6..=6)
        .map(|i| (-0.5 * (i as f32 / 2.).powi(2)).exp())
        .collect();
    let sum: f32 = weights.iter().sum();
    for x in 26..=38 {
        let expected = (255. * weights[x - 26] / sum).round() as i32;
        let actual = pixel(&result, 64, x, 32);
        assert!(
            (i32::from(actual[0]) - expected).abs() <= 2,
            "x={x}, {actual:?} vs {expected}"
        );
        assert_eq!(actual[3], 255);
    }
    for y in 0..64 {
        for x in 0..64 {
            if !(12..52).contains(&x) || !(12..52).contains(&y) {
                assert_eq!(pixel(&result, 64, x, y), pixel(&original, 64, x, y));
            }
        }
    }
}

#[test]
fn warp_round_clip_opacity_and_premultiplied_alpha_are_preserved() {
    let mut h = Harness::new(64, 64);
    let original = h.upload(|x, _| if x >= 32 { [128; 4] } else { [0, 0, 0, 128] });
    let mut b = blur([12., 12., 40., 40.], 3.);
    b.quad.corner_radii = Corners::all(ScaledPixels(20.));
    b.quad.content_mask.bounds = Bounds::new(
        point(ScaledPixels(12.), ScaledPixels(12.)),
        size(ScaledPixels(24.), ScaledPixels(40.)),
    );
    b.opacity = 0.5;
    h.draw(&b);
    let result = h.read();
    assert!(pixel(&result, 64, 31, 32)[0] > 15);
    assert!(pixel(&result, 64, 31, 32)[0] < 35);
    for y in 0..64 {
        for x in 0..64 {
            let p = pixel(&result, 64, x, y);
            assert_eq!(p[3], 128, "Filtering cannot accumulate alpha");
            if x >= 36 || (x == 13 && y == 13) {
                assert_eq!(p, pixel(&original, 64, x, y));
            }
        }
    }
}

#[test]
fn warp_reduced_resolution_edges_and_scratch_reuse_are_stable() {
    let mut h = Harness::new(400, 240);
    h.upload(|_, _| [63, 99, 121, 128]);
    h.draw(&blur([0., 0., 240., 40.], 16.));
    assert_eq!(h.pipeline.stats.texture_allocations, 3);
    let pointer = h
        .pipeline
        .scratch
        .as_ref()
        .unwrap()
        .capture
        .texture
        .as_raw();
    // A smaller region and fractional move reuse allocations, no stale padding.
    h.draw(&blur([0.5, 0.5, 238., 38.], 16.));
    assert_eq!(h.pipeline.stats.texture_allocations, 3);
    assert_eq!(
        h.pipeline
            .scratch
            .as_ref()
            .unwrap()
            .capture
            .texture
            .as_raw(),
        pointer
    );
    let result = h.read();
    for p in result.chunks_exact(4) {
        for (a, b) in p.iter().zip([63, 99, 121, 128]) {
            assert!(
                (i32::from(*a) - b).abs() <= 1,
                "No black or stale edge: {p:?}"
            );
        }
    }
    assert_eq!(h.pipeline.stats.regions, 2);
    let kernels = h.pipeline.stats.kernel_rebuilds;
    h.draw(&blur([0.5, 0.5, 238., 38.], 16.));
    assert_eq!(
        h.pipeline.stats.kernel_rebuilds, kernels,
        "Unchanged sigma reuses CPU weights too"
    );
    assert_eq!(h.pipeline.stats.texture_allocations, 3);
    assert!(h.pipeline.stats.work_pixels < h.pipeline.stats.copied_pixels);
    h.pipeline.resize_scratch([390, 230]);
    assert_eq!(
        h.pipeline
            .scratch
            .as_ref()
            .unwrap()
            .capture
            .texture
            .as_raw(),
        pointer,
        "Window resize must not reallocate still-useful local scratch"
    );
    h.pipeline.resize_scratch([64, 64]);
    assert!(h.pipeline.scratch.is_none());
}

#[test]
fn oversized_region_does_not_allocate_or_disable_smaller_regions() {
    let mut h = Harness::new(64, 64);
    let oversized = Plan::new(&blur([0., 0., 8192., 8192.], 1.), 8192, 8192).unwrap();
    assert!(
        !h.pipeline
            .ensure_scratch(&h.device, oversized, RENDER_TARGET_FORMAT)
            .unwrap()
    );
    assert!(h.pipeline.scratch.is_none());
    assert_eq!(h.pipeline.stats.texture_allocations, 0);
    assert_eq!(h.pipeline.stats.skipped_regions, 1);
    h.upload(|_, _| [100, 100, 100, 255]);
    h.draw(&blur([12., 12., 32., 32.], 8.));
    assert_eq!(h.pipeline.stats.regions, 1);
    assert!(h.pipeline.stats.peak_scratch_bytes < MAX_SCRATCH_BYTES);
}

#[test]
fn cropped_work_matches_full_passes_even_with_poisoned_unused_texels() {
    let mut h = Harness::new(256, 192);
    h.pipeline.force_crop = true;
    let cases = [
        [60., 70., 100., 18.],
        [60.25, 70.75, 100.5, 18.25],
        [0., 0., 39., 27.],
        [230.75, 170.25, 40., 40.],
        [-4.25, 70.1, 9.5, 12.],
        [100.3, 40.7, 1.2, 0.9],
        [30., 20., 180., 140.],
        [72.5, 58.75, 32., 32.],
    ];
    for (case, rect) in cases.into_iter().enumerate() {
        for sigma in [0.25, 1., 2., 7.9, 8., 16., 32., 64.] {
            h.upload(|x, y| {
                let alpha = 90 + ((x * 7 + y * 11 + case as u32 * 13) % 166) as u8;
                [
                    ((x * 29 + y * 5) % (u32::from(alpha) + 1)) as u8,
                    ((x * 3 + y * 37) % (u32::from(alpha) + 1)) as u8,
                    ((x ^ y) % (u32::from(alpha) + 1)) as u8,
                    alpha,
                ]
            });
            h.save_background();
            let mut region = blur(rect, sigma);
            if case % 2 == 1 {
                region.opacity = 0.57;
                region.quad.corner_radii = Corners::all(ScaledPixels(11.5));
                region.quad.content_mask.bounds = Bounds::new(
                    point(ScaledPixels(rect[0] + 0.4), ScaledPixels(rect[1] + 0.2)),
                    size(ScaledPixels(rect[2] - 0.5), ScaledPixels(rect[3] - 0.3)),
                );
            }
            let plan = Plan::new(&region, 256, 192).unwrap();
            h.pipeline
                .ensure_scratch(&h.device, plan, RENDER_TARGET_FORMAT)
                .unwrap();
            let scratch = h.pipeline.scratch.as_mut().unwrap();
            // A tiny floating-point interpolation spill must not read stale
            // or uninitialized NaNs just beyond a partially drawn rectangle.
            for texture in [&scratch.horizontal, &scratch.vertical] {
                let [width, height] = texture.capacity;
                let mut bits = vec![0u16; (width * height * 4) as usize];
                for y in 0..plan.work[1] {
                    for x in 0..plan.work[0] {
                        bits[((y * width + x) * 4) as usize..((y * width + x) * 4 + 4) as usize]
                            .fill(0x7e00);
                    }
                }
                unsafe {
                    h.context.UpdateSubresource(
                        &texture.texture,
                        0,
                        None,
                        bits.as_ptr().cast(),
                        width * 8,
                        0,
                    );
                }
            }
            scratch.initialized = true; // Explicit fixture data already includes finite bucket padding.
            h.pipeline.full_work_reference = false;
            h.draw(&region);
            let optimized = h.read();
            h.reset_background();
            h.pipeline.full_work_reference = true;
            h.draw(&region);
            let reference = h.read();
            let delta = optimized
                .iter()
                .zip(&reference)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            // Exact physical texel centers replace interpolated UVs. On sharp
            // 8-bit noise, floating-point/half-float rounding may differ by 2.
            assert!(
                delta <= 2,
                "case={case}, sigma={sigma}: max channel error={delta}"
            );
        }
    }
}

#[test]
fn new_half_float_padding_is_initialized_once_and_not_per_frame() {
    let mut h = Harness::new(256, 192);
    let original = h.upload(|_, _| [63, 99, 121, 128]);
    let region = blur([32., 24., 180., 140.], 2.);
    let plan = Plan::new(&region, 256, 192).unwrap();
    h.pipeline
        .ensure_scratch(&h.device, plan, RENDER_TARGET_FORMAT)
        .unwrap();
    let scratch = h.pipeline.scratch.as_ref().unwrap();
    unsafe {
        h.context
            .ClearRenderTargetView(scratch.horizontal.rtv.as_ref().unwrap(), &[f32::NAN; 4]);
        h.context
            .ClearRenderTargetView(scratch.vertical.rtv.as_ref().unwrap(), &[f32::NAN; 4]);
    }
    h.draw(&region);
    assert_eq!(h.pipeline.stats.initialization_clears, 2);
    let result = h.read();
    assert!(
        result
            .iter()
            .zip(&original)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    h.draw(&region);
    assert_eq!(
        h.pipeline.stats.initialization_clears, 2,
        "No per-frame full clears"
    );
    assert_eq!(h.pipeline.stats.texture_allocations, 3);
}

#[test]
fn warp_next_region_reads_the_updated_framebuffer() {
    let mut h = Harness::new(64, 64);
    h.upload(|x, _| if x == 32 { [255; 4] } else { [0, 0, 0, 255] });
    let b = blur([8., 8., 48., 48.], 2.);
    h.draw(&b);
    let first = h.read();
    h.draw(&b);
    let second = h.read();
    assert!(pixel(&second, 64, 32, 32)[0] < pixel(&first, 64, 32, 32)[0]);
    assert!(pixel(&second, 64, 28, 32)[0] > pixel(&first, 64, 28, 32)[0]);
}

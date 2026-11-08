//! Yue same-window backdrop filter. GPU-only ROI copy, separable Gaussian,
//! ordered replacement composite. One scratch set is shared by all surfaces.
use super::*;

#[path = "backdrop_profile.rs"]
mod profile;

const WORK_SIGMA: f32 = 8.;
const PAIRS: usize = 12; // ceil(3 * WORK_SIGMA) / 2
const BUCKET: u32 = 64;
// Per-window retained scratch budget. Oversized requests retain their normal
// tint without disabling subsequent small filters or risking allocation churn.
const MAX_SCRATCH_BYTES: u64 = 64 * 1024 * 1024;
const KERNEL_CACHE: usize = 8;
// Rasterizing thin partial quads did not improve the measured small surfaces.
const CROP_MIN_WORK_PIXELS: u64 = 16 * 1024;

#[derive(Clone, Copy, Debug)]
struct Plan {
    output: [f32; 4],
    copy: [u32; 4],
    work: [u32; 2],
    filtered: [u32; 4],
    sigma: [f32; 2],
}

impl Plan {
    fn new(blur: &BackdropBlur, width: u32, height: u32) -> Option<Self> {
        if !blur.sigma.0.is_finite()
            || blur.sigma.0 <= 0.
            || !blur.opacity.is_finite()
            || blur.opacity <= 0.
        {
            return None;
        }
        let bounds = blur.quad.bounds.intersect(&blur.quad.content_mask.bounds);
        let left = bounds.left().0.max(0.);
        let top = bounds.top().0.max(0.);
        let right = bounds.right().0.min(width as f32);
        let bottom = bounds.bottom().0.min(height as f32);
        if !(left < right && top < bottom) {
            return None;
        }
        // Include background outside the visible glass, never zero-pad the ROI.
        let pad = (3. * blur.sigma.0).ceil();
        let x = (left.floor() - pad).max(0.) as u32;
        let y = (top.floor() - pad).max(0.) as u32;
        let r = (right.ceil() + pad).min(width as f32) as u32;
        let b = (bottom.ceil() + pad).min(height as f32) as u32;
        let size = [(r - x).max(1), (b - y).max(1)];
        let factor = (blur.sigma.0 / WORK_SIGMA).max(1.);
        // Floor, rather than ceil: effective sigma cannot exceed WORK_SIGMA.
        let work = size.map(|s| ((s as f32 / factor).floor() as u32).max(1));
        let sigma = [
            (blur.sigma.0 * work[0] as f32 / size[0] as f32).min(WORK_SIGMA),
            (blur.sigma.0 * work[1] as f32 / size[1] as f32).min(WORK_SIGMA),
        ];
        // Include every reconstruction texel plus two guards (one for each
        // intermediate/reconstruction interpolation). Source
        // Gaussian halos and the physical/work sampling grids stay unchanged.
        let scale = [
            work[0] as f32 / size[0] as f32,
            work[1] as f32 / size[1] as f32,
        ];
        let start = [
            (((left - x as f32) * scale[0]).floor() as u32).saturating_sub(2),
            (((top - y as f32) * scale[1]).floor() as u32).saturating_sub(2),
        ];
        let end = [
            ((((right - x as f32) * scale[0]).ceil() as u32).saturating_add(2)).min(work[0]),
            ((((bottom - y as f32) * scale[1]).ceil() as u32).saturating_add(2)).min(work[1]),
        ];
        Some(Self {
            output: [left, top, right - left, bottom - top],
            copy: [x, y, size[0], size[1]],
            work,
            filtered: [start[0], start[1], end[0] - start[0], end[1] - start[1]],
            sigma,
        })
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Params {
    target: [f32; 4],
    viewport: [f32; 2],
    mode: u32,
    pair_count: u32,
    source_size: [f32; 2],
    source_capacity: [f32; 2],
    capture_origin: [f32; 2],
    capture_capacity: [f32; 2],
    capture_size: [f32; 2],
    work_size: [f32; 2],
    bounds: [f32; 4],
    radii: [f32; 4],
    step: [f32; 2],
    center_weight: f32,
    opacity: f32,
    clip: [f32; 4],
    pairs: [[f32; 4]; PAIRS],
}

#[derive(Clone, Copy)]
struct Kernel {
    sigma: f32,
    center: f32,
    count: u32,
    pairs: [[f32; 4]; PAIRS],
}

impl Kernel {
    fn new(sigma: f32) -> Self {
        let mut weights = [0.; PAIRS * 2 + 1];
        let radius = ((sigma * 3.).ceil() as usize).min(PAIRS * 2);
        weights[0] = 1.;
        for (i, weight) in weights.iter_mut().enumerate().take(radius + 1).skip(1) {
            *weight = (-0.5 * (i as f32 / sigma.max(0.0001)).powi(2)).exp();
        }
        let total = 1. + 2. * weights[1..=radius].iter().sum::<f32>();
        let mut pairs = [[0.; 4]; PAIRS];
        let count = radius.div_ceil(2);
        for (pair, out) in pairs.iter_mut().enumerate().take(count) {
            let first = 2 * pair + 1;
            let a = weights[first];
            let b = weights.get(first + 1).copied().unwrap_or(0.);
            let sum = a + b;
            *out = [
                first as f32 + if sum > 0. { b / sum } else { 0. },
                sum / total,
                0.,
                0.,
            ];
        }
        Self {
            sigma,
            center: 1. / total,
            count: count as u32,
            pairs,
        }
    }
    fn apply(self, params: &mut Params) {
        params.center_weight = self.center;
        params.pair_count = self.count;
        params.pairs = self.pairs;
    }
}

struct Texture {
    texture: ID3D11Texture2D,
    srv: Option<ID3D11ShaderResourceView>,
    rtv: Option<ID3D11RenderTargetView>,
    capacity: [u32; 2],
    format: DXGI_FORMAT,
}
impl Texture {
    fn new(
        device: &ID3D11Device,
        capacity: [u32; 2],
        format: DXGI_FORMAT,
        render: bool,
    ) -> Result<Self> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: capacity[0],
            Height: capacity[1],
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0
                | if render {
                    D3D11_BIND_RENDER_TARGET.0
                } else {
                    0
                }) as u32,
            ..Default::default()
        };
        let mut texture = None;
        let mut srv = None;
        let mut rtv = None;
        unsafe {
            device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.as_ref().unwrap();
            device.CreateShaderResourceView(texture, None, Some(&mut srv))?;
            if render {
                device.CreateRenderTargetView(texture, None, Some(&mut rtv))?;
            }
        }
        Ok(Self {
            texture: texture.unwrap(),
            srv,
            rtv,
            capacity,
            format,
        })
    }
}

struct Scratch {
    capture: Texture,
    horizontal: Texture,
    vertical: Texture,
    initialized: bool,
}

/// No resources exist until a visible filter is encountered. The renderer owns
/// this state, not the consumer; no copies of ImagePreview image data are kept.
pub(super) struct BackdropPipeline {
    vertex: ID3D11VertexShader,
    fragment: ID3D11PixelShader,
    cropped_fragment: Option<ID3D11PixelShader>,
    constants: Option<ID3D11Buffer>,
    sampler: Option<ID3D11SamplerState>,
    scratch: Option<Scratch>,
    kernels: [Option<Kernel>; KERNEL_CACHE],
    next_kernel: usize,
    warned_budget: bool,
    profiler: Option<profile::Profiler>,
    #[cfg(test)]
    full_work_reference: bool,
    #[cfg(test)]
    force_crop: bool,
    pub(super) stats: Stats,
}

#[derive(Default, Debug)]
pub(super) struct Stats {
    regions: u32,
    copied_pixels: u64,
    work_pixels: u64,
    texture_allocations: u32,
    skipped_regions: u32,
    peak_scratch_bytes: u64,
    kernel_rebuilds: u32,
    initialization_clears: u32,
}

fn capacity(required: [u32; 2], previous: [u32; 2]) -> [u32; 2] {
    [
        required[0].max(previous[0]).div_ceil(BUCKET) * BUCKET,
        required[1].max(previous[1]).div_ceil(BUCKET) * BUCKET,
    ]
}

fn scratch_bytes(capture: [u32; 2], work: [u32; 2], capture_bpp: u64) -> u64 {
    u64::from(capture[0]) * u64::from(capture[1]) * capture_bpp
        + u64::from(work[0]) * u64::from(work[1]) * 16
}

impl BackdropPipeline {
    pub(super) fn finish_profile_frame(&mut self, context: &ID3D11DeviceContext) {
        if let Some(profiler) = &mut self.profiler {
            profiler.end_frame(context);
        }
    }

    pub(super) fn resize_scratch(&mut self, viewport: [u32; 2]) {
        let limit = capacity(viewport, [0; 2]);
        if self
            .scratch
            .as_ref()
            .is_some_and(|s| s.capture.capacity[0] > limit[0] || s.capture.capacity[1] > limit[1])
        {
            self.scratch = None;
        }
    }
    pub(super) fn new(device: &ID3D11Device) -> Result<Self> {
        let vertex = create_vertex_shader(
            device,
            RawShaderBytes::new(ShaderModule::BackdropBlur, ShaderTarget::Vertex)?.as_bytes(),
        )?;
        let fragment = create_fragment_shader(
            device,
            RawShaderBytes::new(ShaderModule::BackdropBlur, ShaderTarget::Fragment)?.as_bytes(),
        )?;
        let constants = create_constant_buffer::<Params>(device)?;
        let mut sampler = None;
        unsafe {
            device.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                },
                Some(&mut sampler),
            )?;
        }
        Ok(Self {
            vertex,
            fragment,
            cropped_fragment: None,
            constants,
            sampler,
            scratch: None,
            kernels: [None; KERNEL_CACHE],
            next_kernel: 0,
            warned_budget: false,
            profiler: profile::Profiler::from_env(device),
            #[cfg(test)]
            full_work_reference: false,
            #[cfg(test)]
            force_crop: false,
            stats: Stats::default(),
        })
    }

    fn kernel(&mut self, sigma: f32) -> Kernel {
        if let Some(kernel) = self.kernels.iter().flatten().find(|k| k.sigma == sigma) {
            return *kernel;
        }
        let kernel = Kernel::new(sigma);
        self.kernels[self.next_kernel] = Some(kernel);
        self.next_kernel = (self.next_kernel + 1) % KERNEL_CACHE;
        self.stats.kernel_rebuilds += 1;
        kernel
    }

    fn ensure_scratch(
        &mut self,
        device: &ID3D11Device,
        plan: Plan,
        format: DXGI_FORMAT,
    ) -> Result<bool> {
        let capture_bpp = match format {
            DXGI_FORMAT_B8G8R8A8_UNORM => 4,
            DXGI_FORMAT_R16G16B16A16_FLOAT => 8,
            _ => anyhow::bail!("unsupported backdrop target format"),
        };
        let old = self.scratch.as_ref().filter(|s| s.capture.format == format);
        let mut capture = capacity(
            [plan.copy[2], plan.copy[3]],
            old.map_or([0; 2], |s| s.capture.capacity),
        );
        let mut work = capacity(plan.work, old.map_or([0; 2], |s| s.horizontal.capacity));
        if scratch_bytes(capture, work, capture_bpp) > MAX_SCRATCH_BYTES {
            // Wide then tall surfaces must not grow a historical square forever.
            capture = capacity([plan.copy[2], plan.copy[3]], [0; 2]);
            work = capacity(plan.work, [0; 2]);
            if scratch_bytes(capture, work, capture_bpp) > MAX_SCRATCH_BYTES {
                self.stats.skipped_regions += 1;
                if !self.warned_budget {
                    log::warn!(
                        "Backdrop region exceeds 64 MiB scratch budget; keeping tint for that region"
                    );
                    self.warned_budget = true;
                }
                return Ok(false);
            }
        }
        self.stats.peak_scratch_bytes =
            self.stats
                .peak_scratch_bytes
                .max(scratch_bytes(capture, work, capture_bpp));
        if old.is_some_and(|s| s.capture.capacity == capture && s.horizontal.capacity == work) {
            return Ok(true);
        }
        // Allocate coherently before replacing the old set. Buckets eliminate
        // per-pixel reallocation during drags and repeated small layout changes.
        self.scratch = Some(Scratch {
            capture: Texture::new(device, capture, format, false)?,
            horizontal: Texture::new(device, work, DXGI_FORMAT_R16G16B16A16_FLOAT, true)?,
            vertical: Texture::new(device, work, DXGI_FORMAT_R16G16B16A16_FLOAT, true)?,
            initialized: false,
        });
        self.stats.texture_allocations += 3;
        Ok(true)
    }

    fn pass(
        constants: &Option<ID3D11Buffer>,
        context: &ID3D11DeviceContext,
        params: &Params,
        target: &Option<ID3D11RenderTargetView>,
        source: &Option<ID3D11ShaderResourceView>,
        second: &Option<ID3D11ShaderResourceView>,
    ) -> Result<()> {
        update_buffer(
            context,
            constants.as_ref().unwrap(),
            slice::from_ref(params),
        )?;
        unsafe {
            // Unbind before changing RTs: never bind the same subresource as
            // input and output, including across consecutive glass regions.
            context.PSSetShaderResources(2, Some(&[None, None]));
            context.OMSetRenderTargets(Some(slice::from_ref(target)), None);
            context.PSSetShaderResources(2, Some(&[source.clone(), second.clone()]));
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: params.viewport[0],
                Height: params.viewport[1],
                MinDepth: 0.,
                MaxDepth: 1.,
                ..Default::default()
            }]));
            context.Draw(4, 0);
        }
        Ok(())
    }

    pub(super) fn draw(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        render_target: &ID3D11Texture2D,
        target_view: &Option<ID3D11RenderTargetView>,
        viewport: D3D11_VIEWPORT,
        blur: &BackdropBlur,
    ) -> Result<()> {
        let prepare_start = self.profiler.as_ref().map(|_| std::time::Instant::now());
        let Some(plan) = Plan::new(blur, viewport.Width as u32, viewport.Height as u32) else {
            return Ok(());
        };
        let crop = u64::from(plan.work[0]) * u64::from(plan.work[1]) >= CROP_MIN_WORK_PIXELS
            && plan.filtered != [0, 0, plan.work[0], plan.work[1]];
        #[cfg(test)]
        let crop = (crop || self.force_crop) && !self.full_work_reference;
        let plan = if crop {
            plan
        } else {
            Plan {
                filtered: [0, 0, plan.work[0], plan.work[1]],
                ..plan
            }
        };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe {
            render_target.GetDesc(&mut desc);
        }
        if !self.ensure_scratch(device, plan, desc.Format)? {
            return Ok(());
        }
        if let Some(scratch) = self.scratch.as_mut().filter(|scratch| !scratch.initialized) {
            // Uninitialized half-float padding can contain NaNs, which may
            // leak through nominally zero-weight bilinear neighbors.
            unsafe {
                context.ClearRenderTargetView(scratch.horizontal.rtv.as_ref().unwrap(), &[0.; 4]);
                context.ClearRenderTargetView(scratch.vertical.rtv.as_ref().unwrap(), &[0.; 4]);
            }
            scratch.initialized = true;
            self.stats.initialization_clears += 2;
        }
        if crop && self.cropped_fragment.is_none() {
            self.cropped_fragment = Some(create_fragment_shader(
                device,
                RawShaderBytes::new(ShaderModule::BackdropCrop, ShaderTarget::Fragment)?.as_bytes(),
            )?);
        }
        let kernels = plan.sigma.map(|sigma| self.kernel(sigma));
        let fragment = if crop {
            self.cropped_fragment.as_ref().unwrap()
        } else {
            &self.fragment
        };
        if let Some(profiler) = &mut self.profiler {
            profiler.begin(
                context,
                plan.output,
                prepare_start.unwrap().elapsed().as_secs_f64() * 1e6,
            );
        }
        // All fallible resource allocations happened before changing bindings.
        let scratch = self.scratch.as_ref().unwrap();
        let [x, y, width, height] = plan.copy;
        unsafe {
            context.OMSetRenderTargets(None, None);
            context.PSSetShaderResources(2, Some(&[None, None]));
            context.CopySubresourceRegion(
                &scratch.capture.texture,
                0,
                0,
                0,
                0,
                render_target,
                0,
                Some(&D3D11_BOX {
                    left: x,
                    top: y,
                    front: 0,
                    right: x + width,
                    bottom: y + height,
                    back: 1,
                }),
            );
        }
        if let Some(profiler) = &mut self.profiler {
            profiler.mark(context, 1);
        }
        unsafe {
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            context.VSSetShader(&self.vertex, None);
            context.PSSetShader(fragment, None);
            // Replace, NOT source-over: filtered framebuffer already includes
            // the original alpha. Default blend state is disabled, write RGBA.
            context.OMSetBlendState(None, None, u32::MAX);
            context.VSSetConstantBuffers(2, Some(slice::from_ref(&self.constants)));
            context.PSSetConstantBuffers(2, Some(slice::from_ref(&self.constants)));
            context.PSSetSamplers(1, Some(slice::from_ref(&self.sampler)));
        }
        let result = (|| {
            let mut params = Params {
                target: [0., 0., plan.work[0] as f32, plan.work[1] as f32],
                viewport: scratch.horizontal.capacity.map(|n| n as f32),
                source_size: [width as f32, height as f32],
                source_capacity: scratch.capture.capacity.map(|n| n as f32),
                step: [
                    width as f32 / plan.work[0] as f32,
                    height as f32 / plan.work[1] as f32,
                ],
                ..Default::default()
            };
            // Bilinear Gaussian pairs are exact on adjacent texels. Downsample
            // first rather than pairing across non-adjacent original pixels.
            let source = if plan.work != [width, height] {
                params.mode = 2;
                Self::pass(
                    &self.constants,
                    context,
                    &params,
                    &scratch.vertical.rtv,
                    &scratch.capture.srv,
                    &None,
                )?;
                params.mode = 0;
                params.source_size = plan.work.map(|n| n as f32);
                params.source_capacity = scratch.vertical.capacity.map(|n| n as f32);
                &scratch.vertical.srv
            } else {
                &scratch.capture.srv
            };
            if let Some(profiler) = &mut self.profiler {
                profiler.mark(context, 2);
            }
            params.step = [1., 0.];
            params.target = [
                plan.filtered[0] as f32,
                0.,
                plan.filtered[2] as f32,
                plan.work[1] as f32,
            ];
            kernels[0].apply(&mut params);
            Self::pass(
                &self.constants,
                context,
                &params,
                &scratch.horizontal.rtv,
                source,
                &None,
            )?;
            if let Some(profiler) = &mut self.profiler {
                profiler.mark(context, 3);
            }
            params.source_size = plan.work.map(|n| n as f32);
            params.source_capacity = scratch.horizontal.capacity.map(|n| n as f32);
            params.step = [0., 1.];
            params.target = plan.filtered.map(|n| n as f32);
            kernels[1].apply(&mut params);
            Self::pass(
                &self.constants,
                context,
                &params,
                &scratch.vertical.rtv,
                &scratch.horizontal.srv,
                &None,
            )?;
            if let Some(profiler) = &mut self.profiler {
                profiler.mark(context, 4);
            }

            let b = blur.quad.bounds;
            let c = blur.quad.corner_radii;
            params.mode = 1;
            // Integer raster bounds, but preserve fractional clip in the shader.
            let [l, t, w, h] = plan.output;
            params.target = [
                l.floor(),
                t.floor(),
                (l + w).ceil() - l.floor(),
                (t + h).ceil() - t.floor(),
            ];
            params.viewport = [viewport.Width, viewport.Height];
            params.capture_origin = [x as f32, y as f32];
            params.capture_capacity = scratch.capture.capacity.map(|n| n as f32);
            params.capture_size = [width as f32, height as f32];
            params.work_size = plan.work.map(|n| n as f32);
            params.bounds = [b.left().0, b.top().0, b.size.width.0, b.size.height.0];
            params.radii = [
                c.top_left.0,
                c.top_right.0,
                c.bottom_right.0,
                c.bottom_left.0,
            ];
            params.opacity = blur.opacity.clamp(0., 1.);
            params.clip = plan.output;
            Self::pass(
                &self.constants,
                context,
                &params,
                target_view,
                &scratch.capture.srv,
                &scratch.vertical.srv,
            )?;
            if let Some(profiler) = &mut self.profiler {
                profiler.mark(context, 5);
            }
            Ok(())
        })();
        // Restore framebuffer/viewport even if a dynamic constant Map failed.
        // Subsequent ordinary pipelines bind their own shaders, blend and t0/s0.
        unsafe {
            context.PSSetShaderResources(2, Some(&[None, None]));
            context.OMSetRenderTargets(Some(slice::from_ref(target_view)), None);
            context.RSSetViewports(Some(slice::from_ref(&viewport)));
        }
        if let Some(profiler) = &mut self.profiler {
            profiler.end(result.is_ok());
        }
        if result.is_ok() {
            self.stats.regions += 1;
            self.stats.copied_pixels += u64::from(width) * u64::from(height);
            self.stats.work_pixels += u64::from(plan.filtered[2])
                * (u64::from(plan.work[1]) + u64::from(plan.filtered[3]))
                + if plan.work == [width, height] {
                    0
                } else {
                    u64::from(plan.work[0]) * u64::from(plan.work[1])
                };
        }
        result
    }
}

impl DirectXRenderer {
    pub(super) fn draw_filtered_quads(
        &mut self,
        scene: &Scene,
        range: std::ops::Range<usize>,
    ) -> Result<()> {
        if scene.backdrop_blurs.is_empty() {
            return self.draw_quads(range.start, range.len());
        }
        let first_order = scene.quads[range.start].order;
        let last_order = scene.quads[range.end - 1].order;
        let start = scene
            .backdrop_blurs
            .partition_point(|b| b.quad.order < first_order);
        let end = scene
            .backdrop_blurs
            .partition_point(|b| b.quad.order <= last_order);
        let mut cursor = range.start;
        for blur in &scene.backdrop_blurs[start..end] {
            let marker = cursor
                + scene.quads[cursor..range.end].partition_point(|q| q.order < blur.quad.order);
            if cursor < marker {
                self.draw_quads(cursor, marker - cursor)?;
            }
            self.draw_backdrop(blur);
            cursor = marker + 1; // transparent marker must not be shaded
        }
        if cursor < range.end {
            self.draw_quads(cursor, range.end - cursor)?;
        }
        Ok(())
    }

    fn draw_backdrop(&mut self, blur: &BackdropBlur) {
        if self.backdrop_failed {
            return;
        }
        let result = (|| -> Result<()> {
            let devices = self.devices.as_ref().context("devices missing")?;
            let resources = self.resources.as_ref().context("resources missing")?;
            if Plan::new(blur, self.width, self.height).is_none() {
                return Ok(());
            }
            if self.backdrop.is_none() {
                self.backdrop = Some(BackdropPipeline::new(&devices.device)?);
            }
            self.backdrop.as_mut().unwrap().draw(
                &devices.device,
                &devices.device_context,
                resources
                    .render_target
                    .as_ref()
                    .context("render target missing")?,
                &resources.render_target_view,
                resources.viewport,
                blur,
            )
        })();
        if let Err(error) = result {
            if let (Some(backdrop), Some(devices)) = (&mut self.backdrop, &self.devices) {
                backdrop.finish_profile_frame(&devices.device_context);
            }
            log::warn!("Backdrop blur disabled until device recreation: {error:#}");
            self.backdrop = None;
            self.backdrop_failed = true;
        }
    }
}

#[cfg(test)]
#[path = "backdrop_tests.rs"]
mod tests;

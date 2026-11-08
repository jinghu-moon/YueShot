# Yue Windows local Gaussian backdrop extension

Source: crates.io `gpui-pre-windows 0.3.9`, Zed revision
`93d745668805029476e6d7fbb19652116fed51de`. Original Apache-2.0 license retained
in `LICENSE-APACHE`. Cargo registry sources are untouched. Consumers must patch
both this backend and `gpui-pre`; see the workspace root Cargo.toml.

## Changes (2026-11-02)

- `src/directx_renderer.rs`: dispatch filtered quad markers at their exact paint
  order, lazy pipeline ownership, resize/device-lost cleanup, trace counters.
- `src/directx_renderer/backdrop.rs`: clipped framebuffer ROI copy, reusable
  bucketed textures, optional prefilter/downsample, horizontal/vertical paired
  Gaussian, rounded/opacity-aware replacement composite. No CPU readback, no
  animation timer, per-window 64 MiB retained scratch budget. An oversized
  region keeps its normal tint without disabling later small filters.
- `src/shaders.hlsl` and `build.rs`: shader entries and release FXC embedding;
  debug uses the existing runtime compiler. CLAMP sampling of valid texels is
  independent of existing sprite samplers; bindings and framebuffer are restored.
- `src/directx_renderer/backdrop_profile.rs`: optional CPU/stage GPU timing,
  disabled by default (`GPUI_BACKDROP_PROFILE=N` enables sampling every N regions).
  At most one disjoint interval per frame, 8 frame / 128 sample query slots,
  nonblocking DONOTFLUSH polling with explicit S_FALSE handling. Busy rings skip
  samples; diagnostics never request frames or wait on the GPU in production.

The first consumers are ImagePreview's floating toolbar and circular comparison
grip. This backend knows nothing about ImagePreview, Yue colors, icons or tokens.
It executes public GPUI `Styled::backdrop_blur`/`Window::paint_backdrop_blur`
commands. Other renderers retain tint but do not implement the filter.

## Verification and removal

`pwsh -NoProfile -File tools/test-gpui-backdrop.ps1` runs production-pipeline
D3D11 WARP tests without HWND, swap chain or user UI. `-Release` also verifies
the embedded precompiled shader path. Tests cover Gaussian pixel values,
clip/rounding/alpha, repeated framebuffer sampling, pooled allocation, DPI cost
and the allocation budget. Core Scene ordering and paint snapshot replay are
covered by `tools/test-gpui-snapshot.ps1 -Offline`.

`tools/test-gpui-backdrop.ps1 -Profile` explicitly runs the Release hardware
timestamp benchmark (no WARP fallback), contrasting per-stage profiling with
one timestamp pair around all filters while profiling is disabled. Only this
offscreen benchmark flushes/drains queries, outside CPU submission timing.
First measurements and limitations are recorded in `docs/26-backdrop-blur.md`.

### Measured optimization (2026-11-02)

Large work grids (at least 16,384 texels) trim horizontal output columns and
vertical output rows/columns to final reconstruction, with two texel guards.
The original capture, Gaussian weights, work resolution and replacement
composite stay intact. A lazily created `backdrop_crop` shader uses physical
pixel centers; small surfaces retain the original shader. New half-float work
allocations are cleared once so unused padding is finite, never per frame.

The opt-in benchmark now alternates same-device optimized/reference frames
(AB/BA) with both variants' stage profiling disabled. Three runs on RTX 4070
Ti SUPER measured 5.43–5.78% lower GPU P50 for the 1200x800 panel, without a
confirmed improvement for the toolbar/grip. It is not an application FPS claim.
See the design document and `docs/performance/2026-11-02-backdrop-optimization.log`.
Pixel tests compare fractional/edge/round/alpha cases with poisoned unused
texels and verify one-time padding initialization. Existing alpha and outside-ROI
assertions are unchanged. Loop unrolling and direct-source blending experiments
were not retained.

Remove only after an upstream version provides equivalent same-window filtering,
order barriers, clipping/opacity and bounded resource reuse, with both test
suites passing. Do not replace it with a whole-window backdrop material or a
preblurred source image. WARP timings are not hardware GPU performance claims.

## Full-frame nearest and precise-linear sampling (2026-11-03)

The production polychrome sprite shader honors bit 0 of the existing instance
`pad` field. It derives source texels from fragment position and full-image
bounds, then uses `Texture.Load`, clamped to that atlas tile. The ordinary zero
flag continues through the original linear sampler. Grayscale, image alpha,
element opacity, viewport clipping and outer-corner coverage remain supported;
comparison inner edges do not gain a rounded cutout. No extra sampler, texture,
GPU pass or changed instance layout is introduced.

Bit 1 selects precise linear sampling with the same full-frame geometry. Four
texels are loaded and clamped within the atlas tile; RGB is alpha-associated
before bilinear interpolation and returned as straight RGB to the existing
blend. Invisible RGB cannot tint transparent edges, and adjacent atlas images
cannot bleed into the sample. Source alpha and element opacity remain applied
once. The zero-flag path retains the original sampler.

`src/directx_renderer/image_sampling_tests.rs` runs six production-pipeline
D3D11 WARP tests without HWND or swap chain. They check exact enlarged texels,
linear-default distinction, fractional pan/crop, no atlas-neighbor bleed, rounded
coverage, comparison inner edges, source alpha, opacity, grayscale, exact 1:1
linear output and hidden-RGB transparency. Run Debug
and Release (the latter selects the embedded FXC shader) from the repository:

```powershell
cargo test --manifest-path vendor/gpui-pre-windows/Cargo.toml --target-dir target --lib image_sampling_tests --features test-support --offline --config "patch.crates-io.gpui-pre.path='D:/100_Projects/110_Daily/Yue-GPUI-Component/vendor/gpui-pre'" --config profile.dev.debug=0 --config profile.test.debug=0
# Repeat with --release for the embedded shader path.
```

The absolute patch path must refer to the current checkout's maintained core.
Snapshot capture/replay is checked by `tools/test-gpui-snapshot.ps1 -Offline`.
These are pixel/contract checks, not performance measurements or user UI review.

## FP16/scRGB output and full linear composition (2026-11-03)

Opt-in `AutomaticHdr` uses the current window's Win11 Advanced Color state,
SDR white and peak, with runtime interop activation, PRESENT support checks and
checked `SetColorSpace1`. SDR/WCG uses relative-linear FP16 when available;
HDR uses absolute scRGB FP16. Unknown/API/output failures explicitly use SDR.
Notifications are coalesced onto the HWND owner thread; subscriptions, WinRT
initialization and the DispatcherQueue have bounded platform/window lifetimes.
Output generations update shader parameters and invalidate cached presentation.

FP16 atlas storage/pitch, swapchain/resize/MSAA path textures, capture/backdrop
scratch and typed readback use real bytes per pixel. UI/gradients/shadows/path/
monochrome and grayscale-fallback subpixel text compose in linear space. SDR
image filtering linearizes before alpha association; float sources are not
decoded twice. SDR mapping has fixed independent CPU/GPU reference vectors.
Target alpha uses source-over, fixing accumulation above one in legacy blends.
`render_to_image` explicitly re-renders SDR into temporary resources and restores
the original FP16 target; `render_to_native_image` keeps associated native pixels.

```powershell
pwsh -NoProfile -File tools/test-gpui-hdr.ps1
pwsh -NoProfile -File tools/test-gpui-hdr.ps1 -Release
# Opt-in hidden hardware HWND integration; no ShowWindow/Present/HDR toggle:
pwsh -NoProfile -File tools/test-gpui-hdr.ps1 -Native
pwsh -NoProfile -File tools/test-gpui-hdr.ps1 -Native -Release
```

DirectX WARP suites: 31 pass per build plus one color-policy test. The ignored
native test separately passed on this machine in its current SDR state; injected
HDR snapshots cover resource switching, not physical HDR presentation. See
`docs/performance/2026-11-03-hdr-resources.md` for measurements/limits. Real HDR
on/off, cross-display, DPI, actual driver loss and end-to-end resource costs
remain acceptance gates. Remove this patch only after upstream provides the
same explicit encoding/output contract and all SDR/FP16 regression gates pass.

## Persistent nonactivation for pin/tool windows (2026-11-03)

Core `WindowOptions::is_activatable` defaults to true. Windows false is enforced
before native creation, on initial/deferred show, in `WM_MOUSEACTIVATE` and on
explicit activation requests. Nonactivating windows retain pointer input but
never enter the ordinary SetFocus/SetForegroundWindow/synthetic-Alt path.
The policy also sets TOOLWINDOW and removes APPWINDOW before creation.

`tools/test-gpui-window.ps1` checks the default contract. `-Native` explicitly
opts into a short-lived real GPUI popup: it checks creation styles, hidden and
initial show, subsequent show without focus theft, and the real message handler.
A hidden ordinary window is the negative control for unchanged activation.
One native probe passed; the full ordinary backend suite passed 51 with 3
ignored. See `docs/performance/2026-11-03-pin-window-contracts.md` for the isolated
core used during concurrent list work and the limits of these assertions.
Remove only when upstream supports the same lifetime-wide nonactivation policy.

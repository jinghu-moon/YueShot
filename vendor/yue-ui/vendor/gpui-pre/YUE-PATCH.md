# Yue GPUI paint snapshot extension

## Measured geometry cross-constraint reader (2026-11-04)

`ListItemGeometry::cross_extent()` exposes the constraint already stored with
the native size or size hint. Width is the normalized cross axis on both native
axis projections; only `is_measured()` proves actual measurement. No second
measurement map is introduced. Frozen geometry retains its original constraint
after an equal-height update at another width or demotion to an estimate.
Base uses this fact for idempotent size updates and portable-cache validation.
The explicit native virtualization integration suite now has seven contracts;
the new constraint test and Base/Yue cache resize/restore tests must pass before
an upstream API replaces this reader. See
`docs/research/2026-11-04-virtual-list-followup-fixes.md`.

Source: crates.io `gpui-pre 0.3.9`, snapshot `zed@93d7456`. The original
Apache-2.0 license is retained in `LICENSE-APACHE`. No Cargo registry source was
edited. The consuming workspace must patch `gpui-base`, `gpui-pre`, and
`gpui-pre-windows` (for the local Gaussian backdrop effect).

## Paint-only exit frame (2026-11-02)

`Window::capture_paint(origin, paint)` records scene commands while painting
normally. `Window::paint_snapshot(snapshot, new_origin)` translates those commands
and intersects their masks with the current content mask. It rejects other
windows and display scales. It does not restore any descendant element, focus,
Tab registration, input/IME handler, mouse listener, accessibility node, tooltip,
deferred popup, subscription or frame callback. This is a short-lived frozen
paint frame, without GPU readback or a bitmap screenshot.

Paths translate vertices as well as bounds and masks. Glyph/SVG sprites retain
their existing atlas rendering. Images retain their decoded `RenderImage` and
paint arguments and go through `paint_image` again on replay, so eviction and
atlas tile reuse remain safe. Frame paint metadata follows `reuse_paint`, which
also covers images inside cached views and nested captures.

`MotionDisclosure` captures only its live panel's paint, then detaches that
panel immediately on close. The frozen paint is clipped as height decreases.
Completion, geometry invalidation, width changes and motion-disabled landing
discard it. Reopening returns to live content at the current visible height.
Deferred overlays are never captured. Current-window placement can change;
the snapshot follows its panel origin rather than remaining at old coordinates.

Changed upstream files: `src/window.rs` (small image metadata and reuse hooks),
`src/scene.rs` (translation/mask handling), and the new
`src/window/paint_snapshot.rs`. A test-only primitive observation complements the
existing `painted_quads` API. The later backdrop extension is tracked below.

Targeted backend regressions:

```powershell
pwsh -NoProfile -File tools/test-gpui-snapshot.ps1 -Offline
```

The isolated integration fixture uses the production backend and consuming
workspace's dependency pins. It avoids unrelated upstream unit tests whose font
assets and dev-only macros are absent from the crates.io package.

Tests exercise path/sprite/quad translation and clipping, image eviction/reload,
cached paint image ownership, and replay without new input registrations at
1 / 1.5 / 2 display scales. Yue's Collapse headless target additionally exercises
the actual controlled component, input detachment, reversal, reduced motion,
frame scheduling and multi-panel closing. These checks do not replace user UI,
GPU pixel, native-reader or IME verification.

## Same-window backdrop foundation (2026-11-02)

`Styled::backdrop_blur(AbsoluteLength)` and `Window::paint_backdrop_blur` emit a
filter before the element's own shadow, tint and children. Sigma is a logical
Gaussian standard deviation, resolved through rem/DPI, capped at 64 logical
pixels; zero, negative and non-finite values or zero opacity emit nothing.

`Style`/`StyleRefinement` carry the effect without selecting product colors.
`Scene` retains a `BackdropBlur` primitive plus a transparent `Quad` marker,
preserving the existing Quad GPU ABI and PrimitiveBatch interface. Every filter
creates a global draw-order barrier, including in explicit layers. Subsequent
ordinary bounds-tree ordering remains relative within the new epoch; metadata
survives scene replay, cached paint and translated/clipped snapshots. Frame
clear resets both metadata and barriers. The Windows renderer recognizes
markers and filters the current framebuffer; other renderers retain normal tint.

Core integration tests in `tests/backdrop_blur.rs` cover the fluent interface,
global barriers, layers/replay/clear, clipping, rem/DPI/opacity, invalid inputs
and no unsolicited frame callbacks. Existing paint snapshot tests now also
verify filter capture, cached reuse, translation and clipping. Real GPU pixel
tests live in `vendor/gpui-pre-windows`; see its `YUE-PATCH.md` and
`docs/26-backdrop-blur.md` for performance bounds and platform scope.

Remove this extension only when upstream offers equivalent ordering, replay and
same-window filtering and the core/backend contract suites still pass.

## Explicit nearest image sampling (2026-11-03)

`Window::paint_image_nearest` uses the existing decoded image and sprite atlas,
without an enlarged bitmap, CPU readback or another texture upload. It currently
requires the maintained Windows backend; other platforms return a clear error.
The existing `paint_image` signature and linear sampling path are unchanged.

Nearest sprites retain full-image bounds and the entire atlas tile. Content
masks clip the requested viewport without first rounding the visible region to
source texels, preserving fractional pan alignment. Bit 0 of the existing
`PolychromeSprite.pad` field selects sampling; the GPU instance ABI is unchanged.
Visible outer corners retain radii while newly clipped inner corners are square.

Paint snapshots retain this sampling choice, full geometry and image ownership,
including cached paint and atlas eviction/reload. The snapshot contract script
adds a nearest case at 1 / 1.5 / 2 display scales. Windows production-shader WARP
tests cover exact texels, atlas-edge isolation, fractional pan, clipping, rounded
corners, alpha, opacity, grayscale and the unchanged linear default. See the
Windows patch record for commands. ImagePreview uses the path for its explicit
pixel-display preference or an active pixel grid at 400% or above, only for
full-resolution resources; the core has no such UI policy.

## Precise linear images and upload lifetime (2026-11-03)

`Window::paint_image_precise` selects bit 1 of the same instance field. Like
nearest, it preserves full-image bounds and UVs under clipping, avoiding the
ordinary path's rounded source sub-tile geometry. The maintained Windows shader
provides tile-local alpha-weighted bilinear sampling. This is a separate opt-in
API; ordinary `paint_image` geometry and sampler behavior remain unchanged.
Paint snapshots retain and replay the sampling flags and image ownership.

Direct precise/nearest uploads register a `Weak<RenderImage>` per image/frame.
After old frame data is cleared in draw, uploads whose last owner has gone are
removed from the atlas. Retained resources and frozen snapshots keep uploads
alive; the registry adds no strong owner. Explicit `drop_image` also removes the
weak entry. This bounds obsolete direct uploads across result revisions without
one component evicting another consumer's live resource. Retirement needs a
subsequent draw; atlas allocation overhead and host-held resources still count
toward memory. This is not a global memory or VRAM budget.

## Explicit FP16 images and window color output (2026-11-03)

`RenderImageFormat` distinguishes BGRA8/sRGB, relative-linear SDR RGBA16F and
absolute scRGB RGBA16F (1=80 nit). Buffer constructors validate actual storage,
finite half floats and straight alpha; atlas keys, sprite flags, snapshot replay
and weak upload ownership preserve the format. `RenderTargetReadback` carries
native format/stride and associated alpha; it is not an `RgbaImage` byte cast.

`WindowColorPolicy` defaults to SDR. Opt-in `AutomaticHdr` asks the platform for
verified output; `Window::color_output` reports actual mode separately from
display capability/current state, with luminance, generation and fallback cause.
Windows implements the event/renderer lifecycle. Other backends retain SDR.
The legacy test `render_to_image` is explicitly SDR straight-alpha on Windows.

Run `tools/test-gpui-snapshot.ps1 -Offline` for 5 backdrop and 6 snapshot
contracts, including FP16 upload/eviction/replay/owner retirement. Windows
shader/native checks are in its patch record. See
`docs/performance/2026-11-03-hdr-resources.md` for evidence and remaining gates;
core tests do not certify physical HDR output or SnapClip capture.

`tools/test-gpui-snapshot.ps1 -Offline` covers precise-linear cached capture,
translation/clipping, eviction/reload, frozen ownership and 20 consecutive image
replacements, as well as the existing backdrop and snapshot contracts. Component
tests check full UV geometry and static integer-zoom device-pixel alignment at
1 / 1.25 / 1.5 / 2 display scales. Production-shader pixel checks are recorded in
the Windows patch log; no real-window or performance claim follows from these tests.
## Native virtualization seams (2026-11-03)

`uniform_list_range` is the shared half-open f64 arithmetic used by native
UniformList and the Base Fixed virtualizer. It has no per-item metadata or
renderer dependency. Native uniform rendering keeps its existing input owner.

ListState now exposes immutable persistent ListGeometry and ListViewportLayout.
The latter runs the same native measurement algorithm without registering input
or mutating a foreign ScrollHandle; hosts explicitly prepaint with their actual
applied offset. Ordinary List and Owned virtual batches share one wheel handler.
Focus handles can be registered without invalidating heights. Retained offscreen
focus is positioned by its real index rather than after the visible rows.

Hints survive initial layout, width invalidation and splice. Width changes use
absolute anchors and preserve old heights as estimates. Measured status is
separate from a cached fallback: nonpositive/nonfinite heights keep a useful
estimate and remain inexact. Native layout counts, invalid-size counts and budget
exhaustion are observable without a second size table. Scrollbar bounds include
padding and preserve their existing drag-height freeze.

`new_with_uniform_height` initializes the same SumTree with estimates directly,
avoiding an all-unknown tree plus replacement tree. `set_default_item_height`
and `set_item_height_hint` support future splices and changed boundary spacing
without rebuilding an unaffected suffix. Full replacement/reorder remains a
deliberate cold transaction; `remap_items` now transfers measurements and focus
under stable-key/content-version mapping rather than invalidating every item.

Run the explicit `virtualization` integration target with `test-support` (five
contracts), and the Yue headless virtual-list suite. The published archive's
unrelated unit-test asset/macro issues do not affect this production-library
integration target. The source and maintenance/removal conditions are recorded
in `docs/research/2026-11-03-virtual-list-implementation.md`. Physical Windows
presentation and UIA behavior are outside these headless assertions.

### Axis projection, remap and retained worksets

`layout_for_viewport_on_axis` reuses native measurement, budgets and logical
anchors for horizontal intrinsic widths. Its padding argument uses physical
Edges; projection is performed at the measurement boundary. Horizontal children
keep actual desktop layout, hitboxes, focus and paint coordinates. The returned
ListViewportLayout prepaints along its captured axis; only Owned consumers
install its projected native wheel handler. Base maps the native scrollbar
geometry back to physical bounds without copying the measurement tree.

`remap_items` transfers real sizes/focus from old indexes with explicit boundary
gap adjustments. `set_retained_items` independently measures additional working
roots under the existing budget, without laying out intervening rows. Exposed
measurement import/export remains on the single native SumTree; Base owns key,
content and layout validation. These methods register no global input owner.

Native five-contract integration tests, Base/Yue upgrades and the independent
Grid/Masonry consumers are recorded in
`docs/research/2026-11-03-virtual-list-upgrades.md`. Removal requires equivalent
upstream measurement/identity/axis contracts, not only matching method names.

### Incremental per-item hint splice (2026-11-04)

splice_with_size_hints validates every supplied height before mutation, then
uses the same persistent splice implementation as splice_focusable. Existing
measurements, focus handles and frozen geometry survive; inserted rows remain
unmeasured with explicit hints. Base owns domain invariants and N-1 gap handling.
The native integration suite now has six contracts, including malformed-batch
atomicity and ordinary subsequent splices. Detailed evidence is in
docs/research/2026-11-04-virtual-list-refinements.md.

### Precise native scroll position (2026-11-04)

Div/ScrollHandle now share one ScrollOffset containing an f64 logical position,
its Pixels projection and a legacy-write marker, in the original shared Rc.
Wheel accumulation/clamping uses f64 and notifies on exact changes even when
the f32 projection stays equal. Prepaint preserves subpixel input; bounds do
not accumulate overscroll residue. max-offset decimal rounding also uses f64
intermediates so multiplying a large Pixels value by 100 adds no new loss.

ScrollHandle::precise_offset and set_precise_offset(Point<f64>) expose this
same owner (finite setter input). Existing offset/set_offset keep Pixels;
explicit jumps and legacy mutable native writes reset prior fractions, even
when writing the same projection. UniformList adapts to the shared internal
type without changing its geometry/paint contract. No per-item state is added.

The native virtualization integration target has eight tests, including exact
read/write, a same-projection legacy jump and normal later input. Yue adds
Fixed/Known tests at 2^24 for both axes, reverse input, bounds/click hit testing,
origin clamp and Grid merged axes. Base Fixed/Known/Grid/Masonry consume exact
positions before deriving local paint coordinates. Ordinary Div/UniformList
paint and Measured ListState cumulative geometry remain Pixels/f32; this is
not a claim of arbitrary-scale precision for those paths or scrollbar mapping.

Evidence and Release A/B: docs/research/2026-11-04-virtual-list-owner-fixes.md.
Remove this patch only after upstream preserves the same exact-input and
legacy-jump semantics and consumers pass the local-coordinate/hit regressions
without another scroll owner or a Yue-side per-wheel correction.

### Native measured cumulative precision (2026-11-04)

The same ListState SumTree now accumulates and seeks main-axis extents in f64.
Item sizes and local offsets remain Pixels. Derive measured_count from total
minus unmeasured instead of enlarging every summary. Native wheel, scroll_by,
alignment, bounds and scrollbar clamp retain precise sums; legacy geometry,
viewport and scrollbar getters project explicitly. A scrollbar's projected
endpoint maps to the exact end even when it rounded down. New precise geometry,
viewport layout/prepaint and offset APIs share the existing owner and tree.

Base consumes precise geometry and subtracts global offsets before converting
local coordinates. This supersedes the earlier Measured f32 cumulative limit,
not single-item/ordinary-layout/Attached-origin/scrollbar-input Pixels limits.
See optimization-series A/B, rejected wide-summary variant, old failing test,
Yue large-coordinate two-axis hit/input regression and native endpoint test.
Remove only after upstream satisfies these same contracts without a mirror tree.

### Sparse measurement export (2026-11-04)

ListGeometry::measured_items prunes unmeasured subtrees using existing native
summary counts; dense measurements retain a linear traversal. The output
reserves the exact measured count. No parallel size cache or new public API
is introduced. Ordered entries, invalid hints and frozen geometry are covered
by the adoption tests. Sparse and dense Release ABBA evidence is recorded in
docs/research/2026-11-04-virtual-list-adoption.md; remove this patch only when
upstream matches the same semantics and sparse/dense cost.

## Sparse native width invalidation (2026-11-04)

Keep unchanged hinted SumTree subtrees across a width change. A summary flag
tracks both valid and invalid Measured variants without growing the summary;
only candidate paths are rewritten to hints, including unknown defaults when
needed. Dense input retains linear rebuilding. Preserve reset, revision, focus
handles, anchors and frozen snapshots. The adoption report records sparse and
dense ABBA costs and native integration coverage. Remove when upstream has an
equivalent persistent width invalidation path.

## Bounded native size feedback batches (2026-11-04)

Validate all indexes/heights before mutation, borrow ascending input directly,
and stably sort only unordered input. Keep the final duplicate value, focus
handles, measured/hint flags and one geometry revision per nonempty call.
Apply eight unique writes per forward-cursor tree splice so old/new sparse
paths do not coexist for an entire large batch. Singletons add no sorting
buffer; frozen snapshots keep their original tree. A whole-batch variant was
measured and reverted because its transient peak nearly doubled. Continuation
evidence records bounded CPU/allocation gains and the remaining small sparse
batch peak cost. Remove when upstream matches both the semantics and measured
CPU/peak tradeoff without a parallel measurement cache.


### Asymmetric native list overdraw

Keep leading/trailing reserves on the existing ListState owner via
`set_overdraw_edges(before, after)`. The legacy `set_overdraw` resets both edges
symmetrically. Leading layout and budget continuation use the leading reserve;
trailing work remains on the trailing reserve. No second measurement tree,
offset or workset is introduced. Maintained native contracts cover bounded
intrinsic roots and restoration of the legacy symmetric behavior.

### Sparse native table semantics

Forward explicit positive row/column spans into AccessKit alongside native
zero-based indices and logical counts. Ordinary Divs retain only an optional
pointer; only explicit spans allocate the small payload. The native contract
checks actual Node data without creating offscreen cells. AccessKit metadata
does not imply that every platform bridge implements Grid/GridItem providers.

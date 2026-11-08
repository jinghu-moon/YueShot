# Yue Base patches

## Owned measured geometry and reading anchors (2026-11-06)

`virtualization/state.rs` records the native offset after geometry changes.
An Owned Measured source remap or remeasure can change prefix heights under
the existing native logical index. Compare the next native offset to that
geometry baseline when inferring user input, so this internal change does not
cancel the pending domain-key reading anchor. Committing a viewport clears
the baseline; wheel input still cancels/restores through the existing owner.
No GPUI core API or scroll owner is added.

Regression coverage: `tests/virtual_list/owned_reflow.rs` replaces a measured
source and its estimates under a partially read row, then verifies both its
key/offset and subsequent real wheel input. `tests/list_grid.rs` covers the
same failure through a responsive card consumer, including reparenting.
Remove this patch when upstream distinguishes native geometry remapping
from input with the same source/layout/real-input contracts.

## Joint VirtualGrid size transactions (2026-11-05)

`VirtualGrid::update_sizes` and its snapshot-guarded variant preflight both
Attached Fixed/Known axes before applying either batch. Missing keys, duplicate
keys, invalid extents, wrong axis ownership, binding conflicts and stale
snapshots therefore leave both axes unchanged. The existing one-axis
`VirtualizerState::update_sizes` contract remains unchanged; the Grid helper
only combines two already validated updates and emits the normal owner
notifications after each state commit. Regression coverage is in
`crates/yue-component/tests/virtual_list/api_contract.rs`.

Remove this patch when upstream exposes an equivalent paired-axis transaction
with the same stale-snapshot and failure atomicity guarantees.

## Fractional virtualization feedback and singleton cost (2026-11-04)

`virtualization/updates.rs` compares feedback in the backend's stored units:
Known uses its f32 content extent; Measured uses the f32 native slot including
gap. Exact/hint and normalized cross constraints must also match for a no-op.
An epsilon per item could silently lose several pixels over thousands of items.
Masonry external and native layout feedback use the same value equality rule.
Scroll-command convergence tolerances are separate and unchanged.

Single-item linear feedback validates before mutation and bypasses temporary
duplicate/index buffers. Native submission uses one stack tuple; Known keeps
its existing persistent-tree update. Multi-item atomicity, source identity,
frozen geometry, width invalidation and anchors remain in their existing owners.
Regression coverage, two Release ABBA rounds, measured batch tradeoffs and
rollback points are recorded in
`docs/research/2026-11-04-virtual-list-feedback-review.md`.

## Virtualization composition and presentation fixes (2026-11-04)

`virtualization/state.rs` preserves accepted content invalidations across source
transactions with a monotonic base epoch. Clearing a nonempty index-epoch map
creates one conservative working-set preparation barrier; later appends without
new invalidations keep prepared views. Settled geometric destinations bypass
unnecessary Smooth timing, while unmeasured targets and refusing hosts retain
their existing convergence guards.

`masonry.rs` expires writable snapshots on every successful nonempty content
invalidation, including unmeasured and already invalidated items. Key batches
validate before mutation and measurement demotion is grouped per lane.
Releasing an actual mount also advances the writable-snapshot epoch, so a
surviving controller cannot accept feedback from its removed presentation.
`masonry_element.rs` separates required presentation from extra correction work:
all initial roots are laid out; only additionally exposed intrinsic roots use
measurement_budget. Remaining final roots use clipped estimate slots. A new
frame is requested only with real geometry progress and deferred roots. Total
root layouts are bounded by initial W + budget + final W, not budget alone.
Fixed/Known roots are never truncated by a dynamic measurement budget.

Measurement export checks each native record's actual normalized cross-axis
constraint. Mixed or unresolved constraints fail rather than relabeling old
measurements. Validated restore demotes incompatible existing sizes to hints;
equal main-axis updates at a different cross constraint are not discarded.
`grid.rs` prepares each axis once with sequential frozen geometry. A single
O(C) column vector and row cursor replace per-row index copies and per-cell
column source/geometry lookups. No extra state owner or AnyElement cache is added.

Six pre-fix failure gates, boundary regressions, Grid/Masonry release cycles,
Release A/B samples and maintenance commands are recorded in
`docs/research/2026-11-04-virtual-list-followup-fixes.md`. Upstream replacement
must pass these composition/coverage/identity contracts before patch removal.

## Read-only Select All snapshot (2026-11-03)

`text/state.rs` exposes the existing `TextViewState::is_all_selected()` getter
publicly. A content owner can copy retained original text (CRLF, Tab, trailing
whitespace) after Select All instead of copying a rendered HTML/Markdown text
projection. Selection mutation, parsing and the default Copy action are unchanged.
The existing select-all/clear regression checks the getter before, during and
after selection; SnapClip's production pin UI test covers exact original copying.

Run `pwsh -File tools/test-gpui-base.ps1 -Offline -Filter text::state::tests::select_all`
for the focused Base regressions.

## Controlled accordion and safe measured disclosure (2026-11-02)

`accordion/controller.rs` adds explicit descriptors/snapshots, Multiple/Accordion
projection, atomic read-only change requests, keyed trigger/panel focus handles,
per-heading Tab stops, Up/Down/Home/End navigation and focus repair before old
panels are detached. The owner alone commits expanded keys. Change handlers run
outside controller updates so a host may synchronously accept a request.

`AccordionTrigger::in_group` opts into that controller. Standalone triggers keep
their bool/ClickEvent callback and now compose Base Button for native focus,
disabled semantics and single Enter/Space clicks. Physical keydown records the
source; activation remains Button's native key-release click, avoiding a second
toggle. A keyboard-shaped native activation without a physical keydown reports
Accessibility. Mouse and touch report Pointer. Grouped triggers use only the
controller's handler, with ambiguous standalone/group callbacks rejected.

`motion/disclosure.rs` adds opt-in `MotionDisclosure`. It measures the normal-flow,
nonshrinking child at the actual width, uses an interruptible pixel timeline and
shares Base Transition easing/reversal policy. Initially open adopts natural
height; a new expansion starts at zero with no fully shown measurement frame.
Closing requires `None` content immediately and animates only retained geometry.
Stable expanded content uses auto height; dynamic targets retarget current pixels,
while width/geometry/reduced-motion/visibility changes settle. Only visible active
motion requests frames. Stable content is not unnecessarily clipped. Existing
MotionReveal, Collapsible and standalone Accordion mounting behavior are retained.

Validation: production headless Collapse tests cover input, focus, clipping,
geometry, retained input and popup-anchor disposal. The Base accordion fixture
also verifies native structural roles and reentrant accessibility requests.

## Toast foundation extensions (2026-11-02)

Yue uses the existing Base 0.8.0 ToastManager and ToastStack rather than a second
lifecycle/layout implementation. `src/toast.rs` adds independent entry/exit
deadlines, in-place update and timeout reset, remaining/total reading snapshots,
transition sampling and the nearest effective deadline. Late wakes consume time
across phases; only Present reading time pauses. Existing same-id push and the
default natural-height collapse policy remain available.

ToastStack adds opt-in front-card shells, center-front anchoring, external natural
measurement and controlled expansion/sample time. Natural measurements stay
separate from clipped presentation heights. Centering offsets items and hover
bounds, preserving caller `bottom_0()` positioning. Yue supplies only active
items to rank/layout and paints frozen Ending snapshots independently. The Base
defaults retain their existing behavior; `optional_front_shell` and the original
tall-behind/bottom-anchor regressions cover both policies.

`src/motion.rs` exports `spring_at` and `transition_at` for an optional local
diagnostic clock and frame policy. Existing APIs delegate with executor time and
normal scheduling. `src/modal.rs` publishes window-scoped `ModalScopeChanges`
only when the active scope changes, including a monotonic timestamp for precise
pause boundaries; released windows remove their entries.

`src/a11y_live.rs` provides the forwarding `A11yLive` adapter for live, hidden,
atomic and actual semantic-write notification. It adds no layout, focus or input
node. Toast roots can choose their native role/live/hidden policy. Yue uses
non-live visual cards and separately keyed native Status/Alert events with both
label and value; semantic delivery and modal restoration are tested independently
from visual opacity. These hooks do not prove real OS screen-reader output.

Validation: Yue Toast headless regressions and Showcase tests, plus Base's
`toast::tests`, `a11y_live::tests`, `modal::tests` and `motion::` tests. Run
`pwsh -File tools/test-gpui-base.ps1 -Offline -Filter toast::tests` (change Filter
for another module). The script creates a disposable manifest/lock under target,
using the actual vendored source and the workspace's production pins, and removes
the published manifest's unconditional Fast backend development dependencies.
It never edits Cargo registry sources, the vendor manifest or the root lockfile.
Yue owns all colors, spacing, timings, eight-position policy and reading defaults.

## Dialog foundation extensions (2026-11-01)

`src/dialog.rs` adds opt-in controlled `DialogIntent` routing, independent default
submission, configurable deferred priority and popup-owned semantic role. The
controlled path uses bubbling fallback keys so a menu, text editor or button
consumes its own input first. Backdrop dismissal requires a complete primary
press/release; an exiting topmost surface can retain its input barrier while
disabling descendant activation. Existing legacy Dialog callbacks remain intact.

`src/modal.rs` adds window-local active/building `ModalScope`, a forwarding
`ModalBoundary` for native modal/busy/background-hidden semantics, and weak,
live-editability-checked editor candidates. Input state registers its own focus
while rendered in a declaring modal scope; widget traversal and pointer position
do not determine ownership. `Root` exposes trap-aware traversal and checks for a
retained handle's live Root dispatch path. Active modal focus takes precedence
over incidental background focus. Window release drops these weak registrations.
These are unstyled mechanisms, not a global dialog service.

`src/constrained_drag.rs` provides an explicit non-focusable handle, measured
viewport clamping, a pointer threshold, capture move/release and cancellation.
Consumers choose their own draggable region, spacing and animation. Yue uses it
only for an enabled title region; interactive custom headers require a separate
handle. Regression tests cover Dialog input, drag bounds, layer dismissal, focus,
lifecycle, asynchronous tickets and Popup integration. Native node-hook tests do
not stand in for real Windows UIA/screen-reader verification.

Source: crates.io `gpui-base 0.8.0`, upstream `longbridge/gpui-kit`, commit
`1676826d9d49b90f4d7b6808b69924f58796f6f1`, directory `crates/base`.
The distributed Apache-2.0 license, manifest, sources, tests and benches are
retained. Cargo's registry copy is never modified.

The Popup extension changes `src/positioner.rs`:

- Read an optional anchor source during prepaint, after the trigger is measured.
- Expose the existing resolver and decorated viewport bounds for bounded probes;
  support a fixed side without flipping, and include the offset in fit tests.
- Allow a position filter to reject an unsafe frame or provide sampled geometry
  before both hitboxes and child prepaint. Rejected surfaces are not painted.
- Preserve fractional element offsets; Yue snaps the final sample with GPUI's
  physical-pixel helpers before publishing geometry and inserting hitboxes.

## Slider and numeric input (2026-11-01)

`src/slider.rs` and `src/slider/domain.rs` now provide the shared behavior for
YueSlider, ColorPicker and the Layout example:

- `SliderDomain` validates finite ordered bounds and a positive, representable
  step. The step grid starts at min and includes max even for a short final
  interval. Decimal-grid arithmetic uses shortest round-trip f32 decimals.
- Programmatic Base assignments clamp/order and preserve sub-step precision
  (required by ColorPicker); user edits snap to the grid. Percentages come from
  the final value and same-value edits do not emit Change.
- `Slider::snap_points(points, distance)` configures local pointer magnets in
  logical pixels. Each radius is capped at a quarter of the nearest mark gap;
  outside it the normal step grid applies immediately. Exact marks retain their
  off-grid value. Keyboard, accessible actions and number steppers keep the
  domain step, which remains exposed on native Slider/SpinButton nodes. Policy
  changes cancel capture without moving the current value. Same-value Yue owner
  echoes remain no-ops even after removing a mark. No mark policy is installed
  in ColorPicker, whose exact programmatic values remain unchanged.
- Mouse capture uses window MouseMove/MouseUp handlers, without a DnD threshold.
  Thumb-edge grabs preserve the pointer offset. Range-track presses choose the
  nearest endpoint, with the last active endpoint resolving ties; initial ties
  choose upper. Endpoints do not cross and capture continues outside the track.
- Start/Change/Release/Cancel distinguish real edit lifecycles. Cancelled touch,
  disable and window deactivation do not commit. Public `cancel_interaction`
  silently clears the old gesture without altering another control's drag.
- Arrow, Home, End, PageUp/PageDown and held-key completion share the domain.
  `thumb_semantics(true)` places stable focus handles and native Slider roles on
  each endpoint, including orientation, value, dynamic bounds, step, description
  and accessible increment/decrement. Caller focus handles receive tab policy.
  The default root semantic mode remains usable by existing Base consumers.
- Start does not notify observers before applying the same operation's value;
  this prevents ColorPicker mistaking its own user edit for an external setter.

Yue owns presentation, numeric drafts and generation-checked Change/Commit
transactions. These are not extra Base numeric values. Regression coverage:
`crates/yue-component/tests/slider.rs`, `tests/color_picker.rs`, and Base's
retained slider unit tests. Read-only `is_dragging` remains available for visual
feedback; thumb-only presses emit no Change.

When presentation animates a thumb independently of the canonical value,
pointer capture now uses its actual last-laid-out center and preserves the
grab offset. A press on a displaced thumb takes over that visible coordinate
through the existing domain/step path instead of jumping to the old destination.
Touch capture and nearest-thumb selection likewise use the presented geometry.
Ordinary stationary thumb presses remain silent. Yue alone decides whether and
how to animate; the Base mechanism adds no timing or visual policy. Yue's
headless Slider regressions cover mouse and vertical touch takeover.

`src/number_input.rs` adds a description slot and writes finite numeric
value/min/max/step to its native SpinButton.
Accessible increment/decrement uses the same step callback as pointer controls.
`NumberInput::controls(false)` omits the pointer buttons while retaining the
editor, SpinButton semantics, Up/Down and accessible increment/decrement actions.
`tab_stop(false)` applies to the editor's focus handle; disabled still wins.
Slider uses these for the arrow-free field beside end buttons. Real pointer,
keyboard, traversal and invalid-draft regressions live in Yue's Slider tests.
`src/input/base/mod.rs` forwards disabled to the actual native input node through
the existing adapter. Yue's NumberInput now supplies the actual Base Input
editor through `.input(...)`, and adds right-side controls and `on_step`;
Input and Slider integration tests cover real edits, disabled input and numeric
semantics.

`src/test_support.rs` additionally observes existing native description, numeric
value/bounds/step and orientation attributes. It does not inject replacement
semantics or prove Windows UIA/Narrator output.

With `test-support`, Input's last layout also exposes `rendered_text_color()`:
the resolved default color passed to the real TextRun shaping input. This is a
read-only observation, not a second color policy, GPU pixel readback, or a
syntax-highlight override. Yue's numeric Error/Normal regression uses it in all
four theme profiles to catch inherited text color regressions.

`src/motion/presence.rs` retains the previous leg's timing policy when sampling
an interrupted transition. Enter and exit may have different easing and duration;
the old leg must be sampled with its own policy before adopting the new one,
otherwise an exit-to-enter reversal jumps in opacity and position. This is a
mechanism fix, with no default visual policy. Yue Tooltip's twelve-placement
reentry regression exercises asymmetric enter/exit policies through the real
Popup host, checking the exact sampled geometry on reversal.

Yue uses these hooks without copying the resolver. Replace this patch with an
upstream release once equivalent public APIs exist; run the popup and dropdown
regressions before removing it. No styled Component or second GPUI backend is
enabled by this patch.

Cargo applies patches from the consuming workspace root only. A downstream
workspace using Yue as a path dependency must add its own `[patch.crates-io]`
entry pointing to this directory until the required APIs are released upstream.

`src/link.rs` accepts an explicit caller-owned `track_focus` handle, with the
same tab_index/tab_stop policy as its keyed default. Its actual semantic div
also uses Base's existing optional test observation, as Button already does.
YueLink therefore paints its focus ring around the one native Link node rather
than creating a competing focus target. Link integration tests cover activation,
disabled state, caller focus, traversal and opening policy through this node.

`A11yDisabledExt` and its `A11yDisabled` adapter are now public re-exports. The
implementation is unchanged: it forwards layout, input and observation and only
sets the native accessibility disabled flag. Yue's composed menu rows use the
same adapter as Base controls rather than duplicating it; Breadcrumb tests cover
disabled menu semantics as well as skipped keyboard activation.

## Yue Select integration (2026-11-02)

`Select::on_open_request` and `Combobox::on_open_request` return the resulting
open state (`Fn(bool, &mut Window, &mut App) -> bool`). Return false when rejecting
an opening; the primitive keeps focus on the trigger instead of hidden content.
The upstream `on_open_change` notification retains its `()` callback and accepts
the requested state. This distinction lets a consuming application use published
GPUI Kit styled controls and Yue with one patched Base graph. It is exercised by
Base notification/open-request tests and the SnapClip consuming application. A
confirm on an already open Select does not refocus content after its owner
commits, closes and restores trigger focus. A first closed Up/Down request is
consumed after opening, preventing an outer navigator from moving twice.

Select also exposes readonly, required, invalid, accessibility_description,
tab_stop and tab_index. Readonly retains focus and semantics while rejecting
activation. Tab policy refines the supplied FocusHandle, not a second focus
target. The existing no-layout/no-extra-node disabled adapter adds
`SelectionA11y` / `A11ySelectionExt`: disabled, read_only, required, invalid and
multiselectable reach the actual AccessKit node. Native-node tests verify those
flags; they do not establish Windows UIA/Narrator behavior.

Input's `EntityInputHandler::unmark_text` now notifies observers after ending
composition. Some platform commits only unmark the existing preedit without a
second text replacement; consumers must be able to observe that boundary.
This does not emit a second text Change event or add Select behavior to Input.
Yue's headless test drives preedit, guarded Enter, unmark and subsequent query
filtering through the actual InputState. Ordinary Input remains Tab-to-indent;
only Select's local query context supplies its composite traversal.

Regressions: `select::tests` (including owner-rejected open, readonly and caller
tab policy), `combobox::tests`, `a11y_disabled::tests`, and the Input composition
tests. Run them with `tools/test-gpui-base.ps1 -Offline -Filter <filter>`; the
fixture uses the consuming workspace's production backend and lockfile.

## Yue Tabs integration (2026-11-02)

`Tab::in_list` opts into a retained `TabsController` and stable item key while
preserving standalone Tab behavior. `TabsSnapshot` supplies the owner's items,
selection, navigation axis, disabled state and automatic/manual activation.
User activation emits `TabsChangeRequest`; close emits `TabsCloseRequest` with
a suggested enabled neighbor. Callbacks run after releasing the entity borrow
and never silently commit selection or remove items.

The controller owns roving focus, axis-aware arrows, Home/End, Enter/Space,
Delete, key-preserving reorder/removal reconciliation and ScrollHandle geometry
in content coordinates. `Tabs::axis` adds native orientation. `TabPanel` attaches
only active descendants to layout, input and accessibility; its tab stop is
explicit. Yue composes these APIs with its token-driven cells and motion.

Native Tab, TabList and TabPanel roles, selection, disabled state and orientation
are available. GPUI does not publicly expose the stable AccessKit node IDs or
controls/labelled-by builders needed for native Tab/Panel relations; no private
hash reproduction or Cargo registry edits are used. Windows UIA verification
remains separate from native-node and headless regressions.

Run `tools/test-gpui-base.ps1 -Offline -Filter 'tabs::'` and Yue's headless `tabs`
integration target. Consuming workspaces must retain the root Cargo patch.

## Yue loading feedback integration (2026-11-02)

`ProgressValue` normalizes controlled percentages and represents unknown values
without leaking NaN or infinity into native numeric properties. Existing `Progress`
builders remain compatible and now support descriptive `value_text`.

`LoadingDelay` owns one cancellable weak-entity deadline with a generation guard;
it gates presentation without running or extending host work. `loading_phase`
uses one application epoch, requesting frames only for positive-area visible,
unclipped content in an active window. Reduced motion and static variants do not
request loop frames. Returning windows rejoin the common phase.

`LoadingRegion` attaches only its active child branch, projecting Group/busy plus
one Polite Status. `A11yLive::busy` sets or clears the existing native flag without
adding focus targets. Timers, native properties and frame requests are covered by
`loading::tests`, `progress::tests` and `a11y_live::tests`; actual Windows UIA and
reader behavior remain separate from these headless contracts.

## Yue Collapse motion follow-up (2026-11-02)

`MotionDisclosure` passes its request-layout timestamp to prepaint. Natural-height
retargeting and the height laid out for that frame therefore share one clock;
measuring content cannot postpone the start or advance the retarget origin past
the pixels already laid out. Close immediately detaches descendants and clips
the frozen paint frame from the last live render. The maintained `gpui-pre`
patch provides paint-only capture/replay; the current Fast backend retains the
previous geometry-only fallback. Downstream consumers of the production backend
must patch both crates; see `../gpui-pre/YUE-PATCH.md`.

Yue's Collapse regressions drive opening and closing through requested frame
callbacks without forcing window refresh, checking monotonic intermediate heights
and final landing. Painted-quad checks cover hover without a header background in
light/dark themes and both trigger modes. Additional checks cover retained paint
with detached controls, progressive clipping and translated snapshots when
several panels close together. UI motion perception remains user-tested.

## Virtualizer foundation (2026-11-03)

Current follow-up: the 2026-11-04 implementation below supersedes the
per-item Known tree and geometry-dependent preparation described in the
historical 1–8 section. See `docs/research/2026-11-04-virtual-list-refinements.md`.

### Virtualizer refinements (2026-11-04)

Content invalidation now applies to all three sizing backends using sparse
range epochs; stale content snapshots reject delayed writes. Alignment policy
changes invalidate outstanding request identity without remeasuring items.
Prepared item factories receive VirtualizerItemIdentity (index/key), not frame
geometry. Explicit content versions and normal Entity observation own updates.

Known extents use 128-item compact persistent blocks, f32 actual sizes and f64
local prefix indexes under the existing SumTree. Frozen snapshots share blocks;
batched updates copy/recalculate each touched block once. Sequential snapshot
iteration seeks once per contiguous run and skips remote retained gaps.

Splice promises unchanged key/content/hints outside its range and evaluates
only inserted estimates through core splice_with_size_hints. Masonry generic
replacement packs once, while append_source preserves lanes and only estimates
new rows; live snapshots still impose COW costs on its flat index vectors.
See the follow-up record for before/after failures, 6400-step oracle traces,
64-cycle release gates, raw timing/allocation CSVs and remaining boundaries.

These changes remain local until upstream provides equivalent identity,
snapshot, batch/append and preparation contracts; compare behavior and tests
before removing them, rather than replacing methods by matching names.

`virtualization/{mod,geometry,state,element}.rs` replaces the former
`virtual_list.rs` and its v/h constructors. All repository consumers, including
Select, use the new source/sizing/state seam. There is no compatibility engine.
Fixed shares core uniform arithmetic, Known retains one persistent extent tree,
and Measured shares core ListState/SumTree. Base adds no theme or default role.

Owned mounts have one native scroll owner and are guarded by WindowId plus
GlobalElementId. Attached states allocate no ScrollHandle or wheel listener;
their versioned absolute requests are merged and confirmed once by a host.
Bindings have local exclusive axis slots. Snapshots freeze source/geometry,
while mutation rejects stale source, geometry, viewport/input and binding
versions. Domain-key anchors, sparse focus retention, deletion repair, bounded
commands, follow-tail, diagnostics and cancellable user-idle events share this
state. Smooth motion is explicit, known-only and within one viewport; hidden,
inactive and reduced-motion Owned views settle without a frame loop.

Measured slots include gaps in the one native tree. Initial estimates and
splice boundaries account for N-1 gaps; returned item extents exclude spacing.
The native measurement budget defaults to 1,024 per pass and is explicit in
VirtualizerLayout. Diagnostics identify invalid extents and exhausted budgets.

Run `tools/test-gpui-base.ps1 -Offline -Filter virtualization`, Yue's headless
`virtual_list` and `select` targets, and the focused core `virtualization`
integration target. Release CPU/allocator probes are manual ignored tests in
`crates/yue-component/tests/virtual_list/performance.rs`. See
`docs/research/2026-11-03-virtual-list-implementation.md` for raw data, regression
baselines, measured limits and upstream adoption/removal conditions.

## Virtualizer 1–8 capability follow-up (2026-11-03)

`is_scrolling` now means actual activity, with direction and coalesced 120ms
idle; `has_scroll_command` separately describes programmatic command lifetime.
Owned adapters and Attached hosts retain one scroll/input owner. Both axes now
support Fixed/Known/Measured and follow-tail. Horizontal measured layout uses
the native ListState algorithm with physical child coordinates, not a second
measurement engine or rotated paint tree.

Source content revisions allow stable-key measurement transfer on reorder.
Per-item estimates, atomic keyed size/hint batches and snapshot validation keep
late measurements from applying to a replacement generation. Known geometry
uses a persistent f64 SumTree shared by frozen snapshots; point updates copy
tree paths, while gap-only changes do not rebuild N slots. Tree metadata costs
more resident payload than the previous compact prefix; the evidence records
both the allocation cost and local-update benefit.

Extra retained keys/range extraction supplement the contiguous workset.
Scroll padding, host content origin/extent and absolute/relative requests share
the same versioned protocol. `new_with_item_views` retains only workset AnyView
handles under explicit content/layout identity; normal GPUI lifecycle continues
and no AnyElement is cached across frames. Cache export/restore uses pure
measurements plus key anchors, without retaining a former entity or source.

`VirtualGrid` merges two known/fixed Attached axes into one host submission.
`VirtualMasonry` uses stable lanes and the same persistent extent geometry,
real dynamic layout, bounded measurement, focus retention/deletion repair and
explicit cold reflow. Neither adds selection, data loading or Table semantics.

See `docs/research/2026-11-03-virtual-list-upgrades.md` for API mapping, baseline,
headless regression, three-run release CSVs, memory tradeoffs and limitations.
Adopt upstream replacements only after the same ownership/geometry/input and
allocation contracts pass; remove covered patches rather than keeping engines
in parallel. Physical UI/GPU/reader validation remains user-owned.

## Reading anchors and native input ownership (2026-11-04)

VirtualizerState stores key + VirtualizerAnchorPosition (Before/Within/After)
instead of clamping every position into item content. Leading padding belongs
to the first item, a gap to the following item, and trailing padding to the last.
Distances are finite/nonnegative; restoration clamps to current content or
adjacent spacing before the host applies its final reachable-range clamp.
Measured slot gaps are excluded from content extent. Attached commits outside
the list's content region clear reading anchors and stale anchor corrections;
partially overlapping headers and empty item ranges inside large gaps remain
distinct. Explicit positioning/follow-tail retain their existing contracts.

Portable measurement cache schema is now 2, including the anchor variant.
The anchor builder/reader use VirtualizerAnchorPosition; old schema 1 is not
implicitly migrated. Invalid batches fail before mutation. Live invalidation
epochs are not persistent identity: size-affecting content advances source
revision_at, shared typography advances layout revision. Yue re-exports the
new value type and all repository consumers use the updated interface.

Masonry cancels pending positioning on native wheel (even at a boundary) or
observed external native offset changes. cancel_scroll/has_scroll_command are
the explicit consumer seam. Before actual geometry changes, it captures the
current native position against old geometry and compensates only the change;
identical measurement batches do not scan anchors again. Last-written offset
is only an input observation marker, not another scroll owner. Unmount clears
pending/input markers and retains existing stale-feedback protection.

Fixed/Known Owned, Grid and Masonry read/write native ScrollHandle precision.
Local coordinates are derived before conversion to Pixels. Measured ListState
still has f32 cumulative geometry; this patch does not claim arbitrary-scale
precision for that backend, ordinary Div paint or scrollbar mapping.

See docs/research/2026-11-04-virtual-list-owner-fixes.md for five failing old
production contracts, 13 added regressions, stable-frame Release A/B and limits.
Run the Yue virtual_list target, workspace headless regressions, Base geometry
contracts and native virtualization integration tests. Remove covered pieces
only when upstream satisfies the same anchor/cache/input/precision contracts
and the CPU comparison; never retain parallel position or measurement owners.

## Measured optimization series (2026-11-04)

Contiguous working_items iterates native geometry directly without allocating
or sorting an index vector. Sparse retained items preserve the existing sorted
union and direct seek contract. Explicit working_indices still returns a Vec.
The per-change before/after CPU, allocation and regression evidence is recorded
in docs/research/2026-11-04-virtual-list-optimization-series.md. Adopt upstream
only after the same ordered identity, sparse-work and measurement contracts pass.

Masonry reuses one final-geometry item workset for fallback presentation,
prepaint and reading-anchor commit. Geometry-changing passes still recompute;
no elements or writable snapshots are cached across frames. Production source
query counts and all original coverage/budget/input regressions are verified
in the optimization series before/after evidence.

Masonry submits frame measurements only for changed extents or first real
measurement. Public batches still validate every size and duplicate index
before filtering no-ops; no-op batches do not allocate lane groups or advance
revision. Cross/content invalidation marks items unmeasured, so equal sizes
under a new identity still become real measurements. Atomicity and actual
frame query counts are covered by the optimization-series regressions.

Scroll idle detection retains one task per active session. Activity updates
the deadline; the task waits again only for its remaining duration. Unmount
and detach cancel the task; binding/session guards reject stale work. The
optimization-series ABBA fixture verifies no premature end, exactly one
ScrollEnd after 120ms idle, restart, CPU scheduling and allocation costs.

Measured now queries the same native f64 cumulative geometry and uses precise
viewport batch/offset/prepaint APIs. Owned wheel and local paint no longer lose
small increments to an absolute Pixels projection. Single-item Pixels and
external-origin/scrollbar projection limits remain explicit. Upgrade tests and
CPU/allocation tradeoffs, including the rejected wide summary, are recorded
in the optimization series; this is a correctness change, not a speed claim.

## Masonry command baseline (2026-11-04)

Complete the checkpointed command/blank-anchor changes before optimization.
Blank-region anchors carry the measured flag consistently. Command terminal
events settle, cancel, report a missing key or stop after bounded no-progress;
regression coverage asserts exactly one result and retains the existing owner.
The full pre-experiment virtual_list suite passed 99 tests with 8 manual probes
ignored; the added terminal-results test also passes.

## Bounded Masonry View preparation (2026-11-04)

Add opt-in new_with_item_views/content_revision, sharing the existing 1D
working-set cache representation. Cache AnyView only, never frame Elements.
Invalidate by key/source/external/local content stamps; preserve content through
geometry changes and append, and release evicted/swapped/unmounted Views.
Fixed/Known explicit content invalidation now refreshes preparation without
invalidating geometry. Adoption report and ABBA CSVs show stable-range gains,
rapid-jump overhead and unchanged ordinary-renderer allocations. Remove when
upstream provides the same bounded preparation and invalidation contracts.

## Snapshot-friendly Masonry indexes (2026-11-04)

Replace flat lane indexes and source slots with private 256-item shared array
blocks. Frozen snapshots share directories/leaves; append copies directories
and tail blocks rather than all item indexes. Reads stay O(1); copying remains
O(N/256), not a logarithmic persistent tree claim. Adoption report records
append/retained-memory gains and cold construction/no-op feedback costs. Tests
cover block crossings and multiple frozen generations. Remove when upstream
offers equivalent indexing and snapshot preservation with measured tradeoffs.

## Lazy Masonry measurement invalidation (2026-11-04)

Keep a measurement epoch on KnownGeometry and its shared 128-item blocks.
Width/layout invalidation advances only the geometry epoch; old sizes remain
hints. A later measurement or partial-tail append normalizes just that block
before writing, preserving both hint flags and frozen snapshots across repeated
invalidations. No mirror size cache or public API is added. Continuation ABBA
evidence records large-list resize gains, per-block metadata/append peak costs,
and unchanged source queries and row builds. Remove when upstream preserves the
same invalidation/snapshot contracts with equivalent measured costs.

## Virtual layout API preflight (2026-11-04)

Masonry accepts common scroll options while preserving align calls, publishes
read-only capabilities, and rejects unsupported motion before cancelling an
active command. Reduced motion remains instant. Grid exposes and reuses a
typed, side-effect-free axes preflight. Keep layout-specific unsupported
boundaries explicit; remove when upstream supplies these same contracts.

## Single Masonry feedback (2026-11-04)

Handle a single measurement without a sorting buffer or per-lane Vec directory.
Keep the private helper out of the multi-item hot body, retaining validation,
no-op, anchor, one-revision and frozen-snapshot semantics. Batch storage remains
unchanged: inline-buffer and SmallVec experiments were measured and reverted.
Two Release ABBA runs support single-item CPU/allocation gains; the full batch
and frame guard matrix records the limits. No persistent cache or dependency
is added. Remove when upstream supplies equivalent single/batch tradeoffs.

## Multi-item linear feedback buffer (2026-11-04)

Reuse the existing indexed transaction Vec for sorted duplicate validation,
no-op filtering and backend submission; remove the separate HashSet. Validate
no-op duplicates before filtering and keep the complete batch atomic. The
singleton specialization, stored-size/exact/cross equality, native measurement
owner and anchor protocol stay unchanged. Two Release ABBA matrices record
allocation/peak reductions and unchanged source reads; this is not a claim of
GPU FPS improvement. Remove when upstream offers equivalent validated batches.

## Masonry absolute offset commands (2026-11-04)

Add instant finite nonnegative f64 offset targets to the existing one-command
lifecycle and native ScrollHandle. Reuse preflight motion validation, bounded
correction, native input cancellation, unmount cancellation and exactly-once
terminal events. Clamp against current viewport/geometry; offset targets have
no key alignment or persistent tail pin. Key APIs remain compatible. Publish
offset support in that checkpoint; follow-tail and bounded smooth are added
below, while Attached remains unsupported. Six
public-seam tests cover validation atomicity, pre-mount/empty/large coordinates,
dynamic correction/source shrink, input/unmount and bounded termination.
Remove when upstream provides the same command and ownership contracts.

## Masonry tail-follow session (2026-11-04)

Add opt-in `set_follow_tail` / `is_following_tail` to the existing Masonry state.
Append and measurement correction resolve the current geometry tail through the
same native ScrollHandle; there is no second writable offset or per-item cache.
Native wheel and external offset input pause a session away from the tail and
retire the stale layout anchor. Input return is confirmed after the frame's
size correction, so changing estimates cannot leave a reader at the new tail
with a stale paused flag. Explicit key/offset commands pause an existing session
without creating one; source replacement and disabling follow end the session.
At this checkpoint publish follow-tail support only; Attached and reverse/RTL
remain out of scope, while bounded smooth is added below. Public-seam tests cover fixed append, measured append/correction,
native wheel leave/return, source replacement and command priority. Remove when
upstream supplies equivalent single-owner behavior and lifecycle contracts.

## Masonry smooth positioning and Grid motion policy (2026-11-05)

Share the one-dimensional ease-out cubic sampler with Masonry Fixed/Known key
and offset commands. Sample one frame time across geometry correction passes,
re-resolve current geometry, bound the distance by the caller cap and one
viewport, and require both motion and layout convergence before reporting
Settled. Measured/reduced-motion/long-distance requests use the existing safe
instant policy; capability queries distinguish actual animation support.
New commands begin at the native precise position. Input, unmount, target loss
and the existing stall/attempt/time budgets retain exactly-once terminal events.
Advancing timed samples do not consume geometry retries or falsely stall on
sub-tolerance motion; frozen clocks and the two-second deadline remain bounded.
There is no new scroll owner, animation task or per-item geometry cache.

Grid keeps two Attached Fixed/Known axes and one native x/y submission. Honor
reduced motion, clipping and inactive/hidden windows before sampling each axis,
matching VirtualList. Linear/Grid confirmation treats accepted advancing tiny
samples as progress, while a host refusing proposals still reaches NoProgress.
Masonry has the same motion policy while preserving its
progress-driven dynamic measurement continuation. Headless tests cover middle
frames, final targets, cancellation, precision and bounded termination; opt-in
Windows tests also require actual intermediate Grid/Masonry native positions.
See `docs/research/2026-11-05-virtual-list-smooth-layout-review.md` for the
baseline, Release cost guards, exact supported layout boundaries and rollback.
Remove these changes when upstream supplies equivalent behavior and tests.

### Optional motion storage

The initial inline motion representation enlarged the linear/Masonry controller
values from 944/560 to 952/616 bytes on this Windows Release build. Keep timed
Masonry state in one command-owned Box only for explicit Fixed/Known Smooth
requests, and use saturating byte counters for the 32-retry/8-stall bounds.
Controller values return to 944/560 bytes; Instant/Measured requests do not
allocate this motion object. Explicit motion pays one additional state allocation
until completion/cancellation. These are size_of values, not resident heap/RSS.
The record retains the initial timing regressions, the isolation A/B and final
baseline guards; restored value size is not presented as a CPU speedup.


### Grid preparation, bounded directional reserves and cold Masonry recovery

Add opt-in `VirtualGrid::new_with_cell_views` using row/column content stamps,
with workset pruning and owner/unmount release; never retain AnyElement.
Add bounded directional overscan, preserving symmetric defaults and idle.
Masonry samples native input/activity/direction with one cancellable idle task,
allocated only during activity. Its schema-1 value cache exports all lane slots
and actual column constraints; restore validates atomically, skips stale content,
rebuilds explicit lane membership and restores the reading key. Export is O(N),
restore reuses matching slot directories and patches changed measurement blocks.
Changed source order or historical lanes take the O(N log(lanes)) remapping
path plus source lookup costs; construction and O(N) cache storage remain real
costs. Accurate hints on large models can still be cheaper without restore.
Grid freezes source-order prefixes on the same two axis controllers and one
ScrollHandle. Disjoint masks apply during hit testing and paint; frame-local
occlusion adjusts key alignment without rewriting consumer layout. Hidden or
zero-body targets terminate, huge prefixes only materialize visible slots.
Unfrozen Grid keeps its ordinary cell storage; mask buffers are opt-in.

See `docs/research/2026-11-05-virtual-list-next-implementation.md` for measured
adoption costs, default guards, functional/native tests and rollback identities.
Remove these patches when upstream satisfies the same tested contracts.

### Frozen Grid reading-anchor clearance

Capture Grid reading anchors at the body's unobscured leading edge. Resolve
them against the current frozen-track extent after size, gap or content-padding
changes, so stationary geometry is not counted again as scroll compensation.
Do not apply a body anchor when its key moves into the frozen prefix or the
prefix consumes the viewport. Ordinary unfrozen anchors retain their existing
coordinates; no new owner, retained field or cache is introduced.

See `docs/research/2026-11-05-virtual-list-frozen-anchor-review.md` for failing
baseline observations, dual-axis regression and native coverage, Release ABBA
costs and rollback evidence. In the hidden-track resize workload, the corrected
workset is larger and costs more CPU; this patch is adopted for correctness,
not as a speedup. Remove it when upstream satisfies the same tested contract.

### Sparse Grid layout and bounded Measured motion followups

Stream the existing four frozen regions with O(R+C) track scratch rather than
sorting an R*C cell buffer. Keep region/row-major stacking unchanged. Optional
stable row/column key pairs retain only required cells; mount-owned focus repair
precedes view pruning on deletion, unregistration or owner replacement.

Validate explicit Fixed/Known row/column spans before mounting. Sweep sparse
rectangles for overlap, include offscreen origins, render each origin once, and
reject cross-frozen-boundary spans. Track dimensions remain caller-owned.
Resolve explicit min/max/weight auto tracks without measuring cells. Grid RTL
mirrors masks, paint and hit geometry and converts one physical native scroll
owner to logical start/end offsets, preserving keys and reading position.

Measured linear lists animate bounded already measured paths with one sample
per time, remaining-deadline geometry retargeting and existing rejection/timeout
guards. A separate JumpThenSmooth opt-in jumps near distant targets and animates
only a measured tail; it is unsupported on Fixed/Known and Masonry. Completed
animation samples still re-resolve same-time measurement feedback before final
alignment. Unknown geometry falls back to bounded instant convergence.
Revalidate the measured path and the viewport/distance bound on every new time
sample, so later invalidation or resize cannot keep an obsolete animation alive.

Do not absorb the experimental per-frame intrinsic content probe: it added
about 20% CPU in both measured rounds and cannot infer unobserved content's
global maximum. See `docs/research/2026-11-05-virtual-list-five-followups.md` for
contracts, preserved unfavorable samples, native tests, costs and rollback.

### Sparse span query pruning

Reuse the row-max interval tree to visit spans intersecting sparse working
tracks, instead of scanning all descriptors per prepaint. Equal row starts
guarantee column-disjoint sorted intervals after validation, allowing column
pruning without another index or cache. Preserve span ordering, offscreen
origins, frozen masks and retained-cell semantics. Configuration remains
O(S log S); arbitrary two-dimensional worst cases can still query in O(S).
See `docs/research/2026-11-05-virtual-list-four-acceptance.md` for independent
oracles, 0/100/1,000/10,000-span Release ABBA, and retained unfavorable samples.

### Observe actual table semantics

Test snapshots expose optional native table counts, indices and spans from the
same AccessKit Node written by the element. Share the payload across snapshot
clones and allocate nothing for nodes without table metadata. This observation
surface does not synthesize semantic nodes or claim platform reader support.

### Explicit immutable span configuration reuse

VirtualGridSpanSet takes owned immutable descriptors and shares one validated
resolution keyed by both controller identities, source generations and frozen
prefixes. Cache indices only; current track geometry remains frame-local.
Validate axes even on hits, and publish a miss only after complete validation.
Neither descriptors nor resolution retain sources, controllers or windows.
Existing .spans keeps owned storage with no retained configuration cache.
Opt-in storage is O(S), and cold initialization can cost more; see the nine-point
readiness report for warm/cold Release ABBA, default guards and rollback.

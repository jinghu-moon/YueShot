use super::{
    ArrowStyle, PopupAnchorHandle, PopupDismissRoute, PopupId, PopupRuntime, PopupSnapshotHandle,
};
use super::{
    geometry,
    runtime::{
        DismissHandler, PopupDismissReason, PopupDismissRequest, PopupValidity, RegistrationSpec,
    },
};
use gpui_kit::base::{
    Align, Placement, Positioner, Presence, ResolvedPosition, Transition, transition,
};
use gpui_kit::{
    Anchor, AnyElement, App, Bounds, Edges, FocusHandle, IntoElement, ParentElement as _, Pixels,
    Size, Styled as _, Window, canvas, deferred, div, px,
};
use std::{rc::Rc, time::Duration};

#[derive(Clone, Copy)]
pub(crate) enum PopupPlacement {
    Side(Placement, Align),
    Point(Anchor),
}
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum PopupPointerMode {
    #[default]
    PassThrough,
    Block,
}

/// A measured consumer may supply a bounded list of side/alignment/gap inputs.
/// Base still performs every candidate's positioning and viewport clamping.
pub(crate) struct PopupCandidate {
    side: Placement,
    align: Align,
    gap: Pixels,
}
impl PopupCandidate {
    pub(crate) fn new(side: Placement, align: Align, surface_gap: Pixels) -> Self {
        Self {
            side,
            align,
            gap: surface_gap,
        }
    }
}
type CandidateBuilder = Rc<dyn Fn(Size<Pixels>, &Window, &App) -> Vec<PopupCandidate>>;

#[derive(Clone)]
pub(crate) struct PopupMotion {
    enter: Transition,
    exit: Transition,
    entrance_distance: Pixels,
}
impl Default for PopupMotion {
    fn default() -> Self {
        Self::new(
            Transition::new(Duration::ZERO),
            Transition::new(Duration::ZERO),
        )
    }
}
impl PopupMotion {
    pub(crate) fn new(enter: Transition, exit: Transition) -> Self {
        Self {
            enter,
            exit,
            entrance_distance: px(0.),
        }
    }
    pub(crate) fn entrance_distance(mut self, distance: Pixels) -> Self {
        self.entrance_distance = distance;
        self
    }
}

#[derive(Clone)]
struct MovePolicy {
    transition: Transition,
    max_distance: Pixels,
}

/// A frame declaration, never a second open controller. All numeric dimensions
/// are logical pixels resolved by the caller; there are no theme dependencies.
pub(crate) struct PopupSpec {
    id: PopupId,
    parent: Option<PopupId>,
    anchor: PopupAnchorHandle,
    present: bool,
    placement: PopupPlacement,
    fallback: Vec<(Placement, Align)>,
    gap: Pixels,
    tip_gap: bool,
    margin: Pixels,
    pointer: PopupPointerMode,
    arrow: Option<ArrowStyle>,
    outsets: Edges<Pixels>,
    motion: PopupMotion,
    movement: Option<MovePolicy>,
    protected: Vec<(PopupAnchorHandle, Pixels)>,
    route: PopupDismissRoute,
    escape: bool,
    outside: bool,
    exclude: Vec<PopupAnchorHandle>,
    scope: Option<FocusHandle>,
    modal: Option<gpui_kit::base::ModalScope>,
    on_dismiss: Option<DismissHandler>,
    on_position: Option<Rc<dyn Fn(ResolvedPosition)>>,
    candidates: Option<CandidateBuilder>,
    move_anchor: Option<Bounds<Pixels>>,
    dismiss_unplaceable: bool,
    arrow_within_anchor: bool,
}
impl PopupSpec {
    pub(crate) fn new(
        id: PopupId,
        anchor: PopupAnchorHandle,
        present: bool,
        placement: PopupPlacement,
    ) -> Self {
        Self {
            id,
            parent: None,
            anchor,
            present,
            placement,
            fallback: Vec::new(),
            gap: px(0.),
            tip_gap: false,
            margin: px(0.),
            pointer: PopupPointerMode::default(),
            arrow: None,
            outsets: Edges::default(),
            motion: PopupMotion::default(),
            movement: None,
            protected: Vec::new(),
            route: PopupDismissRoute::None,
            escape: false,
            outside: false,
            exclude: Vec::new(),
            scope: None,
            modal: None,
            on_dismiss: None,
            on_position: None,
            candidates: None,
            move_anchor: None,
            dismiss_unplaceable: false,
            arrow_within_anchor: false,
        }
    }
    pub(crate) fn parent(mut self, parent: PopupId) -> Self {
        self.parent = Some(parent);
        self
    }
    pub(crate) fn modal_scope(mut self, scope: Option<gpui_kit::base::ModalScope>) -> Self {
        self.modal = scope;
        self
    }
    pub(crate) fn candidates_with(
        mut self,
        build: impl Fn(Size<Pixels>, &Window, &App) -> Vec<PopupCandidate> + 'static,
    ) -> Self {
        self.candidates = Some(Rc::new(build));
        self
    }
    /// Opt in to a lifecycle close if no complete, safe visual envelope fits.
    pub(crate) fn dismiss_unplaceable(mut self) -> Self {
        self.dismiss_unplaceable = true;
        self
    }
    pub(crate) fn arrow_within_anchor(mut self) -> Self {
        self.arrow_within_anchor = true;
        self
    }
    /// A layout move of the same anchor follows immediately; the supplied
    /// transition is reserved for the explicitly submitted new anchor target.
    pub(crate) fn translate_to_anchor(
        mut self,
        transition: Transition,
        max_distance: Pixels,
        anchor: Bounds<Pixels>,
    ) -> Self {
        self.movement = Some(MovePolicy {
            transition,
            max_distance,
        });
        self.move_anchor = Some(anchor);
        self
    }
    fn reserve(&self) -> Pixels {
        visual_reserve(self.outsets, self.arrow)
    }
    fn candidates_at(
        &self,
        size: Size<Pixels>,
        anchor: Bounds<Pixels>,
        offset: Pixels,
        window: &Window,
        cx: &App,
    ) -> Vec<ResolvedPosition> {
        if let Some(build) = &self.candidates {
            // Custom candidates describe their resting gaps (including a group
            // rail). Apply the shared presence displacement to those gaps too.
            let resting_gap = self.gap
                + if self.tip_gap {
                    self.arrow.map_or(px(0.), |a| a.spec().depth())
                } else {
                    px(0.)
                };
            let displacement = offset - resting_gap;
            return build(size, window, cx)
                .into_iter()
                .take(12)
                .map(|candidate| {
                    Positioner::side(anchor)
                        .placement(candidate.side)
                        .align(candidate.align)
                        .offset(candidate.gap + displacement)
                        .margin(self.margin + self.reserve())
                        .flip(false)
                        .resolve_in(size, window)
                })
                .collect();
        }
        let mut positions = vec![
            positioner(self.placement, anchor, offset, self.margin + self.reserve())
                .resolve_in(size, window),
        ];
        for (side, align) in &self.fallback {
            positions.push(
                Positioner::side(anchor)
                    .placement(*side)
                    .align(*align)
                    .offset(offset)
                    .margin(self.margin + self.reserve())
                    .flip(false)
                    .resolve_in(size, window),
            );
        }
        positions
    }
    fn protected_bounds(&self) -> Option<Vec<Bounds<Pixels>>> {
        self.protected
            .iter()
            .map(|(anchor, clearance)| {
                anchor
                    .bounds()
                    .map(|bounds| geometry::expand(bounds, Edges::all(*clearance)))
            })
            .collect()
    }
    fn geometry_at(
        &self,
        position: ResolvedPosition,
        aim: Bounds<Pixels>,
        viewport: Bounds<Pixels>,
        protected: &[Bounds<Pixels>],
    ) -> Result<(Option<super::arrow::ArrowGeometry>, Bounds<Pixels>), PopupValidity> {
        let arrow = match (self.arrow, position.placement) {
            (Some(arrow), Some(side)) => Some(
                arrow
                    .spec()
                    .resolve(position.bounds, aim.center(), side)
                    .ok_or(PopupValidity::ArrowDoesNotFit)?,
            ),
            _ => None,
        };
        if self.arrow_within_anchor
            && let (Some(arrow), Some(side)) = (arrow, position.placement)
        {
            let tip = arrow.tip();
            let points_to_anchor = match side {
                Placement::Top | Placement::Bottom => tip.x >= aim.left() && tip.x <= aim.right(),
                Placement::Left | Placement::Right => tip.y >= aim.top() && tip.y <= aim.bottom(),
            };
            if !points_to_anchor {
                return Err(PopupValidity::ArrowDoesNotFit);
            }
        }
        let mut visual = geometry::expand(position.bounds, self.outsets);
        if let Some(arrow) = arrow {
            visual = geometry::union(visual, arrow.bounds());
        }
        if !geometry::finite(visual) || !geometry::contains(viewport, visual) {
            return Err(PopupValidity::NoSpace);
        }
        if protected
            .iter()
            .any(|region| geometry::overlaps(visual, *region))
        {
            return Err(PopupValidity::Protected);
        }
        Ok((arrow, visual))
    }
    /// Uses the identical candidate inputs, Base resolver and complete envelope
    /// checks as prepaint; a controller can choose a fade before changing owners.
    pub(crate) fn can_translate(
        &self,
        from: &super::runtime::PopupSnapshot,
        size: Size<Pixels>,
        budget: Pixels,
        window: &Window,
        cx: &App,
    ) -> bool {
        let (Some(anchor), Some(position), Some(visual), Some(protected)) = (
            self.anchor.bounds(),
            from.position(),
            from.visual_bounds(),
            self.protected_bounds(),
        ) else {
            return false;
        };
        let viewport =
            positioner(self.placement, anchor, px(0.), self.margin).available_bounds(window);
        let gap = self.gap
            + if self.tip_gap {
                self.arrow.map_or(px(0.), |a| a.spec().depth())
            } else {
                px(0.)
            };
        self.candidates_at(size, anchor, gap, window, cx)
            .into_iter()
            .find_map(|mut candidate| {
                candidate.bounds = window.pixel_snap_bounds(candidate.bounds);
                let (_, target) = self
                    .geometry_at(candidate, anchor, viewport, &protected)
                    .ok()?;
                Some(
                    position.placement == candidate.placement
                        && position.bounds.size == candidate.bounds.size
                        && geometry::translation_allowed(
                            visual, target, budget, &protected, viewport,
                        ),
                )
            })
            .unwrap_or(false)
    }
    pub(crate) fn surface_gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self.tip_gap = false;
        self
    }
    #[allow(dead_code)] // Next consumer uses tip-to-trigger rather than surface gap.
    pub(crate) fn tip_gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self.tip_gap = true;
        self
    }
    pub(crate) fn margin(mut self, margin: Pixels) -> Self {
        self.margin = margin;
        self
    }
    pub(crate) fn pointer_mode(mut self, mode: PopupPointerMode) -> Self {
        self.pointer = mode;
        self
    }
    pub(crate) fn arrow(mut self, arrow: ArrowStyle) -> Self {
        self.arrow = Some(arrow);
        self
    }
    pub(crate) fn motion(mut self, motion: PopupMotion) -> Self {
        self.motion = motion;
        self
    }
    pub(crate) fn dismiss_route(mut self, route: PopupDismissRoute) -> Self {
        self.route = route;
        self
    }
    pub(crate) fn exclude(mut self, anchor: PopupAnchorHandle) -> Self {
        self.exclude.push(anchor);
        self
    }
    pub(crate) fn on_dismiss(
        mut self,
        handler: impl Fn(PopupDismissRequest, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }
    pub(crate) fn on_position(mut self, handler: impl Fn(ResolvedPosition) + 'static) -> Self {
        self.on_position = Some(Rc::new(handler));
        self
    }
    #[allow(dead_code)] // Explicit opt-in for unfocused Runtime consumers.
    pub(crate) fn dismiss_on(mut self, escape: bool, outside: bool) -> Self {
        self.escape = escape;
        self.outside = outside;
        self
    }
    #[allow(dead_code)]
    pub(crate) fn fallback(
        mut self,
        candidates: impl IntoIterator<Item = (Placement, Align)>,
    ) -> Self {
        self.fallback = candidates.into_iter().take(12).collect();
        self
    }
    #[allow(dead_code)]
    pub(crate) fn protected_regions(
        mut self,
        regions: impl IntoIterator<Item = (PopupAnchorHandle, Pixels)>,
    ) -> Self {
        self.protected = regions.into_iter().collect();
        self
    }
    #[allow(dead_code)]
    pub(crate) fn translate(mut self, transition: Transition, max_distance: Pixels) -> Self {
        self.movement = Some(MovePolicy {
            transition,
            max_distance,
        });
        self
    }
    #[allow(dead_code)]
    pub(crate) fn scope(mut self, scope: FocusHandle) -> Self {
        self.scope = Some(scope);
        self
    }
    pub(crate) fn visual_outsets(mut self, outsets: Edges<Pixels>) -> Self {
        self.outsets = outsets;
        self
    }
}

/// Consume the content factory exactly once, only if Presence needs a surface.
/// Exiting content must be built as paint-only: the typed presentation tells its
/// semantic owner to omit handlers, focus tracking and accessibility roles.
pub(crate) struct PopupHost {
    spec: PopupSpec,
}
#[derive(Clone, Copy)]
pub(crate) struct PopupPresentation {
    interactive: bool,
}
impl PopupPresentation {
    pub(crate) fn is_interactive(self) -> bool {
        self.interactive
    }
}

impl PopupHost {
    /// The same Base content viewport used by the host, including visual
    /// reserves. Consumers can wrap text before committing a natural size.
    pub(crate) fn available_size(
        arrow: Option<ArrowStyle>,
        outsets: Edges<Pixels>,
        margin: Pixels,
        window: &Window,
    ) -> Size<Pixels> {
        Positioner::side(Bounds::default())
            .margin(margin + visual_reserve(outsets, arrow))
            .available_bounds(window)
            .size
    }
    pub(crate) fn new(spec: PopupSpec) -> Self {
        Self { spec }
    }

    /// Layout ceiling after reserving the same visual envelope as the positioner.
    /// Consumers subtract their border/padding before constraining a scroll area.
    pub(crate) fn content_size(&self, window: &Window) -> Size<Pixels> {
        Self::available_size(self.spec.arrow, self.spec.outsets, self.spec.margin, window)
    }

    pub(crate) fn mount(
        self,
        content: impl FnOnce(
            PopupPresentation,
            PopupSnapshotHandle,
            &mut Window,
            &mut App,
        ) -> AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let mut spec = self.spec;
        let runtime = PopupRuntime::for_window(window, cx);
        spec.modal = spec
            .modal
            .or_else(|| gpui_kit::base::building_modal_scope(window, cx));
        let active = gpui_kit::base::active_modal_scope(window, cx);
        if active.as_ref().is_some_and(|s| !s.is_interactive())
            || active.as_ref().map(|s| s.owner()) != spec.modal.as_ref().map(|s| s.owner())
        {
            runtime.deactivate(&spec.id);
            return None;
        }
        let presence = Presence::new((spec.id.local(), "popup-presence"), spec.present)
            .transition(if spec.present {
                spec.motion.enter.clone()
            } else {
                spec.motion.exit.clone()
            })
            .sample(window, cx);
        if !presence.should_render() || (!spec.present && presence.progress <= 0.) {
            runtime.deactivate(&spec.id);
            return None;
        }
        let registration = runtime.register(RegistrationSpec {
            id: spec.id.clone(),
            parent: spec.parent.clone(),
            present: spec.present,
            route: spec.route,
            escape: spec.escape,
            outside: spec.outside,
            exclude: spec.exclude.clone(),
            scope: spec.scope.clone(),
            modal: spec.modal.clone(),
            handler: spec.on_dismiss.clone(),
        });
        let lease =
            window.use_keyed_state((spec.id.local(), "popup-registration"), cx, |_, _| None);
        lease.update(cx, |slot, _| *slot = Some(registration.clone()));
        let snapshot = registration.snapshot();
        let previous = snapshot.read();
        let phase = presence.phase;
        let progress = presence.progress;
        let anchor = spec.anchor.clone();
        let gap = spec.gap
            + if spec.tip_gap {
                spec.arrow.map_or(px(0.), |a| a.spec().depth())
            } else {
                px(0.)
            };
        let offset = if matches!(spec.placement, PopupPlacement::Point(_)) {
            gap
        } else {
            gap - spec.motion.entrance_distance * (1. - progress)
        };
        // Reserve the entire caller-provided shadow, not an extra hitbox. The
        // conservative symmetric margin keeps every candidate's envelope safe.
        let reserve = spec.reserve();
        let make_positioner =
            |bounds| positioner(spec.placement, bounds, offset, spec.margin + reserve);
        let source = anchor.clone();
        let positioned =
            make_positioner(Bounds::default()).anchor_source(move |_, _| source.bounds());
        let positioned = if spec.present && spec.pointer == PopupPointerMode::Block {
            positioned.occlude()
        } else {
            positioned
        };
        let arrow_snapshot = snapshot.clone();
        let arrow_style = spec.arrow;
        let arrow_layer = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                if let (Some(style), Some(geometry)) = (arrow_style, arrow_snapshot.read().arrow())
                {
                    style.paint(geometry, window);
                }
            },
        )
        .absolute()
        .inset_0();
        let child = content(
            PopupPresentation {
                interactive: spec.present,
            },
            snapshot.clone(),
            window,
            cx,
        );
        let child = div()
            .relative()
            .opacity(progress)
            .child(child)
            .child(arrow_layer);
        let priority = registration.priority();
        Some(
            deferred(
                positioned
                    .position_with(move |base_position, window, cx| {
                        // Keep the per-window registry alive in non-Root test/host setups.
                        let _runtime = &runtime;
                        if registration.is_invalidated() {
                            return None;
                        }
                        if let (Some(scope), Some(trap)) =
                            (&spec.scope, gpui_kit::base::active_focus_trap(window, cx))
                            && !trap.contains(scope, window)
                        {
                            snapshot.invalidate(phase, PopupValidity::AnchorInvalid);
                            registration.invalidate(PopupDismissReason::ScopeInvalid, window, cx);
                            return None;
                        }
                        let Some(anchor) = anchor.snapshot() else {
                            snapshot.invalidate(phase, PopupValidity::AnchorInvalid);
                            registration.invalidate(PopupDismissReason::AnchorInvalid, window, cx);
                            return None;
                        };
                        if !registration.is_parent_present() {
                            snapshot.invalidate(phase, PopupValidity::ParentClosed);
                            registration.invalidate(PopupDismissReason::ParentClosed, window, cx);
                            return None;
                        }
                        let Some(base_position) = base_position else {
                            snapshot.invalidate(phase, PopupValidity::Unmeasured);
                            return None;
                        };
                        let viewport =
                            positioner(spec.placement, anchor.bounds, offset, spec.margin)
                                .available_bounds(window);
                        // A lost protected anchor invalidates the proof, rather than
                        // silently removing that object from the protection set.
                        let Some(protected) = spec.protected_bounds() else {
                            snapshot.invalidate(phase, PopupValidity::Protected);
                            if spec.dismiss_unplaceable {
                                registration.invalidate(
                                    PopupDismissReason::Unplaceable,
                                    window,
                                    cx,
                                );
                            }
                            return None;
                        };
                        let candidates = spec.candidates_at(
                            base_position.bounds.size,
                            anchor.bounds,
                            offset,
                            window,
                            cx,
                        );
                        let movement = spec.movement.as_ref().filter(|_| {
                            !cx.reduce_motion()
                                && spec
                                    .move_anchor
                                    .is_none_or(|expected| expected == anchor.bounds)
                        });
                        let mut failure = PopupValidity::NoSpace;
                        for mut candidate in candidates {
                            candidate.bounds = window.pixel_snap_bounds(candidate.bounds);
                            let measure = |position: ResolvedPosition, aim| {
                                spec.geometry_at(position, aim, viewport, &protected)
                            };
                            let Ok((_, target_visual)) = measure(candidate, anchor.bounds)
                                .inspect_err(|error| failure = *error)
                            else {
                                continue;
                            };
                            let policy = if let Some(movement) = movement {
                                if let (Some(from), Some(from_visual)) =
                                    (previous.last_position(), previous.last_visual())
                                {
                                    // Positioner offsets an already-measured child; it
                                    // cannot interpolate its layout size. Refuse a resize
                                    // rather than publish a hitbox unlike the painted box.
                                    if from.placement != candidate.placement
                                        || from.bounds.size != candidate.bounds.size
                                        || !geometry::translation_allowed(
                                            from_visual,
                                            target_visual,
                                            movement.max_distance,
                                            &protected,
                                            viewport,
                                        )
                                    {
                                        failure = PopupValidity::UnsafeMotion;
                                        continue;
                                    }
                                }
                                movement.transition.clone()
                            } else {
                                Transition::new(Duration::ZERO)
                            };
                            // Base owns the transition channel. Sampling in prepaint is
                            // deliberate: the target is only known after this frame's
                            // anchor/layout; the same sample feeds child prepaint/hitboxes.
                            let mut arrow_anchor = anchor.bounds;
                            if spec.present {
                                arrow_anchor = transition(
                                    (spec.id.local(), "popup-arrow-anchor"),
                                    arrow_anchor,
                                    policy.clone(),
                                    window,
                                    cx,
                                );
                                candidate.bounds = transition(
                                    (spec.id.local(), "popup-position"),
                                    candidate.bounds,
                                    policy,
                                    window,
                                    cx,
                                );
                            }
                            candidate.bounds = window.pixel_snap_bounds(candidate.bounds);
                            let Ok((arrow, visual)) = measure(candidate, arrow_anchor)
                                .inspect_err(|error| failure = *error)
                            else {
                                continue;
                            };
                            if let Some(movement) = movement
                                && !geometry::translation_allowed(
                                    visual,
                                    target_visual,
                                    movement.max_distance,
                                    &protected,
                                    viewport,
                                )
                            {
                                failure = PopupValidity::UnsafeMotion;
                                continue;
                            }
                            snapshot.publish(phase, anchor, candidate, visual, arrow);
                            registration.validate();
                            if let Some(observer) = &spec.on_position {
                                observer(candidate);
                            }
                            return Some(candidate);
                        }
                        snapshot.invalidate(phase, failure);
                        if spec.dismiss_unplaceable {
                            registration.invalidate(PopupDismissReason::Unplaceable, window, cx);
                        }
                        None
                    })
                    .child(child),
            )
            .with_priority(priority)
            .into_any_element(),
        )
    }
}

fn visual_reserve(outsets: Edges<Pixels>, arrow: Option<ArrowStyle>) -> Pixels {
    [outsets.left, outsets.right, outsets.top, outsets.bottom]
        .into_iter()
        .fold(px(0.), Pixels::max)
        .max(arrow.map_or(px(0.), |a| a.spec().depth()))
}

fn positioner(
    placement: PopupPlacement,
    anchor: Bounds<Pixels>,
    offset: Pixels,
    margin: Pixels,
) -> Positioner {
    match placement {
        PopupPlacement::Side(side, align) => Positioner::side(anchor)
            .placement(side)
            .align(align)
            .offset(offset)
            .margin(margin),
        PopupPlacement::Point(corner) => Positioner::corner(corner, anchor.origin).margin(margin),
    }
}

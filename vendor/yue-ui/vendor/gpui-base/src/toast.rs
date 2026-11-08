use crate::TestSupportExt as _;
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
use std::{cell::RefCell, collections::VecDeque, rc::Rc, time::Duration};
#[cfg(target_family = "wasm")]
use web_time::Instant;

use gpui::{
    Anchor, AnyElement, App, Div, ElementId, FocusHandle, InteractiveElement, Interactivity,
    IntoElement, MouseMoveEvent, ParentElement, Pixels, RenderOnce, Role, Stateful,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, canvas, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ElementExt as _, StyledExt as _,
    motion::{Spring, spring_at},
};

/// Motion tokens used by an unstyled toast stack.
#[derive(Clone, Copy, Debug)]
pub struct ToastMotion {
    /// Time scale of the stack's motion.
    ///
    /// This is read as two different quantities, because the stack sequences
    /// one thing and interpolates another. [`ToastManager`] treats it as a
    /// deadline: a toast is present once it has elapsed. The layout treats it
    /// as a spring response, which is the scale the reflow is felt at rather
    /// than the moment it stops — a toast arriving still nudges the stack for
    /// a little longer than this.
    pub duration: Duration,
    /// Duration before an ending toast is unmounted.
    pub exit_duration: Duration,
    /// Visible distance between collapsed toast layers.
    pub collapsed_peek: Pixels,
    /// Distance between expanded toast items.
    pub expanded_gap: Pixels,
    /// Fractional width reduction for each collapsed layer.
    pub collapsed_scale_step: f32,
    /// Number of layers visible while the stack is collapsed.
    pub collapsed_visible: usize,
}

impl ToastMotion {
    /// Create motion matching the shadcn/Sonner toaster.
    pub fn sonner() -> Self {
        Self {
            duration: Duration::from_millis(400),
            exit_duration: Duration::from_millis(200),
            collapsed_peek: px(14.),
            expanded_gap: px(14.),
            collapsed_scale_step: 0.05,
            collapsed_visible: 3,
        }
    }
}

impl Default for ToastMotion {
    fn default() -> Self {
        Self::sonner()
    }
}

/// Persistent private layout state used by [`ToastStack`].
#[derive(Clone, Debug, Default)]
pub struct ToastStackState {
    heights: Rc<RefCell<std::collections::HashMap<ElementId, Pixels>>>,
    width: Rc<std::cell::Cell<Pixels>>,
    hovered: Rc<std::cell::Cell<bool>>,
    focused: Rc<std::cell::Cell<bool>>,
}

impl ToastStackState {
    /// Return whether interaction has expanded the stack.
    pub fn is_hovered(&self) -> bool {
        self.hovered.get()
    }
    pub fn is_focused(&self) -> bool {
        self.focused.get()
    }
    /// Seed a natural measurement made without painting or registering input.
    pub fn set_measurement(&self, id: impl Into<ElementId>, height: Pixels, width: Pixels) {
        self.heights.borrow_mut().insert(id.into(), height);
        self.width.set(width);
    }
    pub fn measured_height(&self, id: &ElementId) -> Option<Pixels> {
        self.heights.borrow().get(id).copied()
    }
    pub fn measured_width(&self) -> Pixels {
        self.width.get()
    }
    pub fn clear(&self) {
        self.heights.borrow_mut().clear();
        self.width.set(px(0.));
        self.hovered.set(false);
        self.focused.set(false);
    }
    pub fn is_expanded(&self) -> bool {
        self.hovered.get() || self.focused.get()
    }
}

/// Options applied when a toast enters a [`ToastManager`].
#[derive(Clone, Copy, Debug, Default)]
pub struct ToastOptions {
    /// Active time before automatic dismissal; `None` disables auto-hide.
    pub timeout: Option<Duration>,
}

#[derive(Debug)]
struct ManagedToast<I, T> {
    id: I,
    value: T,
    status: ToastTransitionStatus,
    timeout_remaining: Option<Duration>,
    timeout_total: Option<Duration>,
    transition_elapsed: Duration,
    last_advance: Instant,
}

/// Changes produced when a toast manager advances its lifecycle clock.
#[derive(Debug)]
pub struct ToastAdvance<I, T> {
    /// Whether a mounted toast changed phase or membership.
    pub changed: bool,
    /// Toast ids whose entry transition completed.
    pub presented: Vec<I>,
    /// Toast ids that entered their ending transition.
    pub ending: Vec<I>,
    /// Toast values removed after their ending transition completed.
    pub removed: Vec<(I, T)>,
}

/// Ordered toast storage, lifecycle, auto-hide, limits, and exit coordination.
#[derive(Debug)]
pub struct ToastManager<I, T> {
    entries: VecDeque<ManagedToast<I, T>>,
    transition_duration: Duration,
    exit_duration: Duration,
}

impl<I, T> ToastManager<I, T> {
    /// Create a manager using the supplied motion duration for enter and exit.
    pub fn new(motion: ToastMotion) -> Self {
        Self {
            entries: VecDeque::new(),
            transition_duration: motion.duration,
            exit_duration: motion.exit_duration,
        }
    }

    /// Separates presence deadlines from the stack's layout response.
    pub fn durations(mut self, enter: Duration, exit: Duration) -> Self {
        self.transition_duration = enter;
        self.exit_duration = exit;
        self
    }

    /// Change future and currently running presence deadlines without restarting them.
    pub fn set_durations(&mut self, enter: Duration, exit: Duration) {
        self.transition_duration = enter;
        self.exit_duration = exit;
    }

    /// Time already consumed by the current presence transition.
    pub fn transition_elapsed(&self, id: &I) -> Option<Duration>
    where
        I: Eq,
    {
        self.entries
            .iter()
            .find(|entry| &entry.id == id)
            .map(|entry| entry.transition_elapsed)
    }

    /// Remaining and original reading time. Call `advance` before reading a snapshot.
    pub fn reading_time(&self, id: &I) -> Option<(Duration, Duration)>
    where
        I: Eq,
    {
        let entry = self.entries.iter().find(|entry| &entry.id == id)?;
        Some((entry.timeout_remaining?, entry.timeout_total?))
    }

    /// Earliest presence or reading deadline. Pausing only suppresses reading time.
    pub fn next_deadline(&self, now: Instant, paused: bool) -> Option<Instant> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let remaining = match entry.status {
                    ToastTransitionStatus::Starting => self
                        .transition_duration
                        .saturating_sub(entry.transition_elapsed),
                    ToastTransitionStatus::Present if !paused => entry.timeout_remaining?,
                    ToastTransitionStatus::Present => return None,
                    ToastTransitionStatus::Ending => {
                        self.exit_duration.saturating_sub(entry.transition_elapsed)
                    }
                };
                Some(
                    now + remaining
                        .saturating_sub(now.saturating_duration_since(entry.last_advance)),
                )
            })
            .min()
    }

    /// Return the number of mounted toasts, including ending toasts.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return whether no toast is mounted.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over mounted toast ids, values, and phases in display order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&I, &T, ToastTransitionStatus)> {
        self.entries
            .iter()
            .map(|entry| (&entry.id, &entry.value, entry.status))
    }

    /// Iterate over newest visible active toasts plus ending toasts.
    pub fn visible(&self, limit: usize) -> impl Iterator<Item = (&I, &T, ToastTransitionStatus)> {
        let first = self
            .entries
            .iter()
            .filter(|entry| entry.status != ToastTransitionStatus::Ending)
            .count()
            .saturating_sub(limit);
        let mut active_index = 0usize;
        self.entries.iter().filter_map(move |entry| {
            let visible = if entry.status == ToastTransitionStatus::Ending {
                true
            } else {
                let keep = active_index >= first;
                active_index += 1;
                keep
            };
            visible.then_some((&entry.id, &entry.value, entry.status))
        })
    }

    /// Mutate presentation metadata without replacing identity or lifecycle.
    pub fn get_mut(&mut self, id: &I) -> Option<&mut T>
    where
        I: Eq,
    {
        self.entries
            .iter_mut()
            .find(|entry| &entry.id == id)
            .map(|entry| &mut entry.value)
    }
    /// Reset only the optional reading budget; an outer `None` preserves it.
    pub fn reset_timeout(&mut self, id: &I, timeout: Option<Option<Duration>>)
    where
        I: Eq,
    {
        if let Some(timeout) = timeout {
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|e| &e.id == id && e.status != ToastTransitionStatus::Ending)
            {
                entry.timeout_remaining = timeout;
                entry.timeout_total = timeout;
            }
        }
    }
    pub fn reading_time_at(
        &self,
        id: &I,
        now: Instant,
        paused: bool,
    ) -> Option<(Duration, Duration)>
    where
        I: Eq,
    {
        let entry = self.entries.iter().find(|entry| &entry.id == id)?;
        let elapsed = if entry.status == ToastTransitionStatus::Present && !paused {
            now.saturating_duration_since(entry.last_advance)
        } else {
            Duration::ZERO
        };
        Some((
            entry.timeout_remaining?.saturating_sub(elapsed),
            entry.timeout_total?,
        ))
    }
    pub fn transition_elapsed_at(&self, id: &I, now: Instant) -> Option<Duration>
    where
        I: Eq,
    {
        let entry = self.entries.iter().find(|entry| &entry.id == id)?;
        Some(entry.transition_elapsed + now.saturating_duration_since(entry.last_advance))
    }

    /// Return a mounted toast value by id.
    pub fn get(&self, id: &I) -> Option<&T>
    where
        I: Eq,
    {
        self.entries
            .iter()
            .find_map(|entry| (&entry.id == id).then_some(&entry.value))
    }
}

impl<I: Clone + Eq, T> ToastManager<I, T> {
    /// Add a newest toast, replacing an existing toast with the same id.
    pub fn push(&mut self, id: I, value: T, options: ToastOptions, now: Instant) -> Option<T> {
        let replaced = self
            .entries
            .iter()
            .position(|entry| entry.id == id)
            .and_then(|index| self.entries.remove(index))
            .map(|entry| entry.value);
        self.entries.push_back(ManagedToast {
            id,
            value,
            status: ToastTransitionStatus::Starting,
            timeout_remaining: options.timeout,
            timeout_total: options.timeout,
            transition_elapsed: Duration::ZERO,
            last_advance: now,
        });
        replaced
    }

    /// Replace content in place. An outer `None` preserves the reading clock;
    /// `Some(None)` makes it persistent and `Some(Some(duration))` starts a new clock.
    /// Ending entries cannot be revived by an update.
    pub fn update(&mut self, id: &I, value: T, timeout: Option<Option<Duration>>) -> Result<T, T> {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| &entry.id == id && entry.status != ToastTransitionStatus::Ending)
        else {
            return Err(value);
        };
        if let Some(timeout) = timeout {
            entry.timeout_remaining = timeout;
            entry.timeout_total = timeout;
        }
        Ok(std::mem::replace(&mut entry.value, value))
    }

    /// Begin a toast's exit transition, returning whether its state changed.
    pub fn dismiss(&mut self, id: &I, now: Instant) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| &entry.id == id) else {
            return false;
        };
        if entry.status == ToastTransitionStatus::Ending {
            return false;
        }
        entry.status = ToastTransitionStatus::Ending;
        entry.transition_elapsed = Duration::ZERO;
        entry.last_advance = now;
        true
    }

    /// Begin the exit transition for every active toast.
    pub fn dismiss_all(&mut self, now: Instant) -> Vec<I> {
        let mut changed = Vec::new();
        for entry in &mut self.entries {
            if entry.status != ToastTransitionStatus::Ending {
                entry.status = ToastTransitionStatus::Ending;
                entry.transition_elapsed = Duration::ZERO;
                entry.last_advance = now;
                changed.push(entry.id.clone());
            }
        }
        changed
    }

    /// Advance lifecycle time; active timers pause while `paused` is true.
    pub fn advance(&mut self, now: Instant, paused: bool) -> ToastAdvance<I, T> {
        let mut ending = Vec::new();
        let mut presented = Vec::new();
        let mut changed = false;
        for entry in &mut self.entries {
            let mut delta = now.saturating_duration_since(entry.last_advance);
            entry.last_advance = now;
            loop {
                match entry.status {
                    ToastTransitionStatus::Starting => {
                        let needed = self
                            .transition_duration
                            .saturating_sub(entry.transition_elapsed);
                        if delta >= needed {
                            delta = delta.saturating_sub(needed);
                            entry.status = ToastTransitionStatus::Present;
                            entry.transition_elapsed = Duration::ZERO;
                            presented.push(entry.id.clone());
                            changed = true;
                        } else {
                            entry.transition_elapsed += delta;
                            break;
                        }
                    }
                    ToastTransitionStatus::Present if !paused => {
                        if let Some(remaining) = &mut entry.timeout_remaining {
                            if delta >= *remaining {
                                delta = delta.saturating_sub(*remaining);
                                *remaining = Duration::ZERO;
                                entry.status = ToastTransitionStatus::Ending;
                                entry.transition_elapsed = Duration::ZERO;
                                ending.push(entry.id.clone());
                                changed = true;
                            } else {
                                *remaining -= delta;
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    ToastTransitionStatus::Ending => {
                        entry.transition_elapsed += delta;
                        break;
                    }
                    ToastTransitionStatus::Present => break,
                }
            }
        }
        let mut removed = Vec::new();
        let mut index = 0;
        while index < self.entries.len() {
            if self.entries[index].status == ToastTransitionStatus::Ending
                && self.entries[index].transition_elapsed >= self.exit_duration
            {
                let entry = self.entries.remove(index).expect("toast index is valid");
                removed.push((entry.id, entry.value));
                changed = true;
            } else {
                index += 1;
            }
        }
        ToastAdvance {
            changed,
            presented,
            ending,
            removed,
        }
    }
}

/// A deep toast-stack element that owns measurement, overlap, and expansion motion.
#[derive(IntoElement)]
pub struct ToastStack {
    id: ElementId,
    base: crate::ObservedElement<Stateful<Div>>,
    style: StyleRefinement,
    state: ToastStackState,
    motion: ToastMotion,
    placement: Anchor,
    focus_handle: Option<FocusHandle>,
    children: Vec<(ElementId, AnyElement)>,
    expanded: Option<bool>,
    front_shell: bool,
    center_front: bool,
    measure_items: bool,
    sample_time: Option<(Instant, bool)>,
}

impl ToastStack {
    /// Create a toast stack with Base UI-compatible motion.
    pub fn new(id: impl Into<ElementId>, state: ToastStackState) -> Self {
        let id = id.into();
        Self {
            base: div().id(id.clone()).test_support(),
            id,
            style: StyleRefinement::default(),
            state,
            motion: ToastMotion::sonner(),
            placement: Anchor::TopRight,
            focus_handle: None,
            children: Vec::new(),
            expanded: None,
            front_shell: false,
            center_front: false,
            measure_items: true,
            sample_time: None,
        }
    }

    /// Add a stably keyed toast item to the stack.
    pub fn item(mut self, id: impl Into<ElementId>, child: impl IntoElement) -> Self {
        self.children.push((id.into(), child.into_any_element()));
        self
    }

    /// A controlled expansion target, allowing a consumer-owned leave grace period.
    pub fn expanded(mut self, value: bool) -> Self {
        self.expanded = Some(value);
        self
    }
    /// Opt into front-height folded shells; legacy natural-height geometry is unchanged.
    pub fn front_shell(mut self, value: bool) -> Self {
        self.front_shell = value;
        self
    }
    /// Anchor the foreground centre rather than the entire expanded stack.
    pub fn center_front(mut self, value: bool) -> Self {
        self.center_front = value;
        self
    }
    /// Use natural sizes seeded with `ToastStackState::set_measurement`.
    pub fn measure_items(mut self, value: bool) -> Self {
        self.measure_items = value;
        self
    }
    /// Supply a local monotonic time for a diagnostic playback surface.
    pub fn sample_time(mut self, now: Instant, request_frames: bool) -> Self {
        self.sample_time = Some((now, request_frames));
        self
    }

    /// Set the stack motion tokens.
    pub fn motion(mut self, motion: ToastMotion) -> Self {
        self.motion = motion;
        self
    }

    /// Set the viewport edge used to anchor stack geometry.
    pub fn placement(mut self, placement: Anchor) -> Self {
        self.placement = placement;
        self
    }

    /// Set the focus scope that expands the stack and pauses auto-hide timers.
    pub fn focus_handle(mut self, focus_handle: FocusHandle) -> Self {
        self.focus_handle = Some(focus_handle);
        self
    }
}

impl Styled for ToastStack {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ToastStack {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.children
            .extend(children.into_iter().enumerate().map(|(index, child)| {
                (
                    ElementId::NamedInteger("toast-stack-child".into(), index as u64),
                    child,
                )
            }));
    }
}

impl InteractiveElement for ToastStack {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }

    fn track_focus(mut self, handle: &gpui::FocusHandle) -> Self {
        self.base = self.base.track_focus(handle);
        self
    }
}

impl StatefulInteractiveElement for ToastStack {}

fn stack_geometry(
    heights: &[Pixels],
    gap: Pixels,
    peek: Pixels,
    anchored_bottom: bool,
) -> (Pixels, Pixels, Vec<(Pixels, Pixels)>) {
    let count = heights.len();
    let expanded_height = heights
        .iter()
        .copied()
        .fold(px(0.), |sum, height| sum + height)
        + gap * count.saturating_sub(1) as f32;
    let front_height = heights.last().copied().unwrap_or(px(0.));
    let collapsed_height = heights
        .iter()
        .enumerate()
        .map(|(index, height)| {
            let rank = count - 1 - index;
            *height + peek * rank as f32
        })
        .fold(
            front_height + peek * count.saturating_sub(1) as f32,
            Pixels::max,
        );
    let offsets = heights
        .iter()
        .enumerate()
        .map(|(index, height)| {
            let rank = count - 1 - index;
            let newer_height = heights[(index + 1)..]
                .iter()
                .copied()
                .fold(px(0.), |sum, height| sum + height);
            let expanded = if anchored_bottom {
                expanded_height - newer_height - gap * rank as f32 - *height
            } else {
                newer_height + gap * rank as f32
            };
            let collapsed = if anchored_bottom {
                collapsed_height - *height - peek * rank as f32
            } else {
                peek * rank as f32
            };
            (collapsed, expanded)
        })
        .collect();
    (collapsed_height, expanded_height, offsets)
}

impl RenderOnce for ToastStack {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focused = self
            .focus_handle
            .as_ref()
            .is_some_and(|handle| handle.contains_focused(window, cx));
        self.state.focused.set(focused);
        let expanded = self.expanded.unwrap_or_else(|| self.state.is_expanded());
        let keys = self
            .children
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        self.state
            .heights
            .borrow_mut()
            .retain(|id, _| keys.contains(id));
        let measured_by_id = self.state.heights.borrow().clone();
        let heights = keys
            .iter()
            .map(|id| measured_by_id.get(id).copied().unwrap_or(px(0.)))
            .collect::<Vec<_>>();
        let measured = self.state.heights.clone();
        let peek = self.motion.collapsed_peek;
        let gap = self.motion.expanded_gap;
        let scale_step = self.motion.collapsed_scale_step;
        let collapsed_visible = self.motion.collapsed_visible.max(1);
        let stack_width = self.state.width.get();
        let count = self.children.len();
        let anchored_bottom = matches!(
            self.placement,
            Anchor::BottomLeft | Anchor::BottomCenter | Anchor::BottomRight
        );
        let (mut collapsed_height, expanded_height, mut offsets) = stack_geometry(
            &heights[..count.min(heights.len())],
            gap,
            peek,
            anchored_bottom,
        );
        let front_height = heights.last().copied().unwrap_or(px(0.));
        if self.front_shell {
            collapsed_height = front_height + peek * count.saturating_sub(1) as f32;
            for (ix, (collapsed, _)) in offsets.iter_mut().enumerate() {
                let rank = count - 1 - ix;
                *collapsed = if anchored_bottom {
                    collapsed_height - front_height - peek * rank as f32
                } else {
                    peek * rank as f32
                };
            }
        }
        let (sample_now, request_frames) = self
            .sample_time
            .unwrap_or_else(|| (cx.background_executor().now(), true));
        let measure_items = self.measure_items;
        let center = if self.center_front {
            spring_at(
                (self.id.clone(), "front-center"),
                front_height,
                Spring::new(self.motion.duration).with_epsilon(0.1),
                sample_now,
                request_frames,
                window,
                cx,
            ) / 2.
        } else {
            px(0.)
        };
        // The whole stack reflows every time a toast arrives or leaves, so each
        // layer is sprung: one retargeted mid-move carries its velocity into the
        // new layout instead of restarting. Critically damped, because a height
        // or an opacity that overshoots its target reads as a glitch. Geometry
        // settles in pixels, so its tolerance is coarser than the fade's.
        //
        // A bottom-anchored item's position is composed from two of these — the
        // stack height and the item's own offset — and the stack height only
        // acquires its real target once the new toast has been measured in
        // prepaint, a frame after the offsets know theirs. Two springs sharing a
        // config stay proportional to each other only while they also share a
        // start, so that one frame of skew lets the composed position pass its
        // settled value by a fraction of a pixel before arriving. Both springs
        // snap on settling, so it is a transient, not a resting error.
        let geometry = Spring::new(self.motion.duration).with_epsilon(0.1);
        let fade = Spring::new(self.motion.duration);
        let stack_height = spring_at(
            (self.id.clone(), "height"),
            if expanded {
                expanded_height
            } else {
                collapsed_height
            },
            geometry,
            sample_now,
            request_frames,
            window,
            cx,
        );
        let items = self
            .children
            .into_iter()
            .enumerate()
            .map(move |(index, (item_id, child))| {
                let (collapsed_offset, expanded_offset) =
                    offsets.get(index).copied().unwrap_or((px(0.), px(0.)));
                let measured = measured.clone();
                let measured_id = item_id.clone();
                let rank = count.saturating_sub(1 + index);
                let target_offset = if expanded {
                    expanded_offset
                } else {
                    collapsed_offset
                };
                let offset = spring_at(
                    (item_id.clone(), "offset"),
                    target_offset,
                    geometry,
                    sample_now,
                    request_frames,
                    window,
                    cx,
                );
                let target_inset = if expanded {
                    px(0.)
                } else {
                    stack_width * (scale_step * rank.min(collapsed_visible - 1) as f32 / 2.)
                };
                let inset = spring_at(
                    (item_id.clone(), "inset"),
                    target_inset,
                    geometry,
                    sample_now,
                    request_frames,
                    window,
                    cx,
                );
                let opacity = spring_at(
                    (item_id.clone(), "visibility"),
                    if expanded || rank < collapsed_visible {
                        1.
                    } else {
                        0.
                    },
                    fade,
                    sample_now,
                    request_frames,
                    window,
                    cx,
                );
                div()
                    .id(item_id.clone())
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .top(offset + if anchored_bottom { center } else { -center })
                    .left(inset)
                    .right(inset)
                    .opacity(opacity)
                    .when(!expanded && rank >= collapsed_visible, |this| {
                        this.invisible()
                    })
                    .on_prepaint(move |bounds, _, cx| {
                        let mut heights = measured.borrow_mut();
                        if measure_items
                            && heights.get(&measured_id).copied() != Some(bounds.size.height)
                        {
                            heights.insert(measured_id.clone(), bounds.size.height);
                            cx.refresh_windows();
                        }
                    })
                    .child(child)
            });

        let hovered_state = self.state.hovered.clone();
        let measured_width = self.state.width.clone();
        self.base
            .relative()
            .h(stack_height)
            .when_some(self.focus_handle, |this, handle| this.track_focus(&handle))
            .child(
                canvas(
                    |_, _, _| {},
                    move |mut bounds, _, window, cx| {
                        bounds.origin.y += if anchored_bottom { center } else { -center };
                        if measured_width.replace(bounds.size.width) != bounds.size.width {
                            cx.refresh_windows();
                        }
                        if !expanded {
                            if anchored_bottom {
                                bounds.origin.y += stack_height - collapsed_height;
                            }
                            bounds.size.height = collapsed_height;
                        }
                        let hovered_state = hovered_state.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, _, _, cx| {
                            let hovered = bounds.contains(&event.position);
                            if hovered_state.replace(hovered) != hovered {
                                cx.refresh_windows();
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .children(items)
            .refine_style(&self.style)
    }
}

/// The lifecycle phase exposed by a toast root to application-owned presentation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToastTransitionStatus {
    /// The toast has just been added and may run its enter transition.
    #[default]
    Starting,
    /// The toast is fully present.
    Present,
    /// The toast is closing and remains mounted until its exit transition completes.
    Ending,
}

/// An unstyled semantic toast root. Applications own all presentation and motion.
#[derive(IntoElement)]
pub struct Toast {
    base: crate::ObservedElement<Stateful<Div>>,
    style: StyleRefinement,
    transition_status: ToastTransitionStatus,
    semantic_role: Role,
    live: Option<gpui::accesskit::Live>,
    a11y_hidden: bool,
    children: Vec<AnyElement>,
}

impl Toast {
    /// Create an unstyled semantic toast in the starting transition phase.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id).test_support(),
            style: StyleRefinement::default(),
            transition_status: ToastTransitionStatus::Starting,
            semantic_role: Role::Alert,
            live: None,
            a11y_hidden: false,
            children: Vec::new(),
        }
    }

    /// Set the lifecycle phase used by application-owned toast presentation.
    pub fn transition_status(mut self, status: ToastTransitionStatus) -> Self {
        self.transition_status = status;
        self
    }

    pub fn semantic_role(mut self, role: Role) -> Self {
        self.semantic_role = role;
        self
    }
    pub fn live(mut self, value: gpui::accesskit::Live) -> Self {
        self.live = Some(value);
        self
    }
    pub fn a11y_hidden(mut self, value: bool) -> Self {
        self.a11y_hidden = value;
        self
    }

    /// Return the lifecycle phase used by application-owned toast presentation.
    pub fn status(&self) -> ToastTransitionStatus {
        self.transition_status
    }
}

impl Styled for Toast {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Toast {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(children);
    }
}

impl InteractiveElement for Toast {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }

    fn track_focus(mut self, handle: &gpui::FocusHandle) -> Self {
        self.base = self.base.track_focus(handle);
        self
    }
}

impl StatefulInteractiveElement for Toast {}

impl RenderOnce for Toast {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let base = self
            .base
            .role(self.semantic_role)
            .children(self.children)
            .refine_style(&self.style);
        crate::A11yLive::new(base, self.live.unwrap_or(gpui::accesskit::Live::Off))
            .hidden(self.a11y_hidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Element as _, accesskit, point};

    #[test]
    fn in_place_update_retains_order_phase_and_deadline() {
        let start = Instant::now();
        let mut m = ToastManager::new(ToastMotion::sonner())
            .durations(Duration::from_millis(300), Duration::from_millis(200));
        for id in [1, 2] {
            m.push(
                id,
                "old",
                ToastOptions {
                    timeout: Some(Duration::from_secs(4)),
                },
                start,
            );
        }
        m.advance(start + Duration::from_millis(2300), false);
        assert_eq!(m.update(&1, "new", None), Ok("old"));
        assert_eq!(
            m.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            m.reading_time(&1),
            Some((Duration::from_secs(2), Duration::from_secs(4)))
        );
        assert_eq!(
            m.next_deadline(start + Duration::from_millis(2300), false),
            Some(start + Duration::from_millis(4300))
        );
        assert_eq!(
            m.next_deadline(start + Duration::from_millis(2300), true),
            None
        );
        m.dismiss(&1, start + Duration::from_millis(2300));
        assert_eq!(m.update(&1, "revive", None), Err("revive"));
        assert_eq!(
            m.next_deadline(start + Duration::from_millis(2300), true),
            Some(start + Duration::from_millis(2500))
        );
    }
    #[test]
    fn late_wake_consumes_cross_phase_remainder_and_zero_motion_settles() {
        let start = Instant::now();
        let mut m = ToastManager::new(ToastMotion::sonner())
            .durations(Duration::from_millis(300), Duration::from_millis(200));
        m.push(
            1,
            "a",
            ToastOptions {
                timeout: Some(Duration::from_secs(1)),
            },
            start,
        );
        let delta = m.advance(start + Duration::from_secs(2), false);
        assert_eq!(delta.presented, vec![1]);
        assert_eq!(delta.ending, vec![1]);
        assert_eq!(delta.removed, vec![(1, "a")]);
        assert!(m.is_empty());
        m.set_durations(Duration::ZERO, Duration::ZERO);
        m.push(2, "b", ToastOptions::default(), start);
        m.advance(start, false);
        assert_eq!(m.iter().next().unwrap().2, ToastTransitionStatus::Present);
        m.dismiss(&2, start);
        assert_eq!(m.advance(start, true).removed, vec![(2, "b")]);
    }

    #[test]
    fn manager_pauses_timeout_and_removes_only_after_exit() {
        let motion = ToastMotion::sonner();
        let start = Instant::now();
        let mut manager = ToastManager::new(motion);
        manager.push(
            "a",
            1,
            ToastOptions {
                timeout: Some(Duration::from_secs(5)),
            },
            start,
        );
        manager.advance(start + motion.duration, false);
        manager.advance(start + motion.duration + Duration::from_secs(4), true);
        assert_eq!(
            manager.iter().next().unwrap().2,
            ToastTransitionStatus::Present
        );
        let ending = manager.advance(start + motion.duration + Duration::from_secs(9), false);
        assert_eq!(ending.ending, vec!["a"]);
        assert!(ending.removed.is_empty());
        let removed = manager.advance(
            start + motion.duration + motion.exit_duration + Duration::from_secs(9),
            false,
        );
        assert_eq!(removed.removed, vec![("a", 1)]);
    }

    #[test]
    fn manager_limit_keeps_ending_toasts_mounted() {
        let now = Instant::now();
        let mut manager = ToastManager::new(ToastMotion::sonner());
        for id in ["a", "b", "c"] {
            manager.push(id, id, ToastOptions::default(), now);
        }
        manager.dismiss(&"a", now);
        assert_eq!(
            manager.visible(1).map(|(id, _, _)| *id).collect::<Vec<_>>(),
            vec!["a", "c"]
        );
    }

    #[test]
    fn manager_replaces_duplicate_ids_as_the_newest_toast() {
        let now = Instant::now();
        let mut manager = ToastManager::new(ToastMotion::sonner());
        manager.push("a", 1, ToastOptions::default(), now);
        manager.push("b", 2, ToastOptions::default(), now);
        assert_eq!(manager.push("a", 3, ToastOptions::default(), now), Some(1));
        assert_eq!(
            manager
                .iter()
                .map(|(id, value, _)| (*id, *value))
                .collect::<Vec<_>>(),
            vec![("b", 2), ("a", 3)]
        );
    }

    #[test]
    fn manager_resets_clock_when_reused_after_becoming_empty() {
        let motion = ToastMotion::sonner();
        let start = Instant::now();
        let mut manager = ToastManager::new(motion);
        manager.push("old", 1, ToastOptions::default(), start);
        manager.dismiss(&"old", start);
        manager.advance(start + motion.exit_duration, false);
        let later = start + Duration::from_secs(60);
        manager.push("new", 2, ToastOptions::default(), later);
        manager.advance(later + Duration::from_millis(50), false);
        assert_eq!(
            manager.iter().next().unwrap().2,
            ToastTransitionStatus::Starting
        );
    }

    #[test]
    fn newly_pushed_toast_does_not_inherit_existing_entry_clock() {
        let motion = ToastMotion::sonner();
        let start = Instant::now();
        let mut manager = ToastManager::new(motion);
        manager.push("old", 1, ToastOptions::default(), start);
        let later = start + Duration::from_secs(60);
        manager.push("new", 2, ToastOptions::default(), later);
        manager.advance(later + Duration::from_millis(50), false);
        assert_eq!(
            manager
                .iter()
                .find(|(id, _, _)| **id == "new")
                .map(|(_, _, status)| status),
            Some(ToastTransitionStatus::Starting)
        );
    }

    #[test]
    fn stack_geometry_anchors_newest_item_and_supports_variable_heights() {
        let heights = [px(40.), px(60.), px(80.)];
        let (collapsed, expanded, top) = stack_geometry(&heights, px(12.), px(12.), false);
        assert_eq!(collapsed, px(104.));
        assert_eq!(expanded, px(204.));
        assert_eq!(
            top,
            vec![(px(24.), px(164.)), (px(12.), px(92.)), (px(0.), px(0.))]
        );

        let (_, _, bottom) = stack_geometry(&heights, px(12.), px(12.), true);
        assert_eq!(
            bottom,
            vec![(px(40.), px(0.)), (px(32.), px(52.)), (px(24.), px(124.))]
        );

        let tall_behind = [px(180.), px(60.)];
        let (collapsed, _, top) = stack_geometry(&tall_behind, px(14.), px(14.), false);
        assert_eq!(collapsed, px(194.));
        assert_eq!(top[0].0, px(14.));
        assert!(top[0].0 + tall_behind[0] <= collapsed);
        assert_eq!(top[0].0 - top[1].0, px(14.));

        let (_, _, bottom) = stack_geometry(&tall_behind, px(14.), px(14.), true);
        assert!(bottom[0].0 >= px(0.));
        assert!(bottom[0].0 + tall_behind[0] <= collapsed);
        assert_eq!(
            (bottom[1].0 + tall_behind[1]) - (bottom[0].0 + tall_behind[0]),
            px(14.)
        );
    }

    #[gpui::test]
    fn toast_exposes_alert_semantics(cx: &mut gpui::TestAppContext) {
        let window = cx.add_empty_window();
        window.update(|window, cx| {
            let mut node = accesskit::Node::new(Role::Alert);
            Toast::new("toast")
                .render(window, cx)
                .into_element()
                .write_a11y_info(&mut node);
            assert_eq!(node.role(), Role::Alert);
        });
    }

    #[gpui::test]
    fn optional_front_shell_does_not_change_natural_measurements(cx: &mut gpui::TestAppContext) {
        struct Harness {
            state: ToastStackState,
        }
        impl gpui::Render for Harness {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                ToastStack::new("front-shell", self.state.clone())
                    .front_shell(true)
                    .measure_items(false)
                    .w(px(300.))
                    .debug_selector(|| "front-shell".into())
                    .item("tall", div().h(px(180.)))
                    .item("front", div().h(px(40.)))
            }
        }
        let state = ToastStackState::default();
        state.set_measurement("tall", px(180.), px(300.));
        state.set_measurement("front", px(40.), px(300.));
        let (_, cx) = cx.add_window_view(|_, _| Harness {
            state: state.clone(),
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("front-shell").unwrap().size.height, px(54.));
        assert_eq!(state.measured_height(&"tall".into()), Some(px(180.)));
    }

    #[gpui::test]
    fn stack_bootstraps_measurement_without_zero_height_clipping(cx: &mut gpui::TestAppContext) {
        struct Harness {
            state: ToastStackState,
        }
        impl gpui::Render for Harness {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                ToastStack::new("stack", self.state.clone())
                    .w(px(300.))
                    .item("first", div().h(px(80.)).child("Visible"))
            }
        }

        let (view, cx) = cx.add_window_view(|_, _| Harness {
            state: ToastStackState::default(),
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.state
                    .heights
                    .borrow()
                    .get(&ElementId::from("first"))
                    .copied(),
                Some(px(80.))
            );
        });
    }

    #[gpui::test]
    fn stack_expands_for_hover_and_focus(cx: &mut gpui::TestAppContext) {
        struct Harness {
            state: ToastStackState,
            focus: FocusHandle,
        }
        impl gpui::Render for Harness {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                ToastStack::new("stack", self.state.clone())
                    .focus_handle(self.focus.clone())
                    .w(px(300.))
                    .item("first", div().h(px(80.)))
            }
        }

        let (view, cx) = cx.add_window_view(|_, cx| Harness {
            state: ToastStackState::default(),
            focus: cx.focus_handle().tab_stop(true),
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        cx.simulate_mouse_move(point(px(10.), px(10.)), None, gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(cx, |view, _| assert!(view.state.is_expanded()));

        cx.simulate_mouse_move(point(px(400.), px(400.)), None, gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(cx, |view, _| assert!(!view.state.is_expanded()));

        cx.simulate_mouse_move(point(px(10.), px(10.)), None, gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(cx, |view, _| assert!(view.state.is_expanded()));

        cx.simulate_mouse_move(point(px(400.), px(400.)), None, gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(cx, |view, _| assert!(!view.state.is_expanded()));

        let focus = view.read_with(cx, |view, _| view.focus.clone());
        cx.update(|window, cx| {
            focus.focus(window, cx);
            window.draw(cx).clear(cx);
        });
        view.read_with(cx, |view, _| assert!(view.state.is_expanded()));
    }

    #[gpui::test]
    fn keyed_stack_reflow_moves_from_the_current_visual_position(cx: &mut gpui::TestAppContext) {
        struct Harness {
            state: ToastStackState,
            show_second: bool,
        }
        impl gpui::Render for Harness {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                ToastStack::new("stack", self.state.clone())
                    .w(px(300.))
                    .item(
                        "first",
                        div().debug_selector(|| "first-toast".into()).h(px(40.)),
                    )
                    .when(self.show_second, |stack| {
                        stack.item(
                            "second",
                            div().debug_selector(|| "second-toast".into()).h(px(80.)),
                        )
                    })
            }
        }

        let (view, cx) = cx.add_window_view(|_, _| Harness {
            state: ToastStackState::default(),
            show_second: false,
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.draw(cx).clear(cx);
        });
        let initial_y = cx.debug_bounds("first-toast").unwrap().origin.y;

        view.update(cx, |view, cx| {
            view.show_second = true;
            cx.notify();
        });
        cx.run_until_parked();
        let first_reflow_y = cx.debug_bounds("first-toast").unwrap().origin.y;
        assert_eq!(first_reflow_y, initial_y);

        cx.executor().advance_clock(Duration::from_millis(200));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let middle_y = cx.debug_bounds("first-toast").unwrap().origin.y;
        assert!(middle_y > initial_y);
        assert!(middle_y < initial_y + px(14.));

        cx.executor().advance_clock(Duration::from_millis(200));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("first-toast").unwrap().origin.y,
            initial_y + px(14.)
        );
        assert!(
            cx.debug_bounds("first-toast").unwrap().size.width
                < cx.debug_bounds("second-toast").unwrap().size.width
        );
    }

    #[gpui::test]
    fn bottom_stack_reflow_moves_up_without_an_opposite_direction_jump(
        cx: &mut gpui::TestAppContext,
    ) {
        struct Harness {
            state: ToastStackState,
            show_second: bool,
        }
        impl gpui::Render for Harness {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                div().relative().size(px(300.)).child(
                    ToastStack::new("bottom-stack", self.state.clone())
                        .placement(Anchor::BottomRight)
                        .absolute()
                        .bottom_0()
                        .w(px(300.))
                        .item(
                            "bottom-first",
                            div()
                                .debug_selector(|| "bottom-first-toast".into())
                                .h(px(40.)),
                        )
                        .when(self.show_second, |stack| {
                            stack.item("bottom-second", div().h(px(80.)))
                        }),
                )
            }
        }

        let (view, cx) = cx.add_window_view(|_, _| Harness {
            state: ToastStackState::default(),
            show_second: false,
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.draw(cx).clear(cx);
        });
        // A bottom-anchored item's position is composed from two springs — the
        // stack height and the item's own offset — and the stack height only
        // acquires its real target once the toast has been measured in prepaint.
        // Both the baseline and the final reading are taken settled, so neither
        // carries the fraction of a pixel that separates them mid-flight.
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let initial_y = cx.debug_bounds("bottom-first-toast").unwrap().origin.y;

        view.update(cx, |view, cx| {
            view.show_second = true;
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(
            cx.debug_bounds("bottom-first-toast").unwrap().origin.y,
            initial_y
        );

        cx.executor().advance_clock(Duration::from_millis(200));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let middle_y = cx.debug_bounds("bottom-first-toast").unwrap().origin.y;
        assert!(middle_y < initial_y);
        assert!(
            middle_y >= initial_y - px(14.),
            "middle={middle_y:?}, initial={initial_y:?}"
        );

        cx.executor().advance_clock(Duration::from_secs(1));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("bottom-first-toast").unwrap().origin.y,
            initial_y - px(14.)
        );
    }
}

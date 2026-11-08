use super::*;
use gpui::{Context, Task};
use std::time::Instant;

const IDLE_DELAY: Duration = Duration::from_millis(120);

// Allocated only after input; no per-item tasks or default idle allocation.
#[derive(Default)]
pub(super) struct MasonryActivity {
    scrolling: bool,
    direction: Option<VirtualizerScrollDirection>,
    last_at: Option<Instant>,
    pending: bool,
    pending_direction: Option<VirtualizerScrollDirection>,
    idle: Option<Task<()>>,
}
impl VirtualMasonryState {
    pub fn is_scrolling(&self) -> bool {
        self.activity.as_ref().is_some_and(|a| a.scrolling)
    }
    pub fn scroll_direction(&self) -> Option<VirtualizerScrollDirection> {
        self.activity.as_ref().and_then(|a| a.direction)
    }
    pub(super) fn record_input_direction(&mut self, direction: Option<VirtualizerScrollDirection>) {
        let activity = self.activity.get_or_insert_with(|| Box::default());
        activity.pending = true;
        if direction.is_some() {
            activity.pending_direction = direction;
        }
    }
    /// Sample the native owner using the caller's clock, without writing an offset.
    /// Pure consumers poll idle themselves; the native adapter arms one idle task.
    pub fn observe_scroll_activity(&mut self, now: Instant) {
        self.observe_input();
        if let Some(activity) = &mut self.activity
            && activity.pending
        {
            let direction = activity.pending_direction.take().or(activity.direction);
            if !activity.scrolling || activity.direction != direction {
                self.events.push(VirtualizerEvent::ScrollStateChanged {
                    is_scrolling: true,
                    direction,
                });
            }
            activity.scrolling = true;
            activity.direction = direction;
            activity.last_at = Some(now);
            activity.pending = false;
        }
        self.poll_scroll_idle(now);
    }
    pub fn poll_scroll_idle(&mut self, now: Instant) {
        if self.activity.as_ref().is_some_and(|a| {
            !a.pending
                && a.last_at
                    .is_some_and(|last| now.saturating_duration_since(last) >= IDLE_DELAY)
        }) {
            self.end_scrolling();
        }
    }
    pub(super) fn end_scrolling(&mut self) {
        if self.activity.take().is_some_and(|a| a.scrolling) {
            self.events.push(VirtualizerEvent::ScrollStateChanged {
                is_scrolling: false,
                direction: None,
            });
            self.events.push(VirtualizerEvent::ScrollEnd);
        }
    }
    pub fn arm_scroll_idle(&mut self, cx: &mut Context<Self>) {
        if !self
            .activity
            .as_ref()
            .is_some_and(|a| a.scrolling && a.idle.is_none())
        {
            return;
        }
        let task = cx.spawn(async move |state, cx| {
            let mut delay = IDLE_DELAY;
            loop {
                cx.background_executor().timer(delay).await;
                let next = state
                    .update(cx, |state, cx| {
                        let now = cx.background_executor().now();
                        if let Some(activity) = &state.activity {
                            if activity.pending {
                                return Some(IDLE_DELAY);
                            }
                            if let Some(last) = activity.last_at {
                                let elapsed = now.saturating_duration_since(last);
                                if elapsed < IDLE_DELAY {
                                    return Some(IDLE_DELAY - elapsed);
                                }
                            }
                        } else {
                            return None;
                        }
                        state.end_scrolling();
                        state.emit_events(cx);
                        cx.notify();
                        None
                    })
                    .ok()
                    .flatten();
                let Some(next) = next else {
                    break;
                };
                delay = next;
            }
        });
        if let Some(activity) = &mut self.activity {
            activity.idle = Some(task);
        }
    }
}

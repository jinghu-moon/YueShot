//! A measured, viewport-constrained translation with an explicit drag handle.
//! The consumer owns presentation and enables the handle only when appropriate.
use gpui::{
    AnyElement, Bounds, DispatchPhase, ElementId, InteractiveElement as _, IntoElement,
    MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Styled as _,
    canvas, div, point,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct State {
    offset: Point<Pixels>,
    limits: Option<(Point<Pixels>, Point<Pixels>)>,
    gesture: Option<(Point<Pixels>, Point<Pixels>)>,
    moving: bool,
}

#[derive(Clone, Default)]
pub struct ConstrainedDrag(Rc<RefCell<State>>);
impl ConstrainedDrag {
    pub fn offset(&self) -> Point<Pixels> {
        self.0.borrow().offset
    }
    pub fn cancel(&self) {
        let mut s = self.0.borrow_mut();
        s.gesture = None;
        s.moving = false;
    }
    pub fn reset(&self) {
        *self.0.borrow_mut() = State::default();
    }
    /// Measure the resting centered surface; translation/motion is subtracted by
    /// the caller so resizing or changing rem cannot accumulate a drifting origin.
    pub fn measure(
        &self,
        resting: Bounds<Pixels>,
        viewport: Bounds<Pixels>,
        margin: Pixels,
    ) -> bool {
        let low = point(
            viewport.left() + margin - resting.left(),
            viewport.top() + margin - resting.top(),
        );
        let high = point(
            (viewport.right() - margin - resting.right()).max(low.x),
            (viewport.bottom() - margin - resting.bottom()).max(low.y),
        );
        let mut state = self.0.borrow_mut();
        state.limits = Some((low, high));
        let next = clamp(state.offset, low, high);
        let changed = next != state.offset;
        state.offset = next;
        changed
    }
    /// Installs global capture listeners without a full-window hitbox. The same
    /// handle must also be rendered by `handle`, which alone begins a gesture.
    pub fn capture(&self) -> AnyElement {
        let drag = self.clone();
        canvas(
            |_, _, _| (),
            move |_, _, w, _| {
                let moving = drag.clone();
                w.on_mouse_event(move |event: &MouseMoveEvent, phase, w, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    if !w.is_window_active() || event.pressed_button != Some(MouseButton::Left) {
                        moving.cancel();
                        return;
                    }
                    let mut s = moving.0.borrow_mut();
                    if let (Some((start, offset)), Some((low, high))) = (s.gesture, s.limits) {
                        // Match GPUI's own pointer drag threshold, in logical pixels.
                        if !s.moving && (event.position - start).magnitude() <= 2. {
                            return;
                        }
                        s.moving = true;
                        s.offset = clamp(offset + (event.position - start), low, high);
                        w.refresh();
                        cx.stop_propagation();
                    }
                });
                let released = drag.clone();
                w.on_mouse_event(move |event: &MouseUpEvent, phase, _, _| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        released.cancel();
                    }
                });
            },
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }
    /// This region is non-focusable. Put buttons outside it; arbitrary children
    /// are never searched for possible handles.
    pub fn handle(&self, id: impl Into<ElementId>, content: impl IntoElement) -> AnyElement {
        let drag = self.clone();
        div()
            .id(id)
            .min_w_0()
            .flex_1()
            .cursor_move()
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                let mut s = drag.0.borrow_mut();
                if s.limits.is_some() {
                    s.gesture = Some((event.position, s.offset));
                    s.moving = false;
                }
                cx.stop_propagation();
            })
            .child(content)
            .into_any_element()
    }
}
fn clamp(value: Point<Pixels>, low: Point<Pixels>, high: Point<Pixels>) -> Point<Pixels> {
    point(value.x.clamp(low.x, high.x), value.y.clamp(low.y, high.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};
    #[test]
    fn resizing_reconstrains_and_reset_discards_a_gesture() {
        let drag = ConstrainedDrag::default();
        let resting = Bounds::new(point(px(100.), px(100.)), size(px(200.), px(100.)));
        let viewport = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(300.)));
        drag.0.borrow_mut().offset = point(px(1000.), px(-1000.));
        assert!(drag.measure(resting, viewport, px(16.)));
        assert_eq!(drag.offset(), point(px(84.), px(-84.)));
        assert!(!drag.measure(resting, viewport, px(16.)));
        drag.reset();
        assert_eq!(drag.offset(), Point::default());
    }
}

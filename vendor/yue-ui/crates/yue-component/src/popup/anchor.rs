use gpui_kit::{
    AnyElement, Bounds, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point,
    Styled as _, canvas, div,
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AnchorSnapshot {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) clip: Bounds<Pixels>,
    pub(super) revision: u64,
}

#[derive(Default)]
struct AnchorState {
    measured: Option<AnchorSnapshot>,
    mount: Weak<()>,
    point: bool,
}

/// Retain this with the trigger. The capture element, rather than the handle,
/// owns the measurement's mount lease, including when GPUI reuses a cached tree.
#[derive(Clone, Default)]
pub(crate) struct PopupAnchor(Rc<RefCell<AnchorState>>);

#[derive(Clone)]
pub(crate) struct PopupAnchorHandle(Weak<RefCell<AnchorState>>);

impl PopupAnchor {
    pub(crate) fn handle(&self) -> PopupAnchorHandle {
        PopupAnchorHandle(Rc::downgrade(&self.0))
    }

    pub(crate) fn set_point(&self, position: Point<Pixels>, clip: Bounds<Pixels>) {
        let mut state = self.0.borrow_mut();
        let bounds = Bounds::new(position, Default::default());
        let revision = state.measured.map_or(1, |previous| {
            previous.revision + u64::from(previous.bounds != bounds || previous.clip != clip)
        });
        state.point = true;
        state.measured = Some(AnchorSnapshot {
            bounds,
            clip,
            revision,
        });
    }

    /// Attach as an absolute, inset-zero child of the actual trigger/row.
    pub(crate) fn capture(&self) -> AnyElement {
        let state = self.0.clone();
        let capture = canvas(
            move |bounds, window, cx| {
                // The rendered frame (and cached prepaint ranges) retain keyed
                // state. A transient canvas closure alone dies after paint.
                let lease = window
                    .use_keyed_state("popup-anchor-lease", cx, |window, cx| {
                        let handle = window.window_handle();
                        cx.on_release(move |_, cx| {
                            // Only the source window needs one invalidation
                            // draw when a retained measurement is unmounted.
                            let _ = handle.update(cx, |_, window, _| window.refresh());
                        })
                        .detach();
                        Rc::new(())
                    })
                    .read(cx)
                    .clone();
                let clip = window.content_mask().bounds;
                let mut state = state.borrow_mut();
                let revision = state.measured.map_or(1, |previous| {
                    previous.revision
                        + u64::from(previous.bounds != bounds || previous.clip != clip)
                });
                state.point = false;
                state.mount = Rc::downgrade(&lease);
                state.measured = Some(AnchorSnapshot {
                    bounds,
                    clip,
                    revision,
                });
                drop(state);
                lease
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        div()
            .id("popup-anchor-capture")
            .absolute()
            .inset_0()
            .child(capture)
            .into_any_element()
    }
}

impl PopupAnchorHandle {
    pub(super) fn snapshot(&self) -> Option<AnchorSnapshot> {
        let state = self.0.upgrade()?;
        let state = state.borrow();
        let snapshot = state.measured?;
        if !super::geometry::finite(snapshot.bounds) || !super::geometry::finite(snapshot.clip) {
            return None;
        }
        if state.point {
            return snapshot
                .clip
                .contains(&snapshot.bounds.origin)
                .then_some(snapshot);
        }
        state.mount.upgrade()?;
        super::geometry::overlaps(snapshot.bounds, snapshot.clip).then_some(snapshot)
    }

    pub(crate) fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.snapshot().map(|snapshot| snapshot.bounds)
    }
}

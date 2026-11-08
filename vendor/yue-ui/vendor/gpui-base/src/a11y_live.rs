//! Native live semantics without adding layout, focus or input nodes.
use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, InteractiveElement,
    Interactivity, IntoElement, LayoutId, Pixels, Window, accesskit,
};

/// Native live-region properties on the wrapped semantic node.
pub struct A11yLive<E> {
    inner: E,
    live: accesskit::Live,
    hidden: bool,
    atomic: bool,
    busy: Option<bool>,
    written: Option<std::rc::Rc<dyn Fn()>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{StatefulInteractiveElement as _, div};
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn native_live_node_preserves_label_value_hidden_and_atomic() {
        let delivered = Rc::new(Cell::new(false));
        let mark = delivered.clone();
        let element = A11yLive::new(
            div()
                .id("live")
                .role(gpui::Role::Status)
                .aria_label("Saved")
                .aria_value("Saved"),
            accesskit::Live::Polite,
        )
        .atomic(true)
        .hidden(true)
        .on_written(move || mark.set(true));
        assert!(!delivered.get());
        let mut node = accesskit::Node::new(element.a11y_role().unwrap());
        element.write_a11y_info(&mut node);
        assert_eq!(node.label(), Some("Saved"));
        assert_eq!(node.value(), Some("Saved"));
        assert_eq!(node.live(), Some(accesskit::Live::Polite));
        assert!(node.is_live_atomic());
        assert!(node.is_hidden());
        assert!(delivered.get());
    }
}

impl<E> A11yLive<E> {
    pub fn new(inner: E, live: accesskit::Live) -> Self {
        Self {
            inner,
            live,
            hidden: false,
            atomic: false,
            busy: None,
            written: None,
        }
    }
    pub fn hidden(mut self, value: bool) -> Self {
        self.hidden = value;
        self
    }
    pub fn atomic(mut self, value: bool) -> Self {
        self.atomic = value;
        self
    }
    /// Busy on the existing native node, without adding a focus or layout node.
    pub fn busy(mut self, value: bool) -> Self {
        self.busy = Some(value);
        self
    }
    /// Called only when GPUI actually writes this node into an accessibility tree.
    pub fn on_written(mut self, callback: impl Fn() + 'static) -> Self {
        self.written = Some(std::rc::Rc::new(callback));
        self
    }
}

impl<E: Element> IntoElement for A11yLive<E> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<E: InteractiveElement> InteractiveElement for A11yLive<E> {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.inner.interactivity()
    }
}

impl<E: Element> Element for A11yLive<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner
            .prepaint(id, inspector, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, prepaint, window, cx)
    }

    fn a11y_role(&self) -> Option<gpui::Role> {
        self.inner.a11y_role()
    }

    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
        node.set_live(self.live);
        match self.busy {
            Some(true) => node.set_busy(),
            Some(false) => node.clear_busy(),
            None => {}
        }
        if self.hidden {
            node.set_hidden();
        }
        if self.atomic {
            node.set_live_atomic();
        }
        if let Some(written) = &self.written {
            written();
        }
    }

    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut gpui::A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
    }
}

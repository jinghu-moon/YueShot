use super::*;
use crate::popup::{PopupAnchor, PopupAnchorHandle};
use gpui_kit::{
    Axis, Bounds, Context, Div, Element, EntityId, Global, GlobalElementId, InspectorElementId,
    LayoutId, Pixels, StyleRefinement, WindowId,
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Group {
    pub id: EntityId,
    pub axis: Axis,
}
#[derive(Default)]
struct GroupStack(HashMap<WindowId, Vec<Group>>);
impl Global for GroupStack {}
pub(super) fn current(window: &Window, cx: &App) -> Option<Group> {
    cx.try_global::<GroupStack>()?
        .0
        .get(&window.window_handle().window_id())?
        .last()
        .copied()
}

/// Explicit continuity and protection boundary for one toolbar row or column.
#[derive(IntoElement)]
pub struct YueTooltipGroup {
    id: ElementId,
    axis: Axis,
    base: Div,
}
impl YueTooltipGroup {
    pub fn new(id: impl Into<ElementId>, axis: Axis) -> Self {
        Self {
            id: id.into(),
            axis,
            base: div(),
        }
    }
    /// Registers even a disabled button or one without a tooltip for avoidance.
    pub fn protected_child(mut self, id: impl Into<ElementId>, child: impl IntoElement) -> Self {
        self.base = self.base.child(Protected {
            id: id.into(),
            child: child.into_any_element(),
        });
        self
    }
}
impl gpui_kit::ParentElement for YueTooltipGroup {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.base.extend(children);
    }
}
impl gpui_kit::Styled for YueTooltipGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}
impl RenderOnce for YueTooltipGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = window
            .use_keyed_state((self.id.clone(), "tooltip-group"), cx, |_, _| ())
            .entity_id();
        let axis = self.axis;
        let mut base = gpui_kit::InteractiveElement::id(self.base, self.id).flex();
        if axis == Axis::Vertical {
            base = base.flex_col();
        }
        GroupElement {
            group: Group { id, axis },
            child: base.into_any_element(),
        }
    }
}
struct GroupElement {
    group: Group,
    child: AnyElement,
}
impl IntoElement for GroupElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for GroupElement {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        if cx.try_global::<GroupStack>().is_none() {
            cx.set_global(GroupStack::default());
        }
        let id = window.window_handle().window_id();
        cx.global_mut::<GroupStack>()
            .0
            .entry(id)
            .or_default()
            .push(self.group);
        let layout = self.child.request_layout(window, cx);
        let stack = &mut cx.global_mut::<GroupStack>().0;
        stack.get_mut(&id).unwrap().pop();
        if stack.get(&id).is_some_and(Vec::is_empty) {
            stack.remove(&id);
        }
        (layout, ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}
#[derive(IntoElement)]
struct Protected {
    id: ElementId,
    child: AnyElement,
}
pub(super) struct ProtectedState {
    pub anchor: PopupAnchor,
    pub group: Option<Group>,
}
impl RenderOnce for Protected {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let group = current(window, cx);
        let state = window.use_keyed_state(
            (self.id.clone(), "tooltip-protected"),
            cx,
            |_, _: &mut Context<ProtectedState>| ProtectedState {
                anchor: PopupAnchor::default(),
                group,
            },
        );
        state.update(cx, |s, _| s.group = group);
        if let Some(layer) = super::controller::layer(window, cx) {
            layer.update(cx, |layer, _| {
                layer.protected.insert(state.entity_id(), state.downgrade());
            });
        }
        let capture = state.read(cx).anchor.capture();
        gpui_kit::InteractiveElement::id(div(), self.id)
            .relative()
            .flex()
            .flex_shrink_0()
            .child(self.child)
            .child(capture)
    }
}
pub(super) fn bounds(anchor: &PopupAnchorHandle) -> Option<Bounds<Pixels>> {
    anchor
        .bounds()
        .filter(|b| b.size.width > gpui_kit::px(0.) && b.size.height > gpui_kit::px(0.))
}

use super::*;
use crate::virtualization::element::ItemViews;
use crate::{InteractiveElementExt as _, TestSupportExt as _};
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Div, Element, Entity, GlobalElementId,
    Hitbox, InspectorElementId, InteractiveElement, IntoElement, LayoutId, ParentElement, Stateful,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, point, size,
};

type Renderer = Rc<dyn Fn(VirtualMasonryItem, &mut Window, &mut App) -> AnyElement>;
type ViewFactory = Rc<dyn Fn(VirtualizerItemIdentity, &mut Window, &mut App) -> gpui::AnyView>;
pub struct VirtualMasonry {
    id: ElementId,
    state: Entity<VirtualMasonryState>,
    base: Stateful<Div>,
    render: Renderer,
    view_factory: Option<ViewFactory>,
    content_revision: u64,
}
impl VirtualMasonry {
    pub fn new<R: IntoElement>(
        id: impl Into<ElementId>,
        state: &Entity<VirtualMasonryState>,
        render: impl Fn(VirtualMasonryItem, &mut Window, &mut App) -> R + 'static,
    ) -> Self {
        let id = id.into();
        let observed = state.downgrade();
        Self {
            base: div()
                .id(id.clone())
                .size_full()
                .on_scroll_wheel(move |_, _, cx| {
                    let _ = observed.update(cx, |state, cx| {
                        state.user_scroll_activity();
                        state.emit_events(cx);
                    });
                }),
            id,
            state: state.clone(),
            render: Rc::new(move |item, w, cx| render(item, w, cx).into_any_element()),
            view_factory: None,
            content_revision: 0,
        }
    }
    /// Reuse content preparation for the live workset. Geometry remains frame-local.
    /// Factory inputs outside source revisions require content_revision or explicit
    /// content invalidation. Reactive view layout still runs normally each frame.
    pub fn new_with_item_views<V: Into<gpui::AnyView>>(
        id: impl Into<ElementId>,
        state: &Entity<VirtualMasonryState>,
        factory: impl Fn(VirtualizerItemIdentity, &mut Window, &mut App) -> V + 'static,
    ) -> Self {
        let mut list = Self::new(id, state, |_, _, _| div());
        list.view_factory = Some(Rc::new(move |item, w, cx| factory(item, w, cx).into()));
        list
    }
    pub fn content_revision(mut self, revision: u64) -> Self {
        self.content_revision = revision;
        self
    }
}
impl Styled for VirtualMasonry {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}
impl InteractiveElement for VirtualMasonry {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}
impl StatefulInteractiveElement for VirtualMasonry {}
impl IntoElement for VirtualMasonry {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
pub struct MasonryFrame {
    items: Vec<AnyElement>,
}
struct Mount {
    owner: gpui::WeakEntity<VirtualMasonryState>,
    _observe: gpui::Subscription,
    _release: gpui::Subscription,
    views: Rc<std::cell::RefCell<ItemViews>>,
}
impl Element for VirtualMasonry {
    type RequestLayoutState = MasonryFrame;
    type PrepaintState = Option<Hitbox>;
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn a11y_role(&self) -> Option<gpui::Role> {
        self.base.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut gpui::accesskit::Node) {
        self.base.write_a11y_info(node);
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, MasonryFrame) {
        self.state.update(cx, |s, _| s.claim_mount(w, id.unwrap()));
        let state = self.state.clone();
        let mount = w.use_keyed_state(
            (self.id.clone(), "masonry-mount"),
            cx,
            move |_, cx: &mut gpui::Context<Mount>| Mount {
                owner: state.downgrade(),
                views: Rc::default(),
                _observe: cx.observe(&state, |_, _, cx| cx.notify()),
                _release: cx.on_release(|mount: &mut Mount, cx| {
                    let _ = mount.owner.update(cx, |s, cx| {
                        s.release_mount();
                        s.emit_events(cx);
                    });
                }),
            },
        );
        mount.update(cx, |m, cx| {
            if m.owner != self.state {
                let _ = m.owner.update(cx, |s, cx| {
                    s.release_mount();
                    s.emit_events(cx);
                });
                m.owner = self.state.downgrade();
                m.views.borrow_mut().entries.clear();
                m._observe = cx.observe(&self.state, |_, _, cx| cx.notify());
            }
        });
        if let Some(factory) = self.view_factory.clone() {
            let views = mount.read(cx).views.clone();
            let source = self.state.read(cx).source().clone();
            let owner = self.state.clone();
            let content_revision = self.content_revision;
            self.render = Rc::new(move |item, w, cx| {
                let identity = item.item().identity();
                let key = identity.key().clone();
                let stamp = (
                    source.revision_at(identity.index()),
                    content_revision,
                    owner.read(cx).item_content_revision(identity.index()),
                    identity.index(),
                );
                let existing = views
                    .borrow()
                    .entries
                    .get(&key)
                    .filter(|(old, _)| *old == stamp)
                    .map(|(_, view)| view.clone());
                let view = existing.unwrap_or_else(|| {
                    let view = factory(identity, w, cx);
                    views
                        .borrow_mut()
                        .entries
                        .insert(key, (stamp, view.clone()));
                    view
                });
                view.into_any_element()
            });
        } else {
            mount.read(cx).views.borrow_mut().entries.clear();
        }
        let scroll = self.state.read(cx).scroll_handle().clone();
        self.base = std::mem::replace(&mut self.base, div().id(self.id.clone()))
            .overflow_y_scroll()
            .lock_scroll_axis()
            .track_scroll(&scroll);
        let layout =
            self.base
                .interactivity()
                .request_layout(id, inspector, w, cx, |style, w, cx| {
                    w.request_layout(style, None, cx)
                });
        (layout, MasonryFrame { items: Vec::new() })
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut MasonryFrame,
        w: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        let clip = bounds.intersect(&w.content_mask().bounds);
        let visible = clip.size.width > px(0.)
            && clip.size.height > px(0.)
            && w.is_visible()
            && w.is_window_active();
        let now = cx.background_executor().now();
        self.state.update(cx, |s, cx| {
            if !visible || cx.reduce_motion() {
                s.settle_motion();
            }
            let old_activity = (s.is_scrolling(), s.scroll_direction());
            s.observe_scroll_activity(now);
            if old_activity != (s.is_scrolling(), s.scroll_direction()) {
                cx.notify();
            }
            s.observe_viewport(bounds.size);
            s.begin_command_frame(now);
        });
        let dynamic = self.state.read(cx).is_dynamic();
        let budget = self.state.read(cx).budget();
        let focus = self.state.update(cx, |s, cx| s.focus_keys(w, cx));
        let mut built = std::collections::HashMap::new();
        let mut offset;
        let mut snapshot;
        let mut exhausted = false;
        let mut invalid = Vec::new();
        let mut initial = true;
        let mut corrections = 0;
        let mut progressed = false;
        loop {
            offset = self.state.update(cx, |s, _| s.resolve_offset());
            snapshot = self.state.read(cx).snapshot(offset, bounds.size).unwrap();
            let mut retained = snapshot.retained.to_vec();
            retained.extend(focus.clone());
            snapshot.retained = retained.into();
            let mut updates = Vec::new();
            let indexes = snapshot.working_indices();
            let mut complete = true;
            for ix in indexes {
                if built.contains_key(&ix) {
                    continue;
                }
                // Required presentation is never truncated by a measurement
                // budget. Only extra intrinsic roots exposed by correction use it.
                if !initial && corrections >= budget {
                    exhausted = true;
                    complete = false;
                    break;
                }
                if !initial {
                    corrections += 1;
                }
                let item = snapshot.item(ix).unwrap();
                let key = item.item().key().clone();
                let width = item.cross_extent();
                let height = item.item().extent();
                let was_measured = item.item().is_measured();
                let row = div()
                    .id(key.clone())
                    .w(width)
                    .child((self.render)(item, w, cx));
                let mut element = if dynamic { row } else { row.h(height) }
                    .test_support()
                    .into_any_element();
                let measured = element.layout_as_root(
                    size(
                        AvailableSpace::Definite(width),
                        if dynamic {
                            AvailableSpace::MinContent
                        } else {
                            AvailableSpace::Definite(height)
                        },
                    ),
                    w,
                    cx,
                );
                if dynamic && valid_positive(measured.height) {
                    if !was_measured || measured.height != height {
                        updates.push(VirtualizerSizeUpdate::new(key, measured.height));
                    }
                } else if dynamic {
                    invalid.push(key);
                }
                built.insert(ix, element);
            }
            if !updates.is_empty() {
                progressed |= self.state.update(cx, |s, _| {
                    let before = s.revision;
                    s.update_sizes(&updates).unwrap();
                    s.revision != before
                });
            }
            if !dynamic || !complete || updates.is_empty() {
                break;
            }
            initial = false;
            // Re-query after actual heights; additional intrinsic work is bounded.
        }
        offset = self.state.update(cx, |s, _| s.resolve_offset());
        snapshot = self.state.read(cx).snapshot(offset, bounds.size).unwrap();
        let mut retained = snapshot.retained.to_vec();
        retained.extend(focus);
        snapshot.retained = retained.into();
        // One immutable final-geometry workset serves fallback layout, prepaint
        // and anchor commit. Never reuse it across a geometry revision.
        let items: Vec<_> = snapshot
            .working_indices()
            .into_iter()
            .map(|ix| snapshot.item(ix).unwrap())
            .collect();
        let mut deferred = false;
        for item in &items {
            let ix = item.item().index();
            if built.contains_key(&ix) {
                continue;
            }
            // Budget exhaustion leaves estimated slots, never missing cards.
            // Clip their content to the estimate until next frame measures it.
            let width = item.cross_extent();
            let height = item.item().extent();
            let mut element = div()
                .id(item.item().key().clone())
                .w(width)
                .h(height)
                .overflow_hidden()
                .child((self.render)(item.clone(), w, cx))
                .test_support()
                .into_any_element();
            element.layout_as_root(
                size(
                    AvailableSpace::Definite(width),
                    AvailableSpace::Definite(height),
                ),
                w,
                cx,
            );
            built.insert(ix, element);
            deferred = true;
        }
        let content_size = size(bounds.size.width, Pixels::from(snapshot.total_extent()));
        let hitbox = self.base.interactivity().prepaint(
            id,
            inspector,
            bounds,
            content_size,
            w,
            cx,
            |_, _, hitbox, w, cx| {
                w.with_content_mask(Some(ContentMask { bounds }), |w| {
                    for item in &items {
                        if let Some(mut element) = built.remove(&item.item().index()) {
                            element.prepaint_at(
                                bounds.origin
                                    + point(
                                        item.cross_start(),
                                        Pixels::from(item.item().start() - offset),
                                    ),
                                w,
                                cx,
                            );
                            frame.items.push(element);
                        }
                    }
                });
                hitbox
            },
        );
        if self.view_factory.is_some() {
            let mount: Entity<Mount> =
                w.use_keyed_state((self.id.clone(), "masonry-mount"), cx, |_, _| {
                    unreachable!("mount exists")
                });
            let views = mount.read(cx).views.clone();
            let mut cache = views.borrow_mut();
            let mut touched = std::mem::take(&mut cache.touched);
            touched.clear();
            touched.extend(items.iter().map(|item| item.item().key().clone()));
            cache.entries.retain(|key, _| touched.contains(key));
            cache.touched = touched;
        }
        self.state
            .update(cx, |s, _| s.commit_anchor_items(snapshot.offset(), items));
        self.state.update(cx, |s, cx| {
            if !invalid.is_empty() {
                s.invalidate_items(&invalid).unwrap();
                if !s.invalid_reported {
                    cx.emit(VirtualizerEvent::Diagnostic(
                        "invalid masonry extent; using estimate",
                    ));
                }
            }
            s.invalid_reported = !invalid.is_empty();
            if exhausted && !s.budget_reported {
                cx.emit(VirtualizerEvent::Diagnostic(
                    "masonry measurement budget exhausted",
                ));
            }
            s.budget_reported = exhausted;
            s.settle_follow_tail_input();
            s.confirm_command(!deferred && !exhausted);
            s.arm_scroll_idle(cx);
            s.emit_events(cx);
        });
        if deferred && progressed || visible && self.state.read(cx).has_scroll_command() {
            // Geometry continuation requires progress. Active commands are
            // separately bounded by shared stall/attempt/time limits.
            w.request_animation_frame();
        }
        hitbox
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut MasonryFrame,
        hitbox: &mut Option<Hitbox>,
        w: &mut Window,
        cx: &mut App,
    ) {
        self.base.interactivity().paint(
            id,
            inspector,
            bounds,
            hitbox.as_ref(),
            w,
            cx,
            |_, w, cx| {
                w.with_content_mask(Some(ContentMask { bounds }), |w| {
                    for element in &mut frame.items {
                        element.paint(w, cx);
                    }
                });
            },
        );
    }
}

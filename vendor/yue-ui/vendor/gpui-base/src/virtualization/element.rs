use super::*;
use crate::{InteractiveElementExt as _, TestSupportExt as _};
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Context, Div, Element, Entity,
    GlobalElementId, Hitbox, InspectorElementId, InteractiveElement, IntoElement, LayoutId,
    ListViewportLayout, ParentElement, Role, Stateful, StatefulInteractiveElement, StyleRefinement,
    Styled, Subscription, Window, div, point, px, size,
};

type Renderer = Rc<dyn Fn(VirtualizerItem, &mut Window, &mut App) -> AnyElement>;
type ViewFactory = Rc<dyn Fn(VirtualizerItemIdentity, &mut Window, &mut App) -> gpui::AnyView>;
pub(super) type ViewStamp = (u64, u64, u64, usize);
#[derive(Default)]
pub(super) struct ItemViews {
    pub(super) entries: std::collections::HashMap<ElementId, (ViewStamp, gpui::AnyView)>,
    pub(super) touched: std::collections::HashSet<ElementId>,
}

/// Unstyled one-dimensional presentation of the shared virtualizer state.
/// Raw state and this adapter add no default accessibility role. Consumers own
/// list/listbox/grid semantics and keyboard/selection policy.
pub struct VirtualList {
    id: ElementId,
    state: Entity<VirtualizerState>,
    base: Stateful<Div>,
    render: Renderer,
    item_role: Option<Role>,
    view_factory: Option<ViewFactory>,
    content_revision: u64,
}
impl VirtualList {
    pub fn new<R: IntoElement>(
        id: impl Into<ElementId>,
        state: &Entity<VirtualizerState>,
        render: impl Fn(VirtualizerItem, &mut Window, &mut App) -> R + 'static,
    ) -> Self {
        let id = id.into();
        let observed = state.downgrade();
        let base = div()
            .id(id.clone())
            .size_full()
            .on_scroll_wheel(move |_, _, cx| {
                let _ = observed.update(cx, |state, cx| state.user_scroll_activity(cx));
            });
        Self {
            base,
            id,
            state: state.clone(),
            render: Rc::new(move |item, w, cx| render(item, w, cx).into_any_element()),
            item_role: None,
            view_factory: None,
            content_revision: 0,
        }
    }
    /// Opt-in bounded preparation reuse. Only live working-item entities survive;
    /// their elements still follow GPUI's ordinary frame/layout/event lifecycle.
    /// The factory sees source identity only. Frame geometry, theme and other
    /// reactive dependencies belong in Render; explicit preparation dependencies
    /// outside the source require content_revision or invalidate_items.
    pub fn new_with_item_views<V: Into<gpui::AnyView>>(
        id: impl Into<ElementId>,
        state: &Entity<VirtualizerState>,
        factory: impl Fn(VirtualizerItemIdentity, &mut Window, &mut App) -> V + 'static,
    ) -> Self {
        let mut list = Self::new(id, state, |_, _, _| div());
        list.view_factory = Some(Rc::new(move |item, w, cx| factory(item, w, cx).into()));
        list
    }
    /// Bump for factory inputs outside source content revisions. Updating
    /// a retained child entity itself uses its ordinary Context::notify contract.
    pub fn content_revision(mut self, revision: u64) -> Self {
        self.content_revision = revision;
        self
    }
    pub fn role(mut self, role: Role) -> Self {
        self.base = self.base.role(role);
        self
    }
    pub fn item_role(mut self, role: Option<Role>) -> Self {
        self.item_role = role;
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<gpui::SharedString>) -> Self {
        self.base = self.base.aria_label(label);
        self
    }
}
impl Styled for VirtualList {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}
impl InteractiveElement for VirtualList {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}
impl StatefulInteractiveElement for VirtualList {}
impl IntoElement for VirtualList {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

pub struct VirtualListFrame {
    items: Vec<AnyElement>,
    measured: Option<ListViewportLayout>,
    native: Option<gpui::ListState>,
}
struct Mount {
    owner: gpui::WeakEntity<VirtualizerState>,
    revision: u64,
    _observe: Subscription,
    _release: Subscription,
    views: Rc<std::cell::RefCell<ItemViews>>,
}
impl Element for VirtualList {
    type RequestLayoutState = VirtualListFrame;
    type PrepaintState = Option<Hitbox>;
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn a11y_role(&self) -> Option<Role> {
        self.base.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut gpui::accesskit::Node) {
        self.base.write_a11y_info(node);
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, VirtualListFrame) {
        let state = self.state.clone();
        self.state
            .update(cx, |state, _| {
                state.claim_mount(
                    window.window_handle().window_id(),
                    id.expect("VirtualList has an identity"),
                )
            })
            .expect("one VirtualList mount per virtualizer state");
        let revision = state.read(cx).binding_revision();
        let initial_state = state.clone();
        let mount = window.use_keyed_state(
            (self.id.clone(), "mount"),
            cx,
            move |_, cx: &mut Context<Mount>| {
                let observe = cx.observe(&initial_state, |_, _, cx| cx.notify());
                let owner = initial_state.downgrade();
                let release = cx.on_release(move |mount: &mut Mount, cx| {
                    let _ = mount
                        .owner
                        .update(cx, |state, cx| state.release_mount(mount.revision, cx));
                });
                Mount {
                    owner,
                    revision,
                    _observe: observe,
                    _release: release,
                    views: Rc::new(std::cell::RefCell::new(ItemViews::default())),
                }
            },
        );
        mount.update(cx, |mount, cx| {
            if mount.owner != state || mount.revision != revision {
                let _ = mount
                    .owner
                    .update(cx, |state, cx| state.release_mount(mount.revision, cx));
                mount.owner = state.downgrade();
                mount.revision = revision;
                mount.views.borrow_mut().entries.clear();
                mount._observe = cx.observe(&state, |_, _, cx| cx.notify());
            }
        });
        if let Some(factory) = self.view_factory.clone() {
            let views = mount.read(cx).views.clone();
            views.borrow_mut().touched.clear();
            let source = self.state.read(cx).source_for_render();
            let content_revision = self.content_revision;
            let owner = self.state.clone();
            self.render = Rc::new(move |item, w, cx| {
                let key = item.key().clone();
                let stamp = (
                    source.revision_at(item.index()),
                    content_revision,
                    owner.read(cx).item_content_revision(item.index()),
                    item.index(),
                );
                let existing = {
                    let mut cache = views.borrow_mut();
                    cache.touched.insert(key.clone());
                    cache
                        .entries
                        .get(&key)
                        .filter(|(old, _)| *old == stamp)
                        .map(|(_, view)| view.clone())
                };
                let view = existing.unwrap_or_else(|| {
                    let view = factory(item.identity(), w, cx);
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
        let native = self.state.read(cx).native_measured().cloned();
        if native.is_none() {
            let scroll = self.state.read(cx).native_scroll_handle().unwrap().clone();
            let base = std::mem::replace(&mut self.base, div().id(self.id.clone()));
            self.base = if self.state.read(cx).axis() == Axis::Vertical {
                base.overflow_y_scroll()
            } else {
                base.overflow_x_scroll()
            }
            .lock_scroll_axis()
            .track_scroll(&scroll);
        }
        let layout = self.base.interactivity().request_layout(
            id,
            inspector,
            window,
            cx,
            |style, window, cx| {
                window.with_text_style(style.text_style().cloned(), |window| {
                    window.request_layout(style, None, cx)
                })
            },
        );
        (
            layout,
            VirtualListFrame {
                items: Vec::new(),
                measured: None,
                native,
            },
        )
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut VirtualListFrame,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        self.state
            .update(cx, |state, cx| state.observe_scrollbar_activity(cx));
        self.state
            .update(cx, |state, cx| state.update_focus(window, cx));
        let clip = bounds.intersect(&window.content_mask().bounds);
        let visible = clip.size.width > px(0.)
            && clip.size.height > px(0.)
            && window.is_visible()
            && window.is_window_active();
        if !visible || cx.reduce_motion() {
            self.state.update(cx, |state, _| state.settle_motion());
        }
        let now = cx.background_executor().now();
        let viewport = self.state.read(cx).owned_viewport(bounds).unwrap();
        let (mut snapshot, request) = self
            .state
            .update(cx, |state, _| state.prepare(&viewport, now))
            .unwrap();
        let mut applied = request
            .as_ref()
            .map_or(viewport.offset(), VirtualizerScrollRequest::offset);
        let layout = self.state.read(cx).layout_values();
        let padding = self.state.read(cx).native_padding();
        if let Some(native) = &frame.native {
            native.set_retained_items(
                snapshot
                    .retained_keys()
                    .iter()
                    .filter_map(|key| snapshot.item_for_key(key).ok().map(|item| item.index()))
                    .collect(),
            );
            let render = self.render.clone();
            let role = self.item_role;
            let source = snapshot.clone();
            let axis = source.axis();
            let batch = native.layout_for_precise_viewport_on_axis(
                snapshot.axis(),
                bounds,
                applied,
                padding,
                move |index, w, cx| {
                    let item = source
                        .item(index)
                        .expect("source keys must be unique and invertible");
                    let is_last = index + 1 == source.len();
                    let mut row = item_element(item, role, source.len(), axis, &render, w, cx);
                    if !is_last {
                        row = if axis == Axis::Vertical {
                            row.pb(layout.gap)
                        } else {
                            row.pr(layout.gap)
                        };
                    }
                    row.test_support().into_any_element()
                },
                window,
                cx,
            );
            applied = batch.precise_offset();
            frame.measured = Some(batch);
            self.state
                .update(cx, |state, _| state.sync_measured_geometry());
            // Resolve alignment from real target geometry in this same batch.
            // The second pass is bounded; unknown off-screen items remain hints.
            let current = viewport.clone().with_offset(applied);
            let (_, corrected) = self
                .state
                .update(cx, |state, _| state.prepare(&current, now))
                .unwrap();
            if let Some(corrected) = corrected {
                let target = corrected.offset();
                if (target - applied).abs() > 0.5 {
                    let render = self.render.clone();
                    let role = self.item_role;
                    let source = self.state.read(cx).snapshot(&current).unwrap();
                    let axis = source.axis();
                    let batch = native.layout_for_precise_viewport_on_axis(
                        snapshot.axis(),
                        bounds,
                        target,
                        padding,
                        move |index, w, cx| {
                            let item = source.item(index).unwrap();
                            let is_last = index + 1 == source.len();
                            let mut row =
                                item_element(item, role, source.len(), axis, &render, w, cx);
                            if !is_last {
                                row = if axis == Axis::Vertical {
                                    row.pb(layout.gap)
                                } else {
                                    row.pr(layout.gap)
                                };
                            }
                            row.test_support().into_any_element()
                        },
                        window,
                        cx,
                    );
                    applied = batch.precise_offset();
                    frame.measured = Some(batch);
                    self.state
                        .update(cx, |state, _| state.sync_measured_geometry());
                }
            }
            let final_viewport = viewport.clone().with_offset(applied);
            let (_, final_request) = self
                .state
                .update(cx, |state, _| state.prepare(&final_viewport, now))
                .unwrap();
            self.state.update(cx, |state, _| {
                if let Some(request) = &final_request {
                    state.confirm(request, applied, &final_viewport).unwrap();
                } else {
                    state.commit(&final_viewport).unwrap();
                }
            });
            snapshot = self.state.read(cx).snapshot(&final_viewport).unwrap();
        } else {
            applied = applied.clamp(
                0.0,
                (snapshot.total_extent() - f64::from(viewport.extent())).max(0.0),
            );
            self.state.update(cx, |state, _| {
                state.apply_owned_offset(applied);
                if let Some(request) = &request {
                    state.confirm(request, applied, &viewport).unwrap();
                } else {
                    state
                        .commit(&viewport.clone().with_offset(applied))
                        .unwrap();
                }
            });
            snapshot = self
                .state
                .read(cx)
                .snapshot(&viewport.clone().with_offset(applied))
                .unwrap();
        }
        let content_size = if frame.native.is_some() {
            bounds.size
        } else if snapshot.axis() == Axis::Vertical {
            size(bounds.size.width, Pixels::from(snapshot.total_extent()))
        } else {
            size(Pixels::from(snapshot.total_extent()), bounds.size.height)
        };
        let hitbox = self.base.interactivity().prepaint(
            id,
            inspector,
            bounds,
            content_size,
            window,
            cx,
            |_, _, hitbox, window, cx| {
                if let Some(batch) = &mut frame.measured {
                    batch.prepaint_precise(bounds, applied, window, cx);
                } else {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        for item in snapshot.working_items() {
                            let item = item.expect("valid source key");
                            let main = Pixels::from(item.start() - applied);
                            let extent = item.extent();
                            let row = item_element(
                                item,
                                self.item_role,
                                snapshot.len(),
                                snapshot.axis(),
                                &self.render,
                                window,
                                cx,
                            );
                            let mut element = if snapshot.axis() == Axis::Vertical {
                                row.h(extent)
                            } else {
                                row.w(extent).h_full()
                            }
                            .test_support()
                            .into_any_element();
                            let (origin, available) = if snapshot.axis() == Axis::Vertical {
                                (
                                    bounds.origin + point(px(0.), main),
                                    size(
                                        AvailableSpace::Definite(bounds.size.width),
                                        AvailableSpace::Definite(extent),
                                    ),
                                )
                            } else {
                                (
                                    bounds.origin + point(main, px(0.)),
                                    size(
                                        AvailableSpace::Definite(extent),
                                        AvailableSpace::Definite(bounds.size.height),
                                    ),
                                )
                            };
                            element.layout_as_root(available, window, cx);
                            element.prepaint_at(origin, window, cx);
                            frame.items.push(element);
                        }
                    });
                }
                hitbox
            },
        );
        if visible && self.state.read(cx).has_scroll_command() {
            window.request_animation_frame();
        }
        self.state.update(cx, |state, cx| {
            state.arm_scroll_idle(cx);
            for event in state.take_events() {
                cx.emit(event);
            }
        });
        if self.view_factory.is_some() {
            let mount: Entity<Mount> =
                window.use_keyed_state((self.id.clone(), "mount"), cx, |_, _| {
                    unreachable!("mount exists")
                });
            let views = mount.read(cx).views.clone();
            let mut cache = views.borrow_mut();
            let touched = std::mem::take(&mut cache.touched);
            cache.entries.retain(|key, _| touched.contains(key));
            cache.touched = touched;
        }
        hitbox
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut VirtualListFrame,
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let (Some(batch), Some(native), Some(hitbox)) = (&frame.measured, &frame.native, &hitbox)
        {
            batch.paint_scroll_listener(native, bounds, hitbox, window);
        }
        self.base.interactivity().paint(
            id,
            inspector,
            bounds,
            hitbox.as_ref(),
            window,
            cx,
            |_, window, cx| {
                if let Some(batch) = &mut frame.measured {
                    batch.paint(bounds, window, cx);
                } else {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        for item in &mut frame.items {
                            item.paint(window, cx);
                        }
                    });
                }
            },
        );
    }
}

fn item_element(
    item: VirtualizerItem,
    role: Option<Role>,
    count: usize,
    axis: Axis,
    render: &Renderer,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let index = item.index();
    let row = div().id(item.key().clone());
    let mut row = if axis == Axis::Vertical {
        row.w_full()
    } else {
        row.h_full()
    };
    if let Some(role) = role {
        row = row
            .role(role)
            .aria_position_in_set(index + 1)
            .aria_size_of_set(count);
    }
    row.child(render(item, window, cx))
}

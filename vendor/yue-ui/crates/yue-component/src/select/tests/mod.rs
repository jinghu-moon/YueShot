//! Shared Select fixtures and focused interaction suites.

mod keyboard;

mod pointer;

mod requests;

mod layout;

use super::*;

use gpui_kit::base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::prelude::FluentBuilder as _;

use gpui_kit::{
    AppContext as _, Context, Modifiers, ParentElement as _, Render, Role, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};

struct Probe {
    state: Entity<YueSelectState>,
    events: Vec<SelectEvent>,
    _subscription: gpui_kit::Subscription,
    form: Entity<crate::YueForm>,
    size: SelectSize,
    width: gpui_kit::Pixels,
    visible: bool,
    decorate: Option<Rc<dyn Fn(YueSelect) -> YueSelect>>,
    near_right_edge: bool,
}

impl Probe {
    fn new(mode: SelectMode, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| {
            let mut state = YueSelectState::new(mode, window, cx);
            state
                .set_options(
                    [
                        SelectOption::new("a", "Apple"),
                        SelectOption::new("b", "Banana").disabled(true),
                        SelectOption::new("c", "Cherry"),
                    ],
                    cx,
                )
                .unwrap();
            state
        });
        let subscription = cx.subscribe(&state, |probe, _, event, cx| {
            probe.events.push(event.clone());
            cx.notify();
        });
        let form = cx.new(|_| crate::YueForm::new());
        form.update(cx, |form, _| {
            form.require_select("fruit", &state, "请选择水果");
        });
        Self {
            state,
            events: vec![],
            _subscription: subscription,
            form,
            size: SelectSize::Medium,
            width: px(240.),
            visible: true,
            decorate: None,
            near_right_edge: false,
        }
    }
}

impl Render for Probe {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut select = YueSelect::new("fruit", &self.state)
            .accessibility_label("Fruit")
            .size(self.size)
            .width(self.width)
            .clearable(true);
        if let Some(decorate) = &self.decorate {
            select = decorate(select);
        }
        div()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .w(px(600.))
            .when(self.near_right_edge, |this| {
                this.w(window.viewport_size().width).items_end()
            })
            .child(crate::YueButton::new("before", "Before"))
            .children(self.visible.then_some(select))
            .child(crate::YueButton::new("after", "After"))
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn open(cx: &mut TestAppContext, mode: SelectMode) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(|cx| {
        crate::init(
            cx,
            crate::YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure),
        )
        .unwrap()
    });
    let mut probe = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Probe::new(mode, window, cx));
        probe = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    draw(cx);
    draw(cx);
    (probe.unwrap(), cx)
}

fn find(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|window, _| {
        let found = snapshots(window)
            .into_iter()
            .filter(|snapshot| snapshot.label() == Some(label))
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "one target for {label}: {found:?}");
        found[0].clone()
    })
}

fn click(cx: &mut VisualTestContext, label: &str) {
    let point = find(cx, label).bounds().center();
    cx.simulate_click(point, Modifiers::default());
    draw(cx);
}

fn key(cx: &mut VisualTestContext, key: &str) {
    let key = gpui_kit::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: key.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke: key });
    draw(cx);
}

fn change_count(view: &Entity<Probe>, cx: &VisualTestContext) -> usize {
    view.read_with(cx, |probe, _| {
        probe
            .events
            .iter()
            .filter(|event| matches!(event, SelectEvent::Change(_)))
            .count()
    })
}

fn select_panel(cx: &mut VisualTestContext) -> Option<ElementSnapshot> {
    let id: gpui_kit::ElementId = (gpui_kit::ElementId::from("fruit"), "select-panel").into();
    cx.update(|window, _| {
        snapshots(window)
            .into_iter()
            .find(|node| node.path().last() == Some(&id))
    })
}

fn tick(cx: &mut VisualTestContext, millis: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(millis));
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}

fn panel_alpha(cx: &mut VisualTestContext) -> f32 {
    let panel = select_panel(cx).expect("Select panel remains mounted during its animation");
    cx.update(|window, _| {
        window
            .painted_quads()
            .into_iter()
            .find(|quad| quad.bounds == panel.bounds().scale(window.scale_factor()))
            .and_then(|quad| quad.background.as_solid())
            .expect("panel background submitted to scene")
            .a
    })
}

fn assert_complete_focus_ring(cx: &mut VisualTestContext, frame: gpui_kit::Bounds<Pixels>) {
    cx.update(|window, _| {
        let theme = crate::YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure);
        let ring: gpui_kit::Hsla = crate::tokens::rgba(theme, "--input-focus-ring-color").into();
        let outside = px(crate::tokens::pixels(theme, "--input-focus-ring-width")
            + crate::tokens::pixels(theme, "--input-focus-ring-offset"));
        let expected = gpui_kit::Bounds::new(
            frame.origin - gpui_kit::point(outside, outside),
            frame.size + gpui_kit::size(outside * 2., outside * 2.),
        )
        .scale(window.scale_factor());
        let rings = window
            .painted_quads()
            .into_iter()
            .filter(|quad| quad.border_color == ring && quad.bounds == expected)
            .collect::<Vec<_>>();
        assert!(!rings.is_empty(), "whole focus ring at {expected:?}");
        let half_width = rings[0].border_widths.top / 2.;
        let center = expected.center();
        for point in [
            gpui_kit::point(center.x, expected.top() + half_width),
            gpui_kit::point(center.x, expected.bottom() - half_width),
            gpui_kit::point(expected.left() + half_width, center.y),
            gpui_kit::point(expected.right() - half_width, center.y),
        ] {
            assert!(
                rings
                    .iter()
                    .any(|quad| quad.content_mask.bounds.contains(&point)),
                "focus border at {point:?} must survive clipping: {rings:?}"
            );
        }
    });
}

fn last_request(view: &Entity<Probe>, cx: &VisualTestContext) -> SelectRequestTicket {
    view.read_with(cx, |v, _| {
        v.events
            .iter()
            .rev()
            .find_map(|event| match event {
                SelectEvent::SearchRequested(r)
                | SelectEvent::LoadMoreRequested(r)
                | SelectEvent::CreateRequested(r) => Some(r.ticket().clone()),
                _ => None,
            })
            .expect("request emitted")
    })
}

struct ModalProbe {
    dialog: Entity<crate::YueDialogState>,
    select: Entity<YueSelectState>,
}

impl Render for ModalProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::YueDialog::new("select-modal", crate::current_theme(cx), &self.dialog)
            .title("Modal select")
            .child(YueSelect::new("modal-select", &self.select).accessibility_label("Modal fruit"))
    }
}

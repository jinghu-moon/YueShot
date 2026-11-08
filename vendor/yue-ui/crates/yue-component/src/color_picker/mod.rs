//! Retained HSV color editing with independent HEX/RGB/HSL fields and alpha.

mod element;

use crate::popup::PopupAnchor;

use crate::tokens::{duration, easing, pixels, rems, rgba};

use crate::{YueTheme, control_layout::ControlLayout};

use color::Color;

use gpui_kit::base::{Align, Placement, PopoverState, TestSupportExt as _, Transition, transition};

use gpui_kit::{
    App, Bounds, ElementId, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Rgba, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, prelude::FluentBuilder as _, px,
};

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

mod color;

pub(crate) mod paint;

mod panel;

mod state;

pub use color::ColorFormat;

pub use state::{ColorPickerEvent, YueColorPickerState};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorPickerSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorPickerTrigger {
    #[default]
    Field,
    Swatch,
}

#[derive(Clone)]
pub(super) struct Options {
    theme: YueTheme,
    inherited: bool,
    name: SharedString,
    label: Option<SharedString>,
    presets: Vec<Color>,
    recent: bool,
    disabled: bool,
}

impl Options {
    fn new(theme: YueTheme) -> Self {
        Self {
            theme,
            inherited: false,
            name: SharedString::default(),
            label: None,
            presets: vec![],
            recent: true,
            disabled: false,
        }
    }
    fn resolved(mut self, cx: &App) -> Self {
        if self.inherited {
            self.theme = crate::current_theme(cx);
        }
        if let Some(label) = &self.label
            && self.name.trim().is_empty()
        {
            self.name = label.clone();
        }
        assert!(
            !self.name.trim().is_empty(),
            "ColorPicker requires a visible label or accessibility_label"
        );
        self
    }
}

type ConfirmHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A color field whose nonmodal editor shares the same state as YueColorPickerPanel.
///
/// ```
/// use yue_component::{YueColorPicker, YueColorPickerState, current_theme, gpui_kit};
/// use gpui_kit::{AppContext as _, Context, Entity, IntoElement, Render, Window};
/// struct Properties { fill: Entity<YueColorPickerState> }
/// impl Properties {
///     fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
///         Self { fill: cx.new(|cx| YueColorPickerState::new(window, cx)
///             .default_value(gpui_kit::rgba(0x3366CC80))) }
///     }
/// }
/// impl Render for Properties {
///     fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
///         YueColorPicker::new("fill", current_theme(cx), &self.fill)
///             .label("Fill").use_default_theme()
///     }
/// }
/// ```
#[derive(IntoElement)]
pub struct YueColorPicker {
    id: ElementId,
    state: Entity<YueColorPickerState>,
    options: Options,
    size: ColorPickerSize,
    trigger: ColorPickerTrigger,
    placement: Placement,
    align: Align,
    layout: ControlLayout,
    focus: Option<FocusHandle>,
    tab_stop: bool,
    group_selected: Option<bool>,
    pub(crate) on_confirm: Option<ConfirmHandler>,
}

/// The same editor in a persistent properties pane, without a trigger or popup.
///
/// ```
/// use yue_component::{YueColorPickerPanel, YueColorPickerState, YueTheme, gpui_kit};
/// fn panel(state: &gpui_kit::Entity<YueColorPickerState>, theme: YueTheme) -> YueColorPickerPanel {
///     YueColorPickerPanel::new("fill-panel", theme, state).label("Fill")
///         .width(gpui_kit::px(300.))
/// }
/// ```
#[derive(IntoElement)]
pub struct YueColorPickerPanel {
    id: ElementId,
    state: Entity<YueColorPickerState>,
    options: Options,
    layout: ControlLayout,
}

crate::control_layout::control_layout!(YueColorPicker);

crate::control_layout::control_layout!(YueColorPickerPanel);

macro_rules! builders {
    ($ty:ty) => {
        impl $ty {
            pub fn use_default_theme(mut self) -> Self {
                self.options.inherited = true;
                self
            }
            pub fn theme(mut self, theme: YueTheme) -> Self {
                self.options.theme = theme;
                self.options.inherited = false;
                self
            }
            pub fn label(mut self, label: impl Into<SharedString>) -> Self {
                self.options.label = Some(label.into());
                self
            }
            pub fn accessibility_label(mut self, name: impl Into<SharedString>) -> Self {
                self.options.name = name.into();
                self
            }
            /// # Panics
            /// If a preset has a nonfinite or out-of-range RGBA channel.
            pub fn presets(mut self, colors: impl IntoIterator<Item = Rgba>) -> Self {
                for c in colors {
                    let c = Color::from_rgba(c);
                    if !self.options.presets.contains(&c) {
                        self.options.presets.push(c);
                    }
                }
                self
            }
            pub fn show_recent(mut self, show: bool) -> Self {
                self.options.recent = show;
                self
            }
            pub fn disabled(mut self, disabled: bool) -> Self {
                self.options.disabled = disabled;
                self
            }
        }
    };
}
builders!(YueColorPicker);

builders!(YueColorPickerPanel);

impl YueColorPicker {
    pub(crate) fn group_focus(
        mut self,
        focus: FocusHandle,
        tab_stop: bool,
        selected: bool,
    ) -> Self {
        self.focus = Some(focus);
        self.tab_stop = tab_stop;
        self.group_selected = Some(selected);
        self
    }
    pub fn new(
        id: impl Into<ElementId>,
        theme: YueTheme,
        state: &Entity<YueColorPickerState>,
    ) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            options: Options::new(theme),
            size: Default::default(),
            trigger: Default::default(),
            placement: Placement::Bottom,
            align: Align::Start,
            layout: Default::default(),
            on_confirm: None,
            focus: None,
            tab_stop: true,
            group_selected: None,
        }
    }
    pub fn size(mut self, size: ColorPickerSize) -> Self {
        self.size = size;
        self
    }
    pub fn trigger(mut self, trigger: ColorPickerTrigger) -> Self {
        self.trigger = trigger;
        self
    }
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }
}

impl YueColorPickerPanel {
    pub fn new(
        id: impl Into<ElementId>,
        theme: YueTheme,
        state: &Entity<YueColorPickerState>,
    ) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            options: Options::new(theme),
            layout: Default::default(),
        }
    }
}

#[derive(Clone)]
pub(super) struct Popup {
    state: Entity<PopoverState>,
    trigger: FocusHandle,
    anchor: PopupAnchor,
    format_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Popup {
    fn dismiss(
        &self,
        editor: &Entity<YueColorPickerState>,
        window: &mut Window,
        cx: &mut App,
        restore: bool,
    ) {
        if restore {
            self.trigger.focus(window, cx);
        }
        self.state.update(cx, |s, cx| s.dismiss(window, cx));
        editor.update(cx, |s, cx| s.close(window, cx));
        window.refresh();
    }
}

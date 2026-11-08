//! Controlled background options; checkerboards and gradients are not RGBA values.

mod element;

use crate::{YueColorPickerState, YueTheme, control_layout::ControlLayout};

use gpui_kit::{
    App, Background, ElementId, Entity, FocusHandle, IntoElement, Rgba, SharedString, Window,
};

use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
enum SwatchInk {
    Solid(Rgba),
    Checker(Rgba, Rgba),
    Gradient(Background),
}

#[derive(Clone)]
pub struct YueSwatchOption {
    value: SharedString,
    label: SharedString,
    ink: SwatchInk,
    disabled: bool,
}

impl YueSwatchOption {
    /// # Panics
    /// If a color channel is nonfinite or outside 0..=1.
    pub fn solid(
        value: impl Into<SharedString>,
        label: impl Into<SharedString>,
        color: Rgba,
    ) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            ink: SwatchInk::Solid(valid_color(color)),
            disabled: false,
        }
    }
    /// # Panics
    /// If a color channel is nonfinite or outside 0..=1.
    pub fn checkerboard(
        value: impl Into<SharedString>,
        label: impl Into<SharedString>,
        light: Rgba,
        dark: Rgba,
    ) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            ink: SwatchInk::Checker(valid_color(light), valid_color(dark)),
            disabled: false,
        }
    }
    /// # Panics
    /// If the angle or an RGBA channel is not finite, or a channel is out of range.
    pub fn gradient(
        value: impl Into<SharedString>,
        label: impl Into<SharedString>,
        angle: f32,
        from: Rgba,
        to: Rgba,
    ) -> Self {
        assert!(angle.is_finite(), "Gradient angle must be finite");
        for c in [from, to] {
            assert!(
                [c.r, c.g, c.b, c.a]
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "Gradient channels must be finite in 0..=1"
            );
        }
        let background = gpui_kit::linear_gradient(
            angle.rem_euclid(360.),
            gpui_kit::linear_color_stop(from, 0.),
            gpui_kit::linear_color_stop(to, 1.),
        )
        .color_space(gpui_kit::ColorSpace::Srgb);
        Self {
            value: value.into(),
            label: label.into(),
            ink: SwatchInk::Gradient(background),
            disabled: false,
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

fn valid_color(color: Rgba) -> Rgba {
    assert!(
        [color.r, color.g, color.b, color.a]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
        "Swatch requires finite RGBA in 0..=1"
    );
    color
}

type Handler = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct YueSwatchGroup {
    id: ElementId,
    theme: YueTheme,
    inherited: bool,
    value: SharedString,
    label: SharedString,
    options: Vec<YueSwatchOption>,
    disabled: bool,
    handler: Option<Handler>,
    layout: ControlLayout,
    custom: Option<(SharedString, SharedString, Entity<YueColorPickerState>)>,
    pub(crate) paint_only: bool,
    pub(crate) on_activate: Option<Handler>,
    pub(crate) entry: Option<Rc<RefCell<Vec<FocusHandle>>>>,
}

crate::control_layout::control_layout!(YueSwatchGroup);

impl YueSwatchGroup {
    pub fn new(
        id: impl Into<ElementId>,
        theme: YueTheme,
        selected_value: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            inherited: false,
            value: selected_value.into(),
            label: SharedString::default(),
            options: vec![],
            disabled: false,
            handler: None,
            layout: Default::default(),
            custom: None,
            entry: None,
            on_activate: None,
            paint_only: false,
        }
    }
    pub fn use_default_theme(mut self) -> Self {
        self.inherited = true;
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.inherited = false;
        self
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
    pub fn accessibility_label(self, label: impl Into<SharedString>) -> Self {
        self.label(label)
    }
    /// # Panics
    /// If values repeat, are empty, or a label is empty.
    pub fn options(mut self, options: impl IntoIterator<Item = YueSwatchOption>) -> Self {
        self.options = options.into_iter().collect();
        for (ix, o) in self.options.iter().enumerate() {
            assert!(
                !o.value.trim().is_empty() && !o.label.trim().is_empty(),
                "Swatch needs value and label"
            );
            assert!(
                !self.options[..ix].iter().any(|p| p.value == o.value),
                "Duplicate swatch value"
            );
        }
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn on_change(
        mut self,
        handler: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Some(Rc::new(handler));
        self
    }
    pub fn custom_color(
        mut self,
        value: impl Into<SharedString>,
        label: impl Into<SharedString>,
        state: &Entity<YueColorPickerState>,
    ) -> Self {
        self.custom = Some((value.into(), label.into(), state.clone()));
        self
    }
}

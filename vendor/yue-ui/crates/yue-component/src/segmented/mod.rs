//! Controlled single choice with a shared, moving selection surface.

mod element;

mod behavior;

use std::rc::Rc;

use gpui_kit::{App, ElementId, IntoElement, SharedString, Window};

use crate::{IconDefinition, YueTheme};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SegmentedSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SegmentedVariant {
    #[default]
    Neutral,
    Accent,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SegmentedShape {
    #[default]
    Rounded,
    Pill,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SegmentedDistribution {
    #[default]
    Content,
    Equal,
}

/// A stable domain value and its visible content. Values must be unique in a group.
#[derive(Clone)]
pub struct YueSegmentedOption {
    value: SharedString,
    label: SharedString,
    name: SharedString,
    icon: Option<&'static [u8]>,
    disabled: bool,
}

impl YueSegmentedOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        Self {
            value: value.into(),
            name: label.clone(),
            label,
            icon: None,
            disabled: false,
        }
    }
    pub fn icon(mut self, icon: impl IconDefinition) -> Self {
        self.icon = Some(icon.svg());
        self
    }
    pub fn icon_only(
        value: impl Into<SharedString>,
        icon: impl IconDefinition,
        name: impl Into<SharedString>,
    ) -> Self {
        Self {
            value: value.into(),
            label: SharedString::default(),
            name: name.into(),
            icon: Some(icon.svg()),
            disabled: false,
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

type ChangeHandler = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// The caller owns the value. Activation requests a new value once; write it back
/// and notify the owner to commit a selection. A selected option cannot uncheck itself.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueSegmented, YueSegmentedOption, YueTheme};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let control = YueSegmented::new("period", theme, "day")
///     .label("Period")
///     .options([YueSegmentedOption::new("day", "Day"), YueSegmentedOption::new("week", "Week")]);
/// ```
#[derive(IntoElement)]
pub struct YueSegmented {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    value: SharedString,
    options: Vec<YueSegmentedOption>,
    label: Option<SharedString>,
    accessibility_label: Option<SharedString>,
    size: SegmentedSize,
    variant: SegmentedVariant,
    shape: SegmentedShape,
    distribution: SegmentedDistribution,
    disabled: bool,
    on_change: Option<ChangeHandler>,
    layout: crate::control_layout::ControlLayout,
}

crate::control_layout::control_layout!(YueSegmented);

impl YueSegmented {
    pub fn new(id: impl Into<ElementId>, theme: YueTheme, value: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            value: value.into(),
            options: vec![],
            label: None,
            accessibility_label: None,
            size: SegmentedSize::default(),
            variant: SegmentedVariant::default(),
            shape: SegmentedShape::default(),
            distribution: SegmentedDistribution::default(),
            disabled: false,
            on_change: None,
            layout: Default::default(),
        }
    }
    pub fn use_default_theme(mut self) -> Self {
        self.use_default_theme = true;
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.use_default_theme = false;
        self
    }
    /// # Panics
    /// If values are empty or repeated, or an option has no accessible name.
    pub fn options(mut self, options: impl IntoIterator<Item = YueSegmentedOption>) -> Self {
        self.options = options.into_iter().collect();
        for (ix, option) in self.options.iter().enumerate() {
            assert!(
                !option.value.trim().is_empty(),
                "YueSegmented {:?}: empty option value",
                self.id
            );
            assert!(
                !option.name.trim().is_empty(),
                "YueSegmented {:?}: option {:?} needs an accessible name",
                self.id,
                option.value
            );
            assert!(
                !self.options[..ix].iter().any(|o| o.value == option.value),
                "YueSegmented {:?}: duplicate value {:?}",
                self.id,
                option.value
            );
        }
        self
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }
    pub fn size(mut self, size: SegmentedSize) -> Self {
        self.size = size;
        self
    }
    pub fn variant(mut self, variant: SegmentedVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn shape(mut self, shape: SegmentedShape) -> Self {
        self.shape = shape;
        self
    }
    pub fn distribution(mut self, distribution: SegmentedDistribution) -> Self {
        self.distribution = distribution;
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
        self.on_change = Some(Rc::new(handler));
        self
    }
}

#[cfg(test)]
mod tests;

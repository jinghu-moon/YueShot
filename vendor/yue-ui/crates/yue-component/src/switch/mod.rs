//! switch: public component contract and builders.

mod element;

mod paint;

use std::rc::Rc;

use gpui_kit::{App, ClickEvent, ElementId, IntoElement, SharedString, Window};

use crate::YueTheme;

type ChangeHandler = Rc<dyn Fn(bool, &ClickEvent, &mut Window, &mut App)>;

/// Half of a 90° sweep, expressed as a fraction of a full turn: the thumb icons
/// swing 45° as they cross-fade. The design system has no motion token for an
/// angle, so the spec's value lives here.
const INDICATOR_SWING_TURNS: f32 = 0.125;

/// The size an indicator icon shrinks to at the far end of its fade.
const INDICATOR_MIN_SCALE: f32 = 0.5;

/// Three coordinated Yue Switch size profiles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SwitchSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// Track and thumb corner geometry.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SwitchShape {
    #[default]
    Capsule,
    RoundedRectangle,
}

/// Which icons the thumb shows.
///
/// Each icon fades, shrinks and swings on the switch's own duration and curve,
/// so an icon that should not be seen is the same icon at opacity 0 rather than a
/// mounted or dropped element.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SwitchIndicator {
    #[default]
    None,
    /// The check only: it swings in from the un-checked side as the switch turns
    /// on, and back out as it turns off.
    Check,
    /// The cross only: it swings out towards the checked side as the switch turns
    /// on.
    X,
    /// Both icons stacked in the thumb, cross-fading in opposite directions.
    CheckX,
}

/// A controlled Yue switch. The caller owns and updates the checked value.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueSwitch};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let control = YueSwitch::new("enabled", theme, false, "启用")
///     .on_change(|next, _, _, _| { /* owner stores next, then notifies */ });
/// ```
#[derive(IntoElement)]
pub struct YueSwitch {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    checked: bool,
    disabled: bool,
    loading: bool,
    size: SwitchSize,
    shape: SwitchShape,
    indicator: SwitchIndicator,
    accessibility_label: SharedString,
    on_change: Option<ChangeHandler>,
    layout: crate::control_layout::ControlLayout,
    focus_handle: Option<gpui_kit::FocusHandle>,
    key_context: Option<gpui_kit::SharedString>,
    description: Option<gpui_kit::SharedString>,
    tab_index: isize,
    tab_stop: bool,
}

crate::control_layout::control_layout!(YueSwitch);

impl YueSwitch {
    /// Use caller-owned focus without replacing the activation handler.
    pub fn track_focus(mut self, focus: &gpui_kit::FocusHandle) -> Self {
        self.focus_handle = Some(focus.clone());
        self
    }
    pub fn key_context(mut self, context: impl Into<gpui_kit::SharedString>) -> Self {
        self.key_context = Some(context.into());
        self
    }
    pub fn accessibility_description(
        mut self,
        description: impl Into<gpui_kit::SharedString>,
    ) -> Self {
        self.description = Some(description.into());
        self
    }
    /// Focus traversal order. It is applied to whichever handle the control
    /// tracks, including one passed to `track_focus`, so set the order here
    /// rather than on that handle.
    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = index;
        self
    }
    /// Whether Tab traversal stops on the control; like `tab_index`, it is
    /// applied to a caller-owned handle too. The default is `true`.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Resolve the application default at render time, including later theme changes.
    pub fn use_default_theme(mut self) -> Self {
        self.use_default_theme = true;
        self
    }
    /// Override the application default for this component instance.
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.use_default_theme = false;
        self
    }

    pub fn new(
        id: impl Into<ElementId>,
        theme: YueTheme,
        checked: bool,
        accessibility_label: impl Into<SharedString>,
    ) -> Self {
        let accessibility_label = accessibility_label.into();
        assert!(
            !accessibility_label.is_empty(),
            "YueSwitch requires an accessible name"
        );
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            layout: Default::default(),
            focus_handle: None,
            key_context: None,
            description: None,
            tab_index: 0,
            tab_stop: true,
            checked,
            disabled: false,
            loading: false,
            size: SwitchSize::Medium,
            shape: SwitchShape::Capsule,
            indicator: SwitchIndicator::None,
            accessibility_label,
            on_change: None,
        }
    }

    pub fn size(mut self, size: SwitchSize) -> Self {
        self.size = size;
        self
    }

    pub fn shape(mut self, shape: SwitchShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn indicator(mut self, indicator: SwitchIndicator) -> Self {
        self.indicator = indicator;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Spin the thumb in place while an async commit is in flight.
    ///
    /// The thumb keeps its position and the indicators fade out behind the
    /// spinner. `on_change` is not reported until loading clears, so a slow
    /// commit cannot be toggled twice. The control reports unavailable to
    /// assistive technology and rejects presses while loading.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn on_change(
        mut self,
        handler: impl Fn(bool, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

#[cfg(test)]
mod tests;

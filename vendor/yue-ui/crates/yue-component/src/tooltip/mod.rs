//! Tooltip policy and presentation over the shared, unstyled Popup host.
mod controller;
mod group;
mod overflow;
mod surface;
pub(crate) use controller::attach;

use crate::YueTheme;
use gpui_kit::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Rems, RenderOnce, SharedString, Styled as _, Window, div,
};
pub use group::YueTooltipGroup;
pub use overflow::YueOverflowTooltip;
use std::{cell::Cell, rc::Rc, time::Duration};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TooltipPlacement {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TooltipAlign {
    Start,
    #[default]
    Center,
    End,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TooltipTrigger {
    #[default]
    HoverFocus,
    Hover,
    Focus,
    Click,
    ContextMenu,
    Manual,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TooltipVariant {
    #[default]
    Default,
    Primary,
    Success,
    Danger,
    Warning,
    Light,
}

/// Visual and timing options for a Button's HoverFocus tooltip.
#[derive(Clone, Debug, PartialEq)]
pub struct TooltipOptions {
    placement: Option<TooltipPlacement>,
    align: TooltipAlign,
    arrow: bool,
    variant: TooltipVariant,
    delay: Option<Duration>,
    auto_hide: Option<Duration>,
    dismiss_on_press: bool,
    gap: Option<Rems>,
    anchor_inset: Rems,
}
impl Default for TooltipOptions {
    fn default() -> Self {
        Self {
            placement: None,
            align: TooltipAlign::Center,
            arrow: true,
            variant: TooltipVariant::Default,
            delay: None,
            auto_hide: None,
            dismiss_on_press: true,
            gap: None,
            anchor_inset: gpui_kit::rems(0.),
        }
    }
}
impl TooltipOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn placement(mut self, placement: TooltipPlacement) -> Self {
        self.placement = Some(placement);
        self
    }
    pub fn align(mut self, align: TooltipAlign) -> Self {
        self.align = align;
        self
    }
    pub fn show_arrow(mut self, show: bool) -> Self {
        self.arrow = show;
        self
    }
    pub fn variant(mut self, variant: TooltipVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn show_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
    pub fn auto_hide_after(mut self, duration: Option<Duration>) -> Self {
        self.auto_hide = duration;
        self
    }
    /// Keep a value tooltip visible while its trigger is being manipulated.
    /// Defaults to true for ordinary command/help tooltips.
    pub fn dismiss_on_press(mut self, dismiss: bool) -> Self {
        self.dismiss_on_press = dismiss;
        self
    }
}
type ChangeHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;
type TriggerBuilder = Box<dyn FnOnce(&FocusHandle, &mut Window, &mut App) -> AnyElement>;
#[derive(Clone)]
pub(crate) struct Config {
    text: SharedString,
    theme: YueTheme,
    options: TooltipOptions,
    mode: TooltipTrigger,
    disabled: bool,
    controlled: Option<bool>,
    handler: Option<ChangeHandler>,
    availability: Option<Rc<Cell<bool>>>,
}
impl Config {
    pub(super) fn new(text: SharedString, theme: YueTheme, options: TooltipOptions) -> Self {
        Self {
            text,
            theme,
            options,
            mode: TooltipTrigger::HoverFocus,
            disabled: false,
            controlled: None,
            handler: None,
            availability: None,
        }
    }
    fn same_content(&self, other: &Self) -> bool {
        self.text == other.text
            && self.options == other.options
            && self.mode == other.mode
            && self.disabled == other.disabled
            && self.controlled == other.controlled
            && self.theme.mode() == other.theme.mode()
            && self.theme.accent() == other.theme.accent()
            && self.theme.neutral() == other.theme.neutral()
    }
    fn available(&self) -> bool {
        !self.disabled
            && !self.text.trim().is_empty()
            && self.availability.as_ref().is_none_or(|value| value.get())
    }
    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// A non-interactive explanation. Initialize Yue before opening a Base Root.
/// The trigger retains its original focus, semantic role and command handlers.
///
/// ```no_run
/// use yue_component::{YueTooltip, YueButton, YueTheme, ThemeMode, Accent};
/// use yue_component::gpui_kit::IntoElement as _;
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let help = YueTooltip::new("export-help", theme, "Export the filtered rows")
///     .trigger_with(move |focus, _, _| YueButton::new("export", "Export…")
///         .track_focus(focus).into_any_element());
/// ```
#[derive(IntoElement)]
pub struct YueTooltip {
    id: ElementId,
    config: Config,
    default_theme: bool,
    focus: Option<FocusHandle>,
    child: Option<AnyElement>,
    builder: Option<TriggerBuilder>,
}
impl YueTooltip {
    /// Override the distance from the anchor's focus clearance to the arrow tip.
    /// # Panics
    /// Panics for a negative or non-finite distance.
    pub fn gap(mut self, gap: Rems) -> Self {
        assert!(
            gap.0.is_finite() && gap.0 >= 0.,
            "tooltip gap must be finite and non-negative"
        );
        self.config.options.gap = Some(gap);
        self
    }
    /// Measure a smaller visual anchor without shrinking its pointer target.
    pub(crate) fn anchor_inset(mut self, inset: Rems) -> Self {
        self.config.options.anchor_inset = inset;
        self
    }
    /// Whether pressing a Hover/Focus trigger dismisses its explanation.
    /// Set false for controls whose tooltip reports the live manipulated value.
    pub fn dismiss_on_press(mut self, dismiss: bool) -> Self {
        self.config.options.dismiss_on_press = dismiss;
        self
    }
    pub fn new(id: impl Into<ElementId>, theme: YueTheme, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            config: Config::new(text.into(), theme, TooltipOptions::default()),
            default_theme: false,
            focus: None,
            child: None,
            builder: None,
        }
    }
    pub fn use_default_theme(mut self) -> Self {
        self.default_theme = true;
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.config.theme = theme;
        self.default_theme = false;
        self
    }
    pub fn trigger(mut self, child: impl IntoElement) -> Self {
        self.child = Some(child.into_any_element());
        self.builder = None;
        self
    }
    pub fn trigger_with(
        mut self,
        build: impl FnOnce(&FocusHandle, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.builder = Some(Box::new(build));
        self.child = None;
        self
    }
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }
    pub fn trigger_mode(mut self, mode: TooltipTrigger) -> Self {
        self.config.mode = mode;
        self
    }
    pub fn placement(mut self, placement: TooltipPlacement) -> Self {
        self.config.options.placement = Some(placement);
        self
    }
    pub fn align(mut self, align: TooltipAlign) -> Self {
        self.config.options.align = align;
        self
    }
    pub fn show_arrow(mut self, show: bool) -> Self {
        self.config.options.arrow = show;
        self
    }
    pub fn variant(mut self, variant: TooltipVariant) -> Self {
        self.config.options.variant = variant;
        self
    }
    pub fn show_delay(mut self, delay: Duration) -> Self {
        self.config.options.delay = Some(delay);
        self
    }
    pub fn auto_hide_after(mut self, duration: Option<Duration>) -> Self {
        self.config.options.auto_hide = duration;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }
    pub fn open(mut self, open: bool) -> Self {
        self.config.controlled = Some(open);
        self
    }
    pub fn on_open_change(
        mut self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.config.handler = Some(Rc::new(handler));
        self
    }
}
impl RenderOnce for YueTooltip {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.default_theme {
            self.config.theme = crate::current_theme(cx);
        }
        if self.builder.is_some() && self.focus.is_none() {
            self.focus = Some(
                window
                    .use_keyed_state((self.id.clone(), "tooltip-focus"), cx, |_, cx| {
                        cx.focus_handle()
                    })
                    .read(cx)
                    .clone(),
            );
        }
        let child = if let Some(build) = self.builder {
            Some(build(self.focus.as_ref().unwrap(), window, cx))
        } else {
            self.child
        };
        let attachment = controller::attach(self.id.clone(), self.config, self.focus, window, cx);
        div()
            .id(self.id)
            .relative()
            .flex()
            .flex_shrink_0()
            .children(child)
            .child(attachment)
    }
}

pub(crate) fn init(cx: &mut App) {
    if cx.try_global::<gpui_kit::base::TooltipDefaults>().is_none() {
        gpui_kit::base::TooltipDefaults::new()
            .with_show_delay(crate::tokens::duration(
                crate::current_theme(cx),
                crate::theme::keys::DURATION_300,
            ))
            .install(cx);
    }
    controller::init(cx);
}

#[cfg(all(test, feature = "headless-tests"))]
mod tests;

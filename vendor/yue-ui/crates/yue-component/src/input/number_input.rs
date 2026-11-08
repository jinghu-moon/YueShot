//! Number input contract and step controls.

use super::frame::*;
use super::*;

use gpui_kit::{
    App, DefiniteLength, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _,
    TextAlign, Window, div, prelude::FluentBuilder as _,
};

use crate::YueTheme;

use yue_component_icons::Icon;

use crate::icon::YueIcon;

use crate::tokens::{rems, rgba as color};

/// Yue-styled number input using GPUI Base's spinbutton behavior.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueNumberInput};
/// use yue_component::gpui_kit::{Entity, base::input::InputState};
/// # fn example(state: &Entity<InputState>) {
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let control = YueNumberInput::new("field", theme, state).accessibility_label("名称");
/// # }
/// ```
#[derive(IntoElement)]
pub struct YueNumberInput {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    state: Entity<gpui_kit::base::input::InputState>,
    size: InputSize,
    status: InputStatus,
    accessibility_label: Option<SharedString>,
    accessibility_description: Option<SharedString>,
    on_step: Option<std::rc::Rc<NumberStepHandler>>,
    controls_right: bool,
    controls: bool,
    tab_stop: bool,
    disabled: bool,
    width: Option<DefiniteLength>,
    block: bool,
}

type NumberStepHandler = dyn Fn(gpui_kit::base::StepAction, &mut Window, &mut App);

impl YueNumberInput {
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
        state: &Entity<gpui_kit::base::input::InputState>,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            state: state.clone(),
            size: InputSize::Medium,
            status: InputStatus::Normal,
            accessibility_label: None,
            accessibility_description: None,
            on_step: None,
            controls_right: false,
            controls: true,
            tab_stop: true,
            disabled: false,
            width: None,
            block: false,
        }
    }
    pub fn size(mut self, size: InputSize) -> Self {
        self.size = size;
        self
    }
    pub fn status(mut self, status: InputStatus) -> Self {
        self.status = status;
        self
    }
    /// Name this spin button to assistive technology.
    ///
    /// GPUI Base publishes the spin button node inside [`gpui_kit::base::NumberInput`].
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }
    pub fn accessibility_description(mut self, text: impl Into<SharedString>) -> Self {
        self.accessibility_description = Some(text.into());
        self
    }
    /// Override stepping for a composed editor with a shared numeric model.
    pub fn on_step(
        mut self,
        step: impl Fn(gpui_kit::base::StepAction, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_step = Some(std::rc::Rc::new(step));
        self
    }
    /// Stack the two step controls in a compact trailing lane.
    pub fn controls_right(mut self) -> Self {
        self.controls_right = true;
        self
    }
    /// Hide the step icons while keeping numeric editing, Up/Down and accessibility actions.
    pub fn controls(mut self, visible: bool) -> Self {
        self.controls = visible;
        self
    }
    pub fn tab_stop(mut self, enabled: bool) -> Self {
        self.tab_stop = enabled;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }
    pub fn block(mut self) -> Self {
        self.block = true;
        self
    }
}

impl RenderOnce for YueNumberInput {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        use gpui_kit::base::input::InputEditorStyle;
        let (height, font_size) = match self.size {
            InputSize::Small => (
                crate::theme::keys::INPUT_HEIGHT_SM,
                crate::theme::keys::INPUT_FONT_SIZE_SM,
            ),
            InputSize::Medium => (
                crate::theme::keys::INPUT_HEIGHT_MD,
                crate::theme::keys::INPUT_FONT_SIZE_MD,
            ),
            InputSize::Large => (
                crate::theme::keys::INPUT_HEIGHT_LG,
                crate::theme::keys::INPUT_FONT_SIZE_LG,
            ),
        };
        let foreground = color(
            self.theme,
            if self.disabled {
                crate::theme::keys::INPUT_COLOR_DISABLED
            } else if self.status == InputStatus::Error {
                crate::theme::keys::ERROR_FOREGROUND
            } else {
                crate::theme::keys::INPUT_COLOR
            },
        );
        let state = self.state.clone();
        state.update(cx, |input, cx| {
            input.set_disabled(self.disabled, cx);
            input.set_text_align(TextAlign::Center, cx);
            input.set_editor_style(InputEditorStyle {
                foreground: foreground.into(),
                muted_foreground: color(self.theme, crate::theme::keys::INPUT_PLACEHOLDER_COLOR)
                    .into(),
                caret: color(self.theme, crate::theme::keys::INPUT_COLOR).into(),
                selection: self.theme.selection(),
                ..Default::default()
            });
        });
        let focused = state
            .read(cx)
            .presentation()
            .focus_handle()
            .is_focused(window);
        let border = border_token(self.status, self.disabled, focused);
        div()
            .id(self.id)
            .when(self.block, |this| this.w_full())
            .when_some(self.width, |this, width| this.w(width))
            .child(
                gpui_kit::base::NumberInput::new(&state)
                    // Base shapes ordinary text from the inherited TextStyle.
                    .text_color(foreground)
                    .controls(self.controls)
                    .tab_stop(self.tab_stop)
                    .when(self.controls_right, |b| b.controls_right())
                    .disabled(self.disabled)
                    .when_some(self.accessibility_label, |this, label| {
                        this.accessibility_label(label)
                    })
                    .when_some(self.accessibility_description, |this, text| {
                        this.accessibility_description(text)
                    })
                    .when_some(self.on_step, |this, handler| {
                        this.on_step(move |action, window, cx| handler(action, window, cx))
                    })
                    .input(gpui_kit::base::input::Input::new(&state))
                    .decrement_button(move |button| {
                        button
                            .size(rems(self.theme, font_size))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(rems(
                                self.theme,
                                crate::theme::keys::INPUT_STEPPER_BORDER_RADIUS,
                            ))
                            .when(!self.disabled, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(color(
                                            self.theme,
                                            crate::theme::keys::INPUT_STEPPER_BACKGROUND_HOVER,
                                        ))
                                    })
                                    .active(|style| {
                                        style.bg(color(
                                            self.theme,
                                            crate::theme::keys::INPUT_STEPPER_BACKGROUND_PRESSED,
                                        ))
                                    })
                            })
                            .child(
                                Icon::new(YueIcon::ChevronDown)
                                    .size(rems(self.theme, font_size))
                                    .color_value(color(
                                        self.theme,
                                        crate::theme::keys::INPUT_COLOR,
                                    )),
                            )
                    })
                    .increment_button(move |button| {
                        button
                            .size(rems(self.theme, font_size))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(rems(
                                self.theme,
                                crate::theme::keys::INPUT_STEPPER_BORDER_RADIUS,
                            ))
                            .when(!self.disabled, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(color(
                                            self.theme,
                                            crate::theme::keys::INPUT_STEPPER_BACKGROUND_HOVER,
                                        ))
                                    })
                                    .active(|style| {
                                        style.bg(color(
                                            self.theme,
                                            crate::theme::keys::INPUT_STEPPER_BACKGROUND_PRESSED,
                                        ))
                                    })
                            })
                            .child(
                                Icon::new(YueIcon::ChevronUp)
                                    .size(rems(self.theme, font_size))
                                    .color_value(color(
                                        self.theme,
                                        crate::theme::keys::INPUT_COLOR,
                                    )),
                            )
                    })
                    .text_size(rems(self.theme, font_size))
                    .h(rems(self.theme, height))
                    .px(rems(
                        self.theme,
                        crate::theme::keys::INPUT_NUMBER_PADDING_INLINE,
                    ))
                    .bg(color(
                        self.theme,
                        if self.disabled {
                            crate::theme::keys::INPUT_BACKGROUND_DISABLED
                        } else {
                            crate::theme::keys::INPUT_BACKGROUND
                        },
                    ))
                    .rounded(rems(self.theme, crate::theme::keys::INPUT_BORDER_RADIUS))
                    .border(rems(self.theme, crate::theme::keys::INPUT_BORDER_WIDTH))
                    .border_color(color(self.theme, border)),
            )
    }
}

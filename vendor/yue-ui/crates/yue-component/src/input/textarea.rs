//! Multiline input contract and composition.

use super::frame::*;
use super::*;

use gpui_kit::base::TestSupportExt as _;

use gpui_kit::{
    App, DefiniteLength, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, Styled as _, Window, div,
    prelude::FluentBuilder as _,
};

use crate::YueTheme;

use crate::tokens::{pixels as token_pixels, rems, rgba as color};

/// Yue-styled multi-line text editor backed by GPUI Base `TextareaState`.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueTextarea};
/// use yue_component::gpui_kit::{Entity, base::input::TextareaState};
/// # fn example(state: &Entity<TextareaState>) {
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let control = YueTextarea::new("field", theme, state).accessibility_label("名称");
/// # }
/// ```
#[derive(IntoElement)]
pub struct YueTextarea {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    state: Entity<gpui_kit::base::input::TextareaState>,
    status: InputStatus,
    accessibility_label: Option<SharedString>,
    accessibility_description: Option<SharedString>,
    max_length: Option<usize>,
    show_counter: bool,
    disabled: bool,
    readonly: bool,
    bordered: bool,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    block: bool,
}

impl YueTextarea {
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
        state: &Entity<gpui_kit::base::input::TextareaState>,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            state: state.clone(),
            status: InputStatus::Normal,
            accessibility_label: None,
            accessibility_description: None,
            max_length: None,
            show_counter: false,
            disabled: false,
            readonly: false,
            bordered: true,
            width: None,
            height: None,
            block: false,
        }
    }

    pub fn status(mut self, status: InputStatus) -> Self {
        self.status = status;
        self
    }
    /// Name this control to assistive technology when no [`crate::YueField`] label owns it.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }
    /// Announce a hint, requirement, or error together with this control.
    pub fn accessibility_description(mut self, description: impl Into<SharedString>) -> Self {
        self.accessibility_description = Some(description.into());
        self
    }
    /// Trim every edit that overshoots this character count.
    pub fn max_length(mut self, limit: usize) -> Self {
        self.max_length = Some(limit);
        self
    }
    /// Show the character count inside the frame.
    pub fn show_counter(mut self, show: bool) -> Self {
        self.show_counter = show;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }
    pub fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }
    pub fn height(mut self, height: impl Into<DefiniteLength>) -> Self {
        self.height = Some(height.into());
        self
    }
    pub fn block(mut self) -> Self {
        self.block = true;
        self
    }
}

impl RenderOnce for YueTextarea {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        use gpui_kit::base::input::InputEditorStyle;
        let state = self.state.clone();
        state.update(cx, |input, cx| {
            input.set_disabled(self.disabled, cx);
            input.set_readonly(self.readonly, cx);
            input.set_editor_style(InputEditorStyle {
                foreground: color(
                    self.theme,
                    if self.disabled {
                        crate::theme::keys::INPUT_COLOR_DISABLED
                    } else {
                        crate::theme::keys::INPUT_COLOR
                    },
                )
                .into(),
                muted_foreground: color(self.theme, crate::theme::keys::INPUT_PLACEHOLDER_COLOR)
                    .into(),
                caret: color(self.theme, crate::theme::keys::INPUT_COLOR).into(),
                selection: self.theme.selection(),
                ..Default::default()
            });
            input.set_editor_paddings(gpui_kit::Edges {
                top: gpui_kit::px(token_pixels(
                    self.theme,
                    crate::theme::keys::TEXTAREA_PADDING_BLOCK,
                )),
                right: gpui_kit::px(token_pixels(
                    self.theme,
                    crate::theme::keys::INPUT_PADDING_INLINE_MD,
                )),
                bottom: gpui_kit::px(token_pixels(
                    self.theme,
                    crate::theme::keys::TEXTAREA_PADDING_BLOCK,
                )),
                left: gpui_kit::px(token_pixels(
                    self.theme,
                    crate::theme::keys::INPUT_PADDING_INLINE_MD,
                )),
            });
        });
        if let Some(limit) = self.max_length {
            truncate_to_limit(&state, limit, window, cx);
        }
        let presentation = state.read(cx).presentation();
        let focused = presentation.focus_handle().is_focused(window);
        let value = state.read(cx).value();
        let border = border_token(self.status, self.disabled, focused);
        let mut frame = div()
            .id(self.id)
            .test_support()
            .flex()
            .items_start()
            .text_size(rems(self.theme, crate::theme::keys::INPUT_FONT_SIZE_MD))
            .bg(color(
                self.theme,
                if self.disabled {
                    crate::theme::keys::INPUT_BACKGROUND_DISABLED
                } else {
                    crate::theme::keys::INPUT_BACKGROUND
                },
            ))
            .rounded(rems(self.theme, crate::theme::keys::INPUT_BORDER_RADIUS))
            .border(if self.bordered {
                rems(self.theme, crate::theme::keys::INPUT_BORDER_WIDTH)
            } else {
                gpui_kit::rems(0.0)
            })
            .border_color(color(self.theme, border))
            .when(self.block, |this| this.w_full())
            .when_some(self.width, |this, width| this.w(width))
            .when_some(self.height, |this, height| this.h(height))
            .when(self.disabled, |this| this.opacity(0.72))
            .child(gpui_kit::base::input::Textarea::new(&state));
        if self.show_counter {
            frame = frame.child(
                div()
                    .self_end()
                    .pr(rems(
                        self.theme,
                        crate::theme::keys::INPUT_PADDING_INLINE_SM,
                    ))
                    .child(counter_slot(
                        self.theme,
                        value.chars().count(),
                        self.max_length,
                    )),
            );
        }
        let mut frame = accessible_frame(
            frame,
            FrameA11y {
                role: Role::MultilineTextInput,
                label: self.accessibility_label,
                description: self.accessibility_description,
                placeholder: Some(presentation.placeholder().clone())
                    .filter(|text| !text.is_empty()),
                value: Some(value),
                editable: !self.disabled && !self.readonly,
            },
            &state,
            presentation.focus_handle(),
        );
        if focused && self.status == InputStatus::Normal {
            frame = frame.border_color(color(
                self.theme,
                crate::theme::keys::INPUT_BORDER_COLOR_FOCUS,
            ));
        }
        frame
    }
}

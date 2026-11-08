//! Field layout and semantic forwarding to the actual control.

use super::*;

impl RenderOnce for YueField {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        let bound_error = match &self.form {
            Some((form, id)) => form.read(cx).error(id, cx),
            None => None,
        };
        let status = if bound_error.is_some() {
            InputStatus::Error
        } else {
            self.message_status
        };
        let message = bound_error.or(self.message.clone());
        // While the field has a message, its status is the control's status: a red
        // border never appears without the copy underneath that explains it.
        let child_status = message.is_some().then_some(status);
        let label = self.label.clone();
        let announced = announced(self.required, &self.description, &message);
        let child = self.child.map(|child| match child {
            FieldChild::Select(select) => (*select)
                .required(self.required)
                .when_some(label, |this, label| this.accessibility_label(label))
                .when_some(announced, |this, text| this.accessibility_description(text))
                .when_some(child_status, |this, status| this.status(status))
                .render(window, cx)
                .into_any_element(),
            FieldChild::Element(element) => element,
            FieldChild::Input(input) => input
                .when_some(label, |this, label| this.accessibility_label(label))
                .when_some(announced, |this, description| {
                    this.accessibility_description(description)
                })
                .when_some(child_status, |this, status| this.status(status))
                .render(window, cx)
                .into_any_element(),
            FieldChild::Textarea(textarea) => textarea
                .when_some(label, |this, label| this.accessibility_label(label))
                .when_some(announced, |this, description| {
                    this.accessibility_description(description)
                })
                .when_some(child_status, |this, status| this.status(status))
                .render(window, cx)
                .into_any_element(),
            FieldChild::Number(number) => number
                .when_some(label, |this, label| this.accessibility_label(label))
                .when_some(announced, |this, text| this.accessibility_description(text))
                .when_some(child_status, |this, status| this.status(status))
                .render(window, cx)
                .into_any_element(),
        });
        let message_color = field_message_token(status);
        div()
            .flex()
            .flex_col()
            .gap(rems(self.theme, crate::theme::keys::INPUT_GAP))
            .when_some(self.label, |this, label| {
                this.child(
                    div()
                        .text_color(color(self.theme, crate::theme::keys::INPUT_COLOR))
                        .child(label)
                        .when(self.required, |label| label.child(" *")),
                )
            })
            .when_some(self.description, |this, description| {
                this.child(
                    div()
                        .text_color(color(self.theme, crate::theme::keys::INPUT_AFFIX_COLOR))
                        .text_size(rems(self.theme, crate::theme::keys::INPUT_FONT_SIZE_SM))
                        .child(description),
                )
            })
            .when_some(child, |this, child| this.child(child))
            .when_some(message, |this, message| {
                this.child(
                    div()
                        .text_color(color(self.theme, message_color))
                        .text_size(rems(self.theme, crate::theme::keys::INPUT_FONT_SIZE_SM))
                        .child(message),
                )
            })
    }
}

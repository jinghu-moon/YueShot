use super::*;
use gpui_kit::{InteractiveElement as _, Length, TextRun, canvas};

/// A single-line text slot which explains its full text only when truncated.
#[derive(IntoElement)]
pub struct YueOverflowTooltip {
    id: ElementId,
    text: SharedString,
    theme: YueTheme,
    width: Option<Length>,
    max_width: Option<Length>,
    focus: Option<FocusHandle>,
}
impl YueOverflowTooltip {
    pub fn new(id: impl Into<ElementId>, theme: YueTheme, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            theme,
            width: None,
            max_width: None,
            focus: None,
        }
    }
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }
    pub fn max_width(mut self, width: impl Into<Length>) -> Self {
        self.max_width = Some(width.into());
        self
    }
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }
}
impl RenderOnce for YueOverflowTooltip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui_kit::prelude::FluentBuilder as _;
        let overflow = window
            .use_keyed_state((self.id.clone(), "overflow"), cx, |_, _| {
                Rc::new(Cell::new(false))
            })
            .read(cx)
            .clone();
        let style = window.text_style();
        let size = style.font_size.to_pixels(window.rem_size());
        let line = window.text_system().shape_line(
            self.text.clone(),
            size,
            &[TextRun {
                len: self.text.len(),
                font: style.font(),
                color: style.color,
                ..Default::default()
            }],
            None,
        );
        let natural = line.width();
        let observer = overflow.clone();
        let capture = canvas(
            move |bounds, window, cx| {
                let next = natural > bounds.size.width;
                if observer.replace(next) != next {
                    let observer = observer.clone();
                    let handle = window.window_handle();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            super::controller::availability_changed(&observer, window, cx);
                            window.refresh();
                        });
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let mut config = Config::new(self.text.clone(), self.theme, TooltipOptions::default());
        config.availability = Some(overflow);
        let attachment = super::controller::attach(self.id.clone(), config, self.focus, window, cx);
        div()
            .id(self.id)
            .relative()
            .min_w_0()
            .when_some(self.width, |d, w| d.w(w))
            .when_some(self.max_width, |d, w| d.max_w(w))
            .text_ellipsis()
            .child(self.text)
            .child(capture)
            .child(attachment)
    }
}

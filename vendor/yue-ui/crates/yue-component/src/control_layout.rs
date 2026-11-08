//! Layout-only refinements shared by controls. Visual state stays token-owned.
use gpui_kit::{DefiniteLength, Length, Styled};

#[derive(Clone, Default)]
pub(crate) struct ControlLayout {
    pub(crate) width: Option<DefiniteLength>,
    pub(crate) min_width: Option<Length>,
    pub(crate) max_width: Option<Length>,
    pub(crate) margin: Option<Length>,
    pub(crate) grow: Option<f32>,
}
impl ControlLayout {
    pub(crate) fn apply<T: Styled>(self, mut element: T) -> T {
        if let Some(value) = self.width {
            element = element.w(value);
        }
        if let Some(value) = self.min_width {
            element = element.min_w(value);
        }
        if let Some(value) = self.max_width {
            element = element.max_w(value);
        }
        if let Some(value) = self.margin {
            element = element.m(value);
        }
        if let Some(value) = self.grow {
            element.style().flex_grow = Some(value);
        }
        element
    }
}

macro_rules! control_layout {
    ($component:ty) => {
        impl $component {
            /// Set the control's outer width without changing its visual tokens.
            pub fn width(mut self, width: impl Into<gpui_kit::DefiniteLength>) -> Self {
                self.layout.width = Some(width.into());
                self
            }
            pub fn min_width(mut self, width: impl Into<gpui_kit::Length>) -> Self {
                self.layout.min_width = Some(width.into());
                self
            }
            pub fn max_width(mut self, width: impl Into<gpui_kit::Length>) -> Self {
                self.layout.max_width = Some(width.into());
                self
            }
            /// Space outside the control; internal padding remains token-owned.
            pub fn margin(mut self, margin: impl Into<gpui_kit::Length>) -> Self {
                self.layout.margin = Some(margin.into());
                self
            }
            pub fn flex_grow(mut self, grow: f32) -> Self {
                assert!(grow.is_finite() && grow >= 0.0);
                self.layout.grow = Some(grow);
                self
            }
        }
    };
}
pub(crate) use control_layout;

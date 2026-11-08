//! Tabler SVG icons for GPUI Kit applications.
//!
//! ```rust
//! use yue_component_icons::{Icon, outline};
//!
//! let search = Icon::new(outline::Search).size(gpui_kit::rems(1.0));
//! ```

use gpui_kit::{App, IntoElement, Rems, RenderOnce, Styled as _, Window, svg};

/// Static SVG data and identity implemented by each generated icon marker.
pub trait IconDefinition: Copy + 'static {
    fn name(self) -> &'static str;
    fn style(self) -> &'static str;
    fn svg(self) -> &'static [u8];
}

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

const _: () = {
    assert!(OUTLINE_COUNT > 5_000);
    assert!(FILLED_COUNT > 1_000);
};

/// Renders one Tabler SVG through GPUI's native SVG element.
#[derive(Clone, Copy, IntoElement)]
pub struct Icon {
    data: &'static [u8],
    size: Rems,
    color: Option<gpui_kit::Rgba>,
    rotation: Option<f32>,
}

impl Icon {
    pub fn new(icon: impl IconDefinition) -> Self {
        Self {
            data: icon.svg(),
            size: gpui_kit::rems(1.0),
            color: None,
            rotation: None,
        }
    }

    pub fn size(mut self, size: Rems) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: u32) -> Self {
        self.color = Some(gpui_kit::rgb(color));
        self
    }

    pub fn color_value(mut self, color: gpui_kit::Rgba) -> Self {
        self.color = Some(color);
        self
    }

    /// Rotate the drawing by a fraction of a full turn, about its own centre.
    /// Layout and the hit box keep the declared `size`.
    pub fn rotation(mut self, rotation: f32) -> Self {
        self.rotation = Some(rotation);
        self
    }

    pub const fn data(&self) -> &'static [u8] {
        self.data
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut icon = svg().data(self.data).size(self.size).flex_shrink_0();
        if let Some(rotation) = self.rotation {
            icon = icon.with_transformation(gpui_kit::Transformation::rotate(gpui_kit::radians(
                std::f32::consts::TAU * rotation,
            )));
        }
        if let Some(color) = self.color {
            icon.text_color(color)
        } else {
            icon
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Icon, IconDefinition};

    #[test]
    fn generated_outline_and_filled_assets_are_embedded() {
        let search = Icon::new(super::outline::Search);
        let filled = Icon::new(super::filled::Heart);
        assert!(std::str::from_utf8(search.data()).unwrap().contains("<svg"));
        assert!(std::str::from_utf8(filled.data()).unwrap().contains("<svg"));
        assert_eq!(super::outline::Search.name(), "search");
        assert_eq!(super::filled::Heart.style(), "filled");
    }
}

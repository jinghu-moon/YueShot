//! The icons Yue's own components draw, named by what they mean.
//!
//! Every built-in glyph is a Tabler outline icon from `yue-component-icons`,
//! pinned to the same Tabler release the web package pins. Components ask for
//! [`YueIcon::Close`], never for `outline::X`, so the meaning-to-glyph table
//! lives here once. Shared meanings follow the web's `YUE_ICONS` map
//! (`packages/vue/src/shared/icons.ts`); GPUI numeric controls additionally use
//! `ChevronUp`. The mapping is a documented contract, not shared source.
//!
//! Applications draw their own content with any icon they like; this table only
//! covers the glyphs Yue draws itself.

use gpui_kit::Rems;
use yue_component_icons::{IconDefinition, outline};

use crate::YueTheme;
use crate::tokens::{pixels, rems};

/// One of the glyphs Yue's components draw.
///
/// It is an [`IconDefinition`], so it goes anywhere a Tabler marker does:
/// `Icon::new(YueIcon::Search)`, `.leading_icon(YueIcon::Search)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum YueIcon {
    Close,
    ChevronDown,
    ChevronUp,
    ChevronRight,
    Search,
    Loading,
    Visible,
    Hidden,
    Check,
    Plus,
    Minus,
    Info,
    Success,
    Warning,
    Error,
}

impl YueIcon {
    pub const ALL: [Self; 15] = [
        Self::Close,
        Self::ChevronDown,
        Self::ChevronUp,
        Self::ChevronRight,
        Self::Search,
        Self::Loading,
        Self::Visible,
        Self::Hidden,
        Self::Check,
        Self::Plus,
        Self::Minus,
        Self::Info,
        Self::Success,
        Self::Warning,
        Self::Error,
    ];

    /// The name both projects use for this meaning, as in the design docs.
    pub fn semantic_name(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::ChevronDown => "chevron-down",
            Self::ChevronUp => "chevron-up",
            Self::ChevronRight => "chevron-right",
            Self::Search => "search",
            Self::Loading => "loading",
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Check => "check",
            Self::Plus => "plus",
            Self::Minus => "minus",
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    fn glyph(self) -> &'static dyn Glyph {
        match self {
            Self::Close => &outline::X,
            Self::ChevronDown => &outline::ChevronDown,
            Self::ChevronUp => &outline::ChevronUp,
            Self::ChevronRight => &outline::ChevronRight,
            Self::Search => &outline::Search,
            Self::Loading => &outline::Loader2,
            Self::Visible => &outline::Eye,
            Self::Hidden => &outline::EyeOff,
            Self::Check => &outline::Check,
            Self::Plus => &outline::Plus,
            Self::Minus => &outline::Minus,
            Self::Info => &outline::InfoCircle,
            Self::Success => &outline::CircleCheck,
            Self::Warning => &outline::AlertTriangle,
            Self::Error => &outline::AlertCircle,
        }
    }
}

/// [`IconDefinition`] is not object safe (it is `Copy`), so the table above
/// holds the generated markers behind this narrower view of them.
trait Glyph {
    fn name(&self) -> &'static str;
    fn svg(&self) -> &'static [u8];
}

impl<T: IconDefinition> Glyph for T {
    fn name(&self) -> &'static str {
        IconDefinition::name(*self)
    }

    fn svg(&self) -> &'static [u8] {
        IconDefinition::svg(*self)
    }
}

impl IconDefinition for YueIcon {
    /// The Tabler file the meaning resolves to, such as `x` for `Close`.
    fn name(self) -> &'static str {
        self.glyph().name()
    }

    fn style(self) -> &'static str {
        "outline"
    }

    fn svg(self) -> &'static [u8] {
        self.glyph().svg()
    }
}

/// The three icon steps, `--icon-size-sm|md|lg` (16, 20 and 24px).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IconSize {
    Small,
    Medium,
    Large,
}

impl IconSize {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    /// The step that sits beside text of `text_px`: 12 and 14px text take a
    /// 16px icon, 16px text a 20px icon, anything from 20px up a 24px icon.
    pub fn for_text(text_px: f32) -> Self {
        if text_px >= 20.0 {
            Self::Large
        } else if text_px >= 16.0 {
            Self::Medium
        } else {
            Self::Small
        }
    }

    pub fn token_name(self) -> &'static str {
        match self {
            Self::Small => "--icon-size-sm",
            Self::Medium => "--icon-size-md",
            Self::Large => "--icon-size-lg",
        }
    }

    pub fn pixels(self, theme: YueTheme) -> f32 {
        pixels(theme, self.token_name())
    }

    pub fn rems(self, theme: YueTheme) -> Rems {
        rems(theme, self.token_name())
    }
}

/// Tabler draws on a 24 grid with a stroke of 2, and the SVGs are fixed at
/// compile time, so the stroke an icon renders is always this share of its size.
pub fn rendered_stroke(icon_px: f32) -> f32 {
    icon_px * 2.0 / 24.0
}

#[cfg(test)]
mod tests {
    use super::{IconSize, YueIcon, rendered_stroke};
    use crate::{Accent, ThemeMode, YueTheme};
    use yue_component_icons::IconDefinition;

    /// Shared web meanings plus GPUI's numeric-step ChevronUp. Pin each glyph
    /// so an accidental semantic-to-asset substitution fails explicitly.
    const CONTRACT: [(&str, &str); 15] = [
        ("close", "x"),
        ("chevron-down", "chevron-down"),
        ("chevron-up", "chevron-up"),
        ("chevron-right", "chevron-right"),
        ("search", "search"),
        ("loading", "loader-2"),
        ("visible", "eye"),
        ("hidden", "eye-off"),
        ("check", "check"),
        ("plus", "plus"),
        ("minus", "minus"),
        ("info", "info-circle"),
        ("success", "circle-check"),
        ("warning", "alert-triangle"),
        ("error", "alert-circle"),
    ];

    #[test]
    fn every_meaning_resolves_to_the_documented_tabler_outline() {
        let table: Vec<_> = YueIcon::ALL
            .into_iter()
            .map(|icon| (icon.semantic_name(), icon.name()))
            .collect();
        assert_eq!(table, CONTRACT);
        for icon in YueIcon::ALL {
            assert_eq!(icon.style(), "outline");
            let svg = std::str::from_utf8(icon.svg()).unwrap();
            assert!(svg.contains("stroke-width=\"2\""), "{icon:?}");
            assert!(svg.contains("currentColor"), "{icon:?}");
        }
    }

    #[test]
    fn icon_sizes_pair_with_text_sizes() {
        let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
        let paired: Vec<_> = [12.0, 14.0, 16.0, 20.0, 24.0, 32.0]
            .into_iter()
            .map(|text| IconSize::for_text(text).pixels(theme))
            .collect();
        assert_eq!(paired, [16.0, 16.0, 20.0, 24.0, 24.0, 24.0]);
        assert_eq!(rendered_stroke(24.0), 2.0);
        assert!((rendered_stroke(16.0) * 1.5 - 2.0).abs() < 1e-6);
    }
}

mod paint;
pub(crate) use paint::{CHECK_LINE, MARK_BOX, paint_mark};

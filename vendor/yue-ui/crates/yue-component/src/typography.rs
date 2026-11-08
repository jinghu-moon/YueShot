//! Yue text metrics and Windows system font stacks for the product snapshot.
//!
//! Token shorthands supply weight, size and line height. Effective families use
//! Segoe UI / Microsoft YaHei UI and Consolas / Courier New, matching the
//! previously selected unbundled resource mode. No font binaries are embedded.

use gpui_kit::{FontFallbacks, FontFeatures, FontWeight, SharedString, Styled, px};

use crate::YueTheme;
use crate::tokens::token;

/// A CSS family stack as GPUI wants it: the first family to ask for, and the
/// rest as fallbacks.
///
/// Generic names such as `system-ui` and `sans-serif` name no installed font,
/// so they are dropped rather than handed to the platform as families.
#[derive(Clone, Debug, PartialEq)]
pub struct FontStack {
    pub family: SharedString,
    pub fallbacks: Vec<String>,
}

const GENERIC_FAMILIES: [&str; 8] = [
    "system-ui",
    "ui-monospace",
    "ui-sans-serif",
    "ui-serif",
    "sans-serif",
    "serif",
    "monospace",
    "cursive",
];

impl FontStack {
    pub fn parse(value: &str) -> Option<Self> {
        let mut families = value
            .split(',')
            .map(|family| family.trim().trim_matches(|c| c == '\'' || c == '"'))
            .filter(|family| !family.is_empty() && !GENERIC_FAMILIES.contains(family));
        let family = families.next()?.to_owned();
        Some(Self {
            family: family.into(),
            fallbacks: families.map(str::to_owned).collect(),
        })
    }

    /// The stack a `--font-*` token names.
    ///
    /// # Panics
    ///
    /// If `name` is not a token or names no concrete family.
    pub fn from_token(theme: YueTheme, name: &'static str) -> Self {
        Self::parse(token(theme, name).value())
            .unwrap_or_else(|| panic!("Yue token {name} names no font family"))
    }

    pub fn font_fallbacks(&self) -> Option<FontFallbacks> {
        (!self.fallbacks.is_empty()).then(|| FontFallbacks::from_fonts(self.fallbacks.clone()))
    }
}

/// The effective Windows interface family for `--font-ui`.
pub fn ui_font(_theme: YueTheme) -> FontStack {
    FontStack::parse("'Segoe UI', 'Microsoft YaHei UI'").expect("system font stack")
}

/// The effective Windows code family for `--font-mono`.
pub fn mono_font(_theme: YueTheme) -> FontStack {
    FontStack::parse("Consolas, 'Courier New'").expect("system font stack")
}

/// The effective Windows figures family for `--font-number`.
pub fn number_font(theme: YueTheme) -> FontStack {
    ui_font(theme)
}

/// Figures of one width, the CSS `font-variant-numeric: tabular-nums`.
///
/// A counter, a table column or a price that updates in place should not shift
/// sideways as its digits change.
pub fn tabular_nums() -> FontFeatures {
    FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]))
}

/// One of the `--type-*` text styles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextRole {
    Caption,
    Body,
    BodyLg,
    Label,
    Title,
    Heading3,
    Heading2,
    Heading1,
    Display,
    Code,
    Number,
    Figure,
}

impl TextRole {
    pub const ALL: [Self; 12] = [
        Self::Display,
        Self::Heading1,
        Self::Heading2,
        Self::Heading3,
        Self::Title,
        Self::BodyLg,
        Self::Body,
        Self::Label,
        Self::Caption,
        Self::Code,
        Self::Number,
        Self::Figure,
    ];

    pub fn token_name(self) -> &'static str {
        match self {
            Self::Caption => "--type-caption",
            Self::Body => "--type-body",
            Self::BodyLg => "--type-body-lg",
            Self::Label => "--type-label",
            Self::Title => "--type-title",
            Self::Heading3 => "--type-heading-3",
            Self::Heading2 => "--type-heading-2",
            Self::Heading1 => "--type-heading-1",
            Self::Display => "--type-display",
            Self::Code => "--type-code",
            Self::Number => "--type-number",
            Self::Figure => "--type-figure",
        }
    }

    /// The four values the role's shorthand carries.
    pub fn style(self, theme: YueTheme) -> TypeStyle {
        let name = self.token_name();
        let mut style = TypeStyle::parse(token(theme, name).value())
            .unwrap_or_else(|| panic!("Yue token {name} is not a font shorthand"));
        style.font = if self == Self::Code {
            mono_font(theme)
        } else {
            ui_font(theme)
        };
        style
    }

    /// Sets the role's family, weight, size and line height on an element.
    pub fn apply<E: Styled>(self, theme: YueTheme, element: E) -> E {
        self.style(theme).apply(element)
    }
}

/// A `font` shorthand read as the values GPUI styles with.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeStyle {
    pub weight: f32,
    pub size: f32,
    pub line_height: f32,
    pub font: FontStack,
}

impl TypeStyle {
    /// Reads `400 14px/20px 'Family', fallback, …` — the only order the
    /// `--type-*` tokens are written in.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let (weight, rest) = value.split_once(char::is_whitespace)?;
        let rest = rest.trim_start();
        let (metrics, families) = rest.split_once(char::is_whitespace)?;
        let (size, line_height) = metrics.split_once('/')?;
        let length = |value: &str| value.trim().strip_suffix("px")?.parse::<f32>().ok();
        Some(Self {
            weight: weight.parse().ok()?,
            size: length(size)?,
            line_height: length(line_height)?,
            font: FontStack::parse(families)?,
        })
    }

    pub fn apply<E: Styled>(&self, mut element: E) -> E {
        element.text_style().font_fallbacks = self.font.font_fallbacks();
        element
            .font_family(self.font.family.clone())
            .font_weight(FontWeight(self.weight))
            .text_size(gpui_kit::rems(self.size / 16.0))
            .line_height(px(self.line_height))
    }
}

#[cfg(test)]
mod tests {
    use super::{FontStack, TextRole, TypeStyle, mono_font, number_font, ui_font};
    use crate::{Accent, ThemeMode, YueTheme};

    fn theme() -> YueTheme {
        YueTheme::new(ThemeMode::Light, Accent::Azure)
    }

    #[test]
    fn a_family_stack_drops_quotes_and_generic_names() {
        let stack = FontStack::parse("'Fira Code', ui-monospace, Consolas, monospace").unwrap();
        assert_eq!(stack.family.as_ref(), "Fira Code");
        assert_eq!(stack.fallbacks, vec!["Consolas".to_owned()]);
        assert!(FontStack::parse("system-ui, sans-serif").is_none());
    }

    /// Upstream tokens are retained as design data; effective product families
    /// are resolved separately without distributing those upstream fonts.
    #[test]
    fn upstream_font_tokens_are_preserved_as_design_data() {
        assert_eq!(
            FontStack::from_token(theme(), "--font-ui").family.as_ref(),
            "HarmonyOS Sans SC"
        );
        assert_eq!(
            FontStack::from_token(theme(), "--font-mono")
                .family
                .as_ref(),
            "Fira Code"
        );
        let number = FontStack::from_token(theme(), "--font-number");
        assert_eq!(number.family.as_ref(), "TCloudNumber");
        assert_eq!(number.fallbacks[0], "HarmonyOS Sans SC");
    }

    #[test]
    fn effective_font_stacks_use_windows_system_families() {
        let ui = ui_font(theme());
        assert_eq!(ui.family.as_ref(), "Segoe UI");
        assert_eq!(ui.fallbacks, ["Microsoft YaHei UI"]);
        assert_eq!(mono_font(theme()).family.as_ref(), "Consolas");
        assert_eq!(mono_font(theme()).fallbacks, ["Courier New"]);
        assert_eq!(number_font(theme()), ui);
    }

    #[test]
    fn a_font_shorthand_reads_as_four_values() {
        let style = TypeStyle::parse("600 24px/32px 'HarmonyOS Sans SC', system-ui").unwrap();
        assert_eq!(
            (style.weight, style.size, style.line_height),
            (600.0, 24.0, 32.0)
        );
        assert_eq!(style.font.family.as_ref(), "HarmonyOS Sans SC");
    }

    #[test]
    fn every_text_role_resolves() {
        for role in TextRole::ALL {
            let style = role.style(theme());
            assert!(style.line_height >= style.size, "{role:?}");
        }
        let body = TextRole::Body.style(theme());
        assert_eq!(
            (body.weight, body.size, body.line_height),
            (400.0, 14.0, 20.0)
        );
        assert_eq!(TextRole::Label.style(theme()).weight, 500.0);
        assert_eq!(
            TextRole::Code.style(theme()).font.family.as_ref(),
            "Consolas"
        );
        assert_eq!(
            TextRole::Number.style(theme()).font.family.as_ref(),
            "Segoe UI"
        );
        // Preserve the upstream figure metrics while selecting the system family.
        let figure = TextRole::Figure.style(theme());
        assert_eq!(figure.font.family.as_ref(), "Segoe UI");
        assert_eq!((figure.weight, figure.size), (400.0, 24.0));
        assert_eq!(figure.font.fallbacks[0], "Microsoft YaHei UI");
    }
}

mod generated;
pub use generated::keys;

/// A generated token identity. Use [`keys`] instead of spelling CSS names in components.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenKey(usize);

impl TokenKey {
    pub fn name(self) -> &'static str {
        generated::TOKENS[self.0].0
    }
}

impl std::fmt::Display for TokenKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Precomputed values. Contextual CSS remains available verbatim and requires an
/// explicit adapter (font, shadow, currentColor, or another contextual capability).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TokenValue {
    /// Packed `0xRRGGBBAA` in sRGB.
    Color(u32),
    Pixels(f32),
    Number(f32),
    Milliseconds(f32),
    CubicBezier([f32; 4]),
    Contextual(&'static str),
}

/// A static key or a dynamic CSS name. Static keys resolve directly by index.
pub trait TokenSource: Copy + std::fmt::Display {
    fn key(self) -> Option<TokenKey>;
}
impl TokenSource for TokenKey {
    fn key(self) -> Option<TokenKey> {
        Some(self)
    }
}
impl TokenSource for &str {
    fn key(self) -> Option<TokenKey> {
        generated::TOKENS
            .binary_search_by_key(&self, |entry| entry.0)
            .ok()
            .map(TokenKey)
    }
}
impl TokenSource for &String {
    fn key(self) -> Option<TokenKey> {
        self.as_str().key()
    }
}

/// How much of `--selection-bg` shows under selected text; see [`YueTheme::selection`].
const SELECTION_ALPHA: f32 = 0.3;

/// Theme mode supported by the Yue design token profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeMode {
    Light,
    Dark,
}

/// The theme the user asked for, which may defer to the system.
///
/// [`ThemeMode`] is always a concrete theme; `System` is resolved against the
/// window's appearance, and again from `cx.observe_window_appearance` whenever
/// the system switches.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    /// The mode this preference lands on under `appearance`, usually
    /// `window.appearance()`. The vibrant appearances count as their plain ones.
    pub const fn resolve(self, appearance: gpui_kit::WindowAppearance) -> ThemeMode {
        match self {
            Self::Light => ThemeMode::Light,
            Self::Dark => ThemeMode::Dark,
            Self::System => match appearance {
                gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark => {
                    ThemeMode::Dark
                }
                gpui_kit::WindowAppearance::Light | gpui_kit::WindowAppearance::VibrantLight => {
                    ThemeMode::Light
                }
            },
        }
    }
}

/// Accent profile supported by the Yue design token profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Accent {
    Azure,
    Neutral,
}

/// Which neutral ladder the profile is built on, the `[data-neutral]` switch.
///
/// The tinted ladders match `Neutral` step for step in lightness, so contrast
/// barely moves; they only lend the greys a hue. Dark-mode text and border
/// greys are not on the ladder and stay achromatic.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NeutralTone {
    #[default]
    Neutral,
    /// Cool blue-grey.
    Slate,
    /// Warm grey.
    Stone,
    /// Violet-leaning grey.
    Haze,
}

impl NeutralTone {
    pub const ALL: [NeutralTone; 4] = [Self::Neutral, Self::Slate, Self::Stone, Self::Haze];

    /// The token family the tone reads its ladder from.
    pub const fn family(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Slate => "slate",
            Self::Stone => "stone",
            Self::Haze => "haze",
        }
    }

    fn overrides(self) -> &'static [(&'static str, [&'static str; 4])] {
        match self {
            Self::Neutral => &[],
            Self::Slate => &generated::SLATE_TOKENS,
            Self::Stone => &generated::STONE_TOKENS,
            Self::Haze => &generated::HAZE_TOKENS,
        }
    }
}

/// Stable semantic color roles shared by Yue components.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorRole {
    Page,
    Surface,
    SurfaceComponent,
    ButtonDefaultBackground,
    ButtonDefaultColor,
    ButtonPrimaryBackground,
    ButtonPrimaryColor,
    ButtonSecondaryBackground,
    ButtonSecondaryColor,
    ButtonSecondaryBorder,
    ButtonDangerBackground,
    ButtonDangerColor,
    TextPrimary,
    TextSecondary,
    BorderDefault,
    FocusRing,
    AccentSolid,
    AccentText,
    AccentSubtle,
    AccentSubtleText,
    OnAccent,
}

/// One resolved Yue Design System token for the selected theme profile.
///
/// The catalog keeps the original CSS value because the design system contains
/// colors, lengths, numbers, shadows, curves, and keywords.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct YueToken {
    name: &'static str,
    namespace: &'static str,
    layer: &'static str,
    value: &'static str,
    typed: TokenValue,
}

impl YueToken {
    pub const fn typed(self) -> TokenValue {
        self.typed
    }
    pub const fn name(self) -> &'static str {
        self.name
    }
    pub const fn namespace(self) -> &'static str {
        self.namespace
    }
    pub const fn layer(self) -> &'static str {
        self.layer
    }
    pub const fn value(self) -> &'static str {
        self.value
    }

    pub fn color(self) -> Option<u32> {
        if let TokenValue::Color(value) = self.typed {
            return ((value & 255) == 255).then_some(value >> 8);
        }
        parse_hex(self.value)
    }

    pub fn rgba(self) -> Option<gpui_kit::Rgba> {
        if let TokenValue::Color(value) = self.typed {
            return Some(gpui_kit::rgba(value));
        }
        if self.value == "transparent" {
            return Some(gpui_kit::rgba(0));
        }
        if let Some(color) = parse_hex(self.value) {
            return Some(gpui_kit::rgb(color));
        }
        let value = self
            .value
            .strip_prefix("color-mix(in srgb, ")?
            .strip_suffix(')')?;
        let (left, right) = value.split_once(", ")?;
        let (blend, amount) = if let Some((blend, amount)) = left.split_once(" calc(") {
            (blend, amount.strip_suffix("*100%)")?.parse::<f32>().ok()?)
        } else {
            let (blend, amount) = left.rsplit_once(' ')?;
            (blend, amount.strip_suffix('%')?.parse::<f32>().ok()? / 100.)
        };
        if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
            return None;
        }
        let blend = parse_hex(blend)?;
        if right == "transparent" {
            return Some(gpui_kit::rgba(
                (blend << 8) | ((amount * 255.0).round() as u32),
            ));
        }
        let background = parse_hex(right)?;
        Some(gpui_kit::rgb(mix_srgb(blend, background, amount)))
    }

    pub fn pixels(self) -> Option<f32> {
        if let TokenValue::Pixels(value) = self.typed {
            return Some(value);
        }
        let value = self.value.trim();
        if let Some(value) = value.strip_suffix("px") {
            return value.trim().parse().ok();
        }
        let value = value
            .strip_prefix("calc(")
            .and_then(|value| value.strip_suffix(')'))?;
        if let Some((left, right)) = value.split_once('/') {
            let left = left.trim().strip_suffix("px")?.trim().parse::<f32>().ok()?;
            let right = right.trim().parse::<f32>().ok()?;
            return Some(left / right);
        }
        if let Some((left, right)) = value.split_once('*') {
            let left = left.trim().strip_suffix("px")?.trim().parse::<f32>().ok()?;
            let right = right.trim().parse::<f32>().ok()?;
            return Some(left * right);
        }
        None
    }

    pub fn number(self) -> Option<f32> {
        if let TokenValue::Number(value) = self.typed {
            return Some(value);
        }
        self.value.parse().ok()
    }
}

fn parse_hex(value: &str) -> Option<u32> {
    let value = value.strip_prefix('#')?;
    let value = match value.len() {
        3 => value
            .chars()
            .map(|channel| format!("{channel}{channel}"))
            .collect::<String>(),
        6 => value.to_owned(),
        _ => return None,
    };
    u32::from_str_radix(&value, 16).ok()
}

/// Resolved Yue semantic colors for a theme/accent profile.
///
/// Values are packed RGB (`0xRRGGBB`) generated from the Yue Design System's
/// Token inventory. Convert to GPUI colors with `gpui_kit::rgb(theme.page())`.
#[derive(Clone, Copy, Debug)]
pub struct YueTheme {
    mode: ThemeMode,
    accent: Accent,
    neutral: NeutralTone,
    colors: [u32; 21],
    hover_opacity: f32,
    pressed_opacity: f32,
}

impl YueTheme {
    pub const fn new(mode: ThemeMode, accent: Accent) -> Self {
        let colors = match (mode, accent) {
            (ThemeMode::Light, Accent::Azure) => generated::LIGHT_AZURE,
            (ThemeMode::Dark, Accent::Azure) => generated::DARK_AZURE,
            (ThemeMode::Light, Accent::Neutral) => generated::LIGHT_NEUTRAL,
            (ThemeMode::Dark, Accent::Neutral) => generated::DARK_NEUTRAL,
        };
        let (hover_opacity, pressed_opacity) = match (mode, accent) {
            (ThemeMode::Light, Accent::Azure) => (
                generated::LIGHT_AZURE_HOVER_OPACITY,
                generated::LIGHT_AZURE_PRESSED_OPACITY,
            ),
            (ThemeMode::Dark, Accent::Azure) => (
                generated::DARK_AZURE_HOVER_OPACITY,
                generated::DARK_AZURE_PRESSED_OPACITY,
            ),
            (ThemeMode::Light, Accent::Neutral) => (
                generated::LIGHT_NEUTRAL_HOVER_OPACITY,
                generated::LIGHT_NEUTRAL_PRESSED_OPACITY,
            ),
            (ThemeMode::Dark, Accent::Neutral) => (
                generated::DARK_NEUTRAL_HOVER_OPACITY,
                generated::DARK_NEUTRAL_PRESSED_OPACITY,
            ),
        };
        Self {
            mode,
            accent,
            neutral: NeutralTone::Neutral,
            colors,
            hover_opacity,
            pressed_opacity,
        }
    }

    /// The same profile on another neutral ladder.
    pub const fn with_neutral(mut self, neutral: NeutralTone) -> Self {
        use generated::*;
        self.neutral = neutral;
        self.colors = match (neutral, self.mode, self.accent) {
            (NeutralTone::Neutral, mode, accent) => Self::new(mode, accent).colors,
            (NeutralTone::Slate, ThemeMode::Light, Accent::Azure) => SLATE_LIGHT_AZURE,
            (NeutralTone::Slate, ThemeMode::Dark, Accent::Azure) => SLATE_DARK_AZURE,
            (NeutralTone::Slate, ThemeMode::Light, Accent::Neutral) => SLATE_LIGHT_NEUTRAL,
            (NeutralTone::Slate, ThemeMode::Dark, Accent::Neutral) => SLATE_DARK_NEUTRAL,
            (NeutralTone::Stone, ThemeMode::Light, Accent::Azure) => STONE_LIGHT_AZURE,
            (NeutralTone::Stone, ThemeMode::Dark, Accent::Azure) => STONE_DARK_AZURE,
            (NeutralTone::Stone, ThemeMode::Light, Accent::Neutral) => STONE_LIGHT_NEUTRAL,
            (NeutralTone::Stone, ThemeMode::Dark, Accent::Neutral) => STONE_DARK_NEUTRAL,
            (NeutralTone::Haze, ThemeMode::Light, Accent::Azure) => HAZE_LIGHT_AZURE,
            (NeutralTone::Haze, ThemeMode::Dark, Accent::Azure) => HAZE_DARK_AZURE,
            (NeutralTone::Haze, ThemeMode::Light, Accent::Neutral) => HAZE_LIGHT_NEUTRAL,
            (NeutralTone::Haze, ThemeMode::Dark, Accent::Neutral) => HAZE_DARK_NEUTRAL,
        };
        self
    }

    pub const fn neutral(self) -> NeutralTone {
        self.neutral
    }

    pub const fn mode(self) -> ThemeMode {
        self.mode
    }

    pub const fn accent(self) -> Accent {
        self.accent
    }

    fn profile_index(self) -> usize {
        match (self.mode, self.accent) {
            (ThemeMode::Light, Accent::Azure) => 0,
            (ThemeMode::Dark, Accent::Azure) => 1,
            (ThemeMode::Light, Accent::Neutral) => 2,
            (ThemeMode::Dark, Accent::Neutral) => 3,
        }
    }

    /// Looks up any token from the complete generated Yue catalog.
    pub fn token(self, name: impl TokenSource) -> Option<YueToken> {
        name.key().map(|key| self.resolve(key))
    }

    fn resolve(self, key: TokenKey) -> YueToken {
        let (name, namespace, layer, values) = &generated::TOKENS[key.0];
        // Desktop hairlines need more contrast than the web inventory's faint
        // divider. Keep this product mapping here, outside the source snapshot,
        // so named lookups and static keys share the same tone-aware palette.
        if key == keys::DIVIDER_BORDER_COLOR {
            let color = self.resolve(match self.mode {
                ThemeMode::Light => keys::NEUTRAL_400,
                ThemeMode::Dark => keys::NEUTRAL_700,
            });
            return YueToken {
                name,
                namespace,
                layer,
                value: color.value,
                typed: color.typed,
            };
        }
        let profile = self.profile_index();
        let overrides = self.neutral.overrides();
        let (value, typed) = if let Ok(index) =
            overrides.binary_search_by(|(override_name, _)| override_name.cmp(name))
        {
            let typed = match self.neutral {
                NeutralTone::Slate => generated::SLATE_VALUES[index][profile],
                NeutralTone::Stone => generated::STONE_VALUES[index][profile],
                NeutralTone::Haze => generated::HAZE_VALUES[index][profile],
                NeutralTone::Neutral => unreachable!("the default tone has no overrides"),
            };
            (overrides[index].1[profile], typed)
        } else {
            (values[profile], generated::VALUES[key.0][profile])
        };
        YueToken {
            name,
            namespace,
            layer,
            value,
            typed,
        }
    }

    /// Base's global theme with this profile projected onto it.
    ///
    /// Base paints a few things no Yue component owns, out of its global
    /// `Theme`: scrollbar thumbs, and the selection of an input that leaves it
    /// unset. Left alone that theme stays Base's own light palette, so a dark
    /// Yue window gets a black thumb on a black surface. Base's text nodes also
    /// read their families from it, so the sans and mono families are Yue's;
    /// spacing, radius and the type ramp keep Base's values, since Yue
    /// components carry their own.
    pub fn base_theme(self) -> gpui_kit::base::Theme {
        let color = |name: &str| -> gpui_kit::Hsla {
            self.token(name)
                .and_then(YueToken::rgba)
                .unwrap_or_else(|| panic!("Yue token {name} is not a supported color"))
                .into()
        };
        let colors = gpui_kit::base::ColorTokens {
            background: color("--page"),
            foreground: color("--text-primary"),
            surface: color("--surface"),
            surface_foreground: color("--text-primary"),
            primary: color("--accent-solid"),
            primary_foreground: color("--on-accent"),
            secondary: color("--surface-component"),
            secondary_foreground: color("--text-primary"),
            muted: color("--surface-subtle"),
            muted_foreground: color("--text-secondary"),
            accent: color("--accent-subtle"),
            accent_foreground: color("--accent-subtle-text"),
            destructive: color("--action-danger"),
            destructive_foreground: color("--action-on-danger"),
            border: color("--border-default"),
            input: color("--border-control"),
            ring: color("--focus-ring-color"),
            selection: self.selection(),
        };
        let mut theme = gpui_kit::base::Theme {
            appearance: match self.mode {
                ThemeMode::Light => gpui_kit::base::ThemeAppearance::Light,
                ThemeMode::Dark => gpui_kit::base::ThemeAppearance::Dark,
            },
            ..Default::default()
        };
        theme.tokens.colors = colors;
        theme.tokens.typography.sans = crate::typography::ui_font(self).family;
        theme.tokens.typography.mono = crate::typography::mono_font(self).family;
        theme
    }

    /// Installs [`Self::base_theme`] as the application's Base theme.
    ///
    /// Call it at startup and again whenever the app switches profile; the
    /// rest of Base's global theme, such as scrollbar mode, is kept.
    pub fn install_base_theme(self, cx: &mut gpui_kit::App) {
        let installed = self.base_theme();
        let theme = gpui_kit::base::Theme::global_mut(cx);
        theme.appearance = installed.appearance;
        theme.tokens.colors = installed.tokens.colors;
        theme.tokens.typography.sans = installed.tokens.typography.sans;
        theme.tokens.typography.mono = installed.tokens.typography.mono;
    }

    /// The wash painted under selected text.
    ///
    /// The web paints `--selection-bg` solid and recolours the glyphs with
    /// `--selection-foreground`. GPUI paints the selection beneath glyphs it
    /// cannot recolour, so a solid fill would bury dark text; the same colour
    /// as a translucent wash keeps the hue and leaves the text legible.
    pub fn selection(self) -> gpui_kit::Hsla {
        let selection: gpui_kit::Hsla = self
            .token("--selection-bg")
            .and_then(YueToken::rgba)
            .expect("Yue token --selection-bg is a color")
            .into();
        selection.alpha(SELECTION_ALPHA)
    }

    /// Iterates over every resolved token in the selected theme profile.
    pub fn all_tokens(self) -> impl Iterator<Item = YueToken> {
        generated::TOKENS
            .iter()
            .enumerate()
            .map(move |(index, _)| self.resolve(TokenKey(index)))
    }

    pub const fn token_count(self) -> usize {
        generated::TOKEN_COUNT
    }

    pub const fn color(self, role: ColorRole) -> u32 {
        let index = match role {
            ColorRole::Page => 0,
            ColorRole::Surface => 1,
            ColorRole::SurfaceComponent => 2,
            ColorRole::ButtonDefaultBackground => 3,
            ColorRole::ButtonDefaultColor => 4,
            ColorRole::ButtonPrimaryBackground => 5,
            ColorRole::ButtonPrimaryColor => 6,
            ColorRole::ButtonSecondaryBackground => 7,
            ColorRole::ButtonSecondaryColor => 8,
            ColorRole::ButtonSecondaryBorder => 9,
            ColorRole::ButtonDangerBackground => 10,
            ColorRole::ButtonDangerColor => 11,
            ColorRole::TextPrimary => 12,
            ColorRole::TextSecondary => 13,
            ColorRole::BorderDefault => 14,
            ColorRole::FocusRing => 15,
            ColorRole::AccentSolid => 16,
            ColorRole::AccentText => 17,
            ColorRole::AccentSubtle => 18,
            ColorRole::AccentSubtleText => 19,
            ColorRole::OnAccent => 20,
        };
        self.colors[index]
    }

    pub const fn page(self) -> u32 {
        self.color(ColorRole::Page)
    }

    pub const fn surface(self) -> u32 {
        self.color(ColorRole::Surface)
    }

    pub const fn surface_component(self) -> u32 {
        self.color(ColorRole::SurfaceComponent)
    }

    pub const fn button_default_background(self) -> u32 {
        self.color(ColorRole::ButtonDefaultBackground)
    }
    pub const fn button_default_color(self) -> u32 {
        self.color(ColorRole::ButtonDefaultColor)
    }
    pub const fn button_primary_background(self) -> u32 {
        self.color(ColorRole::ButtonPrimaryBackground)
    }
    pub const fn button_primary_color(self) -> u32 {
        self.color(ColorRole::ButtonPrimaryColor)
    }
    pub const fn button_secondary_background(self) -> u32 {
        self.color(ColorRole::ButtonSecondaryBackground)
    }
    pub const fn button_secondary_color(self) -> u32 {
        self.color(ColorRole::ButtonSecondaryColor)
    }
    pub const fn button_secondary_border(self) -> u32 {
        self.color(ColorRole::ButtonSecondaryBorder)
    }
    pub const fn button_danger_background(self) -> u32 {
        self.color(ColorRole::ButtonDangerBackground)
    }
    pub const fn button_danger_color(self) -> u32 {
        self.color(ColorRole::ButtonDangerColor)
    }
    pub const fn hover_opacity(self) -> f32 {
        self.hover_opacity
    }
    pub const fn pressed_opacity(self) -> f32 {
        self.pressed_opacity
    }

    pub const fn text_primary(self) -> u32 {
        self.color(ColorRole::TextPrimary)
    }

    pub const fn text_secondary(self) -> u32 {
        self.color(ColorRole::TextSecondary)
    }

    pub const fn border_default(self) -> u32 {
        self.color(ColorRole::BorderDefault)
    }

    pub const fn focus_ring(self) -> u32 {
        self.color(ColorRole::FocusRing)
    }

    pub const fn accent_solid(self) -> u32 {
        self.color(ColorRole::AccentSolid)
    }

    pub const fn accent_text(self) -> u32 {
        self.color(ColorRole::AccentText)
    }

    pub const fn accent_subtle(self) -> u32 {
        self.color(ColorRole::AccentSubtle)
    }

    pub const fn accent_subtle_text(self) -> u32 {
        self.color(ColorRole::AccentSubtleText)
    }

    pub const fn on_accent(self) -> u32 {
        self.color(ColorRole::OnAccent)
    }
}

pub(crate) fn mix_srgb(foreground: u32, background: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let foreground = ((foreground >> shift) & 0xff_u32) as f32;
        let background = ((background >> shift) & 0xff_u32) as f32;
        (foreground * amount + background * (1.0 - amount)).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn form_status_text_clears_body_contrast_in_every_profile_and_tone() {
        let luminance = |color: gpui_kit::Rgba| {
            let linear = |channel: f32| {
                if channel <= 0.04045 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
        };
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            for accent in [Accent::Azure, Accent::Neutral] {
                for tone in NeutralTone::ALL {
                    let theme = YueTheme::new(mode, accent).with_neutral(tone);
                    for foreground in [super::keys::ERROR, super::keys::WARNING] {
                        for background in [super::keys::SURFACE, super::keys::PAGE] {
                            let fg = luminance(theme.token(foreground).unwrap().rgba().unwrap());
                            let bg = luminance(theme.token(background).unwrap().rgba().unwrap());
                            let ratio = (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05);
                            assert!(
                                ratio >= 4.5,
                                "{mode:?}/{accent:?}/{tone:?}: {foreground} on {background}: {ratio}"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn generated_keys_values_and_dynamic_names_agree_in_every_profile() {
        use super::{NeutralTone, TokenSource, TokenValue};
        let names: Vec<_> = super::generated::TOKENS.iter().map(|row| row.0).collect();
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            for accent in [Accent::Azure, Accent::Neutral] {
                for tone in NeutralTone::ALL {
                    let theme = YueTheme::new(mode, accent).with_neutral(tone);
                    for token in theme.all_tokens() {
                        assert_eq!(
                            theme.token(token.name()),
                            theme.token(token.name().key().unwrap())
                        );
                        match token.typed() {
                            TokenValue::Color(value) => {
                                let legacy = super::YueToken {
                                    typed: TokenValue::Contextual(token.value()),
                                    ..token
                                };
                                assert_eq!(
                                    legacy.rgba(),
                                    Some(gpui_kit::rgba(value)),
                                    "{}",
                                    token.name()
                                );
                            }
                            TokenValue::Number(value) => assert!(value.is_finite()),
                            TokenValue::Pixels(value) | TokenValue::Milliseconds(value) => {
                                assert!(value.is_finite())
                            }
                            TokenValue::CubicBezier(points) => {
                                assert!(points.iter().all(|v| v.is_finite()))
                            }
                            TokenValue::Contextual(value) => assert_eq!(value, token.value()),
                        }
                    }
                }
            }
        }
        assert!(
            YueTheme::new(ThemeMode::Light, Accent::Azure)
                .token("--missing-token")
                .is_none()
        );
    }
    use super::{Accent, ColorRole, NeutralTone, ThemeMode, ThemePreference, YueTheme};
    use gpui_kit::WindowAppearance;

    #[test]
    fn system_preference_follows_window_appearance() {
        let system = ThemePreference::System;
        assert_eq!(system.resolve(WindowAppearance::Light), ThemeMode::Light);
        assert_eq!(
            system.resolve(WindowAppearance::VibrantLight),
            ThemeMode::Light
        );
        assert_eq!(system.resolve(WindowAppearance::Dark), ThemeMode::Dark);
        assert_eq!(
            system.resolve(WindowAppearance::VibrantDark),
            ThemeMode::Dark
        );
        assert_eq!(ThemePreference::default(), ThemePreference::System);
    }

    #[test]
    fn explicit_preference_ignores_window_appearance() {
        for appearance in [WindowAppearance::Light, WindowAppearance::Dark] {
            assert_eq!(ThemePreference::Light.resolve(appearance), ThemeMode::Light);
            assert_eq!(ThemePreference::Dark.resolve(appearance), ThemeMode::Dark);
        }
    }

    #[test]
    fn all_yue_profiles_resolve_accent_and_surface_roles() {
        let light_azure = YueTheme::new(ThemeMode::Light, Accent::Azure);
        let dark_azure = YueTheme::new(ThemeMode::Dark, Accent::Azure);
        let light_neutral = YueTheme::new(ThemeMode::Light, Accent::Neutral);
        let dark_neutral = YueTheme::new(ThemeMode::Dark, Accent::Neutral);

        assert_eq!(light_azure.color(ColorRole::Page), 0xf5f5f5);
        assert_eq!(dark_azure.color(ColorRole::Page), 0x121212);
        assert_eq!(light_azure.accent_solid(), 0x125fb8);
        // Dark takes the 400 step with a near-black label, the status fills' rule.
        assert_eq!(dark_azure.accent_solid(), 0x60a0f5);
        assert_eq!(dark_azure.on_accent(), 0x121212);
        assert_eq!(dark_azure.focus_ring(), 0x93c1ff);
        assert_eq!(light_neutral.accent_solid(), 0x1f1f1f);
        assert_eq!(dark_neutral.accent_solid(), 0xf5f5f5);
        assert_eq!(dark_neutral.on_accent(), 0x121212);
        assert_eq!(light_azure.button_danger_background(), 0xa14138);
        assert_eq!(light_azure.button_danger_color(), 0xffffff);
        assert_eq!(dark_azure.button_danger_background(), 0xea7d73);
        assert_eq!(dark_azure.button_danger_color(), 0x121212);
        assert_eq!(
            light_azure.token("--error-solid").unwrap().color(),
            Some(0xa14138)
        );
        assert_eq!(
            dark_azure.token("--error-solid").unwrap().color(),
            Some(0xea7d73)
        );
        assert_eq!(
            light_azure.token("--on-error-solid").unwrap().color(),
            Some(0xffffff)
        );
        assert_eq!(
            dark_azure.token("--on-error-solid").unwrap().color(),
            Some(0x121212)
        );
        assert_eq!(light_azure.all_tokens().count(), light_azure.token_count());
        assert!(light_azure.token_count() >= 646);
        let spacing = light_azure.token("--space-8").expect("space token");
        assert_eq!(spacing.namespace(), "space");
        assert_eq!(spacing.layer(), "primitives");
        assert_eq!(spacing.value(), "8px");
        assert_eq!(spacing.pixels(), Some(8.0));
        assert_eq!(
            light_azure.token("--accent-600").unwrap().color(),
            Some(0x1f75db)
        );
    }

    /// Dark surfaces sit on the neutral ladder now, so a tinted ladder moves
    /// them too; the accent and the dark text greys do not move.
    #[test]
    fn a_tinted_ladder_swaps_every_neutral_step() {
        let dark = YueTheme::new(ThemeMode::Dark, Accent::Azure);
        let slate = dark.with_neutral(NeutralTone::Slate);
        assert_eq!(dark.surface(), 0x181818);
        assert_eq!(dark.surface_component(), 0x2b2b2b);
        assert_eq!(slate.surface(), 0x131920);
        assert_eq!(slate.surface_component(), 0x242c36);
        assert_eq!(slate.accent_solid(), dark.accent_solid());
        assert_eq!(slate.text_primary(), dark.text_primary());
        assert_eq!(slate.token("--surface").unwrap().color(), Some(0x131920));
        assert_eq!(
            slate.token("--neutral-500").unwrap().color(),
            Some(0x99a4b2)
        );
        assert_eq!(slate.token("--slate-500").unwrap().color(), Some(0x99a4b2));

        let stone =
            YueTheme::new(ThemeMode::Light, Accent::Neutral).with_neutral(NeutralTone::Stone);
        assert_eq!(stone.accent_solid(), 0x231e18);
        assert_eq!(
            stone.with_neutral(NeutralTone::Neutral).accent_solid(),
            0x1f1f1f
        );
    }

    /// Base paints scrollbars and unset selections from its global theme;
    /// the projected one has to follow the Yue profile, dark included.
    #[test]
    fn the_base_theme_follows_the_profile() {
        let rgb = |value: gpui_kit::Hsla| {
            let value = value.to_rgb();
            (
                (value.r * 255.0).round() as u32,
                (value.g * 255.0).round() as u32,
                (value.b * 255.0).round() as u32,
            )
        };
        let light = YueTheme::new(ThemeMode::Light, Accent::Azure).base_theme();
        let dark = YueTheme::new(ThemeMode::Dark, Accent::Azure).base_theme();
        assert_eq!(light.appearance, gpui_kit::base::ThemeAppearance::Light);
        assert_eq!(dark.appearance, gpui_kit::base::ThemeAppearance::Dark);
        assert_eq!(rgb(dark.tokens.colors.foreground), (0xf5, 0xf5, 0xf5));
        assert_eq!(rgb(dark.tokens.colors.surface), (0x18, 0x18, 0x18));
        assert_eq!(rgb(light.tokens.colors.selection), (0x12, 0x5f, 0xb8));
        assert!((light.tokens.colors.selection.a - 0.3).abs() < 1e-6);
    }
}

use super::element::*;
use crate::tokens::{duration as token_duration, mix_rgba};

use super::*;
use crate::tokens::pixels as token_pixels;
use crate::{Accent, ThemeMode, YueTheme};
use std::time::Duration;
use yue_component_icons::{filled, outline};

#[test]
fn constructors_expose_three_content_forms_and_icon_position() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    assert!(matches!(
        YueButton::text("text", theme, "Save").content,
        ButtonContent::Text(_)
    ));
    assert!(matches!(
        YueButton::icon("search", theme, outline::Search, "Search").content,
        ButtonContent::Icon { .. }
    ));
    let icon_text = YueButton::icon_text("add", theme, filled::Heart, "Favorite")
        .icon_position(IconPosition::Trailing);
    assert!(matches!(icon_text.content, ButtonContent::IconText { .. }));
    assert_eq!(icon_text.icon_position, IconPosition::Trailing);
}

#[test]
#[should_panic(expected = "icon-only YueButton requires an accessible name")]
fn icon_only_button_rejects_an_empty_accessible_name() {
    YueButton::icon(
        "search",
        YueTheme::new(ThemeMode::Light, Accent::Azure),
        outline::Search,
        "",
    );
}

#[test]
fn builder_options_retain_requested_states() {
    let button = YueButton::text(
        "options",
        YueTheme::new(ThemeMode::Light, Accent::Azure),
        "Run",
    )
    .variant(ButtonVariant::Danger)
    .size(ButtonSize::Large)
    .shape(ButtonShape::Circle)
    .loading(true)
    .block()
    .tooltip("Runs task");
    assert_eq!(button.variant, ButtonVariant::Danger);
    assert_eq!(button.size, ButtonSize::Large);
    assert_eq!(button.shape, ButtonShape::Circle);
    assert!(button.loading && button.block && button.tooltip.is_some());

    let pill = YueButton::text(
        "pill",
        YueTheme::new(ThemeMode::Light, Accent::Azure),
        "Pill",
    )
    .shape(ButtonShape::Pill);
    assert_eq!(pill.shape, ButtonShape::Pill);
}

#[test]
fn button_presentation_reads_component_tokens() {
    let light = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let dark = YueTheme::new(ThemeMode::Dark, Accent::Azure);

    assert_eq!(
        token_pixels(light, crate::theme::keys::BUTTON_HEIGHT_SM),
        28.0
    );
    assert_eq!(
        token_pixels(light, crate::theme::keys::BUTTON_HEIGHT_MD),
        32.0
    );
    assert_eq!(
        token_pixels(light, crate::theme::keys::BUTTON_HEIGHT_LG),
        40.0
    );
    assert_eq!(
        token_pixels(light, crate::theme::keys::BUTTON_BORDER_RADIUS),
        4.0
    );
    assert_eq!(token_pixels(light, crate::theme::keys::BUTTON_GAP), 8.0);
    assert_eq!(
        token_duration(light, crate::theme::keys::BUTTON_DURATION),
        Duration::from_millis(100)
    );
    assert_eq!(
        token_duration(light, crate::theme::keys::BUTTON_SPINNER_DURATION),
        Duration::from_millis(900)
    );
    assert_eq!(token_color(light, "--button-danger-background"), 0xa14138);
    assert_eq!(token_color(dark, "--button-danger-background"), 0xea7d73);
    assert!(token_rgba(light, crate::theme::keys::BUTTON_DANGER_BACKGROUND_HOVER).a > 0.99);
    assert!(token_rgba(light, crate::theme::keys::BUTTON_DISABLED_BACKGROUND).a < 1.0);
    assert!(token_rgba(light, crate::theme::keys::BUTTON_DISABLED_COLOR).a < 1.0);
    let _ = token_ease(light, crate::theme::keys::BUTTON_EASE);
    let text_foreground = token_color(light, "--button-link-color");
    let text_hover = mix_rgba(gpui_kit::rgba(0), gpui_kit::rgb(text_foreground), 0.08);
    assert!((text_hover.a - 0.08).abs() < 0.01);
}

#[test]
fn button_group_defaults_to_the_button_gap_token() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let group = ButtonGroup::new(theme);
    assert_eq!(group.gap.0, 0.5);
}

use crate::tokens::{easing as token_ease, rgba as token_rgba};

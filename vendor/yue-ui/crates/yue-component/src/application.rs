//! Application defaults. The consumer continues to own windows and state.
use crate::{Accent, ThemeMode, YueTheme};
use gpui_kit::{App, Global};

struct ApplicationTheme(YueTheme);
impl Global for ApplicationTheme {}
struct ComponentsInitialized;
impl Global for ComponentsInitialized {}

/// Initializes enabled Kit layers and component behavior once, then installs
/// Yue's application theme. Call before opening windows.
pub fn init(cx: &mut App, theme: YueTheme) -> gpui_kit::Result<()> {
    if cx.try_global::<gpui_kit::base::GlobalState>().is_none() {
        gpui_kit::init(cx);
    }
    if cx.try_global::<ComponentsInitialized>().is_none() {
        crate::popup::init(cx);
        crate::dropdown::init(cx);
        crate::tooltip::init(cx);
        crate::dialog::init(cx);
        crate::toast::init(cx);
        crate::select::init(cx);
        crate::image_preview::init(cx);
        cx.set_global(ComponentsInitialized);
    }
    set_theme(cx, theme);
    Ok(())
}

/// Updates the application default and Base's global colors/families, then
/// invalidates windows. Call from an update/event, never from `Render`.
/// Base owns one application-wide theme: independent window palettes require
/// explicit Yue overrides and cannot independently theme Base global consumers.
pub fn set_theme(cx: &mut App, theme: YueTheme) {
    theme.install_base_theme(cx);
    cx.set_global(ApplicationTheme(theme));
    cx.refresh_windows();
}

/// The application default, or Light/Azure before initialization.
pub fn current_theme(cx: &App) -> YueTheme {
    cx.try_global::<ApplicationTheme>().map_or_else(
        || YueTheme::new(ThemeMode::Light, Accent::Azure),
        |theme| theme.0,
    )
}

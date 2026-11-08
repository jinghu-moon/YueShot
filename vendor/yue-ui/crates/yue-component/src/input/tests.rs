use super::{InputStatus, border_token};
use crate::tokens::{pixels as token_pixels, rems, rgba as color};

use crate::{Accent, ThemeMode, YueTheme};

#[test]
fn input_options_and_tokens_cover_every_theme_profile() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            let theme = YueTheme::new(mode, accent);
            assert_eq!(rems(theme, crate::theme::keys::INPUT_HEIGHT_SM).0, 1.75);
            assert_eq!(rems(theme, crate::theme::keys::INPUT_HEIGHT_MD).0, 2.0);
            assert_eq!(rems(theme, crate::theme::keys::INPUT_HEIGHT_LG).0, 2.5);
            assert_eq!(
                token_pixels(theme, crate::theme::keys::TEXTAREA_PADDING_BLOCK),
                8.0
            );
            assert_eq!(color(theme, crate::theme::keys::INPUT_BACKGROUND).a, 1.0);
            assert!(color(theme, crate::theme::keys::INPUT_BACKGROUND_DISABLED).a < 1.0);
            assert!(color(theme, crate::theme::keys::INPUT_BORDER_COLOR_INVALID).a > 0.99);
        }
    }
}

#[test]
fn validation_border_semantics_survive_hover_focus_and_disabled_states() {
    assert_eq!(
        border_token(InputStatus::Normal, false, false),
        crate::theme::keys::INPUT_BORDER_COLOR
    );
    assert_eq!(
        border_token(InputStatus::Normal, false, true),
        crate::theme::keys::INPUT_BORDER_COLOR_FOCUS
    );
    assert_eq!(
        border_token(InputStatus::Success, false, true),
        crate::theme::keys::SUCCESS
    );
    assert_eq!(
        border_token(InputStatus::Warning, false, true),
        crate::theme::keys::WARNING
    );
    assert_eq!(
        border_token(InputStatus::Error, false, true),
        crate::theme::keys::INPUT_BORDER_COLOR_INVALID
    );
    assert_eq!(
        border_token(InputStatus::Error, true, true),
        crate::theme::keys::INPUT_BORDER_COLOR_DISABLED
    );
}

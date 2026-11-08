use super::*;
use crate::theme::keys;
use crate::tokens::{number, pixels};
use crate::{Accent, NeutralTone, ThemeMode};

#[test]
fn check_timeline_opens_the_viewport_before_drawing_the_stroke() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let sample = |milliseconds: f32| element::check_progress(milliseconds / 300., theme);
    assert_eq!(sample(0.), (0., 0.));
    let (slot, ink) = sample(50.);
    assert!(slot > 0. && slot < 1.);
    assert_eq!(ink, 0.);
    let (slot, ink) = sample(100.);
    assert_eq!(slot, 1.);
    assert!(ink.abs() < 1e-6);
    let (slot, ink) = sample(200.);
    assert_eq!(slot, 1.);
    assert!(ink > 0. && ink < 1.);
    assert_eq!(sample(300.), (1., 1.));
    let mut previous = (0., 0.);
    for ms in 0..=300 {
        let next = sample(ms as f32);
        assert!(next.0 >= previous.0 && next.1 >= previous.1);
        previous = next;
    }
}

#[test]
fn defaults_and_modes_are_explicit() {
    let tag = YueTag::new("metadata", "PNG");
    tag.validate();
    assert_eq!(tag.behavior, Behavior::Static);
    assert_eq!(tag.tone, TagTone::Default);
    assert_eq!(tag.variant, TagVariant::Tint);
    assert_eq!(tag.size, TagSize::Medium);
    assert!(tag.on_close.is_none());
    let check = YueTag::checkable("copy", "复制路径", true).on_change(|_, _, _, _| {});
    check.validate();
    assert_eq!(check.tone, TagTone::Primary);
    assert_eq!(check.behavior, Behavior::Checkable(true));
}

#[test]
fn all_color_matrices_and_metrics_resolve_in_every_theme() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            for neutral in [
                NeutralTone::Neutral,
                NeutralTone::Slate,
                NeutralTone::Stone,
                NeutralTone::Haze,
            ] {
                let theme = YueTheme::new(mode, accent).with_neutral(neutral);
                for tone in [
                    TagTone::Default,
                    TagTone::Primary,
                    TagTone::Success,
                    TagTone::Warning,
                    TagTone::Danger,
                ] {
                    for variant in [
                        TagVariant::Filled,
                        TagVariant::Tint,
                        TagVariant::Outline,
                        TagVariant::TintOutline,
                    ] {
                        let colors = element::source_colors(tone, variant, theme);
                        let tone = format!("{tone:?}").to_ascii_lowercase();
                        let variant = match variant {
                            TagVariant::Filled => "filled",
                            TagVariant::Tint => "tint",
                            TagVariant::Outline => "outline",
                            TagVariant::TintOutline => "tint-outline",
                        };
                        for (part, color) in [
                            ("background", colors.background()),
                            ("color", colors.foreground()),
                            ("border-color", colors.border()),
                        ] {
                            let name = format!("--tag-{tone}-{variant}-{part}");
                            assert_eq!(
                                color,
                                theme.token(name.as_str()).unwrap().rgba().unwrap(),
                                "{name}"
                            );
                        }
                    }
                }
                for (size, height, padding, font) in [
                    (TagSize::Small, 20., 6., 11.),
                    (TagSize::Medium, 24., 8., 12.),
                    (TagSize::Large, 30., 10., 13.),
                ] {
                    let (h, p, f) = size.metrics();
                    assert_eq!(
                        (pixels(theme, h), pixels(theme, p), pixels(theme, f)),
                        (height, padding, font)
                    );
                }
                assert_eq!(number(theme, keys::TAG_LINE_HEIGHT), 1.);
                assert!(number(theme, keys::TAG_OPACITY_DISABLED) > 0.);
            }
        }
    }
}

#[test]
#[should_panic(expected = "visible text")]
fn whitespace_label_is_rejected() {
    let _ = YueTag::new("bad", " \n");
}

#[test]
#[should_panic(expected = "requires YueTag::action")]
fn static_tag_cannot_gain_action_handler_accidentally() {
    let _ = YueTag::new("bad", "标签").on_click(|_, _, _| {});
}

#[test]
#[should_panic(expected = "requires on_click")]
fn action_requires_a_handler() {
    YueTag::action("bad", "动作").validate();
}

#[test]
#[should_panic(expected = "requires on_change")]
fn toggle_requires_a_handler() {
    YueTag::checkable("bad", "选择", false).validate();
}

#[test]
#[should_panic(expected = "semantic tones")]
fn custom_colors_cannot_erase_toggle_state() {
    let color = gpui_kit::rgb(0);
    let _ = YueTag::checkable("bad", "选择", false).colors(TagColors::new(color, color, color));
}

#[test]
#[should_panic(expected = "finite sRGB")]
fn nonfinite_custom_color_is_rejected() {
    let color = gpui_kit::Rgba {
        r: f32::NAN,
        g: 0.,
        b: 0.,
        a: 1.,
    };
    let _ = TagColors::new(color, color, color);
}

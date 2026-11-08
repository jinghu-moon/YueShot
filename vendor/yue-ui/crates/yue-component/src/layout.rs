//! The page skeleton: the widths a shell reserves, and what a window size decides.
//!
//! Yue's rule is that a component never learns the page it sits in — it keeps its own size and stays
//! fluid (`apps/docs/design/layout.md` in the design system). So this module ships no container: a
//! 12-column grid engine in a crate whose layout primitive is `flex` would be a second, worse copy of
//! what CSS grid already does on the web. What it does provide are the skeleton's numbers and the
//! decisions a shell makes as the window is resized: which band, which gutter, whether the sidebar
//! folds, and how many cards fit.
//!
//! GPUI has no media queries, so [`Breakpoint`] reads the same `--breakpoint-*` tokens the web sheet
//! declares, from the theme, and the shell asks it whenever it renders. The token file is the only
//! place a number is written; a retuned skeleton moves both sides at once.

use gpui_kit::base::Transition;
use gpui_kit::{Pixels, px};

use crate::YueTheme;
use crate::motion::{MotionDuration, MotionEase, transition};
use crate::tokens::{number, pixels};

/// Where a window sits on Yue's breakpoint scale: `xs` is below `--breakpoint-sm`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Breakpoint {
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}

impl Breakpoint {
    pub const ALL: [Self; 5] = [Self::Xs, Self::Sm, Self::Md, Self::Lg, Self::Xl];

    /// The band a window of `width` falls into. A window exactly on a threshold is in the band above
    /// it, the way a `min-width` media query matches its own breakpoint.
    ///
    /// Called on every resize, not once at startup: which skeleton fits is a fact about the current
    /// window, and a window is the one thing a desktop app's user moves freely.
    pub fn of(theme: YueTheme, width: Pixels) -> Self {
        Self::ALL
            .iter()
            .rev()
            .copied()
            .find(|band| width >= band.min_width(theme))
            .unwrap_or(Self::Xs)
    }

    /// `sm`, `md`, … and `xs` for the band under the first token, which has no token of its own.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Xs => "xs",
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
            Self::Xl => "xl",
        }
    }

    /// The width this band starts at; `0` for [`Breakpoint::Xs`].
    ///
    /// # Panics
    ///
    /// When the theme carries no `--breakpoint-*` token for the band.
    pub fn min_width(self, theme: YueTheme) -> Pixels {
        let name = match self {
            Self::Xs => return px(0.0),
            Self::Sm => "--breakpoint-sm",
            Self::Md => "--breakpoint-md",
            Self::Lg => "--breakpoint-lg",
            Self::Xl => "--breakpoint-xl",
        };
        px(pixels(theme, name))
    }
}

/// Height of the top bar a skeleton reserves for navigation and global actions.
pub fn header_height(theme: YueTheme) -> Pixels {
    px(pixels(theme, crate::theme::keys::LAYOUT_HEADER_HEIGHT))
}

/// Width of an expanded sidebar.
pub fn sidebar_width(theme: YueTheme) -> Pixels {
    px(pixels(theme, crate::theme::keys::LAYOUT_SIDEBAR_WIDTH))
}

/// Width the same sidebar folds to: wide enough for a 40px hit area with 12px either side.
pub fn sidebar_width_collapsed(theme: YueTheme) -> Pixels {
    px(pixels(
        theme,
        crate::theme::keys::LAYOUT_SIDEBAR_WIDTH_COLLAPSED,
    ))
}

/// Below this window width the sidebar folds. TDesign folds at 992; Yue has no 992 step, so the
/// token points at `--breakpoint-lg` and the mapping stays in one place.
pub fn sidebar_collapse_at(theme: YueTheme) -> Pixels {
    px(pixels(
        theme,
        crate::theme::keys::LAYOUT_SIDEBAR_COLLAPSE_AT,
    ))
}

/// Width of a pane the page is split off from the main area: the list column of a list–detail pair,
/// or a right-hand inspector.
pub fn pane_width(theme: YueTheme) -> Pixels {
    px(pixels(theme, crate::theme::keys::LAYOUT_PANE_WIDTH))
}

/// Page padding below `--breakpoint-md`.
pub fn page_gutter(theme: YueTheme) -> Pixels {
    px(pixels(theme, crate::theme::keys::LAYOUT_PAGE_GUTTER))
}

/// Page padding from `--breakpoint-md` up to `--breakpoint-xl`: TDesign's margin.
pub fn page_gutter_md(theme: YueTheme) -> Pixels {
    px(pixels(theme, crate::theme::keys::LAYOUT_PAGE_GUTTER_MD))
}

/// Page padding from `--breakpoint-xl` up.
pub fn page_gutter_lg(theme: YueTheme) -> Pixels {
    px(pixels(theme, crate::theme::keys::LAYOUT_PAGE_GUTTER_LG))
}

/// The page padding a window this wide takes: the gutter steps with the band, 16 → 24 → 32.
pub fn page_gutter_for(theme: YueTheme, width: Pixels) -> Pixels {
    match Breakpoint::of(theme, width) {
        Breakpoint::Xs | Breakpoint::Sm => page_gutter(theme),
        Breakpoint::Md | Breakpoint::Lg => page_gutter_md(theme),
        Breakpoint::Xl => page_gutter_lg(theme),
    }
}

/// Column count the page grid divides into.
pub fn grid_columns(theme: YueTheme) -> usize {
    number(theme, crate::theme::keys::LAYOUT_GRID_COLUMNS).round() as usize
}

/// Space between two grid columns.
pub fn grid_gap(theme: YueTheme) -> Pixels {
    px(pixels(theme, crate::theme::keys::LAYOUT_GRID_GAP))
}

/// Measure a long line of text is capped to.
pub fn container_text(theme: YueTheme) -> Pixels {
    px(pixels(theme, crate::theme::keys::CONTAINER_TEXT))
}

/// Maximum width a page's content grows to before it is centred with gutters either side.
pub fn container_page(theme: YueTheme) -> Pixels {
    px(pixels(theme, crate::theme::keys::CONTAINER_PAGE))
}

/// Whether a window this wide should show its sidebar folded.
pub fn sidebar_collapsed(theme: YueTheme, width: Pixels) -> bool {
    width < sidebar_collapse_at(theme)
}

/// The sidebar width a window this wide should reserve.
pub fn sidebar_width_for(theme: YueTheme, width: Pixels) -> Pixels {
    if sidebar_collapsed(theme, width) {
        sidebar_width_collapsed(theme)
    } else {
        sidebar_width(theme)
    }
}

/// How many columns of at least `item_min` fit across `available`, given the grid's gap.
///
/// The auto-fit count a card grid wants: the page declares a narrowest readable card, not a column
/// count, so the same content keeps its size as the window grows and only the count changes. Never
/// returns zero — a single column that overflows is what a page does below its narrowest band.
pub fn grid_fit(theme: YueTheme, available: Pixels, item_min: Pixels) -> usize {
    let gap = grid_gap(theme).as_f32();
    let available = available.as_f32();
    let item_min = item_min.as_f32().max(1.0);
    let fitted = ((available + gap) / (item_min + gap)).floor() as usize;
    fitted.clamp(1, grid_columns(theme))
}

/// The fold a sidebar makes on its way to the rail: [`MotionEase::Stow`], because the panel is being
/// put where it can be recalled, not dismissed.
pub fn sidebar_collapse_transition(theme: YueTheme) -> Transition {
    transition(theme, MotionDuration::ExitLarge, MotionEase::Stow)
}

/// The same panel coming back out of the rail: an entrance, so it takes the enter pair and
/// decelerates into place. Stow only describes the leaving half.
pub fn sidebar_expand_transition(theme: YueTheme) -> Transition {
    transition(theme, MotionDuration::EnterLarge, MotionEase::Enter)
}

/// The transition for a sidebar heading to `collapsed`; Base reads the policy each frame, so a
/// shell passes this with the current target and the direction picks the pair.
pub fn sidebar_transition(theme: YueTheme, collapsed: bool) -> Transition {
    if collapsed {
        sidebar_collapse_transition(theme)
    } else {
        sidebar_expand_transition(theme)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Breakpoint, grid_columns, grid_fit, grid_gap, header_height, page_gutter, page_gutter_for,
        page_gutter_lg, page_gutter_md, pane_width, sidebar_collapse_at, sidebar_collapsed,
        sidebar_width, sidebar_width_collapsed, sidebar_width_for,
    };
    use crate::{Accent, ThemeMode, YueTheme};
    use gpui_kit::px;

    fn theme() -> YueTheme {
        YueTheme::new(ThemeMode::Light, Accent::Azure)
    }

    /// The skeleton `layout.md` documents, pinned here so a retune on either side fails until the
    /// two agree.
    #[test]
    fn the_skeleton_reads_the_documented_widths() {
        let theme = theme();
        assert_eq!(header_height(theme), px(56.0));
        assert_eq!(sidebar_width(theme), px(232.0));
        assert_eq!(sidebar_width_collapsed(theme), px(64.0));
        assert_eq!(pane_width(theme), px(320.0));
        assert_eq!(
            (
                page_gutter(theme),
                page_gutter_md(theme),
                page_gutter_lg(theme)
            ),
            (px(16.0), px(24.0), px(32.0))
        );
        assert_eq!((grid_columns(theme), grid_gap(theme)), (12, px(16.0)));
    }

    #[test]
    fn the_bands_are_yues_own_scale() {
        let theme = theme();
        let widths: Vec<_> = Breakpoint::ALL
            .iter()
            .map(|band| band.min_width(theme))
            .collect();
        assert_eq!(
            widths,
            vec![px(0.0), px(640.0), px(768.0), px(1024.0), px(1280.0)]
        );
        assert_eq!(
            Breakpoint::ALL.map(|band| band.label()),
            ["xs", "sm", "md", "lg", "xl"]
        );
    }

    /// TDesign's 992 fold maps onto Yue's nearest step, and a window sitting exactly on it is wide
    /// enough to keep the sidebar open.
    #[test]
    fn the_sidebar_folds_at_the_large_breakpoint() {
        let theme = theme();
        assert_eq!(sidebar_collapse_at(theme), px(1024.0));
        assert!(!sidebar_collapsed(theme, px(1024.0)));
        assert!(sidebar_collapsed(theme, px(1023.0)));
        assert_eq!(sidebar_width_for(theme, px(1440.0)), sidebar_width(theme));
        assert_eq!(
            sidebar_width_for(theme, px(900.0)),
            sidebar_width_collapsed(theme)
        );
    }

    #[test]
    fn a_band_is_the_widest_one_the_window_reaches() {
        let theme = theme();
        for (width, band) in [
            (px(320.0), Breakpoint::Xs),
            (px(639.0), Breakpoint::Xs),
            (px(640.0), Breakpoint::Sm),
            (px(768.0), Breakpoint::Md),
            (px(1023.0), Breakpoint::Md),
            (px(1024.0), Breakpoint::Lg),
            (px(1280.0), Breakpoint::Xl),
            (px(1920.0), Breakpoint::Xl),
        ] {
            assert_eq!(Breakpoint::of(theme, width), band, "{width}");
        }
    }

    /// The gutter steps 16 → 24 at md and 24 → 32 at xl, the table `layout.md` documents.
    #[test]
    fn the_gutter_steps_with_the_band() {
        let theme = theme();
        for (width, gutter) in [
            (px(360.0), 16.0),
            (px(767.0), 16.0),
            (px(768.0), 24.0),
            (px(1279.0), 24.0),
            (px(1280.0), 32.0),
        ] {
            assert_eq!(page_gutter_for(theme, width), px(gutter), "{width}");
        }
    }

    /// The count a card grid takes: gap counted once between neighbours, never zero columns, and
    /// never more than the grid divides into.
    #[test]
    fn grid_fit_counts_the_columns_that_fit() {
        let theme = theme();
        let cards = px(240.0);
        assert_eq!(grid_fit(theme, px(240.0), cards), 1);
        assert_eq!(grid_fit(theme, px(496.0), cards), 2);
        assert_eq!(grid_fit(theme, px(495.0), cards), 1);
        assert_eq!(grid_fit(theme, px(1040.0), cards), 4);
        // 1920 is a 12-column page, but 240px cards only fill seven of them; the count is what
        // fits, not what the grid divides into.
        assert_eq!(grid_fit(theme, px(1920.0), cards), 7);
        assert_eq!(grid_fit(theme, px(0.0), cards), 1);
        assert_eq!(grid_fit(theme, px(1000.0), px(0.0)), 12);
    }
}

use crate::VirtualizerError;
use gpui_kit::{AbsoluteLength, Pixels, px};

/// Mutually exclusive fixed-column or minimum-card-width rule.
/// Lengths may use pixels or rems; rems resolve against the actual window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListGrid {
    mode: Mode,
    gap: AbsoluteLength,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Columns(u16),
    MinWidth(AbsoluteLength),
}
/// Resolved content geometry, shared by ordinary and virtual card lists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListGridGeometry {
    columns: u16,
    card_width: Pixels,
    gap: Pixels,
}
impl ListGridGeometry {
    pub fn columns(self) -> u16 {
        self.columns
    }
    pub fn card_width(self) -> Pixels {
        self.card_width
    }
    pub fn gap(self) -> Pixels {
        self.gap
    }
}
impl ListGrid {
    pub fn columns(columns: u16, gap: impl Into<AbsoluteLength>) -> Result<Self, VirtualizerError> {
        if columns == 0 {
            return Err(VirtualizerError::InvalidGeometry("list grid columns"));
        }
        Self::checked(Mode::Columns(columns), gap.into())
    }
    pub fn min_width(
        width: impl Into<AbsoluteLength>,
        gap: impl Into<AbsoluteLength>,
    ) -> Result<Self, VirtualizerError> {
        let width = width.into();
        if !valid(width.to_pixels(px(1.)), true) {
            return Err(VirtualizerError::InvalidGeometry("list grid minimum width"));
        }
        Self::checked(Mode::MinWidth(width), gap.into())
    }
    fn checked(mode: Mode, gap: AbsoluteLength) -> Result<Self, VirtualizerError> {
        if !valid(gap.to_pixels(px(1.)), false) {
            return Err(VirtualizerError::InvalidGeometry("list grid gap"));
        }
        Ok(Self { mode, gap })
    }
    pub fn resolve(self, width: Pixels, rem: Pixels) -> Result<ListGridGeometry, VirtualizerError> {
        if !valid(width, false) || !valid(rem, true) {
            return Err(VirtualizerError::InvalidGeometry("list grid container"));
        }
        let gap = self.gap.to_pixels(rem);
        if !valid(gap, false) {
            return Err(VirtualizerError::InvalidGeometry("list grid resolved gap"));
        }
        let columns = match self.mode {
            Mode::Columns(columns) => columns,
            Mode::MinWidth(min) => {
                let min = min.to_pixels(rem);
                if !valid(min, true) {
                    return Err(VirtualizerError::InvalidGeometry(
                        "list grid resolved minimum",
                    ));
                }
                let columns = ((f64::from(width) + f64::from(gap))
                    / (f64::from(min) + f64::from(gap)))
                .floor()
                .max(1.);
                if columns > f64::from(u16::MAX) {
                    return Err(VirtualizerError::InvalidGeometry(
                        "list grid column capacity",
                    ));
                }
                columns as u16
            }
        };
        let card_width = px(
            ((f64::from(width) - f64::from(columns - 1) * f64::from(gap)) / f64::from(columns))
                .max(0.) as f32,
        );
        Ok(ListGridGeometry {
            columns,
            card_width,
            gap,
        })
    }
}
pub(super) fn valid(value: Pixels, positive: bool) -> bool {
    f64::from(value).is_finite()
        && if positive {
            value > px(0.)
        } else {
            value >= px(0.)
        }
}

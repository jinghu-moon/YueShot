//! Pure sRGB/HSV/HSL math. UI drafts never pass through a permissive CSS parser.
use gpui_kit::Rgba;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorFormat {
    #[default]
    Hex,
    Rgb,
    Hsl,
}
impl ColorFormat {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Hex => "HEX",
            Self::Rgb => "RGB",
            Self::Hsl => "HSL",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Color(pub [u8; 4]);
impl Color {
    pub const BLACK: Self = Self([0, 0, 0, 255]);
    pub fn from_rgba(c: Rgba) -> Self {
        let values = [c.r, c.g, c.b, c.a];
        assert!(
            values
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "ColorPicker requires finite sRGB/alpha channels in 0..=1"
        );
        Self(values.map(|v| (v * 255.).round() as u8))
    }
    pub fn rgba(self) -> Rgba {
        let [r, g, b, a] = self.0.map(|v| v as f32 / 255.);
        Rgba { r, g, b, a }
    }
    pub fn hex(self) -> String {
        let [r, g, b, a] = self.0;
        if a == 255 {
            format!("#{r:02X}{g:02X}{b:02X}")
        } else {
            format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
        }
    }
    pub fn hsv(self, remembered_h: f64) -> [f64; 3] {
        let [r, g, b, _] = self.0.map(|v| v as f64 / 255.);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d == 0. {
            remembered_h
        } else if max == r {
            60. * ((g - b) / d).rem_euclid(6.)
        } else if max == g {
            60. * ((b - r) / d + 2.)
        } else {
            60. * ((r - g) / d + 4.)
        };
        [h, if max == 0. { 0. } else { d / max }, max]
    }
    pub fn from_hsv([h, s, v]: [f64; 3], a: f64) -> Self {
        let c = v * s;
        let x = c * (1. - ((h.rem_euclid(360.) / 60.).rem_euclid(2.) - 1.).abs());
        let m = v - c;
        let rgb = match (h.rem_euclid(360.) / 60.) as u8 {
            0 => [c, x, 0.],
            1 => [x, c, 0.],
            2 => [0., c, x],
            3 => [0., x, c],
            4 => [x, 0., c],
            _ => [c, 0., x],
        };
        Self([
            (255. * (rgb[0] + m)).round() as u8,
            (255. * (rgb[1] + m)).round() as u8,
            (255. * (rgb[2] + m)).round() as u8,
            (255. * a).round() as u8,
        ])
    }
    pub fn hsl(self, remembered_h: f64) -> [f64; 3] {
        let [r, g, b, _] = self.0.map(|v| v as f64 / 255.);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let l = (max + min) / 2.;
        [
            self.hsv(remembered_h)[0],
            if d == 0. {
                0.
            } else {
                d / (1. - (2. * l - 1.).abs())
            },
            l,
        ]
    }
    pub fn from_hsl([h, s, l]: [f64; 3], a: f64) -> Self {
        let v = l + s * l.min(1. - l);
        Self::from_hsv([h, if v == 0. { 0. } else { 2. * (1. - l / v) }, v], a)
    }
    pub fn text(self, format: ColorFormat, h: f64) -> String {
        let [r, g, b, a] = self.0;
        match format {
            ColorFormat::Hex => self.hex(),
            ColorFormat::Rgb => {
                if a == 255 {
                    format!("rgb({r}, {g}, {b})")
                } else {
                    format!("rgba({r}, {g}, {b}, {})", decimal(a as f64 / 255., 5))
                }
            }
            ColorFormat::Hsl => {
                let [h, s, l] = self.hsl(h);
                let parts = format!(
                    "{}, {}%, {}%",
                    decimal(h, 2),
                    decimal(s * 100., 2),
                    decimal(l * 100., 2)
                );
                if a == 255 {
                    format!("hsl({parts})")
                } else {
                    format!("hsla({parts}, {})", decimal(a as f64 / 255., 5))
                }
            }
        }
    }
}
pub(super) fn decimal(v: f64, places: usize) -> String {
    let text = format!("{v:.places$}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}
pub(super) fn parse_hex(text: &str) -> Option<Color> {
    let s = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !matches!(s.len(), 3 | 4 | 6 | 8) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0, 0, 0, 255];
    if s.len() <= 4 {
        for (ix, b) in s.bytes().enumerate() {
            bytes[ix] = (b as char).to_digit(16)? as u8 * 17;
        }
    } else {
        for (ix, pair) in s.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            bytes[ix] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
        }
    }
    Some(Color(bytes))
}
pub(super) fn parse_number(text: &str, max: f64, places: usize) -> Option<f64> {
    let s = text.trim();
    if s.is_empty() {
        return None;
    }
    let mut dots = 0;
    let mut digits = 0;
    for b in s.bytes() {
        if b == b'.' {
            dots += 1;
        } else if b.is_ascii_digit() {
            digits += 1;
        } else {
            return None;
        }
    }
    if digits == 0 || dots > 1 || (places == 0 && dots != 0) || s.ends_with('.') {
        return None;
    }
    if let Some((_, fraction)) = s.split_once('.')
        && fraction.len() > places
    {
        return None;
    }
    let n = s.parse::<f64>().ok()?;
    (n.is_finite() && (0.0..=max).contains(&n)).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_every_hex_length_and_rejects_partial_garbage() {
        assert_eq!(parse_hex(" #f08 "), Some(Color([255, 0, 136, 255])));
        assert_eq!(parse_hex("f08a"), Some(Color([255, 0, 136, 170])));
        assert_eq!(parse_hex("#10203080"), Some(Color([16, 32, 48, 128])));
        for bad in [
            "",
            "#12",
            "#12345",
            "#1234567",
            "#12ZZZZ",
            "123456junk",
            "##ffffff",
        ] {
            assert_eq!(parse_hex(bad), None);
        }
    }
    #[test]
    fn numeric_drafts_are_strict() {
        assert_eq!(parse_number(" 50.2 ", 100., 2), Some(50.2));
        for bad in [
            "", "-1", "NaN", "inf", "100.01", "50%", "1e2", "1.234", "1.", ".",
        ] {
            assert_eq!(parse_number(bad, 100., 2), None, "{bad}");
        }
        assert_eq!(parse_number("1.0", 255., 0), None);
    }
    #[test]
    fn hsv_and_hsl_round_trip_srgb_including_gray_and_black() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let c = Color([r, g, b, 128]);
                    assert_eq!(Color::from_hsv(c.hsv(247.), 128. / 255.), c);
                    assert_eq!(Color::from_hsl(c.hsl(247.), 128. / 255.), c);
                }
            }
        }
        assert_eq!(Color::BLACK.hsv(247.), [247., 0., 0.]);
        assert_eq!(Color::from_hsv([240., 1., 1.], 1.), Color([0, 0, 255, 255]));
        assert_eq!(
            Color::from_hsl([240., 1., 0.5], 1.),
            Color([0, 0, 255, 255])
        );
    }
    #[test]
    fn all_alpha_levels_survive_display_precision_and_hex() {
        for a in 0..=255 {
            let c = Color([51, 102, 204, a]);
            assert_eq!(parse_hex(&c.hex()), Some(c));
            let pct = decimal(a as f64 / 255. * 100., 2).parse::<f64>().unwrap();
            assert_eq!((pct / 100. * 255.).round() as u8, a);
            let fraction = decimal(a as f64 / 255., 5).parse::<f64>().unwrap();
            assert_eq!((fraction * 255.).round() as u8, a);
        }
    }
    #[test]
    #[should_panic(expected = "finite sRGB")]
    fn api_rejects_nonfinite() {
        Color::from_rgba(Rgba {
            r: f32::NAN,
            g: 0.,
            b: 0.,
            a: 1.,
        });
    }
}

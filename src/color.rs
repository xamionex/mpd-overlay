use gtk::gdk_pixbuf::{InterpType, Pixbuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Rgb {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };

    pub fn new(r: f64, g: f64, b: f64) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
        }
    }

    pub fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::new(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)
    }

    pub fn invert(self) -> Self {
        Self::new(1.0 - self.r, 1.0 - self.g, 1.0 - self.b)
    }

    pub fn brightness(self) -> f64 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    pub fn is_closer_to_white(self) -> bool {
        (self.r + self.g + self.b) / 3.0 > 0.5
    }

    /// Linear blend from `self` (t = 0) to `other` (t = 1).
    pub fn mix(self, other: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
        )
    }

    /// Black or white, whichever stands out against this color.
    /// Used for the readability halo behind glowing text.
    pub fn contrasting_extreme(self) -> Self {
        if self.is_closer_to_white() {
            Self::BLACK
        } else {
            Self::WHITE
        }
    }
}

pub fn parse_hex(value: &str, fallback: Rgb) -> Rgb {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 {
        return fallback;
    }

    let parsed = u32::from_str_radix(value, 16);
    match parsed {
        Ok(rgb) => Rgb::from_rgb8(
            ((rgb >> 16) & 0xff) as u8,
            ((rgb >> 8) & 0xff) as u8,
            (rgb & 0xff) as u8,
        ),
        Err(_) => fallback,
    }
}

pub fn parse_color_alpha(value: &str, fallback: f64) -> f64 {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 8 {
        return fallback;
    }
    u8::from_str_radix(&value[6..8], 16)
        .map(|byte| byte as f64 / 255.0)
        .unwrap_or(fallback)
}

pub fn average_color(pixbuf: &Pixbuf) -> Rgb {
    let Some(small) = pixbuf.scale_simple(32, 32, InterpType::Bilinear) else {
        return Rgb::new(0.5, 0.5, 0.5);
    };

    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;
    let pixels = (small.width() * small.height()).max(1) as f64;
    let data = small.read_pixel_bytes();
    let data = data.as_ref();
    let channels = small.n_channels().max(1) as usize;
    let rowstride = small.rowstride().max(0) as usize;

    for y in 0..small.height() as usize {
        for x in 0..small.width() as usize {
            let offset = y * rowstride + x * channels;
            if offset + 2 >= data.len() {
                continue;
            }
            r += data[offset] as f64;
            g += data[offset + 1] as f64;
            b += data[offset + 2] as f64;
        }
    }

    Rgb::from_rgb8((r / pixels) as u8, (g / pixels) as u8, (b / pixels) as u8)
}

pub fn brighten(color: Rgb) -> Rgb {
    let r = color.r * 255.0;
    let g = color.g * 255.0;
    let b = color.b * 255.0;
    let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
    let amount = 100.0;

    Rgb::from_rgb8(
        (r + amount * (255.0 - luminance) / 255.0).min(255.0) as u8,
        (g + amount * (255.0 - luminance) / 255.0).min(255.0) as u8,
        (b + amount * (255.0 - luminance) / 255.0).min(255.0) as u8,
    )
}

/// Mirrors the brightening rule used by petarwidget.html so colors stay readable on dark art.
pub fn brighten_for_overlay(color: Rgb) -> Rgb {
    let brightness = color.brightness();
    if !(0.3..0.6).contains(&brightness) {
        return color;
    }

    brighten(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        assert_eq!(
            parse_hex("#ff8000", Rgb::BLACK),
            Rgb::from_rgb8(255, 128, 0)
        );
        assert_eq!(parse_hex("not-a-color", Rgb::WHITE), Rgb::WHITE);
    }

    #[test]
    fn inverts_colors() {
        assert_eq!(
            Rgb::from_rgb8(255, 0, 255).invert(),
            Rgb::from_rgb8(0, 255, 0)
        );
    }

    #[test]
    fn mixes_colors() {
        assert_eq!(Rgb::BLACK.mix(Rgb::WHITE, 0.0), Rgb::BLACK);
        assert_eq!(Rgb::BLACK.mix(Rgb::WHITE, 1.0), Rgb::WHITE);
        assert_eq!(Rgb::BLACK.mix(Rgb::WHITE, 0.5), Rgb::new(0.5, 0.5, 0.5));
        assert_eq!(Rgb::BLACK.mix(Rgb::WHITE, 7.0), Rgb::WHITE);
    }

    #[test]
    fn halo_contrasts_with_text() {
        assert_eq!(
            Rgb::from_rgb8(240, 220, 200).contrasting_extreme(),
            Rgb::BLACK
        );
        assert_eq!(Rgb::from_rgb8(40, 30, 90).contrasting_extreme(), Rgb::WHITE);
    }

    #[test]
    fn brightens_only_mid_brightness_colors() {
        let dark = Rgb::from_rgb8(20, 20, 20);
        assert_eq!(brighten_for_overlay(dark), dark);

        let mid = Rgb::from_rgb8(100, 100, 100);
        assert!(brighten_for_overlay(mid).brightness() > mid.brightness());
    }
}

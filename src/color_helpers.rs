#![allow(unused_parens)]
use image::{Pixel, Rgb};
use std::cmp::{max, min};

/// Returns the hue value of a pixel in the range [0, 360]
pub fn get_hue(&pixel: &Rgb<u8>) -> u16 {
    let channels = pixel.channels();
    let r: f32 = channels[0] as f32 / 255.0;
    let g: f32 = channels[1] as f32 / 255.0;
    let b: f32 = channels[2] as f32 / 255.0;
    let mut hue: f32 = 0.0;

    if (r >= g && g >= b) {
        //orange
        if r == b {
            return 0;
        }
        hue = 60.0 * (g - b) / (r - b);
    } else if (g > r && r >= b) {
        //Chartreuse
        hue = 120.0 - 60.0 * (r - b) / (g - b);
    } else if (g >= b && b > r) {
        //green
        hue = 120.0 + 60.0 * (b - r) / (g - r);
    } else if (b > g && g > r) {
        //azure
        hue = 240.0 - 60.0 * (g - r) / (b - r);
    } else if (b > r && r >= g) {
        //violet
        hue = 240.0 + 60.0 * (r - g) / (b - g);
    } else if (r >= b && b > g) {
        //rose
        hue = 360.0 - 60.0 * (b - g) / (r - g);
    }

    return hue as u16;
}

/// returns the brightness of a pixel in the range [0, 255]
pub fn get_brightness(&p: &Rgb<u8>) -> u16 {
    let channels = p.channels();
    let (r, g, b) = (channels[0] as f32, channels[1] as f32, channels[2] as f32);
    (0.2126 * r + 0.7152 * g + 0.0722 * b) as u16
}

/// tries to calculate the brightness of a pixel but with added casting and overflowing issues added into the mix.
/// Used for glitchsorting
pub fn get_brightness_flawed(&p: &Rgb<u8>) -> u16 {
    let channels = p.channels();
    let (r, g, b) = (channels[0] as i8, channels[1] as i8, channels[2] as i8);
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) as i8 as u8 as u16
}

/// returns the saturation of a pixel in the range [0, 255]
pub fn get_saturation(&p: &Rgb<u8>) -> u16 {
    let channels = &p.channels();
    let maxrgb: u16 = max(channels[0], max(channels[1], channels[2])).into();
    if maxrgb == 0 {
        return 0_u16;
    }
    let minrgb: u16 = min(channels[0], min(channels[1], channels[2])).into();
    (255 * (maxrgb - minrgb) / maxrgb) as u16
}

/// returns the red channel of a pixel in the range [0, 255]
pub fn get_red(p: &Rgb<u8>) -> u16 {
    p.0[0] as u16
}

/// returns the green channel of a pixel in the range [0, 255]
pub fn get_green(p: &Rgb<u8>) -> u16 {
    p.0[1] as u16
}

/// returns the blue channel of a pixel in the range [0, 255]
pub fn get_blue(p: &Rgb<u8>) -> u16 {
    p.0[2] as u16
}

/// returns how "warm" a pixel is in the range [0, 255]: 0 = blue/cold, 255 = red/warm
pub fn get_warmth(p: &Rgb<u8>) -> u16 {
    ((p.0[0] as i16 - p.0[2] as i16 + 255) / 2) as u16
}

/// Lookup table: sRGB channel value -> linear light
fn srgb_to_linear_table() -> &'static [f32; 256] {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = i as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
    })
}

/// returns the perceived lightness (CIE L*) of a pixel, scaled to the range [0, 255].
/// Looks more even than the simple brightness, especially for saturated colors.
pub fn get_lightness(p: &Rgb<u8>) -> u16 {
    let lin = srgb_to_linear_table();
    let y = 0.2126 * lin[p.0[0] as usize] + 0.7152 * lin[p.0[1] as usize] + 0.0722 * lin[p.0[2] as usize];
    // CIE L* in [0, 100]
    let l = if y > 216.0 / 24389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24389.0 / 27.0
    };
    ((l * 2.55).round() as u16).min(255)
}

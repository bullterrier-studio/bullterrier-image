//! Colour-space conversions: the sRGB transfer function (IEC 61966-2-1), one channel value at a time, and HSV, one pixel
//! at a time. Values outside 0..1 are carried through (the transfer function mirrored around zero), so callers decide
//! how to handle out-of-gamut colours.

const ENCODED_BREAK: f32 = 0.04045; // constant: encoded value where the sRGB linear segment ends (IEC 61966-2-1)
const LINEAR_BREAK: f32 = 0.003_130_8; // constant: the same point in linear light (IEC 61966-2-1)
const SLOPE: f32 = 12.92; // constant: slope of the sRGB linear segment (IEC 61966-2-1)
const OFFSET: f32 = 0.055; // constant: offset of the sRGB power-law segment (IEC 61966-2-1)
const SCALE: f32 = 1.055; // constant: scale of the sRGB power-law segment (IEC 61966-2-1)
const GAMMA: f32 = 2.4; // constant: exponent of the sRGB power-law segment (IEC 61966-2-1)

/// Decode a gamma-encoded sRGB value into linear light (0..1 for 0..1).
pub(crate) fn srgb_to_linear(value: f32) -> f32 {
    let magnitude = value.abs();
    let linear = if magnitude <= ENCODED_BREAK {
        magnitude / SLOPE
    } else {
        ((magnitude + OFFSET) / SCALE).powf(GAMMA)
    };
    linear.copysign(value)
}

/// Encode a linear-light value as gamma-encoded sRGB; the inverse of [`srgb_to_linear`].
pub(crate) fn linear_to_srgb(value: f32) -> f32 {
    let magnitude = value.abs();
    let encoded = if magnitude <= LINEAR_BREAK {
        magnitude * SLOPE
    } else {
        SCALE * magnitude.powf(GAMMA.recip()) - OFFSET
    };
    encoded.copysign(value)
}

const OFFSETS: [f32; 3] = [5.0, 3.0, 1.0]; // constant: the red, green and blue channels' offsets in sectors (HSV)
const SECTORS: f32 = 6.0; // constant: the hue circle is six sectors, red to yellow to green to cyan to blue to magenta (HSV)

/// Convert gamma-encoded sRGB to (hue, saturation, value), each 0..1. Hue is a share of the full colour circle (0 =
/// red, 1/3 = green, 2/3 = blue) in [0, 1], 1 being red again; it is 0 for greys, whose hue is undefined. Saturation
/// is 0 for greys and black.
pub(crate) fn rgb_to_hsv([red, green, blue]: [f32; 3]) -> [f32; 3] {
    let value = red.max(green).max(blue);
    let chroma = value - red.min(green).min(blue);
    if chroma == 0.0 {
        return [0.0, 0.0, value];
    }
    let sector = if red >= green && red >= blue {
        ((green - blue) / chroma).rem_euclid(SECTORS)
    } else if green >= blue {
        (blue - red) / chroma + 2.0 // constant: green's sector starts two sectors on from red (HSV)
    } else {
        (red - green) / chroma + 4.0 // constant: blue's sector starts four sectors on from red (HSV)
    };
    [sector / SECTORS, chroma / value, value]
}

/// Convert (hue, saturation, value) to gamma-encoded sRGB; the inverse of [`rgb_to_hsv`]. Hue is a share of the full
/// colour circle and wraps, so 1.25 and -0.75 both mean 0.25; saturation and value are 0..1, and so is the result.
pub(crate) fn hsv_to_rgb([hue, saturation, value]: [f32; 3]) -> [f32; 3] {
    // each channel is `value`, dipping to value * (1 - saturation) over a trapezoid of the hue circle: the standard
    // branch-free HSV formula
    OFFSETS.map(|offset| {
        let k = (offset + hue * SECTORS).rem_euclid(SECTORS);
        let ramp = k.min(4.0 - k).clamp(0.0, 1.0); // constant: the trapezoid's plateau starts four sectors on (HSV)
        value - value * saturation * ramp
    })
}

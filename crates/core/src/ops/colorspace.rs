//! Colour-space conversions: the sRGB transfer function (IEC 61966-2-1), one channel value at a time. Values outside
//! 0..1 are carried through, mirrored around zero, so callers decide how to handle out-of-gamut colours.

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

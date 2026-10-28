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

#[cfg(test)]
mod tests {
    use super::{linear_to_srgb, srgb_to_linear};
    use proptest::prelude::*;

    /// (encoded, linear) pairs of the IEC 61966-2-1 transfer function, worked out by hand: v / 12.92 up to 0.04045,
    /// above it ((v + 0.055) / 1.055) ** 2.4.
    const PAIRS: [(f32, f32); 6] = [
        (0.0, 0.0),                 // black
        (0.04045, 0.04045 / 12.92), // the top of the linear segment, 0.0031308
        (0.02, 0.02 / 12.92),       // inside the linear segment
        (0.5, 0.214_041_14),        // mid encoded grey is about 21% linear light (published: 0.2140)
        (0.2, 0.033_104_77),        // a dark tone
        (1.0, 1.0),                 // white
    ];

    #[test]
    fn both_directions_match_the_published_transfer_function() {
        for (encoded, linear) in PAIRS {
            assert!((srgb_to_linear(encoded) - linear).abs() < 1e-6, "decoding {encoded}");
            assert!((linear_to_srgb(linear) - encoded).abs() < 1e-6, "encoding {linear}");
        }
    }

    proptest! {
        #[test]
        fn encoding_undoes_decoding_over_the_range_and_just_outside_it(value in -0.25_f32..=1.25) {
            prop_assert!((linear_to_srgb(srgb_to_linear(value)) - value).abs() < 1e-5);
        }

        #[test]
        fn values_below_zero_are_mirrored_exactly(value in 0.0_f32..=1.0) {
            prop_assert_eq!(srgb_to_linear(-value).to_bits(), (-srgb_to_linear(value)).to_bits());
            prop_assert_eq!(linear_to_srgb(-value).to_bits(), (-linear_to_srgb(value)).to_bits());
        }

        #[test]
        fn both_directions_keep_the_order_of_values(low in 0.0_f32..=1.0, step in 0.0_f32..=1.0) {
            let high = low + step;
            prop_assert!(srgb_to_linear(low) <= srgb_to_linear(high));
            prop_assert!(linear_to_srgb(low) <= linear_to_srgb(high));
        }
    }
}

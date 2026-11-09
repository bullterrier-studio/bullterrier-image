//! Colour-space conversions: the sRGB transfer function (IEC 61966-2-1), one channel value at a time, and HSV and the
//! NTSC YIQ of television, one pixel at a time. Values outside 0..1 are carried through (the transfer function mirrored around zero), so callers decide
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

/// The NTSC YIQ of television (FCC 47 CFR 73.682): row 0 gives Y, the luma (the Rec. 601 weights), row 1 I, the
/// orange-cyan axis of the colour, row 2 Q, its green-magenta axis; a grey has I = Q = 0.
const RGB_TO_YIQ: [[f64; 3]; 3] = [
    [0.299, 0.587, 0.114],      // constant: the FCC YIQ matrix, as four-digit coefficients (Y row)
    [0.5959, -0.2746, -0.3213], // constant: the FCC YIQ matrix (I row)
    [0.2115, -0.5227, 0.3112],  // constant: the FCC YIQ matrix (Q row)
];

/// The cofactor of entry (`a`, `b`) of the 3 x 3 matrix `m`: the determinant of what is left without its row and column,
/// the rows and columns taken in a cycle so that the sign is right.
const fn cofactor(m: &[[f64; 3]; 3], a: usize, b: usize) -> f64 {
    let (r0, r1) = ((a + 1) % 3, (a + 2) % 3); // constant: the other two rows of 3, in cyclic order
    let (c0, c1) = ((b + 1) % 3, (b + 2) % 3); // constant: the other two columns of 3, in cyclic order
    m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]
}

/// The inverse of the 3 x 3 matrix `m` (not singular): its adjugate over its determinant.
const fn inverse(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let determinant = m[0][0] * cofactor(m, 0, 0) + m[0][1] * cofactor(m, 0, 1) + m[0][2] * cofactor(m, 0, 2);
    let mut out = [[0.0; 3]; 3];
    let mut row = 0; // constant: a counter starts at the first row
    while row < 3 {
        let mut column = 0; // constant: a counter starts at the first column
        while column < 3 {
            out[row][column] = cofactor(m, column, row) / determinant;
            column += 1;
        }
        row += 1;
    }
    out
}

/// The matrix of [`yiq_to_rgb`].
const YIQ_TO_RGB: [[f64; 3]; 3] = inverse(&RGB_TO_YIQ);

/// `matrix` applied to `pixel`, in f32.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the coefficients are stored in the f32 picture's precision"
)]
fn rotated(matrix: &[[f64; 3]; 3], pixel: [f32; 3]) -> [f32; 3] {
    matrix
        .map(|row| (row[0] * f64::from(pixel[0]) + row[1] * f64::from(pixel[1]) + row[2] * f64::from(pixel[2])) as f32)
}

/// Convert sRGB to the NTSC YIQ of television (FCC 47 CFR 73.682 coefficients): (Y, I, Q), Y in 0..1, I about -0.6..0.6
/// and Q about -0.53..0.53, both 0 for a grey. Nothing is clipped.
pub(crate) fn rgb_to_yiq(pixel: [f32; 3]) -> [f32; 3] {
    rotated(&RGB_TO_YIQ, pixel)
}

/// Convert NTSC YIQ back to sRGB; the inverse of [`rgb_to_yiq`]. Not clipped.
pub(crate) fn yiq_to_rgb(pixel: [f32; 3]) -> [f32; 3] {
    rotated(&YIQ_TO_RGB, pixel)
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

// JFIF full-range YCbCr (ITU-T T.871), as fractions of the 8-bit range: Y the Rec. 601 luma, Cb and Cr the blue and red
// differences, centred on 128 of 255.
const CHROMA_CENTRE: f32 = 128.0 / 255.0; // constant: JFIF centres Cb and Cr on 128 of 255

/// Convert sRGB to JFIF YCbCr (ITU-T T.871), each a fraction of the 8-bit range. Nothing is clipped: red's Cr and blue's Cb
/// reach 255.5/255.
pub(crate) fn rgb_to_ycbcr([red, green, blue]: [f32; 3]) -> [f32; 3] {
    // constant: the JFIF matrix, ITU-T T.871
    [
        0.299 * red + 0.587 * green + 0.114 * blue,
        -0.168_736 * red - 0.331_264 * green + 0.5 * blue + CHROMA_CENTRE,
        0.5 * red - 0.418_688 * green - 0.081_312 * blue + CHROMA_CENTRE,
    ]
}

/// Convert JFIF YCbCr back to sRGB; the inverse of [`rgb_to_ycbcr`], not clipped.
pub(crate) fn ycbcr_to_rgb([luma, blue, red]: [f32; 3]) -> [f32; 3] {
    let (blue, red) = (blue - CHROMA_CENTRE, red - CHROMA_CENTRE);
    // constant: the JFIF matrix from YCbCr to RGB, ITU-T T.871
    [
        luma + 1.402 * red,
        luma - 0.344_136 * blue - 0.714_136 * red,
        luma + 1.772 * blue,
    ]
}

#[cfg(test)]
mod tests {
    //! YIQ against the FCC's published matrix and its known inverse. The conversions are `pub(crate)`, so no public route
    //! shows them apart from the effects built on them.

    use super::{rgb_to_yiq, yiq_to_rgb};

    #[test]
    fn the_primaries_and_white_have_the_published_yiq() {
        for (rgb, yiq) in [
            ([1.0, 0.0, 0.0], [0.299, 0.5959, 0.2115]),
            ([0.0, 1.0, 0.0], [0.587, -0.2746, -0.5227]),
            ([0.0, 0.0, 1.0], [0.114, -0.3213, 0.3112]),
            ([1.0, 1.0, 1.0], [1.0, 0.0, 0.0]),
        ] {
            let got = rgb_to_yiq(rgb);
            assert!(
                got.iter().zip(yiq).all(|(a, b)| (a - b).abs() < 1e-6),
                "{rgb:?}: {got:?}"
            );
        }
    }

    #[test]
    fn the_axes_of_yiq_are_the_published_inverse_and_a_round_trip_returns_the_colour() {
        // the inverse of the FCC matrix, worked out by hand to four places
        for (yiq, rgb) in [
            ([0.0, 1.0, 0.0], [0.9561, -0.2721, -1.1067]),
            ([0.0, 0.0, 1.0], [0.6208, -0.6472, 1.7044]),
            ([1.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        ] {
            let got = yiq_to_rgb(yiq);
            assert!(
                got.iter().zip(rgb).all(|(a, b)| (a - b).abs() < 1e-4),
                "{yiq:?}: {got:?}"
            );
        }
        for red in 0_u8..=4 {
            for green in 0_u8..=4 {
                for blue in 0_u8..=4 {
                    let colour = [red, green, blue].map(|level| f32::from(level) / 4.0);
                    let back = yiq_to_rgb(rgb_to_yiq(colour));
                    assert!(
                        back.iter().zip(colour).all(|(a, b)| (a - b).abs() < 1e-5),
                        "{colour:?}: {back:?}"
                    );
                }
            }
        }
    }
}

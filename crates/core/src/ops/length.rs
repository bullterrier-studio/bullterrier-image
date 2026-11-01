//! Pixel counts as numbers and back, rounded as Python's `round` is, and the most pixels a picture may have.

use crate::Error;

/// The most pixels a picture the core makes may have. A recipe's `size`, `frame` borders and expanded turns can ask for any
/// number, so what they ask is checked against this before a pixel is allocated. A render holds up to three pictures of the
/// working size at once (the working picture, a step's result and the blend of the two, 12 bytes a pixel each), so this is
/// about 2.4 GB of pictures; it does not limit a photo handed in.
const MOST_PIXELS: usize = 1 << 26; // constant: 8192 x 8192; keeps a render near 3 GB, safe on a 16 GB machine

/// Refuses a picture of `height` x `width` pixels if it has more than [`MOST_PIXELS`]; `asked_by` names what asked for
/// it (as `` `size` 4000 `` or `` `frame` ``), so the error says where to look.
///
/// # Errors
/// [`Error::Recipe`] naming `asked_by`, the size and the limit.
pub(crate) fn within_limit(asked_by: &str, height: usize, width: usize) -> Result<(), Error> {
    // saturating: a side near `u32::MAX` squared is still over the limit
    if height.saturating_mul(width) > MOST_PIXELS {
        return Err(Error::Recipe(format!(
            "{asked_by} makes a picture of {width} x {height} pixels, more than the limit of {MOST_PIXELS} pixels"
        )));
    }
    Ok(())
}

/// `length` as a float: lengths are far below 2^53, so nothing is lost.
#[expect(clippy::cast_precision_loss, reason = "a length is far below 2^53")]
pub(crate) const fn float(length: usize) -> f64 {
    length as f64
}

/// `value` rounded to the nearest whole number of pixels, halves to the even one (as Python's `round`): the rounding of
/// every size and position of a frame. A negative value is 0; one too long for a count is the longest.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a rounded length; the cast saturates at both ends"
)]
pub(crate) fn whole(value: f64) -> usize {
    value.round_ties_even() as usize
}

#[cfg(test)]
mod tests {
    use super::{float, whole};

    #[test]
    fn a_half_goes_to_the_even_whole_number_and_a_negative_to_zero() {
        // Python: round(0.5) == 0, round(1.5) == 2, round(2.5) == 2, round(2.4999) == 2, round(-3.2) is below any length
        for (value, expected) in [
            (0.5, 0),
            (1.5, 2),
            (2.5, 2),
            (2.4999, 2),
            (2.5001, 3),
            (-3.2, 0),
            (7.0, 7),
        ] {
            assert_eq!(whole(value), expected, "{value}");
        }
        assert!((float(12_345) - 12_345.0).abs() < f64::EPSILON);
    }
}

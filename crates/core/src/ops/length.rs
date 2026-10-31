//! Pixel counts as numbers and back, rounded as Python's `round` is.

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

//! Cutting a selection by its values: a band of them, chosen fully inside and fading out beyond.

use super::cast::single;
use super::pieces;

/// How much each of `field`'s values is chosen by the band from `low` to `high` with a `feather`: 1 from `low` to
/// `high`, falling linearly to 0 over `feather` beyond either (a `feather` of 0 cuts hard, so a value on a bound is
/// chosen and the next one out is not).
///
/// The arithmetic is single precision, with the three numbers narrowed first, as the values are: a value that is the
/// `f32` nearest to a bound written `0.3` is on the bound, which in double precision it would just be beyond.
pub(crate) fn band(field: &[f32], low: f64, high: f64, feather: f64) -> Vec<f32> {
    let (low, high, feather) = (single(low), single(high), single(feather));
    field
        .iter()
        .map(|&value| {
            // how far beyond the band the value is: negative or 0 inside it, which the clamp and the hard test count as on
            let outside = (low - value).max(value - high);
            if feather <= 0.0 {
                f32::from(u8::from(outside <= 0.0))
            } else {
                (1.0 - outside / feather).clamp(0.0, 1.0)
            }
        })
        .collect()
}

/// The pieces of `field` cut into `count` equal ranges of its scale and the number of pieces: band 0 holds the highest
/// values, so the pieces are numbered from the top. A value on a border between two ranges goes to the lower one. A
/// range no value lies in leaves no piece and the ones below it move up.
///
/// Double precision: a range is where `(1 - value) * count` falls, which in single precision would put a value that is
/// on a border (0.25 of 4 ranges) just on either side of it by the rounding of the product.
pub(crate) fn partition(field: &[f32], count: u32) -> (Vec<i32>, usize) {
    let last = count as usize - 1;
    let range_of: Vec<usize> = field
        .iter()
        .map(|&value| {
            let range = ((1.0 - f64::from(value)) * f64::from(count)).floor();
            #[expect(clippy::cast_possible_truncation, reason = "a whole number from 0 to the count")]
            #[expect(clippy::cast_sign_loss, reason = "from 0 up")]
            let range = range as usize;
            range.min(last)
        })
        .collect();
    let mut present = vec![false; last + 1];
    for &range in &range_of {
        present[range] = true;
    }
    // the number of a range is how many present ranges lie above it
    let mut number = vec![0_i32; last + 1];
    let mut above = 0_i32; // constant: no range lies above the highest
    for (slot, here) in number.iter_mut().zip(&present) {
        *slot = above;
        above += i32::from(*here);
    }
    (
        range_of.iter().map(|&range| number[range]).collect(),
        pieces::count(above),
    )
}

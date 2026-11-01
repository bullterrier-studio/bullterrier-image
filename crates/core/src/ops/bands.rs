//! Cutting a selection by its values: a band of them, chosen fully inside and fading out beyond.

use super::cast::single;

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

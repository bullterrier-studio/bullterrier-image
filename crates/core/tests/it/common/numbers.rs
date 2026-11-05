//! Small numeric conversions the reference computations share.

/// `count` as a float: a count in a test is far below 2^53.
#[expect(clippy::cast_precision_loss, reason = "a count in a test is far below 2^53")]
pub(crate) const fn float(count: usize) -> f64 {
    count as f64
}

/// A small whole number as a float.
pub(crate) fn float_of(number: i64) -> f64 {
    f64::from(i32::try_from(number).unwrap_or(0))
}

/// The sum of `values` in double precision.
pub(crate) fn total(values: &[f32]) -> f64 {
    values.iter().map(|&value| f64::from(value)).sum()
}

//! Narrowing a number written in `f64` to the `f32` a picture holds.

/// `value` as the nearest `f32`. The one place a parameter written as `f64` (a recipe's numbers are) is narrowed to the
/// precision of the maps it is compared with: a value written `0.3` is then the very `f32` a map holds for 0.3.
#[expect(
    clippy::cast_possible_truncation,
    reason = "narrowing is this function's whole purpose"
)]
pub(crate) const fn single(value: f64) -> f32 {
    value as f32
}

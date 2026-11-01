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

#[cfg(test)]
mod tests {
    use super::single;

    #[test]
    fn a_number_becomes_the_nearest_single_the_way_a_map_holds_it() {
        // 0.3 is not representable in either: the f32 nearest to the f64 0.3 is the f32 a map of 0.3 holds, which is not
        // the same number widened back
        assert_eq!(single(0.3).to_bits(), 0x3e99_999a);
        assert!(f64::from(single(0.3)) > 0.3);
        // 0.7 rounds down instead
        assert_eq!(single(0.7).to_bits(), 0x3f33_3333);
        assert!(f64::from(single(0.7)) < 0.7);
        // a whole number and a half are exact
        assert_eq!((single(1.0), single(0.5), single(0.0)), (1.0, 0.5, 0.0));
    }
}

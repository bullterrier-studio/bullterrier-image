//! The standard normal distribution: how far down the bell curve a value lies, the one way effects turn a
//! zero-mean, unit-deviation field into positions spread evenly over 0..1.

/// The standard normal cumulative distribution function at `x`: the chance that a standard normal variable is at most
/// `x`. 0.5 at 0, rising to 1.
pub(crate) fn ndtr(x: f64) -> f64 {
    // `erfc` keeps the small tail accurate, where 1 - erf would lose it
    0.5 * libm::erfc(-x * std::f64::consts::FRAC_1_SQRT_2)
}

#[cfg(test)]
mod tests {
    //! `ndtr` against the tabulated standard normal (Abramowitz and Stegun, table 26.1), its symmetry and its monotonicity.
    //! It is private, and the effects use it only inside a rank, so no public route shows its values.

    use super::ndtr;

    #[test]
    fn known_values_of_the_standard_normal_table() {
        for (x, expected) in [
            (0.0, 0.5),
            (1.0, 0.841_344_746_068_543),
            (1.96, 0.975_002_104_851_780),
            (3.0, 0.998_650_101_968_370),
            (-1.0, 0.158_655_253_931_457),
            (-3.0, 0.001_349_898_031_630),
            (-6.0, 9.865_876_450_377e-10),
        ] {
            assert!(
                (ndtr(x) - expected).abs() < 1e-12 * expected.max(1e-3),
                "ndtr({x}) = {}",
                ndtr(x)
            );
        }
    }

    #[test]
    fn the_two_tails_add_to_one_and_the_function_only_rises() {
        let points: Vec<f64> = (-80..=80).map(|step| f64::from(step) / 10.0).collect();
        for &x in &points {
            assert!((ndtr(x) + ndtr(-x) - 1.0).abs() < 1e-15, "x = {x}");
        }
        assert!(points.windows(2).all(|pair| ndtr(pair[0]) < ndtr(pair[1])));
    }
}

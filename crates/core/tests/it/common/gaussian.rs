//! What the tests of the effects built on a Gaussian blur share: scipy's weights worked out from their definition.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the reach of a kernel is a small whole number"
)]

/// scipy's weights of a Gaussian of `sigma` at distances -reach..=reach (exp(-d^2 / 2 sigma^2), `reach` being
/// `int(truncate x sigma + 0.5)`, summing to 1), and `reach`, the index of the middle one.
pub(crate) fn weights(sigma: f64, truncate: f64) -> (Vec<f64>, usize) {
    let reach = (truncate * sigma + 0.5).floor() as i32;
    let all: Vec<f64> = (-reach..=reach)
        .map(|distance| (-f64::from(distance * distance) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = all.iter().sum();
    (all.iter().map(|weight| weight / total).collect(), reach as usize)
}

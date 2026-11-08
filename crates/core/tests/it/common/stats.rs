//! Sample statistics, for tests of seeded noise at a fixed seed.

use super::numbers::float;

/// The mean of `values`.
pub(crate) fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / float(values.len())
}

/// The population standard deviation of `values`.
pub(crate) fn deviation(values: &[f64]) -> f64 {
    let centre = mean(values);
    (values.iter().map(|value| (value - centre).powi(2)).sum::<f64>() / float(values.len())).sqrt()
}

/// The Pearson correlation of `left` and `right`.
pub(crate) fn correlation(left: &[f64], right: &[f64]) -> f64 {
    let (centre_left, centre_right) = (mean(left), mean(right));
    let covariance: f64 = left
        .iter()
        .zip(right)
        .map(|(l, r)| (l - centre_left) * (r - centre_right))
        .sum();
    let spread = |values: &[f64], centre: f64| values.iter().map(|v| (v - centre).powi(2)).sum::<f64>();
    covariance / (spread(left, centre_left) * spread(right, centre_right)).sqrt()
}

/// The correlation between each value of a `height` x `width` field (row after row) and the one to its right.
pub(crate) fn neighbour_correlation(field: &[f64], width: usize) -> f64 {
    let pairs: Vec<(f64, f64)> = (0..field.len())
        .filter(|at| at % width != width - 1)
        .map(|at| (field[at], field[at + 1]))
        .collect();
    let (here, next): (Vec<f64>, Vec<f64>) = pairs.into_iter().unzip();
    correlation(&here, &next)
}

/// The standard normal cumulative distribution at `x`, by Marsaglia's series (2004), `0.5 + phi(x) (x + x^3/3 + x^5/15 + ...)`:
/// a formula of its own, apart from the core's `erfc`. Accurate to about 1e-15 for `|x|` up to 8.
pub(crate) fn normal_cdf(x: f64) -> f64 {
    let (mut term, mut sum, mut k) = (x, x, 0.0);
    while term.abs() > 1e-17 * sum.abs().max(1e-300) {
        k += 1.0;
        term *= x * x / (2.0 * k + 1.0);
        sum += term;
    }
    0.5 + sum * (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

/// The `x` for which [`normal_cdf`] is `p` (0 < `p` < 1), by bisection.
pub(crate) fn normal_quantile(p: f64) -> f64 {
    let (mut low, mut high) = (-9.0, 9.0);
    for _ in 0..60 {
        let middle = f64::midpoint(low, high);
        if normal_cdf(middle) < p {
            low = middle;
        } else {
            high = middle;
        }
    }
    f64::midpoint(low, high)
}

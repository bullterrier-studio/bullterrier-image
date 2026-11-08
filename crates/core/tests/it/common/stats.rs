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

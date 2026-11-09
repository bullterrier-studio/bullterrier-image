//! The NTSC YIQ of television as the FCC publishes it (47 CFR 73.682), typed here as the oracle's own copy, and the
//! reference of a row of colour smeared by a Gaussian: what the tests of `ntsc` and `vhs` judge their pictures by.

use bullterrier_image::Edge;

use super::edge::folded;
use super::gaussian::weights;

/// (Y, I, Q) of `pixel`: the FCC matrix applied to its red, green and blue.
pub(crate) fn yiq(pixel: &[f32]) -> [f64; 3] {
    let [red, green, blue] = [0, 1, 2].map(|channel| f64::from(pixel[channel]));
    [
        0.299 * red + 0.587 * green + 0.114 * blue,
        0.5959 * red - 0.2746 * green - 0.3213 * blue,
        0.2115 * red - 0.5227 * green + 0.3112 * blue,
    ]
}

/// The (Y, I, Q) of every pixel of `picture` (three channels a pixel, row after row).
pub(crate) fn yiq_of(picture: &[f32]) -> Vec<[f64; 3]> {
    picture.as_chunks::<3>().0.iter().map(|pixel| yiq(pixel)).collect()
}

/// `line` convolved with the Gaussian of `sigma` cut off at `truncate` sigmas, `rule` beyond its ends: from the definition,
/// sample by sample.
pub(crate) fn smoothed(line: &[f64], sigma: f64, truncate: f64, rule: Edge) -> Vec<f64> {
    let (kernel, reach) = weights(sigma, truncate);
    let len = i64::try_from(line.len()).unwrap_or(0);
    let reach = i64::try_from(reach).unwrap_or(0);
    (0..len)
        .map(|at| {
            kernel
                .iter()
                .zip(-reach..=reach)
                .map(|(weight, distance)| weight * line[folded(rule, at + distance, len)])
                .sum()
        })
        .collect()
}

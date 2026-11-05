//! Convolution of a picture with a kernel: every sample becomes a weighted average of the samples around it.
//!
//! ```ignore
//! // a disc of radius 8 px, the edge pixel repeated beyond the picture
//! let disc = Kernel::from_fn((8, 8), |down, right| (8.5 - down.hypot(right)).clamp(0.0, 1.0));
//! let blurred = convolve(pixels, height, width, &disc, Extension::Nearest);
//! ```
//!
//! It is a true convolution, not a correlation: the kernel is flipped, so a kernel that leans to the right puts the
//! light of a pixel to the right of it. The cost grows with the picture and hardly with the kernel: a kernel of a few
//! weights is summed tap by tap, a row of weights is filtered line by line, and any other is filtered by the 2-D
//! Fourier transform of the picture.

use super::blur::Smoothing;
use super::edge::{Extension, Source};
use super::fft::PlaneFilter;
use super::length::float;
use super::par;

/// The most weights a kernel may have to be summed tap by tap: the transform costs the same for any kernel.
const DIRECT_TAPS: usize = 128; // constant: measured at 12 MP, 3 channels, one thread (M4, release): summing 0.13 s and 0.003 s a tap more (0.41 s at 97 taps), the transform 0.69 s whatever the kernel

/// The weights of a convolution: non-negative, summing to 1, over a rectangle of odd sides round a middle weight that
/// weighs the sample itself.
pub(crate) struct Kernel {
    /// How many rows above and below, and columns to the left and right of the middle the weights reach.
    reach: (usize, usize),
    /// The weights row after row, `2 reach.0 + 1` rows of `2 reach.1 + 1`, summing to 1.
    weights: Vec<f64>,
    /// What the weights summed to before they were scaled to 1.
    total: f64,
}

impl Kernel {
    /// The kernel whose weight at `down` rows below and `right` columns to the right of the middle is `weight(down,
    /// right)`, for the rows and columns within `reach` of the middle (`.0` rows, `.1` columns): a weight is not
    /// negative, and at least one is above 0. The weights are scaled to sum to 1, and a margin of zeros is dropped, so
    /// that a line of weights through the middle is a kernel of one row or one column whatever `reach` was.
    pub(crate) fn from_fn(reach: (usize, usize), weight: impl Fn(f64, f64) -> f64) -> Self {
        let (rows, cols) = (2 * reach.0 + 1, 2 * reach.1 + 1);
        let at = |row: usize, col: usize| weight(float(row) - float(reach.0), float(col) - float(reach.1));
        let weights: Vec<f64> = (0..rows * cols).map(|index| at(index / cols, index % cols)).collect();
        // the farthest row and column, from the middle, that holds a weight
        let mut far = (0, 0);
        for (index, _) in weights.iter().enumerate().filter(|(_, weight)| **weight > 0.0) {
            let (row, col) = (index / cols, index % cols);
            far = (far.0.max(row.abs_diff(reach.0)), far.1.max(col.abs_diff(reach.1)));
        }
        let total: f64 = weights.iter().sum();
        let kept = (2 * far.0 + 1, 2 * far.1 + 1);
        let weights = (0..kept.0 * kept.1)
            .map(|index| {
                let (row, col) = (index / kept.1 + reach.0 - far.0, index % kept.1 + reach.1 - far.1);
                weights[row * cols + col] / total
            })
            .collect();
        Self {
            reach: far,
            weights,
            total,
        }
    }

    /// What the weights given to [`Kernel::from_fn`] summed to, before they were scaled to 1: the factor that turns a
    /// mean back into the sum of the weighted samples.
    pub(crate) const fn total(&self) -> f64 {
        self.total
    }

    /// The weights at distance 0, 1, ... from the middle of the one row of a kernel that has one and is the same on
    /// both sides of its middle; none for any other kernel.
    fn half_row(&self) -> Option<Vec<f64>> {
        let row = &self.weights;
        (self.reach.0 == 0
            && (0..self.reach.1).all(|distance| row[distance].to_bits() == row[2 * self.reach.1 - distance].to_bits()))
        .then(|| row[self.reach.1..].to_vec())
    }
}

/// The convolution of `pixels` (`height` x `width` samples of `N` channels, row after row), each channel on its own:
/// output[y, x] is the sum over (i, j) of kernel(i, j) x pixels[y - i, x - j], where `edge` says what lies beyond the
/// picture. The weights are positive and sum to 1, so a value is a mean of what it averages: it lies within the range
/// of the channel and of what `edge` adds to it, which float rounding in the sums, and in the transform, is held to.
///
/// A kernel of one row is applied to each row alone and a kernel of up to `DIRECT_TAPS` weights is summed tap by tap, both as exact as
/// float sums are; any other is filtered by the Fourier transform, within float rounding of the sum. The result is the
/// same on any number of threads.
pub(crate) fn convolve<const N: usize>(
    pixels: &[[f32; N]],
    height: usize,
    width: usize,
    kernel: &Kernel,
    edge: Extension,
) -> Vec<[f32; N]> {
    let planes: Vec<Vec<f32>> = (0..N)
        .map(|channel| pixels.iter().map(|pixel| pixel[channel]).collect())
        .collect();
    let mut done = Method::of(kernel, height, width).apply(&planes, height, width, kernel.reach, edge);
    for (done, plane) in done.iter_mut().zip(&planes) {
        hold_to_range(done, plane, edge);
    }
    let mut convolved = vec![[0.0; N]; pixels.len()];
    for (channel, plane) in done.iter().enumerate() {
        for (pixel, value) in convolved.iter_mut().zip(plane) {
            pixel[channel] = *value;
        }
    }
    convolved
}

/// How a kernel is applied.
enum Method {
    /// Summed tap by tap: the rows and columns of the extended plane to read, and the weight, of each weight above 0.
    Taps(Vec<(usize, usize, f32)>),
    /// A kernel of one row, the same on both sides of its middle, applied to each row alone.
    Rows(Smoothing),
    /// The 2-D Fourier transform.
    Plane(PlaneFilter),
}

impl Method {
    fn of(kernel: &Kernel, height: usize, width: usize) -> Self {
        if let Some(half) = kernel.half_row() {
            return Self::Rows(Smoothing::new(&half, width));
        }
        let cols = 2 * kernel.reach.1 + 1;
        let taps: Vec<(usize, usize, f32)> = kernel
            .weights
            .iter()
            .enumerate()
            .filter(|(_, weight)| **weight > 0.0)
            // the flip: the weight at row a, column b, reads the sample 2 reach - a below and 2 reach - b right of the
            // output's place in the extended plane
            .map(|(index, &weight)| {
                #[expect(clippy::cast_possible_truncation, reason = "narrowing is the point")]
                let weight = weight as f32;
                (2 * kernel.reach.0 - index / cols, 2 * kernel.reach.1 - index % cols, weight)
            })
            .collect();
        if taps.len() <= DIRECT_TAPS {
            Self::Taps(taps)
        } else {
            Self::Plane(PlaneFilter::new(&kernel.weights, kernel.reach, height, width))
        }
    }

    /// `planes` convolved.
    fn apply(
        &self,
        planes: &[Vec<f32>],
        height: usize,
        width: usize,
        reach: (usize, usize),
        edge: Extension,
    ) -> Vec<Vec<f32>> {
        let padded = |plane| extended(plane, height, width, reach, edge);
        match self {
            Self::Taps(taps) => planes
                .iter()
                .map(|plane| {
                    let (padded, wide) = (padded(plane), width + 2 * reach.1);
                    let mut out = vec![0.0; plane.len()];
                    par::rows(&mut out, width, |first, band| {
                        for (offset, row) in band.chunks_exact_mut(width).enumerate() {
                            for &(down, right, weight) in taps {
                                let from = (first + offset + down) * wide + right;
                                for (sum, &sample) in row.iter_mut().zip(&padded[from..]) {
                                    *sum += weight * sample;
                                }
                            }
                        }
                    });
                    out
                })
                .collect(),
            Self::Rows(smoothing) => planes
                .iter()
                .map(|plane| {
                    let mut out = plane.clone();
                    smoothing.smooth(&mut out, width, edge);
                    out
                })
                .collect(),
            // two planes at a time, as the transform filters two
            Self::Plane(filter) => planes
                .chunks(2)
                .flat_map(|pair| {
                    let (first, second) =
                        filter.apply(&padded(&pair[0]), pair.get(1).map(|plane| padded(plane)).as_deref());
                    std::iter::once(first).chain(second)
                })
                .collect(),
        }
    }
}

/// `plane` with `reach.0` rows beyond its top and bottom and `reach.1` columns beyond its sides, as `edge` says.
fn extended(plane: &[f32], height: usize, width: usize, reach: (usize, usize), edge: Extension) -> Vec<f32> {
    let wide = width + 2 * reach.1;
    let mut out = Vec::with_capacity((height + 2 * reach.0) * wide);
    let mut row = Vec::new();
    for index in 0..height + 2 * reach.0 {
        match edge.locate(height, index.cast_signed() - reach.0.cast_signed()) {
            Source::Sample(source) => {
                edge.extend(&plane[source * width..(source + 1) * width], reach.1, &mut row);
                out.extend_from_slice(&row);
            }
            Source::Value(value) => out.resize(out.len() + wide, value),
        }
    }
    out
}

/// `done` held to the range of `plane` and of what `edge` puts beyond it: a mean of them lies within it.
fn hold_to_range(done: &mut [f32], plane: &[f32], edge: Extension) {
    let beyond = match edge {
        Extension::Constant(value) => (value, value),
        _ => (f32::INFINITY, f32::NEG_INFINITY),
    };
    let (low, high) = plane
        .iter()
        .fold(beyond, |(low, high), &value| (low.min(value), high.max(value)));
    for value in done {
        *value = value.clamp(low, high);
    }
}

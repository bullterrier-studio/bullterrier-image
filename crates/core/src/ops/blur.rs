//! Gaussian blurs. [`gaussian`] blurs a one-channel map by three box blurs (Kovesi, "Fast Almost-Gaussian Filtering",
//! 2010), so that it costs the same at every radius: a few additions per pixel, however wide the blur, and the edge
//! pixel repeated beyond the map. [`gaussian_filter`] blurs a picture by the Gaussian itself, as scipy's
//! `gaussian_filter` does.

use super::edge::Extension;
use super::fft::LineFilter;
use super::length::float;
use super::par;
use crate::Picture;

/// How many box blurs make the Gaussian: three are within about 1% of it (see the tests).
const PASSES: usize = 3; // constant: Kovesi's n; the variance of the boxes is matched to sigma, the shape is near-Gaussian

/// The radius in pixels of each box blur of the `PASSES` that together have the variance of a Gaussian of `sigma` px: a
/// box of radius `r` is `2 r + 1` pixels wide and has variance `r (r + 1) / 3`. Two widths (odd, two apart) are mixed
/// to meet the variance as nearly as whole pixels allow; a sigma under about 0.6 px gets none (radius 0 each), which
/// leaves a map as it is.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a width in pixels, small and positive"
)]
fn radii(sigma: f64) -> [usize; PASSES] {
    let passes = float(PASSES);
    let ideal = (12.0 * sigma * sigma / passes + 1.0).sqrt(); // constant: Kovesi eq. 3, the ideal box width
    let mut narrow = ideal.floor() as usize;
    if narrow.is_multiple_of(2) {
        narrow -= 1;
    }
    let (narrow_f, wide) = (float(narrow), narrow + 2);
    // how many of the boxes are the narrow width, so that their variances add up to sigma^2 (Kovesi, eq. 5)
    // constant: the numbers 12, 4 and 3 are Kovesi's eq. 5
    let narrow_count = ((12.0 * sigma * sigma - passes * narrow_f * narrow_f - 4.0 * passes * narrow_f - 3.0 * passes)
        / (-4.0 * narrow_f - 4.0))
        .round() as usize;
    std::array::from_fn(|pass| (if pass < narrow_count { narrow } else { wide } - 1) / 2)
}

/// Blurs the `height` x `width` map `values` (row after row) by a Gaussian of `sigma` px, first along the rows, then down
/// the columns, the edge pixel repeated beyond the picture. A weighted mean of its neighbours, so a value never leaves the
/// range of the map's. `sigma` is finite and at least 0; 0 leaves the map as it is.
pub(crate) fn gaussian(values: &mut [f32], height: usize, width: usize, sigma: f64) {
    let radii = radii(sigma);
    if radii.iter().all(|&radius| radius == 0) {
        return;
    }
    let mut smoother = Smoother::new(radii);
    for row in values.chunks_exact_mut(width) {
        smoother.smooth(row);
    }
    let mut column = vec![0.0_f32; height];
    for start in 0..width {
        for (cell, value) in column.iter_mut().zip(values[start..].iter().step_by(width)) {
            *cell = *value;
        }
        smoother.smooth(&mut column);
        for (value, cell) in values[start..].iter_mut().step_by(width).zip(&column) {
            *value = *cell;
        }
    }
}

/// Smoothing of lines by the box blurs `radii`, with the scratch space it needs kept between lines.
struct Smoother {
    radii: [usize; PASSES],
    /// The line with its edge values repeated by the reach of all the boxes on both sides.
    padded: Vec<f32>,
    /// The padded line as it was before a box blur.
    before: Vec<f32>,
}

impl Smoother {
    const fn new(radii: [usize; PASSES]) -> Self {
        Self {
            radii,
            padded: Vec::new(),
            before: Vec::new(),
        }
    }

    /// `line` blurred, the edge value repeated beyond its ends. The repeat is made once, wide enough that each box
    /// reads only the repeated values and the blurred ones (what blurring the line extended forever would give), not an
    /// edge repeated again after each box.
    fn smooth(&mut self, line: &mut [f32]) {
        let reach: usize = self.radii.iter().sum();
        let (first, last) = (line[0], line[line.len() - 1]);
        self.padded.clear();
        self.padded.resize(reach, first);
        self.padded.extend_from_slice(line);
        self.padded.resize(reach + line.len() + reach, last);
        for radius in self.radii {
            self.before.clone_from(&self.padded);
            boxed(&self.before, radius, &mut self.padded);
        }
        line.copy_from_slice(&self.padded[reach..reach + line.len()]);
    }
}

/// `out` as the mean of the `2 radius + 1` values of `line` around each position, the window slid along so that the
/// cost does not depend on `radius`; the positions within `radius` of an end read the end value, which the caller has
/// padded so far that this is not seen in the line it keeps.
fn boxed(line: &[f32], radius: usize, out: &mut [f32]) {
    let last = line.len() - 1;
    let width = float(2 * radius + 1);
    // the window of position 0 is the first value `radius + 1` times and the next `radius` values
    let mut sum =
        f64::from(line[0]) * float(radius) + (0..=radius).map(|at| f64::from(line[at.min(last)])).sum::<f64>();
    for (position, out) in out.iter_mut().enumerate() {
        #[expect(clippy::cast_possible_truncation, reason = "narrowing is the point")]
        {
            *out = (sum / width) as f32;
        }
        sum += f64::from(line[(position + radius + 1).min(last)]) - f64::from(line[position.saturating_sub(radius)]);
    }
}

/// The kernel reach in px from which the Fourier transform beats summing the taps: summing costs a tap for each of the
/// `2 reach + 1` weights of a sample, the transform the same whatever the reach.
const FFT_FROM_REACH: usize = 16; // constant: measured at 12 MP on one thread (M4): summing 0.65 s at reach 16, 0.32 s at 4; the transform 0.42 s at any reach

/// `picture` blurred by a Gaussian of standard deviation `sigma` px, each channel alone, first along the rows, then down
/// the columns (a Gaussian is separable). Scipy's `gaussian_filter` is the definition: the kernel reaches
/// `int(truncate * sigma + 0.5)` px and its weights are `exp(-d^2 / 2 sigma^2)` for distance `d`, normalised to sum 1;
/// `edge` is what lies beyond the picture's edge. A sigma under about `0.5 / truncate` px has a kernel of one tap and
/// leaves the picture as it is. A weighted mean of its neighbours, so a value stays within the range of the values it
/// averages. `sigma` is finite and at least 0, `truncate` finite and at least 1.
///
/// A kernel reaching under `FFT_FROM_REACH` px is summed tap by tap, as scipy does; a longer one is filtered by the Fourier transform,
/// within float rounding of the sum, in time that grows with the picture and not with the kernel. Memory is the
/// picture's three channels once more, and one channel twice, plus a few lines for each thread.
pub(crate) fn gaussian_filter(picture: &Picture<'_>, sigma: f64, truncate: f64, edge: Extension) -> Vec<[f32; 3]> {
    let (height, width) = (picture.height(), picture.width());
    let pixels = picture.pixels();
    let half = weights(sigma, truncate);
    if half.len() == 1 {
        return pixels.to_vec();
    }
    let (along, down) = (Smoothing::new(&half, width), Smoothing::new(&half, height));
    let mut blurred = vec![[0.0; 3]; pixels.len()];
    for channel in 0..3 {
        let mut plane: Vec<f32> = pixels.iter().map(|pixel| pixel[channel]).collect();
        along.smooth(&mut plane, width, edge);
        let mut turned = transposed(&plane, height, width); // `width` rows of `height`
        down.smooth(&mut turned, height, edge);
        plane = transposed(&turned, width, height);
        for (pixel, value) in blurred.iter_mut().zip(plane) {
            pixel[channel] = value;
        }
    }
    blurred
}

/// The weights of the Gaussian kernel at distance 0, 1, ... from a sample, to its reach, summing to 1 when the other
/// side is counted too.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "scipy's int(): a reach in px, small and positive"
)]
fn weights(sigma: f64, truncate: f64) -> Vec<f64> {
    let reach = (truncate * sigma + 0.5) as usize; // constant: how scipy sizes its kernel
    if reach == 0 {
        return vec![1.0]; // also sigma 0, which would divide by zero below
    }
    let mut half: Vec<f64> = (0..=reach)
        .map(|distance| (-0.5 * (float(distance) / sigma).powi(2)).exp())
        .collect();
    let total = 2.0 * half[1..].iter().sum::<f64>() + half[0];
    for weight in &mut half {
        *weight /= total;
    }
    half
}

/// How the lines of one length are blurred by one kernel.
enum Smoothing {
    /// Summed tap by tap, the weights at distance 0, 1, ... from a sample.
    Direct(Vec<f64>),
    Fourier(LineFilter),
}

impl Smoothing {
    fn new(half: &[f64], len: usize) -> Self {
        if half.len() - 1 < FFT_FROM_REACH {
            Self::Direct(half.to_vec())
        } else {
            Self::Fourier(LineFilter::new(half, len))
        }
    }

    /// The lines of `len` samples that `lines` holds, one after the other, blurred in place. The lines are taken two at
    /// a time from the first, however the rows are cut into bands, so that the result does not depend on the threads.
    fn smooth(&self, lines: &mut [f32], len: usize, edge: Extension) {
        par::rows(lines, 2 * len, |_, band| match self {
            Self::Direct(half) => {
                let (mut extended, mut sums) = (Vec::new(), Vec::new());
                for line in band.chunks_exact_mut(len) {
                    direct(half, line, edge, &mut extended, &mut sums);
                }
            }
            Self::Fourier(filter) => {
                let mut scratch = filter.scratch();
                for pair in band.chunks_mut(2 * len) {
                    let (first, second) = pair.split_at_mut(len);
                    filter.apply(first, (!second.is_empty()).then_some(second), edge, &mut scratch);
                }
            }
        });
    }
}

/// `line` blurred in place by the symmetric kernel of weights `half`; `extended` and `sums` are buffers for the line with
/// its edges extended and for the sums being made. Tap by tap over the whole line, so that the sums of neighbouring
/// samples, which do not depend on each other, are made side by side.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the sum is made in f64 and stored in the f32 picture"
)]
fn direct(half: &[f64], line: &mut [f32], edge: Extension, extended: &mut Vec<f32>, sums: &mut Vec<f64>) {
    let reach = half.len() - 1;
    edge.extend(line, reach, extended);
    sums.clear();
    sums.extend(
        extended[reach..reach + line.len()]
            .iter()
            .map(|&value| half[0] * f64::from(value)),
    );
    for (distance, weight) in half.iter().enumerate().skip(1) {
        let (before, after) = (&extended[reach - distance..], &extended[reach + distance..]);
        for ((sum, &before), &after) in sums.iter_mut().zip(before).zip(after) {
            *sum += weight * (f64::from(before) + f64::from(after));
        }
    }
    for (value, sum) in line.iter_mut().zip(sums.iter()) {
        *value = *sum as f32;
    }
}

/// The `rows` x `cols` map `values` turned on its side: `cols` rows of `rows`.
fn transposed(values: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    const TILE: usize = 16; // constant: output rows made together, so that a read takes a whole 64-byte cache line of f32
    let mut turned = vec![0.0; values.len()];
    par::rows(&mut turned, rows, |first, band| {
        for (tile, out) in band.chunks_mut(TILE * rows).enumerate() {
            let first_col = first + tile * TILE;
            for row in 0..rows {
                let from = &values[row * cols + first_col..][..out.len() / rows];
                for (col, value) in from.iter().enumerate() {
                    out[col * rows + row] = *value;
                }
            }
        }
    });
    turned
}

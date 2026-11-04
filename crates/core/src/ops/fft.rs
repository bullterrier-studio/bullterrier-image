//! Filtering of lines of samples, and of planes, by a kernel through the Fourier transform, at a cost that grows with
//! the size of what is filtered and hardly with the kernel's reach.
//!
//! Two real lines (or planes) are filtered by one complex transform (one in the real part, one in the imaginary part:
//! the kernel is real, so each part is filtered alone), which halves the work. Which goes with which is the caller's to
//! fix by position, never by the thread that has them, so the result does not depend on the number of threads.

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

use super::blur::transposed;
use super::edge::Extension;
use super::length::float;
use super::par;

/// The length of the transform of `n` samples: the smallest of 2^k and 3 x 2^k that holds them. rustfft runs these two
/// families at the same time per sample (1.8 ns on one core of an M4, lines of 4096, 6144 and 8192), where any other length,
/// 5-smooth ones included, takes 1.4 to 2.4 times as long: more than the padding it saves (measured for the Gaussian at
/// 12 MP: 0.50 s at a power of two, 1.04 s at 4320).
pub(crate) fn transform_length(n: usize) -> usize {
    let two = n.next_power_of_two();
    let three = 3 * n.div_ceil(3).next_power_of_two(); // constant: 3 x 2^k, the other length rustfft runs at full speed
    two.min(three)
}

/// A filter for lines of one length by one kernel, planned once and shared between threads (each thread has its own
/// [`Scratch`]).
pub(crate) struct LineFilter {
    /// How far the kernel reaches on each side of a sample.
    reach: usize,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    /// The kernel's spectrum divided by the transform's length (the inverse transform does not scale). A symmetric
    /// kernel has a real spectrum.
    response: Vec<f32>,
}

/// The buffers a thread filters lines with, made by [`LineFilter::scratch`].
pub(crate) struct Scratch {
    /// The two lines with their edges extended, as the real and imaginary parts of the signal the transform reads.
    extended: [Vec<f32>; 2],
    spectrum: Vec<Complex<f32>>,
    work: Vec<Complex<f32>>,
}

impl LineFilter {
    /// A filter of lines of `len` samples by the kernel whose weights at distance 0, 1, ... `reach` from a sample are
    /// `half` (the kernel is symmetric, so the other side is the same).
    pub(crate) fn new(half: &[f64], len: usize) -> Self {
        let reach = half.len() - 1;
        let size = transform_length(len + 2 * reach); // the extended line fits without wrapping into itself
        // The kernel centred on sample 0 of a circular buffer, so that output sample i lines up with input sample i.
        let mut centred = vec![Complex::new(0.0_f64, 0.0); size];
        centred[0].re = half[0];
        for (distance, weight) in half.iter().enumerate().skip(1) {
            centred[distance].re = *weight;
            centred[size - distance].re = *weight;
        }
        // the spectrum in f64: it is read for every line, and its rounding is the filter's
        let scale = float(size);
        FftPlanner::<f64>::new().plan_fft_forward(size).process(&mut centred);
        #[expect(clippy::cast_possible_truncation, reason = "narrowing is the point")]
        let response = centred.iter().map(|bin| (bin.re / scale) as f32).collect();
        let mut planner = FftPlanner::<f32>::new();
        Self {
            reach,
            forward: planner.plan_fft_forward(size),
            inverse: planner.plan_fft_inverse(size),
            response,
        }
    }

    /// Buffers for filtering lines with this filter.
    pub(crate) fn scratch(&self) -> Scratch {
        let work = self
            .forward
            .get_inplace_scratch_len()
            .max(self.inverse.get_inplace_scratch_len());
        Scratch {
            extended: [Vec::new(), Vec::new()],
            spectrum: vec![Complex::new(0.0, 0.0); self.response.len()],
            work: vec![Complex::new(0.0, 0.0); work],
        }
    }

    /// `first` and `second`, if there is one, filtered in place, each as if `edge` extended it beyond its ends. Both
    /// have the length the filter was made for. The weights are positive and sum to 1, so a result lies within the
    /// range of the line it comes from and of what `edge` adds to it: the result is held to it, which moves the
    /// transform's rounding error toward the truth, never outside.
    pub(crate) fn apply(&self, first: &mut [f32], second: Option<&mut [f32]>, edge: Extension, scratch: &mut Scratch) {
        let Scratch {
            extended,
            spectrum,
            work,
        } = scratch;
        edge.extend(first, self.reach, &mut extended[0]);
        match &second {
            Some(second) => edge.extend(second, self.reach, &mut extended[1]),
            None => extended[1].clear(), // read as zeros below
        }
        let ranges = [0, 1].map(|part| range(&extended[part]));
        spectrum.fill(Complex::new(0.0, 0.0));
        for (bin, &value) in spectrum.iter_mut().zip(&extended[0]) {
            bin.re = value;
        }
        for (bin, &value) in spectrum.iter_mut().zip(&extended[1]) {
            bin.im = value;
        }
        self.forward.process_with_scratch(spectrum, work);
        for (bin, weight) in spectrum.iter_mut().zip(&self.response) {
            bin.re *= *weight;
            bin.im *= *weight;
        }
        self.inverse.process_with_scratch(spectrum, work);
        let inside = &spectrum[self.reach..];
        for (value, bin) in first.iter_mut().zip(inside) {
            *value = bin.re.clamp(ranges[0].0, ranges[0].1);
        }
        if let Some(second) = second {
            for (value, bin) in second.iter_mut().zip(inside) {
                *value = bin.im.clamp(ranges[1].0, ranges[1].1);
            }
        }
    }
}

/// The smallest and largest of `values`.
fn range(values: &[f32]) -> (f32, f32) {
    values
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), &value| {
            (low.min(value), high.max(value))
        })
}

/// A filter for planes of one size by one 2-D kernel, planned once. A plane arrives already extended by the kernel's
/// reach on every side (the caller's, because the edge rule is), and the filter gives back the part within.
pub(crate) struct PlaneFilter {
    /// The size of the transform, at least that of the extended planes (see [`transform_length`]).
    rows: usize,
    cols: usize,
    /// The size of the extended planes.
    extended: (usize, usize),
    /// The reach of the kernel: the margin of an extended plane.
    reach: (usize, usize),
    along: Planned,
    down: Planned,
    /// The kernel's spectrum divided by the number of samples (the inverse transform does not scale), in the layout the
    /// planes have between the two passes: `cols` rows of `rows` bins.
    response: Vec<Complex<f32>>,
}

/// The transforms of one length.
struct Planned {
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
}

impl Planned {
    fn new(len: usize) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            forward: planner.plan_fft_forward(len),
            inverse: planner.plan_fft_inverse(len),
        }
    }

    /// Every line of `bins` (rows of the length planned for) transformed, one band after the other.
    fn rows(&self, bins: &mut [Complex<f32>], len: usize, inverse: bool) {
        let fft = if inverse { &self.inverse } else { &self.forward };
        par::rows(bins, len, |_, band| {
            let mut work = vec![Complex::new(0.0, 0.0); fft.get_inplace_scratch_len()];
            fft.process_with_scratch(band, &mut work);
        });
    }
}

impl PlaneFilter {
    /// A filter for `height` x `width` planes by the kernel `weights` (row after row, `2 reach.0 + 1` rows of
    /// `2 reach.1 + 1`), which turns the plane's samples into the kernel's weighted sums as a convolution does: the
    /// kernel's middle weighs the sample itself and the kernel is flipped.
    pub(crate) fn new(weights: &[f64], reach: (usize, usize), height: usize, width: usize) -> Self {
        let extended = (height + 2 * reach.0, width + 2 * reach.1);
        // The extended plane fits without wrapping into itself.
        let (rows, cols) = (transform_length(extended.0), transform_length(extended.1));
        let (along, down) = (Planned::new(cols), Planned::new(rows));
        // The kernel in the corner of a circular buffer, its first row and column at sample (0, 0): the convolution of an
        // extended plane with it puts what the kernel's middle weighs at `2 reach` samples further along than the sample
        // itself, which `apply` reads from.
        let mut corner = vec![Complex::new(0.0_f32, 0.0); rows * cols];
        let scale = float(rows * cols);
        let kernel_cols = 2 * reach.1 + 1;
        for (index, weight) in weights.iter().enumerate() {
            let (row, col) = (index / kernel_cols, index % kernel_cols);
            #[expect(clippy::cast_possible_truncation, reason = "narrowing is the point")]
            {
                corner[row * cols + col].re = (weight / scale) as f32;
            }
        }
        let mut filter = Self {
            rows,
            cols,
            extended,
            reach,
            along,
            down,
            response: Vec::new(),
        };
        filter.response = filter.spectrum(corner, 2 * reach.0 + 1);
        filter
    }

    /// The spectrum of `bins`, `rows` x `cols` of which only the first `filled` rows are not zero (a row of zeros has a
    /// spectrum of zeros), in the turned layout.
    fn spectrum(&self, mut bins: Vec<Complex<f32>>, filled: usize) -> Vec<Complex<f32>> {
        self.along.rows(&mut bins[..filled * self.cols], self.cols, false);
        let mut turned = transposed(&bins, self.rows, self.cols);
        self.down.rows(&mut turned, self.rows, false);
        turned
    }

    /// The `height` x `width` parts within `first` and `second`, if there is one, convolved with the kernel. Both are
    /// `extended` planes (row after row) and the parts are the planes less the margin of the kernel's reach: the
    /// samples whose neighbours the kernel reads are all within the plane, so none wraps around.
    pub(crate) fn apply(&self, first: &[f32], second: Option<&[f32]>) -> (Vec<f32>, Option<Vec<f32>>) {
        let (rows, cols) = (self.rows, self.cols);
        let mut bins = vec![Complex::new(0.0_f32, 0.0); rows * cols];
        par::rows(&mut bins, cols, |first_row, band| {
            for (offset, line) in band.chunks_exact_mut(cols).enumerate() {
                let row = first_row + offset;
                if row >= self.extended.0 {
                    continue;
                }
                let at = row * self.extended.1..(row + 1) * self.extended.1;
                for (bin, &value) in line.iter_mut().zip(&first[at.clone()]) {
                    bin.re = value;
                }
                for (bin, &value) in line.iter_mut().zip(second.map_or(&[][..], |second| &second[at])) {
                    bin.im = value;
                }
            }
        });
        let mut turned = self.spectrum(bins, self.extended.0);
        par::rows(&mut turned, rows, |first_row, band| {
            let responses = &self.response[first_row * rows..];
            for (bin, response) in band.iter_mut().zip(responses) {
                *bin *= response;
            }
        });
        self.down.rows(&mut turned, rows, true);
        let mut bins = transposed(&turned, cols, rows);
        drop(turned);
        let (height, width) = (self.extended.0 - 2 * self.reach.0, self.extended.1 - 2 * self.reach.1);
        // only the rows that are kept are transformed back
        let kept = 2 * self.reach.0..2 * self.reach.0 + height;
        self.along
            .rows(&mut bins[kept.start * cols..kept.end * cols], cols, true);
        let part = |pick: fn(&Complex<f32>) -> f32| {
            let mut plane = vec![0.0; height * width];
            par::rows(&mut plane, width, |first_row, band| {
                for (offset, line) in band.chunks_exact_mut(width).enumerate() {
                    let from = (2 * self.reach.0 + first_row + offset) * cols + 2 * self.reach.1;
                    for (value, bin) in line.iter_mut().zip(&bins[from..]) {
                        *value = pick(bin);
                    }
                }
            });
            plane
        };
        (part(|bin| bin.re), second.map(|_| part(|bin| bin.im)))
    }
}

//! Filtering of lines of samples by a symmetric kernel through the Fourier transform, at a cost per line that grows with
//! the line's length and hardly with the kernel's reach.
//!
//! Two real lines are filtered by one complex transform (one in the real part, one in the imaginary part: the kernel is
//! real, so each part is filtered alone), which halves the work. Which line goes with which is the caller's to fix by
//! the lines' positions, never by the thread that has them, so the result does not depend on the number of threads.

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

use super::edge::Extension;
use super::length::float;

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
        // A power of two: rustfft's radix-4 transform runs about twice as fast as its mixed-radix one for the lengths in
        // between (measured at 12 MP: 0.50 s against 1.04 s for sigma 40), which more than pays for the longer line.
        let size = (len + 2 * reach).next_power_of_two(); // the extended line fits without wrapping into itself
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

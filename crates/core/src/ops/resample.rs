//! Changing the size of a picture or a map without leaving float precision.
//!
//! The same resampling as Pillow's `Image.resize` on float data, so the picture a recipe is rendered at is the one
//! Python made: a separable kernel (rows, then columns) whose support is widened by the shrink, so that every input
//! pixel under an output pixel counts (no aliasing), with each output pixel's weights summing to 1 (a constant stays
//! that constant). A shrink uses the Lanczos kernel, a growth the bicubic one.

use std::cmp::Ordering;
use std::f64::consts::PI;

use super::length::{float, within_limit};
use super::par;
use crate::Error;

/// A kernel: the weight an input pixel at distance `x` (in output pixels' worth) from the output pixel's centre has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    /// `sinc(x) sinc(x / 3)` within 3 of the centre: sharp, for a shrink.
    Lanczos,
    /// The cubic convolution kernel of Keys with `a = -0.5` within 2 of the centre: smooth, for a growth.
    Bicubic,
}

impl Filter {
    /// How far from the output pixel's centre, in output pixels, the kernel reaches.
    const fn support(self) -> f64 {
        match self {
            Self::Lanczos => 3.0, // constant: Lanczos a = 3, Pillow's `LANCZOS` support
            Self::Bicubic => 2.0, // constant: the cubic convolution kernel is zero from 2 on, Pillow's `BICUBIC` support
        }
    }

    /// The weight at distance `x` from the centre.
    fn weight(self, x: f64) -> f64 {
        let x = x.abs();
        match self {
            Self::Lanczos if x < self.support() => sinc(x) * sinc(x / self.support()),
            // constant: the coefficients of Keys' cubic convolution kernel, as Pillow writes them
            Self::Bicubic if x < 1.0 => ((KEYS + 2.0) * x - (KEYS + 3.0)) * x * x + 1.0,
            // constant: the coefficients of Keys' cubic convolution kernel, as Pillow writes them
            Self::Bicubic if x < self.support() => (((x - 5.0) * x + 8.0) * x - 4.0) * KEYS,
            Self::Lanczos | Self::Bicubic => 0.0,
        }
    }
}

/// The free parameter of the cubic convolution kernel.
const KEYS: f64 = -0.5; // constant: Keys 1981's a = -0.5 (it reproduces a quadratic exactly), Pillow's `BICUBIC`

/// `sin(pi x) / (pi x)`, 1 at 0.
fn sinc(x: f64) -> f64 {
    if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) }
}

/// The input pixels that make one output pixel: the first, and the weight of it and of each pixel after it.
struct Tap {
    first: usize,
    weights: Vec<f64>,
}

/// `from` input pixels along one axis resampled to `to` by `filter`: the taps of every output pixel, whose weights sum
/// to 1.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the edges of the taps are rounded positions inside the axis"
)]
fn taps(filter: Filter, from: usize, to: usize) -> Vec<Tap> {
    let scale = float(from) / float(to);
    // a shrink widens the kernel by the factor, so it reaches every pixel an output pixel covers
    let widened = scale.max(1.0);
    let support = filter.support() * widened;
    (0..to)
        .map(|index| {
            let centre = (float(index) + 0.5) * scale;
            let first = (centre - support + 0.5).max(0.0) as usize;
            let end = ((centre + support + 0.5) as usize).min(from);
            let mut weights: Vec<f64> = (first..end)
                .map(|pixel| filter.weight((float(pixel) - centre + 0.5) / widened))
                .collect();
            let total: f64 = weights.iter().sum();
            if total != 0.0 {
                for weight in &mut weights {
                    *weight /= total;
                }
            }
            Tap { first, weights }
        })
        .collect()
}

/// `side` scaled by `longest / long` and rounded to the nearest whole pixel, halves to the even one, at least 1. Integer
/// arithmetic, so that an exact half (85 x 455 / 170 = 227.5) is a half, which a float product can land below. `side` is
/// at most `long`, so the result is at most `longest`.
fn proportional(side: usize, longest: u32, long: usize) -> usize {
    let (side, longest, long) = (side as u128, u128::from(longest), long as u128);
    let (quotient, rest) = ((side * longest) / long, (side * longest) % long);
    let up = match (2 * rest).cmp(&long) {
        Ordering::Greater => true,
        Ordering::Equal => quotient % 2 == 1,
        Ordering::Less => false,
    };
    ((quotient + u128::from(up)) as usize).max(1)
}

/// A change of a picture's or a map's size, planned once from the size it starts at, so that a picture and the maps of
/// it (which have its size) are all resized to the same grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Resize {
    from: (usize, usize),
    to: (usize, usize),
    filter: Filter,
}

impl Resize {
    /// The change of a `height` x `width` thing to the size whose longest side is `longest` px, the aspect kept: the
    /// other side is the shorter one scaled and rounded to the nearest whole pixel (halves to the even one), at least 1.
    /// A shrink uses the Lanczos kernel, a growth the bicubic one. None if it already has that size.
    ///
    /// # Errors
    /// [`Error::Recipe`] if the new size has more pixels than a picture may have (see [`within_limit`]).
    pub(crate) fn to_longest_side(height: usize, width: usize, longest: u32) -> Result<Option<Self>, Error> {
        let scale = f64::from(longest) / float(height.max(width)); // only picks the filter: shrink or growth
        let long = height.max(width);
        let scaled = |side: usize| proportional(side, longest, long);
        let to = (scaled(height), scaled(width));
        if to == (height, width) {
            return Ok(None);
        }
        within_limit(&format!("`size` {longest}"), to.0, to.1)?;
        let filter = if scale < 1.0 { Filter::Lanczos } else { Filter::Bicubic };
        Ok(Some(Self {
            from: (height, width),
            to,
            filter,
        }))
    }

    /// The number of rows after the change.
    pub(crate) const fn height(&self) -> usize {
        self.to.0
    }

    /// The number of columns after the change.
    pub(crate) const fn width(&self) -> usize {
        self.to.1
    }

    /// `pixels`, the pixels of the thing the change was planned for (`C` samples each, row after row), resampled: the
    /// same layout at the new size, every sample within 0..1 (a sharp kernel overshoots near an edge, which a picture
    /// cannot show). A change of the size is planned, so at least one side differs.
    pub(crate) fn apply<const C: usize>(&self, pixels: &[[f32; C]]) -> Vec<[f32; C]> {
        let ((rows, columns), (new_rows, new_columns)) = (self.from, self.to);
        let wide = (columns != new_columns).then(|| across(pixels, rows, &taps(self.filter, columns, new_columns)));
        let mut out = if rows == new_rows {
            wide.unwrap_or_else(|| pixels.to_vec())
        } else {
            down(
                wide.as_deref().unwrap_or(pixels),
                new_columns,
                &taps(self.filter, rows, new_rows),
            )
        };
        par::rows(&mut out, new_columns, |_, band| {
            for sample in band.iter_mut().flatten() {
                *sample = sample.clamp(0.0, 1.0);
            }
        });
        out
    }
}

/// `pixels` (`rows` rows) with every row resampled to the columns `taps` make. The sums are in f64 and rounded once to
/// f32, as Pillow does.
fn across<const C: usize>(pixels: &[[f32; C]], rows: usize, taps: &[Tap]) -> Vec<[f32; C]> {
    let columns = pixels.len() / rows;
    let mut out = vec![[0.0; C]; rows * taps.len()];
    par::rows(&mut out, taps.len(), |first_row, band| {
        for (row, out_row) in band.chunks_exact_mut(taps.len()).enumerate() {
            let row = &pixels[(first_row + row) * columns..][..columns];
            for (out, tap) in out_row.iter_mut().zip(taps) {
                *out = std::array::from_fn(|channel| {
                    #[expect(clippy::cast_possible_truncation, reason = "the sum of f32 samples is stored as f32")]
                    let sample = tap
                        .weights
                        .iter()
                        .enumerate()
                        .map(|(offset, weight)| f64::from(row[tap.first + offset][channel]) * weight)
                        .sum::<f64>() as f32;
                    sample
                });
            }
        }
    });
    out
}

/// `pixels` (rows of `columns` pixels) with the rows resampled to the rows `taps` make.
fn down<const C: usize>(pixels: &[[f32; C]], columns: usize, taps: &[Tap]) -> Vec<[f32; C]> {
    let mut out = vec![[0.0; C]; taps.len() * columns];
    par::rows(&mut out, columns, |first_row, band| {
        let mut sums = vec![[0.0_f64; C]; columns];
        for (tap, out_row) in taps[first_row..].iter().zip(band.chunks_exact_mut(columns)) {
            sums.fill([0.0; C]);
            for (offset, weight) in tap.weights.iter().enumerate() {
                let row = &pixels[(tap.first + offset) * columns..][..columns];
                for (sum, pixel) in sums.iter_mut().zip(row) {
                    for (sum, &sample) in sum.iter_mut().zip(pixel) {
                        *sum += f64::from(sample) * weight;
                    }
                }
            }
            #[expect(clippy::cast_possible_truncation, reason = "the sum of f32 samples is stored as f32")]
            for (out, sum) in out_row.iter_mut().zip(&sums) {
                *out = sum.map(|sample| sample as f32);
            }
        }
    });
    out
}

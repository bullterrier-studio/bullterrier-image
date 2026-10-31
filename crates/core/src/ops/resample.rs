//! Changing the size of a picture or a map without leaving float precision.
//!
//! The same resampling as Pillow's `Image.resize` on float data, so the picture a recipe is rendered at is the one
//! Python made: a separable kernel (rows, then columns) whose support is widened by the shrink, so that every input
//! pixel under an output pixel counts (no aliasing), with each output pixel's weights summing to 1 (a constant stays
//! that constant). A shrink uses the Lanczos kernel, a growth the bicubic one.

use std::f64::consts::PI;

use crate::Error;

/// The most channels a resized thing has (the photo's colours; a map has one): the size of the largest result is
/// checked against it.
const MOST_CHANNELS: usize = 3; // constant: the photo's red, green and blue

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

#[expect(clippy::cast_precision_loss, reason = "a length is far below 2^53")]
const fn float(length: usize) -> f64 {
    length as f64
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
    /// [`Error::Recipe`] if the new size has more pixels than a count can hold.
    pub(crate) fn to_longest_side(height: usize, width: usize, longest: u32) -> Result<Option<Self>, Error> {
        let scale = f64::from(longest) / float(height.max(width));
        let scaled = |side: usize| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a rounded positive length; one too long for a count is refused below"
            )]
            let length = (float(side) * scale).round_ties_even() as usize;
            length.max(1)
        };
        let to = (scaled(height), scaled(width));
        if to == (height, width) {
            return Ok(None);
        }
        if to
            .0
            .checked_mul(to.1)
            .and_then(|pixels| pixels.checked_mul(MOST_CHANNELS))
            .is_none()
        {
            return Err(Error::Recipe(format!(
                "`size` {longest} makes a picture of {} x {} pixels, more than can be held",
                to.1, to.0
            )));
        }
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

    /// `values`, the samples of the thing the change was planned for (`channels` of them for each pixel, pixel after
    /// pixel, row after row), resampled: the same layout at the new size, every sample within 0..1 (a sharp kernel
    /// overshoots near an edge, which a picture cannot show).
    pub(crate) fn apply(&self, values: &[f32], channels: usize) -> Vec<f32> {
        let ((rows, columns), (new_rows, new_columns)) = (self.from, self.to);
        let across = if columns == new_columns {
            None
        } else {
            Some(across(values, rows, channels, &taps(self.filter, columns, new_columns)))
        };
        let wide = across.as_deref().unwrap_or(values);
        let mut out = if rows == new_rows {
            wide.to_vec()
        } else {
            down(wide, new_columns * channels, &taps(self.filter, rows, new_rows))
        };
        for sample in &mut out {
            *sample = sample.clamp(0.0, 1.0);
        }
        out
    }
}

/// `values` (`rows` rows of pixels of `channels` samples) with every row resampled to the columns `taps` make. The
/// sums are in f64 and rounded once to f32, as Pillow does.
fn across(values: &[f32], rows: usize, channels: usize, taps: &[Tap]) -> Vec<f32> {
    let line = values.len() / rows;
    let mut out = Vec::with_capacity(rows * taps.len() * channels);
    for row in values.chunks_exact(line) {
        for tap in taps {
            for channel in 0..channels {
                #[expect(clippy::cast_possible_truncation, reason = "the sum of f32 samples is stored as f32")]
                let sample = tap
                    .weights
                    .iter()
                    .enumerate()
                    .map(|(offset, weight)| f64::from(row[(tap.first + offset) * channels + channel]) * weight)
                    .sum::<f64>() as f32;
                out.push(sample);
            }
        }
    }
    out
}

/// `values` (rows of `line` samples) with the rows resampled to the rows `taps` make.
fn down(values: &[f32], line: usize, taps: &[Tap]) -> Vec<f32> {
    let mut out = Vec::with_capacity(taps.len() * line);
    let mut sums = vec![0.0_f64; line];
    for tap in taps {
        sums.fill(0.0);
        for (offset, weight) in tap.weights.iter().enumerate() {
            let row = &values[(tap.first + offset) * line..][..line];
            for (sum, &sample) in sums.iter_mut().zip(row) {
                *sum += f64::from(sample) * weight;
            }
        }
        #[expect(clippy::cast_possible_truncation, reason = "the sum of f32 samples is stored as f32")]
        out.extend(sums.iter().map(|&sum| sum as f32));
    }
    out
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::Resize;
    use crate::Error;

    /// The plan that takes a `height` x `width` thing to the longest side `longest`.
    fn plan(height: usize, width: usize, longest: u32) -> Result<Resize, Error> {
        Resize::to_longest_side(height, width, longest)?
            .ok_or_else(|| Error::Recipe(format!("{height} x {width} is already {longest} on its longest side")))
    }

    /// `actual` is `expected` to within `tolerance`.
    fn assert_close(actual: &[f32], expected: &[f64], tolerance: f64) {
        assert_eq!(actual.len(), expected.len());
        for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!((f64::from(*a) - e).abs() <= tolerance, "sample {index}: {a} is not {e}");
        }
    }

    #[test]
    fn two_pixels_shrunk_to_one_are_their_mean_in_every_channel() -> Result<(), Error> {
        // the kernel is symmetric about the output pixel's centre, which lies between the two input pixels
        let resize = plan(1, 2, 1)?;
        assert_eq!((resize.height(), resize.width()), (1, 1));
        assert_close(
            &resize.apply(&[0.2, 0.9, 0.0, 0.6, 0.1, 1.0], 3),
            &[0.4, 0.5, 0.5],
            1e-7,
        );
        Ok(())
    }

    #[test]
    fn a_growth_from_two_pixels_to_four_follows_the_cubic_kernel_worked_by_hand() -> Result<(), Error> {
        // 2 -> 4 pixels: output pixel `x` is centred at (x + 0.5) / 2 in input pixels, so the input pixels in reach lie
        // at distances 0.25 and 1.25 from out 0, 0.25 and 0.75 from out 1, and so on mirrored. Keys' kernel (a = -0.5)
        // weighs a distance of 0.25 by 111/128, 0.75 by 29/128 and 1.25 by -9/128. Scaled to sum to 1 the weights are
        // 37/34 and -3/34 for out 0, 111/140 and 29/140 for out 1; out 2 and out 3 mirror them.
        let resize = plan(1, 2, 4)?;
        assert_eq!((resize.height(), resize.width()), (2, 4));
        let out = resize.apply(&[0.2, 0.6], 1);
        let row = [
            (37.0 * 0.2 - 3.0 * 0.6) / 34.0,
            (111.0 * 0.2 + 29.0 * 0.6) / 140.0,
            (29.0 * 0.2 + 111.0 * 0.6) / 140.0,
            (-3.0 * 0.2 + 37.0 * 0.6) / 34.0,
        ];
        // the single input row makes both output rows: one tap of weight 1
        let expected: Vec<f64> = row.iter().chain(&row).copied().collect();
        assert_close(&out, &expected, 1e-7);
        Ok(())
    }

    /// A ramp over `width` pixels (one row, one channel): `0.05 + 0.03 x` at pixel `x`.
    fn ramp(width: usize) -> Vec<f32> {
        (0..width)
            .map(|x| 0.05 + 0.03 * f32::from(u8::try_from(x).unwrap_or(0)))
            .collect()
    }

    #[test]
    fn a_ramp_grown_stays_the_same_ramp_wherever_the_kernel_is_whole() -> Result<(), Error> {
        // the output pixel `x` of a growth from 10 to 25 pixels looks at the input at (x + 0.5) 0.4 - 0.5, in pixels;
        // the cubic kernel is exact for a straight line, so the line's value there is the output. Within 2 pixels of
        // either end the kernel is cut off and renormalised, which is allowed to bend the line.
        let resize = plan(1, 10, 25)?;
        let out = resize.apply(&ramp(10), 1);
        assert_eq!((resize.height(), resize.width()), (2, 25));
        for x in 5..20_u8 {
            let at = (f64::from(x) + 0.5) * 0.4 - 0.5;
            let expected = 0.05 + 0.03 * at;
            for row in 0..2 {
                assert!(
                    (f64::from(out[row * 25 + usize::from(x)]) - expected).abs() < 2e-6,
                    "x {x}: {} is not {expected}",
                    out[row * 25 + usize::from(x)]
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_ramp_shrunk_stays_the_same_ramp_wherever_the_kernel_is_whole() -> Result<(), Error> {
        // 30 to 8 pixels: output pixel `x` looks at the input at (x + 0.5) 3.75 - 0.5. The sharp kernel sampled at whole
        // pixels is not exactly symmetric about that point, so the line is met to 1e-5 (3.5e-6 measured), not exactly.
        let resize = plan(1, 30, 8)?;
        let out = resize.apply(&ramp(30), 1);
        assert_eq!((resize.height(), resize.width()), (1, 8));
        for x in 3..5_u8 {
            let at = (f64::from(x) + 0.5) * 3.75 - 0.5;
            let expected = 0.05 + 0.03 * at;
            assert!((f64::from(out[usize::from(x)]) - expected).abs() < 1e-5, "x {x}");
        }
        Ok(())
    }

    /// `shorter` scaled by `longest / longer` and rounded to the nearest whole number, halves to the even one, by integer
    /// arithmetic: the rounding of Python's `round` without a float in it.
    fn proportional(shorter: usize, longest: usize, longer: usize) -> usize {
        let (whole, rest) = (shorter * longest / longer, shorter * longest % longer);
        let up = match (2 * rest).cmp(&longer) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Equal => whole % 2 == 1,
            std::cmp::Ordering::Less => false,
        };
        (whole + usize::from(up)).max(1)
    }

    #[test]
    fn the_other_side_is_rounded_to_the_nearest_pixel_halves_to_the_even_one_and_is_at_least_one() -> Result<(), Error>
    {
        // (height, width, longest side, the new height and width): 3 x 4 at 2 is 1.5 x 2, a half, to the even 2; 5 x 4
        // at 2 is 2 x 1.6 -> 2; 3 x 8 at 4 is 1.5 x 4 -> 2; 5 x 8 at 4 is 2.5 -> 2 (even); 7 x 8 at 4 is 3.5 -> 4; a
        // very long picture keeps a pixel: 1 x 100 at 10 is 0.1 -> 1
        for (height, width, longest, expected) in [
            (3, 4, 2, (2, 2)),
            (5, 4, 2, (2, 2)),
            (3, 8, 4, (2, 4)),
            (5, 8, 4, (2, 4)),
            (7, 8, 4, (4, 4)),
            (1, 100, 10, (1, 10)),
            (100, 1, 10, (10, 1)),
        ] {
            let resize = plan(height, width, longest)?;
            assert_eq!(
                (resize.height(), resize.width()),
                expected,
                "{height} x {width} at {longest}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_thing_that_already_has_its_longest_side_is_not_resized_and_a_size_too_large_to_hold_is_refused() {
        assert_eq!(Resize::to_longest_side(4, 6, 6).ok(), Some(None));
        assert_eq!(Resize::to_longest_side(6, 4, 6).ok(), Some(None));
        assert_eq!(Resize::to_longest_side(1, 1, 1).ok(), Some(None));
        assert!(Resize::to_longest_side(4, 6, 7).is_ok_and(|plan| plan.is_some()));
        let refused = Resize::to_longest_side(1, 1, u32::MAX);
        assert!(
            matches!(&refused, Err(Error::Recipe(message)) if message.contains("`size`")),
            "{refused:?}"
        );
    }

    proptest! {
        #[test]
        fn a_constant_stays_that_constant_at_any_size_in_any_channels(
            height in 1_usize..24, width in 1_usize..24, longest in 1_u32..60, three in any::<bool>(),
            level in 0.0_f32..=1.0,
        ) {
            let channels = if three { 3 } else { 1 };
            let Some(resize) = Resize::to_longest_side(height, width, longest)? else { return Ok(()) };
            let out = resize.apply(&vec![level; height * width * channels], channels);
            prop_assert_eq!(out.len(), resize.height() * resize.width() * channels);
            prop_assert!(out.iter().all(|sample| (sample - level).abs() < 1e-6), "{:?}", out);
        }

        #[test]
        fn the_result_stays_within_0_and_1_though_the_kernel_overshoots_at_an_edge(
            height in 1_usize..12, width in 1_usize..12, longest in 1_u32..30,
            seed in prop::collection::vec(any::<bool>(), 144),
        ) {
            let Some(resize) = Resize::to_longest_side(height, width, longest)? else { return Ok(()) };
            // hard edges: every sample 0 or 1
            let values: Vec<f32> = seed.iter().cycle().take(height * width).map(|&on| f32::from(u8::from(on))).collect();
            let out = resize.apply(&values, 1);
            prop_assert!(out.iter().all(|sample| (0.0..=1.0).contains(sample)), "{:?}", out);
        }

        #[test]
        fn the_longest_side_is_exact_and_the_other_is_the_aspect_rounded(
            height in 1_usize..400, width in 1_usize..400, longest in 1_u32..800,
        ) {
            let longest_side = longest as usize;
            let (long, short) = (height.max(width), height.min(width));
            let expected_short = proportional(short, longest_side, long);
            let expected = if height >= width { (longest_side, expected_short) } else { (expected_short, longest_side) };
            let made = Resize::to_longest_side(height, width, longest)?.map_or((height, width), |resize| (resize.height(), resize.width()));
            prop_assert_eq!(made, expected);
        }
    }
}

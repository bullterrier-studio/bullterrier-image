//! Edge-aware smoothing by the guided filter (He, Sun and Tang, "Guided Image Filtering", 2010), at a cost that does
//! not grow with the radius.

use super::blur::transposed;
use super::length::float;
use super::par;

/// The `height` x `width` map `source` (row after row) smoothed so that it keeps the edges of `guide`, a map of the same
/// shape: where the guide is flat the source is averaged over the window, where the guide has an edge higher than
/// sqrt(`epsilon`) the source keeps its own side of it.
///
/// The filter fits, in every window, the line source ~ a x guide + b with a = cov(guide, source) / (var(guide) +
/// `epsilon`) and b = mean(source) - a x mean(guide) (He, Sun and Tang, section 3), and each pixel takes the average of
/// the lines of all the windows that hold it, evaluated at its own guide value. A window is the square of 2 x `radius` +
/// 1 pixels around a pixel, cut off at the map's edge. So the result is
/// - the source itself for `radius` 0, and a constant source, whatever the guide, stays that constant;
/// - a plain average of the source over the window, applied twice, for `epsilon` far above the guide's variance;
/// - the source again where it is an affine function of the guide and `epsilon` is far below the guide's variance.
///
/// It is not clipped. With the source as its own guide, a is between 0 and 1, so every output is an average of source
/// values and never leaves their range (up to rounding); with another guide it can leave the range of the source a
/// little near strong edges. `epsilon` is in the units of the guide squared and above 0 (0 divides by the variance of flat
/// windows). The work is proportional to the number of pixels, not to the window: the means are made by cumulative sums,
/// in f64. Memory is five maps of f64.
pub(crate) fn guided_filter(
    guide: &[f32],
    source: &[f32],
    height: usize,
    width: usize,
    radius: usize,
    epsilon: f64,
) -> Vec<f32> {
    let mean = |values: Vec<f64>| box_mean(values, height, width, radius);
    let wide = |values: &[f32]| values.iter().map(|&value| f64::from(value)).collect::<Vec<f64>>();
    let products = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(&a, &b)| f64::from(a) * f64::from(b)).collect();
    let (mean_guide, mut offset) = (mean(wide(guide)), mean(wide(source)));
    let (mut slope, variance) = (mean(products(guide, source)), mean(products(guide, guide)));
    // slope is the mean of guide x source, then the covariance, then a; offset the mean of the source, then b
    for (((slope, offset), &mean_guide), variance) in slope.iter_mut().zip(&mut offset).zip(&mean_guide).zip(variance) {
        *slope = (*slope - mean_guide * *offset) / (variance - mean_guide * mean_guide + epsilon);
        *offset -= *slope * mean_guide;
    }
    let (slope, offset) = (mean(slope), mean(offset));
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the result is made in f64 and stored in an f32 map"
    )]
    guide
        .iter()
        .zip(slope.iter().zip(&offset))
        .map(|(&guide, (slope, offset))| (slope * f64::from(guide) + offset) as f32)
        .collect()
}

/// The mean of `values` (a `height` x `width` map, row after row) over the (2 x `radius` + 1) square around each place,
/// cut off at the map's edge.
fn box_mean(mut values: Vec<f64>, height: usize, width: usize, radius: usize) -> Vec<f64> {
    mean_along_rows(&mut values, width, radius);
    let mut down = transposed(&values, height, width); // `width` rows of `height`
    drop(values);
    mean_along_rows(&mut down, height, radius);
    transposed(&down, width, height)
}

/// Each line of `len` values in `lines` replaced by the mean of the 2 x `radius` + 1 values around each of its own,
/// cut off at the line's ends.
fn mean_along_rows(lines: &mut [f64], len: usize, radius: usize) {
    par::rows(lines, len, |_, band| {
        // before[k] is the sum of the first k values of a line
        let mut before = Vec::with_capacity(len + 1);
        for line in band.chunks_exact_mut(len) {
            before.clear();
            before.push(0.0);
            before.extend(line.iter().scan(0.0, |running, &value| {
                *running += value;
                Some(*running)
            }));
            for (at, value) in line.iter_mut().enumerate() {
                let (first, last) = (at.saturating_sub(radius), at.saturating_add(radius).min(len - 1));
                *value = (before[last + 1] - before[first]) / float(last + 1 - first);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::guided_filter;

    /// The mean of `values` over the (2 x `radius` + 1) square around each place, cut off at the picture's edge, found by
    /// adding up every value in the window.
    fn window_mean(values: &[f64], height: usize, width: usize, radius: usize) -> Vec<f64> {
        let mut out = Vec::new();
        for row in 0..height {
            for column in 0..width {
                let (mut sum, mut count) = (0.0, 0.0);
                for y in row.saturating_sub(radius)..=(row + radius).min(height - 1) {
                    for x in column.saturating_sub(radius)..=(column + radius).min(width - 1) {
                        sum += values[y * width + x];
                        count += 1.0;
                    }
                }
                out.push(sum / count);
            }
        }
        out
    }

    /// He, Sun and Tang's algorithm 1 for a grey guide, in f64, window by window.
    fn reference(guide: &[f32], source: &[f32], height: usize, width: usize, radius: usize, epsilon: f64) -> Vec<f64> {
        let wide = |values: &[f32]| values.iter().map(|&value| f64::from(value)).collect::<Vec<_>>();
        let (g, s) = (wide(guide), wide(source));
        let mean = |values: &[f64]| window_mean(values, height, width, radius);
        let product = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(a, b)| a * b).collect::<Vec<_>>();
        let (mean_g, mean_s) = (mean(&g), mean(&s));
        let (mean_both, mean_square) = (mean(&product(&g, &s)), mean(&product(&g, &g)));
        let slope: Vec<f64> = (0..g.len())
            .map(|at| (mean_both[at] - mean_g[at] * mean_s[at]) / (mean_square[at] - mean_g[at] * mean_g[at] + epsilon))
            .collect();
        let offset: Vec<f64> = (0..g.len()).map(|at| mean_s[at] - slope[at] * mean_g[at]).collect();
        let (mean_slope, mean_offset) = (mean(&slope), mean(&offset));
        (0..g.len())
            .map(|at| mean_slope[at] * g[at] + mean_offset[at])
            .collect()
    }

    /// A different value at nearly every place, from `low` to `high`.
    fn ramp_noise(height: usize, width: usize, low: f32, high: f32, seed: usize) -> Vec<f32> {
        (0..height * width)
            .map(|index| {
                let unit = f32::from(u16::try_from((index * 7919 + index / 7 + seed * 131) % 997).unwrap_or(0)) / 997.0;
                low + (high - low) * unit
            })
            .collect()
    }

    fn farthest(a: &[f32], b: &[f64]) -> f64 {
        assert_eq!(a.len(), b.len());
        a.iter()
            .zip(b)
            .map(|(a, b)| (f64::from(*a) - b).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn the_result_is_the_guided_filter_by_its_definition() {
        let shapes = [(1, 1), (1, 9), (9, 1), (6, 23), (23, 6), (9, 11), (7, 9)];
        for (height, width) in shapes {
            // the guide and the source differ; the second pair is a log-brightness scale, 14 stops of it
            for (guide, source) in [
                (
                    ramp_noise(height, width, 0.0, 1.0, 1),
                    ramp_noise(height, width, 0.2, 0.9, 2),
                ),
                (
                    ramp_noise(height, width, -14.0, 0.0, 3),
                    ramp_noise(height, width, -14.0, 0.0, 3),
                ),
            ] {
                // radius 0 and 1, a window that fits and one far larger than the picture; epsilon tiny, small and huge
                for radius in [0, 1, 3, 40] {
                    for epsilon in [1e-6, 0.01, 100.0] {
                        let found = guided_filter(&guide, &source, height, width, radius, epsilon);
                        let wanted = reference(&guide, &source, height, width, radius, epsilon);
                        let far = farthest(&found, &wanted);
                        assert!(
                            far < 1e-5,
                            "{height} x {width}, radius {radius}, epsilon {epsilon}: {far}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_comparison_tells_a_filter_that_differs_apart() {
        // the known-bad pair: another epsilon and another radius are not the same filter
        let (guide, source) = (ramp_noise(8, 9, 0.0, 1.0, 1), ramp_noise(8, 9, 0.2, 0.9, 2));
        let found = guided_filter(&guide, &source, 8, 9, 2, 0.01);
        assert!(farthest(&found, &reference(&guide, &source, 8, 9, 2, 0.01)) < 1e-5);
        assert!(farthest(&found, &reference(&guide, &source, 8, 9, 2, 0.1)) > 1e-3);
        assert!(farthest(&found, &reference(&guide, &source, 8, 9, 3, 0.01)) > 1e-3);
    }

    #[test]
    fn radius_0_gives_the_source_itself_and_a_constant_source_stays_whatever_the_guide() {
        let guide = ramp_noise(7, 8, 0.0, 1.0, 4);
        let source = ramp_noise(7, 8, -3.0, 5.0, 5);
        let same = guided_filter(&guide, &source, 7, 8, 0, 0.02);
        assert!(same.iter().zip(&source).all(|(a, b)| a.to_bits() == b.to_bits()));
        for radius in [1, 4, 30] {
            let flat = guided_filter(&guide, &[0.7; 56], 7, 8, radius, 0.02);
            assert!(flat.iter().all(|&value| (value - 0.7).abs() < 1e-6), "radius {radius}");
        }
    }

    #[test]
    fn far_above_the_variance_epsilon_gives_the_source_averaged_twice_over_the_window() {
        let (guide, source) = (ramp_noise(9, 10, 0.0, 1.0, 6), ramp_noise(9, 10, 0.1, 0.8, 7));
        let found = guided_filter(&guide, &source, 9, 10, 2, 1e9);
        let once = window_mean(
            &source.iter().map(|&value| f64::from(value)).collect::<Vec<_>>(),
            9,
            10,
            2,
        );
        let twice = window_mean(&once, 9, 10, 2);
        assert!(farthest(&found, &twice) < 1e-5);
    }

    #[test]
    fn far_below_the_variance_epsilon_gives_a_source_that_is_an_affine_function_of_the_guide() {
        let guide = ramp_noise(9, 10, 0.0, 1.0, 8);
        let source: Vec<f32> = guide.iter().map(|&value| 2.0 * value + 0.3).collect();
        let found = guided_filter(&guide, &source, 9, 10, 2, 1e-9);
        let wanted: Vec<f64> = source.iter().map(|&value| f64::from(value)).collect();
        assert!(farthest(&found, &wanted) < 1e-4);
    }

    #[test]
    fn a_source_that_is_its_own_guide_never_leaves_its_range_and_a_small_epsilon_keeps_a_step() {
        let noise = ramp_noise(10, 12, -6.0, 2.0, 9);
        for epsilon in [1e-4, 0.5, 64.0] {
            let found = guided_filter(&noise, &noise, 10, 12, 3, epsilon);
            assert!(
                found.iter().all(|&value| (-6.0 - 1e-5..=2.0 + 1e-5).contains(&value)),
                "epsilon {epsilon}"
            );
        }
        // a step of 4 between two flat halves: sqrt(epsilon) = 0.01 is far below it and the step stays, 8 is above it
        // and it is blurred over the window
        let step: Vec<f32> = (0..8 * 16).map(|at| if at % 16 < 8 { 0.0 } else { 4.0 }).collect();
        let sharp = guided_filter(&step, &step, 8, 16, 3, 1e-4);
        let soft = guided_filter(&step, &step, 8, 16, 3, 64.0);
        assert!((sharp[7] - 0.0).abs() < 1e-3 && (sharp[8] - 4.0).abs() < 1e-3);
        assert!(soft[7] > 0.5 && soft[8] < 3.5);
    }
}

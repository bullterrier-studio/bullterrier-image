//! Gaussian blur of a one-channel map, equal to `scipy.ndimage.gaussian_filter` with the edge pixel repeated beyond the
//! picture (`mode='nearest'`) and a kernel cut off at four sigmas.

/// How far the Gaussian is cut off, in sigmas.
const REACH: f64 = 4.0; // constant: scipy's default `truncate`; the weights beyond it add up to less than 1e-4

/// How many pixels each way the Gaussian of `sigma` px reaches: `int(4 sigma + 0.5)` as in scipy, so a sigma under 0.125
/// px reaches none and leaves a map as it is.
#[expect(clippy::cast_possible_truncation, reason = "a small radius")]
fn radius_of(sigma: f64) -> i32 {
    (REACH * sigma + 0.5).floor() as i32
}

/// The weights of the Gaussian of `sigma` px, one for each offset from `-radius` to `radius`, normalised to add up to 1.
fn kernel(sigma: f64, radius: i32) -> Vec<f64> {
    let weights: Vec<f64> = (-radius..=radius)
        .map(|offset| (-0.5 * (f64::from(offset) / sigma).powi(2)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    weights.into_iter().map(|weight| weight / total).collect()
}

/// Blurs the `height` x `width` map `values` (row after row) by a Gaussian of `sigma` px, first along the rows, then down
/// the columns, each pass rounded to `f32` as scipy's is. A weighted mean of its neighbours, so a value never leaves the
/// range of the map's. `sigma` is finite and at least 0; 0 leaves the map as it is.
///
/// A pixel whose whole window holds one value takes that value without the sum (the weights add up to 1, so the sum
/// would give the same), so a map of long runs, as a piece's indicator is, costs by the length of its borders rather
/// than by its size.
pub(crate) fn gaussian(values: &mut [f32], height: usize, width: usize, sigma: f64) {
    let radius = radius_of(sigma);
    if radius == 0 {
        return;
    }
    let mut smoother = Smoother::new(kernel(sigma, radius), height.max(width));
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

/// One-dimensional smoothing of lines of a map, with the scratch space it needs kept between lines.
struct Smoother {
    weights: Vec<f64>,
    radius: usize,
    /// The line as it was before smoothing.
    before: Vec<f32>,
    /// For each position of the line, the last position of the run of equal values it is in.
    run_end: Vec<usize>,
}

impl Smoother {
    /// A smoother by `weights` (the window, `2 radius + 1` long) of lines up to `longest` long.
    fn new(weights: Vec<f64>, longest: usize) -> Self {
        Self {
            radius: weights.len() / 2,
            weights,
            before: Vec::with_capacity(longest),
            run_end: Vec::with_capacity(longest),
        }
    }

    /// `line` smoothed, the edge value repeated beyond its ends.
    #[expect(clippy::float_cmp, reason = "a run is made of values that are exactly equal")]
    fn smooth(&mut self, line: &mut [f32]) {
        self.before.clear();
        self.before.extend_from_slice(line);
        let last = line.len() - 1;
        self.run_end.clear();
        self.run_end.resize(line.len(), last);
        for position in (0..last).rev() {
            self.run_end[position] = if self.before[position] == self.before[position + 1] {
                self.run_end[position + 1]
            } else {
                position
            };
        }
        for (position, out) in line.iter_mut().enumerate() {
            let first = position.saturating_sub(self.radius);
            let end = (position + self.radius).min(last);
            *out = if self.run_end[first] >= end {
                // the whole window, edge repeats included, is one value
                self.before[first]
            } else if first + 2 * self.radius == end {
                // a window inside the line
                narrow(
                    self.weights
                        .iter()
                        .zip(&self.before[first..=end])
                        .map(|(weight, value)| weight * f64::from(*value))
                        .sum(),
                )
            } else {
                // a window over an end: the positions beyond it are the edge value
                narrow(
                    self.weights
                        .iter()
                        .enumerate()
                        .map(|(offset, weight)| {
                            let at = (position + offset).saturating_sub(self.radius).min(last);
                            weight * f64::from(self.before[at])
                        })
                        .sum(),
                )
            };
        }
    }
}

/// A sum of products of `f32` values and weights that add up to 1, as the `f32` it rounds to.
#[expect(clippy::cast_possible_truncation, reason = "narrowing is the point")]
const fn narrow(sum: f64) -> f32 {
    sum as f32
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::gaussian;

    /// The blur by brute force: every output the sum over `-radius..=radius` of the normalised `exp(-x^2 / 2 sigma^2)`
    /// times the value at the clamped position, along the rows and then down the columns, all in `f64`.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a brute-force oracle on small maps"
    )]
    fn reference(values: &[f32], height: usize, width: usize, sigma: f64) -> Vec<f64> {
        let radius = (4.0 * sigma + 0.5).floor() as i64;
        let weights: Vec<f64> = (-radius..=radius)
            .map(|offset| {
                if radius == 0 {
                    1.0
                } else {
                    (-0.5 * (offset as f64 / sigma).powi(2)).exp()
                }
            })
            .collect();
        let total: f64 = weights.iter().sum();
        let along = |read: &dyn Fn(usize, usize) -> f64, rows: usize, columns: usize| -> Vec<f64> {
            let mut out = Vec::new();
            for row in 0..rows {
                for column in 0..columns {
                    out.push(
                        (-radius..=radius)
                            .zip(&weights)
                            .map(|(offset, weight)| {
                                let source = (column as i64 + offset).clamp(0, columns as i64 - 1) as usize;
                                weight / total * read(row, source)
                            })
                            .sum(),
                    );
                }
            }
            out
        };
        let across = along(&|row, column| f64::from(values[row * width + column]), height, width);
        let flipped = along(&|column, row| across[row * width + column], width, height);
        // `flipped` is the columns as rows: back to row after row
        (0..height * width)
            .map(|index| flipped[(index % width) * height + index / width])
            .collect()
    }

    fn assert_matches(values: &[f32], height: usize, width: usize, sigma: f64) {
        let mut blurred = values.to_vec();
        gaussian(&mut blurred, height, width, sigma);
        let expected = reference(values, height, width, sigma);
        for (index, (got, want)) in blurred.iter().zip(&expected).enumerate() {
            assert!(
                (f64::from(*got) - want).abs() <= 1e-6,
                "{height} x {width} at sigma {sigma}: value {index} is {got}, not {want}"
            );
        }
    }

    proptest! {
        #[test]
        fn it_equals_the_brute_force_blur_of_any_small_map_at_any_sigma(
            height in 1_usize..9,
            width in 1_usize..9,
            sigma in prop::sample::select(vec![0.0, 0.1, 0.5, 1.9, 7.3]),
            seed in prop::collection::vec(0.0_f32..=1.0, 64),
        ) {
            assert_matches(&seed[..height * width], height, width, sigma);
        }
    }

    #[test]
    fn a_single_row_a_single_column_and_a_single_pixel_blur_as_a_line_does() {
        let line = [0.0, 1.0, 0.25, 0.75, 0.5, 0.0, 1.0];
        for sigma in [0.5, 1.9, 7.3] {
            assert_matches(&line, 1, 7, sigma);
            assert_matches(&line, 7, 1, sigma);
        }
        assert_matches(&[0.3], 1, 1, 2.0);
    }

    #[test]
    fn a_sigma_under_an_eighth_of_a_pixel_leaves_the_map_exactly_as_it_is() {
        let values = [0.1, 0.9, 0.3, 0.7, 0.5, 0.2];
        for sigma in [0.0, 0.05, 0.124] {
            let mut blurred = values;
            gaussian(&mut blurred, 2, 3, sigma);
            assert_eq!(blurred, values, "sigma {sigma}");
        }
        // 0.25 has a radius of one pixel: the map changes
        let mut blurred = values;
        gaussian(&mut blurred, 2, 3, 0.25);
        assert_ne!(blurred, values);
    }

    #[test]
    fn a_map_of_long_runs_equals_the_brute_force_blur_though_most_pixels_skip_the_sum() {
        // a 0/1 map of three blocks, 30 x 40, blurred by a kernel of radius 22: most windows are one run
        let (height, width) = (30, 40);
        let values: Vec<f32> = (0..height * width)
            .map(|index| {
                let (row, column) = (index / width, index % width);
                f32::from(u8::from(
                    (row < 12 && column < 18) || (row >= 20 && column >= 25) || column == 39,
                ))
            })
            .collect();
        assert_matches(&values, height, width, 5.0);
        assert_matches(&values, height, width, 1.0);
    }

    #[test]
    fn a_constant_map_stays_that_constant_exactly_and_an_impulse_keeps_its_total() {
        for sigma in [0.5, 3.0, 40.0] {
            let mut flat = vec![0.3_f32; 12 * 17];
            gaussian(&mut flat, 12, 17, sigma);
            assert!(
                flat.iter().all(|value| value.to_bits() == 0.3_f32.to_bits()),
                "sigma {sigma}"
            );
        }
        // an impulse in the middle of a map larger than the kernel: the weights add up to 1
        let mut impulse = vec![0.0_f32; 41 * 41];
        impulse[20 * 41 + 20] = 1.0;
        gaussian(&mut impulse, 41, 41, 2.0);
        let total: f32 = impulse.iter().sum();
        assert!((total - 1.0).abs() <= 1e-5, "{total}");
        assert!(impulse.iter().all(|value| *value >= 0.0));
    }

    #[test]
    fn the_edge_pixel_is_repeated_so_a_map_that_is_one_beyond_its_ends_stays_one_there() {
        // a row of ones with a zero in the middle: at the ends, with the edge repeated, the blur is still near 1, where
        // a zero edge would pull it to about a half
        let mut row = vec![1.0_f32; 31];
        row[15] = 0.0;
        gaussian(&mut row, 1, 31, 2.0);
        assert!(row[0] > 0.99 && row[30] > 0.99, "{} {}", row[0], row[30]);
    }
}

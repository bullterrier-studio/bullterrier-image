//! Gaussian blur of a one-channel map, approximated by three box blurs (Kovesi, "Fast Almost-Gaussian Filtering", 2010),
//! so that it costs the same at every radius: a few additions per pixel, however wide the blur.

use super::length::float;

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
    let passes = PASSES as f64;
    let ideal = (12.0 * sigma * sigma / passes + 1.0).sqrt();
    let mut narrow = ideal.floor() as usize;
    if narrow % 2 == 0 {
        narrow -= 1;
    }
    let (narrow_f, wide) = (narrow as f64, narrow + 2);
    // how many of the boxes are the narrow width so that the variances add up to 12 sigma^2 / 12... (Kovesi, eq. 5)
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
    let mut sum = f64::from(line[0]) * float(radius) + (0..=radius).map(|at| f64::from(line[at.min(last)])).sum::<f64>();
    for (position, out) in out.iter_mut().enumerate() {
        #[expect(clippy::cast_possible_truncation, reason = "narrowing is the point")]
        {
            *out = (sum / width) as f32;
        }
        sum += f64::from(line[(position + radius + 1).min(last)]) - f64::from(line[position.saturating_sub(radius)]);
    }
}

#[cfg(test)]
mod tests {
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

    /// The most any value of `gaussian` differs from the brute-force Gaussian over a map.
    fn worst_gap(values: &[f32], height: usize, width: usize, sigma: f64) -> f64 {
        let mut blurred = values.to_vec();
        gaussian(&mut blurred, height, width, sigma);
        let expected = reference(values, height, width, sigma);
        blurred
            .iter()
            .zip(&expected)
            .map(|(got, want)| (f64::from(*got) - want).abs())
            .fold(0.0, f64::max)
    }

    /// A 0/1 map of blocks, as a piece's indicator is: `height` x `width`, ones in two blocks and the last column.
    fn blocks(height: usize, width: usize) -> Vec<f32> {
        (0..height * width)
            .map(|index| {
                let (row, column) = (index / width, index % width);
                f32::from(u8::from(
                    (row < height * 2 / 5 && column < width * 9 / 20)
                        || (row >= height * 2 / 3 && column >= width * 5 / 8)
                        || column == width - 1,
                ))
            })
            .collect()
    }

    #[test]
    fn it_is_within_a_few_hundredths_of_the_brute_force_gaussian_on_blocks_at_any_width() {
        // the weights of an `each` step, at feathers from a few pixels to wider than the map. Three boxes are not a
        // Gaussian: measured gaps are 0.03 at most from sigma 1.5, and 0.11 at sigma 1, where whole-pixel boxes cannot
        // meet the variance
        let values = blocks(60, 80);
        for (sigma, most) in [
            (1.0, 0.12),
            (1.5, 0.035),
            (2.0, 0.035),
            (3.0, 0.035),
            (5.0, 0.035),
            (12.0, 0.035),
            (40.0, 0.035),
            (150.0, 0.035),
        ] {
            let gap = worst_gap(&values, 60, 80, sigma);
            assert!(gap <= most, "sigma {sigma}: {gap}");
        }
    }

    #[test]
    fn a_row_a_column_and_maps_narrower_than_the_blur_are_blurred_as_the_gaussian_is() {
        for (height, width) in [(1, 40), (40, 1), (3, 9), (9, 3), (1, 1)] {
            let values = blocks(height, width);
            let gap = worst_gap(&values, height, width, 3.0);
            assert!(gap <= 0.035, "{height} x {width}: {gap}");
        }
    }

    #[test]
    fn a_sigma_too_small_to_reach_a_neighbour_leaves_the_map_exactly_as_it_is() {
        let values = [0.1, 0.9, 0.3, 0.7, 0.5, 0.2];
        for sigma in [0.0, 0.05, 0.3] {
            let mut blurred = values;
            gaussian(&mut blurred, 2, 3, sigma);
            assert_eq!(blurred, values, "sigma {sigma}");
        }
        let mut blurred = values;
        gaussian(&mut blurred, 2, 3, 1.0);
        assert_ne!(blurred, values);
    }

    #[test]
    fn a_constant_map_stays_that_constant_and_an_impulse_keeps_its_total() {
        for sigma in [0.5, 3.0, 40.0] {
            let mut flat = vec![0.3_f32; 12 * 17];
            gaussian(&mut flat, 12, 17, sigma);
            assert!(flat.iter().all(|value| (value - 0.3).abs() <= 1e-6), "sigma {sigma}");
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

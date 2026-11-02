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

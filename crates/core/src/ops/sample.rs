//! Reading a picture at a point between its pixels: the one reader every effect that moves a picture's content uses.
//!
//! A [`Sampler`] borrows a picture, a way of reading between pixels ([`Interpolation`]) and a rule for what lies beyond
//! the edge ([`Extension`]), and answers [`Sampler::at`] for any point, inside the picture or not.

use super::edge::{Extension, Source};
use crate::params::Choice;

/// How a point between pixels is read from the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Interpolation {
    /// The closest pixel, a halfway point taking the next one: fastest, keeps every value exactly as it is, but steps and
    /// ghosts show in smooth areas.
    Nearest,
    /// The blend of the four pixels around the point: smooth, slightly soft.
    Bilinear,
    /// A cubic through the sixteen pixels around the point (Catmull-Rom): sharpest and exact on smooth gradients, a little
    /// slower, and may ring beside hard edges.
    Bicubic,
}

/// The most pixels a read uses along an axis.
const TAPS: usize = 4; // constant: a cubic is fitted through four pixels

/// A picture read at any point.
///
/// A pixel's centre is at whole numbers: row 0, column 0 is the middle of the first pixel. The pixels are three colour
/// channels each, read on their own.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sampler<'a> {
    pixels: &'a [[f32; 3]],
    height: usize,
    width: usize,
    interpolation: Interpolation,
    edge: Extension,
}

impl<'a> Sampler<'a> {
    /// A reader of `pixels`, `height` rows of `width` (both at least 1) row after row, that reads between them as
    /// `interpolation` says and beyond them as `edge` says.
    pub(crate) fn new(
        pixels: &'a [[f32; 3]],
        height: usize,
        width: usize,
        interpolation: Interpolation,
        edge: Extension,
    ) -> Self {
        Self {
            pixels,
            height,
            width,
            interpolation,
            edge,
        }
    }

    /// The number of rows of the picture.
    pub(crate) const fn height(&self) -> usize {
        self.height
    }

    /// The number of columns of the picture.
    pub(crate) const fn width(&self) -> usize {
        self.width
    }

    /// The picture at (`row`, `column`), a finite point that may lie anywhere. At a pixel's centre it is that pixel,
    /// exactly. `Nearest` and `Bilinear` give values between the smallest and largest read; `Bicubic` can overshoot them
    /// beside a hard edge (the cubic rings there) and reproduces any picture that is a polynomial of degree 2 or less
    /// in the row and the column.
    pub(crate) fn at(&self, row: f64, column: f64) -> [f64; 3] {
        let (first_row, row_weights) = self.interpolation.taps(row);
        let (first_column, column_weights) = self.interpolation.taps(column);
        let (row_weights, column_weights) = (row_weights.as_slice(), column_weights.as_slice());
        // the sources of the columns are the same for every row
        let mut across = [Source::Value(0.0); TAPS];
        for (source, index) in across.iter_mut().zip(first_column..).take(column_weights.len()) {
            *source = self.edge.locate(self.width, index);
        }
        let mut sum = [0.0; 3];
        for (&row_weight, row_index) in row_weights.iter().zip(first_row..) {
            let down = self.edge.locate(self.height, row_index);
            for (&column_weight, &across) in column_weights.iter().zip(&across) {
                let pixel = match (down, across) {
                    (Source::Sample(down), Source::Sample(across)) => {
                        self.pixels[down * self.width + across].map(f64::from)
                    }
                    (Source::Value(value), _) | (_, Source::Value(value)) => [f64::from(value); 3],
                };
                let weight = row_weight * column_weight;
                for (total, level) in sum.iter_mut().zip(pixel) {
                    *total += weight * level;
                }
            }
        }
        sum
    }
}

/// The weights of the pixels one axis of a read uses, in order: 1, 2 or 4 of them.
struct Taps {
    weights: [f64; TAPS],
    count: usize,
}

impl Taps {
    fn new(weights: &[f64]) -> Self {
        let mut all = [0.0; TAPS];
        all[..weights.len()].copy_from_slice(weights);
        Self {
            weights: all,
            count: weights.len(),
        }
    }

    fn as_slice(&self) -> &[f64] {
        &self.weights[..self.count]
    }
}

impl Interpolation {
    /// The index of the first pixel that a read at `position` along one axis uses, and the weights of it and the pixels
    /// after it (1, 2 or 4 of them, by the reading); they add up to 1.
    fn taps(self, position: f64) -> (isize, Taps) {
        #[expect(clippy::cast_possible_truncation, reason = "a position is far inside isize")]
        let below = position.floor() as isize;
        let t = position - position.floor();
        match self {
            // a halfway point takes the next pixel
            #[expect(clippy::cast_possible_truncation, reason = "as above")]
            Self::Nearest => ((position + 0.5).floor() as isize, Taps::new(&[1.0])),
            Self::Bilinear => (below, Taps::new(&[1.0 - t, t])),
            // constant: Catmull-Rom, the cubic convolution kernel with a = -1/2 (Keys 1981, eq. 4), in Horner form
            Self::Bicubic => (
                below - 1,
                Taps::new(&[
                    t * (t * (1.0 - 0.5 * t) - 0.5),
                    t * t * (1.5 * t - 2.5) + 1.0,
                    t * (t * (2.0 - 1.5 * t) + 0.5),
                    t * t * (0.5 * t - 0.5),
                ]),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The reader against values worked out by hand and polynomial pictures. It is private to the core, and the effects
    //! that use it show only sums of many readings.

    #![expect(clippy::cast_precision_loss, reason = "small picture sizes and test values")]

    use super::{Interpolation, Sampler};
    use crate::ops::edge::Extension;

    const READS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
    const RULES: [Extension; 5] = [
        Extension::Reflect,
        Extension::Mirror,
        Extension::Nearest,
        Extension::Wrap,
        Extension::Constant(0.25),
    ];

    /// A picture of `height` x `width` whose channels are `f(row, column, channel)`.
    fn picture(height: usize, width: usize, f: impl Fn(f64, f64, usize) -> f64) -> Vec<[f32; 3]> {
        #[expect(clippy::cast_possible_truncation, reason = "test values are small")]
        (0..height * width)
            .map(|at| [0, 1, 2].map(|channel| f((at / width) as f64, (at % width) as f64, channel) as f32))
            .collect()
    }

    /// A different value in every channel of every pixel, 0..1.
    fn varied(height: usize, width: usize) -> Vec<[f32; 3]> {
        picture(height, width, |row, column, channel| {
            ((row * 7.0 + column * 13.0 + channel as f64 * 29.0) * 0.618).fract()
        })
    }

    /// Positions spread over and well beyond a 5 x 7 picture, with fractions of every kind.
    fn points() -> Vec<(f64, f64)> {
        (0..400)
            .map(|k| {
                let k = f64::from(k);
                ((k * 0.37).rem_euclid(14.0) - 4.5, (k * 0.61).rem_euclid(17.0) - 5.3)
            })
            .collect()
    }

    #[test]
    fn a_pixels_centre_reads_the_pixel_itself_for_every_reading_and_rule() {
        for (height, width) in [(1, 1), (1, 5), (5, 1), (2, 3), (5, 7)] {
            let pixels = varied(height, width);
            for read in READS {
                for rule in RULES {
                    let sampler = Sampler::new(&pixels, height, width, read, rule);
                    for row in 0..height {
                        for column in 0..width {
                            let expected = pixels[row * width + column].map(f64::from);
                            assert_eq!(sampler.at(row as f64, column as f64), expected, "{read:?} {rule:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_constant_picture_reads_as_that_constant_anywhere() {
        let pixels = vec![[0.3, 0.6, 0.9]; 5 * 7];
        for read in READS {
            for rule in &RULES[..4] {
                let sampler = Sampler::new(&pixels, 5, 7, read, *rule);
                for (row, column) in points() {
                    for (found, expected) in sampler.at(row, column).into_iter().zip([0.3, 0.6, 0.9]) {
                        assert!((found - expected).abs() < 1e-6, "{read:?} {rule:?} at {row}, {column}");
                    }
                }
            }
        }
    }

    #[test]
    fn nearest_and_bilinear_stay_within_the_range_of_the_picture_anywhere() {
        let pixels = varied(5, 7);
        let (low, high) = pixels
            .iter()
            .flatten()
            .fold((1.0_f32, 0.0_f32), |(l, h), &v| (l.min(v), h.max(v)));
        for read in [Interpolation::Nearest, Interpolation::Bilinear] {
            for rule in &RULES[..4] {
                let sampler = Sampler::new(&pixels, 5, 7, read, *rule);
                for (row, column) in points() {
                    for found in sampler.at(row, column) {
                        assert!(
                            found >= f64::from(low) - 1e-9 && found <= f64::from(high) + 1e-9,
                            "{found}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bilinear_follows_a_linear_ramp_exactly_where_it_does_not_leave_the_picture() {
        // channel c holds 2 row + 3 column + c (small whole numbers, exact in f32)
        let pixels = picture(6, 8, |row, column, channel| 2.0 * row + 3.0 * column + channel as f64);
        let sampler = Sampler::new(&pixels, 6, 8, Interpolation::Bilinear, Extension::Nearest);
        for k in 0..60 {
            let (row, column) = (f64::from(k) * 0.0837 % 5.0, f64::from(k) * 0.1171 % 7.0);
            for (channel, found) in sampler.at(row, column).into_iter().enumerate() {
                assert!((found - (2.0 * row + 3.0 * column + channel as f64)).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn bicubic_reproduces_a_quadratic_where_bilinear_and_nearest_do_not() {
        // 0.01 (row^2 + 2 row column + column^2) + 0.1 row, read inside, three taps from every edge
        let truth = |row: f64, column: f64| 0.01 * (row + column).powi(2) + 0.1 * row;
        let pixels = picture(9, 9, |row, column, _| truth(row, column));
        let errors = READS.map(|read| {
            let sampler = Sampler::new(&pixels, 9, 9, read, Extension::Nearest);
            (0..40)
                .map(|k| {
                    let (row, column) = (2.0 + f64::from(k) * 0.1371 % 4.0, 2.0 + f64::from(k) * 0.0953 % 4.0);
                    (sampler.at(row, column)[0] - truth(row, column)).abs()
                })
                .fold(0.0, f64::max)
        });
        assert!(errors[2] < 1e-6, "bicubic {}", errors[2]);
        assert!(errors[1] > 1e-3, "bilinear {}", errors[1]);
        assert!(errors[0] > errors[1], "nearest {}", errors[0]);
    }

    #[test]
    fn bicubic_halfway_between_pixels_weighs_the_four_by_catmull_rom() {
        // the weights at a half are -1/16, 9/16, 9/16, -1/16 (Keys, a = -1/2): [0, 1, 1, 0] is read as 9/8, beyond the pixels
        let pixels = vec![[0.0; 3], [1.0; 3], [1.0; 3], [0.0; 3]];
        let sampler = Sampler::new(&pixels, 1, 4, Interpolation::Bicubic, Extension::Nearest);
        assert!((sampler.at(0.0, 1.5)[0] - 1.125).abs() < 1e-12);
        // a quarter of the way from the second pixel to the third: weights -9/128, 111/128, 29/128, -3/128
        let pixels = vec![[0.0; 3], [1.0; 3], [2.0; 3], [4.0; 3]];
        let sampler = Sampler::new(&pixels, 1, 4, Interpolation::Bicubic, Extension::Nearest);
        let expected = (111.0 + 2.0 * 29.0 - 3.0 * 4.0) / 128.0;
        assert!((sampler.at(0.0, 1.25)[0] - expected).abs() < 1e-12);
    }

    #[test]
    fn nearest_takes_the_next_pixel_at_a_halfway_point() {
        let pixels = varied(1, 4);
        let sampler = Sampler::new(&pixels, 1, 4, Interpolation::Nearest, Extension::Nearest);
        assert_eq!(sampler.at(0.0, 1.5), pixels[2].map(f64::from));
        assert_eq!(sampler.at(0.0, 1.49), pixels[1].map(f64::from));
        // a halfway point before the first pixel takes the first, not the one beyond it (found by wrapping)
        let wrapping = Sampler::new(&pixels, 1, 4, Interpolation::Nearest, Extension::Wrap);
        assert_eq!(wrapping.at(0.0, -0.5), pixels[0].map(f64::from));
    }

    #[test]
    fn beyond_the_edge_each_rule_reads_what_it_names() {
        // one row  a b c d = 0.1 0.2 0.4 0.8; the points -1 and 4 are one pixel beyond each end
        let pixels: Vec<[f32; 3]> = [0.1, 0.2, 0.4, 0.8].map(|value| [value; 3]).to_vec();
        let at = |rule, read, column: f64| Sampler::new(&pixels, 1, 4, read, rule).at(0.0, column)[0];
        let near = Interpolation::Nearest;
        // (rule, value at -1, value at 4)
        let table = [
            (Extension::Reflect, 0.1, 0.8),
            (Extension::Mirror, 0.2, 0.4),
            (Extension::Nearest, 0.1, 0.8),
            (Extension::Wrap, 0.8, 0.1),
            (Extension::Constant(0.25), 0.25, 0.25),
        ];
        for (rule, before, after) in table {
            assert!((at(rule, near, -1.0) - before).abs() < 1e-6, "{rule:?} before");
            assert!((at(rule, near, 4.0) - after).abs() < 1e-6, "{rule:?} after");
        }
        // and between pixels: half way between the last pixel and the one beyond it blends the two
        let bilinear = Interpolation::Bilinear;
        assert!((at(Extension::Wrap, bilinear, 3.5) - 0.45).abs() < 1e-6); // (d + a) / 2
        assert!((at(Extension::Mirror, bilinear, 3.5) - 0.6).abs() < 1e-6); // (d + c) / 2
        assert!((at(Extension::Constant(0.25), bilinear, 3.5) - 0.525).abs() < 1e-6); // (d + 0.25) / 2
        assert!((at(Extension::Reflect, bilinear, 3.5) - 0.8).abs() < 1e-6); // (d + d) / 2
    }

    #[test]
    fn rows_beyond_the_edge_follow_the_rule_too_and_a_constant_fills_the_corner() {
        // two rows: [0.1 0.2] over [0.5 0.9]
        let pixels = vec![[0.1; 3], [0.2; 3], [0.5; 3], [0.9; 3]];
        let nearest = Interpolation::Nearest;
        let at = |rule, row, column| Sampler::new(&pixels, 2, 2, nearest, rule).at(row, column)[0];
        assert!((at(Extension::Wrap, -1.0, 1.0) - 0.9).abs() < 1e-6);
        assert!((at(Extension::Reflect, 2.0, 0.0) - 0.5).abs() < 1e-6);
        assert!((at(Extension::Mirror, -1.0, 1.0) - 0.9).abs() < 1e-6);
        assert!((at(Extension::Constant(0.7), -1.0, 0.0) - 0.7).abs() < 1e-6);
        assert!((at(Extension::Constant(0.7), 0.0, 5.0) - 0.7).abs() < 1e-6);
        assert!((at(Extension::Constant(0.7), 1.0, 1.0) - 0.9).abs() < 1e-6);
    }
}

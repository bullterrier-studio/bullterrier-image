//! Smooth seeded noise laid on the frame of a picture, to bend the coordinates a torn border cuts.
//!
//! The noise is value noise on a hashed lattice: each lattice point has a value drawn from a hash of the seed and the
//! point's position (see [`mix`]), so no array of lattice values is ever made and nothing grows with the number of octaves
//! or with the lattice's fineness. The lattice is fixed in the frame: its spacing is a share of the picture's long side, so
//! the same picture at twice the size has the same noise, sampled at twice as many places.

use super::random::{mix, unit};
use super::{cast, length};

/// How many lattice points a position reads along an axis.
const POINTS: usize = 4; // constant: cubic convolution reads the two points on each side of a position

/// How the noise of [`bend`] is laid.
#[derive(Clone, Debug)]
pub(crate) struct Tear {
    /// The spacing of the coarsest lattice, as a share of the picture's long side.
    pub(crate) scale_share: f64,
    /// How many lattices are summed, each twice as fine as the one before.
    pub(crate) octaves: u32,
    /// The weight of each lattice against the one before it.
    pub(crate) persistence: f64,
    /// The seed of the lattice values.
    pub(crate) seed: u32,
}

/// The shifts of a `height` x `width` picture's pixel centres, along x (the first field) and along y (the second), row after
/// row: each field is smooth noise standardised over the picture to mean 0 and standard deviation 1, so the caller scales it
/// to the shift it wants. Octave `i` has lattice points `scale_share / 2^i` of the long side apart, with values drawn
/// evenly from -1 to 1 by `mix(seed, [axis, i, row, column])` (counting the points from the one just before the picture's
/// first pixel centre, 0) and read at the pixel centres by cubic interpolation through them (Catmull-Rom); the octaves,
/// weighted `persistence^i`, are summed. An octave whose spacing is under a pixel is left out, and so is every later one. A
/// picture without variation (one pixel) is all zeros.
///
/// The noise is a function of the position in the frame, so the same arguments at twice the size give the same noise at
/// twice as many places; it takes memory for the two fields and a few rows, however many octaves.
pub(crate) fn bend(height: usize, width: usize, tear: &Tear) -> [Vec<f32>; 2] {
    [0, 1].map(|axis| field(height, width, tear, axis))
}

/// The field of one `axis` of [`bend`].
fn field(height: usize, width: usize, tear: &Tear, axis: u64) -> Vec<f32> {
    let long_side = length::float(height.max(width));
    let mut sum = vec![0.0_f32; height * width];
    let mut weight = 1.0; // constant: the coarsest octave counts in full
    for octave in 0..tear.octaves {
        // constant: each octave is twice as fine as the one before (lacunarity 2, the standard)
        let spacing = tear.scale_share * long_side / 2.0_f64.powi(octave.cast_signed());
        if spacing < 1.0 {
            break;
        }
        add_octave(&mut sum, (height, width), spacing, weight, |row, column| {
            let drawn = unit(mix(tear.seed, &[axis, u64::from(octave), row as u64, column as u64]));
            2.0 * drawn - 1.0
        });
        weight *= tear.persistence;
    }
    standardised(sum)
}

/// `sum` shifted to mean 0 and scaled to standard deviation 1; all zeros if it has no variation.
fn standardised(mut sum: Vec<f32>) -> Vec<f32> {
    let count = length::float(sum.len());
    let mean = sum.iter().map(|&value| f64::from(value)).sum::<f64>() / count;
    let variance = sum.iter().map(|&value| (f64::from(value) - mean).powi(2)).sum::<f64>() / count;
    let deviation = variance.sqrt();
    for value in &mut sum {
        *value = if deviation > 0.0 {
            cast::single((f64::from(*value) - mean) / deviation)
        } else {
            0.0 // constant: nothing varies, so nothing is left once the mean is taken
        };
    }
    sum
}

/// The lattice point that comes first among the four that a position of `u` lattice spacings reads (`u` is positive), and
/// the weight of each of the four. The points are counted from the one before the picture, so the point before `u`'s own
/// cell, at -1, is 0.
fn cubic(u: f64) -> (usize, [f64; POINTS]) {
    let at = u.floor();
    let t = u - at;
    let (t2, t3) = (t * t, t * t * t);
    // constant: the Catmull-Rom cubic convolution weights of the points -1, 0, 1 and 2 (Keys, 1981, with a = -1/2)
    let weights = [
        0.5 * (-t3 + 2.0 * t2 - t),
        1.0 + t2 * (1.5 * t - 2.5),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ];
    (length::whole(at), weights)
}

/// Adds `weight` times one octave to `sum` (`height` x `width`): the lattice of `spacing` px, valued by `value(row,
/// column)`, read at the pixel centres. Separable: each lattice row is interpolated along x once, into a ring of the four
/// rows that a pixel row reads, and the pixel row is their interpolation along y.
fn add_octave(
    sum: &mut [f32],
    (height, width): (usize, usize),
    spacing: f64,
    weight: f64,
    value: impl Fn(usize, usize) -> f64,
) {
    let across: Vec<(usize, [f64; POINTS])> = (0..width)
        .map(|column| cubic((length::float(column) + 0.5) / spacing)) // constant: a pixel's centre is half a pixel in
        .collect();
    let first = across.first().map_or(0, |at| at.0);
    let last = across.last().map_or(0, |at| at.0 + POINTS - 1);
    let mut lattice = vec![0.0; last - first + 1];
    // the lattice rows that the pixel rows read: row `r` lives in slot `r mod 4`, and is read by four pixel rows at most
    let mut ring: [(usize, Vec<f64>); POINTS] = std::array::from_fn(|_| (usize::MAX, Vec::new()));
    for (row, line) in sum.chunks_exact_mut(width).enumerate().take(height) {
        let (start, down) = cubic((length::float(row) + 0.5) / spacing);
        for lattice_row in start..start + POINTS {
            let slot = &mut ring[lattice_row % POINTS];
            if slot.0 == lattice_row {
                continue;
            }
            for (at, entry) in lattice.iter_mut().enumerate() {
                *entry = value(lattice_row, first + at);
            }
            slot.1 = across
                .iter()
                .map(|(base, along)| (0..POINTS).map(|k| along[k] * lattice[base - first + k]).sum())
                .collect();
            slot.0 = lattice_row;
        }
        for (column, pixel) in line.iter_mut().enumerate() {
            let read: f64 = (0..POINTS)
                .map(|k| down[k] * ring[(start + k) % POINTS].1[column])
                .sum();
            *pixel += cast::single(weight * read);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{Tear, bend};
    use crate::ops::length;
    use crate::ops::random::{mix, unit};

    /// The mean and standard deviation of `values`, in double precision.
    fn mean_and_deviation(values: &[f32]) -> (f64, f64) {
        let count = length::float(values.len());
        let mean = values.iter().map(|&value| f64::from(value)).sum::<f64>() / count;
        let variance = values
            .iter()
            .map(|&value| (f64::from(value) - mean).powi(2))
            .sum::<f64>()
            / count;
        (mean, variance.sqrt())
    }

    fn tear(scale_share: f64, octaves: u32, persistence: f64, seed: u32) -> Tear {
        Tear {
            scale_share,
            octaves,
            persistence,
            seed,
        }
    }

    #[test]
    fn mix_gives_the_splitmix64_words_of_a_transcription_of_the_reference_code() {
        // https://prng.di.unimi.it/splitmix64.c transcribed in Python: one xor and one step for each word; the first is the
        // first published output of the generator started at 0
        let rows: [(u32, &[u64], u64); 4] = [
            (0, &[0], 0xe220_a839_7b1d_cdaf),
            (0, &[0, 0], 0xa706_dd2f_4d19_7e6f),
            (7, &[1, 2, 3], 0xc7d3_c611_58a5_394f),
            (1, &[0, 0, 5, u64::MAX], 0x957e_6d5b_6eba_ba03),
        ];
        for (seed, words, expected) in rows {
            assert_eq!(mix(seed, words), expected, "seed {seed}, words {words:?}");
        }
    }

    #[test]
    fn mix_gives_a_word_for_each_position_of_a_thousand_square_lattice_and_for_each_seed() {
        let words: HashSet<u64> = (0..1000_u64)
            .flat_map(|row| (0..1000_u64).map(move |column| mix(3, &[row, column])))
            .collect();
        assert_eq!(words.len(), 1_000_000);
        assert_ne!(mix(3, &[5, 9]), mix(4, &[5, 9]));
        assert_ne!(mix(3, &[5, 9]), mix(3, &[9, 5]));
    }

    #[test]
    fn unit_is_in_zero_to_below_one_and_takes_the_top_53_bits() {
        assert!((unit(0xe220_a839_7b1d_cdaf) - 0.883_310_808_213_642_6).abs() < 1e-15);
        assert!(unit(0).abs() < f64::EPSILON);
        assert!(unit(u64::MAX) < 1.0);
        assert!(unit(u64::MAX) > 0.999_999_999_999);
    }

    #[test]
    fn each_field_has_mean_zero_and_deviation_one_whatever_the_noise_was() {
        for (height, width, spec) in [
            (60, 80, tear(0.3, 4, 0.55, 1)),
            (30, 200, tear(0.05, 8, 0.9, 2)),
            (97, 61, tear(1.0, 1, 0.0, 3)),
        ] {
            for (axis, field) in bend(height, width, &spec).iter().enumerate() {
                assert_eq!(field.len(), height * width);
                let (mean, deviation) = mean_and_deviation(field);
                assert!(mean.abs() < 1e-4, "{height}x{width} axis {axis}: mean {mean}");
                assert!(
                    (deviation - 1.0).abs() < 1e-4,
                    "{height}x{width} axis {axis}: deviation {deviation}"
                );
            }
        }
    }

    #[test]
    fn the_noise_is_a_pure_function_of_its_arguments_and_the_seed_and_the_axis_each_change_it() {
        let first = bend(40, 50, &tear(0.2, 3, 0.5, 9));
        assert_eq!(
            first,
            bend(40, 50, &tear(0.2, 3, 0.5, 9)),
            "the same arguments, another field"
        );
        assert_ne!(
            first[0],
            bend(40, 50, &tear(0.2, 3, 0.5, 10))[0],
            "the seed was ignored"
        );
        assert_ne!(first[0], first[1], "the two axes share a field");
    }

    #[test]
    fn an_octave_finer_than_a_pixel_and_every_later_one_are_left_out() {
        // long side 40, coarsest spacing 0.1 of it = 4 px, then 2 px, 1 px (kept: the limit itself is a whole pixel), 0.5 px
        let (height, width) = (30, 40);
        let three = bend(height, width, &tear(0.1, 3, 0.6, 5));
        assert_eq!(
            bend(height, width, &tear(0.1, 8, 0.6, 5)),
            three,
            "octaves under a pixel were summed"
        );
        assert_ne!(
            bend(height, width, &tear(0.1, 2, 0.6, 5)),
            three,
            "the octave of exactly one pixel was left out"
        );
    }

    #[test]
    fn persistence_zero_leaves_only_the_coarsest_octave() {
        let coarse = bend(35, 45, &tear(0.4, 1, 0.0, 6));
        assert_eq!(bend(35, 45, &tear(0.4, 5, 0.0, 6)), coarse);
        assert_ne!(bend(35, 45, &tear(0.4, 5, 0.3, 6)), coarse);
    }

    #[test]
    fn a_field_passes_through_the_lattice_values_at_the_pixels_that_sit_on_lattice_points() {
        // one octave, spacing 2.5 px (0.025 of a long side of 100): pixel c has its centre c + 0.5 on lattice point k when
        // c + 0.5 = 2.5 k, so c = 2, 7, 12, ... The field is a straight line in the lattice value there (standardising
        // shifts and scales it alike), so (field - field') / (value - value') is one number for every pair of points.
        let (height, width, seed) = (100, 100, 4);
        let field = &bend(height, width, &tear(0.025, 1, 0.0, seed))[0];
        // the point k = 2 r + 1 sits at the pixel 5 r + 2, and is counted k + 1 from the point before the picture
        let value = |row: usize, column: usize| {
            2.0 * unit(mix(seed, &[0, 0, (2 * row + 2) as u64, (2 * column + 2) as u64])) - 1.0
        };
        let points: Vec<(usize, usize)> = (1..8).flat_map(|row| (1..8).map(move |column| (row, column))).collect();
        let at = |(row, column): (usize, usize)| f64::from(field[(row * 5 + 2) * width + column * 5 + 2]);
        let slope =
            (at(points[1]) - at(points[0])) / (value(points[1].0, points[1].1) - value(points[0].0, points[0].1));
        assert!(slope.abs() > 0.1, "{slope}");
        for pair in points.windows(2) {
            let ratio = (at(pair[1]) - at(pair[0])) / (value(pair[1].0, pair[1].1) - value(pair[0].0, pair[0].1));
            assert!(
                (ratio - slope).abs() < 1e-3 * slope.abs(),
                "{pair:?}: {ratio} against {slope}"
            );
        }
    }

    #[test]
    fn the_noise_between_lattice_points_is_smooth() {
        // spacing 50 px: neighbouring pixels of the standardised field differ by a small part of its deviation (white noise
        // would differ by about 1.4)
        let width = 200;
        let field = &bend(width, width, &tear(0.25, 1, 0.0, 8))[1];
        let steepest = field
            .chunks_exact(width)
            .flat_map(|line| line.windows(2).map(|pair| (pair[1] - pair[0]).abs()))
            .fold(0.0_f32, f32::max);
        assert!(steepest < 0.25, "{steepest}");
        assert!(steepest > 0.0);
    }

    #[test]
    fn the_same_noise_comes_out_at_twice_the_size() {
        // the frame is the same, so a pixel of the small picture is the mean of the 2 x 2 pixels of the large one that cover
        // the same place, to within the interpolation between them and the deviation each was standardised by
        let (small, large) = (
            bend(30, 40, &tear(0.5, 3, 0.5, 11)),
            bend(60, 80, &tear(0.5, 3, 0.5, 11)),
        );
        for axis in 0..2 {
            let worst = (0..30)
                .flat_map(|row| (0..40).map(move |column| (row, column)))
                .map(|(row, column)| {
                    let block: f32 = [(0, 0), (0, 1), (1, 0), (1, 1)]
                        .iter()
                        .map(|(down, across)| large[axis][(2 * row + down) * 80 + 2 * column + across])
                        .sum::<f32>()
                        / 4.0;
                    (small[axis][row * 40 + column] - block).abs()
                })
                .fold(0.0_f32, f32::max);
            assert!(worst < 0.05, "axis {axis}: {worst}");
        }
    }

    #[test]
    fn a_thin_picture_and_a_single_pixel_have_noise_too() {
        let thin = bend(3, 2000, &tear(0.002, 8, 0.55, 1));
        assert!(
            thin.iter()
                .all(|field| field.len() == 6000 && field.iter().all(|value| value.is_finite()))
        );
        assert!(mean_and_deviation(&thin[0]).1 > 0.99);
        // one pixel has no variation to scale: the field is zero
        assert_eq!(bend(1, 1, &tear(1.0, 4, 0.5, 2)), [vec![0.0], vec![0.0]]);
    }
}

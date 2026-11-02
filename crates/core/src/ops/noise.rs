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

//! Smooth seeded noise laid on the frame of a picture, to bend the coordinates a torn border cuts.
//!
//! The noise is value noise on a hashed lattice: each lattice point has a value drawn from a hash of the seed and the
//! point's position (see [`mix`]), so no array of lattice values is ever made and nothing grows with the number of octaves
//! or with the lattice's fineness. The lattice is fixed in the frame: its spacing is a share of the picture's long side, so
//! the same picture at twice the size has the same noise, sampled at twice as many places.

use super::blur::{gaussian_filter_plane, weights};
use super::edge::{Extension, Source};
use super::par;
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

/// How the noise of [`fractal`] is laid.
#[derive(Clone, Debug)]
pub(crate) struct Fractal {
    /// The spacing of the coarsest lattice, in px.
    pub(crate) spacing: f64,
    /// How many lattices are summed.
    pub(crate) octaves: u32,
    /// The weight of each lattice against the one before it.
    pub(crate) persistence: f64,
    /// How many times finer each lattice is than the one before (at least 1).
    pub(crate) lacunarity: f64,
    /// The seed of the lattice values.
    pub(crate) seed: u32,
    /// How far past the lattice's first point the first pixel lies, in px: 0 puts a lattice point on every `spacing`-th pixel
    /// from the first (a spacing of 1 then gives every pixel a value of its own), 0.5 puts the first lattice point half a
    /// pixel before the first pixel.
    pub(crate) origin: f64,
}

impl Fractal {
    /// A number no value of [`field_rows`] reaches, from the lattice alone: a pixel's value is a sum of lattice values read
    /// with Catmull-Rom weights, divided by its own deviation. The weights of an axis sum to at most 5/4 in size, so
    /// the sum of the octaves is at most `(5/4)^2 x (the octave weights)` (lattice values are within -1..1), and the
    /// deviation is at least `sqrt(sum of weight^2 / 3) x 41/64`, because the sum of the squared weights of an axis is
    /// never under 41/64 (its least, in the middle of a cell, is 2 x (1/16)^2 + 2 x (9/16)^2). Both are found by
    /// evaluating the cubic's weights over a cell; 0 if no octave is kept.
    pub(crate) fn bound(&self) -> f64 {
        // constant: the largest sum of the absolute Catmull-Rom weights, 1/16 + 9/16 + 9/16 + 1/16, and the least sum of their squares
        let (largest, least) = (1.25_f64, 41.0 / 64.0);
        let (mut weights, mut squares) = (0.0, 0.0);
        let mut weight = 1.0; // constant: the coarsest octave counts in full
        for octave in 0..self.octaves {
            if self.spacing / self.lacunarity.powi(octave.cast_signed()) < 1.0 {
                break;
            }
            weights += weight;
            squares += weight * weight;
            weight *= self.persistence;
        }
        if squares == 0.0 {
            return 0.0;
        }
        largest * largest * weights / ((squares / 3.0).sqrt() * least)
    }
}

/// The shifts of a `height` x `width` picture's pixels, along x (the first field) and along y (the second), row after row:
/// each field is smooth noise of deviation 1 at every pixel (see [`field_rows`]), so the caller
/// scales it to the shift it wants. Octave `i` has lattice points `spacing / lacunarity^i` apart, with values drawn evenly from -1 to 1
/// by `mix(seed, [axis, i, row, column])` (counting the points from the one just before the first pixel, 0) and
/// read at the pixels by cubic interpolation through them (Catmull-Rom), so a pixel on a lattice point has its value; the
/// octaves, weighted `persistence^i`, are summed. An octave whose spacing is under a pixel is left out, and so is every
/// later one. A picture without variation (one pixel) is all zeros.
///
/// It takes memory for the two fields and a few rows, however many octaves.
pub(crate) fn fractal(height: usize, width: usize, noise: &Fractal) -> [Vec<f32>; 2] {
    [0, 1].map(|axis| field_rows(0, height, width, noise, axis))
}

/// The shifts of a `height` x `width` picture, as [`fractal`] gives them, for the lattice of a [`Tear`], fixed in the frame.
pub(crate) fn bend(height: usize, width: usize, tear: &Tear) -> [Vec<f32>; 2] {
    let long_side = length::float(height.max(width));
    fractal(
        height,
        width,
        &Fractal {
            spacing: tear.scale_share * long_side,
            octaves: tear.octaves,
            persistence: tear.persistence,
            lacunarity: 2.0, // constant: each octave is twice as fine as the one before (the standard)
            seed: tear.seed,
            origin: 0.5, // constant: the noise lies on the frame, where a pixel's centre is half a pixel in
        },
    )
}

/// The `height` rows from canvas row `first_row` on, of a canvas `width` wide, of the field of the independent noise number
/// `stream` for the lattice of `noise` ([`fractal`] gives streams 0 and 1): another stream, another seed's worth of lattice
/// values. A single field costs half of [`fractal`].
///
/// The value at a place depends on
/// the place and the seed only, so rows cut from the canvas are the same rows of the whole field, to the bit.
///
/// Every value is divided by its own standard deviation, which is known without looking at any other pixel. A lattice value
/// is drawn evenly from -1 to 1, so it has mean 0 and variance 1/3 (the variance of a uniform on an interval of width 2
/// is 2^2 / 12); a pixel reads four lattice points a side with the Catmull-Rom weights `w` of its place in the cell, so
/// along an axis its octave sums `w^2` over the four points, and the octaves are independent: the variance of the sum is
/// the sum of `weight^2 / 3 * (sum of w^2 along x) * (sum of w^2 down y)`. A pixel is then of variance 1 wherever it lies,
/// whatever the picture around it.
pub(crate) fn field_rows(first_row: usize, height: usize, width: usize, noise: &Fractal, stream: u64) -> Vec<f32> {
    let (mut sum, variance) = lattice_sum(first_row, height, width, noise, stream);
    for (value, variance) in sum.iter_mut().zip(&variance) {
        *value = if *variance > 0.0 { *value / variance.sqrt() } else { 0.0 }; // constant: nothing varies, so nothing is left
    }
    sum
}

/// The octaves of the lattice of `noise` summed over `height` rows from canvas row `first_row`, and the variance of each
/// sum (see [`field_rows`]).
fn lattice_sum(first_row: usize, height: usize, width: usize, noise: &Fractal, stream: u64) -> (Vec<f32>, Vec<f32>) {
    let mut sum = vec![0.0_f32; height * width];
    let mut variance = vec![0.0_f32; height * width];
    let mut weight = 1.0; // constant: the coarsest octave counts in full
    for octave in 0..noise.octaves {
        let spacing = noise.spacing / noise.lacunarity.powi(octave.cast_signed());
        if spacing < 1.0 {
            break;
        }
        add_octave(
            (&mut sum, &mut variance),
            (first_row, height, width),
            (spacing, noise.origin),
            weight,
            |row, column| {
                let drawn = unit(mix(noise.seed, &[stream, u64::from(octave), row as u64, column as u64]));
                2.0 * drawn - 1.0
            },
        );
        weight *= noise.persistence;
    }
    (sum, variance)
}

/// One standard-normal value (mean 0, deviation 1) of the noise number `stream` at place `at`: a pure function of its
/// arguments, so a field of them does not depend on how its rows are cut into bands.
pub(crate) fn standard_normal(seed: u32, stream: u64, at: usize) -> f32 {
    let word = |part: u64| unit(mix(seed, &[stream, at as u64, part]));
    // Box-Muller (1958): two independent uniforms make a standard normal; 1 - u is in (0, 1], so its log is finite
    let radius = (-2.0 * (1.0 - word(0)).ln()).sqrt();
    cast::single(radius * (std::f64::consts::TAU * word(1)).cos())
}

/// The `rows` rows from canvas row `first_row` on, of a `height` x `width` canvas, of a field of mean 0 and deviation 1
/// whose value at canvas place `at` (row after row) is `draw(at)`, clumped into specks about `size` px wide: up to 1 px
/// every pixel is its own value; above 1 the values are blurred by a Gaussian of sigma `size / 2` (cut off at `truncate`
/// sigmas, `edge` beyond the canvas) and scaled back to deviation 1, so the strength does not depend on `size`. `draw`
/// gives values of mean 0 and deviation 1, independent of each other, from any thread.
///
/// The scale is known without looking at the picture: a blur of independent values of variance 1 by a kernel of weights `k`
/// has the variance of the sum of the squared weights on each draw. The Gaussian is separable, so a place's variance is that
/// of its column times that of its row ([`axis_variance`], which counts what `edge` does at the canvas's border, where one
/// draw is read several times), and each value is divided by its root. A value depends only on the canvas draws within the
/// kernel's reach, which are made for the rows beyond the band too (through `edge` past the canvas), so rows cut from the
/// canvas are the same rows of the whole field, to the bit while the kernel is summed tap by tap, and within float rounding
/// when the blur is a Fourier transform.
pub(crate) fn clumped(
    (height, width): (usize, usize),
    (first_row, rows): (usize, usize),
    size: f64,
    truncate: f64,
    edge: Extension,
    draw: impl Fn(usize) -> f32 + Sync,
) -> Vec<f32> {
    // a speck of 1 px or less is a single pixel: nothing to clump
    let kernel = (size > 1.0)
        .then(|| weights(size / 2.0, truncate))
        .filter(|half| half.len() > 1);
    let reach = kernel.as_ref().map_or(0, |half| half.len() - 1);
    let mut plane = vec![0.0; (rows + 2 * reach) * width];
    par::rows(&mut plane, width, |first, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let canvas_row = (first_row + first + offset).cast_signed() - reach.cast_signed();
            match edge.locate(height, canvas_row) {
                Source::Sample(source) => {
                    for (column, value) in line.iter_mut().enumerate() {
                        *value = draw(source * width + column);
                    }
                }
                Source::Value(constant) => line.fill(constant),
            }
        }
    });
    let Some(half) = kernel else { return plane };
    let blurred = gaussian_filter_plane(&plane, (rows + 2 * reach, width), size / 2.0, truncate, edge);
    // constant: a blur of independent unit values has variance the sum of the squared weights it puts on each draw, along
    // each axis in turn; past the canvas's edge one draw takes the weights of every place that `edge` maps to it
    let across: Vec<f64> = (0..width).map(|at| axis_variance(&half, width, edge, at)).collect();
    let down: Vec<f64> = (first_row..first_row + rows)
        .map(|at| axis_variance(&half, height, edge, at))
        .collect();
    blurred[reach * width..(reach + rows) * width]
        .chunks_exact(width)
        .zip(&down)
        .flat_map(|(line, down)| {
            line.iter()
                .zip(&across)
                .map(move |(&value, across)| cast::single(f64::from(value) / (down * across).sqrt()))
        })
        .collect()
}

/// The variance of a unit-variance independent line blurred by the Gaussian of the half-kernel `half` at place `at` of `len`
/// samples: the sum, over the draws the kernel reaches, of the squared total weight it puts on each (past the ends `edge`
/// sends several places to one draw, which then weighs their weights together).
fn axis_variance(half: &[f64], len: usize, edge: Extension, at: usize) -> f64 {
    let reach = half.len() - 1;
    let mut taps: Vec<(usize, f64)> = (0..=2 * reach)
        .filter_map(
            |tap| match edge.locate(len, (at + tap).cast_signed() - reach.cast_signed()) {
                Source::Sample(source) => Some((source, half[tap.abs_diff(reach)])),
                Source::Value(_) => None,
            },
        )
        .collect();
    taps.sort_by_key(|&(source, _)| source);
    taps.chunk_by(|left, right| left.0 == right.0)
        .map(|same| same.iter().map(|tap| tap.1).sum::<f64>().powi(2))
        .sum()
}

/// `sum` shifted to mean 0 and scaled to standard deviation 1; all zeros if it has no variation. This measures the whole
/// picture, so what it makes is not the same for a part of the picture: only for the kinds of noise that are not
/// [`field_rows`]s.
pub(crate) fn standardised(mut sum: Vec<f32>) -> Vec<f32> {
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

/// The rows `from..to` of a canvas of `canvas` rows that a sampler must hold to read a field at canvas rows between `least`
/// and `most`, each read moved by up to `slack` rows: one row before and two after (the taps of a cubic). If a read may leave
/// the canvas, the whole canvas: beyond it the sampler applies the canvas's edge rule, which a part of it cannot.
pub(crate) fn window(canvas: usize, (least, most): (f64, f64), slack: f64) -> (usize, usize) {
    let (top, bottom) = ((least - slack).floor() - 1.0, (most + slack).floor() + 2.0); // constant: a cubic's taps
    if top >= 0.0 && bottom < length::float(canvas) {
        (length::whole(top), length::whole(bottom) + 1)
    } else {
        (0, canvas)
    }
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

/// Adds `weight` times one octave to `sum` (`height` rows from canvas row `first_row`, `width` wide), and the variance of
/// what it adds to `variance`: the lattice of `spacing` px starting `origin` px before the first pixel's position of the
/// canvas, valued by `value(row, column)`, read at the pixels. Separable: each lattice row is interpolated along x once, into a ring of the four
/// rows that a pixel row reads, and the pixel row is their interpolation along y.
fn add_octave(
    (sum, variance): (&mut [f32], &mut [f32]),
    (first_row, height, width): (usize, usize, usize),
    (spacing, origin): (f64, f64),
    weight: f64,
    value: impl Fn(usize, usize) -> f64,
) {
    let across: Vec<(usize, [f64; POINTS])> = (0..width)
        .map(|column| cubic((length::float(column) + origin) / spacing))
        .collect();
    let first = across.first().map_or(0, |at| at.0);
    let last = across.last().map_or(0, |at| at.0 + POINTS - 1);
    let mut lattice = vec![0.0; last - first + 1];
    // constant: a lattice value has variance 1/3 (uniform on -1..1)
    let own = weight * weight / 3.0;
    // the lattice rows that the pixel rows read: row `r` lives in slot `r mod 4`, and is read by four pixel rows at most
    let mut ring: [(usize, Vec<f64>); POINTS] = std::array::from_fn(|_| (usize::MAX, Vec::new()));
    let lines = sum.chunks_exact_mut(width).zip(variance.chunks_exact_mut(width));
    for (row, (line, spread)) in lines.enumerate().take(height) {
        // the canvas row is a whole number before it is a position, so a band and the whole picture meet the same number
        let (start, down) = cubic((length::float(first_row + row) + origin) / spacing);
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
        let down_squares: f64 = down.iter().map(|w| w * w).sum();
        for (column, pixel) in line.iter_mut().enumerate() {
            let read: f64 = (0..POINTS)
                .map(|k| down[k] * ring[(start + k) % POINTS].1[column])
                .sum();
            *pixel += cast::single(weight * read);
            let along_squares: f64 = across[column].1.iter().map(|w| w * w).sum();
            spread[column] += cast::single(own * along_squares * down_squares);
        }
    }
}

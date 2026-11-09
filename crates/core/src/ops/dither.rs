//! Dithering: choosing a palette colour for every pixel so that, seen from afar, the pixels average to the picture.
//!
//! Error diffusion ([`diffuse`]) passes each pixel's error on to the pixels after it; ordered dithering ([`ordered`])
//! shifts each pixel by a repeating pattern ([`Tile`]) before choosing. Both return the index of the chosen palette colour
//! per pixel (a palette has at most 256 colours); the caller paints them.

use std::sync::OnceLock;

use crate::ops::cluster::{Point, nearest_one};
use crate::ops::length::whole;
use crate::ops::par;
use crate::ops::random::Rng;

/// Where an error-diffusion kernel sends a pixel's error: (columns right, rows down, weight) for the pixels it reaches,
/// and the weights' divisor. Every tap lies after the pixel in reading order.
pub(crate) struct Kernel {
    taps: &'static [(isize, usize, f32)],
    divisor: f32,
}

/// Floyd and Steinberg (1976): 7/16 right, 3/16 below left, 5/16 below, 1/16 below right.
pub(crate) const FLOYD_STEINBERG: Kernel = Kernel {
    taps: &[(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)], // constant: Floyd and Steinberg (1976)
    divisor: 16.0,                                                // constant: Floyd and Steinberg (1976)
};

/// Atkinson (Apple MacPaint): 1/8 to each of six pixels, so a quarter of the error is dropped.
pub(crate) const ATKINSON: Kernel = Kernel {
    taps: &[
        (1, 0, 1.0),
        (2, 0, 1.0),
        (-1, 1, 1.0),
        (0, 1, 1.0),
        (1, 1, 1.0),
        (0, 2, 1.0),
    ], // constant: Atkinson's published error weights
    divisor: 8.0, // constant: Atkinson's published error weights
};

/// The widest sideways reach of a kernel, in columns; the error rows are padded by it on both sides.
const REACH: usize = 2; // constant: Atkinson's taps reach two columns right
/// The rows an error is passed down to, and so how many error rows are alive at once, with the one being read.
const ROWS_ALIVE: usize = 3; // constant: Atkinson's taps reach two rows down

/// The palette colour chosen for each pixel of `values` (`height` x `width`, row after row) by error diffusion: indices
/// into `palette`.
///
/// Pixels are taken in reading order; each gets the palette colour closest to its value plus the error passed to it, and
/// `amount` (0 to 1) of the difference between the two is passed on by `kernel`. An error that falls off the picture is
/// lost.
pub(crate) fn diffuse(
    values: &[Point],
    (height, width): (usize, usize),
    palette: &[Point],
    kernel: &Kernel,
    amount: f32,
) -> Vec<u8> {
    let stride = width + 2 * REACH;
    // the errors waiting for the rows from the current one down; a row's slot is cleared once it is read
    let mut waiting = vec![[0.0_f32; 3]; ROWS_ALIVE * stride];
    let mut chosen = vec![0; height * width];
    let scale = amount / kernel.divisor;
    for row in 0..height {
        for column in 0..width {
            let at = (row % ROWS_ALIVE) * stride + REACH + column;
            let pixel = values[row * width + column];
            let value = [0, 1, 2].map(|channel| pixel[channel] + waiting[at][channel]);
            let pick = nearest_one(&value, palette);
            chosen[row * width + column] = pick;
            let colour = palette[usize::from(pick)];
            let error = [0, 1, 2].map(|channel| (value[channel] - colour[channel]) * scale);
            for &(right, down, weight) in kernel.taps {
                let to = ((row + down) % ROWS_ALIVE) * stride + (REACH + column).wrapping_add_signed(right);
                for (sum, part) in waiting[to].iter_mut().zip(error) {
                    *sum += part * weight;
                }
            }
        }
        let done = (row % ROWS_ALIVE) * stride;
        waiting[done..done + stride].fill([0.0; 3]);
    }
    chosen
}

/// The repeating pattern of an ordered dither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tile {
    /// The 8 x 8 matrix of Bayer's recursive construction, a regular crosshatch.
    Bayer,
    /// A 64 x 64 void-and-cluster tile: the points below any level are spread evenly, with no clumps or lines.
    BlueNoise,
}

/// Bayer's matrix side.
const BAYER_SIDE: usize = 8; // constant: Bayer (1973), the 8 x 8 matrix, 64 levels
/// The blue-noise tile's side.
const BLUE_SIDE: usize = 64; // constant: Ulichney (1993), void-and-cluster: 64 x 64, so 4096 levels
/// The standard deviation in px of the Gaussian that measures how crowded a spot is.
const BLUE_SIGMA: f64 = 1.5; // constant: Ulichney (1993), void-and-cluster
/// The share of the points the first pattern starts with, as a divisor of the tile's cell count.
const BLUE_START: usize = 10; // constant: Ulichney (1993): the first pattern holds a tenth of the points
/// The seed of the first pattern: any seed gives a good tile; this one is fixed, so the pattern is the same everywhere.
const BLUE_SEED: u32 = 0; // constant: any fixed seed makes the same tile everywhere

impl Tile {
    /// The tile's side.
    const fn side(self) -> usize {
        match self {
            Self::Bayer => BAYER_SIDE,
            Self::BlueNoise => BLUE_SIDE,
        }
    }

    /// The tile's ranks, 0 to side x side - 1 each once, row after row. Built once.
    fn ranks(self) -> &'static [u16] {
        static BAYER: OnceLock<Vec<u16>> = OnceLock::new();
        static BLUE: OnceLock<Vec<u16>> = OnceLock::new();
        match self {
            Self::Bayer => BAYER.get_or_init(bayer),
            Self::BlueNoise => BLUE.get_or_init(blue_noise),
        }
    }
}

/// The ranks of the 8 x 8 Bayer matrix: from [[0]], each doubling is [[4M, 4M + 2], [4M + 3, 4M + 1]].
fn bayer() -> Vec<u16> {
    let mut side = 1; // constant: Bayer (1973) starts from the 1 x 1 matrix [[0]]
    let mut ranks = vec![0_u16];
    while side < BAYER_SIDE {
        let mut doubled = vec![0; 4 * side * side]; // constant: a doubling has four quarters
        for row in 0..side {
            for column in 0..side {
                let base = 4 * ranks[row * side + column]; // constant: Bayer (1973), the recursive construction
                let at = |down: usize, right: usize| (row + down * side) * 2 * side + column + right * side;
                doubled[at(0, 0)] = base;
                doubled[at(0, 1)] = base + 2; // constant: Bayer (1973), the recursive construction
                doubled[at(1, 0)] = base + 3; // constant: Bayer (1973), the recursive construction
                doubled[at(1, 1)] = base + 1;
            }
        }
        ranks = doubled;
        side *= 2;
    }
    ranks
}

/// How much a point at (`row`, `column`) crowds every spot of a tile, added (`sign` 1) to or taken (`sign` -1) from
/// `crowded`, from the doubled Gaussian `crowd`, so that a window of the tile's size places it anywhere, wrapping.
fn crowd_by(crowded: &mut [f64], crowd: &[f64], (row, column): (usize, usize), sign: f64) {
    let size = BLUE_SIDE;
    for (line, spots) in crowded.chunks_exact_mut(size).enumerate() {
        let from = (size - row + line) * 2 * size + size - column;
        for (spot, near) in spots.iter_mut().zip(&crowd[from..from + size]) {
            *spot += sign * near;
        }
    }
}

/// The spot of `crowded` among those `counts` that is most crowded (the first of equals).
fn tightest(crowded: &[f64], counts: impl Fn(usize) -> bool) -> usize {
    let mut best = (f64::NEG_INFINITY, 0);
    for (at, &value) in crowded.iter().enumerate() {
        if counts(at) && value > best.0 {
            best = (value, at);
        }
    }
    best.1
}

/// The spot of `crowded` among those `counts` that is least crowded (the first of equals).
fn largest_void(crowded: &[f64], counts: impl Fn(usize) -> bool) -> usize {
    let mut best = (f64::INFINITY, 0);
    for (at, &value) in crowded.iter().enumerate() {
        if counts(at) && value < best.0 {
            best = (value, at);
        }
    }
    best.1
}

/// The spot's (row, column).
const fn at(spot: usize) -> (usize, usize) {
    (spot / BLUE_SIDE, spot % BLUE_SIDE)
}

/// Ranks 0..4095 of the blue-noise tile by void-and-cluster (Ulichney, 1993): the points of every prefix of the ranking
/// are spread as evenly as the tile allows, wrapping around its edges.
fn blue_noise() -> Vec<u16> {
    let size = BLUE_SIDE;
    let total = size * size;
    let wrapped = |index: usize| index.min(size - index);
    // The Gaussian crowding of a point on every spot, twice each way, so that a window of the tile's size onto it places
    // the Gaussian anywhere, wrapping.
    let mut crowd = vec![0.0; 4 * total]; // constant: two copies each way, so four tiles
    for row in 0..2 * size {
        for column in 0..2 * size {
            let (down, right) = (wrapped(row % size), wrapped(column % size));
            #[expect(clippy::cast_precision_loss, reason = "a distance of at most 32 px")]
            let squares = (down * down + right * right) as f64;
            crowd[row * 2 * size + column] = (-squares / (2.0 * BLUE_SIGMA * BLUE_SIGMA)).exp();
        }
    }
    let mut points = vec![false; total];
    for spot in Rng::seeded(BLUE_SEED).sample(total, total / BLUE_START) {
        points[spot] = true;
    }
    let mut crowded = vec![0.0; total];
    for spot in (0..total).filter(|&spot| points[spot]) {
        crowd_by(&mut crowded, &crowd, at(spot), 1.0);
    }
    // move the tightest cluster's point to the largest void until that is where it already is
    loop {
        let cluster = tightest(&crowded, |spot| points[spot]);
        points[cluster] = false;
        crowd_by(&mut crowded, &crowd, at(cluster), -1.0);
        let void = largest_void(&crowded, |spot| !points[spot]);
        points[void] = true;
        crowd_by(&mut crowded, &crowd, at(void), 1.0);
        if void == cluster {
            break;
        }
    }
    let mut ranks = vec![0_u16; total];
    let ones = points.iter().filter(|&&point| point).count();
    let (start_points, start_crowded) = (points.clone(), crowded.clone());
    #[expect(clippy::cast_possible_truncation, reason = "a rank is below 4096")]
    let rank_of = |rank: usize| rank as u16;
    // take the points away, tightest cluster first, counting down
    for rank in (0..ones).rev() {
        let cluster = tightest(&crowded, |spot| points[spot]);
        ranks[cluster] = rank_of(rank);
        points[cluster] = false;
        crowd_by(&mut crowded, &crowd, at(cluster), -1.0);
    }
    points = start_points;
    crowded = start_crowded;
    // add points into the largest voids, counting up to half the tile
    for rank in ones..total / 2 {
        let void = largest_void(&crowded, |spot| !points[spot]);
        ranks[void] = rank_of(rank);
        points[void] = true;
        crowd_by(&mut crowded, &crowd, at(void), 1.0);
    }
    // for the rest the empty spots are the points: the tightest cluster of them is next
    crowded.fill(0.0);
    for spot in (0..total).filter(|&spot| !points[spot]) {
        crowd_by(&mut crowded, &crowd, at(spot), 1.0);
    }
    for rank in total / 2..total {
        let cluster = tightest(&crowded, |spot| !points[spot]);
        ranks[cluster] = rank_of(rank);
        points[cluster] = true;
        crowd_by(&mut crowded, &crowd, at(cluster), -1.0);
    }
    ranks
}

/// Most candidate colours an ordered dither mixes a pixel from, as many as the common form of Knoll's algorithm uses; more
/// would resolve the tile's levels finer and cost more.
const CANDIDATES: usize = 16; // constant: the common form of Knoll's mixing uses 16 candidates

/// The palette colour chosen for each pixel of `values` (`height` x `width`, row after row) by ordered dithering: indices
/// into `palette`, for any palette (not only one that runs from dark to light).
///
/// `values` and `palette` are in a space where distance is the Euclidean one (Oklab or RGB) and whose first channel orders
/// the candidates (lightness, or red). Each pixel first gets a mix that averages to its value: candidate colours picked
/// one after another, each the palette colour closest to the pixel's value plus the error of the candidates so far (Knoll's
/// mixing; the error is what the colours chosen for it still owe it). There are n candidates, n = 1 + round(15 `amount`)
/// with `amount` from 0 to 1: 16 at the full pattern, fewer for a weaker one, and at `amount` 0 only the first, the palette
/// colour nearest to the value. The candidates are put in order of the first channel (ties by palette order) and the
/// `tile`, repeating across the picture from its top left corner, picks one of them: a pixel with level T takes the
/// candidate at position floor(n T), the level of a rank being (rank + 0.5) / count.
pub(crate) fn ordered(
    values: &[Point],
    (height, width): (usize, usize),
    palette: &[Point],
    tile: Tile,
    amount: f32,
) -> Vec<u8> {
    let (side, ranks) = (tile.side(), tile.ranks());
    #[expect(clippy::cast_precision_loss, reason = "a small constant")]
    let extra = (CANDIDATES - 1) as f64;
    let steps = 1 + whole(f64::from(amount) * extra);
    // palette indices, darkest first (ties in palette order), and the place of each in that order
    let mut darkest_first: Vec<u8> = (0..=u8::MAX).take(palette.len()).collect();
    darkest_first.sort_by(|&a, &b| palette[usize::from(a)][0].total_cmp(&palette[usize::from(b)][0]));
    let mut place = vec![0_u8; palette.len()];
    for (at, &index) in darkest_first.iter().enumerate() {
        place[usize::from(index)] = u8::try_from(at).unwrap_or(u8::MAX);
    }
    let mut chosen = vec![0; height * width];
    par::rows(&mut chosen, width, |first_row, band| {
        let mut mix = [0_u8; CANDIDATES];
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first_row + offset;
            for (column, pick) in line.iter_mut().enumerate() {
                let goal = values[row * width + column];
                let mut owed = [0.0_f32; 3];
                for candidate in &mut mix[..steps] {
                    let probe = [0, 1, 2].map(|channel| goal[channel] + owed[channel]);
                    let index = nearest_one(&probe, palette);
                    *candidate = place[usize::from(index)];
                    let colour = palette[usize::from(index)];
                    for channel in 0..3 {
                        owed[channel] += goal[channel] - colour[channel];
                    }
                }
                mix[..steps].sort_unstable();
                // floor(steps x (rank + 0.5) / count), worked in whole numbers
                let rank = usize::from(ranks[(row % side) * side + column % side]);
                let position = (2 * rank + 1) * steps / (2 * side * side);
                *pick = darkest_first[usize::from(mix[position])];
            }
        }
    });
    chosen
}

//! Blue noise: a 64 x 64 tile of threshold ranks with no visible pattern, built once by Ulichney's void-and-cluster
//! method, for `dither`'s `blue_noise` method.

use crate::ops::random::Rng;

/// The blue-noise tile's side.
pub(crate) const SIDE: usize = 64; // constant: Ulichney (1993), void-and-cluster: 64 x 64, so 4096 levels
/// The standard deviation in px of the Gaussian that measures how crowded a spot is.
const SIGMA: f64 = 1.5; // constant: Ulichney (1993), void-and-cluster
/// The share of the points the first pattern starts with, as a divisor of the tile's cell count.
const START: usize = 10; // constant: Ulichney (1993): the first pattern holds a tenth of the points
/// The seed of the first pattern: any seed gives a good tile; this one is fixed, so the pattern is the same everywhere.
const SEED: u32 = 0; // constant: any fixed seed makes the same tile everywhere

/// How much a point at (`row`, `column`) crowds every spot of a tile, added (`sign` 1) to or taken (`sign` -1) from
/// `crowded`, from the doubled Gaussian `crowd`, so that a window of the tile's size places it anywhere, wrapping.
fn crowd_by(crowded: &mut [f64], crowd: &[f64], (row, column): (usize, usize), sign: f64) {
    let size = SIDE;
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
    (spot / SIDE, spot % SIDE)
}

/// Ranks 0..4095 of the blue-noise tile by void-and-cluster (Ulichney, 1993): the points of every prefix of the ranking
/// are spread as evenly as the tile allows, wrapping around its edges.
pub(crate) fn tile() -> Vec<u16> {
    let size = SIDE;
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
            crowd[row * 2 * size + column] = (-squares / (2.0 * SIGMA * SIGMA)).exp();
        }
    }
    let mut points = vec![false; total];
    for spot in Rng::seeded(SEED).sample(total, total / START) {
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

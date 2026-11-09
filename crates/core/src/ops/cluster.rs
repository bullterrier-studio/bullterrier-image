//! Grouping colours into clusters: k-means centres and the nearest-centre assignment.
//!
//! Points are colours in whatever space the caller measures distance in (Euclidean distance is used, so give it a
//! perceptual space such as Oklab). Nothing here knows about pictures.
//!
//! Promises:
//! - Deterministic: the same points, arguments and `seed` give the same centres, at any thread count. All randomness
//!   comes from [`Rng`] seeded with `seed`.
//! - There are at most 256 centres, so a centre is named by a `u8`.
//! - Distances are compared squared (no root) in `f32`; a point equally close to several centres goes to the one listed
//!   first.

use crate::ops::par;
use crate::ops::random::Rng;

/// A colour in the space distances are measured in.
pub(crate) type Point = [f32; 3];

/// How the first centres of [`kmeans`] are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Start {
    /// One at a time, each with probability proportional to its squared distance from those chosen so far (k-means++,
    /// Arthur and Vassilvitskii, 2007), which spreads them out.
    Spread,
    /// `k` different points chosen at random.
    Random,
}

/// The squared distance between two points.
pub(crate) fn squared(from: &Point, to: &Point) -> f32 {
    let (first, second, third) = (from[0] - to[0], from[1] - to[1], from[2] - to[2]);
    first * first + second * second + third * third
}

/// The index of the centre closest to `point`; `centres` has between 1 and 256 entries.
#[expect(
    clippy::cast_possible_truncation,
    reason = "there are at most 256 centres, so an index is a u8"
)]
pub(crate) fn nearest_one(point: &Point, centres: &[Point]) -> u8 {
    let mut best = (f32::INFINITY, 0);
    for (index, centre) in centres.iter().enumerate() {
        let distance = squared(point, centre);
        if distance < best.0 {
            best = (distance, index);
            if distance <= 0.0 {
                break; // nothing is closer, and a later centre could at best tie
            }
        }
    }
    best.1 as u8
}

/// The index of the closest centre for every point (`centres` has between 1 and 256 entries).
pub(crate) fn nearest(points: &[Point], centres: &[Point]) -> Vec<u8> {
    let mut labels = vec![0; points.len()];
    assign(points, centres, &mut labels);
    labels
}

/// [`nearest`] into `labels`, which has a place for every point.
fn assign(points: &[Point], centres: &[Point], labels: &mut [u8]) {
    par::rows(labels, 1, |first, band| {
        for (label, point) in band.iter_mut().zip(&points[first..]) {
            *label = nearest_one(point, centres);
        }
    });
}

/// The centres of `k` clusters of `points` by Lloyd's algorithm (at most 256 of them; `k` is at least 1, `points` not
/// empty).
///
/// Each round assigns every point to its nearest centre, then moves each centre to the mean of its points. A centre left
/// with no points jumps to the point farthest from its own centre (of equally far points, the one listed last), so no
/// centre is wasted. There are fewer than `k` centres only when there are fewer than `k` points. At most `rounds` rounds
/// run, and they stop early once no centre moved farther than `tolerance` in a round (0 runs until nothing moves). `seed`
/// seeds the choice of the first centres, and nothing else is random.
pub(crate) fn kmeans(points: &[Point], k: usize, start: Start, rounds: usize, tolerance: f32, seed: u32) -> Vec<Point> {
    let k = k.min(points.len());
    let mut rng = Rng::seeded(seed);
    let mut centres = first_centres(points, k, start, &mut rng);
    let mut labels = vec![0; points.len()];
    for _ in 0..rounds {
        assign(points, &centres, &mut labels);
        let mut sums = vec![[0.0_f64; 3]; k];
        let mut members = vec![0_usize; k];
        for (point, &label) in points.iter().zip(&labels) {
            let sum = &mut sums[usize::from(label)];
            for (total, &value) in sum.iter_mut().zip(point) {
                *total += f64::from(value);
            }
            members[usize::from(label)] += 1;
        }
        let mut moved = centres.clone();
        let mut empty = Vec::new();
        for (index, (sum, &count)) in sums.iter().zip(&members).enumerate() {
            if count == 0 {
                empty.push(index);
            } else {
                moved[index] = mean(sum, count);
            }
        }
        if !empty.is_empty() {
            relocate(points, &labels, &mut moved, &empty);
        }
        let shift = centres
            .iter()
            .zip(&moved)
            .map(|(a, b)| squared(a, b))
            .fold(0.0, f32::max);
        centres = moved;
        if shift.sqrt() <= tolerance {
            break;
        }
    }
    centres
}

/// `sum` over `count` points, as a point.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a count of pixels is far below 2^53, and the mean is kept in the f32 of the picture"
)]
pub(crate) fn mean(sum: &[f64; 3], count: usize) -> Point {
    sum.map(|total| (total / count as f64) as f32)
}

/// Puts each of the `empty` centres of `moved` on the point farthest from the centre it is given to (the last of equally
/// far points), the farthest first.
fn relocate(points: &[Point], labels: &[u8], moved: &mut [Point], empty: &[usize]) {
    let mut far: Vec<f32> = points
        .iter()
        .zip(labels)
        .map(|(point, &label)| squared(point, &moved[usize::from(label)]))
        .collect();
    for &index in empty {
        // `max_by` keeps the last of equal maxima
        if let Some((at, _)) = far.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)) {
            moved[index] = points[at];
            far[at] = -1.0; // constant: below every distance, so this point is not taken twice
        }
    }
}

/// The `k` starting centres, copied from `points`.
fn first_centres(points: &[Point], k: usize, start: Start, rng: &mut Rng) -> Vec<Point> {
    if start == Start::Random {
        return rng.sample(points.len(), k).into_iter().map(|at| points[at]).collect();
    }
    let first = rng.below(points.len());
    let mut centres = vec![points[first]];
    // the squared distance of every point to the nearest centre chosen so far
    let mut gap: Vec<f32> = points.iter().map(|point| squared(point, &points[first])).collect();
    while centres.len() < k {
        let total: f64 = gap.iter().map(|&distance| f64::from(distance)).sum();
        let pick = if total > 0.0 {
            let target = rng.uniform() * total;
            let (mut reached, mut last) = (0.0, 0);
            let mut chosen = None;
            for (at, &distance) in gap.iter().enumerate().filter(|(_, distance)| **distance > 0.0) {
                last = at;
                reached += f64::from(distance);
                if reached > target {
                    chosen = Some(at);
                    break;
                }
            }
            chosen.unwrap_or(last) // the sum is only rounded short of the target
        } else {
            rng.below(points.len()) // every point is already a centre
        };
        centres.push(points[pick]);
        for (distance, point) in gap.iter_mut().zip(points) {
            *distance = distance.min(squared(point, &points[pick]));
        }
    }
    centres
}

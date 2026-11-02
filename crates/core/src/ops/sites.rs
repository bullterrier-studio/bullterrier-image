//! The seeds of shards: a jittered honeycomb scattered over a frame, crowded towards an impact point, kept to the pixels a
//! selection chooses, and made exactly as many as asked for.

use super::length;
use super::outlines::Frame;
use super::pieces::chosen;
use super::random::Rng;

/// How the seeds of shards lie.
pub(crate) struct Honeycomb {
    /// How many seeds.
    pub(crate) count: u32,
    /// How far each seed strays from its place in the honeycomb, as a share of the spacing, 0 to 1.
    pub(crate) jitter: f64,
    /// Where the seeds crowd to, as shares of the frame across and down.
    pub(crate) impact: [f64; 2],
    /// How much they crowd there, 0 and up: 0 leaves them spread evenly.
    pub(crate) focus: f64,
    /// The seed of the random stream.
    pub(crate) seed: u32,
}

/// About the most points a honeycomb has (rounding its rows and columns can add half as many again) before its seeds are
/// kept to the chosen pixels. A frame that is mostly not chosen asks for a finer honeycomb to land `count` seeds on the
/// chosen part, without a bound on the memory; past this the honeycomb stops getting finer, and the seeds that are missing
/// are drawn at random among the chosen pixels instead.
const MOST_LATTICE: usize = 1 << 20; // constant: a honeycomb finer than this would hold fewer than 500 seeds in 1/2000 of the frame

/// The distance between the rows of a hexagonal lattice, in units of the distance between neighbours in a row.
const HEX_ROWS: f64 = 0.866_025_403_784_438_6; // constant: sqrt(3) / 2, the height of an equilateral triangle of side 1

/// The seeds `(x, y)` in px of `spec` in a `height` x `width` picture: exactly `spec.count`, each in a pixel that `chosen`
/// (a `height` x `width` selection, more than half) chooses, or any pixel without one. `frame` is the box of the chosen
/// pixels (or the picture) and holds at least one of them.
///
/// A honeycomb over the frame, fine enough for about `count` of its points to land on the chosen pixels, is jittered,
/// crowded towards the impact point and cut down to the points in chosen pixels. Of the points beyond `count` random
/// ones are dropped; for the missing ones random chosen pixels are drawn, so that a selection too small for the honeycomb
/// still gets its `count`.
pub(crate) fn sites(
    spec: &Honeycomb,
    frame: Frame,
    selection: Option<&[f32]>,
    height: usize,
    width: usize,
) -> Vec<[f64; 2]> {
    let [left, top, right, bottom] = frame;
    let (across, down) = (right - left, bottom - top);
    let count = spec.count as usize;
    let is_chosen = |index: usize| selection.is_none_or(|values| chosen(values[index]));
    let pixels = (0..height * width).filter(|&index| is_chosen(index)).count();
    let mut rng = Rng::seeded(spec.seed);

    // the honeycomb, `columns` points across in each of `rows` rows
    let wanted = (length::float(count) * across * down / length::float(pixels)).min(length::float(MOST_LATTICE));
    let columns = length::whole((wanted * across * HEX_ROWS / down).sqrt()).clamp(1, MOST_LATTICE);
    let rows = length::whole(wanted / length::float(columns)).max(1);
    let (spacing_across, spacing_down) = (across / length::float(columns), down / length::float(rows));
    let impact = [left + spec.impact[0] * across, top + spec.impact[1] * down];
    let reach = [left, right]
        .into_iter()
        .flat_map(|x| [top, bottom].map(|y| (x - impact[0]).hypot(y - impact[1])))
        .fold(0.0, f64::max);
    let mut kept = Vec::new();
    for row in 0..rows {
        for column in 0..columns {
            // alternate rows are shifted by half a spacing; the quarter keeps both shifts inside the frame
            let shift = if row % 2 == 0 { 0.25 } else { 0.75 }; // constant: a quarter, and a quarter plus half a spacing
            let mut site = [
                left + (length::float(column) + shift) * spacing_across,
                top + (length::float(row) + 0.5) * spacing_down, // constant: the middle of the row
            ];
            for (value, spacing) in site.iter_mut().zip([spacing_across, spacing_down]) {
                *value += (rng.uniform() - 0.5) * spec.jitter * spacing; // constant: the jitter strays either way
            }
            let away = [site[0] - impact[0], site[1] - impact[1]];
            let crowd = (away[0].hypot(away[1]) / reach).powf(spec.focus);
            site = [impact[0] + away[0] * crowd, impact[1] + away[1] * crowd];
            // the pixel holding the point must be in the picture and chosen
            let (x, y) = (site[0].floor(), site[1].floor());
            if (0.0..length::float(width)).contains(&x)
                && (0.0..length::float(height)).contains(&y)
                && is_chosen(length::whole(y) * width + length::whole(x))
            {
                kept.push(site);
            }
        }
    }

    if kept.len() > count {
        return rng.sample(kept.len(), count).into_iter().map(|at| kept[at]).collect();
    }
    // the missing seeds: the chosen pixels with these ranks among them (a pixel can be drawn twice), found in one pass
    let mut ranks: Vec<usize> = (kept.len()..count).map(|_| rng.below(pixels)).collect();
    ranks.sort_unstable();
    let mut drawn = ranks.into_iter().peekable();
    for (rank, index) in (0..height * width).filter(|&index| is_chosen(index)).enumerate() {
        while drawn.next_if_eq(&rank).is_some() {
            kept.push([
                length::float(index % width) + rng.uniform(),
                length::float(index / width) + rng.uniform(),
            ]);
        }
    }
    kept
}

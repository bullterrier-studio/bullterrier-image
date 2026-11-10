//! The seeds of pieces: a jittered honeycomb scattered over a frame, crowded towards an impact point, kept to the pixels a
//! selection chooses, and made exactly as many as asked for ([`sites`]); or points anywhere, or where a picture has detail
//! ([`scatter`], [`detail_weights`]).

use super::color::luminance;
use super::edge::Edge;
use super::edges::{at, sobel};
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

/// How likely a point is in each pixel of `pixels` (`height` x `width`, row after row): weights that add up to 1, a mix of
/// an even share, `1 - detail` of the total (0..1), and a share that follows the picture's edges, `detail` of it, in
/// proportion to how steeply the brightness changes at the pixel (the Sobel strength of the Rec. 709 luminance). A picture
/// with no edge at all has only the even share, so it is even whatever `detail` is. `edge` is what lies beyond the picture's
/// border.
pub(crate) fn detail_weights(
    pixels: &[[f32; 3]],
    (height, width): (usize, usize),
    detail: f64,
    edge: Edge,
) -> Vec<f64> {
    let plane: Vec<f32> = pixels.iter().map(|&pixel| luminance(pixel)).collect();
    let slopes = sobel(&plane, (height, width), edge, 1.0);
    let strength: Vec<f64> = (0..height * width)
        .map(|index| f64::from(slopes[at(width, index / width, index % width)].strength))
        .collect();
    let total: f64 = strength.iter().sum();
    let even = 1.0 / length::float(height * width);
    if total == 0.0 {
        return vec![even; height * width];
    }
    strength
        .iter()
        .map(|level| (1.0 - detail) * even + detail * level / total)
        .collect()
}

/// `count` points over a `height` x `width` picture, `(x, y)` in px: with no `weights` anywhere in the picture, every place
/// equally likely; with `weights` (non-negative, one for each pixel, row after row, adding up to more than 0) in a pixel with
/// a probability proportional to its weight, and anywhere inside the pixel. They come from `rng`.
pub(crate) fn scatter(
    count: usize,
    (height, width): (usize, usize),
    weights: Option<&[f64]>,
    rng: &mut Rng,
) -> Vec<[f64; 2]> {
    let (rows, columns) = (length::float(height), length::float(width));
    let Some(weights) = weights else {
        return (0..count)
            .map(|_| [rng.uniform() * columns, rng.uniform() * rows])
            .collect();
    };
    let mut running = 0.0; // constant: the running total starts at nothing
    let cumulative: Vec<f64> = weights
        .iter()
        .map(|weight| {
            running += weight;
            running
        })
        .collect();
    (0..count)
        .map(|_| {
            let drawn = rng.uniform() * running;
            let pixel = cumulative.partition_point(|upto| *upto <= drawn).min(weights.len() - 1);
            [
                length::float(pixel % width) + rng.uniform(),
                length::float(pixel / width) + rng.uniform(),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `height` x `width` grey picture, dark left of column `edge` and bright from it on.
    fn step(height: usize, width: usize, edge: usize) -> Vec<[f32; 3]> {
        (0..height * width)
            .map(|at| [f32::from(u8::from(at % width >= edge)); 3])
            .collect()
    }

    #[test]
    fn detail_weights_add_up_to_1_and_follow_the_edge_by_detail() {
        let (height, width) = (6, 10);
        let pixels = step(height, width, 5);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-12;
        for detail in [0.0, 0.25, 1.0] {
            let weights = detail_weights(&pixels, (height, width), detail, Edge::Nearest);
            assert!(near(weights.iter().sum::<f64>(), 1.0), "detail {detail}");
            // the share beyond the even one lies on the columns beside the step, 4 and 5, and nowhere else
            let even = (1.0 - detail) / 60.0;
            for (at, weight) in weights.iter().enumerate() {
                let beside = matches!(at % width, 4 | 5);
                assert!(
                    if beside {
                        *weight > even - 1e-15
                    } else {
                        near(*weight, even)
                    },
                    "detail {detail}, pixel {at}: {weight}"
                );
            }
        }
        // all of it on the step at detail 1
        let weights = detail_weights(&pixels, (height, width), 1.0, Edge::Nearest);
        assert!(near(weights.iter().step_by(width).sum::<f64>(), 0.0));
    }

    #[test]
    fn a_picture_with_no_edge_is_even_whatever_the_detail() {
        let weights = detail_weights(&[[0.4; 3]; 12], (3, 4), 1.0, Edge::Reflect);
        assert!(weights.iter().all(|weight| (weight - 1.0 / 12.0).abs() < 1e-15));
    }

    #[test]
    fn scattered_points_follow_the_weights_and_the_seed() {
        let mut weights = vec![0.0; 12];
        weights[7] = 3.0; // row 1, column 3 of a 3 x 4 picture
        let points = scatter(50, (3, 4), Some(&weights), &mut Rng::seeded(1));
        assert_eq!(points.len(), 50);
        assert!(
            points
                .iter()
                .all(|p| (3.0..4.0).contains(&p[0]) && (1.0..2.0).contains(&p[1]))
        );
        let anywhere = scatter(200, (3, 4), None, &mut Rng::seeded(1));
        assert!(
            anywhere
                .iter()
                .all(|p| (0.0..4.0).contains(&p[0]) && (0.0..3.0).contains(&p[1]))
        );
        let again = scatter(200, (3, 4), None, &mut Rng::seeded(1));
        let other = scatter(200, (3, 4), None, &mut Rng::seeded(2));
        assert!(anywhere == again && anywhere != other);
    }
}

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

#[cfg(test)]
mod tests {
    use super::{Honeycomb, sites};
    use crate::ops::length;

    fn comb(count: u32, jitter: f64, focus: f64, seed: u32) -> Honeycomb {
        Honeycomb {
            count,
            jitter,
            impact: [0.5, 0.5],
            focus,
            seed,
        }
    }

    /// The pixel (column, row) holding each seed.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a seed lies within a few thousand pixels of the picture"
    )]
    fn pixels(seeds: &[[f64; 2]]) -> Vec<[i32; 2]> {
        seeds
            .iter()
            .map(|seed| seed.map(|value| value.floor() as i32))
            .collect()
    }

    #[test]
    fn a_honeycomb_without_jitter_has_its_rows_half_a_spacing_apart() {
        // 12 seeds over 8 x 6 px: 4 columns (2 px apart) in 3 rows (2 px apart); the rows alternate between a quarter
        // spacing (0.5 px) in and three quarters (1.5 px), and lie at 1, 3 and 5 px
        let made = sites(&comb(12, 0.0, 0.0, 1), [0.0, 0.0, 8.0, 6.0], None, 6, 8);
        let expected: Vec<[f64; 2]> = [1.0, 3.0, 5.0]
            .iter()
            .zip([0.5, 1.5, 0.5])
            .flat_map(|(y, first)| (0..4).map(move |column| [first + 2.0 * f64::from(column), *y]))
            .collect();
        assert_eq!(made, expected);
    }

    #[test]
    fn the_rows_of_the_honeycomb_are_as_close_as_a_hexagonal_lattice_has_them() {
        // 16 seeds over 8 x 6 px: the points are about 2 px apart along a row and the rows sqrt(3)/2 of that apart, which
        // makes 4 columns in 4 rows (rows 1.5 px apart); a square lattice would make 5 columns in 3 rows
        let made = sites(&comb(16, 0.0, 0.0, 1), [0.0, 0.0, 8.0, 6.0], None, 6, 8);
        let ys: Vec<f64> = made.iter().map(|seed| seed[1]).collect();
        assert_eq!(
            ys,
            [
                0.75, 0.75, 0.75, 0.75, 2.25, 2.25, 2.25, 2.25, 3.75, 3.75, 3.75, 3.75, 5.25, 5.25, 5.25, 5.25
            ]
        );
    }

    #[test]
    fn jitter_moves_a_seed_by_up_to_that_share_of_the_spacing_and_no_more_with_less() {
        // 12 seeds over 80 x 60 px: 4 columns (20 px apart) in 3 rows (20 px apart), all kept at these jitters, so the same
        // random numbers move the same seeds: twice the jitter, twice the move, and at most half the jitter in spacings
        let at = |jitter| sites(&comb(12, jitter, 0.0, 5), [0.0, 0.0, 80.0, 60.0], None, 60, 80);
        let (straight, small, large) = (at(0.0), at(0.2), at(0.4));
        let mut farthest = 0.0_f64;
        for ((still, little), much) in straight.iter().zip(&small).zip(&large) {
            for axis in 0..2 {
                let (little_move, much_move) = (little[axis] - still[axis], much[axis] - still[axis]);
                assert!((much_move - 2.0 * little_move).abs() < 1e-9);
                assert!(much_move.abs() <= 0.2 * 20.0 + 1e-9, "{much_move}");
                farthest = farthest.max(much_move.abs());
            }
        }
        assert!(farthest > 0.1 * 20.0, "{farthest}");
    }

    #[test]
    fn focus_moves_a_seed_to_its_distance_from_the_impact_point_times_that_share_of_the_reach_to_the_power_of_focus() {
        // one seed over 8 x 6 px at (2, 3); the impact point is the middle (4, 3), the reach to the farthest corner 5: the
        // seed is 2 away, at 2/5 of the reach, so it moves to (2/5)^2 = 0.16 of its distance with a focus of 2
        let one = |focus| sites(&comb(1, 0.0, focus, 1), [0.0, 0.0, 8.0, 6.0], None, 6, 8);
        assert_eq!(one(0.0), [[2.0, 3.0]]);
        let crowded = one(2.0);
        assert!((crowded[0][0] - (4.0 - 2.0 * 0.16)).abs() < 1e-12 && (crowded[0][1] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn seeds_beyond_the_count_are_dropped_at_random_not_from_the_end() {
        // 11 seeds over 8 x 6 px make the same 4 x 3 honeycomb as 12 do, so one of its 12 points is dropped: which one varies
        // with the seed, and the others stay in the honeycomb's order
        let frame = [0.0, 0.0, 8.0, 6.0];
        let honeycomb = sites(&comb(12, 0.0, 0.0, 1), frame, None, 6, 8);
        let mut dropped = std::collections::BTreeSet::new();
        for seed in 0..40 {
            let made = sites(&comb(11, 0.0, 0.0, seed), frame, None, 6, 8);
            let missing: Vec<usize> = (0..12).filter(|&at| !made.contains(&honeycomb[at])).collect();
            assert_eq!(missing.len(), 1, "seed {seed}");
            let rest: Vec<[f64; 2]> = honeycomb
                .iter()
                .enumerate()
                .filter(|(at, _)| *at != missing[0])
                .map(|(_, seed)| *seed)
                .collect();
            assert_eq!(made, rest, "seed {seed}");
            dropped.insert(missing[0]);
        }
        assert!(dropped.len() >= 8, "{dropped:?}");
    }

    #[test]
    fn there_are_exactly_as_many_seeds_as_asked_for_all_inside_the_chosen_pixels() {
        // the chosen pixels are the left third of a 60 x 40 picture and a small square beside it
        let selection: Vec<f32> = (0..40 * 60)
            .map(|at| {
                f32::from(u8::from(
                    at % 60 < 20 || (at % 60 >= 40 && at % 60 < 44 && (10..14).contains(&(at / 60))),
                ))
            })
            .collect();
        for seed in 0..25 {
            for (count, jitter, focus) in [
                (1, 0.5, 0.0),
                (17, 0.0, 0.0),
                (17, 1.0, 2.5),
                (500, 0.5, 5.0),
                (3, 1.0, 1.0),
            ] {
                let spec = comb(count, jitter, focus, seed);
                let whole = sites(&spec, [0.0, 0.0, 60.0, 40.0], None, 40, 60);
                assert_eq!(
                    whole.len(),
                    usize::try_from(count).unwrap_or(0),
                    "{count} seeds, seed {seed}"
                );
                assert!(
                    pixels(&whole)
                        .iter()
                        .all(|p| (0..60).contains(&p[0]) && (0..40).contains(&p[1]))
                );
                let part = sites(&spec, [0.0, 0.0, 44.0, 40.0], Some(&selection), 40, 60);
                assert_eq!(
                    part.len(),
                    usize::try_from(count).unwrap_or(0),
                    "{count} seeds in a selection, seed {seed}"
                );
                assert!(
                    pixels(&part)
                        .iter()
                        .all(|p| selection[length::whole(f64::from(p[1] * 60 + p[0]))] > 0.5)
                );
            }
        }
    }

    #[test]
    fn a_selection_of_two_pixels_in_a_huge_picture_still_gets_all_its_seeds_without_an_unbounded_honeycomb() {
        // 2 chosen pixels of 3 million: the honeycomb that would land 500 seeds on them has billions of points
        let mut selection = vec![0.0_f32; 1500 * 2000];
        selection[0] = 1.0;
        selection[1500 * 2000 - 1] = 1.0;
        let made = sites(
            &comb(500, 0.5, 0.0, 4),
            [0.0, 0.0, 2000.0, 1500.0],
            Some(&selection),
            1500,
            2000,
        );
        assert_eq!(made.len(), 500);
        assert!(pixels(&made).iter().all(|p| *p == [0, 0] || *p == [1999, 1499]));
        assert!(pixels(&made).contains(&[0, 0]) && pixels(&made).contains(&[1999, 1499]));
    }

    #[test]
    fn focus_crowds_the_seeds_towards_the_impact_point_the_more_the_larger_it_is() {
        let mean_distance = |focus: f64, impact: [f64; 2]| {
            let spec = Honeycomb {
                impact,
                ..comb(100, 0.5, focus, 2)
            };
            let made = sites(&spec, [0.0, 0.0, 100.0, 100.0], None, 100, 100);
            let at = [impact[0] * 100.0, impact[1] * 100.0];
            made.iter().map(|s| (s[0] - at[0]).hypot(s[1] - at[1])).sum::<f64>() / 100.0
        };
        for impact in [[0.5, 0.5], [0.2, 0.7], [0.0, 0.0]] {
            let spread = [0.0, 1.0, 2.5, 5.0].map(|focus| mean_distance(focus, impact));
            assert!(
                spread.windows(2).all(|pair| pair[1] < pair[0]),
                "{impact:?}: {spread:?}"
            );
        }
    }

    #[test]
    fn the_same_seed_scatters_the_same_seeds_and_another_seed_other_ones() {
        let place = |seed| sites(&comb(30, 0.8, 0.0, seed), [0.0, 0.0, 90.0, 60.0], None, 60, 90);
        assert_eq!(place(7), place(7));
        assert_ne!(place(7), place(8));
    }
}

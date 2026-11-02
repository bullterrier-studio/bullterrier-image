//! Where the seeds of `shards` lie, seen through the pieces they make: a region of shards is the Voronoi cells of its seeds,
//! so a regular honeycomb (no jitter, no focus) is checked against a brute-force nearest-seed scan over seeds listed by
//! hand, and the effects of jitter, focus and the count are checked as properties of the pieces.

mod common;

use bullterrier_image::region::Shards;
use bullterrier_image::{Error, Options, Region};
use common::Outcome;
use common::ops::{first_difference, float, partition, probe};

/// A regular honeycomb of `count` seeds.
fn honeycomb(count: u32) -> Shards {
    Shards {
        count,
        jitter: 0.0,
        focus: 0.0,
        ..Shards::default()
    }
}

/// The pieces of a `height` x `width` picture that `sites` (px) are the cells of, by trying every seed at every pixel
/// centre, the lowest index of those at one distance, numbered by first pixel.
fn nearest_cells(sites: &[[f64; 2]], height: usize, width: usize) -> Vec<i32> {
    let mut numbers: Vec<Option<i32>> = vec![None; sites.len()];
    let mut next = 0;
    (0..height * width)
        .map(|at| {
            let centre = [float(at % width) + 0.5, float(at / width) + 0.5];
            let distance = |site: &[f64; 2]| (site[0] - centre[0]).powi(2) + (site[1] - centre[1]).powi(2);
            let mut best = 0;
            for (index, site) in sites.iter().enumerate() {
                if distance(site) < distance(&sites[best]) {
                    best = index;
                }
            }
            *numbers[best].get_or_insert_with(|| {
                next += 1;
                next - 1
            })
        })
        .collect()
}

#[test]
fn a_honeycomb_without_jitter_has_its_rows_half_a_spacing_apart() -> Outcome {
    // 12 seeds over 8 x 6 px: 4 columns (2 px apart) in 3 rows (2 px apart); the rows alternate between a quarter
    // spacing (0.5 px) in and three quarters (1.5 px), and lie at 1, 3 and 5 px
    let sites: Vec<[f64; 2]> = [1.0, 3.0, 5.0]
        .iter()
        .zip([0.5, 1.5, 0.5])
        .flat_map(|(y, first)| (0..4).map(move |column| [first + 2.0 * f64::from(column), *y]))
        .collect();
    let (labels, pieces) = partition((6, 8), None, honeycomb(12))?;
    assert_eq!(pieces, 12);
    assert_eq!(
        first_difference(&labels, &nearest_cells(&sites, 6, 8)),
        None,
        "{labels:?}"
    );
    Ok(())
}

#[test]
fn the_rows_of_the_honeycomb_are_as_close_as_a_hexagonal_lattice_has_them() -> Outcome {
    // 16 seeds over 8 x 6 px: the points are about 2 px apart along a row and the rows sqrt(3)/2 of that apart, which
    // makes 4 columns in 4 rows (rows 1.5 px apart, at 0.75, 2.25, 3.75 and 5.25); a square lattice would make 5
    // columns in 3 rows
    let sites: Vec<[f64; 2]> = [0.75, 2.25, 3.75, 5.25]
        .iter()
        .zip([0.5, 1.5, 0.5, 1.5])
        .flat_map(|(y, first)| (0..4).map(move |column| [first + 2.0 * f64::from(column), *y]))
        .collect();
    let (labels, pieces) = partition((6, 8), None, honeycomb(16))?;
    assert_eq!(pieces, 16);
    assert_eq!(
        first_difference(&labels, &nearest_cells(&sites, 6, 8)),
        None,
        "{labels:?}"
    );
    Ok(())
}

#[test]
fn of_seeds_at_the_same_distance_from_a_pixel_the_earlier_in_the_honeycomb_has_it() -> Outcome {
    // the honeycomb of 12 seeds over 8 x 6 px has pixel centres exactly between seeds; the scan above gives them to the
    // lowest index, so listing the seeds the other way round must change the labels of those pixels
    let sites: Vec<[f64; 2]> = [1.0, 3.0, 5.0]
        .iter()
        .zip([0.5, 1.5, 0.5])
        .flat_map(|(y, first)| (0..4).map(move |column| [first + 2.0 * f64::from(column), *y]))
        .collect();
    let backwards: Vec<[f64; 2]> = sites.iter().rev().copied().collect();
    let (labels, _) = partition((6, 8), None, honeycomb(12))?;
    assert_ne!(
        first_difference(&nearest_cells(&sites, 6, 8), &nearest_cells(&backwards, 6, 8)),
        None
    );
    assert_eq!(first_difference(&labels, &nearest_cells(&sites, 6, 8)), None);
    Ok(())
}

#[test]
fn seeds_beyond_the_count_are_dropped_at_random_not_from_the_end() -> Outcome {
    // 11 seeds over 8 x 6 px make the same 4 x 3 honeycomb as 12 do, so one of its 12 points is dropped: which one varies
    // with the seed, and the others stay in the honeycomb's order
    let honeycomb_12: Vec<[f64; 2]> = [1.0, 3.0, 5.0]
        .iter()
        .zip([0.5, 1.5, 0.5])
        .flat_map(|(y, first)| (0..4).map(move |column| [first + 2.0 * f64::from(column), *y]))
        .collect();
    let mut dropped = std::collections::BTreeSet::new();
    for seed in 0..40 {
        let (labels, pieces) = partition((6, 8), None, Shards { seed, ..honeycomb(11) })?;
        assert_eq!(pieces, 11, "seed {seed}");
        let which: Vec<usize> = (0..12)
            .filter(|&missing| {
                let rest: Vec<[f64; 2]> = honeycomb_12
                    .iter()
                    .enumerate()
                    .filter(|(at, _)| *at != missing)
                    .map(|(_, site)| *site)
                    .collect();
                first_difference(&labels, &nearest_cells(&rest, 6, 8)).is_none()
            })
            .collect();
        assert_eq!(which.len(), 1, "seed {seed}: no single seed is missing from {labels:?}");
        dropped.insert(which[0]);
    }
    assert!(dropped.len() >= 8, "{dropped:?}");
    Ok(())
}

#[test]
fn every_seed_is_as_likely_to_be_the_one_dropped() -> Outcome {
    // one of 12 seeds is dropped for each seed value: 1,200 values, 100 times each when fair; the margin of 40 % is
    // far wider than chance
    let honeycomb_12: Vec<[f64; 2]> = [1.0, 3.0, 5.0]
        .iter()
        .zip([0.5, 1.5, 0.5])
        .flat_map(|(y, first)| (0..4).map(move |column| [first + 2.0 * f64::from(column), *y]))
        .collect();
    let variants: Vec<Vec<i32>> = (0..12)
        .map(|missing| {
            let rest: Vec<[f64; 2]> = honeycomb_12
                .iter()
                .enumerate()
                .filter(|(at, _)| *at != missing)
                .map(|(_, site)| *site)
                .collect();
            nearest_cells(&rest, 6, 8)
        })
        .collect();
    let mut counts = [0_u32; 12];
    for seed in 0..1200 {
        let (labels, _) = partition((6, 8), None, Shards { seed, ..honeycomb(11) })?;
        let which = variants.iter().position(|variant| *variant == labels);
        counts[which.ok_or_else(|| Error::Recipe(format!("seed {seed}: no seed is missing")))?] += 1;
    }
    assert!(counts.iter().all(|count| (60..=140).contains(count)), "{counts:?}");
    Ok(())
}

#[test]
fn the_pieces_are_never_more_than_the_seeds_and_a_regular_honeycomb_gives_each_seed_a_piece() -> Outcome {
    for (count, jitter, focus, seed) in [(1, 0.5, 0.0, 0), (17, 0.0, 0.0, 3), (17, 1.0, 2.5, 4), (3, 1.0, 1.0, 5)] {
        for fit in [false, true] {
            let region = Shards {
                count,
                jitter,
                focus,
                seed,
                fit: fit.then(|| "subject".to_owned()),
                ..Shards::default()
            };
            // the chosen pixels are the left third of a 60 x 40 picture and a small square beside it
            let subject: Vec<f32> = (0..40 * 60)
                .map(|at| {
                    f32::from(u8::from(
                        at % 60 < 20 || (at % 60 >= 40 && at % 60 < 44 && (10..14).contains(&(at / 60))),
                    ))
                })
                .collect();
            let seen = probe(
                (40, 60),
                fit.then_some(subject),
                Options::default(),
                vec![("s", Region::from(region))],
                (Some("s"), None),
            )?;
            let wanted = usize::try_from(count)?;
            assert!(
                (1..=wanted).contains(&seen.pieces),
                "{count} seeds, fit {fit}: {} pieces",
                seen.pieces
            );
            if jitter == 0.0 && !fit {
                assert_eq!(seen.pieces, wanted);
            }
        }
    }
    Ok(())
}

/// The mean distance from the pixel centres' piece centroids to `at` (px), over the pieces.
fn mean_centroid_distance(labels: &[i32], pieces: usize, width: usize, at: [f64; 2]) -> f64 {
    let mut sums = vec![[0.0_f64; 3]; pieces];
    for (index, &label) in labels.iter().enumerate() {
        let sum = &mut sums[usize::try_from(label).unwrap_or(0)];
        sum[0] += float(index % width) + 0.5;
        sum[1] += float(index / width) + 0.5;
        sum[2] += 1.0;
    }
    sums.iter()
        .map(|sum| (sum[0] / sum[2] - at[0]).hypot(sum[1] / sum[2] - at[1]))
        .sum::<f64>()
        / float(pieces)
}

#[test]
fn focus_crowds_the_shards_towards_the_impact_point_the_more_the_larger_it_is() -> Outcome {
    for impact in [[0.5, 0.5], [0.2, 0.7], [0.0, 0.0]] {
        let spread: Vec<f64> = [0.0, 1.0, 2.5, 5.0]
            .into_iter()
            .map(|focus| {
                let region = Shards {
                    count: 100,
                    jitter: 0.5,
                    focus,
                    impact,
                    seed: 2,
                    ..Shards::default()
                };
                let (labels, pieces) = partition((100, 100), None, region)?;
                Ok(mean_centroid_distance(
                    &labels,
                    pieces,
                    100,
                    [impact[0] * 100.0, impact[1] * 100.0],
                ))
            })
            .collect::<Result<_, Error>>()?;
        assert!(
            spread.windows(2).all(|pair| pair[1] < pair[0]),
            "{impact:?}: {spread:?}"
        );
    }
    Ok(())
}

#[test]
fn shards_laid_over_a_selection_have_their_seeds_inside_it() -> Outcome {
    // the chosen pixels are the left third of a 60 x 40 picture and a small square beside it: the box of the chosen pixels
    // holds a honeycomb whose points mostly fall between the two. A seed outside the chosen pixels would make a piece with
    // no chosen pixel in it (its own pixel is nearest to it), so every piece holds a chosen pixel
    let subject: Vec<f32> = (0..40 * 60)
        .map(|at| {
            f32::from(u8::from(
                at % 60 < 20 || (at % 60 >= 40 && at % 60 < 44 && (10..14).contains(&(at / 60))),
            ))
        })
        .collect();
    for seed in 0..20 {
        let region = Shards {
            count: 17,
            jitter: 0.5,
            fit: Some("subject".to_owned()),
            seed,
            ..Shards::default()
        };
        let seen = probe(
            (40, 60),
            Some(subject.clone()),
            Options::default(),
            vec![("s", Region::from(region))],
            (Some("s"), None),
        )?;
        let chosen_pieces: std::collections::BTreeSet<i32> = seen
            .labels
            .iter()
            .zip(&subject)
            .filter(|&(_, &value)| value > 0.5)
            .map(|(&label, _)| label)
            .collect();
        assert!(
            seen.labels.iter().all(|label| chosen_pieces.contains(label)),
            "seed {seed}: a piece holds no chosen pixel"
        );
    }
    Ok(())
}

//! `slice_shift`: strips and bands of rows slid sideways with wrap. The random stream is the core's own, so the tests
//! check what every stream must give: each row a rotation of itself, slides within `amount`, strips and bands as many and
//! as tall as asked, the same picture for the same seed. The picture says where every pixel was read from, so a row's
//! slide is read off it.

#![expect(clippy::cast_possible_wrap, reason = "small picture sizes")]

use std::collections::BTreeSet;

use bullterrier_image::Error;
use bullterrier_image::effects::slice_shift::Params;

use crate::common::compare::first_difference;
use crate::common::pictures::{coordinates, places};
use crate::common::pixels::run_picture;

const HEIGHT: usize = 200;
const WIDTH: usize = 101;

/// `slice_shift` of the coordinates picture (`HEIGHT` x `WIDTH`) and how far each row slid to the right, in px, as a
/// number from -50 to 50. Fails, naming the row, when a row is not its own pixels rotated, or came from another row.
fn slides(params: &Params) -> Result<Vec<i64>, Error> {
    let out = run_picture(params, HEIGHT, WIDTH, &coordinates(HEIGHT, WIDTH))?;
    let read = places(&out, HEIGHT, WIDTH);
    let mut slid = Vec::new();
    for row in 0..HEIGHT {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "small, non-negative places"
        )]
        let came_from = |column: usize| {
            let (from_row, from_column) = read[row * WIDTH + column];
            (from_row.round() as usize, from_column.round() as usize)
        };
        // the pixel now at column 0 was at column `first`, so the row slid `WIDTH - first` to the right
        let first = came_from(0).1;
        for column in 0..WIDTH {
            assert_eq!(
                came_from(column),
                (row, (first + column) % WIDTH),
                "{params:?}: row {row} is not a rotation of itself at column {column}"
            );
        }
        let to_the_right = ((WIDTH - first) % WIDTH) as i64;
        slid.push(if to_the_right > 50 {
            to_the_right - WIDTH as i64
        } else {
            to_the_right
        });
    }
    Ok(slid)
}

/// The number of runs of rows that slid alike.
fn runs(slides: &[i64]) -> usize {
    1 + slides.windows(2).filter(|pair| pair[0] != pair[1]).count()
}

/// The lengths of the runs of rows that moved (slid by other than 0).
fn moved_runs(slides: &[i64]) -> Vec<usize> {
    slides
        .split(|slide| *slide == 0)
        .map(<[i64]>::len)
        .filter(|length| *length > 0)
        .collect()
}

fn tear(slices: i32, amount: f32, seed: u32) -> Params {
    Params {
        slices,
        amount,
        seed,
        bands: 0,
        ..Params::default()
    }
}

fn bands(bands: i32, lowest: f32, tallest: f32, seed: u32) -> Params {
    Params {
        slices: 0,
        amount: 30.0,
        seed,
        bands,
        band_min_height: lowest,
        band_max_height: tallest,
    }
}

#[test]
fn nothing_slides_without_slices_and_bands_or_without_an_amount() -> Result<(), Error> {
    let picture = coordinates(HEIGHT, WIDTH);
    for params in [
        Params {
            slices: 0,
            bands: 0,
            ..Params::default()
        },
        Params {
            amount: 0.0,
            slices: 30,
            bands: 20,
            ..Params::default()
        },
    ] {
        let out = run_picture(&params, HEIGHT, WIDTH, &picture)?;
        assert_eq!(first_difference(&out, &picture), None, "{params:?}");
    }
    Ok(())
}

#[test]
fn a_seed_gives_the_same_tear_and_another_seed_another() -> Result<(), Error> {
    let picture = coordinates(HEIGHT, WIDTH);
    let torn = |seed| {
        run_picture(
            &Params {
                bands: 5,
                seed,
                ..Params::default()
            },
            HEIGHT,
            WIDTH,
            &picture,
        )
    };
    assert_eq!(first_difference(&torn(7)?, &torn(7)?), None);
    for other in [0, 1, 8, u32::MAX] {
        assert!(
            first_difference(&torn(7)?, &torn(other)?).is_some(),
            "seed {other} tears like seed 7"
        );
    }
    Ok(())
}

#[test]
fn every_strip_slides_by_a_whole_number_of_pixels_within_the_amount_either_way_and_all_of_them_are_reached()
-> Result<(), Error> {
    // one strip a row (200 rows, more strips than rows): the slides are drawn 200 times, whole pixels either way, and an
    // amount rounds to the nearest whole pixel (2.6 -> 3, 2.4 -> 2)
    for (amount, most) in [(3.0, 3), (2.6, 3), (2.4, 2)] {
        let seen: BTreeSet<i64> = slides(&tear(500, amount, 4))?.into_iter().collect();
        assert_eq!(seen, (-most..=most).collect::<BTreeSet<_>>(), "amount {amount}");
    }
    // in a tear of few strips too
    for seed in 0..10 {
        assert!(
            slides(&tear(4, 2.6, seed))?.iter().all(|slide| slide.abs() <= 3),
            "seed {seed}"
        );
    }
    Ok(())
}

#[test]
fn the_picture_is_cut_in_as_many_strips_as_asked_and_a_strip_slides_as_one() -> Result<(), Error> {
    for (slices, most) in [(1, 1), (2, 2), (9, 9), (30, 30)] {
        for seed in 0..5 {
            let found = runs(&slides(&tear(slices, 40.0, seed))?);
            assert!(found <= most, "{slices} strips (seed {seed}) show {found} runs");
        }
    }
    // a tear of nearly all the strips possible, with slides too wide to repeat, shows nearly all of them
    let fine = runs(&slides(&tear(100, 40.0, 3))?);
    assert!((90..=100).contains(&fine), "100 strips show {fine} runs");
    // one strip is the whole picture, slid as one
    assert_eq!(runs(&slides(&tear(1, 40.0, 3))?), 1);
    Ok(())
}

#[test]
fn more_strips_than_rows_cut_at_every_row() -> Result<(), Error> {
    let picture = coordinates(HEIGHT, WIDTH);
    let at = |slices| run_picture(&tear(slices, 40.0, 2), HEIGHT, WIDTH, &picture);
    assert_eq!(first_difference(&at(200)?, &at(500)?), None);
    Ok(())
}

#[test]
fn a_band_is_as_tall_as_the_limits_allow_and_slides_nothing_beside_it() -> Result<(), Error> {
    let mut heights = BTreeSet::new();
    let mut distances = BTreeSet::new();
    for seed in 0..60 {
        let slid = slides(&bands(1, 6.0, 12.0, seed))?;
        distances.extend(slid.iter().copied().filter(|slide| *slide != 0));
        let found = moved_runs(&slid);
        // a slide of 0 (the band stays put) is rare; otherwise one run of rows moved, as tall as a band can be
        if let [height] = found[..] {
            assert!((6..=12).contains(&height), "seed {seed}: a band of {height} rows");
            heights.insert(height);
        } else {
            assert!(found.is_empty(), "seed {seed}: {found:?}");
        }
    }
    assert!(heights.len() >= 5, "heights drawn: {heights:?}");
    // a band slides either way, by up to the amount (30 px)
    assert!(distances.iter().all(|slide| slide.abs() <= 30), "{distances:?}");
    assert!(
        distances.iter().any(|slide| *slide < -20) && distances.iter().any(|slide| *slide > 20),
        "{distances:?}"
    );
    // equal limits give bands of that height; below 1 px a band is a row
    for (lowest, tallest, rows) in [(9.0, 9.0, 9), (0.0, 0.0, 1), (0.2, 0.9, 1)] {
        let mut moved = 0;
        for seed in 0..10 {
            let found = moved_runs(&slides(&bands(1, lowest, tallest, seed))?);
            assert!(
                found.is_empty() || found == [rows],
                "{lowest}..{tallest}, seed {seed}: {found:?}"
            );
            moved += usize::from(found == [rows]);
        }
        assert!(
            moved >= 7,
            "{lowest}..{tallest}: a band of {rows} rows moved in only {moved} of 10 seeds"
        );
    }
    Ok(())
}

#[test]
fn a_band_taller_than_the_picture_is_the_picture_and_slides_it_as_one() -> Result<(), Error> {
    let mut moved_seeds = 0;
    for seed in 0..10 {
        let slid = slides(&bands(1, 2000.0, 2000.0, seed))?;
        assert_eq!(runs(&slid), 1, "seed {seed}");
        moved_seeds += usize::from(slid[0] != 0);
    }
    assert!(moved_seeds >= 8, "only {moved_seeds} of 10 seeds slid the picture");
    Ok(())
}

#[test]
fn bands_are_as_many_as_asked() -> Result<(), Error> {
    // bands of 3 rows: n of them move at most 3n rows, and more bands move more than one does
    let moved = |count| -> Result<usize, Error> {
        Ok(slides(&bands(count, 3.0, 3.0, 11))?
            .iter()
            .filter(|slide| **slide != 0)
            .count())
    };
    assert!(moved(1)? <= 3);
    let ten = moved(10)?;
    assert!((4..=30).contains(&ten), "10 bands of 3 rows move {ten} rows");
    Ok(())
}

#[test]
fn a_row_in_two_bands_slides_by_both() -> Result<(), Error> {
    // two bands of 150 rows in 200 overlap in at least 100 rows: the rows of one band slide by a, the rows of both by
    // a + b, the rows of the other by b (all around the 101 px of a row)
    let mut checked = 0;
    for seed in 0..40 {
        let slid = slides(&bands(2, 150.0, 150.0, seed))?;
        let mut values: Vec<i64> = slid.iter().copied().filter(|slide| *slide != 0).collect();
        values.dedup();
        if let [first, both, last] = values[..] {
            assert_eq!(
                (first + last - both).rem_euclid(WIDTH as i64),
                0,
                "seed {seed}: {values:?}"
            );
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} of 40 seeds had three runs");
    Ok(())
}

#[test]
fn the_glitch_preset_tears_a_few_bands() -> Result<(), Error> {
    // the preset: no strips, 7 bands, 28.8 px, on a 480 px high picture at its working scale of 1
    let glitch = Params {
        slices: 0,
        bands: 7,
        amount: 28.8,
        ..Params::default()
    };
    let slid = slides(&glitch)?;
    assert!(slid.iter().all(|slide| slide.abs() <= 28));
    let moved = slid.iter().filter(|slide| **slide != 0).count();
    assert!((1..=7 * 28).contains(&moved), "{moved} rows moved");
    Ok(())
}

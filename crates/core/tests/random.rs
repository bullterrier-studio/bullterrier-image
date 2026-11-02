//! The seeded random stream, seen through the one place the public API shows it plainly: an `each` step with `Assign::Random`
//! draws, for each piece in order, the number of its look. The oracles are numbers from a transcription of the reference
//! generators (`SplitMix64` and xoshiro256++ of prng.di.unimi.it, with Lemire's bounded draw) written outside this crate.

mod common;

use bullterrier_image::region::{Grid, Shares};
use bullterrier_image::{Assign, Each, Error, Options, Photo, Picture, Presets, Recipe};
use common::Outcome;
use common::ops::level;

/// The index of the look each of `pieces` one-pixel pieces drew, of `looks` looks, with `seed`: a look paints the level
/// `(index + 1) / (looks + 1)`, which the piece's pixel then shows.
fn drawn(seed: u32, looks: usize, pieces: usize) -> Result<Vec<usize>, Error> {
    let steps = (0..looks)
        .map(|index| level(small(index + 1) / small(looks + 1)).map(Some))
        .collect::<Result<Vec<_>, _>>()?;
    let mut each = Each::new("cells", steps);
    each.assign = Assign::Random;
    each.seed = seed;
    each.feather = 0.0;
    let recipe = Recipe::new([each]).with_region(
        "cells",
        Grid {
            columns: Shares::equal(pieces),
            ..Grid::default()
        },
    );
    let pixels = vec![0.05; pieces * 3];
    let out = Photo::new(Picture::new(1, pieces, &pixels)?)
        .render(&recipe, &Presets::new(), Options::default())?
        .into_values();
    out.as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .map(|(piece, pixel)| {
            let shown = f64::from(pixel[0]) * float(looks + 1) - 1.0;
            let index = shown.round();
            if (shown - index).abs() > 1e-3 || index < 0.0 || index >= float(looks) {
                return Err(Error::Recipe(format!("piece {piece} shows {pixel:?}, no look")));
            }
            Ok(float_to_index(index))
        })
        .collect()
}

/// A small count as an f32.
fn small(count: usize) -> f32 {
    f32::from(u8::try_from(count).unwrap_or(u8::MAX))
}

/// `count` as a float: a count here is far below 2^53.
#[expect(clippy::cast_precision_loss, reason = "a count here is far below 2^53")]
const fn float(count: usize) -> f64 {
    count as f64
}

/// A small whole number held in a float.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a look index, checked to lie in 0..looks"
)]
const fn float_to_index(index: f64) -> usize {
    index as usize
}

#[test]
fn the_looks_a_seed_draws_are_those_of_the_reference_generator_with_lemires_bounded_draw() -> Outcome {
    // from a transcription of the reference generators (seed expanded by SplitMix64 into xoshiro256++, then Lemire's
    // multiply-shift rule for a number below the bound): seed 7 among 10 looks and seed 0 among 3, twelve pieces each
    assert_eq!(drawn(7, 10, 12)?, [0, 1, 7, 4, 9, 4, 7, 3, 9, 0, 1, 1]);
    assert_eq!(drawn(0, 3, 12)?, [0, 1, 1, 0, 1, 0, 2, 2, 0, 0, 0, 0]);
    Ok(())
}

#[test]
fn a_draw_reaches_every_look_under_each_count_of_looks_and_never_one_beyond() -> Outcome {
    for looks in 1..=7 {
        let seen = drawn(11, looks, 1000)?;
        assert!(seen.iter().all(|&look| look < looks), "{looks} looks");
        for look in 0..looks {
            assert!(seen.contains(&look), "{looks} looks: look {look} was never drawn");
        }
    }
    Ok(())
}

#[test]
fn a_look_is_drawn_about_as_often_as_any_other_over_a_fixed_seed() -> Outcome {
    // 12 seeds of 1,000 pieces among 6 looks: each look about 2,000 times; the margin only catches a stream that favours
    // a look
    let mut counts = [0_u32; 6];
    for seed in 2024..2036 {
        for look in drawn(seed, 6, 1_000)? {
            counts[look] += 1;
        }
    }
    assert!(counts.iter().all(|count| (1_800..=2_200).contains(count)), "{counts:?}");
    Ok(())
}

#[test]
fn one_look_is_always_the_one_drawn() -> Outcome {
    assert_eq!(drawn(5, 1, 100)?, vec![0; 100]);
    Ok(())
}

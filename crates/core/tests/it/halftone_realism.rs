//! `halftone`'s print realism (`dot_gain`, `ink_noise`, `specks`, `seed`): the oracles are the dot-gain formula, the mean of
//! an even tone, counts of specks, and comparisons between renders that must be alike or must differ.

#![expect(clippy::cast_precision_loss, reason = "small picture sizes")]

use bullterrier_image::Error;
use bullterrier_image::effects::halftone::{Params, Separation, Shape};
use bullterrier_image::params::Color;

use crate::common::pictures::varied;
use crate::common::pixels::run_picture;
use crate::halftone::{flat, oracle, red};

// ---- print realism ------------------------------------------------------------------------------------------------------

/// Black ink on white paper at square marks (a full tone inks every pixel), a cell of 20 and no blur.
fn press() -> Params {
    oracle(Shape::Square)
}

/// The ink map of `params` at an even `tone` on a `size` x `size` picture.
fn ink_at(params: &Params, size: usize, tone: f32) -> Result<Vec<f32>, Error> {
    Ok(red(params, size, size, &flat(size, size, 1.0 - tone))?
        .into_iter()
        .map(|r| 1.0 - r)
        .collect())
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len() as f32
}

#[test]
fn print_realism_at_zero_is_the_clean_screen_whatever_the_seed_and_speck_size() -> Result<(), Error> {
    let rgb = varied(60, 70);
    for separation in [Separation::None, Separation::Cmyk, Separation::Spot, Separation::Rgb] {
        let clean = Params {
            separation,
            ..Params::default()
        };
        let zeroed = Params {
            seed: 77,
            speck_size: 9.0,
            ..clean.clone()
        };
        assert_eq!(
            run_picture(&zeroed, 60, 70, &rgb)?,
            run_picture(&clean, 60, 70, &rgb)?,
            "{separation:?}"
        );
    }
    Ok(())
}

#[test]
fn dot_gain_is_its_value_at_half_tone_and_follows_the_curve_elsewhere() -> Result<(), Error> {
    // a' = a + 4 g a (1 - a): at a = 1/2 it is 0.5 + g; the screen keeps the area within 0.015
    for (gain, tone) in [
        (0.25, 0.5),
        (0.1, 0.5),
        (0.25, 0.2),
        (0.2, 0.8),
        (0.25, 0.0),
        (0.25, 1.0),
    ] {
        let params = Params {
            dot_gain: gain,
            ..press()
        };
        let want = tone + 4.0 * gain * tone * (1.0 - tone);
        let got = mean(&ink_at(&params, 80, tone)?);
        assert!(
            (got - want).abs() < 0.015,
            "gain {gain} tone {tone}: ink {got}, formula {want}"
        );
    }
    // more gain, more ink, at every tone
    for tone in [0.1, 0.3, 0.5, 0.7, 0.9] {
        let mut last = -1.0;
        for gain in [0.0, 0.05, 0.1, 0.15, 0.2, 0.25] {
            let params = Params {
                dot_gain: gain,
                ..press()
            };
            let ink = mean(&ink_at(&params, 80, tone)?);
            assert!(ink > last, "tone {tone} gain {gain}: {ink} after {last}");
            last = ink;
        }
    }
    Ok(())
}

#[test]
fn ink_noise_is_seeded_uneven_and_keeps_the_mean_ink() -> Result<(), Error> {
    let noisy = |seed| Params {
        ink_noise: 1.0,
        seed,
        ..press()
    };
    let (a, a_again, b) = (
        ink_at(&noisy(1), 400, 0.5)?,
        ink_at(&noisy(1), 400, 0.5)?,
        ink_at(&noisy(2), 400, 0.5)?,
    );
    assert_eq!(a, a_again);
    assert_ne!(a, b);
    let clean = ink_at(&press(), 400, 0.5)?;
    assert_ne!(a, clean);
    // 400 cells, each straying by up to 0.25 either way: the mean's spread is about 0.007
    assert!(
        (mean(&a) - mean(&clean)).abs() < 0.03,
        "{} vs {}",
        mean(&a),
        mean(&clean)
    );
    // some cells are inked more and some less than the even screen: the dots differ from one another
    let (high, low) = (
        a.iter().zip(&clean).filter(|(n, c)| **n > **c + 0.05).count(),
        a.iter().zip(&clean).filter(|(n, c)| **n < **c - 0.05).count(),
    );
    assert!(high > 20_000 && low > 20_000, "{high} more, {low} less"); // of 160000 pixels
    // a paper or full ink cell does not stray
    for tone in [0.0, 1.0] {
        assert_eq!(ink_at(&noisy(1), 80, tone)?, ink_at(&press(), 80, tone)?, "tone {tone}");
    }
    Ok(())
}

/// The number of 4-connected groups of paper (red above a half) in `ink` (1 minus red), `size` pixels a side.
fn specks_in(ink: &[f32], size: usize) -> usize {
    let mut seen = vec![false; ink.len()];
    let mut count = 0;
    for start in 0..ink.len() {
        if seen[start] || ink[start] > 0.5 {
            continue;
        }
        count += 1;
        let mut open = vec![start];
        seen[start] = true;
        while let Some(at) = open.pop() {
            let (row, column) = (at / size, at % size);
            let near = [
                (row > 0).then(|| at - size),
                (row + 1 < size).then(|| at + size),
                (column > 0).then(|| at - 1),
                (column + 1 < size).then(|| at + 1),
            ];
            for next in near.into_iter().flatten() {
                if !seen[next] && ink[next] <= 0.5 {
                    seen[next] = true;
                    open.push(next);
                }
            }
        }
    }
    count
}

#[test]
fn specks_bare_the_paper_in_ink_and_only_there_and_their_density_is_the_parameter() -> Result<(), Error> {
    // a full tone is ink everywhere, so every speck shows as a group of paper pixels; 2 per 1000 px^2 of 300 x 300 = 180
    let specked = |specks, speck_size, seed| Params {
        specks,
        speck_size,
        seed,
        ..press()
    };
    let full = ink_at(&specked(2.0, 3.0, 5), 300, 1.0)?;
    let found = specks_in(&full, 300);
    // 180 expected, give or take about 13 (a Poisson count's spread) and a few merging
    assert!((145..=215).contains(&found), "{found} specks of 180");
    // the same look at twice the size in design px: the scaled parameters are a quarter the density and twice the size
    let double = ink_at(&specked(0.5, 6.0, 5), 600, 1.0)?;
    let found_double = specks_in(&double, 600);
    assert!((145..=215).contains(&found_double), "{found_double} specks of 180");
    assert_eq!(specks_in(&ink_at(&specked(0.0, 3.0, 5), 300, 1.0)?, 300), 0);
    // another seed another specks; the same seed the same ones
    assert_eq!(full, ink_at(&specked(2.0, 3.0, 5), 300, 1.0)?);
    assert_ne!(full, ink_at(&specked(2.0, 3.0, 6), 300, 1.0)?);
    // no ink, no specks; in a mid tone a speck only takes ink away
    assert_eq!(ink_at(&specked(5.0, 10.0, 5), 100, 0.0)?, ink_at(&press(), 100, 0.0)?);
    let (with, without) = (ink_at(&specked(5.0, 4.0, 5), 200, 0.5)?, ink_at(&press(), 200, 0.5)?);
    assert!(with.iter().zip(&without).all(|(w, wo)| *w <= *wo + 1e-6));
    assert!(with.iter().zip(&without).any(|(w, wo)| *w < *wo - 0.5));
    Ok(())
}

#[test]
fn print_realism_works_with_every_separation() -> Result<(), Error> {
    let rgb = varied(80, 90);
    for separation in [Separation::None, Separation::Cmyk, Separation::Spot, Separation::Rgb] {
        let clean = Params {
            separation,
            paper: Color([1.0; 3]),
            ..Params::default()
        };
        let real = Params {
            dot_gain: 0.2,
            ink_noise: 0.5,
            specks: 5.0,
            ..clean.clone()
        };
        assert_ne!(
            run_picture(&real, 80, 90, &rgb)?,
            run_picture(&clean, 80, 90, &rgb)?,
            "{separation:?}"
        );
    }
    Ok(())
}

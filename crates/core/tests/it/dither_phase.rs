//! `dither` with `phase`: the pattern shifted right and down by a share of its tile. The oracles are the same picture at
//! phase 0 read back from the shifted place, the identity of phase 1, and the tone of a flat grey, which no shift changes.

use bullterrier_image::Error;
use bullterrier_image::effects::dither::{Method, Palette, Params, Separation, Space};
use bullterrier_image::params::Color;
use serde_json::json;

use crate::common::dither::{BLACK, WHITE, grey_matrix, near, pixels, run, with_matrix};

fn ink(method: Method, phase: f32) -> Params {
    Params {
        method,
        palette: Palette::Custom,
        custom: vec![Color(BLACK), Color(WHITE)],
        space: Space::Rgb,
        phase,
        ..Params::default()
    }
}

fn flat(grey: f32, height: usize, width: usize) -> (usize, usize, Vec<f32>) {
    (height, width, vec![grey; height * width * 3])
}

/// Whether `shifted` is `plain` moved right by `across` and down by `down`, wrapping round the picture's edges.
fn is_moved(plain: &[f32], shifted: &[f32], (height, width): (usize, usize), (down, across): (usize, usize)) -> bool {
    let (plain, shifted) = (pixels(plain), pixels(shifted));
    (0..height).all(|row| {
        (0..width).all(|column| {
            near(
                shifted[row * width + column],
                plain[(row + height - down) % height * width + (column + width - across) % width],
            )
        })
    })
}

fn lit(picture: &[f32]) -> usize {
    pixels(picture).iter().filter(|pixel| pixel[0] > 0.5).count()
}

#[test]
fn a_tile_shifts_right_and_down_by_whole_dots_of_its_side() -> Result<(), Error> {
    // 16 x 16 is whole tiles of every ordered matrix (side 2 .. 8), so a shift of k dots is a wrap of the picture
    let picture = flat(0.3, 16, 16);
    for (method, side) in [
        (Method::Bayer2, 2),
        (Method::Bayer4, 4),
        (Method::Bayer, 8),
        (Method::Clustered, 8),
    ] {
        let plain = run(&ink(method, 0.0), &picture)?;
        for k in 1..side {
            #[expect(clippy::cast_precision_loss, reason = "a small count")]
            let phase = k as f32 / side as f32; // a binary fraction: exact, so floor(phase x side) is k
            let moved = run(&ink(method, phase), &picture)?;
            assert!(
                is_moved(&plain, &moved, (16, 16), (k, k)),
                "{method:?}: phase {k}/{side} is not a shift by {k} dots"
            );
        }
    }
    // the shift is a real one: Bayer at 0.3 differs when moved by one dot
    let (plain, moved) = (
        run(&ink(Method::Bayer, 0.0), &picture)?,
        run(&ink(Method::Bayer, 0.125), &picture)?,
    );
    assert_ne!(plain, moved);
    Ok(())
}

#[test]
fn a_phase_between_two_dots_rounds_down() -> Result<(), Error> {
    let picture = flat(0.3, 16, 16);
    let plain = run(&ink(Method::Bayer, 0.0), &picture)?;
    // 0.2 x 8 = 1.6: one dot
    assert!(is_moved(
        &plain,
        &run(&ink(Method::Bayer, 0.2), &picture)?,
        (16, 16),
        (1, 1)
    ));
    // 0.1 x 8 = 0.8: none
    assert_eq!(run(&ink(Method::Bayer, 0.1), &picture)?, plain);
    Ok(())
}

#[test]
fn a_matrix_shifts_by_a_share_of_its_own_rows_and_columns() -> Result<(), Error> {
    // 2 rows x 4 columns: phase 0.5 is 1 row and 2 columns
    let matrix = grey_matrix(2, 4, &[0.0, 0.5, 0.1, 0.9, 0.3, 0.7, 0.2, 0.6])?;
    let picture = flat(0.4, 8, 8);
    let on = |phase: f32| {
        with_matrix(
            "matrix",
            Some(&matrix),
            &[("matrix", json!("m")), ("phase", json!(phase))],
            &picture,
        )
    };
    assert!(is_moved(&on(0.0)?, &on(0.5)?, (8, 8), (1, 2)));
    Ok(())
}

#[test]
fn white_noise_shifts_by_a_share_of_the_picture_and_wraps_in_both_directions() -> Result<(), Error> {
    // 20 x 30 at phase 0.25: 5 rows and 7 columns (7.5 rounded down); a column shift must not spill into the next row
    let picture = flat(0.4, 20, 30);
    let plain = run(&ink(Method::WhiteNoise, 0.0), &picture)?;
    let moved = run(&ink(Method::WhiteNoise, 0.25), &picture)?;
    assert!(is_moved(&plain, &moved, (20, 30), (5, 7)));
    assert_ne!(plain, moved);
    Ok(())
}

#[test]
fn a_phase_of_one_is_a_phase_of_nothing() -> Result<(), Error> {
    let picture = flat(0.3, 20, 30);
    for method in [
        Method::Bayer,
        Method::BlueNoise,
        Method::Clustered45,
        Method::GameboyCamera,
        Method::WhiteNoise,
    ] {
        assert_eq!(
            run(&ink(method, 1.0), &picture)?,
            run(&ink(method, 0.0), &picture)?,
            "{method:?}"
        );
    }
    Ok(())
}

#[test]
fn a_phase_keeps_the_tone_of_a_flat_grey() -> Result<(), Error> {
    // whole tiles (blue noise is 64 x 64) and a wrapping noise: a shift only permutes the thresholds
    let picture = flat(0.3, 64, 64);
    for method in [
        Method::Bayer,
        Method::BlueNoise,
        Method::Clustered45,
        Method::WhiteNoise,
    ] {
        let plain = lit(&run(&ink(method, 0.0), &picture)?);
        for phase in [0.1, 0.5, 0.9] {
            assert_eq!(
                lit(&run(&ink(method, phase), &picture)?),
                plain,
                "{method:?} at {phase}"
            );
        }
    }
    Ok(())
}

#[test]
fn the_error_diffusion_methods_ignore_it() -> Result<(), Error> {
    let picture = flat(0.3, 16, 16);
    for method in [
        Method::FloydSteinberg,
        Method::Atkinson,
        Method::DotDiffusion,
        Method::Riemersma,
    ] {
        assert_eq!(
            run(&ink(method, 0.5), &picture)?,
            run(&ink(method, 0.0), &picture)?,
            "{method:?}"
        );
    }
    Ok(())
}

#[test]
fn spot_inks_shift_with_it() -> Result<(), Error> {
    let at = |phase| Params {
        separation: Separation::Spot,
        ..ink(Method::Bayer, phase)
    };
    let picture = flat(0.4, 16, 16);
    assert!(is_moved(
        &run(&at(0.0), &picture)?,
        &run(&at(0.375), &picture)?,
        (16, 16),
        (3, 3)
    ));
    Ok(())
}

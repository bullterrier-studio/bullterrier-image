//! `paper_texture`: what the texture adds to a grey picture, judged against hand-worked values (a pure weave is a sum of two
//! cosines) and statistics at a fixed seed: its deviation is `amount`, its fibres are as coarse as `scale` says, a weave
//! repeats every `spacing`, and `fibre` is the share of the variance that is not weave.

use bullterrier_image::Error;
use bullterrier_image::effects::paper_texture::{Kind, Params};

use crate::common::compare::first_bit_difference;
use crate::common::numbers::float;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;
use crate::common::stats::{correlation, deviation, mean, neighbour_correlation};

const KINDS: [Kind; 3] = [Kind::Paper, Kind::Canvas, Kind::Linen];

/// What the texture adds to a mid-grey `size` x `size` picture (channel 0), in the grey's units: a small amount never clips.
fn texture(params: &Params, size: usize) -> Result<Vec<f64>, Error> {
    let out = run_picture(params, size, size, &vec![0.5; size * size * 3])?;
    Ok(out.iter().step_by(3).map(|value| f64::from(value - 0.5)).collect())
}

/// The correlation of each value of a `size` square field with the one `lag` columns to its right.
fn autocorrelation(field: &[f64], size: usize, lag: usize) -> f64 {
    let pairs =
        (0..size).flat_map(|row| (0..size - lag).map(move |column| (row * size + column, row * size + column + lag)));
    let (here, there): (Vec<f64>, Vec<f64>) = pairs.map(|(a, b)| (field[a], field[b])).unzip();
    correlation(&here, &there)
}

#[test]
fn amount_zero_leaves_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(9, 11);
    for kind in KINDS {
        let out = run_picture(
            &Params {
                kind,
                amount: 0.0,
                ..Params::default()
            },
            9,
            11,
            &picture,
        )?;
        assert_eq!(first_bit_difference(&out, &picture), None, "{kind:?}");
    }
    Ok(())
}

#[test]
fn amount_is_the_relative_deviation_and_the_mean_brightness_is_kept() -> Result<(), Error> {
    // on a 0.5 grey the texture multiplies by 1 + amount x t, t of zero mean and unit deviation: deviation 0.5 x amount. The
    // unit deviation is that of how the texture is made, not of one picture, so it is found over many seeds (the weave of
    // a 'canvas' does not change with the seed, but its 16 periods across 128 px have the deviation of a cosine pair)
    for kind in KINDS {
        for amount in [0.05, 0.1, 0.2] {
            let mut added = Vec::new();
            for seed in 0..24 {
                added.extend(texture(
                    &Params {
                        kind,
                        amount,
                        scale: 8.0,
                        spacing: 8.0,
                        seed,
                        ..Params::default()
                    },
                    128,
                )?);
            }
            let unit = 0.5 * f64::from(amount);
            assert!((deviation(&added) / unit - 1.0).abs() < 0.08, "{kind:?} at {amount}");
            assert!(mean(&added).abs() < 0.04 * unit, "{kind:?} at {amount}");
        }
    }
    Ok(())
}

#[test]
fn the_fibres_have_unit_deviation_at_every_pixel_wherever_it_lies_in_a_lattice_cell() -> Result<(), Error> {
    // lattice points 4 px apart: the 16 pixels of a cell see every phase, and a pixel on a lattice point is its lattice value
    // (variance 1/3), one between them a blend of four (less); each is divided by its own deviation, so each has variance 1
    // over the seeds
    for octaves in [1, 3] {
        let fields: Vec<Vec<f64>> = (0..400)
            .map(|seed| {
                texture(
                    &Params {
                        amount: 0.02,
                        scale: 4.0,
                        octaves,
                        seed,
                        ..Params::default()
                    },
                    8,
                )
            })
            .collect::<Result<_, _>>()?;
        for (row, column) in [(0, 0), (0, 1), (1, 1), (2, 0), (2, 2), (3, 1), (5, 6), (7, 7)] {
            let seen: Vec<f64> = fields
                .iter()
                .map(|field| field[row * 8 + column] / (0.5 * 0.02))
                .collect();
            assert!(
                (deviation(&seen) - 1.0).abs() < 0.12 && mean(&seen).abs() < 0.12,
                "{octaves} octaves, pixel ({row}, {column}): deviation {}, mean {}",
                deviation(&seen),
                mean(&seen)
            );
        }
    }
    Ok(())
}

#[test]
fn the_texture_multiplies_every_channel_alike_and_leaves_black_black() -> Result<(), Error> {
    // dark enough that 1 + 0.1 x texture never reaches white
    let colour: Vec<f32> = [0.1, 0.2, 0.4].repeat(64 * 64);
    let out = run_picture(&Params::default(), 64, 64, &colour)?;
    let ratios: Vec<[f64; 3]> = colour
        .as_chunks::<3>()
        .0
        .iter()
        .zip(out.as_chunks::<3>().0)
        .map(|(before, after)| [0, 1, 2].map(|channel| f64::from(after[channel] / before[channel])))
        .collect();
    assert!(
        ratios
            .iter()
            .all(|r| (r[0] - r[1]).abs() < 1e-5 && (r[0] - r[2]).abs() < 1e-5),
        "one texture, every channel"
    );
    let first: Vec<f64> = ratios.iter().map(|r| r[0]).collect();
    assert!(deviation(&first) > 0.05);
    let black = run_picture(
        &Params {
            amount: 1.0,
            ..Params::default()
        },
        16,
        16,
        &vec![0.0; 16 * 16 * 3],
    )?;
    assert!(black.iter().all(|level| *level == 0.0));
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_texture_and_another_seed_another() -> Result<(), Error> {
    let picture = varied(40, 40);
    for kind in KINDS {
        let run = |seed| {
            run_picture(
                &Params {
                    kind,
                    amount: 0.2,
                    seed,
                    ..Params::default()
                },
                40,
                40,
                &picture,
            )
        };
        assert_eq!(first_bit_difference(&run(1)?, &run(1)?), None, "{kind:?}");
        assert!(first_bit_difference(&run(1)?, &run(2)?).is_some(), "{kind:?}");
        assert!(
            first_bit_difference(&run(u32::MAX)?, &run(u32::MAX - 1)?).is_some(),
            "{kind:?}"
        );
    }
    Ok(())
}

#[test]
fn a_larger_scale_correlates_paper_over_a_longer_distance() -> Result<(), Error> {
    let at = |scale| -> Result<f64, Error> {
        let added = texture(
            &Params {
                scale,
                octaves: 1,
                seed: 7,
                ..Params::default()
            },
            192,
        )?;
        Ok(autocorrelation(&added, 192, 4))
    };
    let (fine, medium, coarse) = (at(2.0)?, at(8.0)?, at(32.0)?);
    assert!(
        fine < 0.5 && 0.5 < medium && medium < coarse,
        "{fine} {medium} {coarse}"
    );
    assert!(coarse > 0.9, "{coarse}");
    Ok(())
}

#[test]
fn a_canvas_without_fibre_is_the_sum_of_two_cosines_of_the_thread_pitch() -> Result<(), Error> {
    // two cosines of period 8 over 64 px have zero mean and variance 1/2 each, so their sum already has unit deviation and
    // the texture is cos(2 pi row / 8) + cos(2 pi column / 8) exactly: the grey 0.5 becomes 0.5 x (1 + 0.2 x that)
    let params = Params {
        kind: Kind::Canvas,
        amount: 0.2,
        spacing: 8.0,
        fibre: 0.0,
        ..Params::default()
    };
    let added = texture(&params, 64)?;
    for (row, column) in [(0, 0), (1, 5), (2, 2), (7, 3), (33, 60), (63, 63)] {
        let wave = |at: usize| (std::f64::consts::TAU * float(at) / 8.0).cos();
        let expected = 0.5 * 0.2 * (wave(row) + wave(column));
        assert!((added[row * 64 + column] - expected).abs() < 1e-5, "({row}, {column})");
    }
    Ok(())
}

#[test]
fn a_linen_thread_has_one_strength_over_its_whole_pitch_and_threads_differ() -> Result<(), Error> {
    // linen without fibre: a thread's wave cos(2 pi x / spacing) is scaled as a whole, so columns 1 and pitch - 1 of one
    // pitch (equal cosines) agree, while column pitch - 1 and the next pitch's column 1 (equal cosines) differ
    for (pitch, spacing) in [(8_usize, 8.0_f32), (12, 12.0)] {
        let size = pitch * 6;
        let params = Params {
            kind: Kind::Linen,
            amount: 0.2,
            spacing,
            fibre: 0.0,
            seed: 1,
            ..Params::default()
        };
        let added = texture(&params, size)?;
        let on_row_0 = |column: usize| added[column];
        for start in (0..pitch * 5).step_by(pitch) {
            // the row-0 value is cos(row) x 1 + the column wave: only the column wave moves along the row
            assert!(
                (on_row_0(start + 1) - on_row_0(start + pitch - 1)).abs() < 1e-5,
                "pitch {pitch}, thread at {start}"
            );
        }
        let across: Vec<f64> = (1..5)
            .map(|thread| (on_row_0(thread * pitch - 1) - on_row_0(thread * pitch + 1)).abs())
            .collect();
        assert!(across.iter().all(|gap| *gap > 1e-3), "pitch {pitch}: {across:?}");
    }
    Ok(())
}

#[test]
fn fibre_is_the_share_of_the_texture_variance_that_is_not_weave() -> Result<(), Error> {
    // the weave and the fibres are independent, so the texture's correlation with the pure weave is sqrt(1 - fibre)
    for kind in [Kind::Canvas, Kind::Linen] {
        let base = Params {
            kind,
            amount: 0.2,
            spacing: 8.0,
            fibre: 0.0,
            seed: 4,
            ..Params::default()
        };
        let pure = texture(&base, 192)?;
        for fibre in [0.25, 0.75] {
            let mixed = texture(&Params { fibre, ..base.clone() }, 192)?;
            assert!(
                (correlation(&pure, &mixed) - f64::from(1.0 - fibre).sqrt()).abs() < 0.05,
                "{kind:?} at {fibre}"
            );
        }
    }
    Ok(())
}

#[test]
fn the_fibre_layers_are_set_by_octaves_persistence_and_lacunarity() -> Result<(), Error> {
    let base = Params {
        amount: 0.1,
        scale: 24.0,
        octaves: 3,
        persistence: 0.8,
        lacunarity: 2.0,
        seed: 1,
        ..Params::default()
    };
    // (smoother, rougher): more octaves add fine detail, more persistence keeps the fine layers strong, more lacunarity
    // makes them finer
    let rows = [
        (
            Params {
                octaves: 1,
                ..base.clone()
            },
            Params {
                octaves: 4,
                ..base.clone()
            },
        ),
        (
            Params {
                persistence: 0.1,
                ..base.clone()
            },
            Params {
                persistence: 0.9,
                ..base.clone()
            },
        ),
        (
            Params {
                lacunarity: 1.2,
                ..base.clone()
            },
            Params {
                lacunarity: 3.0,
                ..base.clone()
            },
        ),
    ];
    for (row, (smoother, rougher)) in rows.iter().enumerate() {
        let smooth = autocorrelation(&texture(smoother, 192)?, 192, 1);
        let rough = autocorrelation(&texture(rougher, 192)?, 192, 1);
        assert!(rough < smooth - 0.01, "row {row}: {rough} vs {smooth}");
    }
    Ok(())
}

#[test]
fn all_fibre_makes_a_weave_the_paper_texture_and_paper_ignores_the_thread_pitch() -> Result<(), Error> {
    let paper = texture(
        &Params {
            amount: 0.2,
            seed: 6,
            ..Params::default()
        },
        64,
    )?;
    let canvas = texture(
        &Params {
            kind: Kind::Canvas,
            amount: 0.2,
            fibre: 1.0,
            seed: 6,
            ..Params::default()
        },
        64,
    )?;
    assert!(paper.iter().zip(&canvas).all(|(a, b)| (a - b).abs() < 1e-5));
    let spaced = texture(
        &Params {
            amount: 0.2,
            spacing: 20.0,
            seed: 6,
            ..Params::default()
        },
        64,
    )?;
    assert_eq!(paper, spaced);
    let woven = texture(
        &Params {
            kind: Kind::Canvas,
            amount: 0.2,
            seed: 6,
            ..Params::default()
        },
        64,
    )?;
    assert!((neighbour_correlation(&paper, 64) - neighbour_correlation(&woven, 64)).abs() > 0.01);
    Ok(())
}

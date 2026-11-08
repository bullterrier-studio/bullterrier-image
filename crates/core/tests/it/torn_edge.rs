//! `torn_edge`: which pixels of a grey picture the tear replaces by a colour found nowhere in it, read off the output. The
//! oracles are a straight cut worked out by hand (roughness 0) and the statistics the documentation promises: the tear
//! lies `width` px in on average and wanders by `roughness` x `width`, one line to a side, a side of its own each.

use bullterrier_image::Error;
use bullterrier_image::effects::torn_edge::Params;
use bullterrier_image::params::Color;

use crate::common::compare::{first_bit_difference, first_difference};
use crate::common::numbers::float;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;
use crate::common::stats::{correlation, deviation, mean};

const MAGENTA: [f32; 3] = [1.0, 0.0, 1.0]; // nothing in a grey picture can be mistaken for it

/// Which pixels of a grey `height` x `width` picture `params` tore away (replaced by magenta), row after row.
fn torn(params: &Params, height: usize, width: usize) -> Result<Vec<bool>, Error> {
    let params = Params {
        fill: Color(MAGENTA),
        ..params.clone()
    };
    let out = run_picture(&params, height, width, &vec![0.5; height * width * 3])?;
    Ok(out.as_chunks::<3>().0.iter().map(|pixel| *pixel == MAGENTA).collect())
}

/// In each column from `first` to `last` (exclusive), how many torn pixels there are from the top down to the first kept one.
fn run_from_top(torn: &[bool], width: usize, first: usize, last: usize) -> Vec<f64> {
    (first..last)
        .map(|column| {
            float(
                torn.iter()
                    .skip(column)
                    .step_by(width)
                    .take_while(|gone| **gone)
                    .count(),
            )
        })
        .collect()
}

/// The mask turned a quarter, so that another side is the top: `side` 0 top, 1 bottom, 2 left, 3 right.
fn from_side(torn: &[bool], height: usize, width: usize, side: usize) -> (Vec<bool>, usize) {
    let at = |row: usize, column: usize| torn[row * width + column];
    match side {
        0 => (torn.to_vec(), width),
        1 => (
            (0..height)
                .flat_map(|row| (0..width).map(move |column| (height - 1 - row, column)))
                .map(|(row, column)| at(row, column))
                .collect(),
            width,
        ),
        2 => (
            (0..width)
                .flat_map(|row| (0..height).map(move |column| (column, row)))
                .map(|(row, column)| at(row, column))
                .collect(),
            height,
        ),
        _ => (
            (0..width)
                .flat_map(|row| (0..height).map(move |column| (column, width - 1 - row)))
                .map(|(row, column)| at(row, column))
                .collect(),
            height,
        ),
    }
}

#[test]
fn roughness_zero_gives_a_straight_edge_of_exactly_the_width() -> Result<(), Error> {
    // pixels closer than `width` px to a side are torn away: 8 rows and columns for a width of 7.5 and for one of 8 (a pixel
    // exactly `width` in is kept), 1 for a width of 0.5
    for (width, rows) in [(7.5, 8_usize), (8.0, 8), (0.5, 1)] {
        let mask = torn(
            &Params {
                width,
                roughness: 0.0,
                ..Params::default()
            },
            200,
            190,
        )?;
        for (at, gone) in mask.iter().enumerate() {
            let (row, column) = (at / 190, at % 190);
            let inside = (rows..200 - rows).contains(&row) && (rows..190 - rows).contains(&column);
            assert_eq!(*gone, !inside, "width {width}, ({row}, {column})");
        }
    }
    Ok(())
}

#[test]
fn width_zero_tears_nothing() -> Result<(), Error> {
    let picture = varied(30, 40);
    let out = run_picture(
        &Params {
            width: 0.0,
            roughness: 1.0,
            seed: 3,
            ..Params::default()
        },
        30,
        40,
        &picture,
    )?;
    assert_eq!(first_bit_difference(&out, &picture), None);
    Ok(())
}

#[test]
fn every_pixel_is_untouched_or_exactly_the_fill_and_the_middle_is_whole() -> Result<(), Error> {
    let picture = varied(200, 200);
    let fill = [0.2, 0.4, 0.6];
    let params = Params {
        width: 12.0,
        roughness: 0.5,
        scale: 10.0,
        fill: Color(fill),
        seed: 2,
        ..Params::default()
    };
    let out = run_picture(&params, 200, 200, &picture)?;
    let (mut replaced, mut kept) = (0, 0);
    for (at, (after, before)) in out
        .as_chunks::<3>()
        .0
        .iter()
        .zip(picture.as_chunks::<3>().0)
        .enumerate()
    {
        if after.iter().zip(before).all(|(a, b)| a.to_bits() == b.to_bits()) {
            kept += 1;
        } else {
            assert_eq!(*after, fill, "pixel {at}");
            replaced += 1;
        }
    }
    assert!(replaced > 0 && kept > 0);
    assert_eq!(out.as_chunks::<3>().0[0], fill); // the corner is torn
    for row in 40..160 {
        let span = (row * 200 + 40) * 3..(row * 200 + 160) * 3;
        assert_eq!(
            first_bit_difference(&out[span.clone()], &picture[span]),
            None,
            "row {row}"
        ); // a tear never reaches 40 px here
    }
    Ok(())
}

#[test]
fn the_tear_is_one_line_per_side_with_nothing_torn_off_inside_it() -> Result<(), Error> {
    // going down a column from the top the pixels are removed up to the tear and kept after it, never alternating
    let mask = torn(
        &Params {
            width: 25.0,
            roughness: 0.8,
            scale: 8.0,
            seed: 4,
            ..Params::default()
        },
        400,
        400,
    )?;
    for column in 120..280 {
        let mut kept_seen = false;
        for row in 0..200 {
            let gone = mask[row * 400 + column];
            assert!(
                !(gone && kept_seen),
                "column {column}: a torn pixel below a kept one at row {row}"
            );
            kept_seen |= !gone;
        }
    }
    Ok(())
}

#[test]
fn the_tear_wanders_about_the_width_by_the_roughness_times_the_width() -> Result<(), Error> {
    // the distance from the side has mean `width` and deviation `roughness` x `width` (while it stays positive); measured
    // over the columns of a long top side that no other side's tear reaches, with the ~0.3 px a whole-pixel count adds
    for roughness in [0.1, 0.2, 0.4] {
        let params = Params {
            width: 40.0,
            roughness,
            scale: 12.0,
            seed: 1,
            ..Params::default()
        };
        let mask = torn(&params, 300, 3000)?;
        let distance = run_from_top(&mask, 3000, 300, 2700);
        assert!(
            (mean(&distance) / 40.0 - 1.0).abs() < 0.1,
            "roughness {roughness}: mean {}",
            mean(&distance)
        );
        assert!(
            (deviation(&distance) / (40.0 * f64::from(roughness)) - 1.0).abs() < 0.2,
            "roughness {roughness}: {}",
            deviation(&distance)
        );
    }
    Ok(())
}

#[test]
fn more_roughness_means_a_more_ragged_tear() -> Result<(), Error> {
    let spreads: Vec<f64> = [0.0, 0.1, 0.3, 0.6]
        .into_iter()
        .map(|roughness| {
            let mask = torn(
                &Params {
                    width: 40.0,
                    roughness,
                    seed: 1,
                    ..Params::default()
                },
                300,
                1500,
            )?;
            Ok(deviation(&run_from_top(&mask, 1500, 200, 1300)))
        })
        .collect::<Result<_, Error>>()?;
    assert!(
        spreads[0].abs() < f64::EPSILON && spreads.windows(2).all(|pair| pair[0] < pair[1]),
        "{spreads:?}"
    );
    Ok(())
}

#[test]
fn a_larger_scale_gives_broader_bays_and_a_smaller_one_a_jagged_edge() -> Result<(), Error> {
    let at = |scale| -> Result<f64, Error> {
        let params = Params {
            width: 40.0,
            roughness: 0.3,
            scale,
            octaves: 1,
            ..Params::default()
        };
        let tear = run_from_top(&torn(&params, 300, 1500)?, 1500, 200, 1300);
        Ok(correlation(&tear[..tear.len() - 1], &tear[1..]))
    };
    let (jagged, medium, broad) = (at(2.0)?, at(10.0)?, at(50.0)?);
    assert!(jagged < medium && medium < broad, "{jagged} {medium} {broad}");
    assert!(broad > 0.99, "{broad}");
    Ok(())
}

#[test]
fn the_layers_of_the_tear_are_set_by_octaves_persistence_and_lacunarity() -> Result<(), Error> {
    let base = Params {
        width: 40.0,
        roughness: 0.3,
        scale: 40.0,
        octaves: 3,
        persistence: 0.8,
        lacunarity: 2.0,
        seed: 1,
        ..Params::default()
    };
    // (smoother, rougher): more octaves add fine jaggedness, more persistence keeps the fine layers strong, more
    // lacunarity makes them finer
    let rows = [
        (
            Params {
                octaves: 1,
                ..base.clone()
            },
            Params {
                octaves: 5,
                persistence: 0.8,
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
    let jaggedness = |params: &Params| -> Result<f64, Error> {
        let tear = run_from_top(&torn(params, 300, 1500)?, 1500, 200, 1300);
        Ok(tear.windows(2).map(|pair| (pair[1] - pair[0]).abs()).sum::<f64>() / float(tear.len() - 1))
    };
    for (row, (smoother, rougher)) in rows.iter().enumerate() {
        assert!(jaggedness(rougher)? > 1.2 * jaggedness(smoother)?, "row {row}");
    }
    Ok(())
}

#[test]
fn each_side_has_its_own_tear() -> Result<(), Error> {
    let mask = torn(
        &Params {
            width: 30.0,
            roughness: 0.5,
            scale: 10.0,
            seed: 1,
            ..Params::default()
        },
        300,
        300,
    )?;
    let tears: Vec<Vec<f64>> = (0..4)
        .map(|side| {
            let (seen, width) = from_side(&mask, 300, 300, side);
            run_from_top(&seen, width, 60, 240)
        })
        .collect();
    for (one, other) in [(0, 1), (0, 2), (0, 3), (2, 3)] {
        assert!(
            correlation(&tears[one], &tears[other]).abs() < 0.5,
            "sides {one} and {other}"
        );
    }
    Ok(())
}

#[test]
fn the_same_seed_tears_the_same_line_and_another_seed_another() -> Result<(), Error> {
    let at = |seed| {
        torn(
            &Params {
                seed,
                ..Params::default()
            },
            120,
            160,
        )
    };
    assert!(first_difference(&at(1)?, &at(1)?).is_none());
    assert!(first_difference(&at(1)?, &at(2)?).is_some());
    assert!(first_difference(&at(u32::MAX)?, &at(u32::MAX - 1)?).is_some());
    Ok(())
}

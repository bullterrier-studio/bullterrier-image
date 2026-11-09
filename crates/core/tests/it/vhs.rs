//! `vhs`: a worn tape. Each kind of damage is switched on alone and judged by what it must do: bands only rotate rows,
//! the wobble's deviation in px is the one asked for, a dropout is a white rectangle, static fills a band with grey noise
//! of the known mean, bled colour is the input's colour moved and blurred (worked out from the definition) with
//! brightness kept, tape noise has the asked deviation and leaves colour alone, and the damage depends only on the seed.

#![expect(clippy::cast_precision_loss, reason = "small picture sizes")]

use bullterrier_image::effects::vhs::Params;
use bullterrier_image::{Edge, Error};

use crate::common::compare::first_bit_difference;
use crate::common::pictures::{coordinates, places, ramp, varied};
use crate::common::pixels::run_picture;
use crate::common::stats::{correlation, deviation, mean, neighbour_correlation};
use crate::common::yiq::{smoothed, yiq_of};

/// No damage at all: every kind of it at its minimum.
fn untouched() -> Params {
    Params {
        bands: 0,
        dropouts: 0,
        bleed: 0.0,
        noise: 0.0,
        wobble: 0.0,
        band_static: 0.0,
        ..Params::default()
    }
}

fn channel_of(picture: &[f32], channel: usize) -> Vec<f64> {
    picture.iter().skip(channel).step_by(3).map(|&level| f64::from(level)).collect()
}

#[test]
fn every_damage_at_its_minimum_leaves_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(9, 11);
    for edge in [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap] {
        let params = Params {
            edge,
            ..untouched()
        };
        let out = run_picture(&params, 9, 11, &picture)?;
        assert_eq!(first_bit_difference(&out, &picture), None, "{edge:?}");
    }
    Ok(())
}

#[test]
fn bands_only_rotate_the_rows_they_cover_by_no_more_than_their_tracking_together() -> Result<(), Error> {
    let (height, width) = (60, 41);
    let params = Params {
        bands: 3,
        tracking: 5.0,
        band_height: 10.0,
        edge: Edge::Wrap,
        seed: 4,
        ..untouched()
    };
    let out = run_picture(&params, height, width, &coordinates(height, width))?;
    let read = places(&out, height, width);
    let mut moved = 0;
    for row in 0..height {
        // the pixel now at column 0 was at column `first` of the same row, and the rest follow it round
        let first = read[row * width].1.round() as usize;
        for column in 0..width {
            let (from_row, from_column) = read[row * width + column];
            assert_eq!(from_row.round() as usize, row, "row {row} read from another row");
            assert_eq!(from_column.round() as usize, (first + column) % width, "row {row}, column {column}");
        }
        let slide = (width - first) % width;
        moved += usize::from(slide != 0);
        assert!(slide <= 15 || width - slide <= 15, "row {row} slid {slide} px, past 3 bands of 5");
    }
    assert!((1..=30).contains(&moved), "{moved} rows moved; 3 bands cover at most 30");
    Ok(())
}

#[test]
fn the_wobble_moves_rows_with_the_deviation_asked_for_and_slowly_when_its_size_is_large() -> Result<(), Error> {
    let side = 64;
    let ramp = ramp(side, side);
    // a linear ramp moved by d reads (x - d) / 63 at column x, so each row's drift is read off a column inside it
    let drifts = |wobble_size: f32| -> Result<Vec<f64>, Error> {
        let params = Params {
            wobble: 2.0,
            wobble_size,
            ..untouched()
        };
        let out = run_picture(&params, side, side, &ramp)?;
        Ok((0..side)
            .map(|row| 32.0 - 63.0 * f64::from(out[(row * side + 32) * 3]))
            .collect())
    };
    // a clumped wander has exactly mean 0 and the deviation asked; independent values (size up to 1) only about
    for (wobble_size, slack) in [(0.5, 0.6), (4.0, 2e-3), (8.0, 2e-3)] {
        let drift = drifts(wobble_size)?;
        assert!(mean(&drift).abs() < slack, "size {wobble_size}: mean {}", mean(&drift));
        assert!((deviation(&drift) - 2.0).abs() < slack, "size {wobble_size}: deviation {}", deviation(&drift));
    }
    let slow = drifts(8.0)?;
    assert!(correlation(&slow[1..], &slow[..side - 1]) > 0.6);
    Ok(())
}

#[test]
fn a_dropout_is_a_white_rectangle_as_tall_as_a_noise_speck_and_as_long_as_the_tracking_allows() -> Result<(), Error> {
    let (height, width) = (30, 40);
    let black = vec![0.0; height * width * 3];
    for seed in 0..8 {
        let params = Params {
            dropouts: 1,
            tracking: 12.0,
            noise_size: 2.0,
            seed,
            ..untouched()
        };
        let out = run_picture(&params, height, width, &black)?;
        let white: Vec<(usize, usize)> = (0..height * width)
            .filter(|&at| out[at * 3..at * 3 + 3] == [1.0; 3])
            .map(|at| (at / width, at % width))
            .collect();
        assert!(out.iter().all(|&level| level == 0.0 || level == 1.0), "seed {seed}");
        let (top, bottom) = (white[0].0, white[white.len() - 1].0);
        let (left, right) = (white[0].1, white[1.min(white.len() - 1)..].iter().map(|p| p.1).max().unwrap_or(0));
        let (rows, columns) = (bottom - top + 1, right - left + 1);
        assert_eq!(white.len(), rows * columns, "seed {seed}: not a rectangle");
        assert_eq!(rows, 2.min(height - top), "seed {seed}");
        assert!((6..=12).contains(&columns), "seed {seed}: {columns} px long");
    }
    let none = run_picture(&untouched(), height, width, &black)?;
    assert!(none.iter().all(|&level| level == 0.0));
    Ok(())
}

#[test]
fn static_fills_the_band_with_grey_noise_and_nothing_else_changes() -> Result<(), Error> {
    let side = 40;
    // (the level of the picture, the share of static, the mean and deviation of what a band holds): a share s of a uniform
    // grey of mean 1/2 and deviation 1/sqrt(12) on the level (1 - s)
    for (level, share, expected_mean, expected_deviation) in [(1.0_f32, 0.5_f32, 0.75, 0.144_3), (0.0, 1.0, 0.5, 0.288_7)] {
        let params = Params {
            bands: 3,
            tracking: 0.0,
            band_height: 10.0,
            band_static: share,
            ..untouched()
        };
        let out = run_picture(&params, side, side, &vec![level; side * side * 3])?;
        let rows: Vec<&[f32]> = out.chunks_exact(side * 3).collect();
        let covered: Vec<&&[f32]> = rows.iter().filter(|row| row.iter().any(|&v| v != level)).collect();
        assert!((5..=30).contains(&covered.len()), "level {level}: {} rows", covered.len());
        let grey: Vec<f64> = covered
            .iter()
            .flat_map(|row| {
                row.chunks_exact(3).map(|pixel| {
                    assert!(pixel[0] == pixel[1] && pixel[1] == pixel[2], "static is grey");
                    f64::from(pixel[0])
                })
            })
            .collect();
        assert!((mean(&grey) - expected_mean).abs() < 0.04, "level {level}: mean {}", mean(&grey));
        assert!((deviation(&grey) - expected_deviation).abs() < 0.03, "level {level}: deviation {}", deviation(&grey));
    }
    let nothing = Params {
        bands: 0,
        band_static: 1.0,
        ..untouched()
    };
    let picture = varied(9, 11);
    assert_eq!(first_bit_difference(&run_picture(&nothing, 9, 11, &picture)?, &picture), None);
    Ok(())
}

#[test]
fn bled_colour_is_the_colour_moved_right_and_blurred_while_brightness_stays() -> Result<(), Error> {
    let (rows, width) = (3, 40);
    let colour = [0.7, 0.45, 0.35];
    let picture: Vec<f32> = (0..rows * width)
        .flat_map(|at| if (12..20).contains(&(at % width)) { colour } else { [0.5; 3] })
        .collect();
    let before = yiq_of(&picture);
    for bleed in [4.0_f32, 2.5] {
        let params = Params {
            bleed,
            ..untouched()
        };
        let out = yiq_of(&run_picture(&params, rows, width, &picture)?);
        for axis in 1..3 {
            let line: Vec<f64> = (0..width).map(|column| before[column][axis]).collect();
            // moved right by `bleed`: column x holds the line at x - bleed, between its two nearest pixels
            let last = width as i64 - 1;
            let at = |index: i64| line[index.clamp(0, last) as usize];
            let moved: Vec<f64> = (0..width as i64)
                .map(|x| {
                    let place = x as f64 - f64::from(bleed);
                    let below = place.floor();
                    let t = place - below;
                    (1.0 - t) * at(below as i64) + t * at(below as i64 + 1)
                })
                .collect();
            let expected = smoothed(&moved, f64::from(bleed), 4.0, Edge::Nearest);
            for row in 0..rows {
                for column in 0..width {
                    let got = out[row * width + column];
                    assert!((got[axis] - expected[column]).abs() < 3e-5, "bleed {bleed}, axis {axis}, column {column}");
                    assert!((got[0] - before[row * width + column][0]).abs() < 3e-5, "brightness at column {column}");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn tape_noise_has_the_deviation_asked_clumps_with_its_size_and_leaves_colour_alone() -> Result<(), Error> {
    let side = 40;
    let grey = vec![0.5_f32; side * side * 3];
    // (the speck's width, whether it is clumped): clumped noise has exactly mean 0 and the deviation asked, single pixels only about
    for (noise_size, clumped) in [(1.0, false), (6.0, true)] {
        let params = Params {
            noise: 0.05,
            noise_size,
            ..untouched()
        };
        let out = channel_of(&run_picture(&params, side, side, &grey)?, 0);
        assert!((mean(&out) - 0.5).abs() < if clumped { 1e-3 } else { 6e-3 }, "size {noise_size}: mean {}", mean(&out));
        if clumped {
            assert!((deviation(&out) - 0.05).abs() < 1e-3, "size {noise_size}: deviation {}", deviation(&out));
            assert!(neighbour_correlation(&out, side) > 0.8);
        } else {
            assert!((deviation(&out) - 0.05).abs() < 5e-3, "size {noise_size}: deviation {}", deviation(&out));
            assert!(neighbour_correlation(&out, side).abs() < 0.15);
        }
    }
    let coloured = [0.6_f32, 0.4, 0.3].repeat(side * side);
    let params = Params {
        noise: 0.02,
        ..untouched()
    };
    let out = yiq_of(&run_picture(&params, side, side, &coloured)?);
    let want = yiq_of(&coloured[..3]);
    for (at, pixel) in out.iter().enumerate() {
        assert!((pixel[1] - want[0][1]).abs() < 3e-5 && (pixel[2] - want[0][2]).abs() < 3e-5, "colour at {at}");
    }
    let brightness: Vec<f64> = out.iter().map(|pixel| pixel[0]).collect();
    assert!((deviation(&brightness) - 0.02).abs() < 1e-3);
    Ok(())
}

#[test]
fn the_damage_depends_on_the_seed_alone_and_changing_the_bands_moves_no_other_damage() -> Result<(), Error> {
    let picture = varied(20, 30);
    // tracking 0: the bands exist but slide nothing, so any other change they caused would be a shared random stream
    let worn = |bands, seed| {
        run_picture(
            &Params {
                bands,
                tracking: 0.0,
                dropouts: 4,
                noise: 0.05,
                wobble: 2.0,
                bleed: 3.0,
                seed,
                ..Params::default()
            },
            20,
            30,
            &picture,
        )
    };
    assert_eq!(first_bit_difference(&worn(0, 7)?, &worn(3, 7)?), None);
    assert_eq!(first_bit_difference(&worn(3, 7)?, &worn(3, 7)?), None);
    assert!(first_bit_difference(&worn(3, 7)?, &worn(3, 8)?).is_some());
    Ok(())
}

//! `modulation`: rows of dashes. The oracles are a hand-worked dash pattern (error diffusion of a tone of 0.4 gives
//! off, on, off, on, off), the tone a flat picture must keep within the half dash a row's error can be left over, the
//! transpose a quarter turn must give, and the whole render for rows told their place.

#![expect(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::chunks_exact_to_as_chunks,
    reason = "small pixel counts and coordinates; paper is exactly 1.0; a picture's channels are read three at a time"
)]

use bullterrier_image::Error;
use bullterrier_image::effects::modulation::Params;
use bullterrier_image::params::Color;

use crate::common::pixels::{run_picture, run_rows};

/// A grey picture of `height` x `width`.
fn flat(height: usize, width: usize, level: f32) -> Vec<f32> {
    vec![level; height * width * 3]
}

/// The red channel of the render, row after row.
fn red(params: &Params, height: usize, width: usize, level: f32) -> Result<Vec<f32>, Error> {
    Ok(run_picture(params, height, width, &flat(height, width, level))?
        .chunks_exact(3)
        .map(|pixel| pixel[0])
        .collect())
}

/// Which cells of a row are inked, read at the middle of each: black ink on white paper.
fn inked_cells(out: &[f32], width: usize, row: usize, dash: usize) -> Vec<bool> {
    (0..width / dash)
        .map(|cell| out[row * width + cell * dash + dash / 2] < 0.5)
        .collect()
}

#[test]
fn a_tone_of_two_fifths_of_the_dash_inks_the_second_and_fourth_of_every_five_cells() -> Result<(), Error> {
    // luminance 0.8 on white paper with black ink is a darkness of 0.2, which is 0.4 of a dash of thickness 0.5; the carry
    // runs 0.4 off (0.4), 0.8 on (-0.2), 0.2 off (0.2), 0.6 on (-0.4), 0.0 off, and again
    let out = red(&Params::default(), 16, 40, 0.8)?;
    let want: Vec<bool> = (0..10).map(|cell| cell % 5 == 1 || cell % 5 == 3).collect();
    // the first row of rows is centred on row 4 and is 4 px wide: rows 2 to 5; the next on 12: rows 10 to 13
    for row in [2, 3, 4, 5, 10, 13] {
        assert_eq!(inked_cells(&out, 40, row, 4), want, "row {row}");
    }
    for row in [0, 1, 6, 7, 8, 9, 14, 15] {
        assert!(
            out[row * 40..(row + 1) * 40].iter().all(|&value| value == 1.0),
            "row {row} is paper"
        );
    }
    // ink is the colour, whole: a dash's end pixels are exact, not blended
    assert!(out[3 * 40 + 4..3 * 40 + 8].iter().all(|&value| value == 0.0));
    Ok(())
}

#[test]
fn a_flat_tone_keeps_its_mean_ink_within_the_half_dash_a_row_can_be_left_over() -> Result<(), Error> {
    // 8 rows of 20 cells: a row's carry ends within a half cell, so a row has n on cells with |n - 20 f| <= 0.5, and its ink
    // is n / 20 x thickness: at most 0.5 / 20 x 0.5 from the tone
    let params = Params::default();
    for level in [0.9, 0.8, 0.65, 0.55] {
        let out = red(&params, 64, 80, level)?;
        let ink = 1.0 - out.iter().sum::<f32>() / 5120.0;
        let tone = 1.0 - level;
        assert!(
            (ink - tone).abs() <= 0.5 / 20.0 * 0.5 + 1e-4,
            "luminance {level}: ink {ink}, tone {tone}"
        );
    }
    // darker than the thickness prints at the thickness
    let out = red(&params, 64, 80, 0.1)?;
    let ink = 1.0 - out.iter().sum::<f32>() / 5120.0;
    assert!((ink - 0.5).abs() < 1e-4, "{ink}");
    Ok(())
}

#[test]
fn ink_and_paper_are_the_colours_asked_for_and_tone_runs_from_paper_to_ink() -> Result<(), Error> {
    // light ink on dark paper: a bright picture is the tone that wants ink
    let params = Params {
        color: Color([1.0, 0.5, 0.0]),
        paper: Color([0.0; 3]),
        ..Params::default()
    };
    let out = run_picture(&params, 16, 40, &flat(16, 40, 0.0))?;
    assert!(out.iter().all(|&value| value == 0.0), "paper only");
    let out = run_picture(&params, 16, 40, &flat(16, 40, 1.0))?;
    assert_eq!(
        &out[(3 * 40 + 5) * 3..(3 * 40 + 5) * 3 + 3],
        &[1.0, 0.5, 0.0],
        "ink on a full row"
    );
    Ok(())
}

#[test]
fn weave_runs_every_second_row_the_other_way_and_leaves_the_rest() -> Result<(), Error> {
    // 12 cells of a pattern that is not its own mirror image (off on off on off off on off on off off on)
    let (plain, woven) = (
        Params::default(),
        Params {
            weave: true,
            ..Params::default()
        },
    );
    let (a, b) = (red(&plain, 32, 48, 0.8)?, red(&woven, 32, 48, 0.8)?);
    let mirror = |cells: Vec<bool>| cells.into_iter().rev().collect::<Vec<_>>();
    for row in [2, 5, 18, 21] {
        assert_eq!(
            inked_cells(&a, 48, row, 4),
            inked_cells(&b, 48, row, 4),
            "row {row} runs as it did"
        );
    }
    for row in [10, 13, 26, 29] {
        assert_ne!(
            inked_cells(&a, 48, row, 4),
            inked_cells(&b, 48, row, 4),
            "row {row} turns"
        );
        assert_eq!(
            mirror(inked_cells(&a, 48, row, 4)),
            inked_cells(&b, 48, row, 4),
            "row {row}"
        );
    }
    Ok(())
}

#[test]
fn a_quarter_turn_gives_the_transpose() -> Result<(), Error> {
    let size = 48;
    let straight = red(&Params::default(), size, size, 0.8)?;
    let turned = red(
        &Params {
            angle: 90.0,
            ..Params::default()
        },
        size,
        size,
        0.8,
    )?;
    for row in 0..size {
        for column in 0..size {
            let (a, b) = (turned[row * size + column], straight[column * size + row]);
            assert!((a - b).abs() < 1e-4, "({row}, {column}): {a} against {b}");
        }
    }
    Ok(())
}

#[test]
fn ink_lies_on_the_rows_lines_even_when_they_turn_and_sway() -> Result<(), Error> {
    let params = Params {
        angle: 30.0,
        sway: 3.0,
        wavelength: 40.0,
        ..Params::default()
    };
    let (height, width) = (80, 96);
    let out = red(&params, height, width, 0.6)?;
    let turn = 30.0_f64.to_radians();
    let mut ink = 0;
    for row in 0..height {
        for column in 0..width {
            if out[row * width + column] >= 1.0 {
                continue;
            }
            ink += 1;
            let (x, y) = (column as f64 + 0.5, row as f64 + 0.5);
            let (u, v) = (x * turn.cos() + y * turn.sin(), y * turn.cos() - x * turn.sin());
            let bent = v - 3.0 * (u / 40.0 * std::f64::consts::TAU).sin();
            // the distance to the nearest line of the row lattice, centres at (k + 1/2) x 8: a band of 4 and its one px ramp
            let off = (bent / 8.0 - (bent / 8.0).floor() - 0.5).abs() * 8.0;
            assert!(
                off <= 2.0 + 0.5 + 1e-6,
                "({row}, {column}) is {off} px from its row's line"
            );
        }
    }
    assert!(ink > 300, "{ink} inked pixels");
    Ok(())
}

#[test]
fn rows_told_their_place_are_the_rows_of_the_whole_render() -> Result<(), Error> {
    // a flat picture reads the same anywhere, so only the canvas place can tell the rows apart
    let params = Params {
        angle: 20.0,
        weave: true,
        ..Params::default()
    };
    let (height, width) = (48, 40);
    let values = flat(height, width, 0.7);
    let whole = run_picture(&params, height, width, &values)?;
    let band = run_rows(&params, (height, width), &values, (16, 32), true)?;
    let want = &whole[16 * width * 3..32 * width * 3];
    let at = band.iter().zip(want).position(|(a, b)| (a - b).abs() > 1e-6);
    assert_eq!(at, None, "placed");
    let alone = run_rows(&params, (height, width), &values, (16, 32), false)?;
    assert!(
        alone.iter().zip(want).any(|(a, b)| (a - b).abs() > 1e-6),
        "unplaced rows differ"
    );
    Ok(())
}

#[test]
fn rows_as_wide_as_their_spacing_cover_the_picture_where_it_is_dark() -> Result<(), Error> {
    // at thickness 1 neighbouring rows meet at a pixel row of two half ramps (pixel row 6 of a spacing of 6.75 is
    // 0.75 inside one row and 0.25 inside the next), which add up to ink
    let params = Params {
        thickness: 1.0,
        spacing: 6.75,
        ..Params::default()
    };
    let out = red(&params, 32, 32, 0.0)?;
    assert!(out.iter().all(|&value| value < 1e-6), "a black picture is all ink");
    Ok(())
}

#[test]
fn a_cell_reads_the_picture_where_it_lies_on_a_turned_row() -> Result<(), Error> {
    // black left, white right, rows turned 40 degrees: ink stays left of the border and paper right of it, a few
    // cells of slack where a cell straddles it
    let (height, width) = (96, 96);
    let values: Vec<f32> = (0..height * width * 3)
        .map(|i| if i / 3 % width < width / 2 { 0.0 } else { 1.0 })
        .collect();
    let out: Vec<f32> = run_picture(
        &Params {
            angle: 40.0,
            thickness: 1.0,
            ..Params::default()
        },
        height,
        width,
        &values,
    )?
    .chunks_exact(3)
    .map(|pixel| pixel[0])
    .collect();
    let mean = |from: usize, to: usize| {
        let cells: Vec<f32> = (0..height)
            .flat_map(|row| out[row * width + from..row * width + to].to_vec())
            .collect();
        cells.iter().sum::<f32>() / cells.len() as f32
    };
    assert!(mean(0, 36) < 0.05, "left {}", mean(0, 36));
    assert!(mean(60, 96) > 0.95, "right {}", mean(60, 96));
    Ok(())
}

#[test]
fn a_dash_that_does_not_fall_on_whole_pixels_still_keeps_the_tone() -> Result<(), Error> {
    // cells of 3.5 px: 20 of them across 70 px; the ramp at a dash's end moves ink, never makes or loses any
    let out = red(
        &Params {
            dash: 3.5,
            ..Params::default()
        },
        64,
        70,
        0.8,
    )?;
    let ink = 1.0 - out.iter().sum::<f32>() / out.len() as f32;
    assert!((ink - 0.2).abs() <= 0.5 / 20.0 * 0.5 + 1e-3, "{ink}");
    Ok(())
}

#[test]
fn a_dash_ends_in_a_ramp_one_pixel_wide_across_its_end() -> Result<(), Error> {
    // cells of 3.4 px, the pattern off, on, off, on, off: the second cell spans 3.4 to 6.8. The pixel centred at 3.5 is
    // 0.1 into it: the ramp is 0.5 - 0.1 = 0.4 of the way from the empty cell before it; the pixel centred at 6.5 is 0.3
    // from its end: 0.5 - 0.3 = 0.2 of the way to the empty cell after it
    let out = red(
        &Params {
            dash: 3.4,
            ..Params::default()
        },
        16,
        40,
        0.8,
    )?;
    let near = |pixel: usize, want: f32| {
        let got = out[3 * 40 + pixel];
        assert!((got - want).abs() < 1e-4, "pixel {pixel}: {got} against {want}");
    };
    near(3, 0.4);
    near(4, 0.0);
    near(6, 0.2);
    near(7, 1.0);
    Ok(())
}

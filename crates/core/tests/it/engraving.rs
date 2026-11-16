//! `engraving`: lines whose width is the tone and whose course follows the light. The oracles are hand-worked pixel rows
//! (a 2 px line on an 8 px pitch fills two pixel rows; a bend moves a flat picture's lines by bend x luminance), the tone a
//! flat picture must keep on a pixel lattice, a brute-force sampling of the definition for lines bowed row by row, and the
//! whole render for lines told their place.

#![expect(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    reason = "small pixel counts; paper and ink are exact values"
)]

use bullterrier_image::Error;
use bullterrier_image::effects::engraving::Params;
use bullterrier_image::params::Color;

use crate::common::pixels::{run_picture, run_rows};

fn flat(height: usize, width: usize, level: f32) -> Vec<f32> {
    vec![level; height * width * 3]
}

fn channel(values: &[f32]) -> Vec<f32> {
    values.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect()
}

/// The red channel of the render of `values`.
fn red(params: &Params, height: usize, width: usize, values: &[f32]) -> Result<Vec<f32>, Error> {
    Ok(channel(&run_picture(params, height, width, values)?))
}

/// Black lines on white paper, 8 px apart, along the x axis, not bent.
fn straight() -> Params {
    Params {
        angle: 0.0,
        spacing: 8.0,
        bend: 0.0,
        ..Params::default()
    }
}

fn ink(out: &[f32]) -> f32 {
    1.0 - out.iter().sum::<f32>() / out.len() as f32
}

#[test]
fn a_line_is_as_wide_as_the_darkness_a_share_of_the_spacing() -> Result<(), Error> {
    // darkness 0.25 of 8 px is 2 px centred at y = 4: pixel rows 3 and 4 whole, rows 2 and 5 only touch it
    let out = red(&straight(), 16, 8, &flat(16, 8, 0.75))?;
    for row in 0..16 {
        let want = if matches!(row % 8, 3 | 4) { 0.0 } else { 1.0 };
        assert!(out[row * 8..(row + 1) * 8].iter().all(|&v| v == want), "row {row}");
    }
    // the darkest tone fills the spacing: the lines meet
    let out = red(&straight(), 16, 8, &flat(16, 8, 0.0))?;
    assert!(out.iter().all(|&v| v < 1e-6), "black is all ink");
    Ok(())
}

#[test]
fn a_flat_tone_is_kept_exactly_on_the_pixel_lattice_to_black() -> Result<(), Error> {
    for tone in [0.05_f32, 0.2, 0.5, 0.8, 0.95, 1.0] {
        let out = red(&straight(), 32, 16, &flat(32, 16, 1.0 - tone))?;
        assert!((ink(&out) - tone).abs() < 1e-4, "tone {tone}: ink {}", ink(&out));
    }
    Ok(())
}

#[test]
fn a_bend_moves_the_lines_of_a_flat_picture_by_the_bend_times_its_luminance() -> Result<(), Error> {
    // luminance 0.5: darkness 0.5, 4 px lines, moved 6 x 0.5 = 3 px down: centred at y = 7, 15, ...: pixel rows
    // 5, 6, 7 and 8 (0 of the next 8) (smoothing a flat picture changes nothing, the edge included)
    let params = Params {
        bend: 6.0,
        smooth: 5.0,
        ..straight()
    };
    let out = red(&params, 32, 8, &flat(32, 8, 0.5))?;
    for row in 0..32 {
        let want = if matches!(row % 8, 5..=7 | 0) { 0.0 } else { 1.0 };
        assert!(
            out[row * 8..(row + 1) * 8].iter().all(|&v| (v - want).abs() < 1e-5),
            "row {row}"
        );
    }
    Ok(())
}

#[test]
fn straight_lines_run_along_their_angle() -> Result<(), Error> {
    // at 45 degrees a pixel and the one below and to its right lie on the same line
    let params = Params {
        angle: 45.0,
        ..straight()
    };
    let out = red(&params, 40, 40, &flat(40, 40, 0.7))?;
    for row in 0..39 {
        for column in 0..39 {
            let (a, b) = (out[row * 40 + column], out[(row + 1) * 40 + column + 1]);
            assert!((a - b).abs() < 1e-5, "({row}, {column})");
        }
    }
    assert!(
        (0..39).any(|column| (out[column] - out[40 + column]).abs() > 0.1),
        "and across them it changes"
    );
    Ok(())
}

/// A picture of `height` rows whose luminance follows `level(row)`.
fn rows_of(height: usize, width: usize, level: impl Fn(usize) -> f32) -> Vec<f32> {
    (0..height).flat_map(|row| vec![level(row); width * 3]).collect()
}

#[test]
fn a_steady_slope_only_stretches_the_lines_and_keeps_the_tone() -> Result<(), Error> {
    // luminance falls 0.8 over 128 rows; a bend of 80 moves the lines by 0.5 px more per row down: they spread to
    // 1 / (1 - 0.5) = twice the spacing and their width spreads with them
    let values = rows_of(128, 16, |row| 0.9 - 0.8 * (row as f32 + 0.5) / 128.0);
    let params = Params {
        bend: 80.0,
        smooth: 0.0,
        ..straight()
    };
    let out = red(&params, 128, 16, &values)?;
    assert!((ink(&out) - 0.5).abs() < 0.03, "ink {}", ink(&out));
    Ok(())
}

#[test]
fn a_jump_of_the_bend_gains_or_loses_at_most_one_lines_ink_at_the_jump() -> Result<(), Error> {
    // luminance 0.2 above row 45 and 0.8 below, a bend of 20: the lines below sit 12 px lower than above. Each side keeps
    // its tone, and where the two meet up to one line (at most `spacing` rows of ink per column) is gained or lost
    let (height, width) = (96, 8);
    let values = rows_of(height, width, |row| if row < 45 { 0.2 } else { 0.8 });
    let params = Params {
        bend: 20.0,
        smooth: 0.0,
        ..straight()
    };
    let out = red(&params, height, width, &values)?;
    let rows_of_ink = ink(&out) * height as f32;
    let tone = 0.8 * 45.0 + 0.2 * 51.0;
    let drift = (rows_of_ink - tone).abs();
    assert!(drift <= 8.0, "drift {drift} rows of ink");
    assert!(drift > 0.5, "the jump does move ink: {drift}");
    Ok(())
}

#[test]
fn lines_bowed_row_by_row_are_where_the_definition_puts_them() -> Result<(), Error> {
    // brute force: a row of luminance l has darkness 1 - l and its lines (8 px apart, centred at 4 + 8n) moved 20 l px; the
    // pixel's ink is the share of 64 points across it inside a line half as wide as the darkness times 8 either side
    let (height, width) = (80, 4);
    let light = |row: usize| 0.5 + 0.4 * (row as f32 / 9.0).sin();
    let params = Params {
        bend: 20.0,
        smooth: 0.0,
        ..straight()
    };
    let out = red(&params, height, width, &rows_of(height, width, light))?;
    for row in 0..height {
        let (l, got) = (f64::from(light(row)), f64::from(1.0 - out[row * width]));
        let (moved, half) = (20.0 * l, (1.0 - l) * 8.0 / 2.0);
        let inside = (0..64)
            .filter(|&k| {
                let across = row as f64 + (f64::from(k) + 0.5) / 64.0 - moved;
                let off = across - 8.0 * (across / 8.0).floor() - 4.0;
                off.abs() <= half
            })
            .count();
        assert!(
            (got - inside as f64 / 64.0).abs() < 0.03,
            "row {row}: {got} against {}",
            inside as f64 / 64.0
        );
    }
    Ok(())
}

#[test]
fn smoothing_spreads_the_bend_across_an_edge_and_leaves_the_flat_far_from_it() -> Result<(), Error> {
    // dark left, light right, an edge at column 48: smoothing by 8 px reaches 32 px, so columns 0 to 8 never see it and
    // columns 44 to 52 see it blurred
    let (height, width) = (16, 96);
    let values: Vec<f32> = (0..height * width * 3)
        .map(|i| if i / 3 % width < 48 { 0.2 } else { 0.8 })
        .collect();
    let render = |smooth| {
        red(
            &Params {
                bend: 8.0,
                smooth,
                ..straight()
            },
            height,
            width,
            &values,
        )
    };
    let (sharp, soft) = (render(0.0)?, render(8.0)?);
    let differ = |from: usize, to: usize| {
        (0..height).any(|row| (from..to).any(|c| (sharp[row * width + c] - soft[row * width + c]).abs() > 1e-4))
    };
    assert!(!differ(0, 8), "far from the edge");
    assert!(differ(44, 52), "at the edge");
    Ok(())
}

#[test]
fn ink_and_paper_are_the_colours_asked_for_and_equal_colours_print_paper() -> Result<(), Error> {
    let params = Params {
        color: Color([1.0, 0.5, 0.0]),
        paper: Color([0.0; 3]),
        ..straight()
    };
    let out = run_picture(&params, 8, 8, &flat(8, 8, 0.0))?;
    assert!(out.iter().all(|&v| v == 0.0), "paper only");
    let out = run_picture(&params, 8, 8, &flat(8, 8, 1.0))?;
    assert_eq!(
        &out[(3 * 8 + 1) * 3..(3 * 8 + 1) * 3 + 3],
        &[1.0, 0.5, 0.0],
        "ink on a line"
    );
    let same = Params {
        color: Color([0.3; 3]),
        paper: Color([0.3; 3]),
        ..straight()
    };
    let out = run_picture(&same, 8, 8, &flat(8, 8, 0.9))?;
    assert!(out.iter().all(|&v| (v - 0.3).abs() < 1e-6), "no tone to carry");
    Ok(())
}

#[test]
fn lines_told_their_place_are_the_lines_of_the_whole_render() -> Result<(), Error> {
    // the lines depend on the canvas place and, with a bend, on the smoothed picture: a flat one reads the same anywhere
    let params = Params {
        angle: 20.0,
        ..Params::default()
    };
    let (height, width) = (48, 40);
    let values = flat(height, width, 0.6);
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

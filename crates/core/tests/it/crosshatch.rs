//! `crosshatch`: layers of lines. The oracles are hand-worked pixel rows (a 2 px line on an 8 px pitch fills two pixel
//! rows; a 1 px line half-fills two; two layers at right angles add a grid), the tone a flat picture must keep on a pixel
//! lattice where the layers are independent exactly, the ceiling `1 - (1 - thickness)^layers`, the invariance of a line
//! along its own direction, and the whole render for lines told their place.

#![expect(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    reason = "small pixel counts; paper and ink are exact values"
)]

use bullterrier_image::Error;
use bullterrier_image::effects::crosshatch::Params;
use bullterrier_image::params::Color;

use crate::common::pixels::{run_picture, run_rows};

/// A grey picture of `height` x `width`.
fn flat(height: usize, width: usize, level: f32) -> Vec<f32> {
    vec![level; height * width * 3]
}

/// The red channel of the render of a flat grey picture, row after row.
fn red(params: &Params, height: usize, width: usize, level: f32) -> Result<Vec<f32>, Error> {
    Ok(channel(&run_picture(
        params,
        height,
        width,
        &flat(height, width, level),
    )?))
}

fn channel(values: &[f32]) -> Vec<f32> {
    values.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect()
}

/// Black lines on white paper, `spacing` 8, running along the x axis.
fn rows(layers: i32) -> Params {
    Params {
        layers,
        angle: 0.0,
        spacing: 8.0,
        ..Params::default()
    }
}

fn ink(out: &[f32]) -> f32 {
    1.0 - out.iter().sum::<f32>() / out.len() as f32
}

#[test]
fn a_two_pixel_line_fills_two_pixel_rows_of_every_eight() -> Result<(), Error> {
    // luminance 0.75 is a darkness of 0.25, a line of 0.25 x 8 = 2 px centred at y = 4: pixel rows 3 and 4 (centres 3.5 and
    // 4.5, a pixel each side of the line's middle) are covered whole; rows 2 and 5 touch only its edge
    let out = red(&rows(1), 16, 8, 0.75)?;
    for row in 0..16 {
        let want = if matches!(row % 8, 3 | 4) { 0.0 } else { 1.0 };
        assert!(out[row * 8..(row + 1) * 8].iter().all(|&v| v == want), "row {row}");
    }
    Ok(())
}

#[test]
fn a_line_thinner_than_a_pixel_keeps_its_area() -> Result<(), Error> {
    // darkness 0.125 is 1 px wide: half of each of rows 3 and 4; darkness 0.0625 is half a px: a quarter of each
    for (level, grey) in [(0.875, 0.5), (0.9375, 0.75)] {
        let out = red(&rows(1), 8, 8, level)?;
        for row in 0..8 {
            let want = if matches!(row, 3 | 4) { grey } else { 1.0 };
            assert!(
                out[row * 8..(row + 1) * 8].iter().all(|&v| (v - want).abs() < 1e-6),
                "{level}, row {row}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_second_layer_at_right_angles_adds_what_the_first_leaves() -> Result<(), Error> {
    // darkness 0.4 with lines of at most 0.5: the first layer takes 0.4 (3.2 px), the second asks 1 - 0.6 / 0.6 = 0 and
    // stays out; darkness 0.6: the first prints 0.5 (4 px, pixel rows 2 to 5 whole), the second 1 - 0.4 / 0.5 = 0.2 (1.6 px
    // wide, centred at x = 4: columns 3 and 4, each 0.8 covered)
    let out = red(&rows(2), 16, 16, 0.6)?;
    assert!(out[7 * 16..8 * 16].iter().all(|&v| v == 1.0), "no vertical lines yet");
    let out = red(&rows(2), 16, 16, 0.4)?;
    let at = |row: usize, column: usize| out[row * 16 + column];
    assert_eq!(at(3, 9), 0.0, "a horizontal line is whole ink");
    for column in [3, 4, 11, 12] {
        assert!((at(7, column) - 0.2).abs() < 1e-6, "column {column}: {}", at(7, column));
    }
    for column in [0, 1, 2, 5, 6, 7, 8] {
        assert_eq!(at(7, column), 1.0, "column {column} between lines is paper");
    }
    Ok(())
}

#[test]
fn a_flat_tone_is_kept_exactly_up_to_the_ceiling_where_layers_are_independent() -> Result<(), Error> {
    // lines along x and y on an 8 px lattice: each layer's mean ink is its line's share and the two overlap as a product,
    // so the ink is the tone to rounding, up to 1 - 0.5 x 0.5 = 0.75, and no more
    for tone in [0.1_f32, 0.3, 0.5, 0.6, 0.7] {
        let out = red(&rows(2), 32, 32, 1.0 - tone)?;
        assert!((ink(&out) - tone).abs() < 1e-4, "tone {tone}: ink {}", ink(&out));
    }
    let out = red(&rows(2), 32, 32, 0.1)?;
    assert!(
        (ink(&out) - 0.75).abs() < 1e-4,
        "a dark tone prints at the ceiling: {}",
        ink(&out)
    );
    Ok(())
}

#[test]
fn a_flat_tone_is_kept_on_slanted_layers_within_the_sampling_of_their_edges() -> Result<(), Error> {
    // layers at 45, 90, 135 and 0 degrees: not on the lattice, so each line's ink is the tone only up to how the
    // pixels sample it (a few thousandths of a 96 x 96 area) and the layers overlap as independent only on average
    let params = Params::default();
    for tone in [0.1_f32, 0.3, 0.5, 0.8] {
        let out = red(&params, 96, 96, 1.0 - tone)?;
        assert!((ink(&out) - tone).abs() < 0.02, "tone {tone}: ink {}", ink(&out));
    }
    Ok(())
}

#[test]
fn another_layer_changes_only_the_tones_the_layers_before_it_cannot_print() -> Result<(), Error> {
    // a ramp of darkness 0 to 1 across 64 columns: the first layer prints up to a darkness of 0.5, so more layers change
    // nothing at or below it (the others are turned by the layer count, so only the first is comparable)
    let (height, width) = (24, 64);
    let values: Vec<f32> = (0..height * width)
        .flat_map(|i| [1.0 - ((i % width) as f32 + 0.5) / width as f32; 3])
        .collect();
    let render = |layers| run_picture(&rows(layers), height, width, &values).map(|out| channel(&out));
    let (one, two, six) = (render(1)?, render(2)?, render(6)?);
    let same =
        |a: &[f32], b: &[f32], column: usize| (0..height).all(|row| a[row * width + column] == b[row * width + column]);
    for column in 0..width {
        let darkness = (column as f32 + 0.5) / width as f32;
        assert!(
            same(&one, &six, column) == (darkness <= 0.5),
            "1 against 6 layers, column {column}"
        );
    }
    assert!(!same(&one, &six, 60), "a dark column shows the other layers");
    assert!(!same(&one, &two, 60), "a dark column shows the second layer");
    Ok(())
}

#[test]
fn a_line_runs_along_its_angle() -> Result<(), Error> {
    // at 45 degrees a pixel and the one below and to its right lie on the same line, whatever the tone: the render
    // is unchanged by that step, and a step across the lines changes it
    let out = red(
        &Params {
            layers: 1,
            angle: 45.0,
            ..Params::default()
        },
        40,
        40,
        0.7,
    )?;
    for row in 0..39 {
        for column in 0..39 {
            let (a, b) = (out[row * 40 + column], out[(row + 1) * 40 + column + 1]);
            assert!((a - b).abs() < 1e-5, "({row}, {column})");
        }
    }
    let across = (0..39)
        .filter(|&column| (out[column] - out[40 + column]).abs() > 0.1)
        .count();
    assert!(across > 5, "the lines are not along the other diagonal: {across}");
    Ok(())
}

#[test]
fn ink_and_paper_are_the_colours_asked_for_and_equal_colours_print_paper() -> Result<(), Error> {
    let params = Params {
        color: Color([1.0, 0.5, 0.0]),
        paper: Color([0.0; 3]),
        ..rows(1)
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
        ..rows(1)
    };
    let out = run_picture(&same, 8, 8, &flat(8, 8, 0.9))?;
    assert!(out.iter().all(|&v| (v - 0.3).abs() < 1e-6), "no tone to carry");
    Ok(())
}

#[test]
fn lines_told_their_place_are_the_lines_of_the_whole_render() -> Result<(), Error> {
    // a flat picture reads the same anywhere, so only the canvas place can tell the lines apart
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

#[test]
fn lines_as_wide_as_their_spacing_cover_a_black_picture_whole() -> Result<(), Error> {
    // at thickness 1 neighbouring lines meet at a pixel whose centre is 3.5 px from one and 3.25 from the next (pitch
    // 6.75): 0.375 inside one and 0.625 inside the other, which add up to ink
    let params = Params {
        thickness: 1.0,
        spacing: 6.75,
        ..rows(1)
    };
    let out = red(&params, 32, 32, 0.0)?;
    assert!(out.iter().all(|&v| v < 1e-6), "a black picture is all ink");
    Ok(())
}

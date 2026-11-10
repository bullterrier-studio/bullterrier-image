//! `reflection`: the rows above a shape's lowest row mirrored below it. Every row of the picture is its own grey, so a
//! mirrored row is told by its level: the row k below the floor line, with no blur, is the row k + 1 above it, mixed in
//! by 1 - fade x k / reach, where reach is `height` of the shape's rows.

#![expect(clippy::cast_precision_loss, reason = "rows of a small picture")]

use bullterrier_image::Error;
use bullterrier_image::effects::reflection;

use crate::common::pixels::run_with_field;

const HEIGHT: usize = 20;
const WIDTH: usize = 3;

/// Row `row` is grey `row / HEIGHT`.
fn rows() -> Vec<f32> {
    (0..HEIGHT * WIDTH * 3)
        .map(|at| (at / (WIDTH * 3)) as f32 / HEIGHT as f32)
        .collect()
}

/// A field of `level` on the rows `shape` (in the middle column only), 0 elsewhere.
fn shape(rows: std::ops::Range<usize>, level: f32) -> Vec<f32> {
    (0..HEIGHT * WIDTH)
        .map(|at| {
            if rows.contains(&(at / WIDTH)) && at % WIDTH == 1 {
                level
            } else {
                0.0
            }
        })
        .collect()
}

fn params(height: f32, fade: f32) -> reflection::Params {
    reflection::Params {
        height,
        fade,
        blur: 0.0,
        ..Default::default()
    }
}

/// The level of each row of `picture` (its first channel, left column).
fn levels(picture: &[f32]) -> Vec<f32> {
    (0..HEIGHT).map(|row| picture[row * WIDTH * 3]).collect()
}

fn run(params: &reflection::Params, field: &[f32], picture: &[f32]) -> Result<Vec<f32>, Error> {
    Ok(levels(&run_with_field(params, "field", field, HEIGHT, WIDTH, picture)?))
}

/// The levels expected: the row `floor + k` (k below `depth`) mixed with the row `floor - 1 - k` by `1 - fade k / reach`.
fn mirrored(floor: usize, depth: usize, reach: usize, fade: f32) -> Vec<f32> {
    let level = |row: usize| row as f32 / HEIGHT as f32;
    (0..HEIGHT)
        .map(|row| {
            if (floor..floor + depth).contains(&row) {
                let below = row - floor;
                let strength = 1.0 - fade * below as f32 / reach as f32;
                (1.0 - strength) * level(row) + strength * level(floor - 1 - below)
            } else {
                level(row)
            }
        })
        .collect()
}

fn close(got: &[f32], wanted: &[f32]) -> bool {
    got.iter().zip(wanted).all(|(a, b)| (a - b).abs() < 1e-6)
}

#[test]
fn the_rows_above_the_floor_line_are_mirrored_below_it_as_deep_as_height_of_the_shape_and_fading() -> Result<(), Error>
{
    // the shape spans rows 4 to 9: 6 rows, its floor line below row 9
    let field = shape(4..10, 1.0);
    for (height, fade, depth) in [(0.5, 0.0, 3), (0.5, 1.0, 3), (1.0, 0.7, 6), (0.25, 0.4, 2)] {
        let got = run(&params(height, fade), &field, &rows())?;
        let wanted = mirrored(10, depth, depth, fade);
        assert!(
            close(&got, &wanted),
            "height {height}, fade {fade}: {got:?}, wanted {wanted:?}"
        );
    }
    Ok(())
}

#[test]
fn where_the_picture_ends_first_the_reflection_is_cut_there_but_fades_as_if_it_went_on() -> Result<(), Error> {
    // rows 10 to 17: 8 rows, so a full reflection would reach 8 rows below row 17, but the picture has 2
    let got = run(&params(1.0, 0.8), &shape(10..18, 1.0), &rows())?;
    assert!(close(&got, &mirrored(18, 2, 8, 0.8)), "{got:?}");
    Ok(())
}

#[test]
fn the_blur_sees_the_edge_rule_beyond_the_floor_line_not_the_rows_below_it() -> Result<(), Error> {
    // dark above the floor line, white below it: a blur that read below would brighten the mirror image
    let picture: Vec<f32> = (0..HEIGHT * WIDTH * 3)
        .map(|at| if at / (WIDTH * 3) < 10 { 0.2 } else { 1.0 })
        .collect();
    let blurred = reflection::Params {
        blur: 3.0,
        ..params(0.5, 0.0)
    };
    let got = run(&blurred, &shape(4..10, 1.0), &picture)?;
    assert!(got[10..13].iter().all(|level| (level - 0.2).abs() < 1e-6), "{got:?}");
    assert!(close(&got[13..], &[1.0; HEIGHT - 13]), "{got:?}");
    Ok(())
}

#[test]
fn no_row_of_the_shape_at_or_above_the_threshold_and_no_height_leave_the_picture_as_it_is() -> Result<(), Error> {
    for (settings, field) in [
        (params(1.0, 0.0), shape(4..10, 0.4)),
        (params(1.0, 0.0), vec![0.0; HEIGHT * WIDTH]),
        (params(0.0, 0.0), shape(4..10, 1.0)),
    ] {
        assert!(close(&run(&settings, &field, &rows())?, &levels(&rows())));
    }
    // a lower threshold takes the faint shape in
    let lower = reflection::Params {
        field_threshold: 0.3,
        ..params(0.5, 0.0)
    };
    assert!(close(
        &run(&lower, &shape(4..10, 0.4), &rows())?,
        &mirrored(10, 3, 3, 0.0)
    ));
    Ok(())
}

//! `outline`: a stroke and a halo around a shape. A square shape on a grey picture with a white stroke: along its middle
//! row a pixel k columns outside the square is k px from it (k inside: -k), so the coverage of each pixel is worked by
//! hand from that distance.

use bullterrier_image::Error;
use bullterrier_image::effects::outline::{self, Position};
use bullterrier_image::params::Color;

use crate::common::compare::assert_close;
use crate::common::pixels::run_with_field;

const SIZE: usize = 21;
/// The square's columns (and rows): 7..14.
const SQUARE: std::ops::Range<usize> = 7..14;
const GREY: f32 = 0.2;

fn square(level: f32) -> Vec<f32> {
    (0..SIZE * SIZE)
        .map(|at| {
            if SQUARE.contains(&(at / SIZE)) && SQUARE.contains(&(at % SIZE)) {
                level
            } else {
                0.0
            }
        })
        .collect()
}

/// The coverage of the white stroke at each column of the middle row: how far each pixel moved from grey to white.
fn middle_row(params: &outline::Params, field: &[f32]) -> Result<Vec<f32>, Error> {
    let out = run_with_field(params, "field", field, SIZE, SIZE, &vec![GREY; SIZE * SIZE * 3])?;
    Ok((0..SIZE)
        .map(|column| (out[(10 * SIZE + column) * 3] - GREY) / (1.0 - GREY))
        .collect())
}

fn params(width: f32, position: Position, glow: f32) -> outline::Params {
    outline::Params {
        width,
        position,
        glow,
        ..Default::default()
    }
}

/// The coverage along the middle row that a stroke covering `outside` px beyond the square's edge and `inside` px within
/// it gives: 1 where a pixel's distance is within the reach, the one-pixel ramp past it, 0 beyond.
fn band(outside: f32, inside: f32) -> Vec<f32> {
    (0..SIZE)
        .map(|column| {
            #[expect(clippy::cast_precision_loss, reason = "a column of a small picture")]
            let column = column as f32;
            if column < 7.0 {
                (outside - (7.0 - column) + 1.0).clamp(0.0, 1.0)
            } else if column > 13.0 {
                (outside - (column - 13.0) + 1.0).clamp(0.0, 1.0)
            } else {
                // the square's own pixels: 1 px in on its outermost, the middle one 4 px in
                let depth = (column - 6.0).min(14.0 - column);
                (inside - depth + 1.0).clamp(0.0, 1.0)
            }
        })
        .collect()
}

#[test]
fn the_stroke_lies_on_the_side_position_says_over_the_pixels_its_width_reaches() -> Result<(), Error> {
    let shape = square(1.0);
    for (width, position, outside, inside) in [
        (3.0, Position::Outside, 3.0, 0.0),
        (2.5, Position::Outside, 2.5, 0.0),
        (3.0, Position::Inside, 0.0, 3.0),
        (4.0, Position::Center, 2.0, 2.0),
        (3.0, Position::Center, 1.5, 1.5),
        (0.0, Position::Outside, 0.0, 0.0),
    ] {
        assert_close(
            &middle_row(&params(width, position, 0.0), &shape)?,
            &band(outside, inside),
            1e-6,
            &format!("width {width}, {position:?}"),
        );
    }
    Ok(())
}

#[test]
fn the_halo_fades_quadratically_from_the_stroke_to_nothing_at_glow_px_beyond_it() -> Result<(), Error> {
    let row = middle_row(&params(2.0, Position::Outside, 8.0), &square(1.0))?;
    // the column 7 - k is k px out, 2 of them stroke: past it the halo is (1 - (k - 2) / 8)^2, gone at 10 px
    for (column, wanted) in [
        (6, 1.0),
        (5, 1.0),
        (4, 0.875_f32 * 0.875),
        (1, 0.5 * 0.5),
        (0, 0.375 * 0.375),
    ] {
        assert!((row[column] - wanted).abs() < 1e-6, "column {column}: {}", row[column]);
    }
    let far = middle_row(&params(0.0, Position::Outside, 4.0), &square(1.0))?;
    assert!(
        far[..3].iter().all(|cover| *cover == 0.0),
        "the halo ends at glow px: {far:?}"
    );
    Ok(())
}

#[test]
fn outside_a_soft_shape_the_stroke_lies_behind_it_and_the_threshold_decides_where_the_shape_ends() -> Result<(), Error>
{
    // a square of 1 framed by a ring of 0.3, below the threshold: outside the shape, but covering 0.3 of its pixel
    let mut shape = square(1.0);
    for (at, value) in shape.iter_mut().enumerate() {
        let (row, column) = (at / SIZE, at % SIZE);
        if (6..15).contains(&row) && (6..15).contains(&column) && *value == 0.0 {
            *value = 0.3;
        }
    }
    let row = middle_row(&params(3.0, Position::Outside, 0.0), &shape)?;
    assert!((row[6] - 0.7).abs() < 1e-6 && (row[5] - 1.0).abs() < 1e-6, "{row:?}");
    // at a threshold of 0.3 the ring is the shape's: the stroke starts a column further out
    let lower = outline::Params {
        field_threshold: 0.3,
        ..params(3.0, Position::Outside, 0.0)
    };
    let row = middle_row(&lower, &shape)?;
    assert!(row[6] == 0.0 && (row[3] - 1.0).abs() < 1e-6 && row[2] == 0.0, "{row:?}");
    Ok(())
}

#[test]
fn a_field_with_no_edge_leaves_the_picture_as_it_is() -> Result<(), Error> {
    // all below the threshold (no shape), and all above it (no outside)
    for level in [0.4, 1.0] {
        let field = vec![level; SIZE * SIZE];
        let row = middle_row(&params(6.0, Position::Center, 30.0), &field)?;
        assert!(row.iter().all(|cover| *cover == 0.0), "a field of {level}: {row:?}");
    }
    Ok(())
}

/// The outline of a square across the whole picture, black stroke to white `color_end`, 3 px wide: the stroke is full
/// strength on the 3 columns (rows) beside the square, so there the picture holds the gradient's own value.
fn gradient_params(angle: f32) -> outline::Params {
    outline::Params {
        width: 3.0,
        color: Color([0.0; 3]),
        color_end: Some(Color([1.0; 3])),
        angle,
        ..Default::default()
    }
}

fn red_channel(params: &outline::Params, column: usize, row: usize) -> Result<f32, Error> {
    let out = run_with_field(params, "field", &square(1.0), SIZE, SIZE, &vec![GREY; SIZE * SIZE * 3])?;
    Ok(out[(row * SIZE + column) * 3])
}

#[test]
fn the_stroke_runs_from_color_to_color_end_across_the_picture_not_the_shape() -> Result<(), Error> {
    // linear gradient over the 21 x 21 picture: at a pixel centre (c + 0.5, r + 0.5) the share along the angle is
    // (c + 0.5) / 21 at 0 degrees, (r + 0.5) / 21 at 90, and the reverse at 180 and 270
    #[expect(clippy::cast_precision_loss, reason = "a pixel index of a small picture")]
    let share = |index: usize| (index as f32 + 0.5) / SIZE as f32;
    for (angle, column, row, wanted) in [
        (0.0, 4, 10, share(4)),
        (0.0, 16, 10, share(16)),
        (90.0, 10, 4, share(4)),
        (90.0, 10, 16, share(16)),
        (180.0, 4, 10, 1.0 - share(4)),
        (270.0, 10, 16, 1.0 - share(16)),
    ] {
        let value = red_channel(&gradient_params(angle), column, row)?;
        assert!(
            (value - wanted).abs() < 1e-6,
            "angle {angle}, ({column}, {row}): {value} for {wanted}"
        );
    }
    Ok(())
}

#[test]
fn a_color_end_equal_to_color_gives_the_single_colour_picture_whatever_the_angle() -> Result<(), Error> {
    // the picture without `color_end` is the one the stroke and halo of `color` alone make; a pair of one colour is it
    let base = outline::Params {
        width: 3.0,
        glow: 5.0,
        color: Color([0.9, 0.3, 0.1]),
        ..Default::default()
    };
    let field = square(1.0);
    let pixels = vec![GREY; SIZE * SIZE * 3];
    let single = run_with_field(&base, "field", &field, SIZE, SIZE, &pixels)?;
    for angle in [0.0, 33.0, 180.0] {
        let pair = outline::Params {
            color_end: Some(base.color),
            angle,
            ..base.clone()
        };
        let same = run_with_field(&pair, "field", &field, SIZE, SIZE, &pixels)?;
        assert!(same == single, "angle {angle}");
    }
    Ok(())
}

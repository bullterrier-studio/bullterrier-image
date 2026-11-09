//! `topographic`: contour lines where a field crosses a level. The field is a linear ramp, whose contours are worked by
//! hand: at `levels` bands over `width - 1` px a line falls on every column that is a multiple of `(width - 1) / levels`.

use bullterrier_image::effects::topographic::{Fill, Params};
use bullterrier_image::params::Color;
use bullterrier_image::{Edge, Error};

use crate::common::compare::assert_close;
use crate::common::pictures::{ramp, varied};
use crate::common::pixels::{channel, run_with_field};

const HEIGHT: usize = 6;
/// 65 columns: the field is column / 64, so with 2, 4, 8 or 16 levels a line falls on whole columns.
const WIDTH: usize = 65;

/// The field: 0 on the left column to 1 on the right one.
fn field() -> Vec<f32> {
    channel(&ramp(HEIGHT, WIDTH), 0)
}

fn grey(level: f32) -> Vec<f32> {
    vec![level; HEIGHT * WIDTH * 3]
}

/// The lines unblurred, black picture and white lines: a pixel's red is the strength of the line over it.
fn plain(levels: i32, thickness: f32) -> Params {
    Params {
        levels,
        thickness,
        smoothing: 0.0,
        ..Params::default()
    }
}

/// The first row of the picture's red channel.
fn first_row(picture: &[f32]) -> Vec<f32> {
    channel(picture, 0)[..WIDTH].to_vec()
}

fn run(params: &Params, picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_with_field(params, "field", &field(), HEIGHT, WIDTH, picture)
}

#[test]
fn lines_of_a_linear_field_are_evenly_spaced_stripes() -> Result<(), Error> {
    for (levels, spacing) in [(2, 32), (4, 16), (8, 8), (16, 4)] {
        let out = run(&plain(levels, 1.0), &grey(0.0))?;
        let expected: Vec<f32> = (0..WIDTH)
            .map(|column| {
                if column % spacing == 0 && column > 0 && column < 64 {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
        assert_close(&first_row(&out), &expected, 1e-6, &format!("{levels} levels"));
        // the field does not vary down the picture: every row alike
        assert_close(&channel(&out, 0)[WIDTH..2 * WIDTH], &expected, 1e-6, "second row");
    }
    Ok(())
}

#[test]
fn a_line_covers_as_many_pixels_as_the_thickness() -> Result<(), Error> {
    // 7 lines 8 px apart; each one's coverage across the row adds up to its thickness
    for thickness in [1.0_f32, 2.0, 2.5, 3.5] {
        let row = first_row(&run(&plain(8, thickness), &grey(0.0))?);
        let total: f32 = row.iter().sum();
        assert!((total - 7.0 * thickness).abs() < 1e-3, "thickness {thickness}: {total}");
    }
    Ok(())
}

#[test]
fn lines_are_drawn_over_the_photo_in_the_line_colour() -> Result<(), Error> {
    let params = Params {
        line: Color([1.0, 0.0, 0.0]),
        ..plain(8, 1.0)
    };
    let out = run(&params, &grey(0.25))?;
    let pixel = |column: usize| out[column * 3..column * 3 + 3].to_vec();
    assert_eq!(pixel(16), [1.0, 0.0, 0.0]);
    assert_eq!(pixel(17), [0.25; 3]);
    Ok(())
}

#[test]
fn a_flat_field_has_no_lines() -> Result<(), Error> {
    for level in [0.0, 0.5, 0.375, 1.0] {
        let picture = varied(HEIGHT, WIDTH);
        let out = run_with_field(
            &plain(8, 3.0),
            "field",
            &vec![level; HEIGHT * WIDTH],
            HEIGHT,
            WIDTH,
            &picture,
        )?;
        assert_close(&out, &picture, 0.0, &format!("flat at {level}"));
    }
    Ok(())
}

#[test]
fn bands_fill_each_band_with_its_colour_from_the_ramp() -> Result<(), Error> {
    // two colours, black to white: band k of 8 is k / 7; the field at column c is c / 8 bands up
    let params = Params {
        fill: Fill::Bands,
        colors: vec![Color([0.0; 3]), Color([1.0; 3])],
        ..plain(8, 0.5)
    };
    let row = first_row(&run(&params, &varied(HEIGHT, WIDTH))?);
    for (column, level) in row.iter().enumerate().filter(|(column, _)| column % 8 != 0) {
        let band = f32::from(u8::try_from(column / 8).unwrap_or(0));
        assert!((level - band / 7.0).abs() < 1e-6, "column {column}");
    }
    // the top of the range belongs to the last band
    assert!((row[WIDTH - 1] - 1.0).abs() < 1e-6);
    Ok(())
}

#[test]
fn smoothing_leaves_the_interior_lines_of_a_linear_field_where_they_are() -> Result<(), Error> {
    // a Gaussian keeps a straight slope straight away from the edges
    let params = Params {
        smoothing: 2.0,
        edge: Edge::Nearest,
        ..plain(8, 1.0)
    };
    let row = first_row(&run(&params, &grey(0.0))?);
    for (column, level) in row.iter().enumerate().take(53).skip(12) {
        let expected = if column % 8 == 0 { 1.0 } else { 0.0 };
        assert!((level - expected).abs() < 1e-2, "column {column}: {level}");
    }
    Ok(())
}

//! `parallax`: layers shifted along an angle by how far their field lies from the focus. Checked on hand-worked impulses
//! and blocks, and through the `coordinates` picture, which tells each pixel where it was read from.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "small picture sizes: every count and index is far below 2^52; a shift is a few pixels"
)]

use bullterrier_image::effects::parallax::Params;
use bullterrier_image::{Edge, Error, Interpolation};

use crate::common::compare::first_difference_within;
use crate::common::pictures::{coordinates, impulse, places, varied};
use crate::common::pixels::run_with_field;

const HEIGHT: usize = 20;
const WIDTH: usize = 40;

fn run(params: &Params, field: &[f32], height: usize, width: usize, picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_with_field(params, "field", field, height, width, picture)
}

/// A field with `value` everywhere.
fn flat(value: f32, height: usize, width: usize) -> Vec<f32> {
    vec![value; height * width]
}

/// The positions of the pixels with red above a half, as (row, column).
fn lit(out: &[f32], width: usize) -> Vec<(usize, usize)> {
    (0..out.len() / 3)
        .filter(|&at| out[at * 3] > 0.5)
        .map(|at| (at / width, at % width))
        .collect()
}

#[test]
fn no_amount_leaves_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(7, 9);
    let field = varied(7, 9).into_iter().step_by(3).collect::<Vec<_>>();
    let params = Params {
        amount: 0.0,
        angle: 33.0,
        focus: 0.2,
        ..Params::default()
    };
    assert_eq!(run(&params, &field, 7, 9, &picture)?, picture);
    Ok(())
}

#[test]
fn a_layer_nearer_than_the_focus_moves_along_the_angle_and_a_farther_one_against_it() -> Result<(), Error> {
    // by hand: amount 8, focus 0.5: a flat field of 1 moves 8 x 0.5 = 4 px, of 0 moves -4 px, of 0.5 not at all. A lit pixel
    // at (10, 10) seen at 0 degrees (right), 90 (down), 180 (left), 270 (up)
    let picture = impulse(HEIGHT, WIDTH, 10, 10);
    // (field, angle, where the pixel is seen)
    let rows = [
        (1.0, 0.0, (10, 14)),
        (1.0, 90.0, (14, 10)),
        (1.0, 180.0, (10, 6)),
        (1.0, 270.0, (6, 10)),
        (0.0, 0.0, (10, 6)),
        (0.0, 90.0, (6, 10)),
        (0.5, 0.0, (10, 10)),
        (0.5, 90.0, (10, 10)),
    ];
    for (depth, angle, seen) in rows {
        let params = Params {
            angle,
            amount: 8.0,
            interpolation: Interpolation::Nearest,
            ..Params::default()
        };
        let out = run(&params, &flat(depth, HEIGHT, WIDTH), HEIGHT, WIDTH, &picture)?;
        assert_eq!(lit(&out, WIDTH), vec![seen], "field {depth}, angle {angle}");
    }
    Ok(())
}

#[test]
fn a_flat_field_is_a_uniform_shift_of_amount_times_its_distance_from_the_focus() -> Result<(), Error> {
    // the shift read back from the coordinates picture, away from the border it reads past: a pixel reads the place that far
    // back along the angle
    let (height, width) = (40, 60);
    let picture = coordinates(height, width);
    // (field, focus, amount, angle)
    let rows = [
        (0.9, 0.5, 9.6, 0.0),
        (0.2, 0.5, 9.6, 0.0),
        (1.0, 0.0, 12.0, 90.0),
        (0.3, 0.8, 20.0, 37.0),
        (0.75, 0.25, 6.0, 200.0),
        (0.6, 0.1, 30.0, 359.0),
    ];
    for (depth, focus, amount, angle) in rows {
        let shift = f64::from(amount) * (f64::from(depth) - f64::from(focus));
        let (sin, cos) = f64::from(angle).to_radians().sin_cos();
        let params = Params {
            angle,
            amount,
            focus,
            ..Params::default()
        };
        let out = run(&params, &flat(depth, height, width), height, width, &picture)?;
        let margin = shift.abs().ceil() as usize + 1;
        for (at, (row, column)) in places(&out, height, width).into_iter().enumerate() {
            let (r, c) = (at / width, at % width);
            if (margin..height - margin).contains(&r) && (margin..width - margin).contains(&c) {
                assert!(
                    (row - (r as f64 - shift * sin)).abs() < 1e-3 && (column - (c as f64 - shift * cos)).abs() < 1e-3,
                    "field {depth}, focus {focus}, angle {angle}: pixel ({r}, {c}) read ({row}, {column})"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn the_shift_grows_with_the_distance_in_depth_from_the_focus() -> Result<(), Error> {
    // the left half of the picture at depth 0.9, the right half at 0.6, focus 0.4, amount 10, along the columns: far from the
    // seam (more than the reach of 6 px) the halves shift 5 and 2 px, in the ratio of their distances from the focus
    let (height, width) = (10, 80);
    let field: Vec<f32> = (0..height * width)
        .map(|at| if at % width < width / 2 { 0.9 } else { 0.6 })
        .collect();
    let params = Params {
        amount: 10.0,
        focus: 0.4,
        ..Params::default()
    };
    let out = run(&params, &field, height, width, &coordinates(height, width))?;
    let read = places(&out, height, width);
    let shift_at = |column: usize| column as f64 - read[5 * width + column].1;
    let (near, far) = (shift_at(20), shift_at(60));
    assert!(
        (near - 5.0).abs() < 1e-3 && (far - 2.0).abs() < 1e-3,
        "{near} and {far}"
    );
    Ok(())
}

#[test]
fn a_near_object_lands_whole_over_what_lies_behind_it() -> Result<(), Error> {
    // columns 10..=14 are an object at depth 1 on a background of 0; focus 0, amount 8: the object moves 8 px, the
    // background none. A pixel takes the nearest depth within 8 px, so each of columns 18..=22, which read 10..=14, finds
    // the object in reach and moves with it: the object is seen whole, 8 px on, and nowhere else
    let (height, width) = (6, 40);
    let object = |at: usize| (10..=14).contains(&(at % width));
    let field: Vec<f32> = (0..height * width).map(|at| f32::from(u8::from(object(at)))).collect();
    let picture: Vec<f32> = (0..height * width)
        .flat_map(|at| [f32::from(u8::from(object(at))); 3])
        .collect();
    let params = Params {
        amount: 8.0,
        focus: 0.0,
        interpolation: Interpolation::Nearest,
        ..Params::default()
    };
    let out = run(&params, &field, height, width, &picture)?;
    let expected: Vec<(usize, usize)> = (0..height)
        .flat_map(|row| (18..=22).map(move |column| (row, column)))
        .collect();
    assert_eq!(lit(&out, width), expected);
    Ok(())
}

#[test]
fn a_shift_that_is_not_a_whole_number_of_pixels_looks_that_much_further_for_the_depth() -> Result<(), Error> {
    // the object of the last test, shifted 7.2 px: a pixel is read 7.2 px back, between two pixels, and takes the object's
    // shift when the object is within 8 px (the shift rounded up) of it. Column 22 reads 14.8, a fifth of the way from the
    // object's last pixel to the dark one beyond it, so it is 0.2 lit; column 17 reads 9.8, so it is 0.8 lit; columns 18 to
    // 21 read inside the object; 16 and 23 read dark
    let (height, width) = (3, 40);
    let object = |at: usize| (10..=14).contains(&(at % width));
    let field: Vec<f32> = (0..height * width).map(|at| f32::from(u8::from(object(at)))).collect();
    let picture: Vec<f32> = (0..height * width)
        .flat_map(|at| [f32::from(u8::from(object(at))); 3])
        .collect();
    let params = Params {
        amount: 7.2,
        focus: 0.0,
        ..Params::default()
    };
    let out = run(&params, &field, height, width, &picture)?;
    let row: Vec<f32> = (0..width).map(|column| out[(width + column) * 3]).collect();
    for (column, lit) in [(16, 0.0), (17, 0.8), (18, 1.0), (21, 1.0), (22, 0.2), (23, 0.0)] {
        assert!(
            (row[column] - lit).abs() < 1e-5,
            "column {column}: {} not {lit}",
            row[column]
        );
    }
    Ok(())
}

#[test]
fn the_edge_rule_decides_what_lies_beyond_the_border_for_the_shift_and_for_the_depth() -> Result<(), Error> {
    // a lit pixel in the last column (19) of a row of 20, an object there at depth 1; focus 0, amount 4 along the row, so the
    // reach is 4 px. With 'wrap' the first columns have the object in reach (it is the column before the first), and
    // column 3 reads column -1, which is the lit pixel: it is seen there. With any other rule nothing near the first columns
    // sees the object, and the lit pixel is seen nowhere (column 19 moves to read 15)
    let width = 20;
    let picture = impulse(1, width, 0, 19);
    let mut field = vec![0.0; width];
    field[19] = 1.0;
    for (edge, seen) in [
        (Edge::Wrap, vec![(0, 3)]),
        (Edge::Reflect, vec![]),
        (Edge::Mirror, vec![]),
        (Edge::Nearest, vec![]),
    ] {
        let params = Params {
            amount: 4.0,
            focus: 0.0,
            interpolation: Interpolation::Nearest,
            edge,
            ..Params::default()
        };
        assert_eq!(lit(&run(&params, &field, 1, width, &picture)?, width), seen, "{edge:?}");
    }
    Ok(())
}

#[test]
fn the_field_decides_which_pixels_move() -> Result<(), Error> {
    // the same picture and parameters, two fields: where the field is the focus the picture stays, elsewhere it moves
    let picture = varied(HEIGHT, WIDTH);
    let params = Params::default(); // focus 0.5
    let still = run(&params, &flat(0.5, HEIGHT, WIDTH), HEIGHT, WIDTH, &picture)?;
    assert_eq!(first_difference_within(&still, &picture, 1e-6), None);
    let moved = run(&params, &flat(0.9, HEIGHT, WIDTH), HEIGHT, WIDTH, &picture)?;
    assert!(first_difference_within(&moved, &picture, 0.01).is_some());
    Ok(())
}

#[test]
fn a_full_turn_is_the_same_as_none() -> Result<(), Error> {
    let picture = varied(HEIGHT, WIDTH);
    let field: Vec<f32> = (0..HEIGHT * WIDTH)
        .map(|at| (at % WIDTH) as f32 / (WIDTH - 1) as f32)
        .collect();
    let run_at = |angle| {
        run(
            &Params {
                angle,
                ..Params::default()
            },
            &field,
            HEIGHT,
            WIDTH,
            &picture,
        )
    };
    assert_eq!(first_difference_within(&run_at(0.0)?, &run_at(360.0)?, 1e-5), None);
    Ok(())
}

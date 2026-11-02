//! Where the ends of a curved cut are carried: an end that is not on the border of the frame goes to the nearest point of
//! the border (the earlier on the border's walk when two are as near), and the curve is closed by the walk along the
//! border. (`cuts.rs` has the curves whose ends lie on the border or inside the frame.) Pieces are numbered by their
//! first pixel, so which side of a cut is piece 0 does not matter.

mod common;

use bullterrier_image::Error;
use bullterrier_image::region::{Curve, Cuts};
use common::ops::partition;

/// The pieces of a 6 x 4 picture (the frame, so shares of it are px / 6 and px / 4) cut by the curve `path`, as one line of
/// digits for each row, rows joined by `/`, and the number of pieces.
fn cut(path: &str) -> Result<(String, usize), Error> {
    let cuts = Cuts {
        lines: vec![Curve { path: path.to_owned() }.into()],
        ..Cuts::default()
    };
    let (labels, count) = partition((4, 6), None, cuts)?;
    let rows = labels
        .as_chunks::<6>()
        .0
        .iter()
        .map(|row| row.iter().map(ToString::to_string).collect::<String>())
        .collect::<Vec<_>>()
        .join("/");
    Ok((rows, count))
}

#[test]
fn an_end_beyond_the_frame_is_carried_to_the_nearest_border() -> Result<(), Error> {
    // (3, 0) to (3, 6): the end below the picture is carried straight up to the bottom border at (3, 4)
    assert_eq!(cut("M 0.5 0 L 0.5 1.5")?, ("000111/000111/000111/000111".to_owned(), 2));
    // (3, 4) to (-6, 2): the curve crosses the left border at (0, 3.33) and its end beyond is carried to (0, 2). Within the
    // picture it is the line y = 3.33 + 2x / 9, and only the centre (0.5, 3.5) of the pixels is below it
    assert_eq!(cut("M 0.5 1 L -1 0.5")?, ("000000/000000/000000/100000".to_owned(), 2));
    Ok(())
}

#[test]
fn an_end_as_near_two_borders_goes_to_the_earlier_one_clockwise_from_the_top_left() -> Result<(), Error> {
    // the middle of the 6 x 4 frame (3, 2) is half the frame from the top, the right, the bottom and the left: the top is
    // the earlier, so the curve to the middle of the bottom is closed up to (3, 0) and cuts the frame in two halves
    assert_eq!(cut("M 0.5 0.5 L 0.5 1")?, ("000111/000111/000111/000111".to_owned(), 2));
    Ok(())
}

#[test]
fn a_curve_from_a_corner_to_the_opposite_corner_cuts_the_picture_along_its_diagonal() -> Result<(), Error> {
    // (0, 0) to (6, 4): a pixel centre (c + 1/2, r + 1/2) is below the diagonal y = 2x / 3 when 2c < 3r + 1/2, so row r has
    // its first 1, 2, 4 and 5 pixels (rows 0 to 3) on the side of the first pixel
    assert_eq!(cut("M 0 0 L 1 1")?, ("011111/001111/000011/000001".to_owned(), 2));
    Ok(())
}

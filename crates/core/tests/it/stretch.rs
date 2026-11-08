//! `stretch`: one row or column repeated over the lines beside it. Checked by hand on a line of numbered pixels (where
//! each output pixel says which input pixel it is), on the untouched side, the two axes against each other, and on
//! spans that cover nothing or everything.

use bullterrier_image::Error;
use bullterrier_image::effects::stretch::{Axis, Params, Side};

use crate::common::compare::first_difference;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;

/// A column of `length` pixels (1 wide) numbered 0, 1, 2.. in red (the number over 100), green and blue 0.
fn numbered(length: usize) -> Vec<f32> {
    #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
    (0..length).flat_map(|index| [index as f32 / 100.0, 0.0, 0.0]).collect()
}

/// The numbers `stretch` leaves in a column of `length` numbered pixels.
fn numbers_after(params: &Params, length: usize) -> Result<Vec<usize>, Error> {
    let out = run_picture(params, length, 1, &numbered(length))?;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "small non-negative numbers"
    )]
    Ok(out
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| (pixel[0] * 100.0).round() as usize)
        .collect())
}

fn vertical(at: f32, span: f32, side: Side) -> Params {
    Params {
        axis: Axis::Vertical,
        at,
        span,
        side,
    }
}

#[test]
fn the_line_is_repeated_over_the_span_on_the_sides_asked_for() -> Result<(), Error> {
    // 11 pixels, the line at 0.5 of the way = pixel 5, a span of 2 px
    let cases: [(Side, [usize; 11]); 3] = [
        (Side::Both, [0, 1, 2, 5, 5, 5, 5, 5, 8, 9, 10]),
        (Side::After, [0, 1, 2, 3, 4, 5, 5, 5, 8, 9, 10]),
        (Side::Before, [0, 1, 2, 5, 5, 5, 6, 7, 8, 9, 10]),
    ];
    for (side, expected) in cases {
        assert_eq!(numbers_after(&vertical(0.5, 2.0, side), 11)?, expected, "{side:?}");
    }
    Ok(())
}

#[test]
fn the_line_lies_at_its_fraction_of_the_way_and_the_span_is_rounded_to_whole_pixels() -> Result<(), Error> {
    // 11 pixels: 0.34 of the 10 steps is 3.4 -> pixel 3, 0.36 is 3.6 -> pixel 4; a span of 1.4 is 1 px, of 1.6 is 2 px
    let cases: [(f32, f32, [usize; 11]); 4] = [
        (0.34, 1.4, [0, 1, 3, 3, 3, 5, 6, 7, 8, 9, 10]),
        (0.36, 1.4, [0, 1, 2, 4, 4, 4, 6, 7, 8, 9, 10]),
        (0.34, 1.6, [0, 3, 3, 3, 3, 3, 6, 7, 8, 9, 10]),
        (0.0, 3.0, [0, 0, 0, 0, 4, 5, 6, 7, 8, 9, 10]),
    ];
    for (at, span, expected) in cases {
        assert_eq!(
            numbers_after(&vertical(at, span, Side::Both), 11)?,
            expected,
            "at {at}, span {span}"
        );
    }
    assert_eq!(
        numbers_after(&vertical(1.0, 3.0, Side::Both), 11)?,
        [0, 1, 2, 3, 4, 5, 6, 10, 10, 10, 10]
    );
    Ok(())
}

#[test]
fn a_line_halfway_between_two_pixels_is_the_even_one_and_so_is_a_span_of_a_half() -> Result<(), Error> {
    // rounded to the even whole number, as NumPy and Python do: 4.5 -> 4 and 2.5 -> 2
    assert_eq!(
        numbers_after(&vertical(0.5, 1.0, Side::Both), 10)?,
        [0, 1, 2, 4, 4, 4, 6, 7, 8, 9]
    );
    assert_eq!(numbers_after(&vertical(0.0, 2.5, Side::Both), 6)?, [0, 0, 0, 3, 4, 5]);
    Ok(())
}

#[test]
fn no_span_changes_nothing_and_a_span_of_the_whole_side_gives_the_line_everywhere() -> Result<(), Error> {
    // the line at 0.5 of the way: pixel 0, 3 and 5 of 1, 7 and 11 pixels
    for (length, line) in [(1, 0), (7, 3), (11, 5)] {
        let untouched = numbers_after(&vertical(0.5, 0.0, Side::Both), length)?;
        assert_eq!(untouched, (0..length).collect::<Vec<_>>(), "span 0, length {length}");
        let whole = numbers_after(&vertical(0.5, 2000.0, Side::Both), length)?;
        assert_eq!(whole, vec![line; length], "span 2000, length {length}");
    }
    Ok(())
}

#[test]
fn a_side_that_is_not_smeared_keeps_the_picture_and_so_does_a_line_at_the_edge_it_smears_away_from() -> Result<(), Error>
{
    let length = 9;
    let identity: Vec<usize> = (0..length).collect();
    // before the first pixel there is nothing; after the last, nothing
    assert_eq!(numbers_after(&vertical(0.0, 5.0, Side::Before), length)?, identity);
    assert_eq!(numbers_after(&vertical(1.0, 5.0, Side::After), length)?, identity);
    // a side asked for smears; the other stays as it was
    let after = numbers_after(&vertical(0.5, 2.0, Side::After), length)?;
    assert_eq!(after[..=4], identity[..=4]);
    let before = numbers_after(&vertical(0.5, 2.0, Side::Before), length)?;
    assert_eq!(before[4..], identity[4..]);
    Ok(())
}

#[test]
fn a_constant_picture_stays_constant() -> Result<(), Error> {
    for (height, width) in [(1, 1), (1, 7), (7, 1), (5, 9)] {
        let flat = vec![0.37_f32; height * width * 3];
        for axis in [Axis::Vertical, Axis::Horizontal] {
            let params = Params {
                axis,
                span: 3.0,
                ..Params::default()
            };
            assert_eq!(
                first_difference(&run_picture(&params, height, width, &flat)?, &flat),
                None
            );
        }
    }
    Ok(())
}

/// `picture` (`height` x `width`) turned on its diagonal: a `width` x `height` picture.
fn transposed(picture: &[f32], height: usize, width: usize) -> Vec<f32> {
    (0..width)
        .flat_map(|row| (0..height).flat_map(move |column| picture[(column * width + row) * 3..][..3].to_vec()))
        .collect()
}

#[test]
fn the_horizontal_stretch_of_a_picture_is_the_vertical_one_of_its_transpose() -> Result<(), Error> {
    let (height, width) = (13, 8);
    let picture = varied(height, width);
    for (at, span, side) in [
        (0.5, 3.0, Side::Both),
        (0.2, 4.0, Side::After),
        (0.9, 2.0, Side::Before),
    ] {
        let across = Params {
            axis: Axis::Horizontal,
            at,
            span,
            side,
        };
        let down = Params {
            axis: Axis::Vertical,
            ..across.clone()
        };
        let across_out = run_picture(&across, height, width, &picture)?;
        let down_out = run_picture(&down, width, height, &transposed(&picture, height, width))?;
        let at_index = first_difference(&across_out, &transposed(&down_out, width, height));
        assert_eq!(at_index, None, "{across:?}");
    }
    Ok(())
}

#[test]
fn only_the_stretched_axis_moves_pixels_and_every_pixel_is_one_of_the_input() -> Result<(), Error> {
    // a pixel never leaves its column in a vertical stretch, nor its row in a horizontal one
    let (height, width) = (9, 6);
    let picture = varied(height, width);
    for axis in [Axis::Vertical, Axis::Horizontal] {
        let params = Params {
            axis,
            span: 3.0,
            ..Params::default()
        };
        let out = run_picture(&params, height, width, &picture)?;
        for at in 0..height * width {
            let (row, column) = (at / width, at % width);
            let along: Vec<usize> = match axis {
                Axis::Vertical => (0..height).map(|r| r * width + column).collect(),
                Axis::Horizontal => (0..width).map(|c| row * width + c).collect(),
            };
            assert!(
                along
                    .iter()
                    .any(|&source| picture[source * 3..][..3] == out[at * 3..][..3]),
                "{axis:?}: pixel ({row}, {column}) is no pixel of its line"
            );
        }
    }
    Ok(())
}

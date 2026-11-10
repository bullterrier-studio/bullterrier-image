//! `seams`: a line along the borders between pieces. On a black picture with a white line the coverage of each pixel is
//! its level; a border between two columns (or rows) of pixels is crossed by every row, so for a width of at least 1 px
//! the coverage of a row adds up to the line's width, laid symmetrically over the pixels either side, counted by hand.
//! A thinner line is fainter as well as narrower: its coverage is its width times the one a line of 1 px has.

use bullterrier_image::effects::seams;
use bullterrier_image::{Error, Labels};

use crate::common::pixels::run_with_partition;

const HEIGHT: usize = 6;
const WIDTH: usize = 10;

/// The white line of `width` over black, along the borders of `labels` (`pieces` of them): each pixel's coverage.
fn coverage(width: f32, labels: Vec<i32>, pieces: usize) -> Result<Vec<f32>, Error> {
    let labels = Labels::new(HEIGHT, WIDTH, labels, pieces)?;
    let params = seams::Params {
        width,
        ..Default::default()
    };
    let out = run_with_partition(&params, &labels, &[0.0; HEIGHT * WIDTH * 3])?;
    Ok(out.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect())
}

/// Two pieces, left of column 5 and from it on.
fn halves() -> Vec<i32> {
    (0..HEIGHT * WIDTH).map(|at| i32::from(at % WIDTH >= 5)).collect()
}

fn near(got: &[f32], wanted: &[f32]) -> bool {
    got.iter().zip(wanted).all(|(a, b)| (a - b).abs() < 1e-6)
}

#[test]
fn the_line_is_centred_on_the_border_and_covers_its_width_of_every_row() -> Result<(), Error> {
    // (width, the coverage of columns 2 to 7: the border lies between columns 4 and 5)
    for (width, wanted) in [
        (2.0, [0.0, 0.0, 1.0, 1.0, 0.0, 0.0]),
        (4.0, [0.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
        (3.0, [0.0, 0.5, 1.0, 1.0, 0.5, 0.0]),
        (6.0, [1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        // a hairline: half of each pixel beside the border
        (1.0, [0.0, 0.0, 0.5, 0.5, 0.0, 0.0]),
    ] {
        let cover = coverage(width, halves(), 2)?;
        for row in 0..HEIGHT {
            let line = &cover[row * WIDTH..][..WIDTH];
            assert!(near(&line[2..8], &wanted), "width {width}, row {row}: {line:?}");
            assert!(
                near(&[line[..2].iter().sum(), line[8..].iter().sum()], &[0.0, 0.0]),
                "{line:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_border_between_rows_is_drawn_across_the_rows_beside_it() -> Result<(), Error> {
    let upper_and_lower: Vec<i32> = (0..HEIGHT * WIDTH).map(|at| i32::from(at / WIDTH >= 3)).collect();
    let cover = coverage(2.0, upper_and_lower, 2)?;
    for (at, level) in cover.iter().enumerate() {
        let wanted = if (2..4).contains(&(at / WIDTH)) { 1.0 } else { 0.0 };
        assert!((level - wanted).abs() < 1e-6, "pixel {at}: {level}");
    }
    Ok(())
}

#[test]
fn one_piece_has_no_border_and_a_width_of_0_draws_nothing() -> Result<(), Error> {
    for (width, labels, pieces) in [(8.0, vec![0; HEIGHT * WIDTH], 1), (0.0, halves(), 2)] {
        let cover = coverage(width, labels, pieces)?;
        assert!(
            cover.iter().all(|level| *level == 0.0),
            "width {width}, {pieces} piece(s): {cover:?}"
        );
    }
    Ok(())
}

#[test]
fn a_line_thinner_than_a_pixel_covers_the_pixels_beside_the_border_by_its_width_squared_over_two() -> Result<(), Error>
{
    // by hand: the line reaches width / 2 into each pixel beside the border, and is fainter by its width, so each of the
    // two pixels is covered by width * width / 2
    for (width, each) in [(0.5, 0.125), (0.8, 0.32), (0.25, 0.031_25)] {
        let cover = coverage(width, halves(), 2)?;
        for row in 0..HEIGHT {
            let line = &cover[row * WIDTH..][..WIDTH];
            let wanted = [0.0, 0.0, 0.0, 0.0, each, each, 0.0, 0.0, 0.0, 0.0];
            assert!(near(line, &wanted), "width {width}, row {row}: {line:?}");
        }
    }
    Ok(())
}

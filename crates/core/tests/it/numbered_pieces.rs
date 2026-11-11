//! A partition drawn so its pieces can be told apart: dimmed, white lines on the borders, a number inside each piece.

use crate::common::pictures::varied;
use bullterrier_image::pipeline::number_pieces;
use bullterrier_image::{Error, Image, Labels};

/// A grid of `columns` equal columns over `height` x `width` pixels, numbered left to right.
fn columns(height: usize, width: usize, count: usize) -> Result<Labels, Error> {
    let labels = (0..height * width)
        .map(|index| i32::try_from((index % width) * count / width).unwrap_or(0))
        .collect();
    Labels::new(height, width, labels, count)
}

fn photo(height: usize, width: usize) -> Result<Image, Error> {
    Image::new(height, width, &varied(height, width))
}

#[test]
fn far_from_borders_and_numbers_the_photo_is_dimmed_and_the_borders_are_white() -> Result<(), Error> {
    let input = photo(96, 128)?;
    let out = number_pieces(&input, &columns(96, 128, 2)?)?;
    let (before, after) = (input.into_values(), out.into_values());
    let at = |row: usize, column: usize| (row * 128 + column) * 3;
    for column in 0..32 {
        let i = at(2, column);
        for channel in 0..3 {
            assert!((after[i + channel] - before[i + channel] * 0.35).abs() < 1e-6);
        }
    }
    for column in [63, 64] {
        let i = at(50, column);
        assert!(
            after[i..i + 3].iter().all(|v| (v - 1.0).abs() < 1e-6),
            "column {column} is beside the border"
        );
    }
    assert!(after[at(50, 61)..at(50, 61) + 3].iter().any(|v| (v - 1.0).abs() > 1e-6));
    Ok(())
}

#[test]
fn every_piece_gets_its_number_and_the_result_stays_in_range() -> Result<(), Error> {
    for (height, width, count) in [(1, 1, 1), (1, 7, 2), (7, 1, 1), (60, 600, 20)] {
        let out = number_pieces(&photo(height, width)?, &columns(height, width, count)?)?;
        assert_eq!((out.height(), out.width(), out.channels()), (height, width, 3));
        let values = out.into_values();
        assert!(values.iter().all(|v| (0.0..=1.0).contains(v)));
        if width == 600 {
            let piece = width / count;
            for number in 0..count {
                let inside =
                    (0..height).flat_map(|row| (number * piece + 3..number * piece + piece - 3).map(move |c| (row, c)));
                assert!(
                    inside.into_iter().any(|(row, c)| values[(row * width + c) * 3] > 0.9),
                    "piece {number}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn photo_and_pieces_of_different_sizes_are_refused() -> Result<(), Error> {
    assert!(number_pieces(&photo(4, 4)?, &columns(4, 5, 1)?).is_err());
    Ok(())
}

#[test]
fn a_number_is_centred_on_the_middle_of_its_piece() -> Result<(), Error> {
    let (height, width) = (60, 600);
    let black = Image::new(height, width, &vec![0.0; height * width * 3])?;
    let out = number_pieces(&black, &columns(height, width, 2)?)?.into_values();
    // piece 1 holds columns 300..600; its border with piece 0 is the column pair 299 and 300
    let ink: Vec<(usize, usize)> = (0..height)
        .flat_map(|row| (302..width).map(move |column| (row, column)))
        .filter(|&(row, column)| out[(row * width + column) * 3] > 0.5)
        .collect();
    assert_ne!(ink.len(), 0, "the number is drawn");
    let middle = |pick: fn(&(usize, usize)) -> usize, size: usize| {
        let (low, high) = (
            ink.iter().map(pick).min().unwrap_or(0),
            ink.iter().map(pick).max().unwrap_or(0),
        );
        (low + high).abs_diff(size) // twice the distance of the ink's middle from the piece's middle
    };
    assert!(middle(|&(row, _)| row, height - 1) <= 4, "rows");
    assert!(middle(|&(_, column)| column, 899) <= 4, "columns");
    Ok(())
}

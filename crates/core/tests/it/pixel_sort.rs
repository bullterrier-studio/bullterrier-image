//! `pixel_sort`: runs of pixels inside `low..=high` are put in order of their key and nothing else moves. Hand-worked
//! rows give the exact result; the properties (a line keeps its pixels, a column sort is a row sort turned) hold on a
//! varied picture.

use bullterrier_image::Error;
use bullterrier_image::effects::pixel_sort::{Axis, By, Order, Params};

use crate::common::compare::first_bit_difference;
use crate::common::pixels::run_picture;

/// Pseudo-random channels in 0..1 (a linear congruential generator), so hues and brightnesses vary along every line.
fn scattered(height: usize, width: usize) -> Vec<f32> {
    let mut state = 12_345_u32;
    (0..height * width * 3)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); // Numerical Recipes' constants
            f32::from(u16::try_from(state >> 16).unwrap_or(0)) / 65_535.0
        })
        .collect()
}

fn greys(levels: &[f32]) -> Vec<f32> {
    levels.iter().flat_map(|&level| [level; 3]).collect()
}

fn sorted_greys(levels: &[f32], order: Order, axis: Axis, low: f32, high: f32) -> Result<Vec<f32>, Error> {
    let params = Params {
        low,
        high,
        order,
        axis,
        ..Params::default()
    };
    let (height, width) = if axis == Axis::Horizontal { (1, levels.len()) } else { (levels.len(), 1) };
    let out = run_picture(&params, height, width, &greys(levels))?;
    Ok(out.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect())
}

/// One row, keys 0.9 | 0.5 0.3 | 0.1 | 0.6 0.4: 0.9 and 0.1 are outside 0.2..=0.8 and split two runs.
const ROW: [f32; 6] = [0.9, 0.5, 0.3, 0.1, 0.6, 0.4];

#[test]
fn runs_between_the_thresholds_are_ordered_and_the_rest_stays() -> Result<(), Error> {
    let cases = [
        (Order::Ascending, [0.9, 0.3, 0.5, 0.1, 0.4, 0.6]),
        (Order::Descending, ROW),
    ];
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for (order, expected) in cases {
            assert_eq!(sorted_greys(&ROW, order, axis, 0.2, 0.8)?, expected, "{axis:?} {order:?}");
        }
    }
    Ok(())
}

#[test]
fn the_thresholds_are_inclusive_and_crossed_ones_sort_nothing() -> Result<(), Error> {
    // 0.25 and 0.75 are exactly low and high, so they are inside and the whole row is one run
    let row = [0.75, 0.5, 0.25];
    assert_eq!(sorted_greys(&row, Order::Ascending, Axis::Horizontal, 0.25, 0.75)?, [0.25, 0.5, 0.75]);
    assert_eq!(sorted_greys(&row, Order::Ascending, Axis::Horizontal, 0.8, 0.2)?, row);
    Ok(())
}

#[test]
fn by_hue_equal_keys_keep_their_order() -> Result<(), Error> {
    // hues: blue 2/3, two reds 0 (one paler), green 1/3
    let pixels = [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [1.0, 0.5, 0.5], [0.0, 1.0, 0.0]];
    let picture: Vec<f32> = pixels.iter().flatten().copied().collect();
    let params = Params {
        by: By::Hue,
        low: 0.0,
        high: 1.0,
        ..Params::default()
    };
    let out = run_picture(&params, 1, 4, &picture)?;
    let expected: Vec<f32> = [pixels[1], pixels[2], pixels[3], pixels[0]].iter().flatten().copied().collect();
    assert_eq!(first_bit_difference(&out, &expected), None);
    Ok(())
}

/// Every line (row or column) of `values` as a sorted list of its pixels, for comparing what a line holds.
fn line_pixels(values: &[f32], height: usize, width: usize, axis: Axis) -> Vec<Vec<[u32; 3]>> {
    let pixel = |row: usize, column: usize| {
        let at = (row * width + column) * 3;
        [values[at].to_bits(), values[at + 1].to_bits(), values[at + 2].to_bits()]
    };
    let mut lines: Vec<Vec<[u32; 3]>> = match axis {
        Axis::Horizontal => (0..height).map(|row| (0..width).map(|column| pixel(row, column)).collect()).collect(),
        Axis::Vertical => (0..width).map(|column| (0..height).map(|row| pixel(row, column)).collect()).collect(),
    };
    for line in &mut lines {
        line.sort_unstable();
    }
    lines
}

#[test]
fn every_line_keeps_exactly_its_pixels() -> Result<(), Error> {
    let (height, width) = (13, 17);
    let picture = scattered(height, width);
    for by in [By::Brightness, By::Hue] {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            for order in [Order::Ascending, Order::Descending] {
                let params = Params {
                    by,
                    axis,
                    order,
                    low: 0.1,
                    high: 0.9,
                };
                let out = run_picture(&params, height, width, &picture)?;
                assert_eq!(
                    line_pixels(&out, height, width, axis),
                    line_pixels(&picture, height, width, axis),
                    "{params:?}"
                );
                assert!(first_bit_difference(&out, &picture).is_some(), "{params:?} sorted nothing");
            }
        }
    }
    Ok(())
}

#[test]
fn sorting_columns_is_sorting_rows_of_the_turned_picture() -> Result<(), Error> {
    let (height, width) = (9, 14);
    let picture = scattered(height, width);
    let turn = |values: &[f32], rows: usize, columns: usize| -> Vec<f32> {
        (0..columns)
            .flat_map(|column| (0..rows).flat_map(move |row| (0..3).map(move |c| (row, column, c))))
            .map(|(row, column, channel)| values[(row * columns + column) * 3 + channel])
            .collect()
    };
    let vertical = Params {
        axis: Axis::Vertical,
        ..Params::default()
    };
    let across = run_picture(&Params::default(), width, height, &turn(&picture, height, width))?;
    assert_eq!(
        first_bit_difference(&turn(&across, width, height), &run_picture(&vertical, height, width, &picture)?),
        None
    );
    Ok(())
}

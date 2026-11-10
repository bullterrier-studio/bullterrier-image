//! The noise that tears a border, seen through the borders of a torn grid: one column border wanders as the noise of the
//! x axis, one row border as that of the y axis. The shift of a border at a place is the noise there times `amount`, so
//! where the border lies in each row (or column) reads the noise at the border's place to the pixel.

use crate::common::compare::first_difference;
use crate::common::numbers::float;
use crate::common::ops::{cells, partition};
use bullterrier_image::Error;
use bullterrier_image::region::{Edge, EdgeStyle};

/// A torn edge of `amount` px at the design size, spaced `scale` px at the design size, seeded `seed`.
fn torn(amount: f64, scale: f64, octaves: u32, persistence: f64, seed: u32) -> Edge {
    Edge {
        style: EdgeStyle::Torn,
        amount,
        scale,
        seed,
        octaves,
        persistence,
    }
}

/// How many pixels of each row of a picture `width` wide are in the piece of its first pixel: where the border between two
/// columns is, in px.
fn border_by_row(labels: &[i32], width: usize) -> Vec<f64> {
    labels
        .chunks_exact(width)
        .map(|line| float(line.iter().filter(|&&label| label == line[0]).count()))
        .collect()
}

/// How many pixels of each column of a picture `height` x `width` are in the piece of its first pixel: where the border
/// between two rows is, in px.
fn border_by_column(labels: &[i32], height: usize, width: usize) -> Vec<f64> {
    (0..width)
        .map(|column| {
            let first = labels[column];
            float((0..height).filter(|&row| labels[row * width + column] == first).count())
        })
        .collect()
}

/// The mean and the standard deviation of `values`.
fn mean_and_deviation(values: &[f64]) -> (f64, f64) {
    let count = float(values.len());
    let mean = values.iter().sum::<f64>() / count;
    (
        mean,
        (values.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / count).sqrt(),
    )
}

#[test]
fn a_border_strays_by_the_amount_along_either_axis_whatever_the_layers_of_the_noise() -> Result<(), Error> {
    // 960 x 960 is twice the design size, so an amount of 4.8 design px is 9.6 px. The noise has deviation 1 at every
    // place, so over the picture the border's wander over the rows (columns) has about that deviation and the mean of the straight
    // border; the bends are slow enough that some twenty independent ones lie along the border: a wide margin
    for (octaves, persistence, scale) in [(1, 0.0, 24.0), (4, 0.55, 48.0), (3, 0.9, 96.0)] {
        let edge = torn(4.8, scale, octaves, persistence, 7);
        let (across, _) = partition((960, 960), None, cells(2, 1, edge.clone()))?;
        let (down, _) = partition((960, 960), None, cells(1, 2, edge))?;
        for (axis, borders) in [
            ("x", border_by_row(&across, 960)),
            ("y", border_by_column(&down, 960, 960)),
        ] {
            let (mean, deviation) = mean_and_deviation(&borders);
            assert!(
                (mean - 480.0).abs() < 9.6,
                "{axis}, {octaves} octaves: the border is at {mean}"
            );
            assert!(
                (deviation - 9.6).abs() < 0.4 * 9.6,
                "{axis}, {octaves} octaves: deviation {deviation}"
            );
        }
    }
    Ok(())
}

#[test]
fn the_two_axes_tear_with_different_noise() -> Result<(), Error> {
    // on a square picture the column border of one grid and the row border of another read the x and the y noise: were
    // they one field, the two borders would be the same line
    let edge = torn(24.0, 48.0, 3, 0.5, 9);
    let (across, _) = partition((240, 240), None, cells(2, 1, edge.clone()))?;
    let (down, _) = partition((240, 240), None, cells(1, 2, edge))?;
    assert_ne!(border_by_row(&across, 240), border_by_column(&down, 240, 240));
    Ok(())
}

#[test]
fn an_octave_finer_than_a_pixel_and_every_later_one_are_left_out() -> Result<(), Error> {
    // long side 40 px, so a design px is 1/12 px: a scale of 48 design px is 4 px, then 2 px, 1 px (kept: the limit
    // itself is a whole pixel), then 0.5 px, left out with every octave after it. The amount is the greatest, 48 design
    // px, 4 px.
    let tear = |octaves| partition((30, 40), None, cells(4, 3, torn(48.0, 48.0, octaves, 0.6, 5)));
    let (three, _) = tear(3)?;
    assert_eq!(
        first_difference(&tear(8)?.0, &three),
        None,
        "octaves under a pixel were summed"
    );
    assert!(
        first_difference(&tear(2)?.0, &three).is_some(),
        "the octave of exactly one pixel was left out"
    );
    Ok(())
}

#[test]
fn persistence_zero_leaves_only_the_coarsest_octave() -> Result<(), Error> {
    let tear =
        |octaves, persistence| partition((96, 128), None, cells(4, 3, torn(48.0, 192.0, octaves, persistence, 6)));
    let (coarse, _) = tear(1, 0.0)?;
    assert_eq!(first_difference(&tear(5, 0.0)?.0, &coarse), None);
    assert!(first_difference(&tear(5, 0.3)?.0, &coarse).is_some());
    Ok(())
}

#[test]
fn the_noise_between_lattice_points_is_smooth() -> Result<(), Error> {
    // a 200 px square, the noise spaced 50 px, the amount 48 design px = 20 px: neighbouring rows' borders differ by a
    // small part of that (white noise would differ by about 1.4 times it, 28 px)
    let (labels, _) = partition((200, 200), None, cells(2, 1, torn(48.0, 120.0, 1, 0.0, 8)))?;
    let borders = border_by_row(&labels, 200);
    let steepest = borders
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0_f64, f64::max);
    assert!(steepest < 8.0, "{steepest}");
    assert!(steepest > 0.0);
    Ok(())
}

#[test]
fn a_single_pixel_has_no_noise_to_tear_by() -> Result<(), Error> {
    let (labels, pieces) = partition((1, 1), None, cells(2, 2, torn(48.0, 1.0, 8, 0.55, 1)))?;
    assert_eq!((labels, pieces), (vec![0], 1));
    Ok(())
}

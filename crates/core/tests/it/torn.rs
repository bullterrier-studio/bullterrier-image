//! Regions with a torn edge, seen the way an effect sees them: through the public API alone, by the partition an effect
//! is handed (`common::ops`).

use bullterrier_image::Error;
use bullterrier_image::region::{Edge, EdgeStyle};

use crate::common::compare::first_difference;
use crate::common::ops::{cells, partition};
use crate::common::pieces::numbered_by_first_pixel;

/// A torn edge of `amount` px, spaced `scale` px, seeded `seed`, one octave, the rest of it as a recipe writes it.
fn torn(amount: f64, scale: f64, seed: u32) -> Edge {
    Edge {
        style: EdgeStyle::Torn,
        amount,
        scale,
        seed,
        octaves: 1,
        ..Edge::default()
    }
}

/// `count` as a float: a pixel count is far below 2^53.
#[expect(clippy::cast_precision_loss, reason = "a pixel count is far below 2^53")]
const fn float(count: usize) -> f64 {
    count as f64
}

/// For each row of a two-column grid of `width` px, how many pixels are in the piece of its first pixel: where the border
/// between the two columns is, in px.
fn border_by_row(labels: &[i32], width: usize) -> Vec<f64> {
    labels
        .chunks_exact(width)
        .map(|line| float(line.iter().filter(|&&label| label == line[0]).count()))
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
fn a_torn_edge_that_tears_by_nothing_cuts_the_straight_pieces() -> Result<(), Error> {
    let straight = partition((48, 64), None, cells(3, 2, Edge::default()))?;
    let nothing = partition((48, 64), None, cells(3, 2, torn(0.0, 24.0, 5)))?;
    assert_eq!(
        (first_difference(&straight.0, &nothing.0), straight.1),
        (None, nothing.1)
    );
    let torn_by_something = partition((48, 64), None, cells(3, 2, torn(12.0, 24.0, 5)))?;
    assert!(first_difference(&straight.0, &torn_by_something.0).is_some());
    Ok(())
}

#[test]
fn the_same_seed_tears_the_same_way_and_another_seed_differently() -> Result<(), Error> {
    let first = partition((96, 128), None, cells(2, 2, torn(12.0, 24.0, 1)))?;
    let again = partition((96, 128), None, cells(2, 2, torn(12.0, 24.0, 1)))?;
    let other = partition((96, 128), None, cells(2, 2, torn(12.0, 24.0, 2)))?;
    assert_eq!(first_difference(&first.0, &again.0), None);
    assert!(first_difference(&first.0, &other.0).is_some());
    Ok(())
}

#[test]
fn the_pieces_of_a_torn_grid_partition_the_picture_and_are_numbered_by_first_pixel() -> Result<(), Error> {
    let (labels, count) = partition((120, 160), None, cells(3, 2, torn(24.0, 24.0, 3)))?;
    assert_eq!(labels.len(), 120 * 160);
    assert!((1..=6).contains(&count), "{count}");
    // in reading order: each label first appears right after the greatest label before it
    assert!(numbered_by_first_pixel(&labels, count));
    Ok(())
}

#[test]
fn a_border_strays_by_about_the_amount_in_design_pixels_scaled_to_the_picture() -> Result<(), Error> {
    // a 240 x 240 picture (half as many px as the design size) and a 960 x 960 one (twice as many): the standard deviation
    // of the border's place over the rows is the amount (4.8 design px) in px of the picture. The bends are slow (24 design
    // px) so that a border never turns back on itself, which leaves some 20 independent bends along it: a wide margin
    for (side, expected) in [(240_usize, 2.4), (960, 9.6)] {
        let (labels, count) = partition((side, side), None, cells(2, 1, torn(4.8, 24.0, 4)))?;
        assert_eq!(count, 2);
        let (mean, deviation) = mean_and_deviation(&border_by_row(&labels, side));
        assert!(
            (mean - float(side) / 2.0).abs() < 0.5 * expected,
            "side {side}: the border is at {mean}"
        );
        assert!(
            (deviation - expected).abs() < 0.35 * expected,
            "side {side}: deviation {deviation}"
        );
    }
    Ok(())
}

#[test]
fn the_same_tear_comes_out_at_any_size() -> Result<(), Error> {
    // the picture rendered at 480 and at 960: the border, as a share of the width, is the same to within 1.5 px of 480 (the
    // noise is laid on the frame; only the pixels it is read at differ)
    let edge = Edge {
        octaves: 3,
        ..torn(4.8, 24.0, 6)
    };
    let (small, _) = partition((960, 960), Some(480), cells(2, 1, edge.clone()))?;
    let (large, _) = partition((960, 960), Some(960), cells(2, 1, edge))?;
    let (small, large) = (border_by_row(&small, 480), border_by_row(&large, 960));
    let worst = (0..480)
        .map(|row| (small[row] - (large[2 * row] + large[2 * row + 1]) / 4.0).abs())
        .fold(0.0_f64, f64::max);
    assert!(worst < 1.5, "the borders are {worst} px apart");
    Ok(())
}

#[test]
fn the_largest_tear_on_a_thin_picture_finishes_with_whole_pieces() -> Result<(), Error> {
    let edge = Edge {
        octaves: 8,
        ..torn(48.0, 1.0, 9)
    };
    let (labels, count) = partition((3, 2000), None, cells(4, 2, edge))?;
    assert_eq!(labels.len(), 3 * 2000);
    assert!((1..=8).contains(&count));
    Ok(())
}

//! Regions with a torn edge, seen the way an effect sees them: through the public API alone, with an effect of the test's
//! own that logs the partition it is handed.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::region::{Edge, EdgeStyle, Grid, Shares};
use bullterrier_image::{
    Category, Effect, Error, Image, Input, Inputs, Options, Photo, Picture, Presets, Recipe, Step, register,
};

/// The partition a [`Spy`] was handed: its labels and number of pieces.
type Pieces = (Vec<i32>, usize);

/// An effect that logs the partition it is handed and returns the picture as it is.
struct Spy {
    name: String,
    log: Mutex<Vec<Pieces>>,
}

impl Effect for Spy {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        &[Input::Partition]
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        if let Some(labels) = inputs.partition() {
            self.log
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((labels.values().to_vec(), labels.pieces()));
        }
        Ok(picture.map(|pixel| pixel))
    }
}

/// The labels and number of pieces of `grid` on a grey photo of `height` x `width` pixels, rendered at `size` (the
/// longest side) if given.
fn pieces(height: usize, width: usize, size: Option<u32>, grid: Grid) -> Result<Pieces, Error> {
    static SPIES: AtomicUsize = AtomicUsize::new(0);
    let spy = Arc::new(Spy {
        name: format!("torn_spy_{}", SPIES.fetch_add(1, Ordering::Relaxed)),
        log: Mutex::new(Vec::new()),
    });
    register(spy.clone())?;
    let pixels = vec![0.5; height * width * 3];
    let photo = Photo::new(Picture::new(height, width, &pixels)?);
    let mut step = Step::new(&spy.name);
    step.partition = Some("g".to_owned());
    let recipe = Recipe::new([step]).with_region("g", grid);
    let options = size.map_or_else(Options::default, |size| Options::default().with_size(size));
    photo.render(&recipe, &Presets::new(), options)?;
    let mut seen = spy.log.lock().unwrap_or_else(PoisonError::into_inner);
    seen.pop()
        .ok_or_else(|| Error::Recipe("the spy was handed no partition".to_owned()))
}

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

/// A grid of `columns` x `rows` equal cells with `edge`.
fn cells(columns: usize, rows: usize, edge: Edge) -> Grid {
    Grid {
        edge,
        columns: Shares::equal(columns),
        rows: Shares::equal(rows),
        ..Grid::default()
    }
}

/// `count` as a float: a pixel count is far below 2^53.
#[expect(clippy::cast_precision_loss, reason = "a pixel count is far below 2^53")]
const fn float(count: usize) -> f64 {
    count as f64
}

/// A number of pieces as a label bound, saturating.
fn bound(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// The first index at which the labels differ, if they do (a mismatch of a whole picture is never printed).
fn first_difference(left: &[i32], right: &[i32]) -> Option<usize> {
    if left.len() != right.len() {
        return Some(left.len().min(right.len()));
    }
    left.iter().zip(right).position(|(one, other)| one != other)
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
    let straight = pieces(48, 64, None, cells(3, 2, Edge::default()))?;
    let nothing = pieces(48, 64, None, cells(3, 2, torn(0.0, 24.0, 5)))?;
    assert_eq!(
        (first_difference(&straight.0, &nothing.0), straight.1),
        (None, nothing.1)
    );
    let torn_by_something = pieces(48, 64, None, cells(3, 2, torn(12.0, 24.0, 5)))?;
    assert!(first_difference(&straight.0, &torn_by_something.0).is_some());
    Ok(())
}

#[test]
fn the_same_seed_tears_the_same_way_and_another_seed_differently() -> Result<(), Error> {
    let first = pieces(96, 128, None, cells(2, 2, torn(12.0, 24.0, 1)))?;
    let again = pieces(96, 128, None, cells(2, 2, torn(12.0, 24.0, 1)))?;
    let other = pieces(96, 128, None, cells(2, 2, torn(12.0, 24.0, 2)))?;
    assert_eq!(first_difference(&first.0, &again.0), None);
    assert!(first_difference(&first.0, &other.0).is_some());
    Ok(())
}

#[test]
fn the_pieces_of_a_torn_grid_partition_the_picture_and_are_numbered_by_first_pixel() -> Result<(), Error> {
    let (labels, count) = pieces(120, 160, None, cells(3, 2, torn(24.0, 24.0, 3)))?;
    assert_eq!(labels.len(), 120 * 160);
    assert!((1..=6).contains(&count), "{count}");
    // in reading order: each label first appears right after the greatest label before it
    let mut greatest = -1;
    for &label in &labels {
        assert!(
            label <= greatest + 1 && (0..bound(count)).contains(&label),
            "label {label} after {greatest}"
        );
        greatest = greatest.max(label);
    }
    assert_eq!(greatest + 1, bound(count));
    Ok(())
}

#[test]
fn a_border_strays_by_about_the_amount_in_design_pixels_scaled_to_the_picture() -> Result<(), Error> {
    // a 240 x 240 picture (half as many px as the design size) and a 960 x 960 one (twice as many): the standard deviation
    // of the border's place over the rows is the amount (4.8 design px) in px of the picture. The bends are slow (24 design
    // px) so that a border never turns back on itself, which leaves some 20 independent bends along it: a wide margin
    for (side, expected) in [(240_usize, 2.4), (960, 9.6)] {
        let (labels, count) = pieces(side, side, None, cells(2, 1, torn(4.8, 24.0, 4)))?;
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
    let (small, _) = pieces(960, 960, Some(480), cells(2, 1, edge.clone()))?;
    let (large, _) = pieces(960, 960, Some(960), cells(2, 1, edge))?;
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
    let (labels, count) = pieces(3, 2000, None, cells(4, 2, edge))?;
    assert_eq!(labels.len(), 3 * 2000);
    assert!((1..=8).contains(&count));
    Ok(())
}

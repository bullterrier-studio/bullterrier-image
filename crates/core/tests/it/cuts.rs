//! `cuts` regions, seen the way an effect sees them: through the public API alone, with an effect of the test's own that
//! logs the partition it is handed.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::region::{Curve, Cuts, Edge, EdgeStyle, Line};
use bullterrier_image::{
    Category, Effect, Error, Image, Input, Inputs, Layer, Options, Photo, Picture, Presets, Recipe, Step, register,
};

use crate::common::compare::first_difference;

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

/// The labels and number of pieces of `cuts` on a grey photo of `height` x `width` pixels, with the map `subject` if given.
fn pieces(height: usize, width: usize, subject: Option<Vec<f32>>, cuts: Cuts) -> Result<Pieces, Error> {
    static SPIES: AtomicUsize = AtomicUsize::new(0);
    let spy = Arc::new(Spy {
        name: format!("cuts_spy_{}", SPIES.fetch_add(1, Ordering::Relaxed)),
        log: Mutex::new(Vec::new()),
    });
    register(spy.clone())?;
    let pixels = vec![0.5; height * width * 3];
    let mut photo = Photo::new(Picture::new(height, width, &pixels)?);
    if let Some(values) = subject {
        photo = photo.with_map("subject", Layer::new(height, width, values)?)?;
    }
    let mut step = Step::new(&spy.name);
    step.partition = Some("c".to_owned());
    let recipe = Recipe::new([step]).with_region("c", cuts);
    photo.render(&recipe, &Presets::new(), Options::default())?;
    let mut seen = spy.log.lock().unwrap_or_else(PoisonError::into_inner);
    seen.pop()
        .ok_or_else(|| Error::Recipe("the spy was handed no partition".to_owned()))
}

/// The pieces of a picture as one line of digits for each row, rows joined by `/`: labels below 10 only.
fn drawn(labels: &[i32], width: usize) -> String {
    labels
        .chunks_exact(width)
        .map(|row| row.iter().map(ToString::to_string).collect::<String>())
        .collect::<Vec<_>>()
        .join("/")
}

fn line(x: f64, y: f64, angle: f64) -> Line {
    Line { x, y, angle }
}

fn cuts(lines: impl IntoIterator<Item = Line>) -> Cuts {
    Cuts {
        lines: lines.into_iter().map(Into::into).collect(),
        ..Cuts::default()
    }
}

fn curve(path: &str) -> Cuts {
    Cuts {
        lines: vec![Curve { path: path.to_owned() }.into()],
        ..Cuts::default()
    }
}

/// A number of pieces as a label bound, saturating.
fn bound(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// Whether the labels are the pieces' numbers in reading order: each label first appears right after the greatest before it,
/// and all `count` appear.
fn numbered_by_first_pixel(labels: &[i32], count: usize) -> bool {
    let mut greatest = -1;
    for &label in labels {
        if label > greatest + 1 || !(0..bound(count)).contains(&label) {
            return false;
        }
        greatest = greatest.max(label);
    }
    greatest + 1 == bound(count)
}

#[test]
fn a_line_divides_the_pixels_whose_centres_lie_on_either_side_of_it() -> Result<(), Error> {
    // a 6 x 4 picture, centres at (i + 1/2, j + 1/2); the line passes (3, 2) unless the row says otherwise. Which side is
    // piece 0 is the side of the first pixel, so a line and the one 180 degrees further cut alike
    let (past_the_middle, row_two_centre) = (0.5625, 0.625);
    let rows = [
        (
            "0 degrees: above and below",
            line(0.5, 0.5, 0.0),
            "000000/000000/111111/111111",
        ),
        (
            "180 degrees cuts alike",
            line(0.5, 0.5, 180.0),
            "000000/000000/111111/111111",
        ),
        (
            "90 degrees: left and right",
            line(0.5, 0.5, 90.0),
            "000111/000111/000111/000111",
        ),
        (
            "270 degrees cuts alike",
            line(0.5, 0.5, 270.0),
            "000111/000111/000111/000111",
        ),
        (
            "a full turn is no turn",
            line(0.5, 0.5, 360.0),
            "000000/000000/111111/111111",
        ),
        // y = 2.25: a pixel is on the first pixel's side when its column is at most its row
        (
            "45 degrees falls to the right",
            line(0.5, past_the_middle, 45.0),
            "011111/001111/000111/000011",
        ),
        // y = 2.5 is the centre of row 2: a centre exactly on the line is on the side of the picture's top
        (
            "a centre on the line",
            line(0.5, row_two_centre, 0.0),
            "000000/000000/000000/111111",
        ),
    ];
    for (what, cut, expected) in rows {
        let (labels, count) = pieces(4, 6, None, cuts([cut]))?;
        assert_eq!((drawn(&labels, 6).as_str(), count), (expected, 2), "{what}");
    }
    Ok(())
}

#[test]
fn cuts_that_cross_nowhere_make_one_piece_more_than_there_are_and_crossing_ones_more() -> Result<(), Error> {
    let parallel = cuts([line(0.5, 0.2, 0.0), line(0.5, 0.5, 0.0), line(0.5, 0.8, 0.0)]);
    assert_eq!(pieces(40, 60, None, parallel)?.1, 4);
    let crossing = cuts([line(0.5, 0.5, 0.0), line(0.5, 0.5, 90.0)]);
    let (labels, count) = pieces(4, 6, None, crossing)?;
    assert_eq!((drawn(&labels, 6).as_str(), count), ("000111/000111/222333/222333", 4));
    Ok(())
}

#[test]
fn the_most_cuts_cross_into_pieces_numbered_by_first_pixel_without_a_gap() -> Result<(), Error> {
    // 16 horizontal and 16 vertical lines on 96 x 64: 17 x 17 pieces, with 32 bits of the piece's key in use
    let lines = (1..=16).flat_map(|at| {
        let share = f64::from(at) / 17.0;
        [line(0.5, share, 0.0), line(share, 0.5, 90.0)]
    });
    let (labels, count) = pieces(64, 96, None, cuts(lines))?;
    assert_eq!(count, 17 * 17);
    assert!(numbered_by_first_pixel(&labels, count));
    Ok(())
}

#[test]
fn a_curve_closed_along_the_border_cuts_off_one_side_of_the_frame() -> Result<(), Error> {
    // the frame is 6 x 4 px, so the curves, in shares of it, run in px (3, 0) to (3, 4) and so on
    let rows = [
        ("top to bottom", "M 0.5 0 L 0.5 1", "000111/000111/000111/000111"),
        // the end at (3, 1) is carried straight up to the top border
        (
            "an end inside the frame",
            "M 0.5 0.25 L 0.5 1",
            "000111/000111/000111/000111",
        ),
        // (0, 2) to (3, 4): the corner (0, 4) is added; the triangle below the cut is piece 1
        ("adjacent borders", "M 0 0.5 L 0.5 1", "000000/000000/100000/110000"),
        ("the other way round", "M 0.5 1 L 0 0.5", "000000/000000/100000/110000"),
    ];
    for (what, path, expected) in rows {
        let (labels, count) = pieces(4, 6, None, curve(path))?;
        assert_eq!((drawn(&labels, 6).as_str(), count), (expected, 2), "{what}");
    }
    Ok(())
}

#[test]
fn a_curve_and_a_line_that_cross_make_four_pieces() -> Result<(), Error> {
    let both = Cuts {
        lines: vec![
            line(0.5, 0.5, 0.0).into(),
            Curve {
                path: "M 0.5 0 L 0.5 1".to_owned(),
            }
            .into(),
        ],
        ..Cuts::default()
    };
    let (labels, count) = pieces(4, 6, None, both)?;
    assert_eq!((drawn(&labels, 6).as_str(), count), ("000111/000111/222333/222333", 4));
    Ok(())
}

#[test]
fn cuts_are_laid_over_the_frame_of_their_fit_not_the_picture() -> Result<(), Error> {
    // the subject is columns 8..40 of rows 6..30 of a 64 x 48 picture; a vertical line through the middle of that frame
    // passes column 24
    let subject: Vec<f32> = (0..48 * 64)
        .map(|at| f32::from(u8::from((6..30).contains(&(at / 64)) && (8..40).contains(&(at % 64)))))
        .collect();
    let framed = Cuts {
        fit: Some("subject".to_owned()),
        ..cuts([line(0.5, 0.5, 90.0)])
    };
    let (labels, _) = pieces(48, 64, Some(subject), framed)?;
    assert!((0..48).all(|row| labels[row * 64..row * 64 + 24].iter().all(|&label| label == 0)));
    assert!((0..48).all(|row| labels[row * 64 + 24..(row + 1) * 64].iter().all(|&label| label == 1)));
    Ok(())
}

fn torn(seed: u32, amount: f64) -> Edge {
    Edge {
        style: EdgeStyle::Torn,
        amount,
        scale: 24.0,
        seed,
        octaves: 1,
        ..Edge::default()
    }
}

#[test]
fn a_torn_cut_wanders_by_seed_and_tears_the_straight_cut_by_nothing() -> Result<(), Error> {
    let with = |lines: Vec<_>, edge| Cuts {
        edge,
        lines,
        ..Cuts::default()
    };
    for lines in [
        vec![line(0.5, 0.5, 90.0).into()],
        vec![
            Curve {
                path: "M 0.5 0 L 0.5 1".to_owned(),
            }
            .into(),
        ],
    ] {
        let straight = pieces(96, 128, None, with(lines.clone(), Edge::default()))?;
        let nothing = pieces(96, 128, None, with(lines.clone(), torn(1, 0.0)))?;
        let first = pieces(96, 128, None, with(lines.clone(), torn(1, 12.0)))?;
        let again = pieces(96, 128, None, with(lines.clone(), torn(1, 12.0)))?;
        let other = pieces(96, 128, None, with(lines, torn(2, 12.0)))?;
        assert_eq!(first_difference(&straight.0, &nothing.0), None);
        assert_eq!(first_difference(&first.0, &again.0), None);
        assert!(first_difference(&straight.0, &first.0).is_some());
        assert!(first_difference(&first.0, &other.0).is_some());
        assert!(numbered_by_first_pixel(&first.0, first.1));
    }
    Ok(())
}

#[test]
fn a_torn_line_and_a_torn_curve_along_it_cut_the_same_pieces() -> Result<(), Error> {
    // a straight curve and the line it lies on are two routes to one cut: the pixel holding each bent centre is on the same
    // side of both, so the line decides each centre and the curve the pixel holding it
    for (angle, path) in [(0.0, "M 0 0.5 L 1 0.5"), (90.0, "M 0.5 0 L 0.5 1")] {
        let torn_cut = |cuts: Cuts| Cuts {
            edge: torn(3, 12.0),
            ..cuts
        };
        let by_line = pieces(96, 128, None, torn_cut(cuts([line(0.5, 0.5, angle)])))?;
        let by_curve = pieces(96, 128, None, torn_cut(curve(path)))?;
        assert_eq!(by_line.1, 2);
        assert_eq!(first_difference(&by_line.0, &by_curve.0), None, "{angle} degrees");
    }
    Ok(())
}

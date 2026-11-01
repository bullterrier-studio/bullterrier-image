//! `path` regions, seen the way an effect sees them: through the public API alone, with an effect of the test's own that
//! logs the selection it is handed.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::region::{Edge, EdgeStyle, Path};
use bullterrier_image::{
    Category, Effect, Error, ErrorKind, FieldParam, Image, Inputs, Layer, Options, Photo, Picture, Presets, Recipe,
    Step, register,
};

/// An effect that logs the selection `near` it is handed and returns the picture as it is.
struct Spy {
    name: String,
    fields: Vec<FieldParam>,
    log: Mutex<Vec<Vec<f32>>>,
}

impl Effect for Spy {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn fields(&self) -> &[FieldParam] {
        &self.fields
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        if let Some(layer) = inputs.field("near") {
            self.log
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(layer.values().to_vec());
        }
        Ok(picture.map(|pixel| pixel))
    }
}

/// The selection a step takes from the region `r`, a `path` declared as `path`, on a grey photo of `height` x `width`
/// pixels with the map `subject` if given.
fn selected(height: usize, width: usize, subject: Option<Vec<f32>>, path: Path) -> Result<Vec<f32>, Error> {
    static SPIES: AtomicUsize = AtomicUsize::new(0);
    let spy = Arc::new(Spy {
        name: format!("path_spy_{}", SPIES.fetch_add(1, Ordering::Relaxed)),
        fields: vec![FieldParam::new("near", "r")],
        log: Mutex::new(Vec::new()),
    });
    register(spy.clone())?;
    let pixels = vec![0.5; height * width * 3];
    let mut photo = Photo::new(Picture::new(height, width, &pixels)?);
    if let Some(values) = subject {
        photo = photo.with_map("subject", Layer::new(height, width, values)?)?;
    }
    let recipe = Recipe::new([Step::new(&spy.name)]).with_region("r", path);
    photo.render(&recipe, &Presets::new(), Options::default())?;
    let mut seen = spy.log.lock().unwrap_or_else(PoisonError::into_inner);
    seen.pop()
        .ok_or_else(|| Error::Recipe("the spy was handed no selection".to_owned()))
}

fn path(d: &str) -> Path {
    Path {
        d: d.to_owned(),
        ..Path::default()
    }
}

/// The first index at which the values differ, if they do (a mismatch of a whole picture is never printed).
fn first_difference(left: &[f32], right: &[f32]) -> Option<usize> {
    if left.len() != right.len() {
        return Some(left.len().min(right.len()));
    }
    left.iter()
        .zip(right)
        .position(|(one, other)| one.to_bits() != other.to_bits())
}

/// The sum of `values` in double precision.
fn total(values: &[f32]) -> f64 {
    values.iter().map(|&value| f64::from(value)).sum()
}

#[test]
fn a_path_selects_what_is_inside_it_fully_and_what_is_outside_not_at_all() -> Result<(), Error> {
    // the square from 1/4 to 3/4 of a 64 x 48 frame is columns 16..48 of rows 12..36: pixel aligned
    let values = selected(48, 64, None, path("M 0.25 0.25 H 0.75 V 0.75 H 0.25 Z"))?;
    let expected: Vec<f32> = (0..48 * 64)
        .map(|at| f32::from(u8::from((12..36).contains(&(at / 64)) && (16..48).contains(&(at % 64)))))
        .collect();
    assert_eq!(first_difference(&values, &expected), None);
    Ok(())
}

#[test]
fn a_path_is_laid_over_the_frame_of_its_fit_not_the_picture() -> Result<(), Error> {
    // the subject is the rectangle of columns 8..40, rows 6..30: a path of the whole frame selects exactly that box
    let subject: Vec<f32> = (0..48 * 64)
        .map(|at| f32::from(u8::from((6..30).contains(&(at / 64)) && (8..40).contains(&(at % 64)))))
        .collect();
    let framed = Path {
        fit: Some("subject".to_owned()),
        ..Path::default()
    };
    let values = selected(48, 64, Some(subject.clone()), framed)?;
    assert_eq!(first_difference(&values, &subject), None);
    // and a path of the left half of that frame selects columns 8..24
    let half = Path {
        fit: Some("subject".to_owned()),
        d: "M 0 0 H 0.5 V 1 H 0 Z".to_owned(),
        ..Path::default()
    };
    let values = selected(48, 64, Some(subject), half)?;
    let expected: Vec<f32> = (0..48 * 64)
        .map(|at| f32::from(u8::from((6..30).contains(&(at / 64)) && (8..24).contains(&(at % 64)))))
        .collect();
    assert_eq!(first_difference(&values, &expected), None);
    Ok(())
}

#[test]
fn a_border_pixel_is_covered_by_the_share_of_it_inside_and_more_scanlines_measure_a_slanting_border_better()
-> Result<(), Error> {
    // a triangle of base 0.8 x 64 = 51.2 px and height 0.8 x 48 = 38.4 px: area 983.04 px
    let exact = 0.5 * 51.2 * 38.4;
    let triangle = |samples: u32| {
        selected(
            48,
            64,
            None,
            Path {
                samples,
                ..path("M 0.1 0.1 L 0.9 0.1 L 0.5 0.9 Z")
            },
        )
    };
    for (samples, within) in [(1, 0.05), (4, 0.02), (16, 0.005)] {
        let values = triangle(samples)?;
        assert!(values.iter().all(|value| (0.0..=1.0).contains(value)));
        let error = (total(&values) - exact).abs() / exact;
        assert!(
            error < within,
            "samples {samples}: the area is {} against {exact}",
            total(&values)
        );
    }
    Ok(())
}

#[test]
fn a_torn_path_is_the_same_for_a_seed_and_differs_for_another_and_a_tear_of_nothing_is_straight() -> Result<(), Error> {
    let torn = |amount: f64, seed: u32| Path {
        edge: Edge {
            style: EdgeStyle::Torn,
            amount,
            scale: 12.0,
            seed,
            ..Edge::default()
        },
        ..path("M 0.2 0.2 H 0.8 V 0.8 H 0.2 Z")
    };
    let straight = selected(96, 128, None, path("M 0.2 0.2 H 0.8 V 0.8 H 0.2 Z"))?;
    let first = selected(96, 128, None, torn(8.0, 1))?;
    assert_eq!(first_difference(&first, &selected(96, 128, None, torn(8.0, 1))?), None);
    assert!(first_difference(&first, &selected(96, 128, None, torn(8.0, 2))?).is_some());
    assert!(first_difference(&first, &straight).is_some());
    assert_eq!(
        first_difference(&straight, &selected(96, 128, None, torn(0.0, 1))?),
        None
    );
    // read between the pixels, the coverage stays a coverage
    assert!(first.iter().all(|value| (0.0..=1.0).contains(value)));
    Ok(())
}

#[test]
fn path_data_that_is_no_path_is_refused_naming_the_region_before_anything_runs() {
    let rows = [
        ("", "empty"),
        ("L 0.5 0.5", "start with M"),
        ("M 0 0 L foo", "valid SVG path data"),
        ("M 0 0 X 1 1", "valid SVG path data"),
        ("M 0 0 L 1", "valid SVG path data"),
        ("M 0 0 L 1e999 0", "valid SVG path data"),
        ("M 0 0 C 0 0 1e9 1e9 0 0", "too far"),
    ];
    for (d, mentions) in rows {
        let refused = selected(8, 8, None, path(d));
        assert!(
            matches!(&refused, Err(error) if error.kind() == ErrorKind::Recipe
                && error.to_string().contains("region 'r'") && error.to_string().contains(mentions)),
            "{d:?}: {refused:?}"
        );
    }
}

#[test]
fn a_curve_that_the_frame_stretches_to_too_many_chords_is_refused_when_the_region_is_made() {
    // in the unit frame a control point at 5000 needs a few hundred chords; in a 400 px frame, thousands
    let d = "M 0 0 C 0 0 5000 5000 0 0";
    let refused = selected(300, 400, None, path(d));
    assert!(
        matches!(&refused, Err(error) if error.to_string().contains("region 'r'") && error.to_string().contains("chords")),
        "{refused:?}"
    );
}

#[test]
fn a_slanting_path_is_selected_at_any_number_of_samples_and_size_with_every_value_from_0_to_1() -> Result<(), Error> {
    // the coverage of a pixel is a difference of running sums, which for an empty pixel next to a full one can come out a
    // hair below 0; a selection holds 0 to 1, so such a render used to be refused
    let shapes = [
        "M 0.5 0.05 L 0.95 0.95 L 0.05 0.95 Z",
        "M 0.5 0.02 L 0.61 0.35 L 0.98 0.35 L 0.68 0.57 L 0.79 0.91 L 0.5 0.7 L 0.21 0.91 L 0.32 0.57 L 0.02 0.35 L 0.39 0.35 Z",
    ];
    for d in shapes {
        for samples in [1, 2, 3, 5, 7, 16] {
            for (height, width) in [(3, 3), (5, 7), (16, 9), (40, 40)] {
                let region = Path { samples, ..path(d) };
                let values = selected(height, width, None, region)?;
                assert!(
                    values.iter().all(|value| (0.0..=1.0).contains(value)),
                    "{d} at {samples} samples on {height} x {width}"
                );
            }
        }
    }
    Ok(())
}

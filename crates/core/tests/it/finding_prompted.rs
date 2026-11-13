//! Finding what is clicked, through the public API: what goes into a promptable model (the photo, the clicks and the box in its
//! own 1024 space) and what a map comes out of its candidates. The model is the one external boundary, so a stand-in
//! `Segment` answers; the real model runs in `tests/it/finding_onnx.rs`.

use std::sync::Mutex;

use bullterrier_image::finding::prompted::{Answer, MAX_POINTS, Point, PromptedFinder, Prompts, Query, Segment};
use bullterrier_image::finding::{SAM2_DECODER, SAM2_ENCODER, Tensor, models};
use bullterrier_image::{Error, ErrorKind, Finder, Picture};

use crate::common::Outcome;

/// The side of the model's logits, px.
const GRID: usize = 256;

type Candidates = Box<dyn Fn(&Query) -> Vec<(f32, Vec<f32>)> + Send + Sync>;

/// A stand-in model that keeps what it was asked and answers with `candidates(query)`: (score, logits) each.
struct Stand {
    seen: Mutex<Vec<Query>>,
    candidates: Candidates,
}

impl Stand {
    fn new(candidates: impl Fn(&Query) -> Vec<(f32, Vec<f32>)> + Send + Sync + 'static) -> &'static Self {
        Box::leak(Box::new(Self {
            seen: Mutex::new(Vec::new()),
            candidates: Box::new(candidates),
        }))
    }

    fn asked(&self) -> Result<Query, &'static str> {
        self.seen
            .lock()
            .map_err(|_| "poisoned")?
            .first()
            .cloned()
            .ok_or("the model was not asked")
    }
}

impl Segment for &'static Stand {
    fn segment(&self, query: &Query) -> Result<Answer, Error> {
        self.seen
            .lock()
            .map_err(|_| Error::BackendUnavailable("poisoned".into()))?
            .push(query.clone());
        let candidates = (self.candidates)(query);
        let scores = candidates.iter().map(|(score, _)| *score).collect();
        let logits = candidates
            .into_iter()
            .flat_map(|(_, logits)| logits)
            .collect::<Vec<_>>();
        Ok(Answer {
            scores,
            logits: Tensor::new(vec![logits.len() / (GRID * GRID), GRID, GRID], logits)?,
        })
    }
}

/// A place on the logit grid, px.
fn grid_px(index: usize) -> f32 {
    f32::from(u8::try_from(index).unwrap_or(u8::MAX))
}

/// Logits that are `inside` within `half` grid px (a square) of each place in `at` and `outside` elsewhere.
fn squares(at: &[(f32, f32)], half: f32, inside: f32, outside: f32) -> Vec<f32> {
    (0..GRID * GRID)
        .map(|index| {
            let (row, column) = (grid_px(index / GRID), grid_px(index % GRID));
            if at
                .iter()
                .any(|&(x, y)| (column - x).abs() <= half && (row - y).abs() <= half)
            {
                inside
            } else {
                outside
            }
        })
        .collect()
}

/// A model that is wrong in the ways a bad reading of the query would show: its best candidate is a square round the
/// first positive click (leaving out a square round each negative one); another is everything, another nothing.
fn model() -> &'static Stand {
    Stand::new(|query| {
        let grid = |point: &[f32; 2]| (point[0] / 4.0, point[1] / 4.0);
        let positive: Vec<_> = (0..query.points.len())
            .filter(|&at| query.labels[at] == 1)
            .map(|at| grid(&query.points[at]))
            .collect();
        let negative: Vec<_> = (0..query.points.len())
            .filter(|&at| query.labels[at] == 0)
            .map(|at| grid(&query.points[at]))
            .collect();
        let mut best = squares(&positive[..positive.len().min(1)], 20.0, 8.0, -8.0);
        let hole = squares(&negative, 20.0, -8.0, 8.0);
        for (value, cut) in best.iter_mut().zip(hole) {
            if cut < 0.0 {
                *value = -8.0;
            }
        }
        vec![
            (0.1, vec![8.0; GRID * GRID]),
            (0.9, best),
            (0.5, vec![-8.0; GRID * GRID]),
        ]
    })
}

fn point(x: f64, y: f64, positive: bool) -> Point {
    Point { x, y, positive }
}

fn grey(height: usize, width: usize, value: f32) -> Vec<f32> {
    vec![value; height * width * 3]
}

/// The matte `finder` finds on a grey photo of `height` x `width`, with no edge clean-up.
fn matte(stand: &'static Stand, prompts: Prompts, height: usize, width: usize) -> Result<Vec<f32>, Error> {
    let values = grey(height, width, 0.5);
    Ok(PromptedFinder::new(prompts, stand)
        .with_edge(0.0, 0.0)
        .find(&Picture::new(height, width, &values)?)?
        .values()
        .to_vec())
}

#[test]
fn the_catalogue_lists_both_halves_of_sam_2_with_the_weights_files_beside_them() {
    for model in [SAM2_ENCODER, SAM2_DECODER] {
        assert!(models().contains(&model), "{}", model.name);
        let data = model
            .data
            .map(|data| (data.sha256.len(), data.url.starts_with("https://"), data.size));
        assert!(
            matches!(data, Some((64, true, size)) if size > 1_000_000),
            "{}",
            model.name
        );
    }
}

#[test]
fn a_click_is_given_to_the_model_as_pixels_of_its_square_across_then_down() -> Outcome {
    let stand = model();
    // 40 rows x 60 columns: a quarter of the way across and three quarters down
    matte(stand, Prompts::new(vec![point(0.25, 0.75, true)], None)?, 40, 60)?;
    let asked = stand.asked()?;
    assert_eq!(asked.points, vec![[256.0, 768.0]]);
    assert_eq!(asked.labels, vec![1]);
    assert_eq!(asked.boxes, Vec::<[f32; 4]>::new());
    Ok(())
}

#[test]
fn a_negative_click_is_labelled_0_and_a_box_goes_as_left_top_right_bottom() -> Outcome {
    let stand = model();
    let prompts = Prompts::new(
        vec![point(0.5, 0.5, true), point(0.1, 0.2, false)],
        Some([0.125, 0.25, 0.5, 0.75]),
    )?;
    matte(stand, prompts, 30, 50)?;
    let asked = stand.asked()?;
    assert_eq!(asked.labels, vec![1, 0]);
    assert_eq!(asked.boxes, vec![[128.0, 256.0, 512.0, 768.0]]);
    Ok(())
}

#[test]
fn the_photo_is_given_as_a_1024_square_normalised_as_imagenet() -> Outcome {
    let stand = model();
    matte(stand, Prompts::new(vec![point(0.5, 0.5, true)], None)?, 30, 50)?;
    let image = stand.asked()?.image;
    assert_eq!(image.shape(), [1, 3, 1024, 1024]);
    let area = 1024 * 1024;
    // a flat photo stays flat: (0.5 - mean) / std of each channel
    for (channel, (mean, std)) in [(0.485_f32, 0.229_f32), (0.456, 0.224), (0.406, 0.225)]
        .into_iter()
        .enumerate()
    {
        let wanted = (0.5 - mean) / std;
        assert!(
            image.values()[channel * area..(channel + 1) * area]
                .iter()
                .all(|value| (value - wanted).abs() < 1e-4),
            "channel {channel}"
        );
    }
    Ok(())
}

#[test]
fn the_best_scored_candidate_is_the_map_placed_where_the_click_was_on_a_photo_that_is_not_square() -> Outcome {
    // 40 rows x 60 columns, a click 0.25 across and 0.75 down: column 15, row 30
    let found = matte(model(), Prompts::new(vec![point(0.25, 0.75, true)], None)?, 40, 60)?;
    let at = |row: usize, column: usize| found[row * 60 + column];
    assert!(at(30, 15) > 0.99, "on the click: {}", at(30, 15));
    assert!(
        at(10, 15) < 0.01,
        "same column, higher up (a swap of x and y puts it there)"
    );
    assert!(at(30, 50) < 0.01, "same row, further across");
    // the other candidates were everything and nothing: neither is the map
    assert!(found.iter().any(|&value| value < 0.01) && found.iter().any(|&value| value > 0.99));
    Ok(())
}

#[test]
fn a_negative_click_leaves_out_what_is_round_it() -> Outcome {
    let positive = point(0.5, 0.5, true);
    // 256 x 256 photo: one px per logit px, so the squares of 20 px are seen as they are
    let alone = matte(model(), Prompts::new(vec![positive], None)?, 256, 256)?;
    let with = matte(
        model(),
        Prompts::new(vec![positive, point(0.55, 0.5, false)], None)?,
        256,
        256,
    )?;
    let at = |found: &[f32], column: usize| found[128 * 256 + column];
    assert!(
        at(&alone, 141) > 0.99 && at(&with, 141) < 0.01,
        "13 px right of the click, 2 px from the negative one"
    );
    assert!(
        at(&alone, 120) > 0.99 && at(&with, 120) > 0.99,
        "left of the click, away from the negative one"
    );
    Ok(())
}

#[test]
fn the_probability_is_the_logistic_of_the_logit_so_the_edge_is_where_the_logit_is_zero() -> Outcome {
    // every row: logit (column - 128) / 32
    let ramp: Vec<f32> = (0..GRID * GRID)
        .map(|index| (grid_px(index % GRID) - 128.0) / 32.0)
        .collect();
    let stand = Stand::new(move |_| vec![(0.9, ramp.clone())]);
    let found = matte(stand, Prompts::new(vec![point(0.5, 0.5, true)], None)?, GRID, GRID)?;
    let at = |column: usize| found[column];
    assert!((at(128) - 0.5).abs() < 1e-6, "{}", at(128));
    assert!((at(160) - 0.731_058_6).abs() < 1e-5, "sigmoid(1): {}", at(160));
    assert!((at(96) - 0.268_941_4).abs() < 1e-5, "sigmoid(-1): {}", at(96));
    Ok(())
}

#[test]
fn nothing_that_the_model_is_half_sure_of_is_no_subject_found() -> Outcome {
    let stand = Stand::new(|_| vec![(0.9, vec![-3.0; GRID * GRID])]);
    let refused = matte(stand, Prompts::new(vec![point(0.5, 0.5, true)], None)?, 20, 30);
    assert!(
        matches!(refused, Err(ref error) if error.kind() == ErrorKind::SubjectNotFound),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn an_answer_that_is_not_candidates_by_rows_by_columns_is_refused() -> Outcome {
    struct Flat;
    impl Segment for Flat {
        fn segment(&self, _: &Query) -> Result<Answer, Error> {
            Ok(Answer {
                scores: vec![0.5],
                logits: Tensor::new(vec![4], vec![0.0; 4])?,
            })
        }
    }
    let values = grey(20, 30, 0.5);
    let refused = PromptedFinder::new(Prompts::new(vec![point(0.5, 0.5, true)], None)?, Flat)
        .find(&Picture::new(20, 30, &values)?);
    assert!(
        matches!(refused, Err(Error::BackendUnavailable(ref message)) if message.contains("candidates")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn prompts_are_refused_at_construction_naming_the_field_the_rule_and_the_value() {
    let one = point(0.5, 0.5, true);
    let many = vec![one; MAX_POINTS + 1];
    let bad = |points: Vec<Point>, bounding_box: Option<[f64; 4]>| match Prompts::new(points, bounding_box) {
        Err(Error::Inputs(message)) => message,
        other => format!("no refusal: {other:?}"),
    };
    let rows = [
        (
            "no click, no box",
            bad(vec![], None),
            vec!["points", "at least one click"],
        ),
        (
            "x past 1",
            bad(vec![point(1.5, 0.5, true)], None),
            vec!["points[0].x", "0 to 1", "1.5"],
        ),
        (
            "y below 0",
            bad(vec![one, point(0.5, -0.1, false)], None),
            vec!["points[1].y", "-0.1"],
        ),
        (
            "x not a number",
            bad(vec![point(f64::NAN, 0.5, true)], None),
            vec!["points[0].x", "NaN"],
        ),
        ("too many", bad(many, None), vec!["points", "at most 64", "65"]),
        (
            "box right of left",
            bad(vec![], Some([0.5, 0.1, 0.4, 0.9])),
            vec!["box", "left < right", "0.5"],
        ),
        (
            "box without height",
            bad(vec![one], Some([0.1, 0.5, 0.9, 0.5])),
            vec!["box", "top < bottom"],
        ),
        (
            "box past the picture",
            bad(vec![], Some([0.1, 0.1, 1.2, 0.9])),
            vec!["box.right", "1.2"],
        ),
    ];
    for (rule, message, words) in rows {
        for word in words {
            assert!(message.contains(word), "{rule}: {word} in {message}");
        }
    }
}

#[test]
fn the_most_clicks_are_accepted_and_a_box_alone_is_enough() {
    assert!(Prompts::new(vec![point(0.5, 0.5, true); MAX_POINTS], None).is_ok());
    assert!(Prompts::new(vec![], Some([0.0, 0.0, 1.0, 1.0])).is_ok());
}

#[test]
fn prompts_with_other_clicks_a_label_or_a_box_have_another_identity_and_equal_ones_the_same() -> Result<(), Error> {
    let identity =
        |points: Vec<Point>, bounding_box| Prompts::new(points, bounding_box).map(|prompts| prompts.identity());
    let base = identity(vec![point(0.25, 0.75, true)], None)?;
    assert_eq!(base, identity(vec![point(0.25, 0.75, true)], None)?);
    for (what, other) in [
        ("x", identity(vec![point(0.250_000_1, 0.75, true)], None)?),
        ("y", identity(vec![point(0.25, 0.7, true)], None)?),
        ("swapped", identity(vec![point(0.75, 0.25, true)], None)?),
        ("label", identity(vec![point(0.25, 0.75, false)], None)?),
        (
            "extra",
            identity(vec![point(0.25, 0.75, true), point(0.5, 0.5, true)], None)?,
        ),
        (
            "box",
            identity(vec![point(0.25, 0.75, true)], Some([0.0, 0.0, 1.0, 1.0]))?,
        ),
    ] {
        assert_ne!(base, other, "{what}");
    }
    Ok(())
}

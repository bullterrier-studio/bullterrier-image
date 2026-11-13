//! Finding a map with a promptable model (SAM 2): the clicks and the box that say what is wanted ([`Prompts`]), how they and the
//! photo become the model's input, and how its answer becomes a map ([`PromptedFinder`]). Running the model is [`Segment`]'s.

use super::{DEFAULT_EDGE, DEPTH_MEAN, DEPTH_STD, SUBJECT_SIDE, Tensor, inverse_depth, planes, subject_map};
use crate::ops::length::float;
use crate::ops::resample::Filter;
use crate::{Error, Finder, Layer, Picture};

/// The most clicks one find takes: each is an input of the model's decoder, so the work grows with them.
pub const MAX_POINTS: usize = 64; // constant: far more than anyone clicks; bounds the decoder's input

/// One click: a place in the picture, and whether the thing wanted is there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    /// Across the picture, as a share of its width: 0 left, 1 right.
    pub x: f64,
    /// Down the picture, as a share of its height: 0 top, 1 bottom.
    pub y: f64,
    /// `true` for a click on the thing wanted, `false` for a click on something that must be left out.
    pub positive: bool,
}

/// What is wanted, said by clicks and a box, all as shares of the picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Prompts {
    points: Vec<Point>,
    bounding_box: Option<[f64; 4]>,
}

impl Prompts {
    /// The prompts `points` and `bounding_box` (`[left, top, right, bottom]`, shares of the picture's width and height).
    ///
    /// # Errors
    /// [`Error::Inputs`] naming the field, the rule and the value, when there is neither a click nor a box, more than
    /// [`MAX_POINTS`] clicks, a share outside 0 to 1 (or not a number), or a box with no area.
    pub fn new(points: Vec<Point>, bounding_box: Option<[f64; 4]>) -> Result<Self, Error> {
        if points.is_empty() && bounding_box.is_none() {
            return Err(Error::Inputs(
                "points: a prompted finder needs at least one click (or a box): it finds what is clicked, it does not guess"
                    .to_owned(),
            ));
        }
        if points.len() > MAX_POINTS {
            return Err(Error::Inputs(format!(
                "points: at most {MAX_POINTS} clicks, got {}",
                points.len()
            )));
        }
        for (index, point) in points.iter().enumerate() {
            for (axis, share) in [("x", point.x), ("y", point.y)] {
                share_in_range(&format!("points[{index}].{axis}"), share)?;
            }
        }
        if let Some(edges) = bounding_box {
            for (name, share) in ["left", "top", "right", "bottom"].into_iter().zip(edges) {
                share_in_range(&format!("box.{name}"), share)?;
            }
            if edges[0] >= edges[2] || edges[1] >= edges[3] {
                return Err(Error::Inputs(format!(
                    "box: [left, top, right, bottom] must have left < right and top < bottom, got {edges:?}"
                )));
            }
        }
        Ok(Self { points, bounding_box })
    }

    /// The clicks.
    #[must_use]
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// The box, `[left, top, right, bottom]`, if there is one.
    #[must_use]
    pub const fn bounding_box(&self) -> Option<[f64; 4]> {
        self.bounding_box
    }

    /// The prompts as text, exact (every number as written back to the same value): what a find depends on besides the model
    /// and the photo, so a cache of finds is kept exactly as long as the prompts are the same.
    #[must_use]
    pub fn identity(&self) -> String {
        let points: Vec<String> = self
            .points
            .iter()
            .map(|point| format!("{:?},{:?},{}", point.x, point.y, u8::from(point.positive)))
            .collect();
        format!("points [{}] box {:?}", points.join(" "), self.bounding_box)
    }
}

fn share_in_range(field: &str, share: f64) -> Result<(), Error> {
    if (0.0..=1.0).contains(&share) {
        Ok(())
    } else {
        Err(Error::Inputs(format!(
            "{field}: a share of the picture, from 0 to 1, got {share}"
        )))
    }
}

/// What a promptable model is asked: the photo as it takes it and the prompts in its own 1024 x 1024 space.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    /// The photo, `1 x 3 x 1024 x 1024`, normalised as the model was trained.
    pub image: Tensor,
    /// The clicks as (x, y), px of the 1024 square.
    pub points: Vec<[f32; 2]>,
    /// One per click: 1 for a click on the thing wanted, 0 for one on something to leave out.
    pub labels: Vec<i64>,
    /// The boxes as (left, top, right, bottom), px of the 1024 square: none or one.
    pub boxes: Vec<[f32; 4]>,
}

/// What a promptable model answers: several candidate masks, each with how good the model thinks it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    /// The model's own quality estimate of each candidate (an intersection over union), larger is better.
    pub scores: Vec<f32>,
    /// The candidates as logits (positive: the thing wanted), `candidates x rows x columns`.
    pub logits: Tensor,
}

/// Runs a promptable model. Implemented by the ONNX Runtime backend; a test implements it with a stand-in, the one external
/// boundary of prompted finding.
pub trait Segment: Send + Sync {
    /// The model's candidate masks for `query`.
    ///
    /// # Errors
    /// [`Error::BackendUnavailable`] if the model cannot run here or cannot take `query`.
    fn segment(&self, query: &Query) -> Result<Answer, Error>;
}

/// A [`Finder`] that asks a promptable model what the [`Prompts`] point at: the best of the model's candidates, as a map
/// of the photo's size.
///
/// The candidate the model scores highest is taken; its logits are stretched to the photo and made a probability
/// (`1 / (1 + e^-logit)`), so 0.5 is the model's own edge; the edge is then cleaned as a subject's is
/// ([`refine_edge`](crate::refine_edge)). No pixel at least half sure is [`Error::MapNotFound`] of kind subject.
pub struct PromptedFinder {
    runner: Box<dyn Segment>,
    prompts: Prompts,
    shrink: f64,
    feather: f64,
}

impl PromptedFinder {
    /// A finder for `prompts`, run by `runner`.
    #[must_use]
    pub fn new(prompts: Prompts, runner: impl Segment + 'static) -> Self {
        Self {
            runner: Box::new(runner),
            prompts,
            shrink: DEFAULT_EDGE.0,
            feather: DEFAULT_EDGE.1,
        }
    }

    /// This finder with the edge clean-up set to `shrink` and `feather` px; the defaults are [`DEFAULT_EDGE`].
    #[must_use]
    pub const fn with_edge(mut self, shrink: f64, feather: f64) -> Self {
        self.shrink = shrink;
        self.feather = feather;
        self
    }

    fn query(&self, picture: &Picture<'_>) -> Query {
        let side = float(SUBJECT_SIDE);
        // narrowing to the model's single precision: a pixel position needs no more
        #[expect(
            clippy::cast_possible_truncation,
            reason = "px of a 1024 square fit f32 exactly enough"
        )]
        let px = |share: f64| (share * side) as f32;
        Query {
            // SAM 2 takes `ImageNet`'s normalisation, as Depth Anything does
            image: planes(
                picture,
                (SUBJECT_SIDE, SUBJECT_SIDE),
                Filter::Bilinear,
                DEPTH_MEAN,
                DEPTH_STD,
            ),
            points: self.prompts.points.iter().map(|p| [px(p.x), px(p.y)]).collect(),
            labels: self.prompts.points.iter().map(|p| i64::from(p.positive)).collect(),
            boxes: self
                .prompts
                .bounding_box
                .map(|b| [px(b[0]), px(b[1]), px(b[2]), px(b[3])])
                .into_iter()
                .collect(),
        }
    }
}

impl Finder for PromptedFinder {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
        let answer = self.runner.segment(&self.query(picture))?;
        let &[candidates, rows, columns] = answer.logits.shape() else {
            return Err(wrong(&format!(
                "logits of shape {:?}, not candidates x rows x columns",
                answer.logits.shape()
            )));
        };
        if answer.scores.len() != candidates || candidates == 0 {
            return Err(wrong(&format!(
                "{} scores for {candidates} candidates",
                answer.scores.len()
            )));
        }
        let best = (1..candidates).fold(0, |best, next| {
            if answer.scores[next].total_cmp(&answer.scores[best]).is_gt() {
                next
            } else {
                best
            }
        });
        let size = (picture.height(), picture.width());
        let mask = &answer.logits.values()[best * rows * columns..(best + 1) * rows * columns];
        let probability: Vec<f32> = inverse_depth(mask, (rows, columns), size)
            .into_iter()
            .map(|logit| 1.0 / (1.0 + (-logit).exp()))
            .collect();
        subject_map(&probability, size, size, (self.shrink, self.feather))
    }
}

fn wrong(what: &str) -> Error {
    Error::BackendUnavailable(format!(
        "the model answered with {what}: it is not the model it should be"
    ))
}

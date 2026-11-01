//! The region kinds made from other regions or from the picture: [`Pick`], [`Subject`], [`Band`], [`Bands`],
//! [`Combine`], [`LinearGradient`], [`RadialGradient`] and [`Given`].

use std::fmt;

use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize};

/// One piece a [`Pick`] chooses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Piece {
    /// A piece by its number, counted from 0 in piece order (reading order; bands highest first).
    Number(u32),
    /// Every piece of every shape of an `Svg` partition that has this `id`.
    Id(String),
}

impl From<u32> for Piece {
    fn from(number: u32) -> Self {
        Self::Number(number)
    }
}

impl From<&str> for Piece {
    fn from(id: &str) -> Self {
        Self::Id(id.to_owned())
    }
}

impl<'de> Deserialize<'de> for Piece {
    /// A whole number is a piece number, text a shape's `id`. A negative number is refused: pieces count from 0.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(PieceVisitor)
    }
}

struct PieceVisitor;

impl Visitor<'_> for PieceVisitor {
    type Value = Piece;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a piece number or the id of a shape")
    }

    fn visit_u64<E: serde::de::Error>(self, number: u64) -> Result<Piece, E> {
        u32::try_from(number)
            .map(Piece::Number)
            .map_err(|_| E::custom(format!("piece number {number} is beyond what a partition has")))
    }

    fn visit_i64<E: serde::de::Error>(self, number: i64) -> Result<Piece, E> {
        match u64::try_from(number) {
            Ok(number) => self.visit_u64(number),
            Err(_) => Err(E::custom(format!("piece numbers count from 0, got {number}"))),
        }
    }

    fn visit_str<E: serde::de::Error>(self, id: &str) -> Result<Piece, E> {
        Ok(Piece::Id(id.to_owned()))
    }
}

/// A selection: the pixels of some pieces of a partition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pick {
    /// The name of the partition (grid, shards, cuts, svg or bands) whose pieces are picked; it must be set.
    pub of: String,
    /// The pieces to choose, at least one: numbers counted from 0 in piece order or, for an `Svg` partition, the `id`
    /// of a shape. A number the partition has no piece for chooses nothing, so a recipe also works on a picture too
    /// small to have that many pieces; an id the file lacks is an error.
    pub pieces: Vec<Piece>,
}

impl Default for Pick {
    /// Piece 0 of nothing: `of` must be set.
    fn default() -> Self {
        Self {
            of: String::new(),
            pieces: vec![Piece::Number(0)],
        }
    }
}

/// A selection: the subject of the picture, or, with `per`, the subject found inside each piece of a partition (grid
/// first). Without `per` it is the built-in `subject`, and the other fields do nothing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Subject {
    /// The name of a partition: the subject is found inside each of its pieces separately and the finds are united,
    /// so small things a whole-picture search misses are found. None: the subject of the whole picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per: Option<String>,
    /// How much of its surroundings the finder sees round a piece, as a share of the piece's own size added on each
    /// side, 0 to 1; the view is clipped to the picture. Only the piece's own pixels count. Only matters with `per`.
    pub context: f64,
    /// How sure the finder must be of a pixel for it to count as subject, 0 to 1: a pixel it gives this value or less
    /// is dropped. Raising it keeps only what the finder is certain of; 0 keeps every pixel it gives. Only matters
    /// with `per`.
    pub threshold: f64,
}

impl Default for Subject {
    /// The whole picture's subject.
    fn default() -> Self {
        Self {
            per: None,
            context: 0.25,
            threshold: 0.9,
        }
    }
}

/// A selection: the pixels where another selection lies between two values, fading out linearly beyond them. The
/// values are shares of the selection's own scale (0 to 1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Band {
    /// The name of the selection whose values are cut, which must be set: a built-in one (`depth`: 1 is the nearest;
    /// `light`: 1 is white; `subject`) or one the recipe declares.
    pub of: String,
    /// The lowest value chosen fully, 0 to 1 and at most `high`. Raising it leaves out the lower values.
    pub low: f64,
    /// The highest value chosen fully, 0 to 1. Lowering it leaves out the higher values.
    pub high: f64,
    /// How far beyond `low` and `high` the choice fades out, as a share of the scale, 0 to 1: it falls linearly from 1
    /// at the bound to 0 this far past it. Raising it softens the edge; 0 cuts it hard.
    pub feather: f64,
}

impl Default for Band {
    /// The upper half, fading over a tenth.
    fn default() -> Self {
        Self {
            of: String::new(),
            low: 0.5,
            high: 1.0,
            feather: 0.1,
        }
    }
}

/// A partition into bands of a selection: equal ranges of its scale, numbered highest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Bands {
    /// The name of the selection whose values are cut into bands, which must be set. Band 0 holds its highest values:
    /// the nearest for `depth`, the brightest for `light`.
    pub of: String,
    /// How many equal ranges the scale is cut into, 1 to 100. A value on a border goes to the lower band; a band no
    /// pixel lies in has no piece and the rest are renumbered, highest first.
    pub count: u32,
}

impl Default for Bands {
    /// Three bands of nothing: `of` must be set.
    fn default() -> Self {
        Self {
            of: String::new(),
            count: 3,
        }
    }
}

/// How the selections of a [`Combine`] are put together at each pixel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CombineOp {
    /// Their product: all of them, fading where any fades.
    #[default]
    Multiply,
    /// The least of them: all of them, hard where any is hard.
    Min,
    /// The greatest of them: any of them.
    Max,
}

/// A selection made of other selections: where all of them are chosen (multiply, min) or any of them is (max).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Combine {
    /// How the selections are put together.
    pub op: CombineOp,
    /// The names of the selections to combine, at least two, which must be set: built-in ones (`subject`,
    /// `background`, `depth`, `light`) or ones the recipe declares.
    pub of: Vec<String>,
}

impl Default for Combine {
    /// Two unnamed selections, multiplied: `of` must be set.
    fn default() -> Self {
        Self {
            op: CombineOp::Multiply,
            of: vec![String::new(); 2],
        }
    }
}

/// A selection that rises evenly across the picture in one direction: 0 at one edge, 1 at the opposite one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LinearGradient {
    /// The direction in which the value rises, in degrees clockwise from the right: 0 rises to the right, 90 downward,
    /// 180 to the left, 270 upward. Any finite angle is allowed and is the direction a whole number of turns away. The
    /// value is a share of the picture's extent along that direction, so the far corners are 0 and 1 at any aspect
    /// ratio.
    pub angle: f64,
}

impl Default for LinearGradient {
    /// Rising upward.
    fn default() -> Self {
        Self { angle: 270.0 }
    }
}

/// A selection that rises evenly from a point: 0 at the centre, 1 at `radius` and beyond.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RadialGradient {
    /// Where the centre is across the picture, as a share of its width, 0 to 1: 0 is the left edge, 1 the right.
    pub x: f64,
    /// Where the centre is down the picture, as a share of its height, 0 to 1: 0 is the top edge, 1 the bottom.
    pub y: f64,
    /// The distance from the centre at which the value reaches 1, as a share of the picture's shorter side, 0.01 to 3.
    /// Raising it spreads the gradient wider.
    pub radius: f64,
}

impl Default for RadialGradient {
    /// Centred, reaching the midpoints of the shorter sides' edges.
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            radius: 0.5,
        }
    }
}

/// A selection: a map of the photo that the caller hands in at render time under this region's own name, or that a
/// finder of that name finds: `(height, width)`, 0 to 1, the photo's size. It moves with the photo through the
/// recipe's frame (a turn or a border leaves 0 where the picture is not). It has no fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Given {}

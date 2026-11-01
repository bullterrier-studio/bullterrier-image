//! The region kinds laid over a frame: [`Grid`], [`Shards`], [`Cuts`], [`Path`] and [`Svg`], with the [`Edge`] their
//! borders run along.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::super::read;

/// The most columns (or rows) a [`Grid`] has, and the most shares a [`Shares`] holds.
pub(super) const MOST_SHARES: usize = 1000;

/// The most cuts a [`Cuts`] region has.
pub(super) const MOST_CUTS: usize = 32;

/// How the borders between pieces run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EdgeStyle {
    /// Along the exact borders of the kind.
    #[default]
    Straight,
    /// Every border bent by smooth noise, so the pieces have ragged, wandering edges.
    Torn,
}

/// How the borders between pieces run: straight, or torn like paper by bending the coordinates they cut. The numbers
/// after `style` matter only for a torn edge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Edge {
    /// Straight, or torn.
    pub style: EdgeStyle,
    /// How far a torn border strays, in px at a longest side of 480, 0 to 48: the standard deviation of the shift.
    /// Raising it tears the edge wider; 0 is straight.
    pub amount: f64,
    /// The size of the bends of a torn border, in px at a longest side of 480, 1 to 480: the spacing of the coarsest
    /// noise. Raising it gives long gentle wanders, lowering it a jagged edge.
    pub scale: f64,
    /// Random seed of the tear: the same seed always tears the same way, another seed differently.
    pub seed: u32,
    /// How many layers of noise make the tear, 1 to 8: each is twice as fine as the one before. Raising it adds fine
    /// ragged detail to the long wanders.
    pub octaves: u32,
    /// How strong each finer layer of the tear is compared with the one before, 0 to 1. Raising it makes the edge
    /// rougher; 0 leaves only the coarsest wander.
    pub persistence: f64,
}

impl Default for Edge {
    /// Straight borders, with the numbers of a torn edge at their defaults.
    fn default() -> Self {
        Self {
            style: EdgeStyle::Straight,
            amount: 4.8, // constant: the default of the Python `Edge`, which the other defaults here follow too
            scale: 24.0,
            seed: 0,
            octaves: 6,
            persistence: 0.55,
        }
    }
}

/// Shares of a frame, from left to right or top to bottom: the relative widths of the columns of a [`Grid`]. Written
/// as the list of shares, read as that list or as a count `N`, which is `N` equal shares.
///
/// `[1, 2, 1]` is a narrow, a wide and a narrow column. [`Recipe::checked`](crate::Recipe::checked) wants 1 to 1000
/// positive finite shares.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Shares(pub Vec<f64>);

impl Shares {
    /// `count` equal shares.
    #[must_use]
    pub fn equal(count: usize) -> Self {
        Self(vec![1.0; count])
    }
}

impl Default for Shares {
    /// One share: the whole frame.
    fn default() -> Self {
        Self::equal(1)
    }
}

impl<'de> Deserialize<'de> for Shares {
    /// A whole number from 1 to 1000 is that many equal shares; a list is its numbers, however many (the count is
    /// checked with the recipe).
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            list @ Value::Array(_) => read(list).map(Self).map_err(D::Error::custom),
            other => match other.as_i64().and_then(|count| usize::try_from(count).ok()) {
                Some(count) if (1..=MOST_SHARES).contains(&count) => Ok(Self::equal(count)),
                _ => Err(D::Error::custom(format!(
                    "must be a count or a list of positive numbers, 1 to {MOST_SHARES} of them, got {other}"
                ))),
            },
        }
    }
}

/// A grid of rectangular pieces, any number of columns and rows, equal or in uneven shares. A pixel belongs to the
/// column whose range holds its centre; a centre exactly on a border goes to the later column. Pieces are numbered row
/// after row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Grid {
    /// The name of a selection (a built-in one or another region): the grid is laid over that selection's bounding box
    /// instead of the whole picture, so its borders are shares of that box (object first: cut the subject, not the
    /// sky). Beyond the box the outer pieces run to the picture's edge. None: the whole picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    /// How the borders run.
    pub edge: Edge,
    /// How the frame is divided from left to right.
    pub columns: Shares,
    /// How the frame is divided from top to bottom.
    pub rows: Shares,
}

/// Irregular pieces like broken glass or stone: the cells round seeds scattered over the frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Shards {
    /// The name of a selection the seeds are scattered inside; none: the whole picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    /// How the borders run.
    pub edge: Edge,
    /// How many shards, 1 to 500: exactly this many seeds are scattered, so a picture too small to give each seed a
    /// pixel has fewer pieces. Raising it makes smaller shards.
    pub count: u32,
    /// How far each seed strays from its place in a regular honeycomb, as a share of the spacing, 0 to 1: 0 cuts a
    /// regular honeycomb of equal pieces, 1 lets every seed go anywhere in its own place.
    pub jitter: f64,
    /// Where the shards crowd to when `focus` is above 0, as (x, y) shares of the frame, each 0 to 1 (0, 0 is the top
    /// left corner).
    pub impact: [f64; 2],
    /// How much the shards crowd round `impact`, 0 to 5: 0 spreads them evenly, 2.5 makes small splinters at the
    /// impact point and big pieces far from it, like cracked glass.
    pub focus: f64,
    /// Random seed for where the seeds lie: the same seed always cuts the same shards.
    pub seed: u32,
    /// The name of a shipped set of these fields (`glass`, `torn`, `stone` or `mosaic`), used for every field not
    /// given beside it. None: the defaults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
}

impl Default for Shards {
    /// Twelve shards on a lightly jittered honeycomb crowding to nowhere in particular.
    fn default() -> Self {
        Self {
            fit: None,
            edge: Edge::default(),
            count: 12,
            jitter: 0.5,
            impact: [0.5, 0.5],
            focus: 0.0,
            seed: 0,
            preset: None,
        }
    }
}

/// One straight cut: a line through a point at an angle, from border to border.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Line {
    /// Where the line passes, across, as a share of the frame's width, 0 to 1: 0 is the left edge, 1 the right.
    pub x: f64,
    /// Where the line passes, down, as a share of the frame's height, 0 to 1: 0 is the top edge, 1 the bottom.
    pub y: f64,
    /// The direction of the line, in degrees clockwise from the right as seen on the picture. Any finite angle is
    /// allowed and is the direction a whole number of turns away. A line has no head, so an angle and the one 180
    /// further cut alike.
    pub angle: f64,
}

impl Default for Line {
    /// A horizontal line through the middle.
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            angle: 0.0,
        }
    }
}

/// One curved cut: an open SVG path from border to border.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Curve {
    /// An open SVG path (one `M`, then lines, curves or arcs) whose coordinates are shares of the frame, 0 to 1 across
    /// and down. It should run from one border of the frame to another; an end that does not is carried straight on to
    /// the nearest border.
    pub path: String,
}

impl Default for Curve {
    /// The diagonal.
    fn default() -> Self {
        Self {
            path: "M 0 0 L 1 1".to_owned(),
        }
    }
}

/// One cut of a [`Cuts`] region.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Cut {
    /// A straight cut.
    Line(Line),
    /// A curved cut.
    Curve(Curve),
}

impl From<Line> for Cut {
    fn from(line: Line) -> Self {
        Self::Line(line)
    }
}

impl From<Curve> for Cut {
    fn from(curve: Curve) -> Self {
        Self::Curve(curve)
    }
}

impl<'de> Deserialize<'de> for Cut {
    /// A table with a `path` is a [`Curve`], any other a [`Line`].
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let cut = if value.get("path").is_some() {
            read(value).map(Self::Curve)
        } else {
            read(value).map(Self::Line)
        };
        cut.map_err(D::Error::custom)
    }
}

/// Straight and curved cuts across the frame: the pieces are the regions the cuts divide it into, numbered in reading
/// order. `n` cuts that cross nowhere make `n + 1` pieces, more cuts crossing give more.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cuts {
    /// The name of a selection the frame is that selection's bounding box; none: the whole picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    /// How the borders run.
    pub edge: Edge,
    /// The cuts, 1 to 32.
    pub lines: Vec<Cut>,
}

impl Default for Cuts {
    /// One horizontal cut through the middle.
    fn default() -> Self {
        Self {
            fit: None,
            edge: Edge::default(),
            lines: vec![Cut::Line(Line::default())],
        }
    }
}

/// A selection: everything inside a closed SVG path, softly edged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Path {
    /// The name of a selection the path's frame is the bounding box of; none: the whole picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    /// How the borders run.
    pub edge: Edge,
    /// A closed SVG path whose coordinates are shares of the frame, 0 to 1 across and down: the unit square is
    /// stretched to the frame, so an arc `A 0.25 0.25` is a circle only in a square frame. A pixel is inside by the
    /// nonzero rule (an inner outline drawn the other way round is a hole).
    pub d: String,
    /// How many evenly spaced scanlines measure how much of a border pixel the path covers, 1 to 16: 1 looks at the
    /// middle of the pixel row only, more give a smoother slanting border and cost more time.
    pub samples: u32,
}

impl Default for Path {
    /// The whole frame.
    fn default() -> Self {
        Self {
            fit: None,
            edge: Edge::default(),
            d: "M 0 0 H 1 V 1 H 0 Z".to_owned(),
            samples: 4,
        }
    }
}

/// Shapes from an SVG file: every shape is a piece, and the area outside all of them one more piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Svg {
    /// The name of a selection the file is placed in the bounding box of; none: the whole picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    /// How the borders run.
    pub edge: Edge,
    /// The SVG file to read; it must be set. A relative path in a recipe file is relative to that file. The file is
    /// untrusted input: nothing in it is run or fetched.
    pub file: String,
    /// The most shapes the file may hold, counting what each `use` repeats, 1 to 100000; a file with more is refused.
    pub max_shapes: u32,
    /// The largest file to read, in kilobytes, 1 to 100000; a bigger one is refused without being read.
    pub max_kilobytes: u32,
}

impl Default for Svg {
    /// No file, and the limits of the Python `Svg`.
    fn default() -> Self {
        Self {
            fit: None,
            edge: Edge::default(),
            file: String::new(),
            max_shapes: 1000,
            max_kilobytes: 1024, // one megabyte
        }
    }
}

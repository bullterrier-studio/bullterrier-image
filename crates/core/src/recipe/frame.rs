//! How a recipe frames the photo before its steps run and cuts the picture out after them: [`Frame`].
//!
//! A [`Frame`] is a recipe's geometry as typed values: every field has a default, a range and a meaning, written in
//! its documentation. It is read from a recipe's `frame` (a `[frame]` table in a file) and written back with all
//! fourteen fields; a recipe without one has the default frame, which changes nothing and is not written.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::check::outside;
use crate::params::Color;
use crate::{Constraint, Error};

/// Which way the photo is mirrored before anything else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Flip {
    /// Left as it is.
    #[default]
    None,
    /// Left and right swapped.
    X,
    /// Top and bottom swapped.
    Y,
    /// Both: a half turn.
    Both,
}

/// What a turn does to the frame of the picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Fit {
    /// Keep the largest rectangle of the photo's own aspect that the turned photo covers everywhere: no `fill` shows.
    #[default]
    Crop,
    /// Make the picture as large as the whole turned photo needs and show `fill` in the corners.
    Expand,
}

/// How a turn by an angle that is not a quarter turn reads the photo between its pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Resample {
    /// Blocky and exact.
    Nearest,
    /// Smooth and soft.
    Bilinear,
    /// Smooth and sharper.
    #[default]
    Bicubic,
}

/// Where a crop sits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Anchor {
    /// The middle.
    #[default]
    Center,
    /// Against the top.
    Top,
    /// Against the bottom.
    Bottom,
    /// Against the left.
    Left,
    /// Against the right.
    Right,
    /// The smallest crop of the aspect that holds the subject and its margin, kept inside the photo.
    Subject,
}

/// How the finished picture is cut out into one with an alpha channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Transparent {
    /// Opaque.
    #[default]
    None,
    /// The subject map is the alpha.
    Background,
    /// The circle inscribed in the picture is kept.
    Circle,
    /// The corners are rounded by `corner_radius`.
    Rounded,
}

/// The geometry of a recipe: flip, turn, crop, borders and transparency, applied to the photo and its maps together in
/// that order before the first step (the transparency after the last). The default frame changes nothing. All lengths
/// are shares of the picture, so the same look comes out at any resolution.
///
/// Reading checks the shape: a field that is not one of these, or a value of another kind, is refused. The ranges are
/// the rules of [`Recipe::checked`](crate::Recipe::checked), which names the field (`frame.aspect`), the value and the
/// rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Frame {
    /// Mirror the photo before anything else.
    pub flip: Flip,
    /// Turn the photo about its centre by this many degrees, clockwise when positive. Any finite angle is allowed and
    /// is the same turn as the angle a whole number of turns away (270 is -90). A quarter turn moves every pixel
    /// exactly; any other angle resamples.
    pub angle: f64,
    /// What a turn does to the frame.
    pub fit: Fit,
    /// How a turn that is not a quarter reads the photo between its pixels.
    pub resample: Resample,
    /// Crop to this width over height, 0.1 to 10 (1 is a square, 1.778 is 16:9); none: the photo's own shape.
    pub aspect: Option<f64>,
    /// Where the crop sits.
    pub anchor: Anchor,
    /// Room kept around the subject by a crop on the `Subject` anchor, 0 to 1, as a share of the longer side of the
    /// subject's box.
    pub margin: f64,
    /// Border added above the cropped photo, in `fill`, 0 to 2, as a share of the picture's shorter side.
    pub pad_top: f64,
    /// Border added to the right, as `pad_top`.
    pub pad_right: f64,
    /// Border added below, as `pad_top`.
    pub pad_bottom: f64,
    /// Border added to the left, as `pad_top`.
    pub pad_left: f64,
    /// Colour of the borders and of the corners a turn leaves with `Expand`: `'#rrggbb'`, `'#rgb'` or three numbers
    /// from 0 to 1.
    #[serde(deserialize_with = "colour")]
    pub fill: Color,
    /// How the finished picture is cut out.
    pub transparent: Transparent,
    /// How round the corners of `Rounded` are, 0 to 0.5, as a share of the picture's shorter side.
    pub corner_radius: f64,
}

impl Default for Frame {
    /// The frame that changes nothing.
    fn default() -> Self {
        Self {
            flip: Flip::None,
            angle: 0.0,
            fit: Fit::Crop,
            resample: Resample::Bicubic,
            aspect: None,
            anchor: Anchor::Center,
            margin: 0.0,
            pad_top: 0.0,
            pad_right: 0.0,
            pad_bottom: 0.0,
            pad_left: 0.0,
            fill: Color::default(),
            transparent: Transparent::None,
            corner_radius: 0.2, // constant: the default of the Python `Framing`, a fifth of the shorter side
        }
    }
}

impl Frame {
    /// Whether this frame changes nothing: every field is at its default.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether running this frame needs the subject map: a crop around it (`anchor` `Subject`) or a cut by it
    /// (`transparent` `Background`). The one place that says so; the runner finds the subject for a frame that does.
    #[must_use]
    pub fn needs_subject(&self) -> bool {
        self.anchor == Anchor::Subject || self.transparent == Transparent::Background
    }

    /// The frame the JSON object `value` writes, every number checked, or what is wrong, naming the field.
    ///
    /// # Errors
    /// [`Error::Recipe`] naming `frame`, the field and the rule.
    pub fn from_json(value: Value) -> Result<Self, Error> {
        let frame: Self = super::read(value).map_err(|message| Error::Recipe(format!("frame: {message}")))?;
        frame.check()?;
        Ok(frame)
    }

    /// This frame if every number is within its range, else the first that is not.
    ///
    /// # Errors
    /// [`Error::Recipe`] naming the field (`frame.pad_top`), the value and the rule.
    pub fn check(&self) -> Result<(), Error> {
        finite("angle", self.angle)?;
        if let Some(aspect) = self.aspect {
            within("aspect", aspect, 0.1, 10.0)?;
        }
        within("margin", self.margin, 0.0, 1.0)?;
        for (field, share) in [
            ("pad_top", self.pad_top),
            ("pad_right", self.pad_right),
            ("pad_bottom", self.pad_bottom),
            ("pad_left", self.pad_left),
        ] {
            within(field, share, 0.0, 2.0)?;
        }
        within("corner_radius", self.corner_radius, 0.0, 0.5)?;
        if self.fill.0.iter().any(|channel| !(0.0..=1.0).contains(channel)) {
            return Err(Error::Recipe(format!(
                "`frame.fill` must be '#rrggbb', '#rgb' or three numbers from 0 to 1, got {:?}",
                self.fill.0
            )));
        }
        Ok(())
    }
}

/// `value`, the number `field` of the frame, if it is a number (not infinite, not NaN).
fn finite(field: &str, value: f64) -> Result<(), Error> {
    within(field, value, f64::NEG_INFINITY, f64::INFINITY)
}

/// `value`, the number `field` of the frame, if it lies within `min` and `max`, both inclusive.
fn within(field: &str, value: f64, min: f64, max: f64) -> Result<(), Error> {
    match outside(value, min, max) {
        None => Ok(()),
        Some(said) => Err(Error::Recipe(format!("`frame.{field}` {said}"))),
    }
}

/// A `fill` as a person writes it: `'#rrggbb'`, `'#rgb'` or three numbers from 0 to 1.
fn colour<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Color::parse(&value).ok_or_else(|| D::Error::custom(Constraint::Colour.to_string()))
}

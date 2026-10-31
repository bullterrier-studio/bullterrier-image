//! How a recipe frames the photo before its steps run and cuts the picture out after them: [`Frame`].
//!
//! A [`Frame`] is a recipe's geometry as typed values: every field has a default, a range and a meaning, written in
//! its documentation. It is read from a recipe's `frame` (a `[frame]` table in a file) and written back with all
//! fourteen fields; a recipe without one has the default frame, which changes nothing and is not written.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

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

    /// This frame if every number is within its range, else the first that is not.
    ///
    /// # Errors
    /// [`Error::Recipe`] naming the field (`frame.pad_top`), the value and the rule.
    pub(super) fn check(&self) -> Result<(), Error> {
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
    if value.is_finite() {
        Ok(())
    } else {
        Err(Error::Recipe(format!(
            "`frame.{field}` must be a finite number, got {value}"
        )))
    }
}

/// `value`, the number `field` of the frame, if it lies within `min` and `max`, both inclusive.
fn within(field: &str, value: f64, min: f64, max: f64) -> Result<(), Error> {
    finite(field, value)?;
    if value < min {
        Err(Error::Recipe(format!("`frame.{field}` must be >= {min}, got {value}")))
    } else if value > max {
        Err(Error::Recipe(format!("`frame.{field}` must be <= {max}, got {value}")))
    } else {
        Ok(())
    }
}

/// A `fill` as a person writes it: `'#rrggbb'`, `'#rgb'` or three numbers from 0 to 1.
fn colour<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Color::parse(&value).ok_or_else(|| D::Error::custom(Constraint::Colour.to_string()))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::{Presets, Recipe, Step};

    /// The frame of a recipe as `serde_json` writes it, with all fourteen fields: the defaults, by hand.
    fn defaults() -> Value {
        json!({
            "flip": "none", "angle": 0.0, "fit": "crop", "resample": "bicubic", "aspect": null, "anchor": "center",
            "margin": 0.0, "pad_top": 0.0, "pad_right": 0.0, "pad_bottom": 0.0, "pad_left": 0.0,
            "fill": [0.0, 0.0, 0.0], "transparent": "none", "corner_radius": 0.2,
        })
    }

    /// The frame of the JSON recipe that has one step and `frame` as its frame.
    fn read(frame: &Value) -> Result<Frame, Error> {
        Recipe::from_json(&json!({"steps": [{"effect": "grade"}], "frame": frame}).to_string())
            .map(|recipe| recipe.frame)
    }

    /// `frame` checked as part of a recipe: the first rule it breaks.
    fn checked(frame: Frame) -> Result<(), Error> {
        let mut recipe = Recipe::new([Step::new("grade")]);
        recipe.frame = frame;
        recipe.checked(&Presets::new()).map(|_| ())
    }

    #[test]
    fn the_default_frame_is_empty_and_any_other_is_not() {
        assert!(Frame::default().is_empty());
        let others = [
            Frame {
                flip: Flip::X,
                ..Frame::default()
            },
            Frame {
                angle: 90.0,
                ..Frame::default()
            },
            Frame {
                fit: Fit::Expand,
                ..Frame::default()
            },
            Frame {
                resample: Resample::Nearest,
                ..Frame::default()
            },
            Frame {
                aspect: Some(1.0),
                ..Frame::default()
            },
            Frame {
                anchor: Anchor::Top,
                ..Frame::default()
            },
            Frame {
                margin: 0.1,
                ..Frame::default()
            },
            Frame {
                pad_top: 0.1,
                ..Frame::default()
            },
            Frame {
                pad_right: 0.1,
                ..Frame::default()
            },
            Frame {
                pad_bottom: 0.1,
                ..Frame::default()
            },
            Frame {
                pad_left: 0.1,
                ..Frame::default()
            },
            Frame {
                fill: Color([0.0, 0.0, 1.0]),
                ..Frame::default()
            },
            Frame {
                transparent: Transparent::Circle,
                ..Frame::default()
            },
            Frame {
                corner_radius: 0.0,
                ..Frame::default()
            },
        ];
        for (number, frame) in others.iter().enumerate() {
            assert!(!frame.is_empty(), "frame {number}");
        }
    }

    #[test]
    fn a_frame_is_written_with_every_field_and_reads_back_equal() {
        let frame = Frame {
            flip: Flip::Both,
            angle: -30.5,
            fit: Fit::Expand,
            resample: Resample::Nearest,
            aspect: Some(1.5),
            anchor: Anchor::Subject,
            margin: 0.25,
            pad_top: 0.5,
            pad_right: 0.75,
            pad_bottom: 1.0,
            pad_left: 1.5,
            fill: Color([0.25, 0.5, 0.75]),
            transparent: Transparent::Rounded,
            corner_radius: 0.4,
        };
        let written = json!({
            "flip": "both", "angle": -30.5, "fit": "expand", "resample": "nearest", "aspect": 1.5,
            "anchor": "subject", "margin": 0.25, "pad_top": 0.5, "pad_right": 0.75, "pad_bottom": 1.0,
            "pad_left": 1.5, "fill": [0.25, 0.5, 0.75], "transparent": "rounded", "corner_radius": 0.4,
        });
        assert_eq!(serde_json::to_value(&frame).ok(), Some(written.clone()));
        assert_eq!(read(&written).ok(), Some(frame));
        assert_eq!(serde_json::to_value(Frame::default()).ok(), Some(defaults()));
    }

    #[test]
    fn a_field_left_out_is_read_as_its_default_and_a_fill_as_text_or_numbers() {
        assert_eq!(
            read(&json!({"aspect": 1})).ok(),
            Some(Frame {
                aspect: Some(1.0),
                ..Frame::default()
            })
        );
        assert_eq!(read(&json!({})).ok(), Some(Frame::default()));
        // 0xab / 255, 0x11 / 255 for the long form; each digit doubled for the short one
        let long = read(&json!({"fill": "#ab1122"})).map(|frame| frame.fill);
        assert_eq!(long.ok(), Some(Color([171.0 / 255.0, 17.0 / 255.0, 34.0 / 255.0])));
        let short = read(&json!({"fill": "#a12"})).map(|frame| frame.fill);
        assert_eq!(short.ok(), Some(Color([170.0 / 255.0, 17.0 / 255.0, 34.0 / 255.0])));
        let numbers = read(&json!({"fill": [0, 0.5, 1]})).map(|frame| frame.fill);
        assert_eq!(numbers.ok(), Some(Color([0.0, 0.5, 1.0])));
    }

    #[test]
    fn a_frame_the_shape_does_not_allow_is_refused_naming_the_field() {
        let rows = [
            ("an unknown field", json!({"zoom": 2}), "zoom"),
            ("a word for a number", json!({"angle": "up"}), "angle"),
            ("a switch for a number", json!({"margin": true}), "margin"),
            (
                "a choice that is not listed",
                json!({"flip": "z"}),
                "unknown variant `z`",
            ),
            ("a number for a choice", json!({"anchor": 2}), "anchor"),
            ("a colour of four hex digits", json!({"fill": "#abcd"}), "'#rrggbb'"),
            (
                "a colour with a channel above 1",
                json!({"fill": [0, 0, 2]}),
                "three numbers",
            ),
            ("a colour of two numbers", json!({"fill": [0, 0]}), "three numbers"),
            ("a number for a colour", json!({"fill": 7}), "three numbers"),
        ];
        for (what, frame, said) in rows {
            let result = read(&frame);
            assert!(
                matches!(&result, Err(Error::Recipe(message)) if message.contains(said)),
                "{what}: wanted {said:?}, got {result:?}"
            );
        }
    }

    /// Every number of a frame with an inclusive range: the field, a value just below it, the lowest, the highest
    /// and a value just above it, as the frame's documentation gives them.
    const RANGES: [(&str, f64, f64, f64, f64); 7] = [
        ("aspect", 0.09, 0.1, 10.0, 10.5),
        ("margin", -0.25, 0.0, 1.0, 1.25),
        ("pad_top", -0.5, 0.0, 2.0, 2.5),
        ("pad_right", -0.5, 0.0, 2.0, 2.5),
        ("pad_bottom", -0.5, 0.0, 2.0, 2.5),
        ("pad_left", -0.5, 0.0, 2.0, 2.5),
        ("corner_radius", -0.25, 0.0, 0.5, 0.75),
    ];

    /// The default frame with the number `field` set to `value`.
    fn set(field: &str, value: f64) -> Frame {
        let mut frame = Frame::default();
        match field {
            "aspect" => frame.aspect = Some(value),
            "margin" => frame.margin = value,
            "pad_top" => frame.pad_top = value,
            "pad_right" => frame.pad_right = value,
            "pad_bottom" => frame.pad_bottom = value,
            "pad_left" => frame.pad_left = value,
            "corner_radius" => frame.corner_radius = value,
            _ => frame.angle = value,
        }
        frame
    }

    #[test]
    fn a_number_is_within_its_range_at_both_ends_and_refused_outside_naming_field_value_and_rule() {
        for (field, below, low, high, above) in RANGES {
            for value in [low, f64::midpoint(low, high), high] {
                assert!(checked(set(field, value)).is_ok(), "{field} = {value}");
            }
            for (value, rule) in [(below, format!(">= {low}")), (above, format!("<= {high}"))] {
                let refused = checked(set(field, value)).err().map(|error| error.to_string());
                let said = format!("`frame.{field}` must be {rule}, got {value}");
                assert_eq!(refused.as_deref(), Some(said.as_str()), "{field} = {value}");
            }
        }
    }

    #[test]
    fn an_angle_is_any_finite_number_and_a_number_that_is_not_finite_is_refused() {
        for angle in [0.0, 270.0, -540.5, 1.0e9, 180.0, -180.0] {
            assert!(checked(set("angle", angle)).is_ok(), "angle {angle}");
        }
        for field in ["angle", "aspect", "margin", "pad_top", "corner_radius"] {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let result = checked(set(field, value)).err().map(|error| error.to_string());
                let said = format!("`frame.{field}` must be a finite number, got {value}");
                assert_eq!(result.as_deref(), Some(said.as_str()), "{field} = {value}");
            }
        }
    }

    #[test]
    fn a_fill_with_a_channel_outside_0_to_1_is_refused() {
        let fill = |channels: [f32; 3]| Frame {
            fill: Color(channels),
            ..Frame::default()
        };
        assert!(checked(fill([0.0, 0.5, 1.0])).is_ok());
        for channels in [[-0.01, 0.0, 0.0], [0.0, 1.01, 0.0], [0.0, 0.0, f32::NAN]] {
            let result = checked(fill(channels)).err().map(|error| error.to_string());
            assert!(
                result
                    .as_deref()
                    .is_some_and(|message| message.starts_with("`frame.fill` must be")),
                "{channels:?}: {result:?}"
            );
        }
    }
}

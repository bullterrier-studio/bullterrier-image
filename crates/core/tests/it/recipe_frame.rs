//! A recipe's frame, through the public API alone: what it is made of, how it is written and read, and the ranges
//! `Recipe::checked` holds its numbers to.

use bullterrier_image::frame::{Anchor, Fit, Flip, Resample, Transparent};
use bullterrier_image::params::Color;
use bullterrier_image::{Error, Frame, Presets, Recipe, Step};
use serde_json::{Value, json};

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
    Recipe::from_json(&json!({"steps": [{"effect": "grade"}], "frame": frame}).to_string()).map(|recipe| recipe.frame)
}

/// `frame` checked as part of a recipe: the first rule it breaks.
fn checked(frame: Frame) -> Result<(), Error> {
    let mut recipe = Recipe::new([Step::new("grade")]);
    recipe.frame = frame;
    recipe.checked(&Presets::new()).map(|_| ())
}

#[test]
fn a_frame_needs_the_subject_for_a_crop_around_it_or_a_cut_by_it_and_for_nothing_else() {
    let with = |frame: Frame| frame.needs_subject();
    assert!(!with(Frame::default()));
    assert!(with(Frame {
        anchor: Anchor::Subject,
        ..Frame::default()
    }));
    assert!(with(Frame {
        transparent: Transparent::Background,
        ..Frame::default()
    }));
    assert!(with(Frame {
        anchor: Anchor::Subject,
        transparent: Transparent::Background,
        ..Frame::default()
    }));
    for other in [
        Frame {
            anchor: Anchor::Left,
            aspect: Some(1.0),
            ..Frame::default()
        },
        Frame {
            transparent: Transparent::Circle,
            ..Frame::default()
        },
        Frame {
            transparent: Transparent::Rounded,
            margin: 0.5,
            ..Frame::default()
        },
    ] {
        assert!(!with(other.clone()), "{other:?}");
    }
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

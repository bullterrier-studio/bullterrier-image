//! Own binary: it walks every effect in the process's registry, so effects other tests register would be walked too.
//!
//! Every parameter of every effect the core runs changes the result of that effect: the parameters are public fields
//! of a `Params` struct, so rustc no longer reports one that `render` never reads, and this test does.
//!
//! Each effect is applied to a fixed picture at its defaults, then once for each alternative value of each parameter
//! with only that parameter changed; a parameter is read if some alternative changes the picture. A parameter that
//! matters only under a condition (`grade.pivot` does nothing while `contrast` is 1) has a row in [`conditional`]: the
//! test checks it both ways, so a row that is no longer needed fails too.

use std::error::Error;

use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::{
    Category, Constraint, Effect, Error as CoreError, FieldParam, Image, Inputs, Layer, Lut, Picture, effect, effects,
};
use serde_json::{Map, Value, json};

type Outcome = Result<(), Box<dyn Error>>;

/// A change below this is rounding noise: a pattern turned a whole turn differs by about 2e-6, `grade.pivot` at a
/// `contrast` of 1 is not bit-exact.
const TOLERANCE: f32 = 1e-4;
const SIDE: u8 = 12;

/// A parameter that changes the result only while other parameters have set values.
struct Conditional {
    effect: &'static str,
    field: &'static str,
    /// The values of the other parameters under which it matters.
    with: Vec<(&'static str, Value)>,
}

/// Every parameter that is not read at the defaults, and why.
#[expect(
    clippy::too_many_lines,
    reason = "a table, one row for each parameter that matters only under a condition"
)]
fn conditional() -> Vec<Conditional> {
    vec![
        // contrast leaves values where they are at 1, whatever the pivot
        Conditional {
            effect: "grade",
            field: "pivot",
            with: vec![("contrast", json!(2.0))],
        },
        // the range of colours selected changes nothing while no change is set for them (the defaults keep hue,
        // saturation and lightness)
        Conditional {
            effect: "selective_color",
            field: "hue",
            with: vec![("hue_shift", json!(90.0))],
        },
        Conditional {
            effect: "selective_color",
            field: "width",
            with: vec![("hue_shift", json!(90.0))],
        },
        Conditional {
            effect: "selective_color",
            field: "feather",
            with: vec![("hue_shift", json!(90.0))],
        },
        // the picture's tones repeat, so the methods read the same clip points at the default shares, which fall among ties
        Conditional {
            effect: "gradient_map",
            field: "quantile_method",
            with: vec![("shadow_clip", json!(0.37)), ("highlight_clip", json!(0.13))],
        },
        // a linear band is always centred and has no radius; reversing needs a second repeat
        Conditional {
            effect: "gradient",
            field: "x",
            with: vec![("kind", json!("radial"))],
        },
        Conditional {
            effect: "gradient",
            field: "y",
            with: vec![("kind", json!("radial"))],
        },
        Conditional {
            effect: "gradient",
            field: "radius",
            with: vec![("kind", json!("radial"))],
        },
        Conditional {
            effect: "gradient",
            field: "reflect",
            with: vec![("repeat", json!(2))],
        },
        // a slid strip reaches beyond the edge only when it starts nearer the edge than it slides
        Conditional {
            effect: "lenticular",
            field: "edge",
            with: vec![("strip", json!(2))],
        },
        // the value beyond the edge is seen only by the edge rule that uses it
        Conditional {
            effect: "gaussian_blur",
            field: "constant_value",
            with: vec![("edge", json!("constant"))],
        },
        Conditional {
            effect: "bloom",
            field: "constant_value",
            with: vec![("edge", json!("constant"))],
        },
        Conditional {
            effect: "sharpen",
            field: "constant_value",
            with: vec![("edge", json!("constant"))],
        },
        // a table that is none is the identity, which the amount mixes in without change
        Conditional {
            effect: "lut",
            field: "amount",
            with: vec![("file", json!("table"))],
        },
        // on a 12 px picture the default ghosts (size 30 px) screen it to white, hiding the light's own features; the
        // default ring (72 px out, 12 px wide) lies wholly beyond the picture
        Conditional {
            effect: "lens_flare",
            field: "x",
            with: vec![("ghosts", json!(0))],
        },
        Conditional {
            effect: "lens_flare",
            field: "y",
            with: vec![("ghosts", json!(0))],
        },
        Conditional {
            effect: "lens_flare",
            field: "halo",
            with: vec![("ghosts", json!(0))],
        },
        Conditional {
            effect: "lens_flare",
            field: "ring",
            with: vec![("ghosts", json!(0)), ("halo", json!(0.0)), ("ring_width", json!(300.0))],
        },
        Conditional {
            effect: "lens_flare",
            field: "ring_width",
            with: vec![("ghosts", json!(0)), ("halo", json!(0.0))],
        },
        // the default patch (15 px) covers the whole 12 px picture, whose darkest value is 0: no haze is found, so nothing
        // else matters until the patch is smaller
        Conditional {
            effect: "dehaze",
            field: "amount",
            with: vec![("patch", json!(3))],
        },
        Conditional {
            effect: "dehaze",
            field: "radius",
            with: vec![("patch", json!(3))],
        },
        Conditional {
            effect: "dehaze",
            field: "edge_threshold",
            with: vec![("patch", json!(3))],
        },
        Conditional {
            effect: "dehaze",
            field: "transmission_floor",
            with: vec![("patch", json!(3))],
        },
        Conditional {
            effect: "dehaze",
            field: "airlight_fraction",
            with: vec![("patch", json!(3))],
        },
        Conditional {
            effect: "dehaze",
            field: "edge",
            with: vec![("patch", json!(3))],
        },
        // the seed draws the points of a bright area and their brightness, which the defaults leave all alike
        Conditional {
            effect: "starburst",
            field: "seed",
            with: vec![("spacing", json!(3.0))],
        },
        // a bilinear reading between a pixel and the light stays inside the picture; the cubic looks a pixel past the point
        // it reads, beyond the edge for the readings within a pixel of the border
        Conditional {
            effect: "light_rays",
            field: "edge",
            with: vec![("interpolation", json!("bicubic"))],
        },
        // only a pinch reads from farther out than it shows, and from beyond the edge only when its disc reaches it
        Conditional {
            effect: "pinch_bulge",
            field: "edge",
            with: vec![("amount", json!(-1.0)), ("radius", json!(2.0)), ("x", json!(0.0))],
        },
        // a swirl reads beyond the edge only when its disc reaches it
        Conditional {
            effect: "swirl",
            field: "edge",
            with: vec![("radius", json!(2.0)), ("x", json!(0.0))],
        },
        // the default bend is a barrel that fills the frame and reads only inside the picture: the edge rule and the border
        // show once the 'centre' fit exposes the corners
        Conditional {
            effect: "lens_distortion",
            field: "edge",
            with: vec![("fit", json!("centre"))],
        },
        Conditional {
            effect: "lens_distortion",
            field: "border",
            with: vec![("fit", json!("centre"))],
        },
        // bands are drawn only when there are some, and the heights are then drawn between the two limits
        Conditional {
            effect: "slice_shift",
            field: "band_min_height",
            with: vec![("bands", json!(7))],
        },
        Conditional {
            effect: "slice_shift",
            field: "band_max_height",
            with: vec![("bands", json!(7))],
        },
        // the clumping blur is run only for specks of more than a pixel
        Conditional {
            effect: "grain",
            field: "truncate",
            with: vec![("size", json!(6.0))],
        },
        Conditional {
            effect: "grain",
            field: "edge",
            with: vec![("size", json!(6.0))],
        },
        // the weave is laid only by the woven kinds; 'paper' is fibres alone
        Conditional {
            effect: "paper_texture",
            field: "spacing",
            with: vec![("kind", json!("canvas"))],
        },
        Conditional {
            effect: "paper_texture",
            field: "fibre",
            with: vec![("kind", json!("canvas"))],
        },
        // a dot is drawn only by the 'dots' kind; the default kind is a grid
        Conditional {
            effect: "pattern",
            field: "radius",
            with: vec![("kind", json!("dots"))],
        },
    ]
}

/// A 12 x 12 picture with a different, non-grey colour at (almost) every pixel, so a parameter of any colour step shows;
/// green and blue are squeezed to a shorter range than red, so a step that treats channels alike or apart shows too.
fn picture_values() -> Vec<f32> {
    (0..SIDE)
        .flat_map(|row| (0..SIDE).flat_map(move |column| (0..3_u8).map(move |channel| (row, column, channel))))
        .map(|(row, column, channel)| {
            f32::from((row * 7 + column * 13 + channel * 5) % 23) / (22.0 + 8.0 * f32::from(channel))
        })
        .collect()
}

fn whole(bound: Option<Scalar>) -> Option<i64> {
    match bound? {
        Scalar::Int(number) => Some(number),
        _ => None,
    }
}

fn real(bound: Option<Scalar>) -> Option<f64> {
    match bound? {
        Scalar::Int(number) => i32::try_from(number).ok().map(f64::from),
        Scalar::Float(number) => Some(number),
        _ => None,
    }
}

/// Other values a parameter could take: both bounds and the points a third and two thirds of the way, a switch flipped,
/// every other choice, two colours that are not the defaults of any effect, two lists of such colours.
fn candidates(field: &Field) -> Vec<Value> {
    match field.kind {
        Kind::Float => real(field.min)
            .zip(real(field.max))
            .map(|(low, high)| {
                let third = (high - low) / 3.0;
                [low, low + third, high - third, high]
                    .map(|number| json!(number))
                    .to_vec()
            })
            .unwrap_or_default(),
        Kind::Int => whole(field.min)
            .zip(whole(field.max))
            .map(|(low, high)| {
                let third = (high - low) / 3;
                [low, low + third, high - third, high]
                    .map(|number| json!(number))
                    .to_vec()
            })
            .unwrap_or_default(),
        Kind::Bool => match field.default {
            Scalar::Bool(on) => vec![json!(!on)],
            _ => Vec::new(),
        },
        Kind::Choice => field
            .choices
            .iter()
            .filter(|name| field.default != Scalar::Text(name))
            .map(|name| json!(name))
            .collect(),
        Kind::Color => vec![json!([0.9, 0.1, 0.2]), json!([0.1, 0.8, 0.3])],
        // a name: the test offers its own selection to any effect that is given one (see `render`)
        Kind::Field => vec![json!("ramp")],
        // a name: the test offers its own table to any effect that is given one (see `render`)
        Kind::Lut => vec![json!("table")],
        // the shortest list and one element more (where allowed), of colours that are not the default of any effect
        Kind::Colors => {
            let shortest = field.min_length.unwrap_or(1);
            [shortest, shortest + 1]
                .into_iter()
                .filter(|length| field.max_length.is_none_or(|most| *length <= most))
                .map(|length| {
                    Value::Array(
                        [[0.9, 0.1, 0.2], [0.1, 0.8, 0.3], [0.2, 0.2, 0.9]]
                            .into_iter()
                            .cycle()
                            .take(length)
                            .map(|channels| json!(channels))
                            .collect(),
                    )
                })
                .collect()
        }
    }
}

/// The effect's result for the parameters `given` (the rest at their defaults) on `picture`.
fn render(
    effect: &dyn Effect,
    table: &Table,
    given: &Map<String, Value>,
    picture: &Picture<'_>,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let params = table.check(effect.name(), given)?;
    // a name for a lookup table is a table the effect finds in its inputs: any name gets the negative
    let negative = Lut::new(
        2,
        [0.0; 3],
        [1.0; 3],
        (0..8_usize)
            .map(|entry| [entry % 2, entry / 2 % 2, entry / 4].map(|corner| if corner == 1 { 0.0 } else { 1.0 }))
            .collect(),
    )?;
    // a field named `ramp` is a ramp across the picture, any other name a bumpy selection (hollows and heights, so that an
    // effect that reads where the selection departs from its surroundings sees some)
    let selections = table
        .fields
        .iter()
        .filter(|field| field.kind == Kind::Field)
        .map(|field| {
            let side = usize::from(SIDE);
            let ramp = params[field.name] == "ramp";
            let values: Vec<f32> = (0..SIDE)
                .flat_map(|row| {
                    (0..SIDE).map(move |column| {
                        if ramp {
                            f32::from(column) / f32::from(SIDE - 1)
                        } else {
                            f32::from((row * 5 + column * 3) % 7) / 6.0
                        }
                    })
                })
                .collect();
            Layer::new(side, side, values).map(|layer| (field.name, layer))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut inputs = Inputs::default();
    for (name, layer) in &selections {
        inputs = inputs.with_field(*name, layer);
    }
    for field in table.fields.iter().filter(|field| field.kind == Kind::Lut) {
        if params[field.name].is_string() {
            inputs = inputs.with_lut(field.name, &negative);
        }
    }
    Ok(effect.apply(picture, Value::Object(params), &inputs)?.into_values())
}

fn differs(left: &[f32], right: &[f32]) -> bool {
    left.iter().zip(right).any(|(a, b)| (a - b).abs() > TOLERANCE)
}

/// Whether some other value of `field`, with `context` set, changes the result from what `context` alone gives.
fn changes(
    effect: &dyn Effect,
    table: &Table,
    field: &Field,
    context: &Map<String, Value>,
) -> Result<bool, Box<dyn Error>> {
    let values = picture_values();
    let picture = Picture::new(SIDE.into(), SIDE.into(), &values)?;
    let baseline = render(effect, table, context, &picture)?;
    for candidate in candidates(field) {
        let mut given = context.clone();
        given.insert(field.name.to_owned(), candidate);
        let result = match render(effect, table, &given, &picture) {
            // a value on the wrong side of the parameter it may not exceed is no alternative to try
            Err(error)
                if matches!(
                    error.downcast_ref::<CoreError>(),
                    Some(CoreError::Invalid {
                        constraint: Constraint::AtMostField(..),
                        ..
                    })
                ) =>
            {
                continue;
            }
            other => other?,
        };
        if differs(&baseline, &result) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// What is wrong with the effect's parameters, one sentence each: a parameter nothing reads, or a row of `rows` that
/// is not needed or does not help.
fn problems(effect: &dyn Effect, rows: &[Conditional]) -> Result<Vec<String>, Box<dyn Error>> {
    let Some(table) = effect.params() else {
        return Ok(Vec::new());
    };
    let mut problems = Vec::new();
    for field in &table.fields {
        let name = format!("{}.{}", effect.name(), field.name);
        let alone = changes(effect, &table, field, &Map::new())?;
        match rows
            .iter()
            .find(|row| row.effect == effect.name() && row.field == field.name)
        {
            None if !alone => problems.push(format!("{name} is never read: no other value changes the result")),
            Some(row) => {
                let context: Map<String, Value> = row
                    .with
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), value.clone()))
                    .collect();
                if alone {
                    problems.push(format!(
                        "{name} changes the result alone, so its conditional row is stale"
                    ));
                }
                if !changes(effect, &table, field, &context)? {
                    problems.push(format!(
                        "{name} changes nothing even under its condition {:?}",
                        row.with
                    ));
                }
            }
            None => {}
        }
    }
    Ok(problems)
}

/// Whether `row` names a parameter some effect of the core has.
fn names_a_parameter(row: &Conditional) -> Result<bool, Box<dyn Error>> {
    Ok(effect(row.effect)?
        .params()
        .is_some_and(|table| table.fields.iter().any(|field| field.name == row.field)))
}

/// A foreign effect that brightens by `gain` and declares `ignored`, which it never reads: the known-bad case.
struct Brighten;

impl Effect for Brighten {
    fn name(&self) -> &'static str {
        "brighten"
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn params(&self) -> Option<Table> {
        let float = |name, default| Field {
            name,
            kind: Kind::Float,
            meaning: "A decimal.",
            default: Scalar::Float(default),
            unit: None,
            min: Some(Scalar::Int(0)),
            max: Some(Scalar::Int(2)),
            cyclic: false,
            choices: &[],
            min_length: None,
            max_length: None,
            at_most: None,
            nullable: false,
            drivable: false,
            advanced: false,
        };
        Some(Table {
            doc: "The parameters of brighten.",
            fields: vec![float("gain", 1.0), float("ignored", 0.5)],
        })
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        params: Value,
        _inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        let gain = params["gain"].as_f64().unwrap_or(1.0);
        #[expect(clippy::cast_possible_truncation, reason = "a gain between 0 and 2 fits an f32")]
        let gain = gain as f32;
        Ok(picture.map(|pixel| pixel.map(|value| value * gain)))
    }
}

/// A foreign effect with a field and an optional colour: it darkens by the selection it is handed when `reads`, and
/// adds `tint` when one is set. With `reads` false it ignores its selection: the known-bad case for a field.
struct Follow {
    reads: bool,
}

impl Effect for Follow {
    fn name(&self) -> &'static str {
        "follow"
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn fields(&self) -> &[FieldParam] {
        static FIELDS: std::sync::OnceLock<[FieldParam; 1]> = std::sync::OnceLock::new();
        FIELDS.get_or_init(|| [FieldParam::new("follow", "depth")])
    }

    fn params(&self) -> Option<Table> {
        let base = Field {
            name: "follow",
            kind: Kind::Field,
            meaning: "A selection.",
            default: Scalar::Text("depth"),
            unit: None,
            min: None,
            max: None,
            cyclic: false,
            choices: &[],
            min_length: None,
            max_length: None,
            at_most: None,
            nullable: false,
            drivable: false,
            advanced: false,
        };
        let tint = Field {
            name: "tint",
            kind: Kind::Color,
            meaning: "A colour.",
            default: Scalar::Null,
            nullable: true,
            ..base.clone()
        };
        Some(Table {
            doc: "The parameters of follow.",
            fields: vec![base, tint],
        })
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        params: Value,
        inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        let layer = inputs.field("follow").map(Layer::values).unwrap_or_default();
        let tint = serde_json::from_value::<[f32; 3]>(params["tint"].clone()).ok();
        let width = picture.width();
        Ok(picture.map_at(|row, column, pixel| {
            let weight = if self.reads {
                1.0 - 0.5 * layer.get(row * width + column).copied().unwrap_or(0.0)
            } else {
                1.0
            };
            let mixed = pixel.map(|value| value * weight);
            tint.map_or(mixed, |tint| {
                [0, 1, 2].map(|channel| mixed[channel] * 0.5 + tint[channel] * 0.5)
            })
        }))
    }
}

#[test]
fn a_field_and_an_optional_colour_are_tried_and_found_read() -> Outcome {
    let found = problems(&Follow { reads: true }, &[])?;
    assert!(found.is_empty(), "{found:?}");
    Ok(())
}

#[test]
fn a_field_the_effect_never_reads_is_found_and_named() -> Outcome {
    let found = problems(&Follow { reads: false }, &[])?;
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("follow.follow") && found[0].contains("never read"),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn every_parameter_of_every_effect_of_the_core_is_read() -> Outcome {
    let rows = conditional();
    let names = effects();
    for name in ["grade", "pattern", "pixel_grid", "lut"] {
        assert!(names.iter().any(|known| known == name), "{name} is not among {names:?}");
    }
    let mut walked = 0;
    for name in &names {
        let effect = effect(name)?;
        if effect.params().is_some() {
            walked += 1;
        }
        let problems = problems(&*effect, &rows)?;
        assert!(problems.is_empty(), "{problems:?}");
    }
    assert!(walked >= 3, "only {walked} effects were walked");
    Ok(())
}

#[test]
fn a_parameter_the_effect_never_reads_is_found_and_named() -> Outcome {
    let found = problems(&Brighten, &[])?;
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("brighten.ignored") && found[0].contains("never read"),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn a_conditional_row_that_is_not_needed_or_does_not_help_is_found() -> Outcome {
    let row = |field, value| Conditional {
        effect: "brighten",
        field,
        with: vec![("gain", value)],
    };
    // `gain` matters alone, so a row for it is stale; `ignored` matters under no gain, so its row does not help
    let stale = problems(&Brighten, &[row("gain", json!(2.0))])?;
    assert!(
        stale
            .iter()
            .any(|text| text.contains("brighten.gain") && text.contains("stale")),
        "{stale:?}"
    );
    let useless = problems(&Brighten, &[row("ignored", json!(2.0))])?;
    assert!(
        useless
            .iter()
            .any(|text| text.contains("brighten.ignored") && text.contains("even under")),
        "{useless:?}"
    );
    Ok(())
}

#[test]
fn every_conditional_row_names_a_parameter_of_an_effect() -> Outcome {
    for row in conditional() {
        assert!(names_a_parameter(&row)?, "{}.{}", row.effect, row.field);
    }
    let bogus = Conditional {
        effect: "grade",
        field: "kelvin",
        with: Vec::new(),
    };
    assert!(!names_a_parameter(&bogus)?);
    Ok(())
}

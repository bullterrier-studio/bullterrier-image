//! Every parameter of every effect the core runs changes the result of that effect: the parameters are public fields
//! of a `Params` struct, so rustc no longer reports one that `render` never reads, and this test does.
//!
//! Each effect is applied to a fixed picture at its defaults, then once for each alternative value of each parameter
//! with only that parameter changed; a parameter is read if some alternative changes the picture. A parameter that
//! matters only under a condition (`grade.pivot` does nothing while `contrast` is 1) has a row in [`conditional`]: the
//! test checks it both ways, so a row that is no longer needed fails too.

use std::error::Error;

use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::{Category, Effect, Image, Picture, effect, effects};
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
fn conditional() -> Vec<Conditional> {
    vec![
        // contrast leaves values where they are at 1, whatever the pivot
        Conditional {
            effect: "grade",
            field: "pivot",
            with: vec![("contrast", json!(2.0))],
        },
        // a dot is drawn only by the 'dots' kind; the default kind is a grid
        Conditional {
            effect: "pattern",
            field: "radius",
            with: vec![("kind", json!("dots"))],
        },
    ]
}

/// A 12 x 12 picture with a different, non-grey colour at (almost) every pixel, so a parameter of any colour step shows.
fn picture_values() -> Vec<f32> {
    (0..SIDE)
        .flat_map(|row| (0..SIDE).flat_map(move |column| (0..3_u8).map(move |channel| (row, column, channel))))
        .map(|(row, column, channel)| f32::from((row * 7 + column * 13 + channel * 5) % 23) / 22.0)
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
/// every other choice, two colours that are not the defaults of any effect.
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
    Ok(effect.apply(picture, Value::Object(params))?.into_values())
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
        if differs(&baseline, &render(effect, table, &given, &picture)?) {
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
            drivable: false,
            advanced: false,
        };
        Some(Table {
            doc: "The parameters of brighten.",
            fields: vec![float("gain", 1.0), float("ignored", 0.5)],
        })
    }

    fn apply(&self, picture: &Picture<'_>, params: Value) -> Result<Image, bullterrier_image::Error> {
        let gain = params["gain"].as_f64().unwrap_or(1.0);
        #[expect(clippy::cast_possible_truncation, reason = "a gain between 0 and 2 fits an f32")]
        let gain = gain as f32;
        Ok(picture.map(|pixel| pixel.map(|value| value * gain)))
    }
}

#[test]
fn every_parameter_of_every_effect_of_the_core_is_read() -> Outcome {
    let rows = conditional();
    let names = effects();
    for name in ["grade", "pattern", "pixel_grid"] {
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

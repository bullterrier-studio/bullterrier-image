//! Checking a user's values against an effect's declared parameters.

use serde_json::{Map, Value};

use super::{Field, Kind, Scalar, Table};
use crate::{Constraint, Error};

impl Table {
    /// The complete parameters an effect takes for the values `given`, or the first rule they break.
    ///
    /// The result has every parameter, in normal form: a parameter not given has its declared default; a decimal is a
    /// float even when given as a whole number; a colour is three numbers 0..1 (given as `'#rrggbb'`, `'#rgb'` in
    /// either case, spaces around allowed, or three numbers). Rules: a name the effect does not declare is refused,
    /// listing the declared ones; a decimal takes any number but not a switch or text; a whole number takes only a
    /// whole one (`3.0` is not); a switch takes `true` or `false`; a choice takes a listed name; a number lies within
    /// its inclusive `min` and `max`, except a `cyclic` one (an angle whose range is one turn), which takes any real
    /// and wraps it into the range as `min + (value - min) mod (max - min)` when it lies outside. `given` is not changed.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `effect`, the parameter, the rule and the value.
    pub fn check(&self, effect: &str, given: &Map<String, Value>) -> Result<Map<String, Value>, Error> {
        let invalid = |field: &str, constraint, value: &Value| Error::Invalid {
            place: None,
            effect: effect.to_owned(),
            field: field.to_owned(),
            constraint,
            value: Box::new(value.clone()),
        };
        if let Some((name, value)) = given
            .iter()
            .find(|(name, _)| self.fields.iter().all(|field| field.name != name.as_str()))
        {
            let declared = self.fields.iter().map(|field| field.name).collect();
            return Err(invalid(name, Constraint::Unknown(declared), value));
        }
        self.fields
            .iter()
            .map(|field| {
                let value = given.get(field.name).cloned().unwrap_or_else(|| field.default.value());
                field
                    .normal(&value)
                    .map(|normal| (field.name.to_owned(), normal))
                    .map_err(|constraint| invalid(field.name, constraint, &value))
            })
            .collect()
    }
}

impl Scalar {
    /// The number a bound or default holds, if it is one.
    #[expect(clippy::cast_precision_loss, reason = "declared bounds are small whole numbers")]
    const fn number(self) -> Option<f64> {
        match self {
            Self::Int(whole) => Some(whole as f64),
            Self::Float(real) => Some(real),
            Self::Bool(_) | Self::Text(_) => None,
        }
    }

    /// The value as JSON.
    fn value(self) -> Value {
        match self {
            Self::Int(whole) => Value::from(whole),
            Self::Float(real) => Value::from(real),
            Self::Bool(flag) => Value::from(flag),
            Self::Text(text) => Value::from(text),
        }
    }
}

impl Field {
    /// `value` in normal form, or the rule it breaks.
    fn normal(&self, value: &Value) -> Result<Value, Constraint> {
        match self.kind {
            Kind::Float => value
                .as_f64()
                .ok_or(Constraint::Number)
                .and_then(|real| self.within(self.wrapped(real)))
                .map(Value::from),
            Kind::Int => {
                let whole = value.as_i64().ok_or(Constraint::WholeNumber)?;
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a whole number beyond 2^53 is far outside any range"
                )]
                self.within(whole as f64)?;
                Ok(Value::from(whole))
            }
            Kind::Bool => value.as_bool().map(Value::from).ok_or(Constraint::TrueOrFalse),
            Kind::Choice => value
                .as_str()
                .filter(|name| self.choices.contains(name))
                .map(Value::from)
                .ok_or(Constraint::OneOf(self.choices)),
            Kind::Color => colour(value).map(Value::from).ok_or(Constraint::Colour),
        }
    }

    /// `real` moved into the turn by whole turns when the field is cyclic and it lies outside.
    fn wrapped(&self, real: f64) -> f64 {
        match (
            self.cyclic,
            self.min.and_then(Scalar::number),
            self.max.and_then(Scalar::number),
        ) {
            (true, Some(low), Some(high)) if real < low || real > high => low + (real - low).rem_euclid(high - low),
            _ => real,
        }
    }

    /// `real` if it lies within the inclusive bounds, else the bound it breaks.
    fn within(&self, real: f64) -> Result<f64, Constraint> {
        if let Some((bound, low)) = self.min.and_then(|bound| Some((bound, bound.number()?)))
            && real < low
        {
            return Err(Constraint::AtLeast(bound));
        }
        if let Some((bound, high)) = self.max.and_then(|bound| Some((bound, bound.number()?)))
            && real > high
        {
            return Err(Constraint::AtMost(bound));
        }
        Ok(real)
    }
}

/// The three channels (0..1) a colour value writes: `'#rrggbb'`, `'#rgb'` or a list of three numbers.
fn colour(value: &Value) -> Option<[f64; 3]> {
    match value {
        Value::String(text) => hex(text.trim()),
        Value::Array(items) => {
            let [red, green, blue] = items.as_slice() else {
                return None;
            };
            let channels = [red.as_f64()?, green.as_f64()?, blue.as_f64()?];
            channels
                .iter()
                .all(|channel| (0.0..=1.0).contains(channel))
                .then_some(channels)
        }
        _ => None,
    }
}

/// The channels of `'#rrggbb'` or `'#rgb'` (either case): each byte over 255.
fn hex(text: &str) -> Option<[f64; 3]> {
    let digits: Vec<u32> = text
        .strip_prefix('#')?
        .chars()
        .map(|digit| digit.to_digit(16))
        .collect::<Option<_>>()?;
    let bytes = match *digits.as_slice() {
        // `#rgb` doubles each digit: `#0af` is `#00aaff`
        [red, green, blue] => [red * 17, green * 17, blue * 17],
        [r1, r2, g1, g2, b1, b2] => [r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2],
        _ => return None,
    };
    // constant: 8-bit levels
    Some(bytes.map(|byte| f64::from(byte) / 255.0))
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use crate::params::probe::Probe;
    use crate::params::{Declared, Scalar};
    use crate::{Constraint, Error};

    /// The probe's parameters checked for `given` (a JSON object).
    fn check(given: &Value) -> Result<Value, Error> {
        let given = given.as_object().cloned().unwrap_or_default();
        Probe::table().check("probe", &given).map(Value::Object)
    }

    #[test]
    fn nothing_given_gives_every_default_in_its_normal_form() {
        // a colour is three numbers, a float default is a float, a choice is its name
        let expected = json!({
            "cell": 32.0, "cells": 4, "angle": 0.5, "shape": "fourth_way", "keep": true,
            "tint": [1.0, 128.0 / 255.0, 0.0],
        });
        assert_eq!(check(&json!({})).ok(), Some(expected));
    }

    #[test]
    fn distinct_values_come_back_in_place() {
        let given = json!({"cell": 8, "cells": 7, "angle": 90.0, "shape": "checks", "keep": false, "tint": "#0af"});
        let expected = json!({
            "cell": 8.0, "cells": 7, "angle": 90.0, "shape": "checks", "keep": false,
            "tint": [0.0, 170.0 / 255.0, 1.0],
        });
        assert_eq!(check(&given).ok(), Some(expected));
    }

    #[test]
    fn one_value_given_leaves_the_rest_at_their_defaults() {
        let one = check(&json!({"cells": -2})).ok();
        let field = |name: &str| one.as_ref().map(|checked| checked[name].clone());
        assert_eq!((field("cells"), field("cell")), (Some(json!(-2)), Some(json!(32.0))));
    }

    /// (rule, given for one field, the field, what it turns into): values that pass, at and inside each bound.
    fn good() -> Vec<(&'static str, Value, &'static str, Value)> {
        vec![
            ("float at its lowest", json!({"cell": 2}), "cell", json!(2.0)),
            ("float at its highest", json!({"cell": 512.0}), "cell", json!(512.0)),
            ("float takes a whole number", json!({"cell": 100}), "cell", json!(100.0)),
            ("int at its lowest", json!({"cells": -3}), "cells", json!(-3)),
            ("int at its highest", json!({"cells": 32}), "cells", json!(32)),
            (
                "cyclic: 360 is inside the turn",
                json!({"angle": 360}),
                "angle",
                json!(360.0),
            ),
            ("cyclic: 0 is inside the turn", json!({"angle": 0}), "angle", json!(0.0)),
            (
                "cyclic: a fraction is inside",
                json!({"angle": 0.1}),
                "angle",
                json!(0.1),
            ),
            (
                "cyclic: past the turn wraps",
                json!({"angle": 725}),
                "angle",
                json!(5.0),
            ),
            (
                "cyclic: below the turn wraps",
                json!({"angle": -90}),
                "angle",
                json!(270.0),
            ),
            (
                "cyclic: a hair below wraps to the top",
                json!({"angle": -1e-20}),
                "angle",
                json!(360.0),
            ),
            (
                "a choice by name",
                json!({"shape": "stripes"}),
                "shape",
                json!("stripes"),
            ),
            (
                "colour: short hex",
                json!({"tint": "#0af"}),
                "tint",
                json!([0.0, 170.0 / 255.0, 1.0]),
            ),
            (
                "colour: capital hex, spaces around",
                json!({"tint": " #FF8000 "}),
                "tint",
                json!([1.0, 128.0 / 255.0, 0.0]),
            ),
            (
                "colour: three whole numbers",
                json!({"tint": [0, 1, 1]}),
                "tint",
                json!([0.0, 1.0, 1.0]),
            ),
            (
                "colour: three decimals",
                json!({"tint": [0.25, 0.5, 0.75]}),
                "tint",
                json!([0.25, 0.5, 0.75]),
            ),
            ("bool", json!({"keep": false}), "keep", json!(false)),
        ]
    }

    #[test]
    fn values_the_declaration_allows_are_accepted_in_normal_form() {
        for (rule, given, field, expected) in good() {
            let result = check(&given);
            assert_eq!(
                result.as_ref().ok().map(|checked| checked[field].clone()),
                Some(expected),
                "{rule}: {result:?}"
            );
        }
    }

    /// (rule, given, the field blamed, the rule broken, the value shown).
    #[expect(clippy::too_many_lines, reason = "a table of rows, one rule each")]
    fn bad() -> Vec<(&'static str, Value, &'static str, Constraint, Value)> {
        let shapes = &["stripes", "checks", "fourth_way"][..];
        let fields = vec!["cell", "cells", "angle", "shape", "keep", "tint"];
        vec![
            (
                "unknown key",
                json!({"kelvin": 5000}),
                "kelvin",
                Constraint::Unknown(fields),
                json!(5000),
            ),
            (
                "float below its lowest",
                json!({"cell": 1.5}),
                "cell",
                Constraint::AtLeast(Scalar::Int(2)),
                json!(1.5),
            ),
            (
                "float above its highest",
                json!({"cell": 512.5}),
                "cell",
                Constraint::AtMost(Scalar::Int(512)),
                json!(512.5),
            ),
            (
                "float as text",
                json!({"cell": "8"}),
                "cell",
                Constraint::Number,
                json!("8"),
            ),
            (
                "float as bool",
                json!({"cell": true}),
                "cell",
                Constraint::Number,
                json!(true),
            ),
            (
                "float as null",
                json!({"cell": null}),
                "cell",
                Constraint::Number,
                json!(null),
            ),
            (
                "int below its lowest",
                json!({"cells": -4}),
                "cells",
                Constraint::AtLeast(Scalar::Int(-3)),
                json!(-4),
            ),
            (
                "int above its highest",
                json!({"cells": 33}),
                "cells",
                Constraint::AtMost(Scalar::Int(32)),
                json!(33),
            ),
            (
                "int as a whole decimal",
                json!({"cells": 3.0}),
                "cells",
                Constraint::WholeNumber,
                json!(3.0),
            ),
            (
                "int as a fraction",
                json!({"cells": 2.5}),
                "cells",
                Constraint::WholeNumber,
                json!(2.5),
            ),
            (
                "int as bool",
                json!({"cells": true}),
                "cells",
                Constraint::WholeNumber,
                json!(true),
            ),
            (
                "choice not listed",
                json!({"shape": "stripe"}),
                "shape",
                Constraint::OneOf(shapes),
                json!("stripe"),
            ),
            (
                "choice as number",
                json!({"shape": 2}),
                "shape",
                Constraint::OneOf(shapes),
                json!(2),
            ),
            (
                "bool as number",
                json!({"keep": 1}),
                "keep",
                Constraint::TrueOrFalse,
                json!(1),
            ),
            (
                "bool as text",
                json!({"keep": "true"}),
                "keep",
                Constraint::TrueOrFalse,
                json!("true"),
            ),
            (
                "colour: two hex digits",
                json!({"tint": "#12"}),
                "tint",
                Constraint::Colour,
                json!("#12"),
            ),
            (
                "colour: not hex",
                json!({"tint": "#gggggg"}),
                "tint",
                Constraint::Colour,
                json!("#gggggg"),
            ),
            (
                "colour: no hash",
                json!({"tint": "ff8000"}),
                "tint",
                Constraint::Colour,
                json!("ff8000"),
            ),
            (
                "colour: a channel above 1",
                json!({"tint": [0, 0, 2]}),
                "tint",
                Constraint::Colour,
                json!([0, 0, 2]),
            ),
            (
                "colour: a channel below 0",
                json!({"tint": [-0.5, 0, 1]}),
                "tint",
                Constraint::Colour,
                json!([-0.5, 0, 1]),
            ),
            (
                "colour: two channels",
                json!({"tint": [0, 0]}),
                "tint",
                Constraint::Colour,
                json!([0, 0]),
            ),
            (
                "colour: a bool channel",
                json!({"tint": [true, 0, 0]}),
                "tint",
                Constraint::Colour,
                json!([true, 0, 0]),
            ),
            (
                "colour: a number",
                json!({"tint": 7}),
                "tint",
                Constraint::Colour,
                json!(7),
            ),
        ]
    }

    #[test]
    fn values_that_break_a_rule_are_refused_naming_the_effect_the_field_the_rule_and_the_value() {
        for (rule, given, field, constraint, value) in bad() {
            let result = check(&given);
            assert!(
                matches!(
                    &result,
                    Err(Error::Invalid { place: None, effect, field: blamed, constraint: broken, value: shown })
                        if effect == "probe" && blamed == field && *broken == constraint && **shown == value
                ),
                "{rule}: {result:?}"
            );
        }
    }

    #[test]
    fn a_refusal_reads_in_one_sentence_like_the_python_package_says_it() {
        let message = |given: Value| check(&given).err().map(|error| error.to_string());
        assert_eq!(
            message(json!({"cell": 1.5})).as_deref(),
            Some("effect 'probe': `cell` must be >= 2, got 1.5")
        );
        assert_eq!(
            message(json!({"cells": 33})).as_deref(),
            Some("effect 'probe': `cells` must be <= 32, got 33")
        );
        assert_eq!(
            message(json!({"shape": "stripe"})).as_deref(),
            Some("effect 'probe': `shape` must be one of stripes, checks, fourth_way, got \"stripe\"")
        );
        assert_eq!(
            message(json!({"kelvin": 1})).as_deref(),
            Some(
                "effect 'probe': `kelvin` is not a parameter; the parameters are: cell, cells, angle, shape, keep, tint"
            )
        );
    }

    #[test]
    fn the_given_map_is_left_as_it_was() {
        let given: Map<String, Value> = json!({"cell": 8}).as_object().cloned().unwrap_or_default();
        let before = given.clone();
        assert!(Probe::table().check("probe", &given).is_ok());
        assert_eq!(given, before);
    }
}

//! A table written as data: the ecosystem's parameter format, version 3 (`docs/parameter-format/parameters.md`), read into
//! a [`Table`] for an effect declared at run time, and a table written as a JSON schema. A table the derive writes at build
//! time and one read here are the same value, so the core checks, scales and describes both alike.
//!
//! The strings of a table read here live as long as the process (a [`Table`] holds `&'static str`, as the derive writes
//! them): one effect declared at run time costs the text of its table, once.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use super::{Field, Kind, Scalar, Table, Unit};
use crate::{Error, recipe::STEP_KEYS};

/// The units, in the format's order.
const UNITS: [Unit; 8] = [
    Unit::Px,
    Unit::Fraction,
    Unit::Degrees,
    Unit::Stops,
    Unit::Count,
    Unit::Ratio,
    Unit::Cycles,
    Unit::Density,
];

/// The names a parameter may not take besides the step keys: the inputs the runner offers an effect.
const INPUT_NAMES: [&str; 2] = ["partition", "size_ratio"];

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

fn broken(parameter: &str, why: impl std::fmt::Display) -> Error {
    Error::Declaration(format!("parameter {parameter:?}: {why}"))
}

impl Kind {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "float" => Self::Float,
            "int" => Self::Int,
            "bool" => Self::Bool,
            "choice" => Self::Choice,
            "color" => Self::Color,
            "colors" => Self::Colors,
            "field" => Self::Field,
            "lut" => Self::Lut,
            "text" => Self::Text,
            "picture" => Self::Picture,
            _ => return None,
        })
    }
}

impl Table {
    /// The table `description` writes: `{"doc": text, "fields": [parameter, ..]}`, each parameter as the format describes it
    /// (the form a table serialises to). For an effect declared at run time, whose strings the table keeps for the life of
    /// the process.
    ///
    /// # Errors
    /// [`Error::Declaration`] naming the parameter and the rule it breaks: a parameter that [`Field::from_json`] refuses,
    /// a name written twice, an `at_most` that names no other number of the same unit.
    pub fn from_json(description: &Value) -> Result<Self, Error> {
        let table = description
            .as_object()
            .ok_or_else(|| Error::Declaration("a table is a JSON object with `doc` and `fields`".to_owned()))?;
        let doc = table
            .get("doc")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Declaration("a table has a `doc` text".to_owned()))?;
        let fields = table
            .get("fields")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Declaration("a table has a list of `fields`".to_owned()))?
            .iter()
            .map(Field::from_json)
            .collect::<Result<Vec<_>, _>>()?;
        let mut names = BTreeSet::new();
        for field in &fields {
            if !names.insert(field.name) {
                return Err(broken(field.name, "is declared twice"));
            }
        }
        for field in &fields {
            let Some(other) = field.at_most else { continue };
            let limit = fields
                .iter()
                .find(|candidate| candidate.name == other)
                .ok_or_else(|| broken(field.name, format!("`at_most` names {other:?}, which is no parameter")))?;
            if other == field.name || !matches!(limit.kind, Kind::Float | Kind::Int) || limit.unit != field.unit {
                return Err(broken(
                    field.name,
                    format!("`at_most` names {other:?}, which is not another number in the same unit"),
                ));
            }
        }
        Ok(Self { doc: leak(doc), fields })
    }

    /// The JSON schema (draft 2020-12) of the values of this table's parameters, as an object: each parameter a property
    /// with its `description`, `default`, bounds and the table's own keys `unit`, `cyclic`, `drivable` and `advanced`;
    /// nothing else is allowed and nothing is required (every parameter has a default).
    #[must_use]
    pub fn json_schema(&self) -> Value {
        let properties: Map<String, Value> = self
            .fields
            .iter()
            .map(|field| (field.name.to_owned(), field.json_schema()))
            .collect();
        json!({
            "type": "object",
            "description": self.doc,
            "properties": properties,
            "additionalProperties": false,
            "required": [],
        })
    }
}

impl Field {
    /// The parameter `description` writes, in the form a field serialises to; every rule of the format holds for it.
    ///
    /// # Errors
    /// [`Error::Declaration`] naming the parameter and the first rule it breaks: a name that is not letters, digits and `_`
    /// from a letter, or that a step or an input already uses; an unknown kind or unit; a key that is missing; a number
    /// without a unit or with `min` above `max`; a `cyclic` that is not a turn of degrees or cycles; choices that are none or
    /// repeat; lengths that contradict; a `drivable` that is not a decimal; a default the parameter itself would refuse.
    pub fn from_json(description: &Value) -> Result<Self, Error> {
        let object = description
            .as_object()
            .ok_or_else(|| Error::Declaration("a parameter is a JSON object".to_owned()))?;
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Declaration("a parameter has a `name`".to_owned()))?;
        read(name, object)
    }

    fn json_schema(&self) -> Value {
        let mut schema = match self.kind {
            Kind::Float => json!({"type": "number"}),
            Kind::Int => json!({"type": "integer"}),
            Kind::Bool => json!({"type": "boolean"}),
            Kind::Choice => json!({"type": "string", "enum": self.choices}),
            Kind::Color => colour_schema(),
            Kind::Colors => {
                let mut list = json!({"type": "array", "items": colour_schema()});
                schema_lengths(&mut list, self, "minItems", "maxItems");
                list
            }
            Kind::Text => {
                let mut text = json!({"type": "string"});
                schema_lengths(&mut text, self, "minLength", "maxLength");
                text
            }
            Kind::Field | Kind::Lut | Kind::Picture => json!({"type": "string", "minLength": 1}),
        };
        if self.nullable || matches!(self.kind, Kind::Lut | Kind::Picture) {
            schema = json!({"anyOf": [schema, {"type": "null"}]});
        }
        if let Some(object) = schema.as_object_mut() {
            object.insert("description".to_owned(), json!(self.meaning));
            object.insert("default".to_owned(), self.default.json());
            if let Some(low) = self.min {
                object.insert("minimum".to_owned(), low.json());
            }
            if let Some(high) = self.max {
                object.insert("maximum".to_owned(), high.json());
            }
            object.insert("unit".to_owned(), json!(self.unit.map(Unit::name)));
            object.insert("cyclic".to_owned(), json!(self.cyclic));
            object.insert("drivable".to_owned(), json!(self.drivable));
            object.insert("advanced".to_owned(), json!(self.advanced));
        }
        schema
    }
}

/// A colour as a value is written: `'#rgb'` or `'#rrggbb'`, or three numbers from 0 to 1.
fn colour_schema() -> Value {
    json!({"anyOf": [
        {"type": "string", "pattern": "^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$"},
        {"type": "array", "items": {"type": "number", "minimum": 0, "maximum": 1}, "minItems": 3, "maxItems": 3},
    ]})
}

fn schema_lengths(schema: &mut Value, field: &Field, least: &str, most: &str) {
    if let Some(object) = schema.as_object_mut() {
        if let Some(low) = field.min_length {
            object.insert(least.to_owned(), json!(low));
        }
        if let Some(high) = field.max_length {
            object.insert(most.to_owned(), json!(high));
        }
    }
}

impl Scalar {
    fn json(self) -> Value {
        self.value()
    }
}

/// The whole number `value` holds, if it is one (`3` and `3.0` both are).
fn whole(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        value
            .as_f64()
            .filter(|real| real.fract() == 0.0 && real.abs() < 9e15)
            .map(|real| {
                #[expect(clippy::cast_possible_truncation, reason = "a whole number below 9e15 fits")]
                let whole = real as i64;
                whole
            })
    })
}

/// A bound or default number as the declaration writes it: whole if it is a JSON integer, else a decimal.
fn scalar(value: &Value) -> Option<Scalar> {
    value
        .as_i64()
        .map(Scalar::Int)
        .or_else(|| value.as_f64().map(Scalar::Float))
}

fn required<'a>(name: &str, object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, Error> {
    object.get(key).ok_or_else(|| broken(name, format!("has no `{key}`")))
}

fn flag(name: &str, object: &Map<String, Value>, key: &str) -> Result<bool, Error> {
    match object.get(key) {
        None => Ok(false),
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(_) => Err(broken(name, format!("`{key}` is true or false"))),
    }
}

fn length(name: &str, object: &Map<String, Value>, key: &str) -> Result<Option<usize>, Error> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .map(Some)
            .ok_or_else(|| broken(name, format!("`{key}` is a whole number from 0"))),
    }
}

/// Everything about the parameter called `name` that its `object` writes.
fn read(name: &str, object: &Map<String, Value>) -> Result<Field, Error> {
    let mut characters = name.chars();
    let identifier = characters.next().is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|next| next.is_ascii_alphanumeric() || next == '_');
    if !identifier {
        return Err(broken(name, "is not a name: letters, digits and `_`, from a letter"));
    }
    if STEP_KEYS.contains(&name) || INPUT_NAMES.contains(&name) {
        return Err(broken(name, "is the name of a step key or an input of the runner"));
    }
    let kind_name = required(name, object, "kind")?
        .as_str()
        .ok_or_else(|| broken(name, "`kind` is a text"))?;
    let kind = Kind::named(kind_name).ok_or_else(|| broken(name, format!("has the unknown kind {kind_name:?}")))?;
    let meaning = required(name, object, "meaning")?
        .as_str()
        .ok_or_else(|| broken(name, "`meaning` is a text"))?;
    let cyclic = match required(name, object, "cyclic")? {
        Value::Bool(flag) => *flag,
        _ => return Err(broken(name, "`cyclic` is true or false")),
    };
    let (unit, min, max) = number_parts(name, object, kind, cyclic)?;
    let nullable = nullability(name, object, kind)?;
    let (min_length, max_length, min_distinct) = lengths(name, object, kind)?;
    let default = default_of(name, kind, required(name, object, "default")?, nullable)?;
    let drivable = flag(name, object, "drivable")?;
    if drivable && kind != Kind::Float {
        return Err(broken(name, "is drivable, which only a decimal can be"));
    }
    let at_most = match object.get("at_most") {
        None | Some(Value::Null) => None,
        Some(Value::String(other)) if matches!(kind, Kind::Float | Kind::Int) => Some(leak(other)),
        Some(_) => return Err(broken(name, "`at_most` names another number")),
    };
    let field = Field {
        name: leak(name),
        kind,
        meaning: leak(meaning),
        default,
        unit,
        min,
        max,
        cyclic,
        choices: choices_of(name, object, kind)?,
        min_length,
        max_length,
        min_distinct,
        at_most,
        nullable,
        drivable,
        advanced: flag(name, object, "advanced")?,
    };
    let written: Vec<&str> = match default {
        Scalar::Text(text) if kind == Kind::Color => vec![text],
        Scalar::List(items) => items.to_vec(),
        _ => Vec::new(),
    };
    if written.iter().any(|text| !canonical(text)) {
        return Err(broken(name, "a colour default is written `#rrggbb`, in lower case"));
    }
    if let Err(constraint) = field.normal(&default.value()) {
        return Err(broken(
            name,
            format!("has a default that it would refuse itself: {constraint}"),
        ));
    }
    Ok(field)
}

/// The unit, `min` and `max` of a parameter.
type NumberParts = (Option<Unit>, Option<Scalar>, Option<Scalar>);

/// `min_length`, `max_length` and `min_distinct`.
type Lengths = (Option<usize>, Option<usize>, Option<usize>);

/// The unit and the bounds of a parameter: a number has a unit of the closed list and `min` and `max` of its kind (whole for
/// a whole number), `min` not above `max`, and is cyclic only as a turn; anything else has none of the three.
fn number_parts(name: &str, object: &Map<String, Value>, kind: Kind, cyclic: bool) -> Result<NumberParts, Error> {
    let numeric = matches!(kind, Kind::Float | Kind::Int);
    let unit = match required(name, object, "unit")? {
        Value::Null if !numeric => None,
        Value::String(text) if numeric => Some(
            UNITS
                .iter()
                .copied()
                .find(|known| known.name() == text)
                .ok_or_else(|| broken(name, format!("has the unit {text:?}, outside the closed list")))?,
        ),
        _ if numeric => return Err(broken(name, "a number has one of the units")),
        _ => return Err(broken(name, "has a unit, which only a number has")),
    };
    if !numeric {
        for key in ["min", "max"] {
            if !required(name, object, key)?.is_null() {
                return Err(broken(name, format!("`{key}` is null: only a number has bounds")));
            }
        }
        if cyclic {
            return Err(broken(name, "is cyclic, which only a decimal can be"));
        }
        return Ok((None, None, None));
    }
    let bound = |key: &str| {
        let value = required(name, object, key)?;
        let (found, what) = if kind == Kind::Int {
            (whole(value).map(Scalar::Int), "whole number")
        } else {
            (scalar(value), "number")
        };
        found.ok_or_else(|| broken(name, format!("`{key}` is a {what}")))
    };
    let (min, max) = (bound("min")?, bound("max")?);
    let range = min.number().zip(max.number()).map(|(low, high)| (low, high - low));
    if range.is_some_and(|(_, span)| span < 0.0) {
        return Err(broken(name, "has `min` above `max`"));
    }
    if cyclic {
        let turn = match unit {
            Some(Unit::Degrees) => Some(360.0),
            Some(Unit::Cycles) => Some(1.0),
            _ => None,
        };
        if kind != Kind::Float || turn.is_none() || range.map(|(_, span)| span) != turn {
            return Err(broken(
                name,
                "is cyclic, which only a decimal in degrees (a range of 360) or cycles (1) is",
            ));
        }
    }
    Ok((unit, Some(min), Some(max)))
}

/// Whether a parameter may be left out (`nullable`): never a switch or a choice, always a picture.
fn nullability(name: &str, object: &Map<String, Value>, kind: Kind) -> Result<bool, Error> {
    let nullable = flag(name, object, "nullable")?;
    if nullable && matches!(kind, Kind::Bool | Kind::Choice) {
        return Err(broken(name, "a switch and a choice are never nullable"));
    }
    if kind == Kind::Picture && !nullable {
        return Err(broken(name, "a picture is always `nullable`"));
    }
    Ok(nullable)
}

/// The names of a choice: at least one, all different and not empty. None for any other kind.
fn choices_of(name: &str, object: &Map<String, Value>, kind: Kind) -> Result<&'static [&'static str], Error> {
    let given = object
        .get("choices")
        .and_then(Value::as_array)
        .filter(|names| !names.is_empty());
    match (kind, given) {
        (Kind::Choice, Some(names)) => {
            let mut seen = BTreeSet::new();
            let mut listed = Vec::new();
            for choice in names {
                let text = choice
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| broken(name, "`choices` are non-empty texts"))?;
                if !seen.insert(text) {
                    return Err(broken(name, format!("lists the choice {text:?} twice")));
                }
                listed.push(leak(text));
            }
            Ok(Box::leak(listed.into_boxed_slice()))
        }
        (Kind::Choice, None) => Err(broken(name, "a choice has `choices`, at least one")),
        (_, None) => Ok(&[]),
        (_, Some(_)) => Err(broken(name, "has `choices`, which only a choice has")),
    }
}

/// `min_length`, `max_length` and `min_distinct`, for the kinds that have them.
fn lengths(name: &str, object: &Map<String, Value>, kind: Kind) -> Result<Lengths, Error> {
    let (min_length, max_length, min_distinct) = (
        length(name, object, "min_length")?,
        length(name, object, "max_length")?,
        length(name, object, "min_distinct")?,
    );
    if !matches!(kind, Kind::Colors | Kind::Text) && (min_length.is_some() || max_length.is_some()) {
        return Err(broken(
            name,
            "has a length, which only a list of colours and a text have",
        ));
    }
    if kind != Kind::Text && min_distinct.is_some() {
        return Err(broken(name, "has `min_distinct`, which only a text has"));
    }
    if kind == Kind::Colors && min_length.is_none_or(|least| least == 0) {
        return Err(broken(name, "a list of colours has a `min_length` of at least 1"));
    }
    if min_distinct == Some(0) {
        return Err(broken(name, "`min_distinct` is at least 1"));
    }
    if let (Some(least), Some(most)) = (min_length, max_length)
        && least > most
    {
        return Err(broken(name, "has `min_length` above `max_length`"));
    }
    Ok((min_length, max_length, min_distinct))
}

/// The default `value` as the declaration holds it, of the right sort for `kind`; null only where it may be.
fn default_of(name: &str, kind: Kind, value: &Value, nullable: bool) -> Result<Scalar, Error> {
    let default = match (kind, value) {
        (_, Value::Null) => Scalar::Null,
        (Kind::Float, number) => scalar(number).ok_or_else(|| broken(name, "has a default that is no number"))?,
        (Kind::Int, number) => whole(number)
            .map(Scalar::Int)
            .ok_or_else(|| broken(name, "has a default that is no whole number"))?,
        (Kind::Bool, Value::Bool(flag)) => Scalar::Bool(*flag),
        (Kind::Choice | Kind::Color | Kind::Field | Kind::Text, Value::String(text)) => Scalar::Text(leak(text)),
        (Kind::Colors, Value::Array(items)) => Scalar::List(Box::leak(
            items
                .iter()
                .map(|item| item.as_str().map(leak))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| broken(name, "has a default list that holds more than colour texts"))?
                .into_boxed_slice(),
        )),
        _ => return Err(broken(name, "has a default that does not fit its kind")),
    };
    if default == Scalar::Null && !(nullable || matches!(kind, Kind::Lut | Kind::Picture)) {
        return Err(broken(name, "has a default of null but is not `nullable`"));
    }
    if matches!(kind, Kind::Lut | Kind::Picture) && default != Scalar::Null {
        return Err(broken(name, "a lookup table and a picture have no default but none"));
    }
    Ok(default)
}

/// `text` is a colour written `#rrggbb` in lower case: the one spelling a default has.
fn canonical(text: &str) -> bool {
    text.len() == 7
        && text.starts_with('#')
        && text[1..]
            .chars()
            .all(|digit| digit.is_ascii_digit() || ('a'..='f').contains(&digit))
}

//! The parameters of a recipe that can be set by address: what a sweep varies and a control drives.
//!
//! A parameter is a number or a colour. An address names one: `grade.exposure` (a step by its effect, its `name`, or `grain#2` among several of one
//! effect), `pulse.exposure.end` (the end of a drive; also `.start` and `.levels`), `pulse.opacity`, `each.feather`,
//! `each.looks[0].exposure`, `region.near.low` and `frame.angle`; a colour is `point_light.color`.

use std::borrow::Cow;

use serde_json::{Map, Value};

use super::near::closest;
use super::rows::{EACH_OPACITY, EACH_SEED, FEATHER, FRAME_ROWS, LEVELS, OPACITY, REGION_ROWS, Row};
use super::{Entry, Recipe, Step};
use crate::documentation::quoted as q;
use crate::params::{Color, Kind, Scalar, Unit};
use crate::{Error, Presets};

/// What a parameter holds: a number or a colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParameterKind {
    /// A number: it has a unit and a range.
    Number,
    /// A colour (`point_light.color`): it has none of these.
    Color,
}

/// The value of a parameter: what [`Recipe::with_values`] sets and [`Parameter::value`] reports.
///
/// A colour is stored as sRGB-encoded floats from 0 to 1 and written out by [`Color::to_json`]; every spelling a person may
/// write (`'#rrggbb'`, `'#rgb'`, `[r, g, b]`) is read by [`Setting::read`] into the same one. Alpha is not stored yet: no
/// effect has a colour with one.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Setting {
    /// A number.
    Number(f64),
    /// A colour.
    Color(Color),
}

impl From<f64> for Setting {
    fn from(number: f64) -> Self {
        Self::Number(number)
    }
}

impl From<Color> for Setting {
    fn from(color: Color) -> Self {
        Self::Color(color)
    }
}

impl Setting {
    /// The setting `value` writes, for the address `address` it is meant for: a JSON number is a number, a text
    /// (`'#rrggbb'` or `'#rgb'`) or a list of three numbers from 0 to 1 is a colour. The one reading of a setting that the
    /// bindings share.
    ///
    /// # Errors
    /// [`Error::Recipe`] naming `address` and the spellings a colour may take, for anything else.
    pub fn read(address: &str, value: &Value) -> Result<Self, Error> {
        match value {
            Value::Number(number) => number
                .as_f64()
                .map(Self::Number)
                .ok_or_else(|| Error::Recipe(format!("{}: {value} is not a finite number", q(address)))),
            Value::String(_) | Value::Array(_) => Color::parse(value).map(Self::Color).ok_or_else(|| {
                Error::Recipe(format!(
                    "{}: {value} is not a colour; write '#rrggbb', '#rgb' or [r, g, b] with each from 0 to 1",
                    q(address)
                ))
            }),
            _ => Err(Error::Recipe(format!(
                "{}: {value} is neither a number nor a colour; give a number, or a colour as '#rrggbb', '#rgb' or [r, g, b]",
                q(address)
            ))),
        }
    }

    /// The setting as JSON: a number, or a colour as [`Color::to_json`] writes it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Number(number) => Value::from(*number),
            Self::Color(color) => color.to_json(),
        }
    }
}

/// One parameter of a recipe that can be set (a number or a colour), and what it may be.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Parameter {
    /// Where it is: see [`Recipe::with_values`].
    pub address: String,
    /// Whether it is a number or a colour.
    pub kind: ParameterKind,
    /// Its value now, or none for an optional parameter the recipe leaves unset (the frame's `aspect`, an optional colour
    /// such as `bloom.color`). A parameter a step does not write is at its effect's default.
    pub value: Option<Setting>,
    /// Its unit: `px`, `fraction`, `degrees`, `count`...; empty for a colour.
    pub unit: String,
    /// What it does.
    pub doc: String,
    /// A number that must be whole: a value must be one.
    pub whole: bool,
    /// A direction in degrees: a value and the value a turn further are the same, and any value is allowed.
    pub cyclic: bool,
    /// The lowest value, inclusive (for a cyclic number, where its turn starts); none for a colour.
    pub min: Option<f64>,
    /// The highest value, inclusive (for a cyclic number, where its turn ends); none for a colour.
    pub max: Option<f64>,
    /// An optional colour: the recipe may leave it unset, and then the effect does without. Setting it gives it a value;
    /// to unset it again, take it out of the step's `params`: `with_values` sets and never unsets.
    pub nullable: bool,
}

/// What an address points at.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Home {
    /// The step of that index (counting from 0).
    Step(usize),
    /// The region of that name.
    Region(String),
    /// The frame.
    Frame,
}

/// A parameter of a recipe found by its address.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Located {
    /// Where the parameter lives.
    pub place: Home,
    /// Where in it: the rest of the address (`exposure`, `exposure.end`, `looks[0].exposure`, `low`, `angle`).
    pub rest: String,
    /// The parameter.
    pub parameter: Parameter,
}

fn parameter(address: String, row: &Row, value: Option<Setting>) -> Parameter {
    let number = row.kind == ParameterKind::Number;
    Parameter {
        address,
        kind: row.kind,
        value,
        unit: row.unit.to_string(),
        doc: row.doc.to_string(),
        whole: row.whole,
        cyclic: row.cyclic,
        min: number.then_some(row.min),
        max: number.then_some(row.max),
        nullable: row.nullable,
    }
}

impl Recipe {
    /// What each step's address starts with, in order: its `name`, else its effect (`each` for an each step) when it is the
    /// recipe's only step of that kind, else the effect and its number among those (`grain#2`).
    fn heads(&self) -> Vec<String> {
        let kinds: Vec<&str> = self.steps.iter().map(called).collect();
        let mut seen: Vec<usize> = vec![0; kinds.len()];
        kinds
            .iter()
            .zip(&self.steps)
            .enumerate()
            .map(|(at, (kind, entry))| {
                seen[at] = kinds[..=at].iter().filter(|other| *other == kind).count();
                let total = kinds.iter().filter(|other| *other == kind).count();
                name_of(entry).map_or_else(
                    || {
                        if total == 1 {
                            (*kind).to_owned()
                        } else {
                            format!("{kind}#{}", seen[at])
                        }
                    },
                    str::to_owned,
                )
            })
            .collect()
    }

    /// The numbers of the step `entry` by the rest of an address: an effect's (a driven one as `.start`, `.end` and
    /// `.levels`), then `opacity`; an each step's `feather`, `opacity`, `seed` and its looks' as `looks[0].exposure`.
    fn step_numbers(entry: &Entry) -> Result<Vec<(String, Row)>, Error> {
        match entry {
            Entry::Each(each) => {
                let mut found = vec![
                    ("feather".to_owned(), FEATHER),
                    ("opacity".to_owned(), EACH_OPACITY),
                    ("seed".to_owned(), EACH_SEED),
                ];
                for (index, look) in each.looks.iter().enumerate() {
                    if let Some(look) = look {
                        for (rest, row) in Self::effect_numbers(look)? {
                            found.push((format!("looks[{index}].{rest}"), row));
                        }
                    }
                }
                Ok(found)
            }
            Entry::Step(step) => Self::effect_numbers(step),
        }
    }

    fn effect_numbers(step: &Step) -> Result<Vec<(String, Row)>, Error> {
        let mut found = Vec::new();
        let table = crate::registry::effect(&step.effect)?.params();
        for field in &table.fields {
            if field.kind == Kind::Color {
                found.push((
                    field.name.to_owned(),
                    Row {
                        field: field.name,
                        kind: ParameterKind::Color,
                        nullable: field.nullable,
                        unit: Cow::Borrowed(""),
                        doc: Cow::Borrowed(field.meaning),
                        whole: false,
                        cyclic: false,
                        min: 0.0,
                        max: 1.0,
                    },
                ));
                continue;
            }
            let (Kind::Float | Kind::Int, Some(min), Some(max)) = (field.kind, field.min, field.max) else {
                continue;
            };
            let (min, max) = (scalar(min), scalar(max));
            let row = Row {
                field: field.name,
                kind: ParameterKind::Number,
                nullable: false,
                unit: Cow::Borrowed(field.unit.map_or("", Unit::name)),
                doc: Cow::Borrowed(field.meaning),
                whole: field.kind == Kind::Int,
                cyclic: field.cyclic,
                min,
                max,
            };
            if matches!(step.params.get(field.name), Some(Value::Object(_))) {
                found.push((format!("{}.start", field.name), row.clone()));
                found.push((format!("{}.end", field.name), row));
                found.push((format!("{}.levels", field.name), LEVELS));
            } else {
                found.push((field.name.to_owned(), row));
            }
        }
        found.push(("opacity".to_owned(), OPACITY));
        Ok(found)
    }

    /// The numbers of `target`, by the rest of an address.
    fn numbers_of(&self, target: &Home) -> Result<Vec<(String, Row)>, Error> {
        Ok(match target {
            Home::Step(index) => Self::step_numbers(&self.steps[*index])?,
            Home::Region(name) => {
                let kind = self.region_kind(name);
                REGION_ROWS
                    .iter()
                    .filter(|(known, _)| *known == kind)
                    .flat_map(|(_, rows)| rows.iter())
                    .map(|row| (row.field.to_owned(), row.clone()))
                    .collect()
            }
            Home::Frame => FRAME_ROWS
                .iter()
                .map(|row| (row.field.to_owned(), row.clone()))
                .collect(),
        })
    }

    /// The value now of the parameter `rest` of `target`.
    fn value_of(&self, target: &Home, rest: &str) -> Option<Setting> {
        let number = |value: f64| Some(Setting::Number(value));
        match target {
            Home::Frame => serde_json::to_value(&self.frame)
                .ok()?
                .get(rest)?
                .as_f64()
                .and_then(number),
            Home::Region(name) => serde_json::to_value(self.regions.get(name)?)
                .ok()?
                .get(rest)?
                .as_f64()
                .and_then(number),
            Home::Step(index) => match &self.steps[*index] {
                Entry::Step(step) => step_value(step, rest),
                Entry::Each(each) => match rest {
                    "feather" => number(each.feather),
                    "opacity" => number(each.opacity.unwrap_or(1.0)),
                    "seed" => number(f64::from(each.seed)),
                    looks => {
                        let (index, tail) = looks.strip_prefix("looks[")?.split_once("].")?;
                        each.looks
                            .get(index.parse::<usize>().ok()?)?
                            .as_ref()
                            .and_then(|look| step_value(look, tail))
                    }
                },
            },
        }
    }

    /// The `kind` of the declared region `name`.
    fn region_kind(&self, name: &str) -> String {
        self.regions
            .get(name)
            .and_then(|region| serde_json::to_value(region).ok())
            .and_then(|region| region.get("kind").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_default()
    }

    /// Every parameter of the recipe that can be set, numbers and colours, in the order of the recipe: those of each step
    /// (`pulse.exposure`, `point_light.color`; for a driven number `pulse.exposure.start`, `.end` and `.levels`), its
    /// `opacity`, an each step's `feather`, `opacity`, `seed` and its looks' parameters (`each.looks[0].exposure`), then
    /// the numbers of each region (`region.near.low`), then the frame's (`frame.angle`). A list of colours (a palette) is
    /// not listed yet. A step is addressed by its `name`, else by its effect when the recipe has one such
    /// step (an each step is `each`), else by the effect and its number among them (`grain#2`, from 1).
    ///
    /// # Errors
    /// [`Error::UnknownEffect`] for a step of an effect that is not registered.
    pub fn parameters(&self) -> Result<Vec<Parameter>, Error> {
        let mut found = Vec::new();
        for (index, (head, entry)) in self.heads().iter().zip(&self.steps).enumerate() {
            for (rest, row) in Self::step_numbers(entry)? {
                found.push(parameter(
                    format!("{head}.{rest}"),
                    &row,
                    self.value_of(&Home::Step(index), &rest),
                ));
            }
        }
        for (name, _) in self.regions.iter() {
            for (rest, row) in self.numbers_of(&Home::Region(name.to_owned()))? {
                found.push(parameter(
                    format!("region.{name}.{rest}"),
                    &row,
                    self.value_of(&Home::Region(name.to_owned()), &rest),
                ));
            }
        }
        for (rest, row) in self.numbers_of(&Home::Frame)? {
            found.push(parameter(
                format!("frame.{rest}"),
                &row,
                self.value_of(&Home::Frame, &rest),
            ));
        }
        Ok(found)
    }

    /// The parameter at `address` (see [`parameters`](Self::parameters)); any form an address may take: a step by its effect
    /// when the recipe has one such step even if it is named, or by effect and number (`grain#1.amount`, from 1).
    ///
    /// # Errors
    /// [`Error::Recipe`] naming the address when there is no such parameter, with the parameters there are and the nearest
    /// addresses; [`Error::UnknownEffect`] for a step of an effect that is not registered.
    pub fn parameter(&self, address: &str) -> Result<Parameter, Error> {
        Ok(self.locate(address)?.parameter)
    }

    /// The parameter at `address` and where it lives (see [`parameter`](Self::parameter)): for whoever sets it in a model of
    /// its own.
    ///
    /// # Errors
    /// As [`parameter`](Self::parameter).
    pub fn locate(&self, address: &str) -> Result<Located, Error> {
        let (place, rest, row) = self.reach(address)?;
        let head = match &place {
            Home::Step(index) => self.heads()[*index].clone(),
            Home::Region(name) => format!("region.{name}"),
            Home::Frame => "frame".to_owned(),
        };
        let parameter = parameter(format!("{head}.{rest}"), &row, self.value_of(&place, &rest));
        Ok(Located { place, rest, parameter })
    }

    /// [`with_values`](Self::with_values) for the one parameter at `address` (see [`parameter`](Self::parameter)).
    ///
    /// # Errors
    /// As [`with_values`](Self::with_values).
    pub fn with_value(&self, address: &str, value: impl Into<Setting>, presets: &Presets) -> Result<Self, Error> {
        self.with_values(&[(address, value.into())], presets)
    }

    /// A copy of the recipe with each parameter at an address of `values` (see [`parameter`](Self::parameter)) set to its
    /// value; the recipe itself is left as it is, and nothing is set unless every value is accepted. A number takes a
    /// [`Setting::Number`] (a whole number a whole value, `3.0`, a cyclic angle any) and a colour a [`Setting::Color`]; a
    /// number of a drive is set through its `.start`, `.end` or `.levels`. A colour is set, never unset (see
    /// [`Parameter::nullable`]), and the colours of a list (a palette) have no address yet. The result is
    /// checked (with `presets`) as any recipe is, once, with all the values in: a value out of its range is refused naming
    /// the address, the number and the range, never clamped, and a rule between numbers set together holds for the set.
    ///
    /// # Errors
    /// [`Error::Recipe`] naming the address for an address that points at nothing (listing what is there), two addresses
    /// that reach one parameter, a driven number set by itself, a `.start` of a number that is not driven, a value that is
    /// not finite or not whole where it must be, a colour given to a number or a number to a colour, or a value the recipe refuses (naming the addresses whose values it refuses,
    /// or all of them when only the set breaks a rule); [`Error::UnknownEffect`] for a step of an effect that is not
    /// registered.
    pub fn with_values(&self, values: &[(&str, Setting)], presets: &Presets) -> Result<Self, Error> {
        let json = self.written(values)?;
        let check = |json: &Value| Self::from_json(&json.to_string()).and_then(|changed| changed.checked(presets));
        check(&json).map_err(|error| {
            let alone = |(address, value): &(&str, Setting)| {
                self.written(&[(address, *value)])
                    .and_then(|json| check(&json))
                    .is_err()
            };
            let mut blamed: Vec<&str> = values
                .iter()
                .filter(|each| alone(each))
                .map(|(address, _)| *address)
                .collect();
            if blamed.is_empty() {
                blamed = values.iter().map(|(address, _)| *address).collect();
            }
            let blamed: Vec<String> = blamed.into_iter().map(q).collect();
            Error::Recipe(format!("{}: {error}", blamed.join(", ")))
        })
    }

    /// The recipe in its JSON form with each number of `values` written, not yet checked as a recipe.
    fn written(&self, values: &[(&str, Setting)]) -> Result<Value, Error> {
        let mut json = serde_json::to_value(self).map_err(|error| Error::Recipe(error.to_string()))?;
        let mut reached: Vec<(Home, String, &str)> = Vec::new();
        for &(address, value) in values {
            let (target, rest, row) = self.reach(address)?;
            let refuse = |why: String| Error::Recipe(format!("{}: {why}", q(address)));
            if let Some((_, _, first)) = reached.iter().find(|(home, name, _)| *home == target && *name == rest) {
                return Err(Error::Recipe(format!(
                    "{} and {} reach the same number: give it one value",
                    q(first),
                    q(address)
                )));
            }
            let written = match (row.kind, value) {
                (ParameterKind::Color, Setting::Color(color)) => color.to_json(),
                (ParameterKind::Color, _) => {
                    return Err(refuse(format!(
                        "{} is a colour: give '#rrggbb', '#rgb' or [r, g, b], not a number",
                        q(&rest)
                    )));
                }
                (_, Setting::Color(_)) => {
                    return Err(refuse(format!("{} is a number: give a number, not a colour", q(&rest))));
                }
                (_, Setting::Number(number)) => number_written(&rest, &row, number).map_err(refuse)?,
            };
            let slot = match &target {
                Home::Step(index) => {
                    let step = &mut json["steps"][*index];
                    step_slot(step, &rest).ok_or_else(|| refuse(format!("{} is not set in this step", q(&rest))))?
                }
                Home::Region(name) => &mut json["regions"][name][&rest],
                Home::Frame => {
                    if json.get("frame").is_none() {
                        json["frame"] = Value::Object(Map::new());
                    }
                    &mut json["frame"][&rest]
                }
            };
            *slot = written;
            reached.push((target, rest, address));
        }
        Ok(json)
    }

    /// What `address` points at: the step, region or frame, the rest of the address, and the parameter there.
    fn reach(&self, address: &str) -> Result<(Home, String, Row), Error> {
        let refuse = |why: String| Error::Recipe(format!("{}: {why}", q(address)));
        let parts: Vec<&str> = address.split('.').collect();
        if parts.len() < 2 {
            return Err(refuse(
                "not an address; write a step's parameter as `step.parameter`, a region's number as `region.name.number` and \
                 the frame's as `frame.number`"
                    .to_owned(),
            ));
        }
        let (target, rest, label) = match parts[0] {
            "frame" => (Home::Frame, parts[1..].join("."), "'frame'".to_owned()),
            "region" => {
                if parts.len() < 3 {
                    return Err(refuse(format!(
                        "a region's number is addressed `region.{}.number`",
                        parts[1]
                    )));
                }
                let name = parts[1..parts.len() - 1].join(".");
                if self.regions.get(&name).is_none() {
                    let there: Vec<&str> = self.regions.iter().map(|(name, _)| name).collect();
                    let there = if there.is_empty() {
                        "none".to_owned()
                    } else {
                        there.join(", ")
                    };
                    return Err(refuse(format!("no region {}; the recipe's regions: {there}", q(&name))));
                }
                let label = format!("region {}", q(&name));
                (Home::Region(name), parts[parts.len() - 1].to_owned(), label)
            }
            head => (
                Home::Step(self.step_index(head, address)?),
                parts[1..].join("."),
                q(head),
            ),
        };
        let numbers = self.numbers_of(&target)?;
        if let Some((_, row)) = numbers.iter().find(|(name, _)| *name == rest) {
            return Ok((target, rest, row.clone()));
        }
        if numbers.iter().any(|(name, _)| *name == format!("{rest}.start")) {
            return Err(refuse(format!(
                "{} is driven by a field: set {rest}.start, {rest}.end or {rest}.levels",
                q(&rest)
            )));
        }
        if let Some((base, part)) = rest.rsplit_once('.')
            && numbers.iter().any(|(name, _)| name == base)
            && ["start", "end", "levels"].contains(&part)
        {
            return Err(refuse(format!(
                "{} is not driven by a field, so it has no {}",
                q(base),
                q(part)
            )));
        }
        let all: Vec<String> = self
            .parameters()?
            .into_iter()
            .map(|parameter| parameter.address)
            .collect();
        let near = closest(address, all.iter().map(String::as_str));
        let hint = if near.is_empty() {
            String::new()
        } else {
            format!("; did you mean {}?", near.join(", "))
        };
        let there: Vec<&str> = numbers.iter().map(|(name, _)| name.as_str()).collect();
        Err(refuse(format!(
            "{label} has no parameter {}; its parameters: {}{hint}",
            q(&rest),
            there.join(", ")
        )))
    }

    /// The index of the step an address starting with `head` means: a step's name, its effect (`each` for an each step) when
    /// the recipe has one such step, or its effect and number (`grain#2`, from 1).
    fn step_index(&self, head: &str, address: &str) -> Result<usize, Error> {
        let refuse = |why: String| Error::Recipe(format!("{}: {why}", q(address)));
        let heads = self.heads();
        if let Some(at) = heads.iter().position(|known| known == head) {
            return Ok(at);
        }
        let (kind, number) = head.split_once('#').unwrap_or((head, ""));
        let same: Vec<usize> = (0..self.steps.len())
            .filter(|at| called(&self.steps[*at]) == kind)
            .collect();
        if !number.is_empty() && !same.is_empty() {
            return match number.parse::<usize>() {
                Ok(n) if (1..=same.len()).contains(&n) => Ok(same[n - 1]),
                _ => Err(refuse(format!(
                    "no step {}; the recipe has {} {} steps",
                    q(head),
                    same.len(),
                    q(kind)
                ))),
            };
        }
        if same.len() == 1 && number.is_empty() {
            return Ok(same[0]); // the recipe's only step of that effect, though it has a name
        }
        if same.len() > 1 {
            let tail = address.split_once('.').map_or("", |(_, tail)| tail);
            let numbered: Vec<String> = (1..=same.len()).map(|n| format!("{kind}#{n}.{tail}")).collect();
            return Err(refuse(format!(
                "the recipe has {} {} steps; write one of {}",
                same.len(),
                q(kind),
                numbered.join(", ")
            )));
        }
        let mut listed = heads;
        listed.extend(self.regions.iter().map(|(name, _)| format!("region.{name}")));
        listed.push("frame".to_owned());
        Err(refuse(format!(
            "no step, region or frame called {}; it has: {}",
            q(head),
            listed.join(", ")
        )))
    }
}

/// The number `value` of the parameter `rest` as it is written: refused unless finite, and whole where it must be.
fn number_written(rest: &str, row: &Row, value: f64) -> Result<Value, String> {
    if !value.is_finite() {
        return Err(format!("{value} is not a finite number"));
    }
    if !row.whole {
        return Ok(Value::from(value));
    }
    if value.fract() != 0.0 {
        return Err(format!("{} is a whole number, got {value}", q(rest)));
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a whole value of a range that fits an i64"
    )]
    Ok(Value::from(value as i64))
}

/// The slot in the step `step` (in the JSON form) that holds the number `rest`: a parameter, a part of a drive, `opacity`, an
/// each step's own, or a number of one of its looks (`looks[0].exposure`).
fn step_slot<'a>(step: &'a mut Value, rest: &str) -> Option<&'a mut Value> {
    if let Some(inside) = rest.strip_prefix("looks[") {
        let (index, tail) = inside.split_once("].")?;
        return step_slot(step.get_mut("looks")?.get_mut(index.parse::<usize>().ok()?)?, tail);
    }
    if step.get("each").is_some() || rest == "opacity" {
        return Some(&mut step[rest]);
    }
    if let Some((name, part)) = rest.split_once('.') {
        return Some(
            step.get_mut("params")?
                .get_mut(name)?
                .as_object_mut()?
                .entry(part)
                .or_insert(Value::Null),
        );
    }
    if step.get("params").is_none() {
        step["params"] = Value::Object(Map::new());
    }
    Some(&mut step["params"][rest])
}

/// The value now of the parameter `rest` of the step `step`: written, else the effect's default.
fn step_value(step: &Step, rest: &str) -> Option<Setting> {
    if rest == "opacity" {
        let default = crate::registry::effect(&step.effect).map_or(1.0, |found| found.opacity());
        return Some(Setting::Number(step.opacity.unwrap_or(default)));
    }
    if let Some((name, part)) = rest.split_once('.') {
        return step.params.get(name)?.get(part)?.as_f64().map(Setting::Number);
    }
    let table = crate::registry::effect(&step.effect).ok()?.params();
    let field = table.fields.iter().find(|field| field.name == rest);
    if field.is_some_and(|field| field.kind == Kind::Color) {
        let written = step.params.get(rest).cloned().or_else(|| match field?.default {
            Scalar::Text(hex) => Some(Value::from(hex)),
            _ => None,
        });
        return Color::parse(&written?).map(Setting::Color);
    }
    step.params
        .get(rest)
        .and_then(Value::as_f64)
        .or_else(|| Some(scalar(field?.default)))
        .map(Setting::Number)
}

/// What an entry of the recipe is called in an address when it has no name: its effect, or `each`.
fn called(entry: &Entry) -> &str {
    match entry {
        Entry::Step(step) => &step.effect,
        Entry::Each(_) => "each",
    }
}

fn name_of(entry: &Entry) -> Option<&str> {
    match entry {
        Entry::Step(step) => step.name.as_deref(),
        Entry::Each(each) => each.name.as_deref(),
    }
}

fn scalar(value: Scalar) -> f64 {
    match value {
        #[expect(
            clippy::cast_precision_loss,
            reason = "the bounds of a number are small whole numbers"
        )]
        Scalar::Int(whole) => whole as f64,
        Scalar::Float(real) => real,
        _ => f64::NAN,
    }
}

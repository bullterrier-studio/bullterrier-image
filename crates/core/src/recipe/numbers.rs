//! The numbers of a recipe that can be set by address: what a sweep varies.
//!
//! An address names one number: `grade.exposure` (a step by its effect, its `name`, or `grain#2` among several of one
//! effect), `pulse.exposure.end` (the end of a drive; also `.start` and `.levels`), `pulse.opacity`, `each.feather`,
//! `each.looks[0].exposure`, `region.near.low` and `frame.angle`.

use serde_json::{Map, Value};

use super::near::closest;
use super::{Entry, Recipe, Step};
use crate::params::{Kind, Scalar};
use crate::{Error, Presets};

/// One number of a recipe that can be set, and what it may be.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Number {
    /// Where it is: see [`Recipe::with_value`].
    pub address: String,
    /// A whole number: a value must be one.
    pub whole: bool,
    /// A direction in degrees: a value and the value a turn further are the same, and any value is allowed.
    pub cyclic: bool,
    /// The lowest value, inclusive (for a cyclic number, where its turn starts).
    pub min: f64,
    /// The highest value, inclusive (for a cyclic number, where its turn ends).
    pub max: f64,
}

/// A number of a step, region or the frame, by the rest of its address.
#[derive(Clone, Copy)]
pub(crate) struct Row {
    pub(crate) field: &'static str,
    pub(crate) whole: bool,
    pub(crate) cyclic: bool,
    pub(crate) min: f64,
    pub(crate) max: f64,
}

const fn real(field: &'static str, min: f64, max: f64) -> Row {
    Row {
        field,
        whole: false,
        cyclic: false,
        min,
        max,
    }
}

const fn whole(field: &'static str, min: f64, max: f64) -> Row {
    Row {
        field,
        whole: true,
        cyclic: false,
        min,
        max,
    }
}

const fn turn(field: &'static str, min: f64, max: f64) -> Row {
    Row {
        field,
        whole: false,
        cyclic: true,
        min,
        max,
    }
}

/// The most a seed is: it is 32 bits.
const SEED: f64 = 4_294_967_295.0; // constant: u32::MAX

/// The numbers of the regions of each kind that can be set (the kind is the `kind` of its file), with the range
/// [`Region::check`](super::Region) holds each to: the table is for a sweep, and a test holds the two together.
pub(crate) const REGION_ROWS: &[(&str, &[Row])] = &[
    (
        "shards",
        &[
            whole("count", 1.0, 500.0),
            real("jitter", 0.0, 1.0),
            real("focus", 0.0, 5.0),
            whole("seed", 0.0, SEED),
        ],
    ),
    ("path", &[whole("samples", 1.0, 16.0)]),
    (
        "svg",
        &[
            whole("max_shapes", 1.0, 100_000.0),
            whole("max_kilobytes", 1.0, 100_000.0),
        ],
    ),
    ("subject", &[real("context", 0.0, 1.0), real("threshold", 0.0, 1.0)]),
    (
        "band",
        &[real("low", 0.0, 1.0), real("high", 0.0, 1.0), real("feather", 0.0, 1.0)],
    ),
    ("bands", &[whole("count", 1.0, 100.0)]),
    ("linear_gradient", &[turn("angle", 0.0, 360.0)]),
    (
        "radial_gradient",
        &[real("x", 0.0, 1.0), real("y", 0.0, 1.0), real("radius", 0.01, 3.0)],
    ),
];

/// The numbers of the frame that can be set, with the range [`Frame::check`](super::Frame) holds each to.
pub(crate) const FRAME_ROWS: &[Row] = &[
    turn("angle", -180.0, 180.0),
    real("aspect", 0.1, 10.0),
    real("margin", 0.0, 1.0),
    real("pad_top", 0.0, 2.0),
    real("pad_right", 0.0, 2.0),
    real("pad_bottom", 0.0, 2.0),
    real("pad_left", 0.0, 2.0),
    real("corner_radius", 0.0, 0.5),
];

/// The most levels a drive has, and the fewest.
const LEVELS: (f64, f64) = (2.0, 16.0); // constant: the range `Recipe::checked` holds `levels` to

/// What an address points at.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Step(usize),
    Region(String),
    Frame,
}

fn number(address: String, row: &Row) -> Number {
    Number {
        address,
        whole: row.whole,
        cyclic: row.cyclic,
        min: row.min,
        max: row.max,
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
                    ("feather".to_owned(), real("feather", 0.0, 24.0)),
                    ("opacity".to_owned(), real("opacity", 0.0, 1.0)),
                    ("seed".to_owned(), whole("seed", 0.0, SEED)),
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
        if let Some(table) = crate::registry::effect(&step.effect)?.params() {
            for field in &table.fields {
                let (Kind::Float | Kind::Int, Some(min), Some(max)) = (field.kind, field.min, field.max) else {
                    continue;
                };
                let (min, max) = (scalar(min), scalar(max));
                let row = Row {
                    field: field.name,
                    whole: field.kind == Kind::Int,
                    cyclic: field.cyclic,
                    min,
                    max,
                };
                if matches!(step.params.get(field.name), Some(Value::Object(_))) {
                    found.push((format!("{}.start", field.name), row));
                    found.push((format!("{}.end", field.name), row));
                    found.push((format!("{}.levels", field.name), whole("levels", LEVELS.0, LEVELS.1)));
                } else {
                    found.push((field.name.to_owned(), row));
                }
            }
        }
        found.push(("opacity".to_owned(), real("opacity", 0.0, 1.0)));
        Ok(found)
    }

    /// The numbers of `target`, by the rest of an address.
    fn numbers_of(&self, target: &Target) -> Result<Vec<(String, Row)>, Error> {
        Ok(match target {
            Target::Step(index) => Self::step_numbers(&self.steps[*index])?,
            Target::Region(name) => {
                let kind = self.region_kind(name);
                REGION_ROWS
                    .iter()
                    .filter(|(known, _)| *known == kind)
                    .flat_map(|(_, rows)| rows.iter())
                    .map(|row| (row.field.to_owned(), *row))
                    .collect()
            }
            Target::Frame => FRAME_ROWS.iter().map(|row| (row.field.to_owned(), *row)).collect(),
        })
    }

    /// The `kind` of the declared region `name`.
    fn region_kind(&self, name: &str) -> String {
        self.regions
            .get(name)
            .and_then(|region| serde_json::to_value(region).ok())
            .and_then(|region| region.get("kind").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_default()
    }

    /// Every number of the recipe that can be set, in the order of the recipe: the numbers of each step (`pulse.exposure`;
    /// for a driven one `pulse.exposure.start`, `.end` and `.levels`), its `opacity`, an each step's `feather`, `opacity`,
    /// `seed` and its looks' numbers (`each.looks[0].exposure`), then the numbers of each region (`region.near.low`), then
    /// the frame's (`frame.angle`). A step is addressed by its `name`, else by its effect when the recipe has one such
    /// step (an each step is `each`), else by the effect and its number among them (`grain#2`, from 1).
    ///
    /// # Errors
    /// [`Error::UnknownEffect`] for a step of an effect that is not registered.
    pub fn numbers(&self) -> Result<Vec<Number>, Error> {
        let mut found = Vec::new();
        for (head, entry) in self.heads().iter().zip(&self.steps) {
            for (rest, row) in Self::step_numbers(entry)? {
                found.push(number(format!("{head}.{rest}"), &row));
            }
        }
        for (name, _) in self.regions.iter() {
            for (rest, row) in self.numbers_of(&Target::Region(name.to_owned()))? {
                found.push(number(format!("region.{name}.{rest}"), &row));
            }
        }
        for (rest, row) in self.numbers_of(&Target::Frame)? {
            found.push(number(format!("frame.{rest}"), &row));
        }
        Ok(found)
    }

    /// The number at `address` (see [`numbers`](Self::numbers)); any form an address may take: a step by its effect when the
    /// recipe has one such step even if it is named, or by effect and number (`grain#1.amount`, from 1).
    ///
    /// # Errors
    /// [`Error::Recipe`] naming the address when there is no such number, with the numbers there are and the nearest
    /// addresses; [`Error::UnknownEffect`] for a step of an effect that is not registered.
    pub fn number(&self, address: &str) -> Result<Number, Error> {
        let (target, rest, row) = self.reach(address)?;
        let head = match &target {
            Target::Step(index) => self.heads()[*index].clone(),
            Target::Region(name) => format!("region.{name}"),
            Target::Frame => "frame".to_owned(),
        };
        Ok(number(format!("{head}.{rest}"), &row))
    }

    /// A copy of the recipe with the number at `address` (see [`number`](Self::number)) set to `value`; the recipe itself is
    /// left as it is. A whole number takes a whole `value`, a cyclic angle any, and a number of a drive is set through its
    /// `.start`, `.end` or `.levels`. The result is checked (with `presets`) as any recipe is, so a value out of its range
    /// is refused naming the address, the number and the range, never clamped.
    ///
    /// # Errors
    /// [`Error::Recipe`] naming the address for an address that points at nothing (listing what is there), a driven number
    /// set by itself, a `.start` of a number that is not driven, a `value` that is not finite or not whole where it must
    /// be, or a value the recipe refuses; [`Error::UnknownEffect`] for a step of an effect that is not registered.
    pub fn with_value(&self, address: &str, value: f64, presets: &Presets) -> Result<Self, Error> {
        let (target, rest, row) = self.reach(address)?;
        let refuse = |why: String| Error::Recipe(format!("{address:?}: {why}"));
        if !value.is_finite() {
            return Err(refuse(format!("{value} is not a finite number")));
        }
        let written = if row.whole {
            if value.fract() != 0.0 {
                return Err(refuse(format!("{rest:?} is a whole number, got {value}")));
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a whole value of a range that fits an i64"
            )]
            Value::from(value as i64)
        } else {
            Value::from(value)
        };
        let mut json = serde_json::to_value(self).map_err(|error| refuse(error.to_string()))?;
        let slot = match &target {
            Target::Step(index) => {
                let step = &mut json["steps"][*index];
                step_slot(step, &rest).ok_or_else(|| refuse(format!("{rest:?} is not set in this step")))?
            }
            Target::Region(name) => &mut json["regions"][name][&rest],
            Target::Frame => {
                if json.get("frame").is_none() {
                    json["frame"] = Value::Object(Map::new());
                }
                &mut json["frame"][&rest]
            }
        };
        *slot = written;
        Self::from_json(&json.to_string())
            .and_then(|changed| changed.checked(presets))
            .map_err(|error| refuse(error.to_string()))
    }

    /// What `address` points at: the step, region or frame, the rest of the address, and the number there.
    fn reach(&self, address: &str) -> Result<(Target, String, Row), Error> {
        let refuse = |why: String| Error::Recipe(format!("{address:?}: {why}"));
        let parts: Vec<&str> = address.split('.').collect();
        if parts.len() < 2 {
            return Err(refuse(
                "not an address; write a step's number as `step.number`, a region's as `region.name.number` and the \
                 frame's as `frame.number`"
                    .to_owned(),
            ));
        }
        let (target, rest, label) = match parts[0] {
            "frame" => (Target::Frame, parts[1..].join("."), "'frame'".to_owned()),
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
                    return Err(refuse(format!("no region {name:?}; the recipe's regions: {there}")));
                }
                let label = format!("region {name:?}");
                (Target::Region(name), parts[parts.len() - 1].to_owned(), label)
            }
            head => (
                Target::Step(self.step_index(head, address)?),
                parts[1..].join("."),
                format!("{head:?}"),
            ),
        };
        let numbers = self.numbers_of(&target)?;
        if let Some((_, row)) = numbers.iter().find(|(name, _)| *name == rest) {
            return Ok((target, rest, *row));
        }
        if numbers.iter().any(|(name, _)| *name == format!("{rest}.start")) {
            return Err(refuse(format!(
                "{rest:?} is driven by a field: set {rest}.start, {rest}.end or {rest}.levels"
            )));
        }
        if let Some((base, part)) = rest.rsplit_once('.')
            && numbers.iter().any(|(name, _)| name == base)
            && ["start", "end", "levels"].contains(&part)
        {
            return Err(refuse(format!(
                "{base:?} is not driven by a field, so it has no {part:?}"
            )));
        }
        let all: Vec<String> = self.numbers()?.into_iter().map(|number| number.address).collect();
        let near = closest(address, all.iter().map(String::as_str));
        let hint = if near.is_empty() {
            String::new()
        } else {
            format!("; did you mean {}?", near.join(", "))
        };
        let there: Vec<&str> = numbers.iter().map(|(name, _)| name.as_str()).collect();
        Err(refuse(format!(
            "{label} has no number {rest:?}; its numbers: {}{hint}",
            there.join(", ")
        )))
    }

    /// The index of the step an address starting with `head` means: a step's name, its effect (`each` for an each step) when
    /// the recipe has one such step, or its effect and number (`grain#2`, from 1).
    fn step_index(&self, head: &str, address: &str) -> Result<usize, Error> {
        let refuse = |why: String| Error::Recipe(format!("{address:?}: {why}"));
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
                    "no step {head:?}; the recipe has {} {kind:?} steps",
                    same.len()
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
                "the recipe has {} {kind:?} steps; write one of {}",
                same.len(),
                numbered.join(", ")
            )));
        }
        let mut listed = heads;
        listed.extend(self.regions.iter().map(|(name, _)| format!("region.{name}")));
        listed.push("frame".to_owned());
        Err(refuse(format!(
            "no step, region or frame called {head:?}; it has: {}",
            listed.join(", ")
        )))
    }
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
        return step.get_mut("params")?.get_mut(name)?.get_mut(part);
    }
    if step.get("params").is_none() {
        step["params"] = Value::Object(Map::new());
    }
    Some(&mut step["params"][rest])
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

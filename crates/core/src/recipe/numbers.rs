//! The numbers of a recipe that can be set by address: what a sweep varies.
//!
//! An address names one number: `grade.exposure` (a step by its effect, its `name`, or `grain#2` among several of one
//! effect), `pulse.exposure.end` (the end of a drive; also `.start` and `.levels`), `pulse.opacity`, `each.feather`,
//! `each.looks[0].exposure`, `region.near.low` and `frame.angle`.

use std::borrow::Cow;

use serde_json::{Map, Value};

use super::near::closest;
use super::{Entry, Recipe, Step};
use crate::documentation::quoted as q;
use crate::params::{Kind, Scalar, Unit};
use crate::{Error, Presets};

/// One number of a recipe that can be set, and what it may be.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Number {
    /// Where it is: see [`Recipe::with_value`].
    pub address: String,
    /// Its value now, or none for an optional number the recipe leaves unset (the frame's `aspect`). A number a step does not
    /// write is at its effect's default.
    pub value: Option<f64>,
    /// Its unit: `px`, `fraction`, `degrees`, `count`...
    pub unit: String,
    /// What it does.
    pub doc: String,
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
#[derive(Clone)]
pub(crate) struct Row {
    pub(crate) field: &'static str,
    pub(crate) unit: Cow<'static, str>,
    pub(crate) doc: Cow<'static, str>,
    pub(crate) whole: bool,
    pub(crate) cyclic: bool,
    pub(crate) min: f64,
    pub(crate) max: f64,
}

const fn real(field: &'static str, unit: &'static str, min: f64, max: f64, doc: &'static str) -> Row {
    Row {
        field,
        unit: Cow::Borrowed(unit),
        doc: Cow::Borrowed(doc),
        whole: false,
        cyclic: false,
        min,
        max,
    }
}

const fn whole(field: &'static str, unit: &'static str, min: f64, max: f64, doc: &'static str) -> Row {
    Row {
        field,
        unit: Cow::Borrowed(unit),
        doc: Cow::Borrowed(doc),
        whole: true,
        cyclic: false,
        min,
        max,
    }
}

const fn turn(field: &'static str, unit: &'static str, min: f64, max: f64, doc: &'static str) -> Row {
    Row {
        field,
        unit: Cow::Borrowed(unit),
        doc: Cow::Borrowed(doc),
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
            whole(
                "count",
                "count",
                1.0,
                500.0,
                "How many shards (1 to 500): exactly this many seeds are scattered, all inside the `fit` selection when there is one; a picture too small to give each seed a pixel has fewer pieces. Raising it makes smaller shards.",
            ),
            real(
                "jitter",
                "fraction",
                0.0,
                1.0,
                "How far each seed strays from its place in a regular honeycomb, as a share of the spacing (0 to 1): 0 cuts a regular honeycomb of equal pieces, 1 lets every seed anywhere in its own place, an uneven scatter.",
            ),
            real(
                "focus",
                "ratio",
                0.0,
                5.0,
                "How much the shards crowd round `impact` (0 to 5): a seed at a share r of the way to the farthest corner moves to r to the power 1 + focus of it, so 0 leaves them spread evenly and 2.5 makes small splinters at the impact point and big pieces far from it, like cracked glass.",
            ),
            whole(
                "seed",
                "count",
                0.0,
                SEED,
                "Random seed for where the seeds lie: the same seed always cuts the same shards, another seed other ones.",
            ),
        ],
    ),
    (
        "path",
        &[whole(
            "samples",
            "count",
            1.0,
            16.0,
            "How many evenly spaced scanlines measure how much of a border pixel the path covers (1 to 16): 1 looks at the middle of the pixel row only, more give a smoother slanting border and cost more time.",
        )],
    ),
    (
        "svg",
        &[
            whole(
                "max_shapes",
                "count",
                1.0,
                100_000.0,
                "The most shapes the file may hold, counting what each `use` repeats (1 to 100000); a file with more is refused, which guards against a small file that draws a huge number of shapes.",
            ),
            whole(
                "max_kilobytes",
                "count",
                1.0,
                100_000.0,
                "The largest file to read, in kilobytes (1 to 100000); a bigger one is refused, reading no more than that.",
            ),
        ],
    ),
    (
        "subject",
        &[
            real(
                "context",
                "fraction",
                0.0,
                1.0,
                "How much of its surroundings the finder sees round a piece, as a share of the piece's own size added on each side (0 to 1); the view is clipped to the picture. Only the piece's own pixels count. Raising it shows the finder more context, so a subject cut by the piece's border is recognised; 0 shows the bare piece. Only matters with `per`.",
            ),
            real(
                "threshold",
                "fraction",
                0.0,
                1.0,
                "How sure the finder must be of a pixel for it to count as subject (0 to 1): a pixel it gives a value of this or less is dropped. Raising it keeps only what the finder is certain of and drops the faint subjects it invents in a plain piece, but also the uncertain parts of a real one; 0 keeps every pixel the finder gives. Only matters with `per`.",
            ),
        ],
    ),
    (
        "band",
        &[
            real(
                "low",
                "fraction",
                0.0,
                1.0,
                "The lowest value chosen fully (0 to 1, at most `high`): every pixel whose value lies from `low` to `high` is chosen. Raising it leaves out the lower values; 0 reaches down to the bottom of the scale.",
            ),
            real(
                "high",
                "fraction",
                0.0,
                1.0,
                "The highest value chosen fully (0 to 1): see `low`. Lowering it leaves out the higher values; 1 reaches up to the top of the scale.",
            ),
            real(
                "feather",
                "fraction",
                0.0,
                1.0,
                "How far beyond `low` and `high` the choice fades out, as a share of the scale (0 to 1): it falls linearly from 1 at the bound to 0 this far past it. Raising it softens the edge; 0 cuts it hard.",
            ),
        ],
    ),
    (
        "bands",
        &[whole(
            "count",
            "count",
            1.0,
            100.0,
            "How many equal ranges the scale is cut into (1 to 100). Band 0 is the highest, the last the lowest: by value, not in reading order. A value on a border goes to the lower band (0 to the lowest); a band no pixel lies in has no piece and the rest are renumbered, highest first. Raising it makes thinner bands.",
        )],
    ),
    (
        "components",
        &[
            real(
                "threshold",
                "fraction",
                0.0,
                1.0,
                "The value (0 to 1) from which a pixel of the selection is in a piece. Raising it keeps only what the selection is surer of; lowering it takes in more of a soft edge.",
            ),
            real(
                "min_area",
                "fraction",
                0.0,
                1.0,
                "The smallest piece kept, as a share of the frame's area (0 to 1): a smaller one joins the rest. Raising it drops specks; 0 keeps every piece, down to a single pixel.",
            ),
            whole(
                "max_pieces",
                "count",
                1.0,
                10_000.0,
                "The most pieces the selection may have after the small ones are dropped (1 to 10000); more is refused. Raising it allows a busier picture.",
            ),
        ],
    ),
    (
        "linear_gradient",
        &[turn(
            "angle",
            "degrees",
            0.0,
            360.0,
            "The direction in which the value rises, clockwise from the right (0 to 360): 0 rises to the right, 90 downward, 180 to the left, 270 upward. The value is a share of the picture's extent along that direction, so the far corners are 0 and 1 at any aspect ratio.",
        )],
    ),
    (
        "radial_gradient",
        &[
            real(
                "x",
                "fraction",
                0.0,
                1.0,
                "Where the centre is across the picture, as a share of its width (0 to 1): 0 is the left edge, 1 the right.",
            ),
            real(
                "y",
                "fraction",
                0.0,
                1.0,
                "Where the centre is down the picture, as a share of its height (0 to 1): 0 is the top edge, 1 the bottom.",
            ),
            real(
                "radius",
                "fraction",
                0.01,
                3.0,
                "The distance from the centre at which the value reaches 1, as a share of the picture's shorter side (0.01 to 3). Raising it spreads the gradient wider; 0.5, centred, reaches the midpoints of the shorter sides' edges.",
            ),
        ],
    ),
];

/// The numbers of the frame that can be set, with the range [`Frame::check`](super::Frame) holds each to.
pub(crate) const FRAME_ROWS: &[Row] = &[
    turn(
        "angle",
        "degrees",
        -180.0,
        180.0,
        "Turn the photo about its centre, clockwise when positive (-180 to 180; any other angle is the same turn inside that range, 270 is -90). A quarter turn moves every pixel exactly; any other angle resamples the picture.",
    ),
    real(
        "aspect",
        "ratio",
        0.1,
        10.0,
        "Crop to this width over height (1 is a square, 1.778 is 16:9, 0.8 is 4:5; 0.1 to 10), or leave the photo's own shape when not set. Where the crop sits is `anchor`.",
    ),
    real(
        "margin",
        "fraction",
        0.0,
        1.0,
        "Room kept around the subject by a crop on the 'subject' anchor, as a share of the longer side of the subject's box (0 to 1; 0.1 is a tenth). Raising it shows more of the surroundings. Unused by other anchors.",
    ),
    real(
        "pad_top",
        "fraction",
        0.0,
        2.0,
        "Border added above the cropped photo, in `fill`, as a share of the picture's shorter side (0 to 2). Raising it makes room above the subject.",
    ),
    real(
        "pad_right",
        "fraction",
        0.0,
        2.0,
        "Border added to the right, as `pad_top` (a share of the shorter side).",
    ),
    real(
        "pad_bottom",
        "fraction",
        0.0,
        2.0,
        "Border added below, as `pad_top` (a share of the shorter side).",
    ),
    real(
        "pad_left",
        "fraction",
        0.0,
        2.0,
        "Border added to the left, as `pad_top` (a share of the shorter side).",
    ),
    real(
        "corner_radius",
        "fraction",
        0.0,
        0.5,
        "How round the corners of `transparent` 'rounded' are, as a share of the picture's shorter side (0 to 0.5: 0 is square, 0.5 is as round as the picture allows). Unused by the other cuts.",
    ),
];

/// The opacity of a step, and the numbers of an each step.
const OPACITY: Row = real(
    "opacity",
    "fraction",
    0.0,
    1.0,
    "How much of the step shows: 0 none, 1 fully; when none is given, the effect's own default (`registry.opacity`, 1 for most effects, and for an `each` step).",
);
const EACH_OPACITY: Row = real(
    "opacity",
    "fraction",
    0.0,
    1.0,
    "How much of the step shows: 0 none, 1 fully; when none is given, the effect's own default (`registry.opacity`, 1 for most effects, and for an `each` step).",
);
const FEATHER: Row = real(
    "feather",
    "px",
    0.0,
    24.0,
    "How softly a piece's look fades into its neighbours' across a border, in px (0 to 24): the standard deviation of a Gaussian. 0 gives hard borders; about 0.5 only smooths the stair-steps of the pixels; raising it blends the looks over a wider band.",
);
const EACH_SEED: Row = whole(
    "seed",
    "count",
    0.0,
    SEED,
    "Random seed of assign='random': the same seed always gives the same looks to the same pieces.",
);
const LEVELS: Row = whole(
    "levels",
    "count",
    2.0,
    16.0,
    "How many evenly spaced values between `start` and `end` the effect is rendered at (2 to 16); each pixel mixes the two nearest. More is smoother and costs more renders; 2 mixes the effect at `start` and at `end`.",
); // constant: the range `Recipe::checked` holds `levels` to

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

/// A number of a recipe found by its address.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Located {
    /// Where the number lives.
    pub place: Home,
    /// Where in it: the rest of the address (`exposure`, `exposure.end`, `looks[0].exposure`, `low`, `angle`).
    pub rest: String,
    /// The number.
    pub number: Number,
}

fn number(address: String, row: &Row, value: Option<f64>) -> Number {
    Number {
        address,
        value,
        unit: row.unit.to_string(),
        doc: row.doc.to_string(),
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
            let (Kind::Float | Kind::Int, Some(min), Some(max)) = (field.kind, field.min, field.max) else {
                continue;
            };
            let (min, max) = (scalar(min), scalar(max));
            let row = Row {
                field: field.name,
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

    /// The value now of the number `rest` of `target`.
    fn value_of(&self, target: &Home, rest: &str) -> Option<f64> {
        match target {
            Home::Frame => serde_json::to_value(&self.frame).ok()?.get(rest)?.as_f64(),
            Home::Region(name) => serde_json::to_value(self.regions.get(name)?).ok()?.get(rest)?.as_f64(),
            Home::Step(index) => match &self.steps[*index] {
                Entry::Step(step) => step_value(step, rest),
                Entry::Each(each) => match rest {
                    "feather" => Some(each.feather),
                    "opacity" => Some(each.opacity.unwrap_or(1.0)),
                    "seed" => Some(f64::from(each.seed)),
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
        for (index, (head, entry)) in self.heads().iter().zip(&self.steps).enumerate() {
            for (rest, row) in Self::step_numbers(entry)? {
                found.push(number(
                    format!("{head}.{rest}"),
                    &row,
                    self.value_of(&Home::Step(index), &rest),
                ));
            }
        }
        for (name, _) in self.regions.iter() {
            for (rest, row) in self.numbers_of(&Home::Region(name.to_owned()))? {
                found.push(number(
                    format!("region.{name}.{rest}"),
                    &row,
                    self.value_of(&Home::Region(name.to_owned()), &rest),
                ));
            }
        }
        for (rest, row) in self.numbers_of(&Home::Frame)? {
            found.push(number(
                format!("frame.{rest}"),
                &row,
                self.value_of(&Home::Frame, &rest),
            ));
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
        Ok(self.locate(address)?.number)
    }

    /// The number at `address` and where it lives (see [`number`](Self::number)): for whoever sets it in a model of its own.
    ///
    /// # Errors
    /// As [`number`](Self::number).
    pub fn locate(&self, address: &str) -> Result<Located, Error> {
        let (place, rest, row) = self.reach(address)?;
        let head = match &place {
            Home::Step(index) => self.heads()[*index].clone(),
            Home::Region(name) => format!("region.{name}"),
            Home::Frame => "frame".to_owned(),
        };
        let number = number(format!("{head}.{rest}"), &row, self.value_of(&place, &rest));
        Ok(Located { place, rest, number })
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
        let refuse = |why: String| Error::Recipe(format!("{}: {why}", q(address)));
        if !value.is_finite() {
            return Err(refuse(format!("{value} is not a finite number")));
        }
        let written = if row.whole {
            if value.fract() != 0.0 {
                return Err(refuse(format!("{} is a whole number, got {value}", q(&rest))));
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
        Self::from_json(&json.to_string())
            .and_then(|changed| changed.checked(presets))
            .map_err(|error| refuse(error.to_string()))
    }

    /// What `address` points at: the step, region or frame, the rest of the address, and the number there.
    fn reach(&self, address: &str) -> Result<(Home, String, Row), Error> {
        let refuse = |why: String| Error::Recipe(format!("{}: {why}", q(address)));
        let parts: Vec<&str> = address.split('.').collect();
        if parts.len() < 2 {
            return Err(refuse(
                "not an address; write a step's number as `step.number`, a region's as `region.name.number` and the \
                 frame's as `frame.number`"
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
        let all: Vec<String> = self.numbers()?.into_iter().map(|number| number.address).collect();
        let near = closest(address, all.iter().map(String::as_str));
        let hint = if near.is_empty() {
            String::new()
        } else {
            format!("; did you mean {}?", near.join(", "))
        };
        let there: Vec<&str> = numbers.iter().map(|(name, _)| name.as_str()).collect();
        Err(refuse(format!(
            "{label} has no number {}; its numbers: {}{hint}",
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

/// The value now of the number `rest` of the step `step`: written, else the effect's default.
fn step_value(step: &Step, rest: &str) -> Option<f64> {
    if rest == "opacity" {
        let default = crate::registry::effect(&step.effect).map_or(1.0, |found| found.opacity());
        return Some(step.opacity.unwrap_or(default));
    }
    if let Some((name, part)) = rest.split_once('.') {
        return step.params.get(name)?.get(part)?.as_f64();
    }
    step.params.get(rest).and_then(Value::as_f64).or_else(|| {
        let table = crate::registry::effect(&step.effect).ok()?.params();
        let field = table.fields.iter().find(|field| field.name == rest)?;
        Some(scalar(field.default))
    })
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

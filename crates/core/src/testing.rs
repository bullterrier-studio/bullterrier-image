//! What the unit tests of the core share: a registry that holds a `probe` effect (one parameter of every kind, see
//! `params::probe`) and a `seams` effect that takes a partition and describes no parameters of its own, beside the
//! core's, and the foreign effects that tests register: [`Paint`], [`Recorder`], [`Counting`], [`Shrinks`] and [`Replaces`],
//! and [`Finds`], a finder that counts its calls, and [`regions`], the declared regions a JSON table writes.
//!
//! Integration tests (`crates/core/tests/`) cannot reach this module: they use the core's own effects, or a double
//! written against the public [`Effect`] trait.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Deserialize;
use serde_json::Value;

use crate::effect::{Category, Effect, FieldParam, Input, Meta, Native};
use crate::params::probe::Probe;
use crate::params::{Params, Table};
use crate::registry::Registry;
use crate::{Error, Finder, Image, Inputs, Layer, Picture, Regions};

static PROBE: Meta = Meta {
    category: Category::Utilities,
    opacity: 0.5,
    inputs: &[],
};

/// An effect that draws along the borders of a partition and leaves its parameters to its owner.
struct Seams;

impl Effect for Seams {
    fn name(&self) -> &'static str {
        "seams"
    }

    fn category(&self) -> Category {
        Category::Stylised
    }

    fn inputs(&self) -> &[Input] {
        &[Input::Partition]
    }

    fn apply(&self, _picture: &Picture<'_>, _params: Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        Err(Error::foreign("seams", "the tests never run it"))
    }
}

/// The regions the JSON table `declared` writes (name to a table with its `kind`), in its order. A table that is no
/// regions is a mistake of the test, so it is an empty declaration, which the test then notices.
pub(crate) fn regions(declared: Value) -> Regions {
    serde_json::from_value(declared).unwrap_or_default()
}

/// A registry of the core's effects with `probe` and `seams` added.
pub(crate) fn registry() -> Result<Registry, Error> {
    let registry = Registry::new();
    registry.register(Arc::new(Native::new(
        "probe",
        &PROBE,
        |picture: &Picture<'_>, _: &Probe, _: &Inputs<'_>| picture.map(|pixel| pixel),
    )))?;
    registry.register(Arc::new(Seams))?;
    Ok(registry)
}

/// A foreign effect that paints the picture with the `level` of its parameters; its steps default to an opacity of
/// 0.25 and it declares the size ratio.
pub(crate) struct Paint {
    pub(crate) name: &'static str,
}

impl Effect for Paint {
    fn name(&self) -> &str {
        self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn opacity(&self) -> f64 {
        0.25
    }

    fn inputs(&self) -> &[Input] {
        &[Input::SizeRatio]
    }

    fn apply(&self, picture: &Picture<'_>, params: Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        let level = params["level"]
            .as_f64()
            .ok_or_else(|| Error::foreign(self.name, "no level"))?;
        #[expect(clippy::cast_possible_truncation, reason = "a test level is a small number")]
        Ok(picture.map(|_| [level as f32; 3]))
    }
}

/// What a [`Recorder`] was handed by one `apply`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Received {
    pub(crate) picture: Vec<f32>,
    pub(crate) params: Value,
    pub(crate) size_ratio: f64,
    pub(crate) partition: Option<Vec<i32>>,
    pub(crate) fields: BTreeMap<String, Vec<f32>>,
}

/// A foreign effect that declares the inputs it is made with, logs the parameters and inputs it receives and returns
/// the picture as it is: the oracle for "an undeclared input is absent" and for the parameters a step resolves to. It
/// describes no parameters unless given a table.
pub(crate) struct Recorder {
    name: &'static str,
    inputs: &'static [Input],
    fields: Vec<FieldParam>,
    table: Option<Table>,
    log: Mutex<Vec<Received>>,
}

impl Recorder {
    /// A recorder called `name` that declares `inputs` and one field parameter for each of `fields`, whose default
    /// field is the parameter's name upcased.
    pub(crate) fn new(name: &'static str, inputs: &'static [Input], fields: &[&str]) -> Self {
        Self {
            name,
            inputs,
            fields: fields
                .iter()
                .map(|parameter| FieldParam::new(*parameter, parameter.to_uppercase()))
                .collect(),
            table: None,
            log: Mutex::new(Vec::new()),
        }
    }

    /// This recorder describing its parameters by `table`, as a core effect does: the core then checks and scales a
    /// step's values before it sees them.
    pub(crate) fn with_table(mut self, table: Table) -> Self {
        self.table = Some(table);
        self
    }

    /// What each `apply` so far received, in order.
    pub(crate) fn received(&self) -> Vec<Received> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Forget what was received so far.
    pub(crate) fn clear(&self) {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}

impl Effect for Recorder {
    fn name(&self) -> &str {
        self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        self.inputs
    }

    fn fields(&self) -> &[FieldParam] {
        &self.fields
    }

    fn params(&self) -> Option<Table> {
        self.table.clone()
    }

    fn apply(&self, picture: &Picture<'_>, params: Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        let fields = inputs
            .field_parameters()
            .filter_map(|parameter| Some((parameter.to_owned(), inputs.field(parameter)?.values().to_vec())))
            .collect();
        self.log.lock().unwrap_or_else(PoisonError::into_inner).push(Received {
            picture: picture.values().to_vec(),
            params,
            size_ratio: inputs.size_ratio(),
            partition: inputs.partition().map(|labels| labels.values().to_vec()),
            fields,
        });
        Ok(picture.map(|pixel| pixel))
    }
}

/// A foreign effect that declares a partition, returns the picture as it is and counts its calls: the oracle for "the
/// effect was never run".
pub(crate) struct Counting {
    calls: AtomicUsize,
}

impl Counting {
    pub(crate) const fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }

    /// How many times `apply` ran.
    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Effect for Counting {
    fn name(&self) -> &'static str {
        "counting"
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        &[Input::Partition]
    }

    fn apply(&self, picture: &Picture<'_>, _params: Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(picture.map(|pixel| pixel))
    }
}

/// A foreign effect that breaks the contract: it returns a single pixel, whatever the size of the picture.
pub(crate) struct Shrinks;

impl Effect for Shrinks {
    fn name(&self) -> &'static str {
        "shrinks"
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn apply(&self, _picture: &Picture<'_>, _params: Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        Image::new(1, 1, &[0.0; 3])
    }
}

/// A foreign effect that ignores the picture and returns `values` (pixel after pixel, row after row) instead: the
/// source a blend mode is laid over the photo with.
pub(crate) struct Replaces {
    pub(crate) name: &'static str,
    pub(crate) height: usize,
    pub(crate) width: usize,
    pub(crate) values: Vec<f32>,
}

impl Effect for Replaces {
    fn name(&self) -> &str {
        self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn apply(&self, _picture: &Picture<'_>, _params: Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        Image::new(self.height, self.width, &self.values)
    }
}

/// What a [`Finds`] answers: a map, or an error, for the picture it is asked about.
type Answer = Box<dyn Fn(&Picture<'_>) -> Result<Layer, Error> + Send + Sync>;

/// A finder that answers with what `answer` makes of the picture and counts its calls: the oracle for "the map was found
/// once" and "the finder was never asked".
pub(crate) struct Finds {
    answer: Answer,
    calls: AtomicUsize,
}

impl Finds {
    /// A finder whose answer to every picture is `answer` of it.
    pub(crate) fn new(answer: impl Fn(&Picture<'_>) -> Result<Layer, Error> + Send + Sync + 'static) -> Self {
        Self {
            answer: Box::new(answer),
            calls: AtomicUsize::new(0),
        }
    }

    /// A finder that answers a map of `level` everywhere, of the size of the picture it is asked about.
    pub(crate) fn flat(level: f32) -> Self {
        Self::new(move |picture| Layer::new(picture.height(), picture.width(), vec![level; picture.pixels().len()]))
    }

    /// How many times the finder was asked.
    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Finder for Finds {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        (self.answer)(picture)
    }
}

/// The parameters of an effect that measures: one number of every unit, the whole-number ones as `i32`.
#[derive(Debug, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub(crate) struct Measured {
    /// A length in px.
    #[param(default = 10.0, unit = px, min = 2, max = 100, drivable)]
    pub(crate) length: f32,
    /// A whole number of px.
    #[param(default = 10, unit = px, min = 3, max = 100)]
    pub(crate) cell: i32,
    /// Marks per 1000 design px squared.
    #[param(default = 8.0, unit = density, min = 2, max = 1000)]
    pub(crate) marks: f32,
    /// A count of marks.
    #[param(default = 5, unit = count, min = 0, max = 100)]
    pub(crate) copies: i32,
    /// A share of something.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub(crate) share: f32,
    /// A direction.
    #[param(default = 90.0, unit = degrees, min = 0, max = 360)]
    pub(crate) turn: f32,
    /// A ratio.
    #[param(default = 2.0, unit = ratio, min = 0, max = 5)]
    pub(crate) gain: f32,
    /// Photographic stops.
    #[param(default = 1.0, unit = stops, min = -2, max = 2)]
    pub(crate) stops: f32,
    /// Turns of a cycle.
    #[param(default = 1.5, unit = cycles, min = 0, max = 10)]
    pub(crate) laps: f32,
    /// A signed offset in px, which may be negative.
    #[param(default = -10.0, unit = px, min = -50, max = 50)]
    pub(crate) shift: f32,
}

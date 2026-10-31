//! What the unit tests of the core share: a registry that holds a `probe` effect (one parameter of every kind, see
//! `params::probe`) and a `seams` effect that takes a partition and describes no parameters of its own, beside the
//! core's, and the foreign effects that tests register: [`Paint`], [`Recorder`] and [`Counting`].
//!
//! Integration tests (`crates/core/tests/`) cannot reach this module: they use the core's own effects, or a double
//! written against the public [`Effect`] trait.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;

use crate::effect::{Category, Effect, FieldParam, Input, Meta, Native};
use crate::params::probe::Probe;
use crate::registry::Registry;
use crate::{Error, Image, Inputs, Picture};

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
        Err(Error::Foreign {
            effect: "seams".to_owned(),
            message: "the tests never run it".to_owned(),
        })
    }
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
        let level = params["level"].as_f64().ok_or_else(|| Error::Foreign {
            effect: self.name.to_owned(),
            message: "no level".to_owned(),
        })?;
        #[expect(clippy::cast_possible_truncation, reason = "a test level is a small number")]
        Ok(picture.map(|_| [level as f32; 3]))
    }
}

/// What a [`Recorder`] was handed by one `apply`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Received {
    pub(crate) size_ratio: f64,
    pub(crate) partition: Option<Vec<i32>>,
    pub(crate) fields: BTreeMap<String, Vec<f32>>,
}

/// A foreign effect that declares the inputs it is made with, logs what it receives of them and returns the picture
/// as it is: the oracle for "an undeclared input is absent".
pub(crate) struct Recorder {
    name: &'static str,
    inputs: &'static [Input],
    fields: Vec<FieldParam>,
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
            log: Mutex::new(Vec::new()),
        }
    }

    /// What each `apply` so far received, in order.
    pub(crate) fn received(&self) -> Vec<Received> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).clone()
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

    fn apply(&self, picture: &Picture<'_>, _params: Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        let fields = inputs
            .field_parameters()
            .filter_map(|parameter| Some((parameter.to_owned(), inputs.field(parameter)?.values().to_vec())))
            .collect();
        self.log.lock().unwrap_or_else(PoisonError::into_inner).push(Received {
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

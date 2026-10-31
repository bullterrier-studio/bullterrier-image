//! What the unit tests of recipes share: a registry that holds a `probe` effect (one parameter of every kind, see
//! `params::probe`) and a `seams` effect that takes a partition and describes no parameters of its own, beside the
//! core's.

use std::sync::Arc;

use crate::effect::{Category, Effect, Input, Meta, Native};
use crate::params::probe::Probe;
use crate::registry::Registry;
use crate::{Error, Image, Picture};

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

    fn apply(&self, _picture: &Picture<'_>, _params: serde_json::Value) -> Result<Image, Error> {
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
        |picture: &Picture<'_>, _: &Probe| picture.map(|pixel| pixel),
    )))?;
    registry.register(Arc::new(Seams))?;
    Ok(registry)
}

//! The pieces of a recipe that are not steps, in their normal form: a region of one kind, the frame and a drive.
//!
//! Each function reads the fields it is given as the core reads a recipe file (strictly: a field it does not have or a
//! value of another kind is refused), runs the rules of the piece that need nothing outside it, and returns every field
//! with its value, none ones included. The Python classes hold what comes back. Names a piece refers to (`of`, `fit`,
//! `per`, a drive's `by`) are not looked up: those are the recipe's.

use bullterrier_image::{Drive, Error, Frame, Region};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Map, Value};

use super::{core_error, from_json, to_json};

/// The fields of `fields` as a JSON object, or the refusal.
fn object(fields: &Bound<'_, PyDict>) -> PyResult<Map<String, Value>> {
    match to_json(fields.as_any())? {
        Value::Object(map) => Ok(map),
        _ => Err(core_error(Error::Recipe("fields are a table".to_owned()))),
    }
}

/// A region of the kind `kind` (`grid`, `shards`, `cuts`, `path`, `svg`, `pick`, `subject`, `band`, `bands`, `components`,
/// `combine`, `linear_gradient`, `radial_gradient` or `given`) with the fields `fields` (a dict; each has a default): the dict of
/// every field of that kind with its value, a count of shares as the list, a nested `edge` as its dict.
///
/// `CoreError` (a `ValueError`) naming the region and the field, for a field the kind lacks, a value of another kind
/// or one outside its range.
#[pyfunction]
pub(crate) fn region<'py>(py: Python<'py>, kind: &str, fields: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
    let mut table = object(fields)?;
    table.insert("kind".to_owned(), Value::from(kind));
    let region: Region = serde_json::from_value(Value::Object(table))
        .map_err(|error| core_error(Error::Recipe(format!("region '{kind}': {error}"))))?;
    region.check_shape(kind).map_err(core_error)?;
    let mut normal = serde_json::to_value(&region).map_err(|error| core_error(Error::Recipe(error.to_string())))?;
    if let Value::Object(map) = &mut normal {
        map.shift_remove("kind");
    }
    from_json(py, &normal)
}

/// The frame with the fields `fields` (a dict; each has a default): the dict of every field with its value.
///
/// `CoreError` naming `frame` and the field, for a field it lacks, a value of another kind or one outside its range.
#[pyfunction]
pub(crate) fn frame<'py>(py: Python<'py>, fields: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
    let frame = Frame::from_json(Value::Object(object(fields)?)).map_err(core_error)?;
    let normal = serde_json::to_value(&frame).map_err(|error| core_error(Error::Recipe(error.to_string())))?;
    from_json(py, &normal)
}

/// The drive with the keys `fields` (`by`, `start`, `end`, optionally `levels`): the dict of the four with their values.
///
/// `CoreError` naming the key, for one it lacks, a missing one, a value of another kind or `levels` outside 2 to 16.
#[pyfunction]
pub(crate) fn drive<'py>(py: Python<'py>, fields: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
    let drive = Drive::from_json(&Value::Object(object(fields)?)).map_err(core_error)?;
    from_json(py, &Value::from(drive))
}

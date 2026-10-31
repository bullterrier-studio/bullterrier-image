//! `bullterrier_image._native._core`: runs the effects of the Rust core on numpy pictures. Generic: it names no effect, so
//! porting one never changes it.

use bullterrier_image::Picture;
use numpy::{PyArray1, PyArray3, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::IntoPyObjectExt as _;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde_json::Value;

/// The effects of bullterrier-image's Rust core, run on numpy pictures.
#[pymodule]
mod _core {
    #[pymodule_export]
    use super::{apply, effects, params};
}

/// Run the effect called `effect` on `rgb` (float32, C-contiguous, (height, width, 3)) with `params` (exactly the
/// effect's parameters, validated and scaled); a new array of `rgb`'s shape. The effect runs without the GIL.
///
/// `ValueError` if the effect, its parameters or the array are not what the core expects: a programming error, since
/// the package validates a user's values before they get here.
///
/// The caller must not modify `rgb` while this runs. The effect reads the array's memory in place, with the GIL
/// released, and numpy's borrow flags guard only against other Rust borrows: a Python thread that writes into `rgb`
/// meanwhile makes the core read memory that is changing, which is undefined behaviour. The input is deliberately not
/// copied (a 1080p float32 frame is about 25 MB, as much work as a whole light effect); `numpy` works the same way on
/// arrays it is given. A thread that renders must own its frame, or share it read-only.
#[pyfunction]
fn apply<'py>(
    py: Python<'py>,
    effect: &str,
    rgb: &Bound<'py, PyAny>,
    params: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyArray3<f32>>> {
    let array = rgb.cast::<PyArray3<f32>>().map_err(|_| picture_error(rgb))?;
    let &[height, width, 3] = array.shape() else {
        return Err(picture_error(rgb));
    };
    if !array.is_c_contiguous() {
        return Err(picture_error(rgb));
    }
    let readonly = array.try_readonly().map_err(value_error)?;
    let picture = Picture::new(height, width, readonly.as_slice().map_err(value_error)?).map_err(value_error)?;
    let params = to_json(params.as_any())?;
    let image = py
        .detach(|| bullterrier_image::apply(effect, &picture, params))
        .map_err(value_error)?;
    let shape = [image.height(), image.width(), 3];
    PyArray1::from_vec(py, image.into_values()).reshape(shape)
}

/// The names of the effects the core implements, sorted.
#[pyfunction]
fn effects() -> Vec<String> {
    bullterrier_image::effects()
}

/// The parameters of the effect called `effect`, as its Rust struct declares them: a dict `{"doc": str, "fields":
/// [...]}`, each field a dict with `name`, `kind` (`"float"`, `"int"`, `"bool"`, `"choice"` or `"color"`), `doc`,
/// `default`, `unit`, `min`, `max`, `cyclic`, `choices`, `drivable` and `advanced`. A default or bound is the literal
/// the declaration wrote, a whole number an `int` and a decimal a `float`; a colour's default is its `'#rrggbb'` text and
/// a choice's its name.
///
/// `ValueError` if the core implements no effect of that name.
#[pyfunction]
fn params<'py>(py: Python<'py>, effect: &str) -> PyResult<Bound<'py, PyAny>> {
    let table = bullterrier_image::effect(effect)
        .map_err(value_error)?
        .params()
        .ok_or_else(|| PyValueError::new_err(format!("{effect:?} describes its own parameters")))?;
    from_json(py, &serde_json::to_value(table).map_err(value_error)?)
}

/// A `ValueError` carrying `error`'s message.
#[expect(clippy::needless_pass_by_value, reason = "map_err passes the error by value")]
fn value_error(error: impl ToString) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// The `ValueError` for an `rgb` that is not a C-contiguous float32 (height, width, 3) array.
fn picture_error(rgb: &Bound<'_, PyAny>) -> PyErr {
    let found = match rgb.cast::<numpy::PyUntypedArray>() {
        Ok(array) => format!("an array of shape {:?}, dtype {}", array.shape(), array.dtype()),
        Err(_) => format!("{}", rgb.get_type()),
    };
    PyValueError::new_err(format!(
        "rgb must be a C-contiguous float32 array of shape (h, w, 3), got {found}"
    ))
}

/// `value` (parameters as Pydantic's `model_dump` gives them) as JSON, for the core to read with serde.
fn to_json(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if value.is_none() {
        Ok(Value::Null)
    } else if let Ok(flag) = value.cast::<PyBool>() {
        Ok(Value::Bool(flag.is_true()))
    } else if let Ok(whole) = value.cast::<PyInt>() {
        let number = whole
            .extract::<i64>()
            .map_err(|_| PyValueError::new_err(format!("a parameter is out of range for 64 bits: {whole}")))?;
        Ok(Value::from(number))
    } else if let Ok(real) = value.cast::<PyFloat>() {
        serde_json::Number::from_f64(real.value())
            .map(Value::Number)
            .ok_or_else(|| PyValueError::new_err(format!("a parameter is not finite: {}", real.value())))
    } else if let Ok(text) = value.cast::<PyString>() {
        Ok(Value::String(text.to_str()?.to_owned()))
    } else if let Ok(list) = value.cast::<PyList>() {
        list.iter()
            .map(|item| to_json(&item))
            .collect::<PyResult<_>>()
            .map(Value::Array)
    } else if let Ok(tuple) = value.cast::<PyTuple>() {
        tuple
            .iter()
            .map(|item| to_json(&item))
            .collect::<PyResult<_>>()
            .map(Value::Array)
    } else if let Ok(dict) = value.cast::<PyDict>() {
        dict.iter()
            .map(|(key, item)| Ok((key.extract::<String>()?, to_json(&item)?)))
            .collect::<PyResult<_>>()
            .map(Value::Object)
    } else {
        Err(PyValueError::new_err(format!(
            "a parameter of type {} has no JSON form",
            value.get_type()
        )))
    }
}

/// `value` as the Python object of its kind: the inverse of `to_json`.
fn from_json<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Value::Null => Ok(py.None().into_bound(py)),
        Value::Bool(flag) => flag.into_bound_py_any(py),
        // an integer literal stays an `int`, a decimal one a `float`
        Value::Number(number) => match number.as_i64() {
            Some(whole) => whole.into_bound_py_any(py),
            None => number
                .as_f64()
                .ok_or_else(|| PyValueError::new_err(format!("a number has no Python form: {number}")))?
                .into_bound_py_any(py),
        },
        Value::String(text) => text.into_bound_py_any(py),
        Value::Array(items) => {
            let items = items
                .iter()
                .map(|item| from_json(py, item))
                .collect::<PyResult<Vec<_>>>()?;
            Ok(PyList::new(py, items)?.into_any())
        }
        Value::Object(entries) => {
            let dict = PyDict::new(py);
            for (key, item) in entries {
                dict.set_item(key, from_json(py, item)?)?;
            }
            Ok(dict.into_any())
        }
    }
}

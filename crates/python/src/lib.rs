//! `bullterrier_image._native._core`: runs the effects of the Rust core on numpy pictures. Generic: it names no effect, so
//! porting one never changes it.

use std::sync::Arc;

use bullterrier_image::{BlendMode, Category, Effect, Error, Image, Input, Picture, Recipe, STEP_KEYS};
use numpy::{PyArray1, PyArray3, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::IntoPyObjectExt as _;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde_json::{Map, Value, json};

/// The effects of bullterrier-image's Rust core, run on numpy pictures.
#[pymodule]
mod _core {
    #[pymodule_export]
    use super::{apply, check, effect, effects, params, read_recipe, register, unregister, vocabulary};
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
/// [...]}`, each field a dict with `name`, `kind` (`"float"`, `"int"`, `"bool"`, `"choice"` or `"color"`), `meaning`,
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

/// What the effect called `effect` says about itself: a dict `{"name": str, "category": str, "opacity": float,
/// "inputs": [str]}`.
///
/// `ValueError` if there is no effect of that name.
#[pyfunction]
fn effect<'py>(py: Python<'py>, effect: &str) -> PyResult<Bound<'py, PyAny>> {
    let effect = bullterrier_image::effect(effect).map_err(value_error)?;
    let inputs: Vec<_> = effect.inputs().iter().map(|input| input.name()).collect();
    from_json(
        py,
        &json!({
            "name": effect.name(),
            "category": effect.category().name(),
            "opacity": effect.opacity(),
            "inputs": inputs,
        }),
    )
}

/// The names recipes and effects are described with, in the order the catalogue lists them: a dict `{"categories":
/// [str], "inputs": [str], "blend_modes": [str], "step_keys": [str]}`.
#[pyfunction]
fn vocabulary(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let categories: Vec<_> = Category::ALL.iter().map(|category| category.name()).collect();
    let inputs: Vec<_> = Input::ALL.iter().map(|input| input.name()).collect();
    let blend_modes: Vec<_> = BlendMode::ALL.iter().map(|mode| mode.name()).collect();
    from_json(
        py,
        &json!({"categories": categories, "inputs": inputs, "blend_modes": blend_modes, "step_keys": STEP_KEYS}),
    )
}

/// The complete parameters the effect called `effect` takes for the values `params`, as its Rust struct declares them:
/// a dict with every parameter, defaults filled in, a decimal as a `float` (also when given as a whole number), a
/// whole number as an `int` and a colour as a list of three floats 0..1 (given as `'#rrggbb'`, `'#rgb'` or three
/// numbers). A number outside its inclusive range is refused, except an angle declared cyclic, which any real wraps into.
///
/// `ValueError` with the sentence naming the effect, the parameter, the rule and the value, for the first rule `params`
/// break; also if the core implements no such effect or the effect describes its own parameters (a Python function
/// registered with `register`).
#[pyfunction]
fn check<'py>(py: Python<'py>, effect: &str, params: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
    let table = bullterrier_image::effect(effect)
        .map_err(value_error)?
        .params()
        .ok_or_else(|| PyValueError::new_err(format!("{effect:?} describes its own parameters")))?;
    let given = params
        .iter()
        .map(|(key, value)| Ok((key.extract::<String>()?, to_json(&value)?)))
        .collect::<PyResult<Map<_, _>>>()?;
    let checked = table.check(effect, &given).map_err(value_error)?;
    from_json(py, &Value::Object(checked))
}

/// The recipe that the text `text` writes, in `syntax` (`"toml"`, a recipe file, or `"json"`), as a dict: `{"steps":
/// [...], "name", "description", "size", "frame", "regions"}` with a key left out when the text left it unset; a
/// step is `{"effect", "params", "preset", "partition", "mask", "opacity", "blend_mode", "name"}` and an each step
/// `{"each", "looks", "assign", "seed", "feather", "mask", "opacity", "blend_mode", "name"}` likewise (a skipped look
/// is `None`). Only the shape and the rules of the file's format version are read: whether the effects exist or
/// the values are in range is for the recipe's check, and an opacity left out stays unset.
///
/// `ValueError` for text that is not that format or breaks one of those rules, naming the step and the key.
#[pyfunction]
fn read_recipe<'py>(py: Python<'py>, text: &str, syntax: &str) -> PyResult<Bound<'py, PyAny>> {
    let recipe = match syntax {
        "toml" => Recipe::from_toml(text),
        "json" => Recipe::from_json(text),
        other => {
            return Err(PyValueError::new_err(format!(
                "a recipe is written in 'toml' or 'json', not {other:?}"
            )));
        }
    }
    .map_err(value_error)?;
    from_json(py, &serde_json::to_value(&recipe).map_err(value_error)?)
}

/// Add the Python function `function(rgb, params) -> rgb` to the core's effects as `name`, in `category` (a name from
/// `vocabulary()`), with `opacity` as the default opacity of its steps. `rgb` is a float32 `(height, width, 3)` array
/// and `params` a dict, as `apply` was given; the function returns a float32 array of the same shape. Then `apply`,
/// `effects` and `effect` know it like the core's own. The name is not checked for form, only for being free.
///
/// `ValueError` if the name is taken (by a core effect as well), the category is unknown, `opacity` is not a finite
/// number from 0 to 1, or `function` is not callable. A function that raises, or returns anything but a float32 array
/// of the picture's shape, makes `apply` raise `ValueError` naming the effect.
#[pyfunction]
fn register(name: &str, category: &str, opacity: f64, function: &Bound<'_, PyAny>) -> PyResult<()> {
    let category = Category::ALL
        .iter()
        .copied()
        .find(|known| known.name() == category)
        .ok_or_else(|| PyValueError::new_err(format!("{category:?} is not a category of effects")))?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(PyValueError::new_err(format!(
            "opacity must be a number from 0 to 1, got {opacity}"
        )));
    }
    if !function.is_callable() {
        return Err(PyValueError::new_err(format!(
            "function must be callable, got {}",
            function.get_type()
        )));
    }
    let effect = PythonEffect {
        name: name.to_owned(),
        category,
        opacity,
        function: function.clone().unbind(),
    };
    bullterrier_image::register(Arc::new(effect)).map_err(value_error)
}

/// Remove the effect `register` added as `name`. `ValueError` if there is none: the core's own effects stay.
#[pyfunction]
fn unregister(name: &str) -> PyResult<()> {
    if bullterrier_image::unregister(name) {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!("no effect was registered as {name:?}")))
    }
}

/// An effect that is a Python function, run by the core like its own.
struct PythonEffect {
    name: String,
    category: Category,
    opacity: f64,
    function: Py<PyAny>,
}

impl PythonEffect {
    /// Call the function with `picture` and `params`; its result as an image of the picture's size, or what is wrong.
    fn call(&self, py: Python<'_>, picture: &Picture<'_>, params: &Value) -> Result<Image, String> {
        let shape = [picture.height(), picture.width(), 3];
        let rgb = PyArray1::from_slice(py, picture.values())
            .reshape(shape)
            .map_err(|error| error.to_string())?;
        let params = from_json(py, params).map_err(|error| error.to_string())?;
        let result = self
            .function
            .call1(py, (rgb, params))
            .map_err(|error| error.to_string())?;
        let result = result.bind(py);
        let wrong = || {
            format!(
                "must return a float32 ndarray of shape {shape:?}, got {}",
                found(result)
            )
        };
        let array = result.cast::<PyArray3<f32>>().map_err(|_| wrong())?;
        if array.shape() != shape {
            return Err(wrong());
        }
        let values: Vec<f32> = array.readonly().as_array().iter().copied().collect();
        Image::new(shape[0], shape[1], &values).map_err(|error| error.to_string())
    }
}

impl Effect for PythonEffect {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        self.category
    }

    fn opacity(&self) -> f64 {
        self.opacity
    }

    fn apply(&self, picture: &Picture<'_>, params: Value) -> Result<Image, Error> {
        // `_core.apply` runs the core without the interpreter lock; the function needs it back
        Python::attach(|py| self.call(py, picture, &params)).map_err(|message| Error::Foreign {
            effect: self.name.clone(),
            message,
        })
    }
}

/// A `ValueError` carrying `error`'s message.
#[expect(clippy::needless_pass_by_value, reason = "map_err passes the error by value")]
fn value_error(error: impl ToString) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// The `ValueError` for an `rgb` that is not a C-contiguous float32 (height, width, 3) array.
fn picture_error(rgb: &Bound<'_, PyAny>) -> PyErr {
    PyValueError::new_err(format!(
        "rgb must be a C-contiguous float32 array of shape (h, w, 3), got {}",
        found(rgb)
    ))
}

/// What `value` is, for a message about an array that was expected: its shape and dtype if it is an array, else its type.
fn found(value: &Bound<'_, PyAny>) -> String {
    match value.cast::<numpy::PyUntypedArray>() {
        Ok(array) => format!("an array of shape {:?}, dtype {}", array.shape(), array.dtype()),
        Err(_) => format!("{}", value.get_type()),
    }
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

//! `bullterrier_image._native._core`: runs the effects of the Rust core on numpy pictures. Generic: it names no effect, so
//! porting one never changes it.

mod run;

use std::sync::Arc;

use bullterrier_image::{
    BlendMode, Category, Effect, Error, ErrorKind, FieldParam, Image, Input, Inputs, Labels, Layer, Picture, Recipe,
    STEP_KEYS,
};
use numpy::{PyArray1, PyArray2, PyArray3, PyArrayMethods, PyReadonlyArray3, PyUntypedArrayMethods};
use pyo3::IntoPyObjectExt as _;
use pyo3::create_exception;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde_json::{Map, Value, json};

/// The effects of bullterrier-image's Rust core, run on numpy pictures.
#[pymodule]
mod _core {
    use pyo3::prelude::*;

    /// Put the exception where `import` finds it again, so pickling it (to send it between processes) works.
    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        module
            .py()
            .get_type::<super::CoreError>()
            .setattr("__module__", "bullterrier_image._native._core")
    }

    #[pymodule_export]
    use super::{CoreError, apply, check, effect, effects, params, read_recipe, register, unregister, vocabulary};

    #[pymodule_export]
    use super::run::{check_recipe, render, trace};
}

create_exception!(
    _core,
    CoreError,
    PyValueError,
    "An error of the Rust core: a `ValueError` whose `kind` attribute names which of the package's exceptions it is \
     (`ErrorKind::name` in the core: `recipe`, `unknown_effect`, `subject_not_found`, `depth_not_found`, \
     `map_not_found`, `invalid_image`, `non_finite_image`, or `other`: a programming error). The package turns the \
     kind into its `BullterrierImageError` subclass. When a Python function registered with `register` raised, its \
     own exception comes back instead, unchanged."
);

/// Run the effect called `effect` on `rgb` (float32, C-contiguous, (height, width, 3)) with `params` (exactly the
/// effect's parameters, validated and scaled); a new array of `rgb`'s shape. The effect runs without the GIL.
///
/// A failure is a `CoreError` (a `ValueError`) whose `kind` says which of the package's exceptions it stands for, or,
/// when a Python function registered with `register` raised, that very exception.
///
/// The keyword arguments are what is on offer to the effect, which receives the part it declares and nothing else:
/// `size_ratio` (working pixels per design pixel, a finite number above 0), `partition` (an int32 `(height, width)`
/// array of piece numbers from 0; the pieces are as many as the largest number plus one) and `fields` (a dict from a
/// field parameter's name to a float32 `(height, width)` array of finite values within 0..1).
///
/// `CoreError` if the effect, its parameters, the array or an input are not what the core expects, or the effect
/// declares an input that is not on offer: a programming error, since the package validates a user's values before
/// they get here.
///
/// The caller must not modify `rgb` while this runs. The effect reads the array's memory in place, with the GIL
/// released, and numpy's borrow flags guard only against other Rust borrows: a Python thread that writes into `rgb`
/// meanwhile makes the core read memory that is changing, which is undefined behaviour. The input is deliberately not
/// copied (a 1080p float32 frame is about 25 MB, as much work as a whole light effect); `numpy` works the same way on
/// arrays it is given. A thread that renders must own its frame, or share it read-only.
#[pyfunction]
#[pyo3(signature = (effect, rgb, params, *, size_ratio=1.0, partition=None, fields=None))]
fn apply<'py>(
    py: Python<'py>,
    effect: &str,
    rgb: &Bound<'py, PyAny>,
    params: &Bound<'py, PyDict>,
    size_ratio: f64,
    partition: Option<&Bound<'py, PyAny>>,
    fields: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyArray3<f32>>> {
    let readonly = photo_array(rgb)?;
    let picture = picture_of(&readonly)?;
    let params = to_json(params.as_any())?;
    if !(size_ratio.is_finite() && size_ratio > 0.0) {
        return Err(PyValueError::new_err(format!(
            "size_ratio must be a finite number above 0, got {size_ratio}"
        )));
    }
    let labels = partition.map(labels_of).transpose()?;
    let layers = fields.map(layers_of).transpose()?.unwrap_or_default();
    let mut inputs = Inputs::default().with_size_ratio(size_ratio);
    if let Some(labels) = &labels {
        inputs = inputs.with_partition(labels);
    }
    for (parameter, layer) in &layers {
        inputs = inputs.with_field(parameter.as_str(), layer);
    }
    let image = py
        .detach(|| bullterrier_image::apply(effect, &picture, params, &inputs))
        .map_err(core_error)?;
    let shape = [image.height(), image.width(), 3];
    PyArray1::from_vec(py, image.into_values()).reshape(shape)
}

/// The names of the effects the core can run, sorted: its own and the Python functions registered with `register`. With
/// `declared`, only the effects whose parameters the core declares (`params` describes them): the ones it implements
/// itself.
#[pyfunction]
#[pyo3(signature = (*, declared=false))]
fn effects(declared: bool) -> Vec<String> {
    let mut names = bullterrier_image::effects();
    if declared {
        names.retain(|name| bullterrier_image::effect(name).is_ok_and(|found| found.params().is_some()));
    }
    names
}

/// The parameters of the effect called `effect`, as its Rust struct declares them: a dict `{"doc": str, "fields":
/// [...]}`, each field a dict with `name`, `kind` (`"float"`, `"int"`, `"bool"`, `"choice"` or `"color"`), `meaning`,
/// `default`, `unit`, `min`, `max`, `cyclic`, `choices`, `drivable` and `advanced`. A default or bound is the literal
/// the declaration wrote, a whole number an `int` and a decimal a `float`; a colour's default is its `'#rrggbb'` text and
/// a choice's its name.
///
/// `CoreError` if the core implements no effect of that name.
#[pyfunction]
fn params<'py>(py: Python<'py>, effect: &str) -> PyResult<Bound<'py, PyAny>> {
    let table = bullterrier_image::effect(effect)
        .map_err(core_error)?
        .params()
        .ok_or_else(|| PyValueError::new_err(format!("{effect:?} describes its own parameters")))?;
    from_json(py, &serde_json::to_value(table).map_err(value_error)?)
}

/// What the effect called `effect` says about itself: a dict `{"name": str, "category": str, "opacity": float,
/// "inputs": [str], "fields": {str: str}}`, where `fields` maps each parameter that names a field to the field a step
/// that does not set it uses.
///
/// `CoreError` if there is no effect of that name.
#[pyfunction]
fn effect<'py>(py: Python<'py>, effect: &str) -> PyResult<Bound<'py, PyAny>> {
    let effect = bullterrier_image::effect(effect).map_err(core_error)?;
    let inputs: Vec<_> = effect.inputs().iter().map(|input| input.name()).collect();
    let fields: Map<String, Value> = effect
        .fields()
        .iter()
        .map(|field| (field.parameter.clone(), Value::from(field.default.clone())))
        .collect();
    from_json(
        py,
        &json!({
            "name": effect.name(),
            "category": effect.category().name(),
            "opacity": effect.opacity(),
            "inputs": inputs,
            "fields": fields,
        }),
    )
}

/// The names recipes and effects are described with, in the order the catalogue lists them: a dict `{"categories":
/// [str], "inputs": [str], "blend_modes": [str], "step_keys": [str], "error_kinds": [str]}`; the last are the `kind`s
/// of `CoreError`.
#[pyfunction]
fn vocabulary(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let categories: Vec<_> = Category::ALL.iter().map(|category| category.name()).collect();
    let inputs: Vec<_> = Input::ALL.iter().map(|input| input.name()).collect();
    let blend_modes: Vec<_> = BlendMode::ALL.iter().map(|mode| mode.name()).collect();
    let error_kinds: Vec<_> = ErrorKind::ALL.iter().map(|kind| kind.name()).collect();
    from_json(
        py,
        &json!({
            "categories": categories,
            "inputs": inputs,
            "blend_modes": blend_modes,
            "step_keys": STEP_KEYS,
            "error_kinds": error_kinds,
        }),
    )
}

/// The complete parameters the effect called `effect` takes for the values `params`, as its Rust struct declares them:
/// a dict with every parameter, defaults filled in, a decimal as a `float` (also when given as a whole number), a
/// whole number as an `int` and a colour as a list of three floats 0..1 (given as `'#rrggbb'`, `'#rgb'` or three
/// numbers). A number outside its inclusive range is refused, except an angle declared cyclic, which any real wraps into.
///
/// `CoreError` with the sentence naming the effect, the parameter, the rule and the value, for the first rule `params`
/// break; also if the core implements no such effect or the effect describes its own parameters (a Python function
/// registered with `register`).
#[pyfunction]
fn check<'py>(py: Python<'py>, effect: &str, params: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
    let table = bullterrier_image::effect(effect)
        .map_err(core_error)?
        .params()
        .ok_or_else(|| PyValueError::new_err(format!("{effect:?} describes its own parameters")))?;
    let given = params
        .iter()
        .map(|(key, value)| Ok((key.extract::<String>()?, to_json(&value)?)))
        .collect::<PyResult<Map<_, _>>>()?;
    let checked = table.check(effect, &given).map_err(core_error)?;
    from_json(py, &Value::Object(checked))
}

/// The recipe that the text `text` writes, in `syntax` (`"toml"`, a recipe file, or `"json"`), as a dict: `{"steps":
/// [...], "name", "description", "size", "frame", "regions"}` with a key left out when the text left it unset; a
/// step is `{"effect", "params", "preset", "partition", "mask", "opacity", "blend_mode", "name"}` and an each step
/// `{"each", "looks", "assign", "seed", "feather", "mask", "opacity", "blend_mode", "name"}` likewise (a skipped look
/// is `None`). Only the shape and the rules of the file's format version are read: whether the effects exist or
/// the values are in range is for the recipe's check, and an opacity left out stays unset.
///
/// `CoreError` for text that is not that format or breaks one of those rules, naming the step and the key.
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
    .map_err(core_error)?;
    from_json(py, &serde_json::to_value(&recipe).map_err(value_error)?)
}

/// Add the Python function `function(rgb, params, **inputs) -> rgb` to the core's effects as `name`, in `category` (a
/// name from `vocabulary()`), with `opacity` as the default opacity of its steps. `rgb` is a float32 `(height, width,
/// 3)` array and `params` a dict, as `apply` was given; the function returns a float32 array of the same shape. Then
/// `apply`, `effects` and `effect` know it like the core's own. The name is not checked for form, only for being free.
///
/// `inputs` names the inputs from `vocabulary()` that the function declares and `fields` maps each of its parameters
/// that names a field to the field a step that does not set it uses. The function receives each declared input as a
/// keyword argument and nothing else: `size_ratio` as a float, `partition` as an int32 `(height, width)` array, and
/// each field parameter as a float32 `(height, width)` array under the parameter's name.
///
/// `ValueError` if the name is taken (by a core effect as well), the category or an input is unknown, `opacity` is not
/// a finite number from 0 to 1, a field parameter is named like an input, or `function` is not callable. A function
/// that raises, or returns anything but a float32 array of the picture's shape, makes `apply` raise `ValueError`
/// naming the effect.
#[pyfunction]
#[pyo3(signature = (name, category, opacity, function, *, inputs=Vec::new(), fields=None))]
fn register(
    name: &str,
    category: &str,
    opacity: f64,
    function: &Bound<'_, PyAny>,
    inputs: Vec<String>,
    fields: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
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
    let inputs = inputs
        .into_iter()
        .map(|given| {
            Input::ALL
                .iter()
                .copied()
                .find(|known| known.name() == given)
                .ok_or_else(|| PyValueError::new_err(format!("{given:?} is not an input of effects")))
        })
        .collect::<PyResult<Vec<_>>>()?;
    let fields = fields
        .map(|fields| {
            fields
                .iter()
                .map(|(parameter, default)| {
                    let parameter = parameter.extract::<String>()?;
                    if Input::ALL.iter().any(|input| input.name() == parameter) {
                        return Err(PyValueError::new_err(format!(
                            "the field parameter {parameter:?} is named like an input"
                        )));
                    }
                    Ok(FieldParam::new(parameter, default.extract::<String>()?))
                })
                .collect::<PyResult<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let effect = PythonEffect {
        name: name.to_owned(),
        category,
        opacity,
        inputs,
        fields,
        function: function.clone().unbind(),
    };
    bullterrier_image::register(Arc::new(effect)).map_err(core_error)
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
    inputs: Vec<Input>,
    fields: Vec<FieldParam>,
    function: Py<PyAny>,
}

impl PythonEffect {
    /// Call the function with `picture`, `params` and the inputs it declares; its result as an image of the picture's
    /// size, or what is wrong: the function's own exception, kept whole, or text saying what it returned.
    fn call(
        &self,
        py: Python<'_>,
        picture: &Picture<'_>,
        params: &Value,
        inputs: &Inputs<'_>,
    ) -> Result<Image, Failure> {
        let shape = [picture.height(), picture.width(), 3];
        let rgb = PyArray1::from_slice(py, picture.values()).reshape(shape)?;
        let params = from_json(py, params)?;
        let given = self.keywords(py, picture, inputs)?;
        let result = self.function.call(py, (rgb, params), Some(&given))?;
        let result = result.bind(py);
        let wrong = || {
            format!(
                "must return a float32 ndarray of shape {shape:?}, got {}",
                found(result)
            )
        };
        let array = result.cast::<PyArray3<f32>>().map_err(|_| wrong())?;
        if array.shape() != shape {
            return Err(wrong().into());
        }
        let values: Vec<f32> = array.readonly().as_array().iter().copied().collect();
        Ok(Image::new(shape[0], shape[1], &values)?)
    }

    /// The keyword arguments the function declares, from `inputs`.
    fn keywords<'py>(
        &self,
        py: Python<'py>,
        picture: &Picture<'_>,
        inputs: &Inputs<'_>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let shape = [picture.height(), picture.width()];
        let given = PyDict::new(py);
        let absent = |input: &str| PyValueError::new_err(format!("`{input}` is declared but not given"));
        if self.inputs.contains(&Input::SizeRatio) {
            given.set_item(Input::SizeRatio.name(), inputs.size_ratio())?;
        }
        if self.inputs.contains(&Input::Partition) {
            let labels = inputs.partition().ok_or_else(|| absent(Input::Partition.name()))?;
            given.set_item(
                Input::Partition.name(),
                PyArray1::from_slice(py, labels.values()).reshape(shape)?,
            )?;
        }
        for field in &self.fields {
            let layer = inputs.field(&field.parameter).ok_or_else(|| absent(&field.parameter))?;
            given.set_item(
                &field.parameter,
                PyArray1::from_slice(py, layer.values()).reshape(shape)?,
            )?;
        }
        Ok(given)
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

    fn inputs(&self) -> &[Input] {
        &self.inputs
    }

    fn fields(&self) -> &[FieldParam] {
        &self.fields
    }

    fn apply(&self, picture: &Picture<'_>, params: Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        // `_core.apply` runs the core without the interpreter lock; the function needs it back
        Python::attach(|py| self.call(py, picture, &params, inputs))
            .map_err(|failure| Error::foreign(self.name.clone(), failure))
    }
}

/// What a Python effect can fail with: the exception its function raised (kept whole, so it can be raised again as it
/// was) or text saying what it returned.
type Failure = Box<dyn std::error::Error + Send + Sync>;

/// The exception for `error`: a `CoreError` of the error's kind, except that the exception a registered Python function
/// raised is given back as it was, so the caller catches the class the function chose.
fn core_error(error: Error) -> PyErr {
    let error = match error {
        Error::Foreign { effect, source } => match source.downcast::<PyErr>() {
            Ok(raised) => return *raised,
            Err(source) => Error::Foreign { effect, source },
        },
        other => other,
    };
    let kind = error.kind().name();
    let problem = CoreError::new_err(error.to_string());
    Python::attach(|py| match problem.value(py).setattr("kind", kind) {
        Ok(()) => problem,
        Err(unset) => unset,
    })
}

/// A `ValueError` carrying `error`'s message.
#[expect(clippy::needless_pass_by_value, reason = "map_err passes the error by value")]
fn value_error(error: impl ToString) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// The labelling `partition` (an int32 `(height, width)` array) makes, among as many pieces as its largest number plus
/// one.
fn labels_of(partition: &Bound<'_, PyAny>) -> PyResult<Labels> {
    let array = partition.cast::<PyArray2<i32>>().map_err(|_| {
        PyValueError::new_err(format!(
            "partition must be an int32 array of shape (h, w), got {}",
            found(partition)
        ))
    })?;
    let &[height, width] = array.shape() else {
        return Err(PyValueError::new_err(format!(
            "partition must be an int32 array of shape (h, w), got {}",
            found(partition)
        )));
    };
    let labels: Vec<i32> = array.readonly().as_array().iter().copied().collect();
    let pieces = labels
        .iter()
        .copied()
        .max()
        .map_or(0, |largest| usize::try_from(largest).map_or(0, |largest| largest + 1));
    Labels::new(height, width, labels, pieces).map_err(core_error)
}

/// The map of every `(parameter, array)` in `fields`: float32 `(height, width)` arrays.
fn layers_of(fields: &Bound<'_, PyDict>) -> PyResult<Vec<(String, Layer)>> {
    fields
        .iter()
        .map(|(parameter, array)| {
            let parameter = parameter.extract::<String>()?;
            let wrong = || {
                PyValueError::new_err(format!(
                    "the field {parameter:?} must be a float32 array of shape (h, w), got {}",
                    found(&array)
                ))
            };
            let typed = array.cast::<PyArray2<f32>>().map_err(|_| wrong())?;
            let &[height, width] = typed.shape() else {
                return Err(wrong());
            };
            let values: Vec<f32> = typed.readonly().as_array().iter().copied().collect();
            let layer = Layer::new(height, width, values)
                .map_err(|error| core_error(Error::Inputs(format!("the field {parameter:?}: {error}"))))?;
            Ok((parameter, layer))
        })
        .collect()
}

/// The array `rgb` for reading: a C-contiguous float32 `(height, width, 3)` array, borrowed in place.
fn photo_array<'py>(rgb: &Bound<'py, PyAny>) -> PyResult<PyReadonlyArray3<'py, f32>> {
    let array = rgb.cast::<PyArray3<f32>>().map_err(|_| picture_error(rgb))?;
    if !matches!(array.shape(), &[_, _, 3]) || !array.is_c_contiguous() {
        return Err(picture_error(rgb));
    }
    array.try_readonly().map_err(value_error)
}

/// The picture `readonly` holds, read in place.
fn picture_of<'a>(readonly: &'a PyReadonlyArray3<'_, f32>) -> PyResult<Picture<'a>> {
    let &[height, width, _] = readonly.shape() else {
        return Err(value_error("a picture is (height, width, 3)"));
    };
    Picture::new(height, width, readonly.as_slice().map_err(value_error)?).map_err(core_error)
}

/// The `CoreError` of the invalid-image kind for an `rgb` that is not a C-contiguous float32 (height, width, 3) array.
fn picture_error(rgb: &Bound<'_, PyAny>) -> PyErr {
    core_error(Error::InvalidImage(format!(
        "rgb must be a C-contiguous float32 array of shape (h, w, 3), got {}",
        found(rgb)
    )))
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

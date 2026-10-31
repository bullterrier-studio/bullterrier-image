//! `_core.render` and `_core.trace`: a recipe run on a numpy photo by the core's runner.

use std::sync::Arc;

use bullterrier_image::{Error, Finder, Image, Layer, Options, Photo, Presets, Recipe, Stage};
use numpy::{AllowTypeChange, PyArray1, PyArray3, PyArrayLike2, PyArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::{core_error, found, photo_array, picture_of, to_json, value_error};

/// Run `recipe` on `rgb` (float32, C-contiguous, `(height, width, 3)`) and return the picture it makes: a new array of
/// the same shape. The core's runner does the work, without the GIL; it borrows `rgb` as `apply` does, so the same
/// warning holds: do not modify `rgb` while this runs.
///
/// `recipe` is the recipe as a dict, in the form `read_recipe` returns: `{"steps": [...], ...}`. Its steps run in
/// order, each on the previous one's output, laid over it by the step's `blend_mode`, `opacity` and `mask`. A parameter
/// an effect declares drivable may be a drive, `{"by": ..., "start": ..., "end": ..., "levels": ...}`. A recipe with a
/// non-default `frame` or an `each` step is refused: the core does not run them yet.
///
/// `maps` hands the photo's maps in by name: `subject` (white = subject), `depth` (1 = nearest) and one for each region
/// of kind `given` the recipe declares, each a float array `(height, width)` of finite values within 0..1. `finders`
/// gives, by the same names, an object with a method `find(rgb)`, which is called with the photo as given (a float32
/// array) at most once, when a step uses the map and none was handed in, and returns the map or raises: its exception
/// comes back from `render` as it was. `background` and `light` are made from the photo. A name that is no map of the
/// recipe is refused.
///
/// `presets` holds the presets the steps name, by effect: the TOML text of the effect's presets file, one `[name]` table
/// for each preset with its `description` and the effect's parameters (`{"grade": "[warm]\ndescription = ...\n"}`).
///
/// `size` is the longest side in px of the picture the steps work on and the render returns (a whole number from 1 up; a
/// negative one is an `OverflowError`): the photo and its maps are resampled to it, the aspect kept. Without it the
/// recipe's own `size` applies, and without either the photo keeps its size. Every `px` value of the recipe is written
/// for a longest side of 480 and multiplied by the working picture's longest side over 480, so the look is the same at
/// any size.
///
/// A failure is a `CoreError` (a `ValueError`) whose `kind` says which of the package's exceptions it stands for, or,
/// when a Python function the run called raised (an effect, a finder), that very exception.
#[pyfunction]
#[pyo3(signature = (rgb, recipe, *, maps=None, finders=None, presets=None, size=None))]
pub(crate) fn render<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    recipe: &Bound<'py, PyDict>,
    maps: Option<&Bound<'py, PyDict>>,
    finders: Option<&Bound<'py, PyDict>>,
    presets: Option<&Bound<'py, PyDict>>,
    size: Option<u32>,
) -> PyResult<Bound<'py, PyArray3<f32>>> {
    let readonly = photo_array(rgb)?;
    let picture = picture_of(&readonly)?;
    let photo = photo_with(Photo::new(picture), maps, finders)?;
    let (recipe, presets) = (recipe_of(recipe)?, presets_of(presets)?);
    let options = size.map_or_else(Options::default, |size| Options::default().with_size(size));
    let image = py
        .detach(|| photo.render(&recipe, &presets, options))
        .map_err(core_error)?;
    image_array(py, image)
}

/// Every stage of rendering `recipe` on `rgb`, with the arguments and the failures of `render`: a list of `(kind,
/// number, array)`, in this order. `("input", None, rgb)` is the photo, `("subject", None, map)` and `("depth", None,
/// map)` are the maps the recipe uses (float32 `(height, width)`, if it uses them) and `("step", n, picture)` is the
/// picture after step `n`, counting from 1. The last stage is exactly what `render` returns. Naming the stages is left
/// to the caller.
#[pyfunction]
#[pyo3(signature = (rgb, recipe, *, maps=None, finders=None, presets=None, size=None))]
pub(crate) fn trace<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    recipe: &Bound<'py, PyDict>,
    maps: Option<&Bound<'py, PyDict>>,
    finders: Option<&Bound<'py, PyDict>>,
    presets: Option<&Bound<'py, PyDict>>,
    size: Option<u32>,
) -> PyResult<Bound<'py, PyList>> {
    let readonly = photo_array(rgb)?;
    let picture = picture_of(&readonly)?;
    let photo = photo_with(Photo::new(picture), maps, finders)?;
    let (recipe, presets) = (recipe_of(recipe)?, presets_of(presets)?);
    let options = size.map_or_else(Options::default, |size| Options::default().with_size(size));
    let stages = py
        .detach(|| photo.trace(&recipe, &presets, options))
        .map_err(core_error)?;
    let shown = stages
        .into_iter()
        .map(|stage| stage_of(py, stage))
        .collect::<PyResult<Vec<_>>>()?;
    PyList::new(py, shown)
}

/// `photo` with the maps handed in and the finders given.
fn photo_with<'a>(
    mut photo: Photo<'a>,
    maps: Option<&Bound<'_, PyDict>>,
    finders: Option<&Bound<'_, PyDict>>,
) -> PyResult<Photo<'a>> {
    for (name, array) in maps.into_iter().flatten() {
        let name = name.extract::<String>()?;
        let layer = layer_of(&name, &array).map_err(core_error)?;
        photo = photo.with_map(name, layer).map_err(core_error)?;
    }
    for (name, finder) in finders.into_iter().flatten() {
        let name = name.extract::<String>()?;
        if !finder.hasattr("find")? {
            return Err(PyValueError::new_err(format!(
                "the finder of {name:?} needs a `find` method taking the photo and returning the map, got {}",
                finder.get_type()
            )));
        }
        photo = photo.with_finder(
            name.clone(),
            Arc::new(PyFinder {
                name,
                finder: finder.unbind(),
            }),
        );
    }
    Ok(photo)
}

/// The recipe the dict `recipe` writes.
fn recipe_of(recipe: &Bound<'_, PyDict>) -> PyResult<Recipe> {
    Recipe::from_json(&to_json(recipe.as_any())?.to_string()).map_err(core_error)
}

/// The presets of each effect in `presets`, which are TOML text.
fn presets_of(presets: Option<&Bound<'_, PyDict>>) -> PyResult<Presets> {
    let mut all = Presets::new();
    for (effect, text) in presets.into_iter().flatten() {
        all.add_toml(&effect.extract::<String>()?, &text.extract::<String>()?)
            .map_err(core_error)?;
    }
    Ok(all)
}

/// The map `name` that `array` holds: any array of numbers `(height, width)` (converted to float32), once it is a map: finite
/// and within 0..1.
fn layer_of(name: &str, array: &Bound<'_, PyAny>) -> Result<Layer, Error> {
    let numbers: PyArrayLike2<'_, f32, AllowTypeChange> = array.extract().map_err(|_| {
        Error::map_not_found(
            name,
            format!(
                "the map '{name}' must be an array of numbers of shape (h, w), got {}",
                found(array)
            ),
        )
    })?;
    let view = numbers.as_array();
    let (height, width) = view.dim();
    Layer::new(height, width, view.iter().copied().collect())
        .map_err(|why| Error::map_not_found(name, format!("the map '{name}' is no map of the photo: {why}")))
}

/// A Python object with a method `find(rgb)`, as the core's finder of the map `name`.
struct PyFinder {
    name: String,
    finder: Py<PyAny>,
}

impl PyFinder {
    /// Who failed, for an error that carries the exception its finder raised.
    fn who(&self) -> String {
        format!("the finder of '{}'", self.name)
    }
}

impl Finder for PyFinder {
    /// Calls the object's `find` with a copy of the photo, under the GIL. What it raises is kept whole, to be raised
    /// again; what it returns must be a map.
    fn find(&self, picture: &bullterrier_image::Picture<'_>) -> Result<Layer, Error> {
        Python::attach(|py| {
            let shape = [picture.height(), picture.width(), 3];
            let rgb = PyArray1::from_slice(py, picture.values())
                .reshape(shape)
                .map_err(|error| Error::foreign(self.who(), error))?;
            let answer = self
                .finder
                .call_method1(py, "find", (rgb,))
                .map_err(|raised| Error::foreign(self.who(), raised))?;
            let answer = answer.bind(py);
            if answer.is_none() {
                return Err(Error::map_not_found(
                    &self.name,
                    format!(
                        "the finder of '{0}' returned None: it must return the map '{0}' (float32, the photo's height \
                         and width, 0..1) or raise when it finds none",
                        self.name
                    ),
                ));
            }
            layer_of(&self.name, answer)
        })
    }
}

/// `image` as a numpy array `(height, width, 3)`.
fn image_array(py: Python<'_>, image: Image) -> PyResult<Bound<'_, PyArray3<f32>>> {
    let shape = [image.height(), image.width(), 3];
    PyArray1::from_vec(py, image.into_values()).reshape(shape)
}

/// The tuple `(kind, number, array)` a stage is shown as.
fn stage_of(py: Python<'_>, stage: Stage) -> PyResult<Bound<'_, PyTuple>> {
    let map = |layer: Layer| PyArray1::from_slice(py, layer.values()).reshape([layer.height(), layer.width()]);
    let (kind, number, array) = match stage {
        Stage::Input(image) => ("input", None, image_array(py, image)?.into_any()),
        Stage::Subject(layer) => ("subject", None, map(layer)?.into_any()),
        Stage::Depth(layer) => ("depth", None, map(layer)?.into_any()),
        Stage::Step { number, image } => ("step", Some(number), image_array(py, image)?.into_any()),
        other => {
            return Err(value_error(format!(
                "this binding does not know the stage {other:?} of a newer core"
            )));
        }
    };
    PyTuple::new(
        py,
        [
            kind.into_pyobject(py)?.into_any(),
            number.into_pyobject(py)?.into_any(),
            array,
        ],
    )
}

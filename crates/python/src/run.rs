//! `_core.render` and `_core.trace`: a recipe run on a numpy photo by the core's runner.

use std::num::NonZeroUsize;
use std::sync::Arc;

use bullterrier_image::{Error, ErrorKind, Finder, Image, Layer, MapKind, Options, Photo, Presets, Recipe, Stage};
use numpy::{AllowTypeChange, PyArray1, PyArray3, PyArrayLike2, PyArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyTuple};
use serde_json::json;

use super::lut::tables_of;
use super::{CoreError, core_error, found, from_json, photo_array, picture_of, pictures_of, to_json, value_error};

/// Run `recipe` on `rgb` (float32, C-contiguous, `(height, width, 3)`) and return the picture it makes: a new float32
/// array `(height, width, 3)`, or `(height, width, 4)` (straight alpha last) when the recipe's frame has a `transparent`
/// cut. The core's runner does the work, without the GIL; it borrows `rgb` as `apply` does, so the same
/// warning holds: do not modify `rgb` while this runs.
///
/// `recipe` is the recipe as a dict, in the form `read_recipe` returns: `{"steps": [...], ...}`. Its steps run in
/// order, each on the previous one's output, laid over it by the step's `blend_mode`, `opacity` and `mask`. A parameter
/// an effect declares drivable may be a drive, `{"by": ..., "start": ..., "end": ..., "levels": ...}`. A recipe whose
/// steps use a region of kind `shards`, `cuts` or `svg` is refused: the core does not make
/// those yet. An `each` step with `assign` `random` draws its looks from the core's own seeded stream, which is not
/// numpy's.
///
/// `maps` hands the photo's maps in by name: `subject` (white = subject), `depth` (1 = nearest) and one for each region
/// of kind `given` the recipe declares, each a float array `(height, width)` of finite values within 0..1. `finders`
/// gives, by the same names, an object with a method `find(rgb)`, which is called with the photo as given (a float32
/// array) at most once, when a step uses the map and none was handed in, and returns the map or raises: its exception
/// comes back from `render` as it was, except a `CoreError` whose `kind` is `subject_not_found`, `depth_not_found` or
/// `map_not_found`, which says the finder found nothing: the render fails as for any map that is missing. The
/// `subject` finder is also asked about the crop of each piece for a `subject` region found `per` piece; there,
/// "found nothing" means the piece adds nothing. `background` and `light` are made from the photo. A name that is no map of the
/// recipe is refused.
///
/// `luts` hands lookup tables in by name: a dict `{name: Lut}`. A step's lookup table parameter (such as `lut`'s `file`)
/// that holds a name in it uses that table, shared with every render it is given to; any other name is read as the path
/// of a `.cube` file, once per render, and a path that cannot be read is a `CoreError` of kind `lut` naming it.
///
/// `pictures` hands pictures in by name: a dict `{name: array}`, each a float32 `(height, width, 4)` array of straight-alpha RGBA
/// values within 0..1. A step's picture parameter (such as `blend`'s `image`) that holds a name in it uses that picture; any
/// other name is read as the path of an image file, once per render, and a path that cannot be read is a `CoreError` of
/// kind `image_read` naming it.
///
/// `presets` holds the presets the steps name, by effect: the TOML text of the effect's presets file, one `[name]` table
/// for each preset with its `description` and the effect's parameters (`{"grade": "[warm]\ndescription = ...\n"}`).
///
/// `size` is the longest side in px of the picture the steps work on and the render returns (a whole number from 1 up): the photo and its maps are resampled to it, the aspect kept. Without it the
/// recipe's own `size` applies, and without either the photo keeps its size. Every `px` value of the recipe is written
/// for a longest side of 480 and multiplied by the working picture's longest side over 480, so the look is the same at
/// any size.
///
/// `threads` is how many threads the render may use for the rows of its work (laying a step over its input, framing and
/// resizing, an effect that works pixel by pixel): 1 (none: the default) starts none, 0 is one for each core, and the picture
/// is the same bytes at any number. A number above 256 is a `CoreError` of kind `recipe`. The render itself stays on the
/// calling thread, so a Python effect it runs keeps its context.
///
/// `band` is how many rows of the working picture the render makes at a time (a positive whole number; none: the whole
/// picture): the same picture, in less memory for a run of steps that read a bounded neighbourhood of a pixel. A `band`
/// of 0 is a `CoreError` of kind `recipe`.
///
/// A failure is a `CoreError` (a `ValueError`) whose `kind` says which of the package's exceptions it stands for, or,
/// when a Python function the run called raised (an effect, a finder), that very exception.
#[pyfunction]
#[pyo3(signature = (rgb, recipe, *, maps=None, finders=None, luts=None, pictures=None, presets=None, size=None, threads=None, band=None))]
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the Python function's keywords"
)]
pub(crate) fn render<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    recipe: &Bound<'py, PyDict>,
    maps: Option<&Bound<'py, PyDict>>,
    finders: Option<&Bound<'py, PyDict>>,
    luts: Option<&Bound<'py, PyDict>>,
    pictures: Option<&Bound<'py, PyDict>>,
    presets: Option<&Bound<'py, PyDict>>,
    size: Option<&Bound<'py, PyAny>>,
    threads: Option<&Bound<'py, PyAny>>,
    band: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyArray3<f32>>> {
    let readonly = photo_array(rgb)?;
    let picture = picture_of(&readonly)?;
    let photo = photo_with(Photo::new(picture), maps, finders, luts, pictures)?;
    let (recipe, presets) = (recipe_of(recipe)?, presets_of(presets)?);
    let options = options_of(size, threads, band)?;
    let image = py
        .detach(|| photo.render(&recipe, &presets, options))
        .map_err(core_error)?;
    image_array(py, image)
}

/// Check `recipe` (a dict, as for `render`) against the rules of a recipe without running it: the effects exist, every
/// value is within its range, the frame is valid, a mask names a region. `presets` as for `render`.
///
/// Returns nothing; a `CoreError` (a `ValueError`) naming the step or `frame.<field>`, the value and the rule for the
/// first rule the recipe breaks.
#[pyfunction]
#[pyo3(signature = (recipe, *, presets=None))]
pub(crate) fn check_recipe(recipe: &Bound<'_, PyDict>, presets: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    recipe_of(recipe)?
        .checked(&presets_of(presets)?)
        .map(|_| ())
        .map_err(core_error)
}

/// What rendering `recipe` (a dict, as for `render`) asks of the photo: `{"maps": [str], "finds_pieces": bool}`. `maps`
/// are the names the photo must be handed or find a map under (sorted): `subject` for the built-in `subject` and
/// `background`, a `subject` region found without `per` and a frame that crops around it or cuts by it, `depth`, and the
/// name of each `given` region a step uses, however indirectly; never `light`, which is made from the photo. A region
/// that is declared and used by no step needs nothing. `finds_pieces` says a region the steps use finds the subject in
/// each piece of a partition, so the photo needs a `subject` finder (a map of the whole photo does not do). `presets` as
/// for `render`: a preset can set the field a step works from.
///
/// `CoreError` for whatever makes the recipe invalid, as `check_recipe`.
#[pyfunction]
#[pyo3(signature = (recipe, *, presets=None))]
pub(crate) fn needs<'py>(
    py: Python<'py>,
    recipe: &Bound<'py, PyDict>,
    presets: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyAny>> {
    let needs = recipe_of(recipe)?.needs(&presets_of(presets)?).map_err(core_error)?;
    from_json(py, &json!({"maps": needs.maps, "finds_pieces": needs.finds_pieces}))
}

/// The names a photo can be handed a map or a finder under for `recipe` (a dict, as for `render`): `subject`, `depth`,
/// then each region of kind `given` in the order it was declared. A render refuses a photo with any other. Nothing is
/// checked.
#[pyfunction]
pub(crate) fn known_maps(recipe: &Bound<'_, PyDict>) -> PyResult<Vec<String>> {
    Ok(recipe_of(recipe)?.known_maps().into_iter().map(str::to_owned).collect())
}

/// Every stage of rendering `recipe` on `rgb`, with the arguments and the failures of `render`: a list of `(kind,
/// number or name, array)`, in this order. `("input", None, rgb)` is the photo, `("subject", None, map)` and
/// `("depth", None, map)` are the maps the recipe uses (float32 `(height, width)`, if it uses them),
/// `("selection", name, map)` and `("partition", name, labels)` are each region the steps use, or that a region they use
/// is made from, in the order the recipe declares them (a selection is a float32 `(height, width)` map; a partition an
/// int32 `(height, width)` array of piece numbers from 0), `("step", n, picture)` is the picture after step `n`,
/// counting from 1, and `("cut_out", None, picture)`, only when the recipe's frame cuts the picture out, is that picture
/// with its alpha, `(height, width, 4)`. The last stage is exactly what `render` returns. Naming the stages is left to
/// the caller.
#[pyfunction]
#[pyo3(signature = (rgb, recipe, *, maps=None, finders=None, luts=None, pictures=None, presets=None, size=None, threads=None))]
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the Python function's keywords"
)]
pub(crate) fn trace<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    recipe: &Bound<'py, PyDict>,
    maps: Option<&Bound<'py, PyDict>>,
    finders: Option<&Bound<'py, PyDict>>,
    luts: Option<&Bound<'py, PyDict>>,
    pictures: Option<&Bound<'py, PyDict>>,
    presets: Option<&Bound<'py, PyDict>>,
    size: Option<&Bound<'py, PyAny>>,
    threads: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let readonly = photo_array(rgb)?;
    let picture = picture_of(&readonly)?;
    let photo = photo_with(Photo::new(picture), maps, finders, luts, pictures)?;
    let (recipe, presets) = (recipe_of(recipe)?, presets_of(presets)?);
    let options = options_of(size, threads, None)?;
    let stages = py
        .detach(|| photo.trace(&recipe, &presets, options))
        .map_err(core_error)?;
    let shown = stages
        .into_iter()
        .map(|stage| stage_of(py, stage))
        .collect::<PyResult<Vec<_>>>()?;
    PyList::new(py, shown)
}

/// The options of a render: the longest side `size`, if asked for, `threads` (0: every core) and the `band` of rows.
///
/// The one place a value that is no whole number in the type's range (a bool, a float, text, a negative number, one past
/// the largest) becomes a `CoreError` of kind `recipe` naming the option and the value; the core refuses the rest (a size
/// of 0, more threads than allowed, a picture over the pixel limit).
fn options_of(
    size: Option<&Bound<'_, PyAny>>,
    threads: Option<&Bound<'_, PyAny>>,
    band: Option<&Bound<'_, PyAny>>,
) -> PyResult<Options> {
    let mut options = Options::default();
    if let Some(threads) = threads {
        options = options.with_threads(whole::<usize>("threads", threads)?);
    }
    if let Some(size) = size {
        options = options.with_size(whole::<u32>("size", size)?);
    }
    if let Some(band) = band {
        let rows = NonZeroUsize::new(whole::<usize>("band", band)?).ok_or_else(|| refused("band", band))?;
        options = options.with_band(rows);
    }
    Ok(options)
}

/// `value` as the whole number the option `name` is: not a bool, and within the range of `T`.
fn whole<T: TryFrom<u64>>(name: &str, value: &Bound<'_, PyAny>) -> PyResult<T> {
    if value.is_instance_of::<PyBool>() {
        return Err(refused(name, value));
    }
    let number = value.extract::<u64>().map_err(|_| refused(name, value))?;
    T::try_from(number).map_err(|_| refused(name, value))
}

/// The `CoreError` of kind `recipe` for an option `name` given `value`, which is no whole number in range.
fn refused(name: &str, value: &Bound<'_, PyAny>) -> PyErr {
    let shown = value
        .repr()
        .map_or_else(|_| "an object".to_owned(), |text| text.to_string());
    core_error(Error::Recipe(format!(
        "`{name}` must be a whole number in range, got {shown}"
    )))
}

/// `photo` with the maps handed in, the finders given and the lookup tables given.
fn photo_with<'a>(
    mut photo: Photo<'a>,
    maps: Option<&Bound<'_, PyDict>>,
    finders: Option<&Bound<'_, PyDict>>,
    luts: Option<&Bound<'_, PyDict>>,
    pictures: Option<&Bound<'_, PyDict>>,
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
    for (name, table) in tables_of(luts)? {
        photo = photo.with_lut(name, table);
    }
    for (name, image) in pictures_of(pictures)? {
        photo = photo.with_image(name, Arc::new(image));
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

    /// The error for what the finder raised: "found nothing" if it is a `CoreError` of one of the not-found kinds, as the
    /// error of that kind (the finder's own words, the kind it said: only the subject's is a piece with no subject in it),
    /// else the exception itself, to come back as it was.
    fn failed(&self, py: Python<'_>, raised: PyErr) -> Error {
        const NOT_FOUND: [(ErrorKind, MapKind); 3] = [
            (ErrorKind::SubjectNotFound, MapKind::Subject),
            (ErrorKind::DepthNotFound, MapKind::Depth),
            (ErrorKind::MapNotFound, MapKind::Other),
        ];
        let value = raised.value(py);
        let said = raised
            .is_instance_of::<CoreError>(py)
            .then(|| value.getattr("kind").and_then(|kind| kind.extract::<String>()).ok())
            .flatten()
            .and_then(|said| {
                NOT_FOUND
                    .iter()
                    .find(|(kind, _)| kind.name() == said)
                    .map(|(_, map)| *map)
            });
        match said {
            Some(kind) => Error::MapNotFound {
                map: self.name.clone(),
                kind,
                message: value.to_string(),
            },
            None => Error::foreign(self.who(), raised),
        }
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
                .map_err(|raised| self.failed(py, raised))?;
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

/// `image` as a numpy array `(height, width, channels)`: 3, or 4 with alpha.
fn image_array(py: Python<'_>, image: Image) -> PyResult<Bound<'_, PyArray3<f32>>> {
    let shape = [image.height(), image.width(), image.channels()];
    PyArray1::from_vec(py, image.into_values()).reshape(shape)
}

/// The tuple `(kind, number or name, array)` a stage is shown as: the number of a step, the name of a region.
fn stage_of(py: Python<'_>, stage: Stage) -> PyResult<Bound<'_, PyTuple>> {
    let map = |layer: Layer| PyArray1::from_slice(py, layer.values()).reshape([layer.height(), layer.width()]);
    let (kind, place, array): (_, Option<Bound<'_, PyAny>>, _) = match stage {
        Stage::Input(image) => ("input", None, image_array(py, image)?.into_any()),
        Stage::Subject(layer) => ("subject", None, map(layer)?.into_any()),
        Stage::Depth(layer) => ("depth", None, map(layer)?.into_any()),
        Stage::Selection { name, layer } => (
            "selection",
            Some(name.into_pyobject(py)?.into_any()),
            map(layer)?.into_any(),
        ),
        Stage::Partition { name, labels } => {
            let shape = [labels.height(), labels.width()];
            let array = PyArray1::from_slice(py, labels.values()).reshape(shape)?;
            ("partition", Some(name.into_pyobject(py)?.into_any()), array.into_any())
        }
        Stage::Step { number, image } => (
            "step",
            Some(number.into_pyobject(py)?.into_any()),
            image_array(py, image)?.into_any(),
        ),
        Stage::CutOut(image) => ("cut_out", None, image_array(py, image)?.into_any()),
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
            place.unwrap_or_else(|| py.None().into_bound(py)),
            array,
        ],
    )
}

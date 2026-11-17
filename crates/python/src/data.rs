//! `_core.ink_planes`, `_core.palette_map` and `_core.characters`: the data the last step of a recipe makes beside its
//! picture (see the core's `Photo::data`).

use bullterrier_image::{Error, Options, Photo, Presets, Recipe};
use numpy::{PyArray1, PyArray2, PyArrayMethods};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::core_error;
use super::run::{Count, SIZE, options_of, photo_with, presets_of, recipe_of};

/// `#rrggbb` for a colour in 0..1.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level in 0..1 times 255, rounded, is a byte"
)]
fn hex([red, green, blue]: [f32; 3]) -> String {
    let byte = |level: f32| (level.clamp(0.0, 1.0) * 255.0).round() as u8; // constant: 8-bit levels
    format!("#{:02x}{:02x}{:02x}", byte(red), byte(green), byte(blue))
}

/// `recipe` set up on `rgb` as `render` sets it up (the same keywords), then `ask` of the photo, without the GIL.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the Python function's keywords"
)]
fn asked<'py, T: Send>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    recipe: &Bound<'py, PyDict>,
    maps: Option<&Bound<'py, PyDict>>,
    finders: Option<&Bound<'py, PyDict>>,
    luts: Option<&Bound<'py, PyDict>>,
    pictures: Option<&Bound<'py, PyDict>>,
    presets: Option<&Bound<'py, PyDict>>,
    size: Option<Count<SIZE>>,
    ask: impl FnOnce(&Photo<'_>, &Recipe, &Presets, Options) -> Result<T, Error> + Send,
) -> PyResult<T> {
    let readonly = super::photo_array(rgb)?;
    let picture = super::picture_of(&readonly)?;
    let photo = photo_with(Photo::new(picture), maps, finders, luts, pictures)?;
    let (recipe, presets) = (recipe_of(recipe)?, presets_of(presets)?);
    let options = options_of(size, Count::one(), None)?;
    py.detach(|| ask(&photo, &recipe, &presets, options))
        .map_err(core_error)
}

/// The ink coverage planes of the last step of `recipe` on `rgb`: a list of float32 `(height, width)` arrays in 0..1, one
/// for each ink. The keywords are those of `render` (without `threads` and `band`). `CoreError` of kind `recipe` naming the
/// last step's effect if it makes no planes with its parameters, or is laid over its input.
#[pyfunction]
#[pyo3(signature = (rgb, recipe, *, maps=None, finders=None, luts=None, pictures=None, presets=None, size=None))]
#[expect(
    clippy::too_many_arguments,
    clippy::redundant_closure_for_method_calls,
    reason = "the arguments are the Python function's keywords; a method path does not fit the closure's lifetimes"
)]
pub(crate) fn ink_planes<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    recipe: &Bound<'py, PyDict>,
    maps: Option<&Bound<'py, PyDict>>,
    finders: Option<&Bound<'py, PyDict>>,
    luts: Option<&Bound<'py, PyDict>>,
    pictures: Option<&Bound<'py, PyDict>>,
    presets: Option<&Bound<'py, PyDict>>,
    size: Option<Count<SIZE>>,
) -> PyResult<Vec<Bound<'py, PyArray2<f32>>>> {
    let planes = asked(
        py,
        rgb,
        recipe,
        maps,
        finders,
        luts,
        pictures,
        presets,
        size,
        |photo, recipe, presets, options| photo.ink_planes(recipe, presets, options),
    )?;
    planes
        .iter()
        .map(|layer| PyArray1::from_slice(py, layer.values()).reshape([layer.height(), layer.width()]))
        .collect()
}

/// The palette-index map of the last step of `recipe` on `rgb` and its palette: `(labels, colours)`, an int32
/// `(height, width)` array of indices into `colours`, a list of `'#rrggbb'` text.
#[pyfunction]
#[pyo3(signature = (rgb, recipe, *, maps=None, finders=None, luts=None, pictures=None, presets=None, size=None))]
#[expect(
    clippy::too_many_arguments,
    clippy::redundant_closure_for_method_calls,
    reason = "the arguments are the Python function's keywords; a method path does not fit the closure's lifetimes"
)]
pub(crate) fn palette_map<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    recipe: &Bound<'py, PyDict>,
    maps: Option<&Bound<'py, PyDict>>,
    finders: Option<&Bound<'py, PyDict>>,
    luts: Option<&Bound<'py, PyDict>>,
    pictures: Option<&Bound<'py, PyDict>>,
    presets: Option<&Bound<'py, PyDict>>,
    size: Option<Count<SIZE>>,
) -> PyResult<(Bound<'py, PyArray2<i32>>, Vec<String>)> {
    let (map, colours) = asked(
        py,
        rgb,
        recipe,
        maps,
        finders,
        luts,
        pictures,
        presets,
        size,
        |photo, recipe, presets, options| photo.palette_map(recipe, presets, options),
    )?;
    let labels = PyArray1::from_slice(py, map.values()).reshape([map.height(), map.width()])?;
    Ok((labels, colours.into_iter().map(hex).collect()))
}

/// The characters of the last step of `recipe` on `rgb`: a `Characters`.
#[pyfunction]
#[pyo3(signature = (rgb, recipe, *, maps=None, finders=None, luts=None, pictures=None, presets=None, size=None))]
#[expect(
    clippy::too_many_arguments,
    clippy::redundant_closure_for_method_calls,
    reason = "the arguments are the Python function's keywords; a method path does not fit the closure's lifetimes"
)]
pub(crate) fn characters<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    recipe: &Bound<'py, PyDict>,
    maps: Option<&Bound<'py, PyDict>>,
    finders: Option<&Bound<'py, PyDict>>,
    luts: Option<&Bound<'py, PyDict>>,
    pictures: Option<&Bound<'py, PyDict>>,
    presets: Option<&Bound<'py, PyDict>>,
    size: Option<Count<SIZE>>,
) -> PyResult<Characters> {
    asked(
        py,
        rgb,
        recipe,
        maps,
        finders,
        luts,
        pictures,
        presets,
        size,
        |photo, recipe, presets, options| photo.characters(recipe, presets, options),
    )
    .map(Characters)
}

/// A grid of characters with the colour of each, as `ascii` draws them: `rows` lines of `columns` characters.
#[pyclass(frozen, module = "bullterrier_image._native._core")]
pub(crate) struct Characters(bullterrier_image::Characters);

#[pymethods]
impl Characters {
    /// The number of lines.
    #[getter]
    fn rows(&self) -> usize {
        self.0.rows()
    }

    /// The number of characters in a line.
    #[getter]
    fn columns(&self) -> usize {
        self.0.columns()
    }

    /// The characters, `rows` lines of `columns` characters joined by `\n`.
    #[getter]
    fn text(&self) -> String {
        self.0.text()
    }

    /// The ink colour of each character as `'#rrggbb'`, one list per line.
    #[getter]
    fn colors(&self) -> Vec<Vec<String>> {
        self.0
            .colors()
            .chunks(self.0.columns())
            .map(|line| line.iter().copied().map(hex).collect())
            .collect()
    }

    /// The paper colour as `'#rrggbb'`.
    #[getter]
    fn paper(&self) -> String {
        hex(self.0.paper())
    }

    /// The text for a terminal, in 24-bit ANSI colour.
    fn ansi(&self) -> String {
        self.0.ansi()
    }

    /// The text as an HTML fragment: a `<pre>` of coloured `<span>`s.
    fn html(&self) -> String {
        self.0.html()
    }
}

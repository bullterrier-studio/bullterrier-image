//! Finding the subject and the depth of a photo with the core's finders: Apple Vision or ONNX Runtime and the models.

use std::path::PathBuf;

use bullterrier_image::finding::ModelKind;
use bullterrier_image::finding::onnx::Options;
use bullterrier_image_finders::{
    Segmenter, depth_finder, depth_identity, inverse_depth, subject_finder, subject_identity,
};
use numpy::{PyArray1, PyArray2, PyArrayMethods};
use pyo3::prelude::*;

use super::{core_error, photo_array, picture_of};

/// The options the keywords and the environment ask for: the environment (`BT_IMAGE_ONNX_LIB`, `BT_IMAGE_ALLOW_DOWNLOAD`
/// and the model variables) first, then what the caller names. The `library` is a default the caller found (the one the
/// Python package `onnxruntime` carries): the variable wins over it.
fn options(
    kind: ModelKind,
    library: Option<PathBuf>,
    model_file: Option<PathBuf>,
    cache: Option<PathBuf>,
    allow_download: bool,
) -> Options {
    let mut options = Options::from_env(kind);
    if let Some(library) = library {
        options = options.with_default_library(library);
    }
    if let Some(file) = model_file {
        options = options.with_model_file(file);
    }
    if let Some(folder) = cache {
        options = options.with_cache(folder);
    }
    if allow_download {
        options = options.with_download(true);
    }
    options
}

/// The matte of the main subject of `rgb` (a float32 `(height, width, 3)` array), the photo's size, float32 0..1.
///
/// `segmenter` is `"vision"` or `"onnx"`; none leaves the choice to `BT_IMAGE_SEGMENTER`, then to the platform (Apple
/// Vision on macOS 14 and later, the portable model elsewhere). `shrink` and `feather` (px) clean the edge. `library` is
/// the ONNX Runtime library file to use when `BT_IMAGE_ONNX_LIB` names none; `model_file` a local copy of the model (else
/// `BT_IMAGE_SUBJECT_MODEL`, else the cache folder `cache`, default the platform's); `allow_download` (or
/// `BT_IMAGE_ALLOW_DOWNLOAD=1`) lets the model be downloaded when it is not there.
///
/// `CoreError` of kind `subject_not_found` if nothing stands out, `backend_unavailable` for what cannot run here (the
/// message says what to do).
#[pyfunction]
#[pyo3(signature = (rgb, *, segmenter=None, shrink=1.5, feather=1.0, library=None, model_file=None, cache=None, allow_download=false))]
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the Python function's keywords"
)]
pub(crate) fn find_subject<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    segmenter: Option<&str>,
    shrink: f64,
    feather: f64,
    library: Option<PathBuf>,
    model_file: Option<PathBuf>,
    cache: Option<PathBuf>,
    allow_download: bool,
) -> PyResult<Bound<'py, PyArray2<f32>>> {
    let readonly = photo_array(rgb)?;
    let picture = picture_of(&readonly)?;
    let segmenter = segmenter.map(Segmenter::named).transpose().map_err(core_error)?;
    let onnx = options(ModelKind::Subject, library, model_file, cache, allow_download);
    let matte = py
        .detach(|| subject_finder(segmenter, &onnx, (shrink, feather))?.find(&picture))
        .map_err(core_error)?;
    PyArray1::from_slice(py, matte.values()).reshape([matte.height(), matte.width()])
}

/// The depth of `rgb` (a float32 `(height, width, 3)` array), the photo's size, float32 0..1: 1 nearest, 0 farthest, per
/// photo (all 0 when the model tells nothing apart). The keywords are as for `find_subject`, with `BT_IMAGE_DEPTH_MODEL`.
///
/// `CoreError` of kind `backend_unavailable` for what cannot run here (the message says what to do).
#[pyfunction]
#[pyo3(signature = (rgb, *, library=None, model_file=None, cache=None, allow_download=false))]
pub(crate) fn find_depth<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    library: Option<PathBuf>,
    model_file: Option<PathBuf>,
    cache: Option<PathBuf>,
    allow_download: bool,
) -> PyResult<Bound<'py, PyArray2<f32>>> {
    let readonly = photo_array(rgb)?;
    let picture = picture_of(&readonly)?;
    let onnx = options(ModelKind::Depth, library, model_file, cache, allow_download);
    let depth = py.detach(|| depth_finder(&onnx)?.find(&picture)).map_err(core_error)?;
    PyArray1::from_slice(py, depth.values()).reshape([depth.height(), depth.width()])
}

/// The model's relative inverse depth of `rgb` (a float32 `(height, width, 3)` array), the photo's size, float32, larger =
/// nearer, on the model's own scale (not scaled to the photo). The keywords are as for `find_depth`.
///
/// `CoreError` of kind `backend_unavailable` for what cannot run here (the message says what to do).
#[pyfunction]
#[pyo3(signature = (rgb, *, library=None, model_file=None, cache=None, allow_download=false))]
pub(crate) fn find_inverse_depth<'py>(
    py: Python<'py>,
    rgb: &Bound<'py, PyAny>,
    library: Option<PathBuf>,
    model_file: Option<PathBuf>,
    cache: Option<PathBuf>,
    allow_download: bool,
) -> PyResult<Bound<'py, PyArray2<f32>>> {
    let readonly = photo_array(rgb)?;
    let picture = picture_of(&readonly)?;
    let onnx = options(ModelKind::Depth, library, model_file, cache, allow_download);
    let answer = py.detach(|| inverse_depth(&picture, &onnx)).map_err(core_error)?;
    let shape = [answer.shape()[0], answer.shape()[1]];
    PyArray1::from_slice(py, answer.values()).reshape(shape)
}

/// What the answers of the subject finder `find_subject` would make depend on, as text: the SHA-256 of the portable model
/// (`onnx:sha256 ...`; the catalogue's for the standard model, else of the local `model_file` or
/// `BT_IMAGE_SUBJECT_MODEL`) and the release of the ONNX Runtime library that runs it (`library` is as for `find_subject`), or
/// the macOS version for Apple Vision (`vision:macOS ...`). `segmenter` is as for
/// `find_subject`.
///
/// `CoreError` of kind `backend_unavailable` for an unknown segmenter, Vision where it cannot run, or a model file that
/// is not a file.
#[pyfunction]
#[pyo3(signature = (*, segmenter=None, library=None, model_file=None))]
pub(crate) fn subject_finder_identity(
    segmenter: Option<&str>,
    library: Option<PathBuf>,
    model_file: Option<PathBuf>,
) -> PyResult<String> {
    let segmenter = segmenter.map(Segmenter::named).transpose().map_err(core_error)?;
    let onnx = options(ModelKind::Subject, library, model_file, None, false);
    subject_identity(segmenter, &onnx).map_err(core_error)
}

/// What the answers of the depth finder depend on, as text: `onnx:sha256 ` and the SHA-256 of the model (the catalogue's
/// for the standard model, else of the local `model_file` or `BT_IMAGE_DEPTH_MODEL`).
///
/// `CoreError` of kind `backend_unavailable` for a model file that is not a file.
#[pyfunction]
#[pyo3(signature = (*, library=None, model_file=None))]
pub(crate) fn depth_finder_identity(library: Option<PathBuf>, model_file: Option<PathBuf>) -> PyResult<String> {
    let onnx = options(ModelKind::Depth, library, model_file, None, false);
    depth_identity(&onnx).map_err(core_error)
}

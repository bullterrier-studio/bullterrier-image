//! The photo pipeline's project: read from a file, or checked when made in code.

use std::path::PathBuf;

use bullterrier_image::pipeline::Project;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::{core_error, from_json, to_json};

/// The project that the file `path` (a `bt-image.toml`) describes, as a dict: `photos` and `recipes` (lists of paths),
/// `output`, and the settings `formats`, `sizes` (largest first), `quality`, `subject_share` and `steps`.
///
/// `CoreError` of kind `project`, starting with the file's name, for a file that is not TOML, an unknown key, globs that
/// match no photo, no recipe file, or a value a project refuses.
#[pyfunction]
#[expect(clippy::needless_pass_by_value, reason = "PyO3 extracts a path argument by value")]
pub(crate) fn read_project(py: Python<'_>, path: PathBuf) -> PyResult<Bound<'_, PyAny>> {
    let project = Project::read(&path).map_err(core_error)?;
    from_json(py, &project.to_json())
}

/// The project `given` (a dict as `read_project` returns, whose `recipes` are paths or any dict standing for a recipe
/// built in code) held to the rules of a project, as a dict with its settings normalised (the sizes largest first).
///
/// `CoreError` of kind `project` naming the key and the value.
#[pyfunction]
pub(crate) fn check_project<'py>(py: Python<'py>, given: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
    let project = Project::from_json(&to_json(given)?.to_string()).map_err(core_error)?;
    from_json(py, &project.to_json())
}

//! The photo pipeline's project: read from a file, or checked when made in code.

use std::path::PathBuf;

use std::collections::BTreeMap;
use std::sync::Arc;

use bullterrier_image::pipeline::{Found, Given, Project, RunOptions, needed_maps, run};
use bullterrier_image::{Error, Presets};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::lut::tables_of;
use super::run::PyFinder;
use super::{core_error, from_json, pictures_of, to_json};

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

/// The project `given` (as for `check_project`), its recipes written out (a path, or the recipe as a dict), as the maps its
/// recipes need a finder for: a sorted list of names (`subject`, `depth`, the name of a `given` region).
///
/// `CoreError` naming what loading and checking the recipes refuses.
#[pyfunction]
pub(crate) fn project_maps<'py>(py: Python<'py>, given: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
    let project = Project::from_json(&to_json(given)?.to_string()).map_err(core_error)?;
    let presets = Presets::shipped().map_err(core_error)?;
    let maps = needed_maps(&project, &presets).map_err(core_error)?;
    from_json(py, &serde_json::json!(maps))
}

/// Processes every photo of the project `given` (as for `check_project`) with every recipe, and writes the record and the
/// report; returns the items, one for each pair, as dicts (see `read_manifest`'s items: `photo`, `recipe`, `status`, `outputs`,
/// `warnings`, `error`, `steps`, `seconds`, `fingerprint`).
///
/// `finders` is `{map name: (identity, finder)}`, a finder an object with a `find` method; `pictures` and `luts` are what the
/// recipes built in code name; `generated` is the local time of the run as the record writes it; `log` (optional) is called
/// with one line for each pair. A failure of one pair is that pair's item. `CoreError` of kind `project` for what a project
/// refuses (see the Rust `run`), and of the kind of anything else that stops the run.
#[pyfunction]
#[pyo3(signature = (given, finders, *, pictures=None, luts=None, force=false, dry_run=false, only=None, generated="", log=None))]
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the Python function's keywords"
)]
pub(crate) fn run_project<'py>(
    py: Python<'py>,
    given: &Bound<'py, PyDict>,
    finders: &Bound<'py, PyDict>,
    pictures: Option<&Bound<'py, PyDict>>,
    luts: Option<&Bound<'py, PyDict>>,
    force: bool,
    dry_run: bool,
    only: Option<String>,
    generated: &str,
    log: Option<Py<PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let project = Project::from_json(&to_json(given)?.to_string()).map_err(core_error)?;
    let mut inputs = Given::default();
    for (name, pair) in finders.iter() {
        let name = name.extract::<String>()?;
        let (identity, finder): (String, Py<PyAny>) = pair.extract()?;
        let found = Found {
            finder: Arc::new(PyFinder {
                name: name.clone(),
                finder,
            }),
            identity,
        };
        inputs.finders.insert(name, found);
    }
    inputs.pictures = pictures_of(pictures)?
        .into_iter()
        .map(|(name, image)| (name, Arc::new(image)))
        .collect::<BTreeMap<_, _>>();
    inputs.luts = tables_of(luts)?.into_iter().collect();
    let presets = Presets::shipped().map_err(core_error)?;
    let generated = generated.to_owned();
    let items = py
        .detach(|| {
            let options = RunOptions {
                force,
                dry_run,
                only: only.as_deref(),
                generated: &generated,
            };
            run(&project, &presets, &options, &inputs, &mut |line| {
                let Some(log) = &log else { return Ok(()) };
                // what the callback raises (an interrupt included) stops the run and comes back as it was
                Python::attach(|py| {
                    log.call1(py, (line,))
                        .map(drop)
                        .map_err(|raised| Error::foreign("the log of the run", raised))
                })
            })
        })
        .map_err(core_error)?;
    let written = items
        .iter()
        .map(|item| {
            serde_json::to_value(item).map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
        })
        .collect::<PyResult<Vec<_>>>()?;
    PyList::new(
        py,
        written
            .iter()
            .map(|item| from_json(py, item))
            .collect::<PyResult<Vec<_>>>()?,
    )
}

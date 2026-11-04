//! `_core.Lut`: a 3D colour lookup table held by the core, shared with every render that uses it.

use std::path::PathBuf;
use std::sync::Arc;

use numpy::{PyArray1, PyArray4, PyArrayMethods, PyReadonlyArrayDyn, PyUntypedArrayMethods};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::core_error;

/// A 3D colour lookup table: `size` entries per axis, each an output colour, over an input domain. It is checked when it
/// is made, held by the Rust core and shared, not copied, by every render and `apply` it is given to.
///
/// `Lut(table, domain_min=(0, 0, 0), domain_max=(1, 1, 1))`: `table` is a float32 array `(n, n, n, 3)` with 2 <= n <=
/// 256, indexed `[red, green, blue]` by the input colour's position on each axis; `domain_min` and `domain_max` are the
/// input values at the first and last entry of each axis (three finite numbers each, the minimum below the maximum).
/// `CoreError` of kind `lut` for anything else.
#[pyclass(frozen, name = "Lut", module = "bullterrier_image._native._core")]
pub(crate) struct PyLut {
    pub(crate) table: Arc<bullterrier_image::Lut>,
}

#[pymethods]
impl PyLut {
    #[new]
    #[expect(clippy::needless_pass_by_value, reason = "pyo3 extracts an argument by value")]
    #[pyo3(signature = (table, domain_min=[0.0; 3], domain_max=[1.0; 3]))]
    fn new(table: PyReadonlyArrayDyn<'_, f32>, domain_min: [f32; 3], domain_max: [f32; 3]) -> PyResult<Self> {
        let shape = table.shape().to_vec();
        let size = shape.first().copied().unwrap_or(0);
        if shape != [size, size, size, 3] {
            return Err(core_error(bullterrier_image::Error::Lut(format!(
                "a lookup table is a float array of shape (n, n, n, 3), got shape {shape:?}"
            ))));
        }
        let view = table.as_array();
        // the core lists the entries red changing fastest; the array is indexed [red, green, blue]
        let entries = (0..size * size * size)
            .map(|entry| {
                let (red, green, blue) = (entry % size, entry / size % size, entry / (size * size));
                [0, 1, 2].map(|channel| view[[red, green, blue, channel]])
            })
            .collect();
        let table = bullterrier_image::Lut::new(size, domain_min, domain_max, entries).map_err(core_error)?;
        Ok(Self { table: Arc::new(table) })
    }

    /// `Lut.identity(size)`: the table of `size` entries per axis (2 to 256) over the domain 0..1 that changes nothing.
    #[staticmethod]
    fn identity(size: usize) -> PyResult<Self> {
        let table = bullterrier_image::Lut::identity(size).map_err(core_error)?;
        Ok(Self { table: Arc::new(table) })
    }

    /// `Lut.from_cube(path)`: the table of the Adobe/Resolve `.cube` file at `path` (UTF-8 text, at most 64 MiB; a 1D table
    /// is refused). `CoreError` of kind `lut` naming the file, and the number of the line of a fault in it.
    #[staticmethod]
    #[expect(clippy::needless_pass_by_value, reason = "pyo3 extracts an argument by value")]
    fn from_cube(path: PathBuf) -> PyResult<Self> {
        let table = bullterrier_image::Lut::from_cube_file(&path).map_err(core_error)?;
        Ok(Self { table: Arc::new(table) })
    }

    /// The number of entries per axis.
    #[getter]
    fn size(&self) -> usize {
        self.table.size()
    }

    /// The input value at the first entry of each axis (red, green, blue).
    #[getter]
    fn domain_min(&self) -> (f32, f32, f32) {
        let [red, green, blue] = self.table.domain_min();
        (red, green, blue)
    }

    /// The input value at the last entry of each axis (red, green, blue).
    #[getter]
    fn domain_max(&self) -> (f32, f32, f32) {
        let [red, green, blue] = self.table.domain_max();
        (red, green, blue)
    }

    /// The entries as a new float32 array `(n, n, n, 3)`, indexed `[red, green, blue]`.
    fn table<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray4<f32>>> {
        let size = self.table.size();
        let entries = self.table.entries();
        let mut values = Vec::with_capacity(entries.len() * 3);
        for red in 0..size {
            for green in 0..size {
                for blue in 0..size {
                    values.extend_from_slice(&entries[red + size * (green + size * blue)]);
                }
            }
        }
        PyArray1::from_vec(py, values).reshape([size, size, size, 3])
    }
}

/// The tables of a Python dict `{name: Lut}`, as the core holds them.
pub(crate) fn tables_of(luts: Option<&Bound<'_, PyDict>>) -> PyResult<Vec<(String, Arc<bullterrier_image::Lut>)>> {
    luts.into_iter()
        .flatten()
        .map(|(name, lut)| Ok((name.extract::<String>()?, Arc::clone(&lut.cast::<PyLut>()?.get().table))))
        .collect()
}

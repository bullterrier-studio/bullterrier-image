//! `_core.Gpu`: the core's GPU context on this machine's GPU, which a render is given as `gpu=`.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::core_error;

/// A GPU context on this machine's high-performance GPU (Metal, Vulkan or Direct3D 12): its own device, the core's kernels
/// compiled on it as they are needed, and a count of the pictures sent to it and read back. Give it to `render(...,
/// gpu=)`; share one between renders and threads. `Gpu()` takes the device (blocking; without the GIL); a machine with no
/// GPU is a `CoreError` of kind `gpu` naming why.
#[pyclass(frozen, name = "Gpu", module = "bullterrier_image._native._core")]
pub(crate) struct PyGpu {
    pub(crate) gpu: bullterrier_image::Gpu,
}

#[pymethods]
impl PyGpu {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        let gpu = py
            .detach(bullterrier_image::Gpu::request_blocking)
            .map_err(core_error)?;
        Ok(Self { gpu })
    }

    /// The adapter the device is on, as its driver names it: name, backend, kind and driver.
    #[getter]
    fn adapter(&self) -> &str {
        self.gpu.adapter()
    }

    /// The most pixels a picture on this device can have.
    #[getter]
    fn largest_picture(&self) -> usize {
        self.gpu.largest_picture()
    }

    /// What has crossed between the CPU and this context since it was made or last reset: `{"uploads": int, "downloads":
    /// int, "bytes": int}` (pictures and maps sent, pictures read back, the bytes of both).
    #[getter]
    fn transfers<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let transfers = self.gpu.transfers();
        let counts = PyDict::new(py);
        counts.set_item("uploads", transfers.uploads)?;
        counts.set_item("downloads", transfers.downloads)?;
        counts.set_item("bytes", transfers.bytes)?;
        Ok(counts)
    }

    /// Sets the count of `transfers` back to zero.
    fn reset_transfers(&self) {
        self.gpu.reset_transfers();
    }

    fn __repr__(&self) -> String {
        format!("Gpu(adapter={:?})", self.gpu.adapter())
    }
}

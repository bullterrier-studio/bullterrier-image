//! Contact sheets: the core's, on numpy pictures.

use bullterrier_image::{Image, Tile};
use numpy::{PyArray1, PyArray3, PyArrayMethods, PyReadonlyArray3, PyUntypedArrayMethods};
use pyo3::prelude::*;
use pyo3::types::PyList;

use super::{core_error, value_error};

/// The contact sheet of `tiles`, a list of `(label, picture)` pairs: each picture a C-contiguous float32 `(height, width, 3)`
/// array, or `(height, width, 4)` with straight alpha last (shown over a checkerboard). Laid out in `columns` columns, each
/// picture scaled to fit a `tile` px square with its label under it, a label too wide for the tile set smaller and cut with
/// `…` if it still does not fit. A new float32 `(height, width, 3)` array.
///
/// `CoreError` for no tile, no column or a tile of 0 px, and for a picture that is no such array.
#[pyfunction]
pub(crate) fn contact_sheet<'py>(
    py: Python<'py>,
    tiles: &Bound<'py, PyList>,
    columns: usize,
    tile: usize,
) -> PyResult<Bound<'py, PyArray3<f32>>> {
    let mut held: Vec<(String, Image)> = Vec::with_capacity(tiles.len());
    for item in tiles.iter() {
        let (label, picture): (String, Bound<'py, PyAny>) = item.extract()?;
        let array = picture
            .cast::<PyArray3<f32>>()
            .map_err(|_| value_error("a tile's picture is a float32 array of shape (h, w, 3) or (h, w, 4)"))?;
        let readonly: PyReadonlyArray3<'_, f32> = array.try_readonly().map_err(value_error)?;
        let values = readonly
            .as_slice()
            .map_err(|_| value_error("a tile's picture must be C-contiguous"))?;
        let image = match *readonly.shape() {
            [height, width, 3] => Image::new(height, width, values),
            [height, width, 4] => Image::rgba(height, width, values),
            _ => {
                return Err(value_error(
                    "a tile's picture is a float32 array of shape (h, w, 3) or (h, w, 4)",
                ));
            }
        }
        .map_err(core_error)?;
        held.push((label, image));
    }
    let shown: Vec<Tile<'_>> = held.iter().map(|(label, image)| Tile::new(label, image)).collect();
    let sheet = py
        .detach(|| bullterrier_image::contact_sheet(&shown, columns, tile))
        .map_err(core_error)?;
    let shape = [sheet.height(), sheet.width(), sheet.channels()];
    PyArray1::from_vec(py, sheet.into_values()).reshape(shape)
}

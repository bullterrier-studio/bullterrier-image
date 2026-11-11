//! `_core.palette`: a picture's dominant colours as hex text.

use pyo3::prelude::*;

use super::{core_error, photo_array, picture_of};

/// The `colors` dominant colours of `rgb` (a C-contiguous float32 array `(height, width, 3)`, levels 0..1) as `'#rrggbb'`
/// text, largest share of the pixels first; fewer when the picture has fewer distinct colours. The same `rgb`, `colors`
/// and `seed` give the same list. The core does the work without the GIL.
///
/// `CoreError` of kind `recipe` naming `colors` and its value unless it is from 1 to 256, or `seed` and its value unless it is
/// from 0 to 4294967295; of kind `invalid_image` for an
/// `rgb` that is no such array.
#[pyfunction]
pub(crate) fn palette(py: Python<'_>, rgb: &Bound<'_, PyAny>, colors: i64, seed: i128) -> PyResult<Vec<String>> {
    let readonly = photo_array(rgb)?;
    let picture = picture_of(&readonly)?;
    let found = py
        .detach(|| bullterrier_image::palette(&picture, colors, seed))
        .map_err(core_error)?;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a level in 0..1 times 255, rounded, is a byte"
    )]
    let byte = |level: f32| (level.clamp(0.0, 1.0) * 255.0).round() as u8; // constant: 8-bit levels
    Ok(found
        .into_iter()
        .map(|[red, green, blue]| format!("#{:02x}{:02x}{:02x}", byte(red), byte(green), byte(blue)))
        .collect())
}

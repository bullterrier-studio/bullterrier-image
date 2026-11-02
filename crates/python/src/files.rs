//! `_core.read`, `_core.read_map` and `_core.write`: image files, read and written by the core.

use std::path::PathBuf;

use bullterrier_image::{Error, Image};
use numpy::{PyArray1, PyArray2, PyArray3, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::prelude::*;

use super::{core_error, found, value_error};

/// Read the image file at `path` (a `str` or a path) as a float32 C-contiguous array `(height, width, 3)`, levels 0..1,
/// or `(height, width, 4)` with straight alpha last when the file has transparency (an alpha channel, a transparent
/// palette entry or a colour key). The picture is upright (EXIF orientation applied), its first frame if it has several,
/// and in sRGB (an embedded profile is applied). The format is found by content: PNG, JPEG, GIF, WebP, TIFF and BMP.
/// The core does the work without the GIL.
///
/// A failure is a `CoreError` of kind `image_read`, naming the path: the file is missing, not an image, damaged, in a
/// layout the core does not read (a palette TIFF) or over 178,956,970 pixels.
#[pyfunction]
#[expect(clippy::needless_pass_by_value, reason = "pyo3 extracts a path as an owned PathBuf")]
pub(crate) fn read(py: Python<'_>, path: PathBuf) -> PyResult<Bound<'_, PyArray3<f32>>> {
    let image = py.detach(|| bullterrier_image::read(&path)).map_err(core_error)?;
    let shape = [image.height(), image.width(), image.channels()];
    PyArray1::from_vec(py, image.into_values()).reshape(shape)
}

/// Read the image file at `path` as a map: a float32 `(height, width)` array, 0..1. A file with transparency gives its
/// alpha; any other gives its brightness (a 16-bit file keeps its range). Orientation, formats and errors as for
/// `read`; a TIFF of float levels, or of integers beyond 16 bits, is refused as `image_read`, naming its mode.
#[pyfunction]
#[expect(clippy::needless_pass_by_value, reason = "pyo3 extracts a path as an owned PathBuf")]
pub(crate) fn read_map(py: Python<'_>, path: PathBuf) -> PyResult<Bound<'_, PyArray2<f32>>> {
    let layer = py.detach(|| bullterrier_image::read_map(&path)).map_err(core_error)?;
    PyArray1::from_slice(py, layer.values()).reshape([layer.height(), layer.width()])
}

/// Write `image` (float32, C-contiguous, `(height, width, 3)`, or `(height, width, 4)` with straight alpha last) to
/// `path` (a `str` or a path); the extension picks the format: `.png`, `.jpg` or `.jpeg`, `.webp` (lossless). Levels are
/// rounded to 8 bits; one outside 0..1 is cut to the nearest end. `quality` (1..100) applies to JPEG only. The file
/// carries a small sRGB profile and no other metadata. The core does the work without the GIL.
///
/// A failure is a `CoreError` of kind `backend_unavailable` (another extension, or a JPEG with alpha), `image_write`
/// (the file cannot be created), `invalid_image` (an array that is no such picture) or `other` (`quality` out of range).
#[pyfunction]
#[pyo3(signature = (path, image, quality=95))]
#[expect(clippy::needless_pass_by_value, reason = "pyo3 extracts a path as an owned PathBuf")]
pub(crate) fn write(py: Python<'_>, path: PathBuf, image: &Bound<'_, PyAny>, quality: i64) -> PyResult<()> {
    let quality = u8::try_from(quality)
        .map_err(|_| core_error(Error::Inputs(format!("quality is from 1 to 100, got {quality}"))))?;
    let image = image_of(image)?;
    py.detach(|| bullterrier_image::write_with_quality(&path, &image, quality))
        .map_err(core_error)
}

/// The image the C-contiguous float32 array `array` `(height, width, 3 or 4)` holds, copied.
fn image_of(array: &Bound<'_, PyAny>) -> PyResult<Image> {
    let invalid = || {
        core_error(Error::InvalidImage(format!(
            "image must be a C-contiguous float32 array of shape (h, w, 3) or (h, w, 4), got {}",
            found(array)
        )))
    };
    let typed = array.cast::<PyArray3<f32>>().map_err(|_| invalid())?;
    let &[height, width, channels] = typed.shape() else {
        return Err(invalid());
    };
    if !typed.is_c_contiguous() || !(channels == 3 || channels == 4) {
        return Err(invalid());
    }
    let readonly = typed.try_readonly().map_err(value_error)?;
    let values = readonly.as_slice().map_err(value_error)?;
    if channels == 3 {
        Image::new(height, width, values)
    } else {
        Image::rgba(height, width, values)
    }
    .map_err(core_error)
}

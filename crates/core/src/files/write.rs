//! Writing image files: PNG, JPEG and lossless WebP, each with the sRGB profile and no other metadata.

use std::ffi::OsStr;
use std::fmt::Display;
use std::path::Path;

use image::codecs::jpeg::{JpegEncoder, PixelDensity};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageEncoder};

use crate::{Error, Image};

/// The sRGB profile every file embeds: littlecms's built-in one (588 bytes), with the creation date in its header
/// (bytes 24..36) set to 2000-01-01 00:00:00 instead of the time it was made, so a file's bytes do not depend on when it
/// was written.
pub(super) const SRGB: &[u8] = include_bytes!("../../data/srgb.icc");

/// The JPEG quality of [`write()`](crate::write), the one the Python package saved with.
const DEFAULT_QUALITY: u8 = 95; // constant: the quality `imageio.save` has always defaulted to

/// The highest resolution a file can say: a JPEG's density is 16 bits (a PNG's would hold far more, one number for both).
const HIGHEST_DPI: f64 = 65_535.0; // constant: the largest number of a JPEG's 16-bit density
/// Metres to inches: a PNG says pixels per metre.
const INCH: f64 = 0.0254; // constant: an inch is exactly 25.4 mm

/// What a write may be told besides the picture and the path: the JPEG quality and the resolution. Made with
/// [`WriteOptions::new`] and the `with_` methods; nothing in a recipe sets any of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WriteOptions {
    quality: u8,
    dpi: Option<f64>,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteOptions {
    /// JPEG quality 95 and no resolution.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            quality: DEFAULT_QUALITY,
            dpi: None,
        }
    }

    /// These options with the JPEG `quality`, from 1 (smallest file, coarsest) to 100 (largest, finest); the scaling of the
    /// quantisation tables is the reference one of libjpeg (the IJG formula). A PNG or a WebP is lossless and ignores it.
    /// Checked when the file is written.
    #[must_use]
    pub const fn with_quality(mut self, quality: u8) -> Self {
        self.quality = quality;
        self
    }

    /// These options with the resolution `dpi`, dots per inch (1 to 65535), written as the PNG `pHYs` chunk (pixels per
    /// metre, rounded) or the JPEG's JFIF density (whole dots per inch, rounded). It says how large the picture prints; no
    /// pixel changes. Checked when the file is written; a WebP holds none and refuses it.
    #[must_use]
    pub const fn with_dpi(mut self, dpi: f64) -> Self {
        self.dpi = Some(dpi);
        self
    }
}

/// The file formats the core writes, found by the extension.
#[derive(Clone, Copy, PartialEq)]
enum Format {
    Png,
    Jpeg,
    Webp,
}

/// Write `image` to the file at `path`, in the format its extension names (`.png`, `.jpg`, `.jpeg` or `.webp`, in any
/// case), replacing a file already there. A JPEG is written at quality 95 and no file says a resolution; see [`write_with`]
/// to choose.
///
/// Levels are 0..1 as 8-bit numbers, `level * 255 + 0.5` cut to a whole number, so a level read from an 8-bit file
/// is written back as it was. An image with alpha gives a file with a transparency channel (straight: the colour not
/// multiplied by it), which PNG and WebP can hold.
///
/// A colour level outside 0..1 is cut to the nearest end (the pictures the runner makes are in range already; the
/// Python package refuses such an array before it gets here).
///
/// The file embeds a small sRGB profile, so every viewer shows the colours the effects made, and carries no other
/// metadata: nothing from the photo it came from, no EXIF and no GPS position. The same image gives the same bytes
/// every time. PNG is lossless, written at the best compression with adaptive filtering; WebP is lossless; JPEG keeps full-resolution colour
/// (4:4:4, no chroma subsampling).
///
/// # Errors
/// [`Error::BackendUnavailable`], naming the extension, if it is not one of those, or for a JPEG of an image with alpha
/// (use `.png` or `.webp`). [`Error::ImageWrite`], naming the path, if the file cannot be created (a missing folder, a
/// path that is not writable) or the picture is too large for the format (a JPEG is at most 65535 pixels on a side, a
/// WebP 16384). No
/// file is created or changed for any of these but a failure of the file system itself.
pub fn write(path: &Path, image: &Image) -> Result<(), Error> {
    write_with(path, image, &WriteOptions::new())
}

/// [`write()`] with `options`: the JPEG quality and the resolution (see [`WriteOptions`]).
///
/// # Errors
/// [`Error::Inputs`] naming `quality` if it is not from 1 to 100 (whatever the format), or naming `dpi` and its value
/// unless it is a finite number from 1 to 65535 (whatever the format); [`Error::BackendUnavailable`] for a `dpi` with a WebP,
/// which holds none; those of [`write()`] otherwise.
pub fn write_with(path: &Path, image: &Image, options: &WriteOptions) -> Result<(), Error> {
    let WriteOptions { quality, dpi } = *options;
    let format = format_of(path)?;
    if !(1..=100).contains(&quality) {
        return Err(Error::Inputs(format!("quality is from 1 to 100, got {quality}")));
    }
    if let Some(dpi) = dpi {
        if !(1.0..=HIGHEST_DPI).contains(&dpi) {
            return Err(Error::Inputs(format!("dpi is from 1 to 65535, got {dpi}")));
        }
        if format == Format::Webp {
            return Err(Error::BackendUnavailable(
                "cannot write a dpi into a '.webp' picture, which holds no resolution: use .png or .jpg".to_owned(),
            ));
        }
    }
    if format == Format::Jpeg && image.channels() == 4 {
        return Err(Error::BackendUnavailable(format!(
            "cannot write '.{}' pictures with transparency: use .png or .webp",
            extension(path).unwrap_or_default()
        )));
    }
    let fail = |reason: &dyn Display| Error::ImageWrite(format!("cannot write {}: {reason}", path.display()));
    let (height, width) = (
        u32::try_from(image.height()).map_err(|error| fail(&error))?,
        u32::try_from(image.width()).map_err(|error| fail(&error))?,
    );
    let (levels, colour) = eight_bit(image);
    // The whole file is made in memory first, so a picture that cannot be encoded leaves no file.
    let mut bytes = Vec::new();
    match format {
        Format::Png => embed(
            PngEncoder::new_with_quality(&mut bytes, CompressionType::Best, FilterType::Adaptive),
            &levels,
            width,
            height,
            colour,
        )
        .map(|()| {
            if let Some(dpi) = dpi {
                physical_size(&mut bytes, dpi);
            }
        }),
        Format::Jpeg => {
            let mut encoder = JpegEncoder::new_with_quality(&mut bytes, quality);
            if let Some(dpi) = dpi {
                encoder.set_pixel_density(PixelDensity::dpi(dots(dpi)));
            }
            embed(encoder, &levels, width, height, colour)
        }
        Format::Webp => embed(WebPEncoder::new_lossless(&mut bytes), &levels, width, height, colour),
    }
    .map_err(|error| fail(&error))?;
    std::fs::write(path, bytes).map_err(|error| fail(&error))
}

/// The extension of `path` in lower case, `None` for a file that has none (a name that is only an extension, such as
/// `.png`, is a hidden file and has none).
fn extension(path: &Path) -> Option<String> {
    path.extension().and_then(OsStr::to_str).map(str::to_lowercase)
}

/// The format of the file at `path`, from its extension.
fn format_of(path: &Path) -> Result<Format, Error> {
    match extension(path).as_deref() {
        Some("png") => Ok(Format::Png),
        Some("jpg" | "jpeg") => Ok(Format::Jpeg),
        Some("webp") => Ok(Format::Webp),
        other => {
            let what = other.map_or_else(
                || "a file with no extension".to_owned(),
                |extension| format!("'.{extension}' pictures"),
            );
            Err(Error::BackendUnavailable(format!(
                "cannot write {what}: use .png, .jpg or .webp"
            )))
        }
    }
}

/// The channels of `image` as 8-bit numbers, pixel after pixel, and their colour type.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level is 0..1, so level * 255 + 0.5 is 0.5..255.5 and cutting it to a whole number is the rounding"
)]
fn eight_bit(image: &Image) -> (Vec<u8>, ExtendedColorType) {
    let level = |level: f32| (level * 255.0 + 0.5) as u8; // constant: the largest 8-bit level, and half a level to round
    let pixels = image.as_picture();
    match image.alpha() {
        None => (
            pixels.pixels().iter().flat_map(|pixel| pixel.map(level)).collect(),
            ExtendedColorType::Rgb8,
        ),
        Some(alpha) => (
            pixels
                .pixels()
                .iter()
                .zip(alpha)
                .flat_map(|(&[red, green, blue], &alpha)| [red, green, blue, alpha].map(level))
                .collect(),
            ExtendedColorType::Rgba8,
        ),
    }
}

/// Encodes `levels` with `encoder`, after giving it the sRGB profile and nothing else.
fn embed(
    mut encoder: impl ImageEncoder,
    levels: &[u8],
    width: u32,
    height: u32,
    colour: ExtendedColorType,
) -> Result<(), image::ImageError> {
    encoder
        .set_icc_profile(SRGB.to_vec())
        .map_err(image::ImageError::Unsupported)?;
    encoder.write_image(levels, width, height, colour)
}

/// `dpi` as the whole number of dots a JPEG says (the range is checked: 1 to 65535).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "dpi is from 1 to 65535, so its rounding fits 16 bits"
)]
fn dots(dpi: f64) -> u16 {
    dpi.round() as u16
}

/// The PNG `file` with a `pHYs` chunk for `dpi` (1 to 65535) right after its `IHDR`, which is before the pixels as the
/// standard asks: pixels per metre, the same in x and y, unit 1 (the metre).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "dpi is from 1 to 65535, so pixels per metre (at most 2.6 million) fit 32 bits"
)]
fn physical_size(file: &mut Vec<u8>, dpi: f64) {
    const AFTER_HEADER: usize = 33; // constant: the 8-byte signature and the 25 bytes of the IHDR chunk
    let per_metre = (dpi / INCH).round() as u32;
    let mut body = b"pHYs".to_vec();
    body.extend_from_slice(&per_metre.to_be_bytes());
    body.extend_from_slice(&per_metre.to_be_bytes());
    body.push(1); // constant: the unit specifier "metre"
    let mut chunk = 9_u32.to_be_bytes().to_vec(); // constant: the chunk's data is two 4-byte numbers and a unit byte
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    file.splice(AFTER_HEADER..AFTER_HEADER, chunk);
}

/// The CRC-32 of `bytes` as PNG defines it (ISO 3309, polynomial 0xEDB88320, bit by bit: a chunk of 13 bytes does not
/// need a table).
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                0xEDB8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

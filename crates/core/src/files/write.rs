//! Writing image files: PNG, JPEG and lossless WebP, each with the sRGB profile and no other metadata.

use std::ffi::OsStr;
use std::fmt::Display;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageEncoder};

use crate::{Error, Image};

/// The sRGB profile every file embeds: littlecms's built-in one (588 bytes), with the creation date in its header
/// (bytes 24..36) set to 2000-01-01 00:00:00 instead of the time it was made, so a file's bytes do not depend on when it
/// was written.
const SRGB: &[u8] = include_bytes!("../../data/srgb.icc");

/// The JPEG quality of [`write()`](crate::write), the one the Python package saved with.
const DEFAULT_QUALITY: u8 = 95; // constant: the quality `imageio.save` has always defaulted to

/// The file formats the core writes, found by the extension.
#[derive(Clone, Copy, PartialEq)]
enum Format {
    Png,
    Jpeg,
    Webp,
}

/// Write `image` to the file at `path`, in the format its extension names (`.png`, `.jpg`, `.jpeg` or `.webp`, in any
/// case), replacing a file already there. A JPEG is written at quality 95; see [`write_with_quality`] to choose.
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
/// every time. PNG is lossless at its default compression; WebP is lossless; JPEG keeps full-resolution colour
/// (4:4:4, no chroma subsampling).
///
/// # Errors
/// [`Error::BackendUnavailable`], naming the extension, if it is not one of those, or for a JPEG of an image with alpha
/// (use `.png` or `.webp`). [`Error::ImageWrite`], naming the path, if the file cannot be created (a missing folder, a
/// path that is not writable) or the picture is too large for the format (a JPEG is at most 65535 pixels on a side, a
/// WebP 16384). No
/// file is created or changed for any of these but a failure of the file system itself.
pub fn write(path: &Path, image: &Image) -> Result<(), Error> {
    write_with_quality(path, image, DEFAULT_QUALITY)
}

/// [`write()`] with the JPEG `quality`, from 1 (smallest file, coarsest) to 100 (largest, finest): the scaling of the
/// quantisation tables is the reference one of libjpeg (the IJG formula). A PNG or a WebP is lossless and ignores it.
///
/// # Errors
/// [`Error::Inputs`] if `quality` is not from 1 to 100, whatever the format; those of [`write()`] otherwise.
pub fn write_with_quality(path: &Path, image: &Image, quality: u8) -> Result<(), Error> {
    let format = format_of(path)?;
    if !(1..=100).contains(&quality) {
        return Err(Error::Inputs(format!("quality is from 1 to 100, got {quality}")));
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
        Format::Png => embed(PngEncoder::new(&mut bytes), &levels, width, height, colour),
        Format::Jpeg => embed(
            JpegEncoder::new_with_quality(&mut bytes, quality),
            &levels,
            width,
            height,
            colour,
        ),
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

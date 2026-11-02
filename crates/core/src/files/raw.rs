//! What the `image` crate does not hand over: a TIFF's embedded profile (it reads one only if the picture holds
//! more bytes than the profile has entries of a few dozen bytes each) and the inks of a CMYK file (it mixes them to
//! RGB as if no profile were there).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use image::metadata::Orientation;
use image::{DynamicImage, ExtendedColorType, ImageBuffer, ImageFormat};
use tiff::ColorType;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::tags::Tag;
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use super::icc::Profile;
use super::{Decoded, MAX_ALLOC, within_limit};

/// The profile embedded in the TIFF at `path`, if it has a readable one.
pub(super) fn tiff_profile(path: &Path) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    Decoder::new(BufReader::new(file))
        .ok()?
        .get_tag_u8_vec(Tag::IccProfile)
        .ok()
}

/// Whether the file at `path`, of `format` and (as the decoder says) `original` colour type, has CMYK samples. The
/// `image` crate says so for a TIFF but not for a JPEG, which is asked of its header.
pub(super) fn is_cmyk(path: &Path, format: Option<ImageFormat>, original: ExtendedColorType) -> bool {
    match format {
        Some(ImageFormat::Tiff) => matches!(original, ExtendedColorType::Cmyk8 | ExtendedColorType::Cmyk16),
        Some(ImageFormat::Jpeg) => std::fs::read(path).is_ok_and(|bytes| {
            let mut decoder = jpeg_decoder(&bytes);
            decoder.decode_headers().is_ok()
                && matches!(decoder.input_colorspace(), Some(ColorSpace::CMYK | ColorSpace::YCCK))
        }),
        _ => false,
    }
}

/// Whether the JPEG `bytes` reach their end-of-image marker. The decoder fills what a cut-short file misses with grey
/// instead of refusing it, so the file is walked first: segment by segment (an inner JPEG, such as an EXIF thumbnail,
/// sits inside a segment and is skipped with it), and through the entropy-coded data of each scan to the next marker.
/// Bytes after the end-of-image marker do not matter.
pub(super) fn jpeg_is_complete(bytes: &[u8]) -> bool {
    const FILL: u8 = 0xFF; // constant: JPEG (T.81 B.1.1.2) starts every marker with it, and pads with more of it
    let mut at = 0;
    while let Some(offset) = bytes[at..]
        .windows(2)
        .position(|pair| pair[0] == FILL && pair[1] != 0 && pair[1] != FILL)
    {
        let (marker, start) = (bytes[at + offset + 1], at + offset + 2);
        at = match marker {
            0xD9 => return true,                // constant: EOI, end of image
            0xD8 | 0x01 | 0xD0..=0xD7 => start, // constant: SOI, TEM and RSTn: markers without a length
            _ => match bytes.get(start..start + 2) {
                Some(length) => start + usize::from(u16::from_be_bytes([length[0], length[1]])),
                None => return false,
            },
        };
        if at > bytes.len() {
            return false;
        }
    }
    false
}

/// The CMYK file at `path` (a TIFF of 8-bit inks, or a JPEG of any CMYK kind), upright, in sRGB: its inks as stored,
/// converted by `profile`. None for any other file.
pub(super) fn cmyk_decoded(
    path: &Path,
    format: Option<ImageFormat>,
    orientation: Orientation,
    profile: &Profile,
) -> Result<Option<Decoded>, String> {
    let inks = match format {
        Some(ImageFormat::Tiff) => tiff_inks(path)?,
        Some(ImageFormat::Jpeg) => jpeg_inks(path)?,
        _ => None,
    };
    let Some((width, height, inks)) = inks else {
        return Ok(None);
    };
    // The four inks stand in for the channels of an RGBA picture while they are turned upright, so the colour is made
    // once, from the inks, and no picture of floats is made on the way.
    let mut stored = DynamicImage::ImageRgba8(
        ImageBuffer::from_raw(width, height, inks).ok_or("its data is shorter than its size")?,
    );
    stored.apply_orientation(orientation);
    let colour = profile
        .convert_inks(stored.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(Some(Decoded {
        height: stored.height() as usize,
        width: stored.width() as usize,
        colour,
        alpha: None,
        grey: false,
        wide: None,
    }))
}

/// Width, height and the inks (1 = full ink, four per pixel) of the CMYK TIFF at `path`, if that is what it is.
fn tiff_inks(path: &Path) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let size = usize::try_from(MAX_ALLOC).unwrap_or(usize::MAX);
    let mut limits = Limits::default();
    limits.decoding_buffer_size = size;
    limits.intermediate_buffer_size = size;
    let mut decoder = Decoder::new(BufReader::new(file))
        .map_err(|error| error.to_string())?
        .with_limits(limits);
    let planar = decoder
        .find_tag_unsigned::<u16>(Tag::PlanarConfiguration)
        .map_err(|error| error.to_string())?;
    if decoder.colortype().map_err(|error| error.to_string())? != ColorType::CMYK(8) || planar == Some(2) {
        return Ok(None);
    }
    let (width, height) = decoder.dimensions().map_err(|error| error.to_string())?;
    within_limit(width, height)?;
    match decoder.read_image().map_err(|error| error.to_string())? {
        DecodingResult::U8(inks) => Ok(Some((width, height, inks))),
        _ => Ok(None),
    }
}

/// A decoder of the JPEG `bytes`, with the limits the `image` crate sets: none, since the core limits pixels itself.
fn jpeg_decoder(bytes: &[u8]) -> JpegDecoder<ZCursor<&[u8]>> {
    let options = DecoderOptions::default()
        .set_strict_mode(false)
        .set_max_width(usize::MAX)
        .set_max_height(usize::MAX);
    JpegDecoder::new_with_options(ZCursor::new(bytes), options)
}

/// Width, height and the inks (1 = full ink, four per pixel) of the CMYK or YCCK JPEG at `path`, if that is what it is.
/// A JPEG of four components stores its inks inverted (Adobe's convention, which every reader takes for all of them).
fn jpeg_inks(path: &Path) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let mut decoder = jpeg_decoder(&bytes);
    decoder.decode_headers().map_err(|error| format!("{error:?}"))?;
    let Some(stored @ (ColorSpace::CMYK | ColorSpace::YCCK)) = decoder.input_colorspace() else {
        return Ok(None);
    };
    let (width, height) = decoder.dimensions().ok_or("it has no size")?;
    let (width, height) = (
        u32::try_from(width).map_err(|error| error.to_string())?,
        u32::try_from(height).map_err(|error| error.to_string())?,
    );
    within_limit(width, height)?;
    decoder.set_options(decoder.options().jpeg_set_out_colorspace(stored)); // the components as stored
    let mut inks = decoder.decode().map_err(|error| format!("{error:?}"))?;
    for pixel in inks.as_chunks_mut::<4>().0 {
        if stored == ColorSpace::YCCK {
            ycck_to_cmyk(pixel);
        }
        for level in pixel {
            *level = 255 - *level;
        }
    }
    Ok(Some((width, height, inks)))
}

/// The components `pixel` (luma, blue and red differences, black) of a YCCK JPEG as the cyan, magenta, yellow and black
/// of a CMYK one, which are 255 minus the RGB the first three make; the black is as it is.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level is rounded and clipped to 0..=255 first"
)]
fn ycck_to_cmyk(pixel: &mut [u8]) {
    let luma = f32::from(pixel[0]);
    let (blue, red) = (f32::from(pixel[1]) - 128.0, f32::from(pixel[2]) - 128.0);
    // constant: the JFIF (ITU-T T.871) matrix from YCbCr to RGB
    let rgb = [
        luma + 1.402 * red,
        luma - 0.344_136 * blue - 0.714_136 * red,
        luma + 1.772 * blue,
    ];
    for (level, light) in pixel.iter_mut().zip(rgb) {
        *level = 255 - light.round().clamp(0.0, 255.0) as u8;
    }
}

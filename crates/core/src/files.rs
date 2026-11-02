//! Reading and writing image files.

use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use image::metadata::Orientation;
use image::{DynamicImage, ImageBuffer, ImageDecoder, ImageFormat, ImageReader};
use tiff::ColorType;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::tags::Tag;

use crate::{Error, Image, Layer};

use icc::{Model, Profile};

mod icc;
mod raw;
mod write;

pub use write::{write, write_with_quality};

/// The most pixels (width times height) a file may hold to be read: twice Pillow's `MAX_IMAGE_PIXELS` of 89,478,485,
/// its decompression-bomb limit, so what the Python package read stays readable. Checked from the file's header,
/// before any pixel is decoded or allocated.
pub(crate) const MAX_PIXELS: u64 = 2 * 89_478_485; // constant: Pillow's decompression-bomb limit

/// The most bytes a decoder may allocate at once while reading a file: [`MAX_PIXELS`] pixels of the widest kind it reads,
/// RGBA of 16 bits (8 bytes). It bounds what a file's header does not say, as the limit on pixels bounds what it does.
const MAX_ALLOC: u64 = MAX_PIXELS * 8; // constant: 4 channels of 2 bytes

/// The channels of a decoded file, as the decoder lays them out.
#[derive(Clone, Copy)]
enum Layout {
    Grey,
    GreyAlpha,
    /// Three equal channels, a grey file widened (a TIFF of 32-bit levels).
    GreyRgb,
    Rgb,
    Rgba,
}

/// The levels a file holds beyond what 0..1 can say, as a TIFF of 32-bit levels does.
enum Wide {
    /// Floating-point levels (Pillow's mode `F`), clipped to 0..1.
    Float,
    /// 32-bit integer levels (Pillow's mode `I`), over 65535 and clipped to 0..1; the range the file held.
    Integer { lowest: i64, highest: i64 },
}

/// A file's first frame, upright, as 0..1 levels.
struct Decoded {
    height: usize,
    width: usize,
    colour: Vec<[f32; 3]>,
    alpha: Option<Vec<f32>>,
    /// The file has one grey channel: its brightness is that level itself, not a weighting of three equal ones.
    grey: bool,
    wide: Option<Wide>,
}

/// Read the image file at `path`: RGB, or RGBA with straight alpha (the colour not multiplied by it) when the file has
/// an alpha channel, a transparent palette entry or a colour key (a GIF always counts: its format may make an index
/// transparent). Levels are `f32` in 0..1, the file's own numbers over its maximum: an 8-bit level over 255, a 16-bit
/// one over 65535; a grey file has equal red, green and blue; a 1-bit file is 0 or 1. The format is found by content,
/// not by the extension: PNG, JPEG (greyscale, colour and CMYK or YCCK, which are mixed to RGB if the file has no
/// usable profile), GIF, WebP, TIFF and BMP. A TIFF of 32-bit integer levels (Pillow's mode `I`) is read as 16-bit grey
/// is, the level over 65535, and one of floating-point levels (mode `F`) as its level; both are clipped to 0..1.
/// A palette TIFF (one with a `ColorMap`) is not read; palette PNG, GIF and BMP are.
///
/// The picture is upright: an EXIF orientation is applied. A file of several frames (an animated GIF or PNG, a
/// multi-page TIFF) gives its first.
///
/// An embedded ICC profile is converted to sRGB, from the colour model of the file itself: the grey levels of a grey
/// file, the red, green and blue of a colour one, the inks of a CMYK TIFF or JPEG (the inks as
/// stored, not the RGB a decoder mixes from them). Alpha is not touched. A profile that cannot be read, or
/// that is for another colour space than the file's, is ignored and the levels are taken as sRGB, as viewers do; so are
/// those of 16-bit grey and of floating-point TIFFs, and the inks of a 16-bit CMYK TIFF; so is the profile [`write()`]
/// embeds, which is sRGB, so a file the core wrote reads back as written. The same file gives the same
/// image every time.
///
/// # Errors
/// [`Error::ImageRead`], naming the path and the reason, if the file is missing, a folder, not an image, damaged or
/// truncated, in a layout the core does not read (a palette TIFF, a floating-point TIFF holding NaN), or has more
/// pixels than 178,956,970 (twice Pillow's `MAX_IMAGE_PIXELS` of 89,478,485), which is checked from the header before
/// anything is decoded.
pub fn read(path: &Path) -> Result<Image, Error> {
    let Decoded {
        height,
        width,
        colour,
        alpha,
        ..
    } = decode(path, true)?;
    let image = Image::from_pixels(height, width, colour);
    Ok(match alpha {
        Some(alpha) => image.with_alpha(alpha),
        None => image,
    })
}

/// Read the image file at `path` as a map of a photo someone already has (its subject, its depth, a drawing's
/// silhouette): one value from 0 to 1 per pixel.
///
/// A file with transparency (some pixel not fully opaque) gives its alpha. Any other gives its brightness, so a
/// black-and-white mask or a greyscale depth map (white = subject, white = nearest) works as it is, and so does one
/// saved with an opaque alpha channel. An embedded colour profile is not applied: a map is data, so its levels are the
/// file's own and the same map saved as 8-bit or 16-bit, tagged or not, reads the same. Brightness is the grey level
/// itself for a grey file, else `(299 red + 587 green + 114 blue) / 1000` (ITU-R BT.601 luma). A 16-bit file keeps its
/// range: the level over 65535.
///
/// Orientation, the first-frame rule, the formats and the limit on pixels are as in [`read`], so the map lines up with
/// the photo it was made for. Levels with no faithful 0..1 are refused instead of clipped: a TIFF of floating-point
/// levels (mode `F`), and one of 32-bit integers (mode `I`) holding a level outside 0..=65535.
///
/// # Errors
/// [`Error::ImageRead`] as for [`read`]; for those two TIFFs the message names the mode.
pub fn read_map(path: &Path) -> Result<Layer, Error> {
    let Decoded {
        height,
        width,
        colour,
        alpha,
        grey,
        wide,
    } = decode(path, false)?;
    match wide {
        Some(Wide::Float) => {
            return Err(unreadable(
                path,
                &"its mode 'F' (float levels) has no white: save the map as 8- or 16-bit greyscale",
            ));
        }
        Some(Wide::Integer { lowest, highest }) if lowest < 0 || highest > 65535 => {
            return Err(unreadable(
                path,
                &format!(
                    "its mode 'I' holds levels {lowest}..{highest}, outside 16 bits (0..65535): save the map as 8- or 16-bit greyscale"
                ),
            ));
        }
        _ => {}
    }
    let values = match alpha.filter(|alpha| alpha.iter().any(|&level| level < 1.0)) {
        Some(alpha) => alpha,
        None if grey => colour.iter().map(|&[level, ..]| level).collect(),
        None => colour
            .iter()
            .map(|&[red, green, blue]| (299.0 * red + 587.0 * green + 114.0 * blue) / 1000.0) // constant: BT.601 luma
            .collect(),
    };
    Layer::new(height, width, values)
}

/// The refusal of the file at `path` for `reason`.
fn unreadable(path: &Path, reason: &dyn Display) -> Error {
    Error::ImageRead(format!("cannot read {} as an image: {reason}", path.display()))
}

/// Refuses a size of more than [`MAX_PIXELS`] pixels, from the header alone.
fn within_limit(width: u32, height: u32) -> Result<(), String> {
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(format!(
            "it is {width} x {height} pixels, more than the {MAX_PIXELS} the core reads"
        ));
    }
    Ok(())
}

/// The first frame of the file at `path`, upright; its embedded profile is applied if `colour_managed` (a picture's
/// colours) and left alone otherwise (a map's levels are data).
fn decode(path: &Path, colour_managed: bool) -> Result<Decoded, Error> {
    let refuse = |reason: &dyn Display| unreadable(path, reason);
    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| refuse(&error))?;
    // The decoders' own limits stay off but one: the memory a decoder may ask for, since a file can claim a size in its
    // header and then decode to another (a GIF frame larger than its screen, a profile that inflates to gigabytes).
    let mut limits = image::Limits::no_limits();
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let format = reader.format();
    // The JPEG decoder fills in what a file cut short misses, so such a file is refused here.
    if format == Some(ImageFormat::Jpeg)
        && !std::fs::read(path)
            .map_err(|error| refuse(&error))
            .map(|bytes| raw::jpeg_is_complete(&bytes))?
    {
        return Err(refuse(&"the file ends before the picture does: it is cut short"));
    }
    let mut embedded = None;
    let (mut picture, orientation, wide) = if let Some((picture, orientation, wide)) = wide_tiff(path, format)? {
        (picture, orientation, Some(wide))
    } else {
        let mut decoder = reader.into_decoder().map_err(|error| refuse(&error))?;
        let (width, height) = decoder.dimensions();
        within_limit(width, height).map_err(|reason| refuse(&reason))?;
        // A profile the file cannot give is no profile: the colours are then taken as sRGB.
        embedded = if !colour_managed {
            None
        } else if format == Some(ImageFormat::Tiff) {
            raw::tiff_profile(path)
        } else {
            decoder.icc_profile().ok().flatten()
        };
        let orientation = decoder.orientation().map_err(|error| refuse(&error))?;
        // The image crate mixes the inks of a CMYK file to RGB, so its profile is applied to the inks or to nothing.
        if embedded.is_some()
            && raw::is_cmyk(path, format, decoder.original_color_type())
            && let Some(profile) = embedded.take().and_then(|bytes| Profile::new(&bytes, Model::Cmyk))
            && let Some(decoded) =
                raw::cmyk_decoded(path, format, orientation, &profile).map_err(|reason| refuse(&reason))?
        {
            return Ok(decoded);
        }
        let picture = DynamicImage::from_decoder(decoder).map_err(|error| refuse(&error))?;
        (picture, orientation, None)
    };
    picture.apply_orientation(orientation);
    let (height, width) = (picture.height() as usize, picture.width() as usize);
    let (mut colour, alpha, grey) = match &picture {
        DynamicImage::ImageLuma8(raw) => levels(raw.as_raw(), 255.0, Layout::Grey),
        DynamicImage::ImageLumaA8(raw) => levels(raw.as_raw(), 255.0, Layout::GreyAlpha),
        DynamicImage::ImageRgb8(raw) => levels(raw.as_raw(), 255.0, Layout::Rgb),
        DynamicImage::ImageRgba8(raw) => levels(raw.as_raw(), 255.0, Layout::Rgba),
        DynamicImage::ImageLuma16(raw) => levels(raw.as_raw(), 65535.0, Layout::Grey),
        DynamicImage::ImageLumaA16(raw) => levels(raw.as_raw(), 65535.0, Layout::GreyAlpha),
        DynamicImage::ImageRgb16(raw) => levels(raw.as_raw(), 65535.0, Layout::Rgb),
        DynamicImage::ImageRgba16(raw) => levels(raw.as_raw(), 65535.0, Layout::Rgba),
        DynamicImage::ImageRgb32F(raw) if wide.is_some() => levels(raw.as_raw(), 1.0, Layout::GreyRgb),
        DynamicImage::ImageRgb32F(raw) => levels(raw.as_raw(), 1.0, Layout::Rgb),
        other => {
            return Err(refuse(&format!(
                "its pixel layout {:?} is not one the core reads",
                other.color()
            )));
        }
    };
    // 16-bit grey and float levels are taken as sRGB: no profile is applied to them.
    let model = match &picture {
        DynamicImage::ImageLuma8(_) | DynamicImage::ImageLumaA8(_) => Some(Model::Grey),
        DynamicImage::ImageRgb8(_)
        | DynamicImage::ImageRgba8(_)
        | DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageRgba16(_) => Some(Model::Rgb),
        _ => None,
    };
    if let (Some(model), Some(bytes)) = (model, embedded)
        && let Some(profile) = Profile::new(&bytes, model)
    {
        profile.convert(&mut colour).map_err(|error| refuse(&error))?;
    }
    Ok(Decoded {
        height,
        width,
        colour,
        alpha,
        grey,
        wide,
    })
}

/// A TIFF of one 32-bit sample per pixel (integers or floats), which the `image` crate does not decode: its levels
/// as a three-channel float image of equal channels, its orientation and what it held. None for any other file.
fn wide_tiff(path: &Path, format: Option<ImageFormat>) -> Result<Option<(DynamicImage, Orientation, Wide)>, Error> {
    if format != Some(ImageFormat::Tiff) {
        return Ok(None);
    }
    let refuse = |reason: &dyn Display| unreadable(path, reason);
    let file = File::open(path).map_err(|error| refuse(&error))?;
    let mut decoder = Decoder::new(BufReader::new(file))
        .map_err(|error| refuse(&error))?
        .with_limits(Limits::unlimited()); // the pixel limit below is the core's
    if decoder.colortype().map_err(|error| refuse(&error))? != ColorType::Gray(32) {
        return Ok(None);
    }
    let (width, height) = decoder.dimensions().map_err(|error| refuse(&error))?;
    within_limit(width, height).map_err(|reason| refuse(&reason))?;
    let orientation = decoder
        .find_tag_unsigned::<u64>(Tag::Orientation)
        .map_err(|error| refuse(&error))?
        .and_then(|value| Orientation::from_exif(u8::try_from(value).ok()?))
        .unwrap_or(Orientation::NoTransforms);
    let (levels, wide) = match decoder.read_image().map_err(|error| refuse(&error))? {
        DecodingResult::F32(values) if values.iter().any(|value| value.is_nan()) => {
            return Err(refuse(&"it holds levels that are not numbers (NaN)"));
        }
        DecodingResult::F32(values) => (values.iter().map(|level| level.clamp(0.0, 1.0)).collect(), Wide::Float),
        DecodingResult::I32(values) => integer_levels(values.iter().map(|&value| i64::from(value))),
        DecodingResult::U32(values) => integer_levels(values.iter().map(|&value| i64::from(value))),
        _ => return Err(refuse(&"its levels are neither integers nor floats")),
    };
    let widened = levels.iter().flat_map(|&level| [level; 3]).collect();
    let buffer =
        ImageBuffer::from_raw(width, height, widened).ok_or_else(|| refuse(&"its data is shorter than its size"))?;
    Ok(Some((DynamicImage::ImageRgb32F(buffer), orientation, wide)))
}

/// Integer `values` as levels over 65535 clipped to 0..1, with the range they held.
#[expect(
    clippy::cast_precision_loss,
    reason = "a level is rounded to f32, as Pillow's float32 array does"
)]
fn integer_levels(values: impl Iterator<Item = i64> + Clone) -> (Vec<f32>, Wide) {
    let lowest = values.clone().min().unwrap_or(0);
    let highest = values.clone().max().unwrap_or(0);
    let levels = values.map(|value| (value as f32 / 65535.0).clamp(0.0, 1.0)).collect();
    (levels, Wide::Integer { lowest, highest })
}

/// The channels `raw` as 0..1 levels (each over `white`, the largest the file's depth holds): the colour of every
/// pixel, its alpha if the layout has one, and whether the file is grey.
fn levels<T: Copy + Into<f32>>(raw: &[T], white: f32, layout: Layout) -> (Vec<[f32; 3]>, Option<Vec<f32>>, bool) {
    let level = |value: T| value.into() / white;
    let grey = matches!(layout, Layout::Grey | Layout::GreyAlpha | Layout::GreyRgb);
    let (colour, alpha) = match layout {
        Layout::Grey => (raw.iter().map(|&grey| [level(grey); 3]).collect(), None),
        Layout::GreyAlpha => {
            let (pairs, _) = raw.as_chunks::<2>();
            (
                pairs.iter().map(|&[grey, _]| [level(grey); 3]).collect(),
                Some(pairs.iter().map(|&[_, alpha]| level(alpha)).collect()),
            )
        }
        Layout::Rgb | Layout::GreyRgb => {
            let (triples, _) = raw.as_chunks::<3>();
            (
                triples
                    .iter()
                    .map(|&[red, green, blue]| [level(red), level(green), level(blue)])
                    .collect(),
                None,
            )
        }
        Layout::Rgba => {
            let (quads, _) = raw.as_chunks::<4>();
            (
                quads
                    .iter()
                    .map(|&[red, green, blue, _]| [level(red), level(green), level(blue)])
                    .collect(),
                Some(quads.iter().map(|&[_, _, _, alpha]| level(alpha)).collect()),
            )
        }
    };
    (colour, alpha, grey)
}

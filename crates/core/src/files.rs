//! Reading image files.

use std::fmt::Display;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader};

use crate::{Error, Image, Layer};

/// The most pixels (width times height) a file may hold to be read: twice Pillow's `MAX_IMAGE_PIXELS` of 89,478,485,
/// its decompression-bomb limit, so what the Python package read stays readable. Checked from the file's header,
/// before any pixel is decoded or allocated.
pub(crate) const MAX_PIXELS: u64 = 2 * 89_478_485; // constant: Pillow's decompression-bomb limit

/// The channels of a decoded file, as the decoder lays them out.
#[derive(Clone, Copy)]
enum Layout {
    Grey,
    GreyAlpha,
    Rgb,
    Rgba,
}

/// A file's first frame, upright, as 0..1 levels.
struct Decoded {
    height: usize,
    width: usize,
    colour: Vec<[f32; 3]>,
    alpha: Option<Vec<f32>>,
    /// The file has one grey channel: its brightness is that level itself, not a weighting of three equal ones.
    grey: bool,
}

/// Read the image file at `path`: RGB, or RGBA with straight alpha (the colour not multiplied by it) when the file has
/// an alpha channel, a transparent palette entry or a colour key (a GIF always counts: its format may make an index
/// transparent). Levels are `f32` in 0..1, the file's own numbers over its maximum: an 8-bit level over 255, a 16-bit
/// one over 65535; a grey file has equal red, green and blue; a 1-bit file is 0 or 1. The format is found by content,
/// not by the extension: PNG, JPEG (greyscale, colour and CMYK or YCCK, which are mixed to RGB without a profile),
/// GIF, WebP, TIFF (8- and 16-bit) and BMP.
///
/// The picture is upright: an EXIF orientation is applied. A file of several frames (an animated GIF or PNG, a
/// multi-page TIFF) gives its first. Levels are taken as sRGB; an embedded colour profile is not applied yet. The same
/// file gives the same image every time.
///
/// # Errors
/// [`Error::ImageRead`], naming the path and the reason, if the file is missing, a folder, not an image, damaged or
/// truncated, in a layout the core does not read (a TIFF of 32-bit integer or floating-point levels), or has more
/// pixels than 178,956,970 (twice Pillow's `MAX_IMAGE_PIXELS` of 89,478,485), which is checked from the header before
/// anything is decoded.
pub fn read(path: &Path) -> Result<Image, Error> {
    let Decoded {
        height,
        width,
        colour,
        alpha,
        ..
    } = decode(path)?;
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
/// saved with an opaque alpha channel. Brightness is the grey level itself for a grey file, else
/// `(299 red + 587 green + 114 blue) / 1000` (ITU-R BT.601 luma). A 16-bit file keeps its range: the level over 65535.
///
/// Orientation, the first-frame rule, the formats and the limit on pixels are as in [`read`], so the map lines up with
/// the photo it was made for. Levels outside 0..1 do not exist in what the core decodes: a TIFF of floating-point or
/// 32-bit integer levels is refused as in [`read`].
///
/// # Errors
/// [`Error::ImageRead`] as for [`read`].
pub fn read_map(path: &Path) -> Result<Layer, Error> {
    let Decoded {
        height,
        width,
        colour,
        alpha,
        grey,
    } = decode(path)?;
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

/// The first frame of the file at `path`, upright.
fn decode(path: &Path) -> Result<Decoded, Error> {
    let refuse =
        |reason: &dyn Display| Error::ImageRead(format!("cannot read {} as an image: {reason}", path.display()));
    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| refuse(&error))?;
    reader.no_limits(); // the pixel limit below is the core's, and the first to apply
    let mut decoder = reader.into_decoder().map_err(|error| refuse(&error))?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(refuse(&format!(
            "it is {width} x {height} pixels, more than the {MAX_PIXELS} the core reads"
        )));
    }
    // Colour management hook: `decoder.icc_profile()` is the embedded profile, to convert from before the levels are used.
    let orientation = decoder.orientation().map_err(|error| refuse(&error))?;
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|error| refuse(&error))?;
    picture.apply_orientation(orientation);
    let (height, width) = (picture.height() as usize, picture.width() as usize);
    let (colour, alpha, grey) = match &picture {
        DynamicImage::ImageLuma8(raw) => levels(raw.as_raw(), 255.0, Layout::Grey),
        DynamicImage::ImageLumaA8(raw) => levels(raw.as_raw(), 255.0, Layout::GreyAlpha),
        DynamicImage::ImageRgb8(raw) => levels(raw.as_raw(), 255.0, Layout::Rgb),
        DynamicImage::ImageRgba8(raw) => levels(raw.as_raw(), 255.0, Layout::Rgba),
        DynamicImage::ImageLuma16(raw) => levels(raw.as_raw(), 65535.0, Layout::Grey),
        DynamicImage::ImageLumaA16(raw) => levels(raw.as_raw(), 65535.0, Layout::GreyAlpha),
        DynamicImage::ImageRgb16(raw) => levels(raw.as_raw(), 65535.0, Layout::Rgb),
        DynamicImage::ImageRgba16(raw) => levels(raw.as_raw(), 65535.0, Layout::Rgba),
        other => {
            return Err(refuse(&format!(
                "its pixel layout {:?} is not one the core reads",
                other.color()
            )));
        }
    };
    Ok(Decoded {
        height,
        width,
        colour,
        alpha,
        grey,
    })
}

/// The channels `raw` as 0..1 levels (each over `white`, the largest the file's depth holds): the colour of every
/// pixel, its alpha if the layout has one, and whether the file is grey.
fn levels<T: Copy + Into<f32>>(raw: &[T], white: f32, layout: Layout) -> (Vec<[f32; 3]>, Option<Vec<f32>>, bool) {
    let level = |value: T| value.into() / white;
    match layout {
        Layout::Grey => (raw.iter().map(|&grey| [level(grey); 3]).collect(), None, true),
        Layout::GreyAlpha => {
            let (pairs, _) = raw.as_chunks::<2>();
            (
                pairs.iter().map(|&[grey, _]| [level(grey); 3]).collect(),
                Some(pairs.iter().map(|&[_, alpha]| level(alpha)).collect()),
                true,
            )
        }
        Layout::Rgb => {
            let (triples, _) = raw.as_chunks::<3>();
            (
                triples
                    .iter()
                    .map(|&[red, green, blue]| [level(red), level(green), level(blue)])
                    .collect(),
                None,
                false,
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
                false,
            )
        }
    }
}

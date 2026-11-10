//! Apple Vision's subject segmentation (the "lift subject" feature), called from Rust as pyobjc called it. It runs on the
//! Neural Engine and needs no download. Only macOS 14 and later have it.

#![expect(
    unsafe_code,
    reason = "Apple Vision and CoreVideo are Objective-C and C APIs: objc2 marks each call unsafe, and each is made with the arguments the documentation shows"
)]

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    kCVPixelFormatType_OneComponent32Float,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSProcessInfo};
use objc2_vision::{VNGenerateForegroundInstanceMaskRequest, VNImageRequestHandler};

use bullterrier_image::finding::subject_matte;
use bullterrier_image::{Error, Finder, Layer, Picture};

/// The first macOS with the foreground instance mask request.
const FIRST_MACOS: isize = 14; // constant: VNGenerateForegroundInstanceMaskRequest is new in macOS 14

/// The subject finder of Apple Vision: the people, pets and objects that stand out.
#[derive(Clone, Copy, Debug)]
pub struct Vision {
    shrink: f64,
    feather: f64,
}

/// Whether this macOS has the request (14 or later).
pub(crate) fn supported() -> bool {
    NSProcessInfo::processInfo().operatingSystemVersion().majorVersion >= FIRST_MACOS
}

impl Vision {
    /// The Vision finder.
    ///
    /// # Errors
    /// [`Error::BackendUnavailable`] naming the fix if this macOS is before 14, which has no such request.
    pub fn new() -> Result<Self, Error> {
        require(NSProcessInfo::processInfo().operatingSystemVersion().majorVersion)?;
        Ok(Self {
            shrink: 1.5,
            feather: 1.0,
        })
    }

    /// This finder with the edge clean-up set to `shrink` and `feather` px (the defaults are 1.5 and 1).
    #[must_use]
    pub const fn with_edge(mut self, shrink: f64, feather: f64) -> Self {
        self.shrink = shrink;
        self.feather = feather;
        self
    }
}

/// `Ok` for a macOS of major version `major` that has the request.
fn require(major: isize) -> Result<(), Error> {
    if major >= FIRST_MACOS {
        return Ok(());
    }
    Err(Error::BackendUnavailable(format!(
        "Apple Vision's subject segmentation needs macOS {FIRST_MACOS} or later (this is macOS {major}): choose the \\
         portable model instead (segmenter 'onnx', or BT_IMAGE_SEGMENTER=onnx), or give the subject map yourself \\
         (--map subject=FILE)"
    )))
}

impl Finder for Vision {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
        let (height, width) = (picture.height(), picture.width());
        let (mask, from) = foreground_mask(picture)?;
        subject_matte(&mask, from, (height, width), (self.shrink, self.feather))
    }
}

/// `picture` as the bytes of a PNG file, 8 bits a channel.
fn png(picture: &Picture<'_>) -> Result<Vec<u8>, Error> {
    let eight_bit: Vec<u8> = picture
        .values()
        .iter()
        .map(|&value| {
            // rounded to the nearest level: the value is within 0..1, so the cast cannot overflow
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "0..=255 by the clamp"
            )]
            let level = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            level
        })
        .collect();
    let mut file = Vec::new();
    PngEncoder::new(&mut file)
        .write_image(
            &eight_bit,
            u32::try_from(picture.width()).unwrap_or(u32::MAX),
            u32::try_from(picture.height()).unwrap_or(u32::MAX),
            ExtendedColorType::Rgb8,
        )
        .map_err(|error| Error::BackendUnavailable(format!("cannot hand the picture to Apple Vision: {error}")))?;
    Ok(file)
}

/// Vision's mask of every foreground subject of `picture` (0..1, row after row) and its size (rows, columns), which is
/// Vision's own and not the picture's.
fn foreground_mask(picture: &Picture<'_>) -> Result<(Vec<f32>, (usize, usize)), Error> {
    let unavailable =
        |what: &str| Error::BackendUnavailable(format!("Apple Vision could not segment the picture: {what}"));
    let data = NSData::with_bytes(&png(picture)?);
    let handler =
        VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &NSDictionary::new());
    // SAFETY: a new request of a class that exists on macOS 14 and later, which `Vision::new` checked
    let request = unsafe { VNGenerateForegroundInstanceMaskRequest::new() };
    let requests = NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(request.clone()))]);
    handler
        .performRequests_error(&requests)
        .map_err(|error| unavailable(&error.localizedDescription().to_string()))?;
    // SAFETY: results of a request that has been performed
    let Some(observation) = unsafe { request.results() }.and_then(|results| results.firstObject()) else {
        return Err(Error::map_not_found(
            "subject",
            "no subject found in the picture (no person, pet or object stood out)",
        ));
    };
    // SAFETY: the observation's instances, scaled to the image of the handler that made it
    let buffer = unsafe {
        observation
            .generateScaledMaskForImageForInstances_fromRequestHandler_error(&observation.allInstances(), &handler)
    }
    .map_err(|error| unavailable(&error.localizedDescription().to_string()))?;
    // SAFETY: the buffer is locked for reading while its rows are copied, and unlocked after
    unsafe {
        CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::ReadOnly);
        let (width, height, stride) = (
            CVPixelBufferGetWidth(&buffer),
            CVPixelBufferGetHeight(&buffer),
            CVPixelBufferGetBytesPerRow(&buffer),
        );
        let floats = CVPixelBufferGetPixelFormatType(&buffer) == kCVPixelFormatType_OneComponent32Float;
        let base = CVPixelBufferGetBaseAddress(&buffer).cast::<u8>().cast_const();
        let rows = std::slice::from_raw_parts(base, stride * height);
        let mut mask = Vec::with_capacity(width * height);
        for row in 0..height {
            for column in 0..width {
                mask.push(if floats {
                    let at = row * stride + 4 * column;
                    let level = f32::from_le_bytes([rows[at], rows[at + 1], rows[at + 2], rows[at + 3]]);
                    // Vision's mask is kept at 8 bits, as the Python finder did
                    (level.clamp(0.0, 1.0) * 255.0 + 0.5).floor() / 255.0
                } else {
                    f32::from(rows[row * stride + column]) / 255.0
                });
            }
        }
        CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags::ReadOnly);
        Ok((mask, (height, width)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_macos_before_14_is_refused_naming_the_fix_and_14_and_later_are_not() {
        assert!(require(14).is_ok() && require(26).is_ok());
        let refused = require(13);
        assert!(
            matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("macOS 14") && message.contains("BT_IMAGE_SEGMENTER=onnx") && message.contains("macOS 13")),
            "{refused:?}"
        );
    }
}

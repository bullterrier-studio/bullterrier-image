//! Converting the colours of a file from its embedded ICC profile to sRGB.

use std::sync::Arc;

use moxcms::{CmsError, ColorProfile, DataColorSpace, Layout, TransformExecutor, TransformOptions};

use super::write::SRGB;

/// Pixels converted at a time, so the working buffers stay small whatever the picture's size.
const CHUNK: usize = 1 << 16; // constant: 65536 pixels, 768 KiB of output

/// The colour model a file's samples are in.
#[derive(Clone, Copy)]
pub(super) enum Model {
    /// One level per pixel.
    Grey,
    /// Red, green and blue.
    Rgb,
    /// Cyan, magenta, yellow and black inks, 1 = full ink.
    Cmyk,
}

impl Model {
    /// The samples of one pixel.
    fn samples(self) -> usize {
        match self {
            Self::Grey => 1,
            Self::Rgb => 3,
            Self::Cmyk => 4,
        }
    }

    /// The colour space a profile for this model declares.
    fn space(self) -> DataColorSpace {
        match self {
            Self::Grey => DataColorSpace::Gray,
            Self::Rgb => DataColorSpace::Rgb,
            Self::Cmyk => DataColorSpace::Cmyk,
        }
    }

    fn layout(self) -> Layout {
        match self {
            Self::Grey => Layout::Gray,
            Self::Rgb => Layout::Rgb,
            Self::Cmyk => Layout::Cmyka, // moxcms names its four-ink layout so; there is no alpha
        }
    }
}

/// A profile ready to convert the samples of one colour model to sRGB.
pub(super) struct Profile {
    model: Model,
    transform: Arc<dyn TransformExecutor<f32> + Send + Sync>,
}

impl Profile {
    /// The embedded profile `bytes` for samples of `model`, or None if it is unreadable or describes another colour
    /// space or cannot convert: the colours are then taken as sRGB, as viewers do.
    pub(super) fn new(bytes: &[u8], model: Model) -> Option<Self> {
        // The profile this core writes is sRGB by definition: converting by it would only add the rounding error of
        // two definitions of sRGB (up to 1.2 levels of 255), so a file the core wrote reads back as written.
        if bytes == SRGB {
            return None;
        }
        let profile = ColorProfile::new_from_slice(bytes).ok()?;
        if profile.color_space != model.space() {
            return None;
        }
        // The default is fixed-point arithmetic, which loses the colours beyond sRGB's gamut that wide profiles hold.
        let options = TransformOptions {
            prefer_fixed_point: false,
            ..TransformOptions::default()
        };
        let transform = profile
            .create_transform_f32(model.layout(), &ColorProfile::new_srgb(), Layout::Rgb, options)
            .ok()?;
        Some(Self { model, transform })
    }

    /// Converts `colour`, the pixels of a file of grey or RGB samples (a grey file has its level in all three channels),
    /// to sRGB in place, clipped to 0..1.
    pub(super) fn convert(&self, colour: &mut [[f32; 3]]) -> Result<(), CmsError> {
        let width = self.model.samples();
        let mut levels = vec![0.0; colour.len().min(CHUNK) * width];
        for chunk in colour.chunks_mut(CHUNK) {
            let levels = &mut levels[..chunk.len() * width];
            for (level, pixel) in levels.chunks_exact_mut(width).zip(&*chunk) {
                level.copy_from_slice(&pixel[..width]);
            }
            self.srgb(levels, chunk)?;
        }
        Ok(())
    }

    /// The sRGB colour, clipped to 0..1, of each pixel of `inks` (four per pixel, 1 = 255).
    pub(super) fn convert_inks(&self, inks: &[u8]) -> Result<Vec<[f32; 3]>, CmsError> {
        let mut colour = vec![[0.0; 3]; inks.len() / 4];
        let mut levels = vec![0.0; inks.len().min(CHUNK * 4)];
        for (from, to) in inks.chunks(CHUNK * 4).zip(colour.chunks_mut(CHUNK)) {
            let levels = &mut levels[..from.len()];
            for (level, &ink) in levels.iter_mut().zip(from) {
                *level = f32::from(ink) / 255.0; // constant: the largest 8-bit level
            }
            self.srgb(levels, to)?;
        }
        Ok(colour)
    }

    /// Writes into `to` the sRGB colour, clipped to 0..1, of the pixels whose samples are `levels`.
    fn srgb(&self, levels: &[f32], to: &mut [[f32; 3]]) -> Result<(), CmsError> {
        self.transform.transform(levels, to.as_flattened_mut())?;
        for level in to.as_flattened_mut() {
            *level = level.clamp(0.0, 1.0);
        }
        Ok(())
    }
}

//! Converting the colours of a file from its embedded ICC profile to sRGB.

use std::sync::Arc;

use moxcms::{CmsError, ColorProfile, DataColorSpace, Layout, TransformExecutor, TransformOptions};

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
        let profile = ColorProfile::new_from_slice(bytes).ok()?;
        if profile.color_space != model.space() {
            return None;
        }
        // The default is fixed-point arithmetic, which loses the colours beyond sRGB's gamut that wide profiles hold.
        let mut options = TransformOptions::default();
        options.prefer_fixed_point = false;
        let transform = profile
            .create_transform_f32(model.layout(), &ColorProfile::new_srgb(), Layout::Rgb, options)
            .ok()?;
        Some(Self { model, transform })
    }

    /// The sRGB colour, clipped to 0..1, of each pixel of `samples` (`model` samples per pixel, each over `white`, the
    /// largest level of its depth).
    pub(super) fn to_srgb<T: Copy + Into<f32>>(&self, samples: &[T], white: f32) -> Result<Vec<[f32; 3]>, CmsError> {
        let width = self.model.samples();
        let mut colour = vec![[0.0; 3]; samples.len() / width];
        let mut levels = vec![0.0; samples.len().min(CHUNK * width)];
        for (from, to) in samples
            .chunks(CHUNK * width)
            .zip(colour.as_flattened_mut().chunks_mut(CHUNK * 3))
        {
            let levels = &mut levels[..from.len()];
            for (level, &sample) in levels.iter_mut().zip(from) {
                *level = sample.into() / white;
            }
            self.transform.transform(levels, to)?;
        }
        for level in colour.as_flattened_mut() {
            *level = level.clamp(0.0, 1.0);
        }
        Ok(colour)
    }
}

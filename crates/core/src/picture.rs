//! The picture an effect reads and the image it makes.

use std::fmt;

use crate::Error;
use crate::ops::par;

/// A picture borrowed from its owner: `height` rows of `width` pixels, row after row, each pixel three channels (red,
/// green, blue), nominally 0..1.
#[derive(Clone, Copy)]
pub struct Picture<'a> {
    height: usize,
    width: usize,
    pixels: &'a [[f32; 3]],
}

impl fmt::Debug for Picture<'_> {
    /// The shape only: a picture can hold millions of pixels.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Picture")
            .field("height", &self.height)
            .field("width", &self.width)
            .finish_non_exhaustive()
    }
}

impl<'a> Picture<'a> {
    /// The picture of `height` x `width` pixels whose channels are `values`, pixel after pixel, row after row.
    ///
    /// # Errors
    /// [`Error::Size`] unless `height` and `width` are at least 1 and there are exactly `height * width * 3` values: a
    /// picture has a pixel, so nothing downstream has to ask whether it does. [`Error::NonFinite`] if a value is NaN or
    /// an infinity (the message names the first such pixel): the one place outside pixels enter the core, so no picture
    /// holds one.
    pub fn new(height: usize, width: usize, values: &'a [f32]) -> Result<Self, Error> {
        let (pixels, rest) = values.as_chunks::<3>();
        if !(height > 0 && width > 0 && rest.is_empty() && height.checked_mul(width) == Some(pixels.len())) {
            return Err(Error::Size {
                height,
                width,
                values: values.len(),
            });
        }
        if let Some(index) = pixels
            .iter()
            .position(|pixel| !pixel.iter().all(|level| level.is_finite()))
        {
            return Err(Error::NonFinite(format!(
                "a picture's levels must be numbers, but it holds NaN or an infinity: the first is at row {}, column {}",
                index / width,
                index % width
            )));
        }
        Ok(Self { height, width, pixels })
    }

    /// The number of rows.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// The number of columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// The channel values, three per pixel, pixel after pixel, row after row: what [`Picture::new`] was given.
    #[must_use]
    pub fn values(&self) -> &'a [f32] {
        self.pixels.as_flattened()
    }

    /// The pixels, row after row.
    pub(crate) const fn pixels(&self) -> &'a [[f32; 3]] {
        self.pixels
    }

    /// The first `rows` rows of the picture (1 to its height), a picture of their own: what lies beyond its last row is
    /// what a filter's edge rule puts there, not the rest of this picture.
    pub(crate) fn top(&self, rows: usize) -> Self {
        Self {
            height: rows,
            width: self.width,
            pixels: &self.pixels[..rows * self.width],
        }
    }

    /// A new image of this picture's size whose every pixel is `change` of the pixel at its place: the shape of an
    /// effect that works pixel by pixel. `change` is called for pixels in any order, and from the threads the render was
    /// given ([`Options::with_threads`](crate::Options::with_threads)): the pixel it gives must depend only on its arguments.
    #[must_use]
    pub fn map(&self, change: impl Fn([f32; 3]) -> [f32; 3] + Sync) -> Image {
        self.map_at(|_, _, pixel| change(pixel))
    }

    /// Like [`Picture::map`], for an effect that also depends on where a pixel is: `change` is given the pixel's row
    /// (from the top), its column (from the left) and the pixel. An effect that ignores the picture's content uses
    /// only the position. As for [`map`](Self::map), `change` is called in any order and from any of the render's threads.
    #[must_use]
    pub fn map_at(&self, change: impl Fn(usize, usize, [f32; 3]) -> [f32; 3] + Sync) -> Image {
        let mut pixels = vec![[0.0; 3]; self.pixels.len()];
        par::rows(&mut pixels, self.width, |first_row, band| {
            let first = first_row * self.width;
            for (index, (out, &pixel)) in band.iter_mut().zip(&self.pixels[first..]).enumerate() {
                let index = first + index;
                *out = change(index / self.width, index % self.width, pixel);
            }
        });
        Image::from_pixels(self.height, self.width, pixels)
    }
}

/// An image an effect made, of the size of the picture it read: pixel after pixel, row after row, three channels each.
///
/// An image can have a fourth channel, alpha (0 transparent, 1 opaque; straight, the colour not multiplied by it): a
/// file read with transparency ([`read`](crate::read), [`Image::rgba`]), or a render's `frame.transparent` cut-out, made
/// after the last step. An effect never sees one: what it reads is a [`Picture`], whose colour channels are all of it.
#[derive(Clone, PartialEq)]
pub struct Image {
    height: usize,
    width: usize,
    pixels: Vec<[f32; 3]>,
    alpha: Option<Vec<f32>>,
}

impl fmt::Debug for Image {
    /// The shape only: an image can hold millions of pixels.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Image")
            .field("height", &self.height)
            .field("width", &self.width)
            .field("channels", &(if self.alpha.is_some() { 4 } else { 3 }))
            .finish_non_exhaustive()
    }
}

impl Image {
    /// The image of `height` x `width` pixels whose channels are `values`, pixel after pixel, row after row: for an
    /// effect that makes its pixels elsewhere.
    ///
    /// # Errors
    /// [`Error::Size`] as for [`Picture::new`].
    pub fn new(height: usize, width: usize, values: &[f32]) -> Result<Self, Error> {
        let picture = Picture::new(height, width, values)?;
        Ok(Self::from_pixels(height, width, picture.pixels().to_vec()))
    }

    /// The image of `height` x `width` pixels with an alpha channel, whose `values` are red, green, blue and alpha of
    /// each pixel, row after row: alpha 0 is transparent, 1 opaque, and the colour is straight, not multiplied by it.
    ///
    /// # Errors
    /// [`Error::InvalidImage`] unless `height` and `width` are at least 1 and there are exactly `height * width * 4`
    /// values, or if an alpha is outside 0..=1. [`Error::NonFinite`] if a value is NaN or an infinity (the message names
    /// the first such pixel).
    pub fn rgba(height: usize, width: usize, values: &[f32]) -> Result<Self, Error> {
        let (quads, rest) = values.as_chunks::<4>();
        if !(height > 0 && width > 0 && rest.is_empty() && height.checked_mul(width) == Some(quads.len())) {
            return Err(Error::InvalidImage(format!(
                "{} values do not make {height} x {width} pixels of 4 channels",
                values.len()
            )));
        }
        if let Some(index) = quads
            .iter()
            .position(|quad| !quad.iter().all(|level| level.is_finite()))
        {
            return Err(Error::NonFinite(format!(
                "a picture's levels must be numbers, but it holds NaN or an infinity: the first is at row {}, column {}",
                index / width,
                index % width
            )));
        }
        if let Some(index) = quads.iter().position(|quad| !(0.0..=1.0).contains(&quad[3])) {
            return Err(Error::InvalidImage(format!(
                "alpha is from 0 to 1, but row {}, column {} has {}",
                index / width,
                index % width,
                quads[index][3]
            )));
        }
        let pixels = quads.iter().map(|&[red, green, blue, _]| [red, green, blue]).collect();
        Ok(Self::from_pixels(height, width, pixels).with_alpha(quads.iter().map(|quad| quad[3]).collect()))
    }

    /// The image of `height` x `width` `pixels`, row after row; the caller made exactly that many.
    pub(crate) const fn from_pixels(height: usize, width: usize, pixels: Vec<[f32; 3]>) -> Self {
        Self {
            height,
            width,
            pixels,
            alpha: None,
        }
    }

    /// This image with `alpha`, one value from 0 to 1 for each pixel, row after row; the caller made exactly that many.
    pub(crate) fn with_alpha(self, alpha: Vec<f32>) -> Self {
        Self {
            alpha: Some(alpha),
            ..self
        }
    }

    /// The alpha of each pixel, row after row, if the image has any.
    pub(crate) fn alpha(&self) -> Option<&[f32]> {
        self.alpha.as_deref()
    }

    /// The number of rows.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// The number of columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// The number of channels of each pixel: 3 (red, green, blue), or 4 with alpha last.
    #[must_use]
    pub const fn channels(&self) -> usize {
        if self.alpha.is_some() { 4 } else { 3 }
    }

    /// The channel values, [`channels`](Self::channels) per pixel, pixel after pixel, row after row.
    #[must_use]
    pub fn into_values(self) -> Vec<f32> {
        match self.alpha {
            None => self.pixels.into_flattened(),
            Some(alpha) => self
                .pixels
                .into_iter()
                .zip(alpha)
                .flat_map(|([red, green, blue], alpha)| [red, green, blue, alpha])
                .collect(),
        }
    }

    /// This image read as a picture, without copying it: the input of the next effect.
    #[must_use]
    pub fn as_picture(&self) -> Picture<'_> {
        Picture {
            height: self.height,
            width: self.width,
            pixels: &self.pixels,
        }
    }
}

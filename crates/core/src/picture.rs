//! The picture an effect reads and the image it makes.

use crate::Error;

/// A picture borrowed from its owner: `height` rows of `width` pixels, row after row, each pixel three channels (red,
/// green, blue), nominally 0..1.
#[derive(Clone, Copy, Debug)]
pub struct Picture<'a> {
    height: usize,
    width: usize,
    pixels: &'a [[f32; 3]],
}

impl<'a> Picture<'a> {
    /// The picture of `height` x `width` pixels whose channels are `values`, pixel after pixel, row after row.
    ///
    /// # Errors
    /// [`Error::Size`] unless there are exactly `height * width * 3` values.
    pub fn new(height: usize, width: usize, values: &'a [f32]) -> Result<Self, Error> {
        let (pixels, rest) = values.as_chunks::<3>();
        if rest.is_empty() && height.checked_mul(width) == Some(pixels.len()) {
            Ok(Self { height, width, pixels })
        } else {
            Err(Error::Size {
                height,
                width,
                values: values.len(),
            })
        }
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

    /// A new image of this picture's size whose every pixel is `change` of the pixel at its place: the shape of an
    /// effect that works pixel by pixel.
    #[must_use]
    pub fn map(&self, change: impl Fn([f32; 3]) -> [f32; 3]) -> Image {
        Image {
            height: self.height,
            width: self.width,
            pixels: self.pixels.iter().map(|&pixel| change(pixel)).collect(),
        }
    }
}

/// An image an effect made, of the size of the picture it read: pixel after pixel, row after row, three channels each.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    height: usize,
    width: usize,
    pixels: Vec<[f32; 3]>,
}

impl Image {
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

    /// The channel values, three per pixel, pixel after pixel, row after row.
    #[must_use]
    pub fn into_values(self) -> Vec<f32> {
        self.pixels.into_flattened()
    }
}

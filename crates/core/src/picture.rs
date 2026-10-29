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
        self.map_at(|_, _, pixel| change(pixel))
    }

    /// Like [`Picture::map`], for an effect that also depends on where a pixel is: `change` is given the pixel's row
    /// (from the top), its column (from the left) and the pixel. An effect that ignores the picture's content uses
    /// only the position.
    #[must_use]
    pub fn map_at(&self, change: impl Fn(usize, usize, [f32; 3]) -> [f32; 3]) -> Image {
        Image {
            height: self.height,
            width: self.width,
            pixels: self
                .pixels
                .iter()
                .enumerate()
                .map(|(index, &pixel)| change(index / self.width, index % self.width, pixel))
                .collect(),
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

#[cfg(test)]
mod tests {
    use super::Picture;
    use crate::Error;

    #[test]
    fn map_at_hands_each_pixel_with_its_row_and_column_in_reading_order() -> Result<(), Error> {
        // 2 rows of 3 pixels, each pixel's channels all its own number 0 to 5
        let values: Vec<f32> = (0_u8..6).flat_map(|pixel| [f32::from(pixel); 3]).collect();
        let picture = Picture::new(2, 3, &values)?;
        // the new pixel records where it was found (row, column) and the value it held
        let image = picture.map_at(|row, column, pixel| {
            let place = |index: usize| u8::try_from(index).map_or(f32::NAN, f32::from);
            [place(row), place(column), pixel[0]]
        });
        assert_eq!((image.height(), image.width()), (2, 3));
        assert_eq!(
            image.into_values(),
            [0., 0., 0., 0., 1., 1., 0., 2., 2., 1., 0., 3., 1., 1., 4., 1., 2., 5.]
        );
        Ok(())
    }
}

//! The picture an effect reads and the image it makes.

use std::fmt;

use crate::Error;

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
    /// picture has a pixel, so nothing downstream has to ask whether it does.
    pub fn new(height: usize, width: usize, values: &'a [f32]) -> Result<Self, Error> {
        let (pixels, rest) = values.as_chunks::<3>();
        if height > 0 && width > 0 && rest.is_empty() && height.checked_mul(width) == Some(pixels.len()) {
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

    /// The channel values, three per pixel, pixel after pixel, row after row: what [`Picture::new`] was given.
    #[must_use]
    pub fn values(&self) -> &'a [f32] {
        self.pixels.as_flattened()
    }

    /// The pixels, row after row.
    pub(crate) const fn pixels(&self) -> &'a [[f32; 3]] {
        self.pixels
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
        Image::from_pixels(
            self.height,
            self.width,
            self.pixels
                .iter()
                .enumerate()
                .map(|(index, &pixel)| change(index / self.width, index % self.width, pixel))
                .collect(),
        )
    }
}

/// An image an effect made, of the size of the picture it read: pixel after pixel, row after row, three channels each.
///
/// A render can end in an image with a fourth channel, alpha (0 transparent, 1 opaque; straight, the colour not
/// multiplied by it): the recipe's `frame.transparent` cut-out, made after the last step. An effect never sees one:
/// what it reads is a [`Picture`], whose colour channels are all of it.
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

#[cfg(test)]
mod tests {
    use super::{Image, Picture};
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

    #[test]
    fn the_values_of_a_picture_and_of_an_image_made_from_them_are_the_ones_given() -> Result<(), Error> {
        let values = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        let picture = Picture::new(1, 2, &values)?;
        assert_eq!(picture.values(), values);
        let image = Image::new(1, 2, picture.values())?;
        assert_eq!((image.height(), image.width()), (1, 2));
        assert_eq!(image.into_values(), values);
        Ok(())
    }

    #[test]
    fn an_image_read_as_a_picture_has_its_size_and_its_values() -> Result<(), Error> {
        let values = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        let image = Image::new(2, 1, &values)?;
        let picture = image.as_picture();
        assert_eq!((picture.height(), picture.width()), (2, 1));
        assert_eq!(picture.values(), values);
        Ok(())
    }

    #[test]
    fn an_image_with_alpha_has_four_channels_per_pixel_and_is_read_by_an_effect_as_its_colour_alone()
    -> Result<(), Error> {
        let colour = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        let plain = Image::new(1, 2, &colour)?;
        assert_eq!(plain.channels(), 3);
        let cut = plain.with_alpha(vec![0.7, 0.9]);
        assert_eq!(cut.channels(), 4);
        assert_eq!(cut.as_picture().values(), colour);
        assert_eq!(cut.into_values(), [0.1, 0.2, 0.3, 0.7, 0.4, 0.5, 0.6, 0.9]);
        Ok(())
    }

    #[test]
    fn a_picture_or_image_with_no_row_or_no_column_is_refused_and_one_pixel_is_enough() {
        for (height, width) in [(0, 5), (5, 0), (0, 0)] {
            let picture = Picture::new(height, width, &[]);
            let image = Image::new(height, width, &[]);
            for refused in [picture.map(|_| ()), image.map(|_| ())] {
                assert!(
                    matches!(&refused, Err(error @ Error::Size { .. }) if error.to_string().contains("at least one")),
                    "{height} x {width}: {refused:?}"
                );
            }
        }
        assert!(Picture::new(1, 1, &[0.1, 0.2, 0.3]).is_ok());
        assert!(Image::new(1, 1, &[0.1, 0.2, 0.3]).is_ok());
    }

    #[test]
    fn an_image_of_values_that_do_not_make_its_size_is_refused() {
        assert!(matches!(
            Image::new(1, 2, &[0.0; 5]),
            Err(Error::Size { values: 5, .. })
        ));
        assert!(matches!(
            Image::new(2, 2, &[0.0; 6]),
            Err(Error::Size { values: 6, .. })
        ));
    }
}

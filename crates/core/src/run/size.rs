//! The picture a render works on: [`Options`] (the size), and the working picture, framed and sized, with the ratio of its
//! pixels to design pixels.

use std::sync::Arc;

use super::frame::Geometry;
use crate::ops::length;
use crate::ops::resample::Resize;
use crate::{Error, Frame, Image, Layer, Picture};

/// The longest side in px that a recipe's pixel values are written for.
const DESIGN_SIZE: f64 = 480.0; // constant: the design size of the format, Python's `DESIGN_SIZE`

/// Working pixels per design pixel of the working `picture`: its longest side over [`DESIGN_SIZE`]. The one decision of how
/// a length in a recipe follows the size of the picture.
pub(super) fn ratio_of(picture: &Picture<'_>) -> f64 {
    length::float(picture.height().max(picture.width())) / DESIGN_SIZE
}

/// What a render is asked for besides the recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Options {
    size: Option<u32>,
    threads: usize,
    band: Option<usize>,
}

impl Default for Options {
    /// No size asked for, one thread, the whole picture at once.
    fn default() -> Self {
        Self {
            size: None,
            threads: 1,
            band: None,
        }
    }
}

impl Options {
    /// These options with `size`, the longest side in px of the picture the render works on and returns: the photo is
    /// resampled to it, its aspect kept, and so are its maps. It wins over the recipe's own `size`; without either, the
    /// photo's own size is kept. The render is refused for a size of 0 and for one that makes a picture of more than
    /// 67,108,864 pixels (8192 x 8192), as it is for a `frame` that does: before a pixel of either picture is made.
    ///
    /// A recipe's pixel values (`px`) are written for a longest side of 480; a render multiplies them by the working
    /// picture's longest side over 480, so the look is the same at any size.
    #[must_use]
    pub const fn with_size(mut self, size: u32) -> Self {
        self.size = Some(size);
        self
    }

    /// These options with `threads`, how many threads the render may use: 1 (the default) runs everything on the calling
    /// thread and starts none, 0 uses one for each core of the machine, and no more than 256 are allowed (the render is
    /// refused, naming the number, before anything runs). The picture a render returns does not depend on it: the
    /// same bytes at any number of threads. Only work that is the same for every row of the picture, such as laying a
    /// step over its input, framing and resizing the photo, and an effect that works pixel by pixel, is shared out.
    ///
    /// The default is one because the usual use of more cores is to render several pictures at once (see
    /// [`Photo`](crate::Photo), which can be shared between threads), where each render using every core would
    /// only compete with the others. A single picture, or a few, wants more:
    ///
    /// ```text
    /// let image = photo.render(&look, &presets, Options::default().with_threads(0))?;
    /// ```
    #[must_use]
    pub const fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    /// These options with `rows`, how many rows of the working picture a render makes at a time: the same picture as the
    /// whole-picture render (byte for byte, at any number of threads), made in bands so that the memory a run of steps
    /// needs beyond the picture it makes is about a band's, not several pictures'. Without it the render works on the
    /// whole picture.
    ///
    /// A run of steps in a row that each read a bounded neighbourhood of a pixel (see [`Footprint`](crate::Footprint)) is
    /// made band by band, each band from the rows of the picture before it and as many more above and below as the
    /// steps' margins add up to; the larger the margins, the less a small band saves, and a run whose margins reach
    /// the picture runs whole. A step that may read any pixel (an effect whose footprint is `Whole`, an `each` step or a
    /// step driven by a map) runs on the whole picture, and the runs of steps between such steps are banded. Where a
    /// picture is as tall as a band or less, the render is the whole-picture one. [`trace`](crate::Photo::trace) ignores
    /// it: it keeps a picture for every step.
    ///
    /// A recipe's `px` values scale by the working picture as a whole, never by a band: the look is the same.
    ///
    /// ```text
    /// let image = photo.render(&look, &presets, Options::default().with_band(64))?;
    /// ```
    ///
    /// A `rows` of 0 is refused by the render, naming it, before anything runs ([`Error::Recipe`](crate::Error::Recipe)).
    #[must_use]
    pub const fn with_band(mut self, rows: usize) -> Self {
        self.band = Some(rows);
        self
    }

    /// The rows a band is, if the render is banded.
    pub(super) const fn band(self) -> Option<usize> {
        self.band
    }

    /// The number of threads asked for.
    pub(super) const fn threads(self) -> usize {
        self.threads
    }

    /// The size asked for, if any.
    pub(super) const fn size(self) -> Option<u32> {
        self.size
    }
}

/// The picture a render works on: the photo as given, or, when the recipe frames it or the render is asked for another
/// size, a copy that the render owns, framed and then resized. Maps follow the picture the same way.
pub(super) struct Working<'a> {
    given: Picture<'a>,
    /// The framed, resized copy; none when neither changes the photo.
    made: Option<Image>,
    framing: Option<Geometry>,
    resize: Option<Resize>,
}

impl<'a> Working<'a> {
    /// The working picture for `photo`: framed by `frame`, then at the longest side `size` (none: as framed). `subject`
    /// is the subject map of the photo as given, which a frame that [`needs_subject`](Frame::needs_subject) is given.
    ///
    /// # Errors
    /// [`Error::Recipe`] for a size of 0, or a size or frame that makes a picture of more than 67,108,864 pixels,
    /// [`Error::MapNotFound`] for a subject a crop needs and the photo cannot give.
    pub(super) fn new(
        photo: Picture<'a>,
        frame: &Frame,
        size: Option<u32>,
        subject: Option<&Layer>,
    ) -> Result<Self, Error> {
        if size == Some(0) {
            return Err(Error::Recipe("`size` must be >= 1, got 0".to_owned()));
        }
        let framing = Geometry::plan(frame, photo.height(), photo.width(), subject)?;
        let (height, width) = framing.as_ref().map_or((photo.height(), photo.width()), |framing| {
            (framing.height(), framing.width())
        });
        // both pictures are planned, and refused if over the pixel limit, before either is made
        let resize = size
            .map(|size| Resize::to_longest_side(height, width, size))
            .transpose()?
            .flatten();
        let framed = framing.as_ref().map(|framing| framing.photo(photo.values()));
        let values = framed.as_deref().unwrap_or_else(|| photo.values());
        let made = match &resize {
            Some(resize) => Some(Image::from_pixels(
                resize.height(),
                resize.width(),
                resize.apply(values.as_chunks::<3>().0),
            )),
            None => framed
                .as_deref()
                .map(|values| Image::new(height, width, values))
                .transpose()?,
        };
        Ok(Self {
            given: photo,
            made,
            framing,
            resize,
        })
    }

    /// The picture the steps work on.
    pub(super) fn picture(&self) -> Picture<'_> {
        self.made.as_ref().map_or(self.given, Image::as_picture)
    }

    /// Working pixels per design pixel: see [`ratio_of`].
    pub(super) fn ratio(&self) -> f64 {
        ratio_of(&self.picture())
    }

    /// `map`, a map of the photo as given, as a map of the working picture: the map itself if the picture is the photo,
    /// else framed and resampled to the same grid.
    ///
    /// # Errors
    /// [`Error::Inputs`] only if the framing or resampling broke the rule of a map, which neither can.
    pub(super) fn follow(&self, map: Arc<Layer>) -> Result<Arc<Layer>, Error> {
        let framed = self.framing.as_ref().map(|framing| framing.map(&map)).transpose()?;
        let Some(resize) = &self.resize else {
            return Ok(framed.map_or(map, Arc::new));
        };
        let current = framed.as_ref().unwrap_or(&map);
        Ok(Arc::new(Layer::new(
            resize.height(),
            resize.width(),
            resize.apply(current.values().as_chunks::<1>().0).into_flattened(),
        )?))
    }
}

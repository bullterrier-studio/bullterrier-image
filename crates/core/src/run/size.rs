//! The picture a render works on: [`Options`] (the size), and the working picture, framed and sized, with the ratio of its
//! pixels to design pixels.

use std::num::NonZeroUsize;
use std::ops::Range;
use std::sync::Arc;

use super::frame::Geometry;
use crate::ops::length;
use crate::ops::resample::Resize;
use crate::{Error, Frame, Image, Layer, Picture, Place};

/// The longest side in px that a recipe's pixel values are written for.
const DESIGN_SIZE: f64 = 480.0; // constant: the design size of the format, Python's `DESIGN_SIZE`

/// Working pixels per design pixel for a longest side of `longest_side` px: the one decision of how a length in a recipe
/// follows the size of the picture.
pub(super) fn ratio_of(longest_side: f64) -> f64 {
    longest_side / DESIGN_SIZE
}

/// What a render is asked for besides the recipe: its size, threads, bands, canvas, window, room and (with the feature
/// `gpu`) the backend. Two options are equal when every choice is; a [`Gpu`](crate::Gpu) is the same one only if it is a
/// clone of it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Options {
    size: Option<u32>,
    threads: usize,
    band: Option<NonZeroUsize>,
    canvas: Option<(u32, u32)>,
    window: Option<Window>,
    /// The room around the box of the picture, as top, right, bottom and left.
    room: [u32; 4],
    /// The GPU the render runs on; none: the CPU.
    #[cfg(feature = "gpu")]
    gpu: Option<crate::Gpu>,
}

/// The rows of a canvas that the picture handed to a render is (see [`Options::with_window`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Window {
    first_row: u32,
    width: u32,
    height: u32,
}

/// Where the working picture lies on its canvas, and which part of it is the box: what an effect's [`Place`] is made from.
#[derive(Clone, Copy, Debug)]
pub(super) struct Origin {
    canvas_height: usize,
    canvas_width: usize,
    /// The canvas row of the working picture's row 0.
    first_row: usize,
    /// The room round the box in the canvas: top, right, bottom, left.
    room: [usize; 4],
}

/// A part of the working picture that a step is run on, and where that part lies on the canvas it sees.
#[derive(Clone, Debug)]
pub(super) struct Part {
    /// Its rows and columns of the working picture.
    pub(super) rows: Range<usize>,
    pub(super) columns: Range<usize>,
    /// Where its first row and column lie on the canvas the effect sees (the box, with a room).
    pub(super) place: Place,
}

impl Origin {
    /// A picture of `height` x `width` that is its own canvas, with no room.
    #[cfg(feature = "gpu")]
    pub(super) const fn whole(height: usize, width: usize) -> Self {
        Self {
            canvas_height: height,
            canvas_width: width,
            first_row: 0,
            room: [0; 4],
        }
    }

    /// Whether the picture has room around its box.
    pub(super) fn has_room(self) -> bool {
        self.room != [0; 4]
    }

    /// The part of the `rows` rows of the working picture from row `at` that a step runs on: all of them, or, for a step that
    /// is `boxed` (it places itself by its picture or reads all of it) and a picture with room, the rows and columns of
    /// the box among them. None if no row of them is in the box.
    pub(super) fn part(self, at: usize, rows: usize, boxed: bool) -> Option<Part> {
        let [top, right, bottom, left] = self.room;
        let (first, last) = (self.first_row + at, self.first_row + at + rows);
        if !boxed || !self.has_room() {
            return Some(Part {
                rows: at..at + rows,
                columns: 0..self.canvas_width,
                place: Place::new(self.canvas_height, self.canvas_width, first),
            });
        }
        let (start, end) = (first.max(top), last.min(self.canvas_height - bottom));
        (start < end).then(|| Part {
            rows: at + start - first..at + end - first,
            columns: left..self.canvas_width - right,
            place: Place::new(
                self.canvas_height - top - bottom,
                self.canvas_width - left - right,
                start - top,
            ),
        })
    }
}

impl Default for Options {
    /// No size asked for, one thread, the whole picture at once.
    fn default() -> Self {
        Self {
            size: None,
            threads: 1,
            band: None,
            canvas: None,
            window: None,
            room: [0; 4],
            #[cfg(feature = "gpu")]
            gpu: None,
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
    /// same bytes at any number of threads. The threads are those of a pool of that size shared by every render at that count in the
    /// process (started the first time and kept), so renders running at once on several threads of the caller share n
    /// threads and do not each start n. Only work that is the same for every row of the picture, such as laying a
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
    /// whole-picture render (equal within float rounding, 1e-4; most effects byte for byte; at any number of threads), made in bands so that the memory a run of steps
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
    /// Choosing `rows`: a smaller band holds less and takes longer. Measured on 24 MP (4000 x 6000) with `grade`,
    /// `vignette`, `gaussian_blur` radius 2 and `grain`, one run each; memory is the peak beyond the photo, as a multiple
    /// of the image the render makes; time is against the whole-picture render, at 1 / 4 threads:
    ///
    /// ```text
    /// rows    memory   time
    /// 64      1.20x    +62% / +87%
    /// 256     1.35x    +11% / +27%
    /// 1024    1.92x    0% / +13%
    /// whole   3.00x    baseline
    /// ```
    ///
    /// A `Whole` step in the recipe gets no memory benefit. Blurs done by Fourier transform (16 px a side or more) cost
    /// more as bands shrink.
    ///
    /// ```text
    /// let rows = NonZeroUsize::new(64).ok_or("a band has rows")?;
    /// let image = photo.render(&look, &presets, Options::default().with_band(rows))?;
    /// ```
    #[must_use]
    pub const fn with_band(mut self, rows: NonZeroUsize) -> Self {
        self.band = Some(rows);
        self
    }

    /// These options with `canvas`, the full canvas that the picture the render returns is placed on, `width` x `height`
    /// in px of that picture. The render's `px` values then scale by the canvas's longest side over 480 instead of the
    /// picture's own, so that the pictures placed on one canvas, whatever their sizes, share a length: a `px` value means
    /// the same number of output pixels on each. Nothing else changes: the picture is not resized, and an effect that
    /// works out its place in the picture still takes the picture as the whole.
    ///
    /// The render is refused, naming `canvas`, for a side of 0, before anything runs. The canvas only fixes the scale: it
    /// may be smaller than the picture (a layer scaled past the canvas's edge). A canvas of the picture's own size is no
    /// canvas.
    #[must_use]
    pub const fn with_canvas(mut self, width: u32, height: u32) -> Self {
        self.canvas = Some((width, height));
        self
    }

    /// These options with `window`: the picture the render is handed is the `rows` rows of a canvas `canvas_width` x
    /// `canvas_height` from canvas row `first_row`, in px of that picture. It makes a render of a band of a larger
    /// canvas the same pixels as the same rows of the render of the whole: an effect's place (a vignette's distance
    /// from the middle, noise numbered by position, a gradient, a screen's lattice) is its place on the canvas, and the
    /// design scale follows the window's canvas unless [`with_canvas`](Self::with_canvas) is given: then that canvas fixes
    /// the scale and the window only the place.
    ///
    /// To make rows `top` to `bottom` of a picture whose effects reach `margin` rows (see
    /// [`Recipe::band_margin`](crate::Recipe::band_margin)), hand the render the rows `top - margin` to `bottom + margin` (fewer where the picture
    /// ends) with `first_row` their first, and keep the rows `top` to `bottom` of what it returns: the margin rows are
    /// not the whole render's.
    ///
    /// The render is refused, naming `window`, before anything runs, for a picture that is not `canvas_width` wide, one
    /// that does not lie within the canvas, a canvas side of 0, and for a recipe whose `frame` or `size` (or an `size` in
    /// the options) changes the picture's rows: they are the whole picture's, not a band's. The maps and pictures a
    /// step names are those of the band: a recipe that uses one is the caller's to band (see [`Recipe::band_margin`](crate::Recipe::band_margin)).
    #[must_use]
    pub const fn with_window(mut self, first_row: u32, canvas_width: u32, canvas_height: u32) -> Self {
        self.window = Some(Window {
            first_row,
            width: canvas_width,
            height: canvas_height,
        });
        self
    }

    /// These options with room of `top`, `right`, `bottom` and `left` px around the box of the picture: the picture handed to
    /// the render is the box grown by that room (a caller makes room for what a look paints beyond its content: a halo, a
    /// shadow; see [`Recipe::spread`](crate::Recipe::spread)), and the render is of the box. With a [window](Self::with_window) the
    /// room is in its canvas.
    ///
    /// A step that places itself by its picture (an effect that takes the picture's place: a vignette, a centre, noise numbered
    /// by position, `frame`) or reads all of it (a statistic, a warp about the middle) is run on the box alone, as it would be
    /// on the picture without room: the box it makes is the same bits, its `px` follow the box's longest side, and the room
    /// round it is left as the step received it. Every other step runs on the whole grown picture, so what it paints into the room
    /// (a halo) is kept, and reads the room's pixels at the box's edge. A later step that places itself so does not reach a halo
    /// an earlier one painted.
    ///
    /// The render is refused, naming `room`, before anything runs, for a room that leaves no box, and for a recipe whose `frame`
    /// or `size` (or a `size` in the options) changes the picture: the room is a part of the picture as it is. The maps a step
    /// names are of the grown picture.
    #[must_use]
    pub const fn with_room(mut self, top: u32, right: u32, bottom: u32, left: u32) -> Self {
        self.room = [top, right, bottom, left];
        self
    }

    /// These options with the render on `gpu` (feature `gpu`): the recipe is the same, and so is the [`Image`] a
    /// [`render`](crate::Photo::render) returns (one readback at the end), held to the bound each kernel states, not to the
    /// CPU's bytes (the CPU is the reference). The photo is framed, resized and its maps made on the CPU as before; then it is
    /// sent to the device once and each step runs there as its effect's kernel, laid over its input there too (blend mode,
    /// opacity, mask). A step whose effect has no kernel (see [`Effect::kernel`](crate::Effect::kernel)), a driven step, an
    /// `each` step and one that reads a field or a partition run on the CPU: the picture is read back, the step runs as it
    /// would without the GPU, and the result is sent again (each counted in [`Gpu::transfers`](crate::Gpu::transfers)).
    /// Nothing falls back silently: a picture larger than the device holds is an error naming both sizes.
    ///
    /// [`with_threads`](Self::with_threads) applies to the CPU's part of the work only. A GPU render refuses
    /// [`with_band`](Self::with_band), [`with_window`](Self::with_window) and [`with_room`](Self::with_room) (an error
    /// naming the option, never ignored), and so do [`trace`](crate::Photo::trace), [`region_map`](crate::Photo::region_map)
    /// and the data calls ([`ink_planes`](crate::Photo::ink_planes)), which run on the CPU only.
    #[cfg(feature = "gpu")]
    #[must_use]
    pub fn with_gpu(mut self, gpu: &crate::Gpu) -> Self {
        self.gpu = Some(gpu.clone());
        self
    }

    /// The GPU the render runs on, if any.
    #[cfg(feature = "gpu")]
    pub(super) const fn gpu(&self) -> Option<&crate::Gpu> {
        self.gpu.as_ref()
    }

    /// `Ok` unless these options name a GPU, which `call` (a render that runs on the CPU only) does not take.
    ///
    /// # Errors
    /// [`Error::Gpu`] naming `call`.
    #[cfg(feature = "gpu")]
    pub(super) fn cpu_only(&self, call: &str) -> Result<(), Error> {
        if self.gpu.is_some() {
            return Err(Error::Gpu(format!(
                "`{call}` runs on the CPU only: call it with options that have no `with_gpu`"
            )));
        }
        Ok(())
    }

    /// `Ok` unless these options hold one that a GPU render does not take yet.
    ///
    /// # Errors
    /// [`Error::Gpu`] naming the option: a band, a window or room.
    #[cfg(feature = "gpu")]
    pub(super) fn gpu_takes(&self) -> Result<(), Error> {
        let refused = if self.band.is_some() {
            Some("with_band")
        } else if self.window.is_some() {
            Some("with_window")
        } else if self.room != [0; 4] {
            Some("with_room")
        } else {
            None
        };
        match refused {
            Some(option) => Err(Error::Gpu(format!(
                "a GPU render does not take `{option}` yet: render on the CPU (no `with_gpu`) for it"
            ))),
            None => Ok(()),
        }
    }

    /// The rows a band is, if the render is banded.
    pub(super) const fn band(&self) -> Option<NonZeroUsize> {
        self.band
    }

    /// The number of threads asked for.
    pub(super) const fn threads(&self) -> usize {
        self.threads
    }

    /// The size asked for, if any.
    pub(super) const fn size(&self) -> Option<u32> {
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

    /// Working pixels per design pixel (see [`ratio_for`]) of the working picture.
    ///
    /// # Errors
    /// As [`ratio_for`].
    pub(super) fn ratio(&self, options: &Options) -> Result<f64, Error> {
        let picture = self.picture();
        ratio_for(picture.height(), picture.width(), options)
    }

    /// Where the working picture lies on its canvas: on itself, unless `options` give a window, and what part of it is the
    /// box, unless they give room.
    ///
    /// # Errors
    /// [`Error::Recipe`] for a window the picture is not the rows of (see [`Options::with_window`]), and for room that leaves no
    /// box or a recipe that changes the picture (see [`Options::with_room`]).
    pub(super) fn origin(&self, options: &Options) -> Result<Origin, Error> {
        let picture = self.picture();
        let (height, width) = (picture.height(), picture.width());
        let room = options.room.map(|side| side as usize);
        let (what, canvas_height, canvas_width, first_row) = match options.window {
            Some(window) => (
                "window",
                window.height as usize,
                window.width as usize,
                window.first_row as usize,
            ),
            None => ("room", height, width, 0),
        };
        let refuse = |why: String| Error::Recipe(format!("`{what}` {why}"));
        if options.window.is_some() || room != [0; 4] {
            if canvas_width == 0 || canvas_height == 0 {
                return Err(refuse(format!(
                    "{canvas_width} x {canvas_height} must have both sides at least 1"
                )));
            }
            if self.made.is_some() {
                return Err(refuse(
                    "needs the photo as it is: the recipe's `frame` or `size` changes it".to_owned(),
                ));
            }
            if canvas_width != width {
                return Err(refuse(format!(
                    "is {canvas_width} px wide, the picture {width}: a window is rows of the canvas"
                )));
            }
            if first_row + height > canvas_height {
                return Err(refuse(format!(
                    "of rows {first_row}..{} does not lie in a canvas of {canvas_height} rows",
                    first_row + height
                )));
            }
        }
        let [top, right, bottom, left] = room;
        if top + bottom >= canvas_height || left + right >= canvas_width {
            return Err(Error::Recipe(format!(
                "`room` of {top}, {right}, {bottom}, {left} px (top, right, bottom, left) leaves no box in a picture of \
                 {canvas_height} x {canvas_width}"
            )));
        }
        Ok(Origin {
            canvas_height,
            canvas_width,
            first_row,
            room,
        })
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

/// Working pixels per design pixel of a working picture of `height` x `width`, the one decision of how a length in a recipe
/// follows the size of the picture: the longest side over [`DESIGN_SIZE`], of the canvas `options` name, else of the box
/// of the window's canvas, else of the box of the picture (the picture less its room).
///
/// # Errors
/// [`Error::Recipe`] for a canvas with a side of 0.
pub(super) fn ratio_for(height: usize, width: usize, options: &Options) -> Result<f64, Error> {
    let [top, right, bottom, left] = options.room;
    let boxed = |across: u32, down: u32| (across.saturating_sub(left + right), down.saturating_sub(top + bottom));
    let canvas = match (options.canvas, options.window) {
        (Some(canvas), _) => Some(canvas),
        (None, Some(window)) => Some(boxed(window.width, window.height)),
        (None, None) => None,
    };
    let Some((canvas_width, canvas_height)) = canvas else {
        let side = height
            .saturating_sub((top + bottom) as usize)
            .max(width.saturating_sub((left + right) as usize));
        return Ok(ratio_of(length::float(side)));
    };
    if canvas_width == 0 || canvas_height == 0 {
        return Err(Error::Recipe(format!(
            "`canvas` {canvas_width} x {canvas_height} must have both sides at least 1"
        )));
    }
    Ok(ratio_of(f64::from(canvas_width.max(canvas_height))))
}

//! What an effect reads around a pixel: the answer a banded renderer needs to render some rows of a picture without the
//! rest.

/// What an effect reads around a pixel, given its parameters (see [`Effect::footprint`](crate::Effect::footprint)).
///
/// It is about rows, the unit a banded renderer cuts a picture by. The promise of [`Pixel`](Self::Pixel) and
/// [`Radius`](Self::Radius): the rows of the output from `a` to `b` are the same whether the effect runs on the whole
/// picture or only on the rows of the input from `radius` rows above `a` to `radius` rows below `b` (fewer where the
/// picture ends), with the same size ratio and the maps the effect is given cut to the same rows. So the effect reads
/// no row farther than that, and its answer does not depend on the row it is at, the picture's height or a statistic
/// of the whole picture: an effect that does is [`Whole`](Self::Whole), as is any whose answer is not worked out.
/// Whole is always true; a smaller answer must hold.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Footprint {
    /// Each output pixel is a function of the input pixel in the same place alone.
    Pixel,
    /// Each output row is a function of the input rows at most this many design pixels away (a finite number of 0 or
    /// more); it may read the whole of each of them, so a sideways reach costs nothing. Scale it by the size ratio like
    /// any pixel value, see [`margin`](Self::margin).
    Radius(f64),
    /// An output pixel may depend on any input pixel, on the row it is in or on the picture's height.
    Whole,
}

/// The rows beyond `ceil(radius * ratio)` that a margin keeps for reading between pixels: bicubic interpolation reads from
/// one row above to two below the row it is at (a window of 3 rows is the widest of any effect's neighbourhood).
const TAPS: usize = 2; // constant: the extra rows of a bicubic window

impl Footprint {
    /// `Radius(radius)`, or [`Whole`](Self::Whole) if the effect `wraps` the picture round its edge: a row near the top
    /// then reads the bottom.
    #[must_use]
    pub(crate) fn reach(radius: f64, wraps: bool) -> Self {
        if wraps { Self::Whole } else { Self::Radius(radius) }
    }

    /// The rows of margin to render above and below a band of rows for a picture of `ratio` working pixels per design
    /// pixel: none for [`Pixel`](Self::Pixel); for [`Radius`](Self::Radius) the radius rounded up to a whole row, and two
    /// rows more for the pixels an interpolation reads beside the place it is asked for; `None` for
    /// [`Whole`](Self::Whole), which has no margin short of the picture.
    #[must_use]
    pub fn margin(self, ratio: f64) -> Option<usize> {
        match self {
            Self::Pixel => Some(0),
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a radius is a finite number of 0 or more; a margin beyond the picture means the whole of it"
            )]
            Self::Radius(radius) => Some((radius * ratio).ceil() as usize + TAPS),
            Self::Whole => None,
        }
    }
}

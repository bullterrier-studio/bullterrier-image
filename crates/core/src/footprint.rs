//! What an effect reads around a pixel: the answer a banded renderer needs to render some rows of a picture without the
//! rest.

/// What an effect reads around a pixel, given its parameters (see [`Effect::footprint`](crate::Effect::footprint)).
///
/// The parameters are those the effect is rendered with: scaled to the picture, floors included (see
/// `Table::scaled`), so every distance is in working pixels and the footprint and the
/// render cannot disagree about the scaling.
///
/// It is about rows, the unit a banded renderer cuts a picture by. The promise of [`Pixel`](Self::Pixel) and
/// [`Radius`](Self::Radius): the rows of the output from `a` to `b` are the same whether the effect runs on the whole
/// picture or only on the rows of the input from `radius` rows above `a` to `radius` rows below `b` (fewer where the
/// picture ends), with the same size ratio, the same [`Place`](crate::Place) on the canvas and the maps the effect is given cut to the
/// same rows. So the effect reads no row farther than that, and its answer does not depend on the row it is at or the
/// picture's height except through the place it is told, nor on a statistic of the whole picture: an effect that does is
/// [`Whole`](Self::Whole), as is any whose answer is not worked out.
/// Whole is always true; a smaller answer must hold.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Footprint {
    /// Each output pixel is a function of the input pixel in the same place alone.
    Pixel,
    /// Each output row is a function of the input rows at most this many working pixels away (a finite number of 0 or
    /// more); it may read the whole of each of them, so a sideways reach costs nothing. See [`margin`](Self::margin).
    Radius(f64),
    /// As [`Radius`](Self::Radius) (`0` for an effect that reads only its own pixel), and the effect sums a blur by a
    /// Fourier transform, whose sum depends on the rows it is given: a window of rows differs from the whole render in the
    /// last bits (measured up to 7.3e-5), so [`exact_margin`](Self::exact_margin) is none (a window that need only look the
    /// same: [`Recipe::close_band_margin`](crate::Recipe::close_band_margin)).
    Fourier(f64),
    /// As [`Radius`](Self::Radius), and the effect reads rows at a distance too fine for every row to carry (see
    /// `ops::shift::rows_exact`): the position it reads rounds by the row it is worked out from, so a window of rows that
    /// starts low differs from the whole render in the last bits and [`exact_margin`](Self::exact_margin) is none.
    Rounded(f64),
    /// An output pixel may depend on any input pixel, on the row it is in or on the picture's height.
    Whole,
}

/// The rows beyond `ceil(radius)` that a margin keeps for reading between pixels: bicubic interpolation reads from
/// one row above to two below the row it is at (a window of 3 rows is the widest of any effect's neighbourhood).
pub(crate) const TAPS: usize = 2; // constant: the extra rows of a bicubic window

impl Footprint {
    /// `Radius(radius)`, or [`Whole`](Self::Whole) if the effect `wraps` the picture round its edge: a row near the top
    /// then reads the bottom.
    #[must_use]
    pub(crate) fn reach(radius: f64, wraps: bool) -> Self {
        if wraps { Self::Whole } else { Self::Radius(radius) }
    }

    /// This footprint for an effect whose blur is summed by a Fourier transform when `summed`: a
    /// [`Radius`](Self::Radius) or [`Pixel`](Self::Pixel) becomes [`Fourier`](Self::Fourier), another stays as it is.
    #[must_use]
    pub(crate) fn transformed(self, summed: bool) -> Self {
        match self {
            Self::Radius(radius) if summed => Self::Fourier(radius),
            Self::Pixel if summed => Self::Fourier(0.0),
            other => other,
        }
    }

    /// This footprint for an effect that reads rows at a distance too fine for every row to carry when `fine`: a
    /// [`Radius`](Self::Radius) or [`Pixel`](Self::Pixel) becomes [`Rounded`](Self::Rounded), another stays as it is.
    #[must_use]
    pub(crate) fn rounded(self, fine: bool) -> Self {
        match self {
            Self::Radius(radius) if fine => Self::Rounded(radius),
            Self::Pixel if fine => Self::Rounded(0.0),
            other => other,
        }
    }

    /// The rows of margin to render above and below a band of rows: none for [`Pixel`](Self::Pixel); for
    /// [`Radius`](Self::Radius) and [`Fourier`](Self::Fourier) the radius rounded up to a whole row, and two
    /// rows more for the pixels an interpolation reads beside the place it is asked for; `None` for
    /// [`Whole`](Self::Whole), which has no margin short of the picture.
    #[must_use]
    pub fn margin(self) -> Option<usize> {
        match self {
            Self::Pixel => Some(0),
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a radius is a finite number of 0 or more; a margin beyond the picture means the whole of it"
            )]
            Self::Radius(radius) | Self::Fourier(radius) | Self::Rounded(radius) => Some(radius.ceil() as usize + TAPS),
            Self::Whole => None,
        }
    }

    /// [`margin`](Self::margin) for a window whose rows must not depend at all on how the picture is cut: none for
    /// [`Fourier`](Self::Fourier) and [`Rounded`](Self::Rounded) as well as [`Whole`](Self::Whole).
    #[must_use]
    pub fn exact_margin(self) -> Option<usize> {
        match self {
            Self::Fourier(_) | Self::Rounded(_) => None,
            other => other.margin(),
        }
    }
}

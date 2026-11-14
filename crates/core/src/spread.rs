//! How far an effect paints beyond the edge of the picture it is given: the answer a caller needs to hand a render a
//! picture with room around it, so a halo is not cut at the edge.

use crate::Footprint;
use crate::footprint::TAPS;

/// How far an effect, given its parameters, can paint beyond the edge of what it is given (see
/// [`Effect::spread`](crate::Effect::spread)): the room a halo needs.
///
/// The promise of [`Beyond`](Self::Beyond): take a picture that is plain black except for some content, and the box that
/// holds the content. Whatever the effect makes of it that differs from what it makes of an all-black picture of the same
/// size lies within that box grown by the sides' numbers. So a picture grown by that room on each side (black, the effect's
/// own geometry unchanged) holds the whole of the effect's halo. An effect that only recolours pixels, or that paints
/// inside the shapes it is given, spreads by none. Sides are in working pixels (parameters scaled to the picture, as for
/// [`Footprint`](crate::Footprint)), a whole or a fraction of a pixel. Unbounded is always true; a smaller answer must
/// hold.
///
/// Spreads add along a chain of steps: the second step's halo can start where the first's ends.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Spread {
    /// The effect paints at most this many working pixels (each 0 or more, finite) beyond the top, right, bottom and left
    /// edge.
    Beyond {
        /// Pixels above the top edge.
        top: f64,
        /// Pixels right of the right edge.
        right: f64,
        /// Pixels below the bottom edge.
        bottom: f64,
        /// Pixels left of the left edge.
        left: f64,
    },
    /// The effect may paint anywhere: no room holds it (it wraps the picture round its edge, say).
    Unbounded,
}

impl Spread {
    /// No spread: the effect paints nothing beyond the edge.
    pub const NONE: Self = Self::all(0.0);

    /// `reach` pixels (0 or more, finite) beyond every edge.
    #[must_use]
    pub const fn all(reach: f64) -> Self {
        Self::Beyond {
            top: reach,
            right: reach,
            bottom: reach,
            left: reach,
        }
    }

    /// The spread of an effect that reaches as far sideways as it reads rows, whatever its footprint says: no room for
    /// [`Pixel`](Footprint::Pixel); for [`Radius`](Footprint::Radius) the radius and the pixels a margin adds (a kernel's
    /// reach rounded up to a whole pixel, a window of an interpolation) on every side; [`Unbounded`](Self::Unbounded) for
    /// [`Whole`](Footprint::Whole). The tests of `spread` hold each effect that uses it to its word.
    #[must_use]
    pub(crate) fn of_footprint(footprint: Footprint) -> Self {
        match footprint {
            Footprint::Pixel => Self::NONE,
            #[expect(clippy::cast_precision_loss, reason = "a window of a few pixels")]
            Footprint::Radius(radius) => Self::all(radius + TAPS as f64),
            _ => Self::Unbounded,
        }
    }

    /// The spread of what is moved `down` and `right` (working pixels, either sign, finite) by a shift that may read
    /// between two pixels: on the side it moves toward, the distance and one pixel more, on the others none.
    #[must_use]
    pub(crate) fn moved(down: f64, right: f64) -> Self {
        let toward = |distance: f64| if distance > 0.0 { distance + 1.0 } else { 0.0 };
        Self::Beyond {
            top: toward(-down),
            right: toward(right),
            bottom: toward(down),
            left: toward(-right),
        }
    }

    /// This spread then `next`: the sides added, as along a chain of steps. Unbounded if either is.
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        self.combine(next, |a, b| a + b)
    }

    /// The widest of this spread and `other` on each side: for alternatives, of which one runs. Unbounded if either is.
    #[must_use]
    pub fn widest(self, other: Self) -> Self {
        self.combine(other, f64::max)
    }

    fn combine(self, other: Self, side: fn(f64, f64) -> f64) -> Self {
        match (self, other) {
            (
                Self::Beyond {
                    top,
                    right,
                    bottom,
                    left,
                },
                Self::Beyond {
                    top: t,
                    right: r,
                    bottom: b,
                    left: l,
                },
            ) => Self::Beyond {
                top: side(top, t),
                right: side(right, r),
                bottom: side(bottom, b),
                left: side(left, l),
            },
            _ => Self::Unbounded,
        }
    }

    /// The room, in whole pixels, to give a picture on its `[top, right, bottom, left]` sides: each side rounded up; none
    /// for [`Unbounded`](Self::Unbounded).
    #[must_use]
    pub fn pixels(self) -> Option<[usize; 4]> {
        let Self::Beyond {
            top,
            right,
            bottom,
            left,
        } = self
        else {
            return None;
        };
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a side is a finite number of 0 or more; a halo is rounded up to a whole pixel"
        )]
        let whole = |side: f64| side.ceil() as usize;
        Some([whole(top), whole(right), whole(bottom), whole(left)])
    }
}

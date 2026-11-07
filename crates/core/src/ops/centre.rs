//! A picture warped about a centre: every pixel shows the picture at a place worked out from where the pixel lies.
//!
//! A warp says, for a [`Spot`] (a pixel and how far it is from the centre), the place it reads; [`Centre::warp`] reads
//! the picture there for every pixel and holds the result to 0..1.

use super::par;
use super::sample::Sampler;
use crate::Image;

/// A point of the picture that a warp is about, in pixels from the first row and the first column (it may sit between
/// pixels).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Centre {
    row: f64,
    column: f64,
}

/// A pixel of the picture and its place measured from the [`Centre`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Spot {
    centre: Centre,
    /// How far below the centre the pixel is, in pixels (above is negative).
    pub(crate) down: f64,
    /// How far right of the centre the pixel is, in pixels (left is negative).
    pub(crate) across: f64,
}

impl Centre {
    /// The centre at fraction (`x`, `y`) of the way from the first pixel to the last, across and down: 0 is the first
    /// column or row, 0.5 the middle (between the two middle pixels of a side with an even number), 1 the last, so the
    /// centre can sit on a pixel. The picture is `height` x `width`, both at least 1.
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    pub(crate) fn new(height: usize, width: usize, x: f64, y: f64) -> Self {
        Self {
            row: y * (height - 1) as f64,
            column: x * (width - 1) as f64,
        }
    }

    /// The picture of `sampler` with every pixel replaced by the picture read at the place `source` gives for it as
    /// (row, column), a pixel's centre at whole numbers. Each colour channel is read on its own and held to 0..1 (a
    /// cubic reading can overshoot). `source` must give a finite place; it is called for the pixels in any order and from
    /// the render's threads, so it depends only on its argument. Reading every pixel where it is returns the picture.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "a picture side is far below 2**52; the readings are made in f64 and stored in the f32 image"
    )]
    pub(crate) fn warp(self, sampler: &Sampler<'_>, source: impl Fn(Spot) -> (f64, f64) + Sync) -> Image {
        let (height, width) = (sampler.height(), sampler.width());
        let mut out = vec![[0.0; 3]; height * width];
        par::rows(&mut out, width, |first_row, band| {
            for (at, pixel) in band.iter_mut().enumerate() {
                let (row, column) = (first_row + at / width, at % width);
                let (read_row, read_column) = source(Spot {
                    centre: self,
                    down: row as f64 - self.row,
                    across: column as f64 - self.column,
                });
                *pixel = sampler
                    .at(read_row, read_column)
                    .map(|level| level.clamp(0.0, 1.0) as f32);
            }
        });
        Image::from_pixels(height, width, out)
    }
}

impl Spot {
    /// The pixel's distance from the centre, in pixels.
    pub(crate) fn distance(&self) -> f64 {
        self.down.hypot(self.across)
    }

    /// The place at the pixel's offset from the centre multiplied by `scale` (2 is twice as far out, 0.5 half as far)
    /// and then turned by `turn` radians, as (row, column). A positive `turn` makes a pixel read the place counter-clockwise
    /// of it as seen with rows running down and columns right (a pixel right of the centre reads above it for a quarter
    /// turn), so the picture's content appears turned clockwise; a negative one the other way. The centre pixel is its own
    /// place.
    pub(crate) fn moved(&self, scale: f64, turn: f64) -> (f64, f64) {
        let (down, across) = (self.down * scale, self.across * scale);
        let (sin, cos) = turn.sin_cos();
        (
            self.centre.row + down * cos - across * sin,
            self.centre.column + down * sin + across * cos,
        )
    }
}

//! Turning a picture about its centre, clockwise for a positive angle.
//!
//! A quarter turn moves every sample exactly. Any other angle is an affine transform read in the picture's pixel-corner
//! coordinates (the centre of the pixel in column `x` is at `x + 0.5`), sampled as Pillow's `Image.transform` does:
//! nearest (in 16.16 fixed point, where a position falling exactly on the border between two pixels goes the way
//! Pillow's integers send it), bilinear, or bicubic with Keys' kernel at `a = -1` (the kernel of Pillow's transform,
//! not the one it resizes with). A bilinear or bicubic sample is read with its position clipped to the picture's edge;
//! a pixel whose centre falls outside the picture is 0. Samples are interleaved: `channels` of them for each pixel,
//! row after row.

use super::frame::{Rect, Shape, window};
use super::length::{float, whole};
use super::par;
use crate::frame::{Fit, Resample};

/// Degrees in a quarter turn.
const QUARTER: f64 = 90.0; // constant: a right angle
/// Degrees in a half turn, from which an angle is wrapped into -180 up to 180.
const HALF: f64 = 180.0; // constant: a straight angle
/// Degrees in a turn.
const TURN: f64 = 360.0; // constant: a full turn
/// Positions of the nearest pixel are whole numbers of this many parts of a pixel.
const FIXED: f64 = 65536.0; // constant: 16.16 fixed point, as Pillow's integer affine transform
/// The bits of a position after the point.
const FIXED_BITS: u32 = 16; // constant: 16.16 fixed point, as Pillow's integer affine transform
/// Sizes are kept to this many parts of a pixel.
const PARTS: f64 = 1e6; // constant: six digits, so that the 1e-16 of a cosine at a quarter turn cannot move a size

/// A turn of a picture, planned once from its size so that the photo and its maps are turned alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Turn {
    from: (usize, usize),
    /// The size of the turned picture (rows, columns).
    to: (usize, usize),
    /// The angle in degrees, from -180 up to 180.
    angle: f64,
    fit: Fit,
    resample: Resample,
}

impl Turn {
    /// The turn of a `height` x `width` picture by `angle` degrees, clockwise when positive (an angle is the one a whole
    /// number of turns away, from -180 up to 180). `Expand` makes the picture as large as the whole turned picture needs
    /// (the corners it leaves are filled); `Crop` keeps the largest rectangle of the picture's aspect that the turned
    /// picture covers everywhere. The sizes are kept to a millionth of a pixel before they are cut to whole pixels, so
    /// that the 1e-16 of a cosine at a quarter turn cannot move one by a pixel.
    pub(crate) fn new(height: usize, width: usize, angle: f64, fit: Fit, resample: Resample) -> Self {
        let angle = (angle + HALF).rem_euclid(TURN) - HALF;
        let radians = angle.to_radians();
        let (cos, sin) = (radians.cos().abs(), radians.sin().abs());
        let (h, w) = (float(height), float(width));
        let up = |pixels: f64| whole(millionths(pixels).ceil());
        let down = |pixels: f64| whole(millionths(pixels).floor()).max(1);
        let to = match fit {
            Fit::Expand => (up(w * sin + h * cos), up(w * cos + h * sin)),
            Fit::Crop => {
                let scale = (1.0 / (cos + h / w * sin)).min(1.0 / (w / h * sin + cos));
                (down(h * scale), down(w * scale))
            }
        };
        Self {
            from: (height, width),
            to,
            angle,
            fit,
            resample,
        }
    }

    /// The number of rows of the turned picture.
    pub(crate) const fn height(&self) -> usize {
        self.to.0
    }

    /// The number of columns of the turned picture.
    pub(crate) const fn width(&self) -> usize {
        self.to.1
    }

    /// `values`, the samples of a picture of the planned size, turned: `fill` (one value for each channel) shows where the
    /// turned picture has nothing of the original, softly over a pixel at its edge.
    pub(crate) fn apply(&self, values: &[f32], channels: usize, fill: &[f32]) -> Vec<f32> {
        let shape = Shape {
            height: self.from.0,
            width: self.from.1,
            channels,
        };
        if self.angle % QUARTER == 0.0 {
            return self.quarter(values, shape);
        }
        let mut turned = self.transformed(values, shape);
        if self.fit == Fit::Expand {
            self.soften_edge(&mut turned, shape, fill);
        }
        par::rows(&mut turned, self.to.1 * shape.channels, |_, band| {
            for sample in band {
                *sample = sample.clamp(0.0, 1.0);
            }
        });
        turned
    }

    /// A turn by a whole number of quarters: the samples moved, then, for `Crop`, the centred part that fits.
    fn quarter(&self, values: &[f32], shape: Shape) -> Vec<f32> {
        let Shape {
            height,
            width,
            channels,
        } = shape;
        let quarters = whole((self.angle / QUARTER).rem_euclid(TURN / QUARTER));
        let (rows, columns) = if quarters.is_multiple_of(2) {
            (height, width)
        } else {
            (width, height)
        };
        let mut turned = Vec::with_capacity(values.len());
        for row in 0..rows {
            for column in 0..columns {
                // where the sample came from
                let (from_row, from_column) = match quarters {
                    0 => (row, column),
                    1 => (height - 1 - column, row),
                    2 => (height - 1 - row, width - 1 - column),
                    // the last quarter turn, counter-clockwise
                    _ => (column, width - 1 - row),
                };
                turned.extend_from_slice(&values[(from_row * width + from_column) * channels..][..channels]);
            }
        }
        match self.fit {
            Fit::Expand => turned,
            Fit::Crop => {
                let rect = Rect {
                    top: (rows - self.to.0) / 2,
                    left: (columns - self.to.1) / 2,
                    height: self.to.0,
                    width: self.to.1,
                };
                window(
                    &turned,
                    Shape {
                        height: rows,
                        width: columns,
                        channels,
                    },
                    rect,
                )
            }
        }
    }

    /// The transform: where in the original each turned pixel's centre is read.
    fn affine(&self) -> Affine {
        let radians = -self.angle.to_radians(); // the matrix turns counter-clockwise by `radians`
        let (xx, xy, yx, yy) = (radians.cos(), -radians.sin(), radians.sin(), radians.cos());
        let (centre_x, centre_y) = (float(self.to.1) / 2.0, float(self.to.0) / 2.0);
        let (half_width, half_height) = (float(self.from.1) / 2.0, float(self.from.0) / 2.0);
        Affine {
            xx,
            xy,
            x0: half_width - xx * centre_x - xy * centre_y,
            yx,
            yy,
            y0: half_height - yx * centre_x - yy * centre_y,
        }
    }

    /// The turned picture, sampled; 0 where a pixel's centre falls outside the original.
    fn transformed(&self, values: &[f32], shape: Shape) -> Vec<f32> {
        let smooth = match self.resample {
            Resample::Nearest => return self.nearest(values, shape),
            Resample::Bilinear => Smooth::Linear,
            Resample::Bicubic => Smooth::Cubic,
        };
        let affine = self.affine();
        let mut turned = vec![0.0; self.to.0 * self.to.1 * shape.channels];
        par::rows(&mut turned, self.to.1 * shape.channels, |first_row, band| {
            for (row, out_row) in band.chunks_exact_mut(self.to.1 * shape.channels).enumerate() {
                for (column, out) in out_row.chunks_exact_mut(shape.channels).enumerate() {
                    let (from_x, from_y) = affine.at(float(column) + 0.5, float(first_row + row) + 0.5);
                    for (channel, out) in out.iter_mut().enumerate() {
                        *out = sample(values, shape, smooth, from_x, from_y, channel);
                    }
                }
            }
        });
        turned
    }

    /// The turned picture with each pixel the original pixel its centre falls in, in whole numbers of 1/65536 of a pixel
    /// as Pillow does it, the origin moved to the first pixel's centre.
    fn nearest(&self, values: &[f32], shape: Shape) -> Vec<f32> {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a position in 1/65536 of a pixel inside a picture fits"
        )]
        let fixed = |value: f64| (value * FIXED + 0.5).floor() as i64;
        let moved = self.affine();
        let (xx, xy, yx, yy) = (fixed(moved.xx), fixed(moved.xy), fixed(moved.yx), fixed(moved.yy));
        let x0 = fixed(moved.x0 + moved.xx / 2.0 + moved.xy / 2.0);
        let y0 = fixed(moved.y0 + moved.yx / 2.0 + moved.yy / 2.0);
        let mut turned = vec![0.0; self.to.0 * self.to.1 * shape.channels];
        par::rows(&mut turned, self.to.1 * shape.channels, |first_row, band| {
            for (row, out_row) in band.chunks_exact_mut(self.to.1 * shape.channels).enumerate() {
                for (column, out) in out_row.chunks_exact_mut(shape.channels).enumerate() {
                    let (x, y) = (
                        i64::try_from(column).unwrap_or(i64::MAX),
                        i64::try_from(first_row + row).unwrap_or(i64::MAX),
                    );
                    let (from_x, from_y) = (
                        (x0 + x * xx + y * xy) >> FIXED_BITS,
                        (y0 + x * yx + y * yy) >> FIXED_BITS,
                    );
                    let inside = usize::try_from(from_x).ok().zip(usize::try_from(from_y).ok());
                    // a pixel whose centre falls outside the picture stays 0
                    if let Some((x, y)) = inside.filter(|&(x, y)| x < shape.width && y < shape.height) {
                        out.copy_from_slice(&values[(y * shape.width + x) * shape.channels..][..shape.channels]);
                    }
                }
            }
        });
        turned
    }

    /// `turned` with `fill` mixed in over the pixel at the turned picture's edge: how far inside the original's edge a
    /// pixel's centre lies, in pixels, plus a half, from 0 to 1, is how much of the pixel is the picture.
    fn soften_edge(&self, turned: &mut [f32], shape: Shape, fill: &[f32]) {
        let (width, height) = (float(self.from.1), float(self.from.0));
        let (centre_x, centre_y) = (float(self.to.1) / 2.0, float(self.to.0) / 2.0);
        // about the turned picture's centre, which the original's centre is carried to
        let about_centre = Affine {
            x0: width / 2.0,
            y0: height / 2.0,
            ..self.affine()
        };
        par::rows(turned, self.to.1 * shape.channels, |first_row, band| {
            for (row, out_row) in band.chunks_exact_mut(self.to.1 * shape.channels).enumerate() {
                for (column, pixel) in out_row.chunks_exact_mut(shape.channels).enumerate() {
                    let (from_x, from_y) =
                        about_centre.at(float(column) + 0.5 - centre_x, float(first_row + row) + 0.5 - centre_y);
                    let inside = from_x.min(width - from_x).min(from_y.min(height - from_y));
                    #[expect(clippy::cast_possible_truncation, reason = "a share from 0 to 1 is stored as f32")]
                    let covered = (inside + 0.5).clamp(0.0, 1.0) as f32;
                    for (sample, fill) in pixel.iter_mut().zip(fill) {
                        *sample = *sample * covered + fill * (1.0 - covered);
                    }
                }
            }
        });
    }
}

/// `pixels` to a millionth of a pixel.
fn millionths(pixels: f64) -> f64 {
    (pixels * PARTS).round_ties_even() / PARTS
}

/// Where in the original (pixel-corner coordinates) a pixel of the turned picture whose centre is `(x, y)` is read:
/// `(xx x + xy y + x0, yx x + yy y + y0)`.
#[derive(Clone, Copy)]
struct Affine {
    xx: f64,
    xy: f64,
    x0: f64,
    yx: f64,
    yy: f64,
    y0: f64,
}

impl Affine {
    fn at(&self, x: f64, y: f64) -> (f64, f64) {
        (self.xx * x + self.xy * y + self.x0, self.yx * x + self.yy * y + self.y0)
    }
}

/// How a position between pixels is read: from the four pixels around it or the sixteen.
#[derive(Clone, Copy)]
enum Smooth {
    Linear,
    Cubic,
}

/// The value of `channel` of the picture `values` at `(x, y)` in pixel-corner coordinates; 0 outside the picture.
fn sample(values: &[f32], shape: Shape, smooth: Smooth, x: f64, y: f64, channel: usize) -> f32 {
    if x < 0.0 || x >= float(shape.width) || y < 0.0 || y >= float(shape.height) {
        return 0.0;
    }
    // the sample at a whole pixel, the position held to the picture's edge
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "held to a pixel of the picture"
    )]
    let at = |column: f64, row: f64| -> f64 {
        let column = column.clamp(0.0, float(shape.width - 1)) as usize;
        let row = row.clamp(0.0, float(shape.height - 1)) as usize;
        f64::from(values[(row * shape.width + column) * shape.channels + channel])
    };
    // the pixel whose centre is at or before the position, and how far past it (0 to 1)
    let (column, row) = ((x - 0.5).floor(), (y - 0.5).floor());
    let (across, down) = (x - 0.5 - column, y - 0.5 - row);
    let value = match smooth {
        Smooth::Linear => {
            let line = |row: f64| at(column, row) + (at(column + 1.0, row) - at(column, row)) * across;
            line(row) + (line(row + 1.0) - line(row)) * down
        }
        Smooth::Cubic => {
            let line = |row: f64| {
                cubic(
                    at(column - 1.0, row),
                    at(column, row),
                    at(column + 1.0, row),
                    at(column + 2.0, row),
                    across,
                )
            };
            cubic(line(row - 1.0), line(row), line(row + 1.0), line(row + 2.0), down)
        }
    };
    #[expect(clippy::cast_possible_truncation, reason = "the sample is stored as f32")]
    let stored = value as f32;
    stored
}

/// The cubic through four samples at `-1, 0, 1, 2` read at `position` (0 to 1) between the middle two: Keys' kernel at
/// `a = -1`, which is what Pillow's transform uses.
fn cubic(first: f64, second: f64, third: f64, fourth: f64, position: f64) -> f64 {
    let (p2, p3, p4) = (
        third - first,
        2.0 * (first - second) + third - fourth,
        second - first - third + fourth,
    );
    second + position * (p2 + position * (p3 + position * p4))
}

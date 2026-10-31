//! The pixel geometry of framing a picture: mirroring, the box a crop keeps, and borders.
//!
//! Every function works on interleaved `f32` samples (`channels` of them for each pixel, pixel after pixel, row after
//! row) and on whole pixels: the caller turns shares of the picture into pixels. A picture and the maps of it go through
//! the same boxes, one channel for a map and three for the photo, so they stay on the same pixels. Positions and sizes
//! are rounded as Python's `round` is: the nearest whole pixel, halves to the even one.

use super::length::{float, whole};

/// The size and layout of what a function is given: `height` rows of `width` pixels of `channels` samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shape {
    pub(crate) height: usize,
    pub(crate) width: usize,
    pub(crate) channels: usize,
}

/// A rectangle of a picture, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) top: usize,
    pub(crate) left: usize,
    pub(crate) height: usize,
    pub(crate) width: usize,
}

/// Where in a picture a crop of another shape sits: in the middle, or against one side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Center,
    Top,
    Bottom,
    Left,
    Right,
}

/// Pixels added to each edge of a picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Edges {
    pub(crate) top: usize,
    pub(crate) right: usize,
    pub(crate) bottom: usize,
    pub(crate) left: usize,
}

impl Edges {
    /// The edges that are the given shares (top, right, bottom, left) of `short`, the picture's shorter side.
    pub(crate) fn of_shares(top: f64, right: f64, bottom: f64, left: f64, short: usize) -> Self {
        let pixels = |share: f64| whole(share * float(short));
        Self {
            top: pixels(top),
            right: pixels(right),
            bottom: pixels(bottom),
            left: pixels(left),
        }
    }

    /// Whether no edge grows.
    pub(crate) const fn is_zero(self) -> bool {
        self.top == 0 && self.right == 0 && self.bottom == 0 && self.left == 0
    }
}

/// `values` mirrored left to right (`across`) and/or top to bottom (`down`).
pub(crate) fn flip(values: &[f32], shape: Shape, across: bool, down: bool) -> Vec<f32> {
    let Shape {
        height,
        width,
        channels,
    } = shape;
    let mut out = Vec::with_capacity(values.len());
    for row in 0..height {
        let from = if down { height - 1 - row } else { row };
        let line = &values[from * width * channels..][..width * channels];
        if across {
            out.extend(line.rchunks_exact(channels).flatten());
        } else {
            out.extend_from_slice(line);
        }
    }
    out
}

/// The part of `values` that `rect` covers.
pub(crate) fn window(values: &[f32], shape: Shape, rect: Rect) -> Vec<f32> {
    let Shape { width, channels, .. } = shape;
    let mut out = Vec::with_capacity(rect.height * rect.width * channels);
    for row in rect.top..rect.top + rect.height {
        out.extend_from_slice(&values[(row * width + rect.left) * channels..][..rect.width * channels]);
    }
    out
}

/// `values` on a larger canvas with `edges` added, each channel of the added pixels set to its value in `fill` (one
/// per channel).
pub(crate) fn pad(values: &[f32], shape: Shape, edges: Edges, fill: &[f32]) -> Vec<f32> {
    let Shape {
        height,
        width,
        channels,
    } = shape;
    let wide = width + edges.left + edges.right;
    let border = |pixels: usize| fill.iter().copied().cycle().take(pixels * channels);
    let mut out = Vec::with_capacity((height + edges.top + edges.bottom) * wide * channels);
    out.extend(border(edges.top * wide));
    for line in values.chunks_exact(width * channels) {
        out.extend(border(edges.left));
        out.extend_from_slice(line);
        out.extend(border(edges.right));
    }
    out.extend(border(edges.bottom * wide));
    out
}

/// The box around the subject of `map` (`height` x `width`; the pixels at one half or more), with `margin` (a share of
/// the box's longer side) added on every side and kept inside the picture. None when no pixel reaches one half.
pub(crate) fn subject_box(map: &[f32], height: usize, width: usize, margin: f64) -> Option<Rect> {
    // the rows and columns of the first and last pixel at one half or more
    let mut edges: Option<(usize, usize, usize, usize)> = None;
    for index in (0..height * width).filter(|&index| map[index] >= 0.5) {
        let (row, column) = (index / width, index % width);
        edges = Some(match edges {
            None => (row, row, column, column),
            Some((top, bottom, left, right)) => (top.min(row), bottom.max(row), left.min(column), right.max(column)),
        });
    }
    let (top, bottom, left, right) = edges?;
    let (bottom, right) = (bottom + 1, right + 1);
    let extra = whole(margin * float((bottom - top).max(right - left)));
    let (top, left) = (top.saturating_sub(extra), left.saturating_sub(extra));
    let (bottom, right) = ((bottom + extra).min(height), (right + extra).min(width));
    Some(Rect {
        top,
        left,
        height: bottom - top,
        width: right - left,
    })
}

/// The largest part of a `height` x `width` picture whose width over height is `aspect`, in the middle or against the
/// `side` (a rectangle that fills the picture one way has no room to move that way).
pub(crate) fn crop_box(height: usize, width: usize, aspect: f64, side: Side) -> Rect {
    let (crop_width, crop_height) = largest(aspect, width, height);
    // the share of the free room before the crop: 0 at the start, a half in the middle, 1 at the end
    let across = match side {
        Side::Left => 0.0,
        Side::Right => 1.0,
        Side::Center | Side::Top | Side::Bottom => 0.5,
    };
    let down = match side {
        Side::Top => 0.0,
        Side::Bottom => 1.0,
        Side::Center | Side::Left | Side::Right => 0.5,
    };
    Rect {
        top: whole(float(height - crop_height) * down),
        left: whole(float(width - crop_width) * across),
        height: crop_height,
        width: crop_width,
    }
}

/// The smallest part of a `height` x `width` picture that holds the `subject` box and has width over height `aspect`,
/// centred on the subject and moved inside the picture; when the picture is too small to hold it at that aspect, the
/// largest such part that fits, centred on it. Without an `aspect`, the subject's box itself.
pub(crate) fn crop_around(height: usize, width: usize, subject: Rect, aspect: Option<f64>) -> Rect {
    let Some(aspect) = aspect else { return subject };
    let (mut crop_width, mut crop_height) = if float(subject.width) < aspect * float(subject.height) {
        (whole((aspect * float(subject.height)).ceil()), subject.height)
    } else {
        (subject.width, whole((float(subject.width) / aspect).ceil()))
    };
    if crop_width > width || crop_height > height {
        (crop_width, crop_height) = largest(aspect, width, height);
    }
    // the crop's start along one axis: its centre on the subject's, moved inside the picture (a start before 0 is 0)
    let centred = |start: usize, length: usize, crop: usize, room: usize| {
        whole(float(start) + (float(length) - float(crop)) / 2.0).min(room - crop)
    };
    Rect {
        top: centred(subject.top, subject.height, crop_height, height),
        left: centred(subject.left, subject.width, crop_width, width),
        height: crop_height,
        width: crop_width,
    }
}

/// The width and height of the largest rectangle of `aspect` (width over height) inside `width` x `height`.
fn largest(aspect: f64, width: usize, height: usize) -> (usize, usize) {
    if float(width) > float(height) * aspect {
        (whole(float(height) * aspect).max(1), height)
    } else {
        (width, whole(float(width) / aspect).max(1))
    }
}

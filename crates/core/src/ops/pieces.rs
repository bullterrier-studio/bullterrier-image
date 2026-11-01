//! Pieces: numbering the parts a region cuts a picture into, and what is made from the numbers.
//!
//! A partition numbers its pieces from 0 with no number unused, and, except bands, in reading order: piece `k + 1` is
//! the one whose first pixel comes later in the raster scan (left to right along each row, rows from the top) than the
//! first pixel of piece `k`.

use super::length;

/// A rectangle of a picture, in pixels: columns `left..right` of rows `top..bottom`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub(crate) left: usize,
    pub(crate) top: usize,
    pub(crate) right: usize,
    pub(crate) bottom: usize,
}

impl Bounds {
    /// The whole of a `height` x `width` picture.
    pub(crate) const fn whole(height: usize, width: usize) -> Self {
        Self {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    }
}

/// The labels of pixels that `raw` gives a kind each (a number below `kinds`), renumbered by the first pixel of each
/// kind in the raster scan, and the number of pieces. Kinds no pixel has leave no gap.
pub(crate) fn renumber(kinds: usize, raw: impl Iterator<Item = usize>) -> (Vec<i32>, usize) {
    let mut numbered: Vec<Option<i32>> = vec![None; kinds];
    let mut next = 0_i32; // constant: pieces are numbered from 0
    let labels = raw
        .map(|kind| {
            *numbered[kind].get_or_insert_with(|| {
                next += 1;
                next - 1
            })
        })
        .collect();
    (labels, count(next))
}

/// A number of pieces, counted as the `i32` labels are.
#[expect(clippy::cast_sign_loss, reason = "a count of pieces is not negative")]
pub(crate) const fn count(pieces: i32) -> usize {
    pieces as usize
}

/// The piece a label names, for indexing: labels are never negative (see [`Labels`](crate::Labels)).
#[expect(clippy::cast_sign_loss, reason = "a label is not negative")]
pub(crate) const fn piece(label: i32) -> usize {
    label as usize
}

/// Whether a pixel the selection chooses to the degree `value` is chosen: more than half.
pub(crate) fn chosen(value: f32) -> bool {
    value > 0.5
}

/// The two pieces of a hard-edged selection: the pixels it chooses (more than half) and the others, numbered by first
/// pixel; one piece if it chooses all or none.
pub(crate) fn of_selection(values: &[f32]) -> (Vec<i32>, usize) {
    renumber(2, values.iter().map(|&value| usize::from(chosen(value))))
}

/// The smallest rectangle that holds every pixel `values` (a `height` x `width` selection) chooses; none if it chooses
/// no pixel.
pub(crate) fn bounds(values: &[f32], height: usize, width: usize) -> Option<Bounds> {
    let mut found: Option<Bounds> = None;
    for (row, line) in values.chunks_exact(width).enumerate().take(height) {
        let (Some(first), Some(last)) = (
            line.iter().position(|&value| chosen(value)),
            line.iter().rposition(|&value| chosen(value)),
        ) else {
            continue;
        };
        found = Some(found.map_or(
            Bounds {
                left: first,
                top: row,
                right: last + 1,
                bottom: row + 1,
            },
            |so_far| Bounds {
                left: so_far.left.min(first),
                right: so_far.right.max(last + 1),
                bottom: row + 1,
                ..so_far
            },
        ));
    }
    found
}

/// The selection of the pixels whose piece is one of `numbers`: 1 there, 0 elsewhere. A number the partition (of
/// `pieces` pieces) has no piece for chooses nothing.
pub(crate) fn pick(labels: &[i32], pieces: usize, numbers: &[u32]) -> Vec<f32> {
    let mut picked = vec![false; pieces];
    for number in numbers {
        if let Some(slot) = picked.get_mut(*number as usize) {
            *slot = true;
        }
    }
    labels
        .iter()
        .map(|&label| f32::from(u8::from(picked[piece(label)])))
        .collect()
}

/// The smallest rectangle round each of the `pieces` pieces of `labels` (a `height` x `width` partition), piece by piece.
/// Every piece of a partition the core makes has a pixel (see the module's note: no number is unused), so every rectangle
/// exists and the rectangles are numbered as the pieces are; a piece without a pixel would shift the numbers.
pub(crate) fn piece_bounds(labels: &[i32], height: usize, width: usize, pieces: usize) -> Vec<Bounds> {
    let mut boxes: Vec<Option<Bounds>> = vec![None; pieces];
    for (row, line) in labels.chunks_exact(width).enumerate().take(height) {
        for (column, &label) in line.iter().enumerate() {
            let slot = &mut boxes[piece(label)];
            *slot = Some(slot.map_or(
                Bounds {
                    left: column,
                    top: row,
                    right: column + 1,
                    bottom: row + 1,
                },
                |so_far| Bounds {
                    left: so_far.left.min(column),
                    right: so_far.right.max(column + 1),
                    bottom: row + 1,
                    ..so_far
                },
            ));
        }
    }
    boxes.into_iter().flatten().collect()
}

/// `window` grown on each side by `context` times its own extent that way (rounded to whole pixels, halves to the even
/// one), and clipped to the `height` x `width` picture.
pub(crate) fn grown(window: Bounds, context: f64, height: usize, width: usize) -> Bounds {
    let across = length::whole(context * length::float(window.right - window.left));
    let down = length::whole(context * length::float(window.bottom - window.top));
    Bounds {
        left: window.left.saturating_sub(across),
        top: window.top.saturating_sub(down),
        right: width.min(window.right.saturating_add(across)),
        bottom: height.min(window.bottom.saturating_add(down)),
    }
}

/// The pixels of `window` in a picture `width` pixels wide whose pixels are `pixels`, row after row.
pub(crate) fn crop(pixels: &[[f32; 3]], width: usize, window: Bounds) -> Vec<[f32; 3]> {
    let mut out = Vec::with_capacity((window.right - window.left) * (window.bottom - window.top));
    for row in window.top..window.bottom {
        out.extend_from_slice(&pixels[row * width + window.left..row * width + window.right]);
    }
    out
}

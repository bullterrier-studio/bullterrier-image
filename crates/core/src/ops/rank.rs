//! Rank filters: [`minimum_filter`] and [`maximum_filter`] replace each value by the smallest or the largest in the square
//! around it, as scipy's `ndimage.minimum_filter` and `maximum_filter` do.

use super::blur::transposed;
use super::edge::Extension;
use super::par;

/// The `height` x `width` map `values` (row after row) with every value replaced by the smallest of the `size` x `size`
/// values around it, `edge` saying what lies beyond the map. The window of a pixel at `(row, column)` spans rows `row -
/// size / 2` to `row + size - size / 2 - 1` (the same for columns), so an odd `size` is centred on the pixel and an even
/// one is half a pixel off, toward the start. The result takes values of the map only, so it stays within its range; a
/// `size` of 1 leaves the map as it is. `size` is at least 1 and the map is not empty.
///
/// The time is proportional to the pixels and does not grow with `size`: the minimum of every window is the smaller of
/// two running minima taken from the middle of the window outward (van Herk, and Gil and Werman, 1992 and 1993).
pub(crate) fn minimum_filter(values: &[f32], height: usize, width: usize, size: usize, edge: Extension) -> Vec<f32> {
    let mut along = values.to_vec();
    smooth_rows(&mut along, width, size, edge);
    let mut down = transposed(&along, height, width); // `width` rows of `height`
    drop(along);
    smooth_rows(&mut down, height, size, edge);
    transposed(&down, width, height)
}

/// [`minimum_filter`] with the largest value of each window in place of the smallest.
pub(crate) fn maximum_filter(values: &[f32], height: usize, width: usize, size: usize, edge: Extension) -> Vec<f32> {
    // the largest of a window is the smallest of the negated values, negated back (exact in floating point)
    let negated: Vec<f32> = values.iter().map(|value| -value).collect();
    let edge = match edge {
        Extension::Constant(value) => Extension::Constant(-value),
        other => other,
    };
    let mut out = minimum_filter(&negated, height, width, size, edge);
    for value in &mut out {
        *value = -*value;
    }
    out
}

/// Each line of `len` values in `lines` replaced by the smallest value in the `size` values around each of its own.
fn smooth_rows(lines: &mut [f32], len: usize, size: usize, edge: Extension) {
    par::rows(lines, len, |_, band| {
        let mut scratch = Scratch::default();
        for line in band.chunks_exact_mut(len) {
            scratch.minimum(line, size, edge);
        }
    });
}

/// The buffers one line's filter needs, kept between lines.
#[derive(Default)]
struct Scratch {
    /// The line with the window's reach beyond each end.
    extended: Vec<f32>,
    /// For each value of `extended`, the smallest from the start of its block of `size` values up to it.
    from_block_start: Vec<f32>,
    /// The same, from it to the end of its block.
    to_block_end: Vec<f32>,
}

impl Scratch {
    /// `line` with every value replaced by the smallest of the `size` values around it. A window of `size` values
    /// starting at `at` covers either one block of the extended line (when `at` is a multiple of `size`) or the end of
    /// one block and the start of the next, so its minimum is the smaller of two running minima.
    fn minimum(&mut self, line: &mut [f32], size: usize, edge: Extension) {
        let Self {
            extended,
            from_block_start,
            to_block_end,
        } = self;
        // the window of the value at `i` is the extended values `i..i + size`
        edge.extend(line, size / 2, extended);
        from_block_start.clear();
        from_block_start.extend_from_slice(extended);
        to_block_end.clear();
        to_block_end.extend_from_slice(extended);
        for at in 1..extended.len() {
            if at % size != 0 {
                from_block_start[at] = from_block_start[at].min(from_block_start[at - 1]);
            }
        }
        for at in (0..extended.len() - 1).rev() {
            if (at + 1) % size != 0 {
                to_block_end[at] = to_block_end[at].min(to_block_end[at + 1]);
            }
        }
        for (at, value) in line.iter_mut().enumerate() {
            *value = to_block_end[at].min(from_block_start[at + size - 1]);
        }
    }
}

#[cfg(test)]
mod tests {
    //! The filter against the smallest value of each window found by looking at every value in it, where the picture's
    //! edge is found by folding an index back into the line one bounce at a time (the filter finds it by modular
    //! arithmetic). The filter is private to the core, and the effect that uses it shows only a combination of it with others.

    use super::{maximum_filter, minimum_filter};
    use crate::ops::edge::Extension;

    /// A different value at nearly every place, 0..1.
    fn values(height: usize, width: usize) -> Vec<f32> {
        (0..height * width)
            .map(|index| f32::from(u16::try_from((index * 7919 + index / 7) % 997).unwrap_or(0)) / 997.0)
            .collect()
    }

    /// Where the sample at `index` of a line of `len` comes from, by bouncing back from the ends one at a time; none for
    /// the constant rule beyond the line.
    fn source(rule: Extension, mut index: isize, len: isize) -> Option<usize> {
        let inside = |index: isize| (0..len).contains(&index);
        let found = match rule {
            Extension::Constant(_) => return inside(index).then(|| usize::try_from(index).unwrap_or(0)),
            Extension::Nearest => index.clamp(0, len - 1),
            Extension::Wrap => {
                while !inside(index) {
                    index += if index < 0 { len } else { -len };
                }
                index
            }
            Extension::Reflect => {
                while !inside(index) {
                    index = if index < 0 { -index - 1 } else { 2 * len - 1 - index };
                }
                index
            }
            Extension::Mirror => {
                while len > 1 && !inside(index) {
                    index = if index < 0 { -index } else { 2 * (len - 1) - index };
                }
                if len > 1 { index } else { 0 }
            }
        };
        usize::try_from(found).ok()
    }

    /// The smallest (or the `largest`) value in the `size` x `size` window around every place, by looking at all of them.
    fn brute_force(
        values: &[f32],
        height: usize,
        width: usize,
        size: usize,
        rule: Extension,
        largest: bool,
    ) -> Vec<f32> {
        let (start, keep): (f32, fn(f32, f32) -> f32) = if largest {
            (f32::NEG_INFINITY, f32::max)
        } else {
            (f32::INFINITY, f32::min)
        };
        let (h, w, s) = (height.cast_signed(), width.cast_signed(), size.cast_signed());
        let mut out = Vec::new();
        for row in 0..h {
            for column in 0..w {
                let mut kept = start;
                for y in row - s / 2..row + s - s / 2 {
                    for x in column - s / 2..column + s - s / 2 {
                        let at = source(rule, y, h).zip(source(rule, x, w));
                        let beyond = if let Extension::Constant(value) = rule {
                            value
                        } else {
                            start
                        };
                        kept = keep(kept, at.map_or(beyond, |(y, x)| values[y * width + x]));
                    }
                }
                out.push(kept);
            }
        }
        out
    }

    const RULES: [Extension; 5] = [
        Extension::Reflect,
        Extension::Mirror,
        Extension::Nearest,
        Extension::Wrap,
        Extension::Constant(0.5),
    ];

    #[test]
    fn the_extreme_is_that_of_every_window_for_every_rule_size_and_shape() {
        // pictures from one pixel to a few, thin ones, with windows from 1 to far wider than the picture, even and odd
        let shapes = [(1, 1), (1, 7), (7, 1), (2, 3), (3, 2), (5, 4), (4, 9), (9, 4), (11, 13)];
        for rule in RULES {
            for (height, width) in shapes {
                let picture = values(height, width);
                for size in 1..=17 {
                    for (name, largest, filter) in [
                        (
                            "minimum",
                            false,
                            minimum_filter as fn(&[f32], usize, usize, usize, Extension) -> Vec<f32>,
                        ),
                        ("maximum", true, maximum_filter),
                    ] {
                        let found = filter(&picture, height, width, size, rule);
                        let expected = brute_force(&picture, height, width, size, rule, largest);
                        assert!(
                            found == expected,
                            "{name}, {rule:?}, {height} x {width}, size {size}: first differs at {:?}",
                            found
                                .iter()
                                .zip(&expected)
                                .position(|(a, b)| a.to_bits() != b.to_bits())
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_comparison_tells_a_filter_that_differs_apart() {
        // the known-bad pair: another rule, another size, and a window moved half a pixel are not the same minimum
        let picture = values(5, 6);
        let found = brute_force(&picture, 5, 6, 4, Extension::Reflect, false);
        assert_ne!(found, brute_force(&picture, 5, 6, 4, Extension::Wrap, false));
        assert_ne!(found, brute_force(&picture, 5, 6, 5, Extension::Reflect, false));
        assert_ne!(found, minimum_filter(&picture, 5, 6, 4, Extension::Mirror));
        assert_ne!(found, brute_force(&picture, 5, 6, 4, Extension::Reflect, true));
    }

    #[test]
    fn a_window_of_one_leaves_the_map_and_a_constant_map_stays() {
        let picture = values(6, 5);
        assert_eq!(minimum_filter(&picture, 6, 5, 1, Extension::Wrap), picture);
        for rule in RULES.iter().take(4) {
            assert_eq!(minimum_filter(&[0.3; 35], 5, 7, 6, *rule), [0.3; 35]);
        }
    }

    #[test]
    fn a_lone_dark_pixel_grows_to_the_window_and_its_place_decides_which_side_gets_more() {
        // 7 x 7 white, one black pixel at (3, 3); a window of 4 around (r, c) holds rows r-2..=r+1: it holds the black
        // pixel for r in 2..=5 (and c the same), not for r 1 or 6
        let mut picture = vec![1.0_f32; 49];
        picture[3 * 7 + 3] = 0.0;
        let out = minimum_filter(&picture, 7, 7, 4, Extension::Nearest);
        for row in 0..7 {
            for column in 0..7 {
                let dark = (2..=5).contains(&row) && (2..=5).contains(&column);
                assert!(
                    (out[row * 7 + column] - f32::from(u8::from(!dark))).abs() < f32::EPSILON,
                    "({row}, {column})"
                );
            }
        }
    }
}

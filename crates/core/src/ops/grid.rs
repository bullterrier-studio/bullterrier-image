//! A grid: rectangular pieces in columns and rows of given shares.

use super::length;
use super::pieces::{Bounds, renumber};

/// The positions in px, from `low` to `high`, of the borders between pieces of the relative widths `shares`.
fn borders(shares: &[f64], low: usize, high: usize) -> Vec<f64> {
    let (low, high) = (length::float(low), length::float(high));
    let total: f64 = shares.iter().sum();
    let mut so_far = 0.0; // constant: no share has been taken before the first
    shares
        .iter()
        .take(shares.len().saturating_sub(1))
        .map(|share| {
            so_far += share;
            low + (high - low) * so_far / total
        })
        .collect()
}

/// How many of the sorted `borders` lie at or before each pixel centre of `count` pixels (centre `i + 0.5`): the
/// column, or row, of the pixel. A centre exactly on a border is past it, in the later piece.
fn places(borders: &[f64], count: usize) -> Vec<usize> {
    (0..count)
        .map(|at| {
            let centre = length::float(at) + 0.5;
            borders.partition_point(|border| *border <= centre)
        })
        .collect()
}

/// The pieces of a `height` x `width` picture divided into columns of the relative widths `columns` across the
/// rectangle `frame` and rows of the relative heights `rows` down it, and the number of pieces: a pixel belongs to the
/// column and row whose range holds its centre, and the outer pieces run on to the edges of the picture. Numbered by
/// first pixel, so row after row, among those that have a pixel.
///
/// With `shift` (the shifts of the pixel centres along x and along y, row after row), the centres are moved before they
/// are classified, so the borders between pieces wander.
pub(crate) fn grid(
    height: usize,
    width: usize,
    frame: Bounds,
    columns: &[f64],
    rows: &[f64],
    shift: Option<[&[f32]; 2]>,
) -> (Vec<i32>, usize) {
    let (across, down) = (
        borders(columns, frame.left, frame.right),
        borders(rows, frame.top, frame.bottom),
    );
    let Some([along_x, along_y]) = shift else {
        let (across, down) = (places(&across, width), places(&down, height));
        return renumber(
            columns.len() * rows.len(),
            down.iter()
                .flat_map(|&row| across.iter().map(move |&column| row * columns.len() + column)),
        );
    };
    renumber(
        columns.len() * rows.len(),
        (0..height * width).map(|at| {
            let (row, column) = (at / width, at % width);
            let x = length::float(column) + 0.5 + f64::from(along_x[at]); // constant: a pixel's centre is half a pixel in
            let y = length::float(row) + 0.5 + f64::from(along_y[at]); // constant: a pixel's centre is half a pixel in
            down.partition_point(|border| *border <= y) * columns.len() + across.partition_point(|border| *border <= x)
        }),
    )
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{Bounds, grid};

    /// The labels of a `rows` x `columns`-cell grid over a `height` x `width` picture, shifted by `x` and `y` everywhere.
    fn shifted(height: usize, width: usize, cells: (usize, usize), x: f32, y: f32) -> Vec<i32> {
        let (along_x, along_y) = (vec![x; height * width], vec![y; height * width]);
        let (columns, rows) = (vec![1.0; cells.1], vec![1.0; cells.0]);
        grid(
            height,
            width,
            Bounds::whole(height, width),
            &columns,
            &rows,
            Some([&along_x, &along_y]),
        )
        .0
    }

    #[test]
    fn a_shift_moves_the_centres_before_they_are_classified_and_a_centre_on_a_border_is_past_it() {
        // 6 x 1, two columns, border at 3: the centres are 0.5 ... 5.5; moved by +0.5, pixel 2 is at 3.0, on the border
        assert_eq!(shifted(1, 6, (1, 2), 0.5, 0.0), [0, 0, 1, 1, 1, 1]);
        assert_eq!(shifted(1, 6, (1, 2), 0.0, 0.0), [0, 0, 0, 1, 1, 1]);
        assert_eq!(shifted(1, 6, (1, 2), -0.5, 0.0), [0, 0, 0, 1, 1, 1]);
        assert_eq!(shifted(1, 6, (1, 2), -0.75, 0.0), [0, 0, 0, 0, 1, 1]);
        // shifted far beyond the picture: one piece
        assert_eq!(shifted(1, 6, (1, 2), 100.0, 0.0), [0; 6]);
    }

    #[test]
    fn the_first_shift_moves_the_columns_and_the_second_the_rows() {
        // 4 x 4, 2 x 2 cells, borders at 2: the first shift of 1 puts the centres 1.5 ... 4.5 to 2.5 ... 5.5 past the
        // border except the first
        assert_eq!(
            shifted(4, 4, (2, 2), 1.0, 0.0),
            [0, 1, 1, 1, 0, 1, 1, 1, 2, 3, 3, 3, 2, 3, 3, 3]
        );
        assert_eq!(
            shifted(4, 4, (2, 2), 0.0, 1.0),
            [0, 0, 1, 1, 2, 2, 3, 3, 2, 2, 3, 3, 2, 2, 3, 3]
        );
    }

    proptest! {
        #[test]
        fn a_zero_shift_gives_the_pieces_of_the_straight_grid(
            height in 1_usize..12,
            width in 1_usize..12,
            columns in proptest::collection::vec(0.1_f64..5.0, 1..5),
            rows in proptest::collection::vec(0.1_f64..5.0, 1..5),
        ) {
            let zero = vec![0.0_f32; height * width];
            let frame = Bounds::whole(height, width);
            prop_assert_eq!(
                grid(height, width, frame, &columns, &rows, Some([&zero, &zero])),
                grid(height, width, frame, &columns, &rows, None)
            );
        }
    }
}

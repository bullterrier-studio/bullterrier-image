//! A picture cut into a grid of equal cells (for example character-sized ones), and what can be said of each cell.
//!
//! Cells are sized in px, so this module knows nothing about fonts: callers pass the cell size. The grid is anchored at the
//! picture's top left corner; a cell cut off by the right or bottom edge counts the picture's pixels in it, as if the rest
//! were black.

use crate::ops::length::float;

/// The grid of `rows` x `columns` cells of `cell_width` x `cell_height` px that covers a `height` x `width` picture.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Grid {
    pub(crate) rows: usize,
    pub(crate) columns: usize,
    pub(crate) cell_width: usize,
    pub(crate) cell_height: usize,
    height: usize,
    width: usize,
}

impl Grid {
    /// The grid over a picture of `height` x `width` px; both and the cell's size (`cell_width`, `cell_height`) at least 1.
    pub(crate) fn new((height, width): (usize, usize), (cell_width, cell_height): (usize, usize)) -> Self {
        Self {
            rows: height.div_ceil(cell_height),
            columns: width.div_ceil(cell_width),
            cell_width,
            cell_height,
            height,
            width,
        }
    }

    /// The number of cells.
    pub(crate) const fn len(&self) -> usize {
        self.rows * self.columns
    }

    /// The `(row, column)` px of the picture that cell number `cell` (row after row) covers, left to right, top to bottom;
    /// none beyond the picture's edge.
    pub(crate) fn pixels(&self, cell: usize) -> impl Iterator<Item = (usize, usize)> + use<> {
        let (top, left) = (
            cell / self.columns * self.cell_height,
            cell % self.columns * self.cell_width,
        );
        let (bottom, right) = (
            (top + self.cell_height).min(self.height),
            (left + self.cell_width).min(self.width),
        );
        (top..bottom).flat_map(move |row| (left..right).map(move |column| (row, column)))
    }

    /// One flag per cell: whether some channel of some pixel in it is above `threshold` (0..1). Cells of black background
    /// are not worth moving.
    pub(crate) fn lit(&self, pixels: &[[f32; 3]], threshold: f32) -> Vec<bool> {
        (0..self.len())
            .map(|cell| {
                self.pixels(cell)
                    .any(|(row, column)| pixels[row * self.width + column].iter().any(|&level| level > threshold))
            })
            .collect()
    }

    /// One flag per cell: whether the average of `matte` (one value, 0..1, per pixel) over the cell is above `threshold`.
    /// At 0.5 more than half of a cell's pixels are inside; the part of a cell beyond the picture counts as outside, so a
    /// dark part of the figure still counts.
    pub(crate) fn figure(&self, matte: &[f32], threshold: f32) -> Vec<bool> {
        let area = float(self.cell_width * self.cell_height);
        (0..self.len())
            .map(|cell| {
                let sum: f64 = self
                    .pixels(cell)
                    .map(|(row, column)| f64::from(matte[row * self.width + column]))
                    .sum();
                sum / area > f64::from(threshold)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Grid;

    #[test]
    fn a_grid_covers_the_picture_with_whole_cells_and_each_pixel_once() {
        // 7 x 10 px in cells of 4 wide, 3 high: 3 rows and 3 columns, the last of each cut off
        let grid = Grid::new((7, 10), (4, 3));
        assert_eq!((grid.rows, grid.columns, grid.len()), (3, 3, 9));
        let mut seen = [0_u8; 70];
        for cell in 0..grid.len() {
            for (row, column) in grid.pixels(cell) {
                seen[row * 10 + column] += 1;
            }
        }
        assert!(seen.iter().all(|&times| times == 1));
        // the corner cell holds the one row and two columns left over
        assert_eq!(grid.pixels(8).collect::<Vec<_>>(), [(6, 8), (6, 9)]);
    }

    #[test]
    fn a_cell_is_lit_when_some_channel_of_some_pixel_is_above_the_threshold_and_not_at_it() {
        let grid = Grid::new((4, 4), (2, 2));
        let mut pixels = vec![[0.0_f32; 3]; 16];
        pixels[5] = [0.0, 0.3, 0.0]; // row 1, column 1: in cell 0
        pixels[15] = [0.2, 0.0, 0.0]; // row 3, column 3: in cell 3, exactly at the threshold
        assert_eq!(grid.lit(&pixels, 0.2), [true, false, false, false]);
    }

    #[test]
    fn a_cell_is_in_the_figure_when_the_matte_averages_above_the_threshold() {
        let grid = Grid::new((2, 5), (2, 2)); // three cells, the last 1 px wide of 2: half of it is beyond the picture
        let matte = [1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0];
        // cell 0 all inside, cell 1 a quarter... rows [1,0] and [0,0]: 1/4; cell 2 the column [1, 1] of a cell of 4: 2/4
        assert_eq!(grid.figure(&matte, 0.4), [true, false, true]);
        assert_eq!(grid.figure(&matte, 0.5), [true, false, false]);
    }
}

//! Rows of dashes laid on a canvas: the lattice of rows and cells, the tone of a cell, one-dimensional error diffusion along
//! a row, and the coverage a row paints on a pixel. `modulation` is the first user.
//!
//! The canvas is read in pixels with a pixel's corner at whole numbers (its centre at a half), rotated by an angle so that a
//! row runs along `u` and rows are `spacing` apart along `v`. A cell is `dash` long along `u` and one `spacing` wide across.

use super::length::float;
use super::par;

/// The picture a lattice is laid on and the numbers that shape it, all in canvas px.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shape {
    /// Degrees clockwise on screen from the x axis: how the rows run.
    pub(crate) angle: f64,
    /// The distance between rows.
    pub(crate) spacing: f64,
    /// The length of a cell along its row.
    pub(crate) dash: f64,
    /// A row's width as a share of `spacing`.
    pub(crate) thickness: f64,
    /// How far a row swings to either side of its line.
    pub(crate) sway: f64,
    /// The length of one swing.
    pub(crate) wavelength: f64,
    /// The canvas, rows then columns.
    pub(crate) canvas: (usize, usize),
}

/// The rows and cells that cover the canvas, and which cells are inked.
pub(crate) struct Rows {
    shape: Shape,
    sin: f64,
    cos: f64,
    /// The index of the first row, and of the first cell of every row.
    first: (i64, i64),
    lines: usize,
    cells: usize,
}

#[expect(clippy::cast_possible_truncation, reason = "a floor of a length on a canvas")]
fn floor(value: f64) -> i64 {
    value.floor() as i64
}

#[expect(clippy::cast_possible_truncation, reason = "a ceiling of a length on a canvas")]
fn ceil(value: f64) -> i64 {
    value.ceil() as i64
}

/// Taps along each side of a cell whose tone is the mean of their reads.
const TAPS: usize = 4; // constant: a 4 x 4 grid of reads averages the cell's area

impl Rows {
    /// The lattice for `shape`: the rows and cells that reach every corner of the canvas, a row more on each side for
    /// those a sway or an edge brings in.
    pub(crate) fn new(shape: Shape) -> Self {
        let turn = shape.angle.rem_euclid(360.0).to_radians(); // constant: a full turn in degrees
        let (sin, cos) = turn.sin_cos();
        let (height, width) = (float(shape.canvas.0), float(shape.canvas.1));
        let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)];
        let along = corners.map(|(x, y)| x * cos + y * sin);
        let across = corners.map(|(x, y)| y * cos - x * sin);
        let (low, high) = (
            along.iter().copied().fold(f64::MAX, f64::min),
            along.iter().copied().fold(f64::MIN, f64::max),
        );
        let (near, far) = (
            across.iter().copied().fold(f64::MAX, f64::min),
            across.iter().copied().fold(f64::MIN, f64::max),
        );
        let first_cell = floor(low / shape.dash);
        let first_row = floor((near - shape.sway) / shape.spacing) - 1;
        let last_row = ceil((far + shape.sway) / shape.spacing) + 1;
        let cells = ceil(high / shape.dash) - first_cell;
        Self {
            shape,
            sin,
            cos,
            first: (first_row, first_cell),
            lines: usize::try_from(last_row - first_row).unwrap_or(1),
            cells: usize::try_from(cells).unwrap_or(1).max(1),
        }
    }

    /// How far the row's line bends away from straight at `u`.
    fn bend(&self, u: f64) -> f64 {
        self.shape.sway * (u / self.shape.wavelength * std::f64::consts::TAU).sin()
    }

    /// Which cells are inked: row after row, `cells()` to a row. `darkness(x, y)` is how much ink the picture asks for at
    /// a canvas point (0..1); a cell inks as many dashes as that share of `thickness` needs, found by diffusing its error
    /// along the row, from the far end in rows `weave` turns round (every other one).
    pub(crate) fn inked(&self, weave: bool, darkness: impl Fn(f64, f64) -> f64 + Sync) -> Vec<bool> {
        let mut inked = vec![false; self.lines * self.cells];
        let Shape {
            dash,
            spacing,
            thickness,
            ..
        } = self.shape;
        par::rows(&mut inked, self.cells, |first, band| {
            let mut tones = vec![0.0f32; self.cells];
            for (offset, out) in band.chunks_exact_mut(self.cells).enumerate() {
                let row = self.first.0 + i64::try_from(first + offset).unwrap_or(0);
                for (cell, tone) in tones.iter_mut().enumerate() {
                    let j = self.first.1 + i64::try_from(cell).unwrap_or(0);
                    let mut sum = 0.0_f64; // constant: no tap has been read yet
                    for a in 0..TAPS {
                        let u = (float_i(j) + (float(a) + 0.5) / float(TAPS)) * dash;
                        for b in 0..TAPS {
                            let v = (float_i(row) + (float(b) + 0.5) / float(TAPS)) * spacing + self.bend(u);
                            sum += darkness(u * self.cos - v * self.sin, u * self.sin + v * self.cos);
                        }
                    }
                    #[expect(clippy::cast_possible_truncation, reason = "a tone")]
                    {
                        *tone = (sum / float(TAPS * TAPS) / thickness).min(1.0) as f32;
                    }
                }
                diffuse(&tones, weave && row.rem_euclid(2) == 1, out);
            }
        });
        inked
    }

    /// How much ink the rows paint on the pixel whose centre is the canvas point (`x`, `y`), 0..1: the row it lies in and
    /// the nearer neighbour, each a band of `thickness` x `spacing` with a one px ramp for an edge, along the cells of
    /// `inked` (from [`Rows::inked`]), a cell's end a ramp too, so neighbouring dashes join.
    #[expect(
        clippy::many_single_char_names,
        reason = "canvas coordinates x, y and the rotated u, v"
    )]
    pub(crate) fn coverage(&self, inked: &[bool], x: f64, y: f64) -> f32 {
        let Shape {
            spacing,
            dash,
            thickness,
            ..
        } = self.shape;
        let u = x * self.cos + y * self.sin;
        let v = y * self.cos - x * self.sin - self.bend(u);
        let place = v / spacing;
        let row = floor(place);
        let neighbour = if place - float_i(row) >= 0.5 { row + 1 } else { row - 1 }; // constant: the nearer row is within half a spacing
        let (cell, last) = (floor(u / dash), self.cells.saturating_sub(1));
        let at = |row: i64, cell: i64| -> f64 {
            let Ok(index) = usize::try_from(row - self.first.0) else {
                return 0.0;
            };
            if index >= self.lines {
                return 0.0;
            }
            let column = usize::try_from(cell - self.first.1).unwrap_or(0).min(last);
            f64::from(u8::from(inked[index * self.cells + column]))
        };
        let along = |row: i64| -> f64 {
            let (on, before, after) = (at(row, cell), at(row, cell - 1), at(row, cell + 1));
            let t = u - float_i(cell) * dash;
            let ramp = |share: f64| share.clamp(0.0, 1.0); // constant: a ramp is one px wide
            (on + (before - on) * ramp(0.5 - t) + (after - on) * ramp(t - dash + 0.5)).clamp(0.0, 1.0) // constant: the ramp is centred on the cell's end
        };
        let band = |row: i64| -> f64 {
            let off = (v - (float_i(row) + 0.5) * spacing).abs();
            (0.5 + thickness * spacing / 2.0 - off).clamp(0.0, 1.0) // constant: the ramp is centred on the edge
        };
        let covered = band(row) * along(row) + band(neighbour) * along(neighbour);
        #[expect(clippy::cast_possible_truncation, reason = "a share")]
        {
            covered.min(1.0) as f32
        }
    }
}

#[expect(clippy::cast_precision_loss, reason = "an index far below 2^53")]
const fn float_i(index: i64) -> f64 {
    index as f64
}

/// Which of `tones` (0..1, the share of a dash each wants) are inked, as error diffusion along one row decides: a cell is
/// inked where its tone plus the error carried so far reaches one half, and the rest of what it wanted or got over goes on to
/// the next cell, starting from the first, or from the last when `reverse`. `out` is as long as `tones`.
pub(crate) fn diffuse(tones: &[f32], reverse: bool, out: &mut [bool]) {
    let mut carried = 0.0_f32; // constant: nothing is owed before the first cell
    let mut step = |index: usize| {
        let want = tones[index] + carried;
        let inked = want >= 0.5; // constant: the midpoint of 0..1
        out[index] = inked;
        carried = want - f32::from(u8::from(inked));
    };
    if reverse {
        (0..tones.len()).rev().for_each(&mut step);
    } else {
        (0..tones.len()).for_each(&mut step);
    }
}

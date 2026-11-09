//! The Gray-Scott reaction-diffusion model: two chemicals, u and v, that spread over a grid and react where they meet.

use super::edge::{Extension, Source};
use super::par;

// constant: the diffusion rates of chemicals u and v of the Gray-Scott model in grid units per step, a ratio of 2 to 1
// (Pearson 1993, "Complex patterns in a simple system": 2.097e-5 and 1.05e-5 on a grid spacing of 0.01, time step 1)
const DIFFUSION_U: f32 = 0.2097;
// constant: the diffusion rate of v, half u's (Pearson 1993)
const DIFFUSION_V: f32 = 0.105;
// constant: where a cell is seeded, chemical u falls to half and v rises to a quarter (Pearson 1993)
const SEED_U: f32 = 0.5;
// constant: as above, v rises to a quarter (Pearson 1993)
const SEED_V: f32 = 0.25;

/// The grid indices `edge` gives for the neighbours before and after each of `len` cells along an axis.
fn neighbours(len: usize, edge: Extension) -> Vec<(usize, usize)> {
    // `Edge` never gives a constant, so a value beyond the edge does not arise; it would read the cell itself
    let at = |own: usize, step: isize| match edge.locate(len, own.cast_signed() + step) {
        Source::Sample(index) => index,
        Source::Value(_) => own,
    };
    (0..len).map(|own| (at(own, -1), at(own, 1))).collect()
}

/// The concentration of chemical u, per cell, after `steps` steps of the model on a `rows` x `columns` grid (`columns`
/// cells a row), starting where `seeded` is true from u = 1/2 and v = 1/4, elsewhere from u = 1 and v = 0.
///
/// Each step, every cell spreads u and v to its four neighbours by the five-point Laplacian (v half as fast as u) and
/// reacts: u += Du lap u - u v^2 + `feed` (1 - u), v += Dv lap v + u v^2 - (`feed` + `kill`) v, with a time step of 1.
/// `edge` says what lies beyond the grid. Cells are updated from the previous step only, so the result is the same at any
/// thread count. Memory is two grids, swapped each step.
pub(crate) fn grow(
    seeded: &[bool],
    (rows, columns): (usize, usize),
    (feed, kill): (f32, f32),
    steps: usize,
    edge: Extension,
) -> Vec<f32> {
    let mut now: Vec<[f32; 2]> = seeded
        .iter()
        .map(|&on| if on { [SEED_U, SEED_V] } else { [1.0, 0.0] })
        .collect();
    let mut next = vec![[0.0; 2]; now.len()];
    let (down, across) = (neighbours(rows, edge), neighbours(columns, edge));
    for _ in 0..steps {
        let before = &now;
        par::rows(&mut next, columns, |first_row, band| {
            for (offset, line) in band.chunks_exact_mut(columns).enumerate() {
                let (above, below) = down[first_row + offset];
                let (up, middle, low) = (
                    &before[above * columns..(above + 1) * columns],
                    &before[(first_row + offset) * columns..(first_row + offset + 1) * columns],
                    &before[below * columns..(below + 1) * columns],
                );
                for (column, out) in line.iter_mut().enumerate() {
                    let (left, right) = across[column];
                    let [u, v] = middle[column];
                    let around = |chemical: usize| {
                        up[column][chemical] + low[column][chemical] + middle[left][chemical] + middle[right][chemical]
                            - 4.0 * middle[column][chemical] // constant: the five-point Laplacian counts four neighbours
                    };
                    let reaction = u * v * v;
                    *out = [
                        u + DIFFUSION_U * around(0) - reaction + feed * (1.0 - u),
                        v + DIFFUSION_V * around(1) + reaction - (feed + kill) * v,
                    ];
                }
            }
        });
        std::mem::swap(&mut now, &mut next);
    }
    now.into_iter().map(|[u, _]| u).collect()
}

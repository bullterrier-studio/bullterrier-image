//! The nearest of some sites to a point, found through a grid of buckets, and the cells those sites cut a picture into.

use super::length;
use super::pieces::renumber;
use super::raster::centre;

/// Sites kept in a grid of square cells, about two sites to a cell, for finding the one nearest to a point.
pub(crate) struct Buckets {
    sites: Vec<[f64; 2]>,
    /// The corner of the grid with the smallest coordinates.
    origin: [f64; 2],
    /// The side of a cell.
    side: f64,
    /// How many cells the grid has across and down.
    cells: [usize; 2],
    /// Where the sites of each cell begin in `members` (one more entry than cells: where the last ends).
    starts: Vec<usize>,
    /// The index of each site, cell after cell, in ascending order within a cell.
    members: Vec<usize>,
}

impl Buckets {
    /// The buckets of `sites`.
    pub(crate) fn new(sites: &[[f64; 2]]) -> Self {
        let (mut origin, mut far) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for site in sites {
            for axis in 0..2 {
                origin[axis] = origin[axis].min(site[axis]);
                far[axis] = far[axis].max(site[axis]);
            }
        }
        let extent = [far[0] - origin[0], far[1] - origin[1]];
        // about two sites to a cell over the box, or along its length if the sites lie in a line
        let wanted = length::float(sites.len()) / 2.0; // constant: two sites to a cell
        let side = (extent[0] * extent[1] / wanted)
            .sqrt()
            .max(extent[0].max(extent[1]) / wanted);
        // sites that all coincide fit one cell of any side
        let side = if side > 0.0 { side } else { 1.0 }; // constant: any side holds sites that all coincide
        let cells = extent.map(|extent| length::whole((extent / side).floor()) + 1);
        let cell_of = |site: &[f64; 2]| {
            let place = |axis: usize| length::whole(((site[axis] - origin[axis]) / side).floor()).min(cells[axis] - 1);
            place(1) * cells[0] + place(0)
        };
        // counting sort of the sites into cells, keeping their order within a cell
        let mut starts = vec![0; cells[0] * cells[1] + 1];
        for site in sites {
            starts[cell_of(site) + 1] += 1;
        }
        for at in 1..starts.len() {
            starts[at] += starts[at - 1];
        }
        let mut next = starts.clone();
        let mut members = vec![0; sites.len()];
        for (index, site) in sites.iter().enumerate() {
            let cell = cell_of(site);
            members[next[cell]] = index;
            next[cell] += 1;
        }
        Self {
            sites: sites.to_vec(),
            origin,
            side,
            cells,
            starts,
            members,
        }
    }

    /// The index of the site nearest to `(x, y)`, which may lie anywhere, outside the sites' box too. Of sites at the same
    /// distance the one with the lowest index; there is at least one site.
    pub(crate) fn nearest(&self, x: f64, y: f64) -> usize {
        // the cell holding the point, or the one nearest to it if the point lies beyond the grid
        let home = |axis: usize, point: f64| {
            length::whole(((point - self.origin[axis]) / self.side).floor()).min(self.cells[axis] - 1)
        };
        let (home_across, home_down) = (home(0, x), home(1, y));
        let [cells_across, cells_down] = self.cells;
        // (squared distance, index) of the best so far
        let mut best = (f64::INFINITY, 0_usize);
        // by then every cell has been scanned
        for ring in 0..=cells_across.max(cells_down) {
            // the cells `ring` steps from the home cell, the square's top and bottom rows whole, the rows between at their
            // two ends only; those outside the grid are left out
            for down in home_down.saturating_sub(ring)..=(home_down + ring).min(cells_down - 1) {
                if down + ring == home_down || down == home_down + ring {
                    for across in home_across.saturating_sub(ring)..=(home_across + ring).min(cells_across - 1) {
                        self.scan(down * cells_across + across, x, y, &mut best);
                    }
                } else {
                    if home_across >= ring {
                        self.scan(down * cells_across + home_across - ring, x, y, &mut best);
                    }
                    if home_across + ring < cells_across {
                        self.scan(down * cells_across + home_across + ring, x, y, &mut best);
                    }
                }
            }
            // a site not scanned lies in a cell more than `ring` away from the home one along an axis, so more than `ring`
            // sides from the point (which is in its cell, or beyond the grid, farther)
            let reached = length::float(ring) * self.side;
            if best.0 <= reached * reached {
                break;
            }
        }
        best.1
    }

    /// Improves `best` (squared distance, index) with the sites of `cell`, for the point `(x, y)`.
    fn scan(&self, cell: usize, x: f64, y: f64, best: &mut (f64, usize)) {
        for &index in &self.members[self.starts[cell]..self.starts[cell + 1]] {
            let [sx, sy] = self.sites[index];
            let distance = (sx - x).powi(2) + (sy - y).powi(2); // constant: squared distance
            if distance.total_cmp(&best.0).then(index.cmp(&best.1)).is_lt() {
                *best = (distance, index);
            }
        }
    }
}

/// The pieces of a `height` x `width` picture that `sites` cut it into, a pixel belonging to the cell of the site nearest to
/// its centre, and the number of pieces; numbered by first pixel, so a site that is nearest to no centre makes no piece.
/// With `shift` (the shifts of the pixel centres along x and along y, row after row) the centres are moved first, so the
/// borders between cells wander.
pub(crate) fn cells(height: usize, width: usize, sites: &[[f64; 2]], shift: Option<[&[f32]; 2]>) -> (Vec<i32>, usize) {
    let buckets = Buckets::new(sites);
    renumber(
        sites.len(),
        (0..height * width).map(|at| {
            let [x, y] = centre(at, width, shift);
            buckets.nearest(x, y)
        }),
    )
}

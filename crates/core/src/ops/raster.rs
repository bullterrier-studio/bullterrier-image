//! Polygons on a pixel grid: how much of each pixel a set of closed outlines covers, and the pixels they paint.
//!
//! Coordinates are px with pixel `(row, column)` covering `[column, column + 1) x [row, row + 1)`. An outline is a list of
//! corners, closed by itself; several outlines in one [`Polygons`] are one shape (a hole is an outline drawn the other way
//! round inside another). Work goes one scanline at a time, so memory stays at a few rows however big the outline is.

use super::length;

/// What decides that a point is inside outlines that cross each other or wind round it more than once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rule {
    /// Inside when the outlines wind round the point a net number of times (a hole is an outline drawn the other way).
    NonZero,
    /// Inside when a ray from the point crosses the outlines an odd number of times (any inner outline is a hole).
    EvenOdd,
}

/// Closed outlines in px, each closed by an edge from its last corner back to its first.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Polygons {
    corners: Vec<[f64; 2]>,
    /// Where each outline begins in `corners`.
    starts: Vec<usize>,
}

impl Polygons {
    /// Begins an outline at `corner`.
    pub(crate) fn begin(&mut self, corner: [f64; 2]) {
        self.starts.push(self.corners.len());
        self.corners.push(corner);
    }

    /// Adds `corner` to the outline begun last.
    pub(crate) fn push(&mut self, corner: [f64; 2]) {
        self.corners.push(corner);
    }

    /// How many corners all the outlines have.
    pub(crate) const fn corners(&self) -> usize {
        self.corners.len()
    }

    /// How many outlines there are.
    pub(crate) const fn outlines(&self) -> usize {
        self.starts.len()
    }

    /// The corners of the outline `number`.
    pub(crate) fn outline(&self, number: usize) -> &[[f64; 2]] {
        let end = self.starts.get(number + 1).copied().unwrap_or(self.corners.len());
        &self.corners[self.starts[number]..end]
    }

    /// The edges that are not horizontal (they cross no scanline), each from a corner to the next, the last corner of an
    /// outline to its first.
    #[expect(
        clippy::float_cmp,
        reason = "an edge is horizontal only when its ends have exactly the same height"
    )]
    fn slanting(&self) -> impl Iterator<Item = Edge> + '_ {
        (0..self.starts.len()).flat_map(move |number| {
            let end = self.starts.get(number + 1).copied().unwrap_or(self.corners.len());
            let outline = &self.corners[self.starts[number]..end];
            (0..outline.len()).filter_map(move |at| {
                let (start, finish) = (outline[at], outline[(at + 1) % outline.len()]);
                (start[1] != finish[1]).then_some(Edge { start, finish })
            })
        })
    }
}

/// An edge between two corners with different heights.
#[derive(Clone, Copy)]
struct Edge {
    start: [f64; 2],
    finish: [f64; 2],
}

impl Edge {
    /// Where the edge crosses the height `y`.
    fn at(&self, y: f64) -> f64 {
        let run = [self.finish[0] - self.start[0], self.finish[1] - self.start[1]];
        self.start[0] + (y - self.start[1]) * run[0] / run[1]
    }

    /// Whether the edge goes down the picture (towards larger heights).
    fn descends(&self) -> bool {
        self.finish[1] > self.start[1]
    }
}

/// The height of scanline `number`, where each pixel row has `samples` of them, evenly spaced and each in the middle of its
/// share of the row.
fn scanline(number: usize, samples: usize) -> f64 {
    (length::float(number) + 0.5) / length::float(samples) // constant: a scanline sits in the middle of its share of a row
}

/// The scanlines (of `count`) that `edge` crosses, from the first to just past the last. An edge crosses a scanline when its
/// upper end is at or above it and its lower end below it, so a corner exactly on a scanline is counted once.
fn crossed(edge: &Edge, count: usize, samples: usize) -> std::ops::Range<usize> {
    let (low, high) = (edge.start[1].min(edge.finish[1]), edge.start[1].max(edge.finish[1]));
    // the first scanline at or below `height`: a binary search, as the scanlines rise
    let before = |height: f64| {
        let (mut first, mut last) = (0, count);
        while first < last {
            let middle = first + (last - first) / 2;
            if scanline(middle, samples) < height {
                first = middle + 1;
            } else {
                last = middle;
            }
        }
        first
    };
    before(low)..before(high)
}

/// How many crossings of an edge with a scanline filling `shape` in a picture of `height` rows takes, with `samples`
/// scanlines a row: the work of [`coverage`] and [`paint`].
pub(crate) fn crossings(shape: &Polygons, height: usize, samples: usize) -> u64 {
    let count = height * samples;
    shape
        .slanting()
        .map(|edge| {
            let scans = crossed(&edge, count, samples);
            (scans.end - scans.start) as u64
        })
        .fold(0, u64::saturating_add)
}

/// How many pixels of a `height` x `width` picture the box round `shape` holds: what painting it can set at most.
pub(crate) fn box_pixels(shape: &Polygons, height: usize, width: usize) -> u64 {
    let (mut low, mut high) = ([f64::MAX; 2], [f64::MIN; 2]);
    for number in 0..shape.outlines() {
        for corner in shape.outline(number) {
            for axis in [0, 1] {
                low[axis] = low[axis].min(corner[axis]);
                high[axis] = high[axis].max(corner[axis]);
            }
        }
    }
    // the part of the box inside the picture along an axis, in whole pixels
    let inside = |axis: usize, size: usize| {
        length::whole(
            (high[axis].min(length::float(size)) - low[axis].max(0.0))
                .max(0.0)
                .ceil(),
        )
    };
    inside(0, width) as u64 * inside(1, height) as u64
}

/// Whether filling takes no more than `budget` crossings: the work `shape` makes of `height` rows with `samples` scanlines
/// a row, before any is done; else the refusal, ending in `advice`.
fn affordable(shape: &Polygons, height: usize, samples: usize, budget: u64, advice: &str) -> Result<(), String> {
    let work = crossings(shape, height, samples);
    if work > budget {
        return Err(format!(
            "filling would take {work} crossings of its edges with scanlines, over the limit of {budget}: {advice}"
        ));
    }
    Ok(())
}

/// The stretches `(from, to)` in px across each scanline of `count` (`samples` a row) that `shape` covers by `rule`, as
/// `(scanline, stretches)` for every scanline from the first an edge crosses to the end of the row of the last.
fn scans(shape: &Polygons, rule: Rule, count: usize, samples: usize) -> impl Iterator<Item = (usize, Vec<(f64, f64)>)> {
    // the edges by the first scanline they cross, so the ones a scanline crosses are found by moving on a cursor
    let mut edges: Vec<(Edge, std::ops::Range<usize>)> = shape
        .slanting()
        .map(|edge| {
            let scans = crossed(&edge, count, samples);
            (edge, scans)
        })
        .filter(|(_, scans)| !scans.is_empty())
        .collect();
    edges.sort_by_key(|(_, scans)| scans.start);
    // whole rows: the one the last scanline is in is finished
    let range = match (
        edges.first().map(|(_, scans)| scans.start),
        edges.iter().map(|(_, scans)| scans.end).max(),
    ) {
        (Some(first), Some(last)) => first..last.div_ceil(samples) * samples,
        _ => 0..0,
    };
    let mut next = 0; // constant: no edge has begun yet
    let mut active: Vec<usize> = Vec::new();
    let mut hits: Vec<(f64, i32)> = Vec::new();
    range.map(move |scan| {
        while next < edges.len() && edges[next].1.start <= scan {
            active.push(next);
            next += 1;
        }
        active.retain(|&at| edges[at].1.end > scan);
        let y = scanline(scan, samples);
        hits.clear();
        hits.extend(active.iter().map(|&at| {
            let edge = &edges[at].0;
            (edge.at(y), if edge.descends() { 1 } else { -1 })
        }));
        hits.sort_by(|one, other| one.0.total_cmp(&other.0));
        // Every closed outline crosses a scanline as often one way as the other, so the running sum returns to 0 at the end
        // of the scanline: a stretch begins at each crossing after which the point is inside and ends at the next.
        let mut winding = 0; // constant: no crossing has turned yet
        let mut stretches = Vec::new();
        for (at, &(x, direction)) in hits.iter().enumerate() {
            winding += direction;
            let inside = match rule {
                Rule::NonZero => winding != 0,
                Rule::EvenOdd => at % 2 == 0, // constant: odd crossings so far, counting from 1
            };
            if inside {
                stretches.push((x, hits.get(at + 1).map_or(x, |next| next.0)));
            }
        }
        (scan, stretches)
    })
}

/// How much of each pixel of a `height` x `width` picture `shape` covers by the nonzero rule: 0 to 1, row after row.
///
/// Horizontally the share is exact; vertically each pixel row is measured at `samples` evenly spaced scanlines, so the error
/// of a slanting border shrinks as `samples` grows (1 looks at the middle of the row only).
///
/// # Errors
/// A message if the fill would take more than `budget` crossings of an edge with a scanline (what the outline and the
/// picture's rows make of it; an outline with many edges over many rows is slow), before any work is done.
pub(crate) fn coverage(
    height: usize,
    width: usize,
    shape: &Polygons,
    samples: usize,
    budget: u64,
) -> Result<Vec<f32>, String> {
    affordable(
        shape,
        height,
        samples,
        budget,
        "use fewer corners or a smaller `samples`",
    )?;
    let mut out = vec![0.0_f32; height * width];
    // constant: the ramp of a row has width + 1 pixel boundaries and two more cells for the second difference
    let mut steps = vec![0.0_f64; width + 3];
    for (scan, stretches) in scans(shape, Rule::NonZero, height * samples, samples) {
        for (from, to) in stretches {
            add_stretch(&mut steps, width, from, to, 1.0 / length::float(samples));
        }
        if (scan + 1) % samples == 0 {
            let row = scan / samples;
            finish_row(&mut steps, &mut out[row * width..(row + 1) * width]);
        }
    }
    Ok(out)
}

/// Sets `target` (a `height` x `width` map, row after row) to `value` at every pixel whose centre `shape` covers by `rule`.
/// A centre exactly on an edge is covered when the shape is to its right or below it.
///
/// # Errors
/// A message if the fill would take more than `budget` crossings of an edge with a scanline, before any work is done.
pub(crate) fn paint(
    target: &mut [i32],
    height: usize,
    width: usize,
    shape: &Polygons,
    rule: Rule,
    value: i32,
    budget: u64,
) -> Result<(), String> {
    affordable(shape, height, 1, budget, "use fewer corners")?;
    fill(target, height, width, shape, rule, value);
    Ok(())
}

/// As [`paint`] for a caller that has counted the crossings (see [`crossings`]) against its own budget.
pub(crate) fn fill(target: &mut [i32], height: usize, width: usize, shape: &Polygons, rule: Rule, value: i32) {
    // the first pixel whose centre (at i + 1/2) is at or past `x`, kept in the row
    let pixel = |x: f64| length::whole((x - 0.5).ceil().clamp(0.0, length::float(width)));
    for (row, stretches) in scans(shape, rule, height, 1) {
        for (from, to) in stretches {
            target[row * width + pixel(from)..row * width + pixel(to)].fill(value);
        }
    }
}

/// The centre of pixel `at` of a picture `width` pixels wide (row after row), in px; moved by `shift` (the shifts along x and
/// along y, row after row) if there is one.
pub(crate) fn centre(at: usize, width: usize, shift: Option<[&[f32]; 2]>) -> [f64; 2] {
    // constant: a pixel's centre is half a pixel in
    let [x, y] = [length::float(at % width) + 0.5, length::float(at / width) + 0.5];
    match shift {
        None => [x, y],
        Some([along_x, along_y]) => [x + f64::from(along_x[at]), y + f64::from(along_y[at])],
    }
}

/// `values` (a `height` x `width` map) read bilinearly at the pixel centres moved by `shift` (along x and along y, row after
/// row): at each pixel the map interpolated between the four pixel centres round the moved centre, a centre moved beyond
/// the picture reading the pixel at its edge.
pub(crate) fn read_bent(values: &[f32], height: usize, width: usize, shift: [&[f32]; 2]) -> Vec<f32> {
    // the reading position in pixel centres (pixel `i` at `i`), kept inside the picture
    let place = |at: usize, moved: f32, size: usize| -> (usize, usize, f64) {
        let position = (length::float(at) + f64::from(moved)).clamp(0.0, length::float(size - 1));
        let before = length::whole(position.floor());
        (before, (before + 1).min(size - 1), position - position.floor())
    };
    (0..height * width)
        .map(|at| {
            let (row, down, across_row) = place(at / width, shift[1][at], height);
            let (column, right, across_column) = place(at % width, shift[0][at], width);
            let read = |r: usize, c: usize| f64::from(values[r * width + c]);
            let top = read(row, column) * (1.0 - across_column) + read(row, right) * across_column;
            let bottom = read(down, column) * (1.0 - across_column) + read(down, right) * across_column;
            super::cast::single(top * (1.0 - across_row) + bottom * across_row)
        })
        .collect()
}

/// `values` (a `height` x `width` map) read at the pixel holding each pixel centre moved by `shift` (along x and along y,
/// row after row): a centre moved beyond the picture reads the pixel at its edge.
pub(crate) fn read_floor<T: Copy>(values: &[T], height: usize, width: usize, shift: [&[f32]; 2]) -> Vec<T> {
    // the pixel holding the centre of pixel `at` moved by `moved`: a centre sits half a pixel in
    let place = |at: usize, moved: f32, size: usize| {
        length::whole(
            (length::float(at) + 0.5 + f64::from(moved))
                .floor()
                .clamp(0.0, length::float(size - 1)),
        )
    };
    (0..height * width)
        .map(|at| values[place(at / width, shift[1][at], height) * width + place(at % width, shift[0][at], width)])
        .collect()
}

/// Adds the stretch from `from` to `to` px (across the row), counting `weight` of a scanline, to the second differences
/// `steps` of the row's coverage.
///
/// A stretch from `a` to `b` covers pixel `i` by `clip(i + 1 - a, 0, b - a) - clip(i - a, 0, b - a)`, the difference of two
/// ramps `max(i - a, 0)` and `max(i - b, 0)`; a ramp starting at `a` is a second difference of `ceil(a) - a` at `ceil(a)` and
/// `1 - (ceil(a) - a)` after it.
fn add_stretch(steps: &mut [f64], width: usize, from: f64, to: f64, weight: f64) {
    for (edge, weight) in [(from, weight), (to, -weight)] {
        let edge = edge.clamp(0.0, length::float(width));
        let cell = edge.ceil();
        let index = length::whole(cell);
        steps[index] += weight * (cell - edge);
        steps[index + 1] += weight * (1.0 - (cell - edge)); // constant: the ramp's second difference is 1 minus its first
    }
}

/// The coverage of a pixel row from its second differences `steps` (cleared for the next row) into `row`.
fn finish_row(steps: &mut [f64], row: &mut [f32]) {
    let (mut slope, mut ramp) = (0.0, 0.0);
    let mut ramps = Vec::with_capacity(row.len() + 1);
    for step in steps.iter().take(row.len() + 1) {
        slope += step;
        ramp += slope;
        ramps.push(ramp);
    }
    for (pixel, pair) in row.iter_mut().zip(ramps.windows(2)) {
        // the difference of two running sums cancels to a hair below 0 (about -5e-17) beside a full pixel, or above 1; the
        // coverage is 0 to 1, and this is where it is cast, so the one place it is held there
        *pixel = super::cast::single((pair[1] - pair[0]).clamp(0.0, 1.0));
    }
    steps.fill(0.0);
}

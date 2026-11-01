//! Polygons on a pixel grid: how much of each pixel a set of closed outlines covers by the nonzero rule.
//!
//! Coordinates are px with pixel `(row, column)` covering `[column, column + 1) x [row, row + 1)`. An outline is a list of
//! corners, closed by itself; several outlines in one [`Polygons`] are one shape (a hole is an outline drawn the other way
//! round inside another). Work goes one scanline at a time, so memory stays at a few rows however big the outline is.

use super::length;

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

    /// The corners of each outline.
    #[cfg(test)]
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
/// scanlines a row: the work of [`coverage`].
fn crossings(shape: &Polygons, height: usize, samples: usize) -> u64 {
    let count = height * samples;
    shape
        .slanting()
        .map(|edge| {
            let scans = crossed(&edge, count, samples);
            (scans.end - scans.start) as u64
        })
        .fold(0, u64::saturating_add)
}

/// How much of each pixel of a `height` x `width` picture `shape` covers by the nonzero rule (a point is inside when the
/// outlines wind round it a net number of times): 0 to 1, row after row.
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
    let work = crossings(shape, height, samples);
    if work > budget {
        return Err(format!(
            "filling the path would take {work} crossings of its edges with scanlines, over the limit of {budget}: use fewer \
             corners or a smaller `samples`"
        ));
    }
    let count = height * samples;
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
    let mut out = vec![0.0_f32; height * width];
    let mut next = 0;
    let mut active: Vec<usize> = Vec::new();
    // constant: the ramp of a row has width + 1 pixel boundaries and two more cells for the second difference
    let mut steps = vec![0.0_f64; width + 3];
    let mut hits: Vec<(f64, i32)> = Vec::new();
    let (Some(first), Some(last)) = (
        edges.first().map(|(_, scans)| scans.start),
        edges.iter().map(|(_, scans)| scans.end).max(),
    ) else {
        return Ok(out);
    };
    // whole rows: the one the last scanline is in is finished
    for scan in first..last.div_ceil(samples) * samples {
        while next < edges.len() && edges[next].1.start <= scan {
            active.push(next);
            next += 1;
        }
        active.retain(|&at| edges[at].1.end > scan);
        let (row, y) = (scan / samples, scanline(scan, samples));
        hits.clear();
        hits.extend(active.iter().map(|&at| {
            let edge = &edges[at].0;
            (edge.at(y), if edge.descends() { 1 } else { -1 })
        }));
        hits.sort_by(|one, other| one.0.total_cmp(&other.0));
        // Every closed outline crosses a scanline as often one way as the other, so the running sum returns to 0 at the end
        // of the scanline: a stretch begins at each crossing after which it is not 0 and ends at the next.
        let mut winding = 0;
        for (at, &(x, direction)) in hits.iter().enumerate() {
            winding += direction;
            if winding != 0 {
                let end = hits.get(at + 1).map_or(x, |next| next.0);
                add_stretch(&mut steps, width, x, end, 1.0 / length::float(samples));
            }
        }
        if (scan + 1) % samples == 0 {
            finish_row(&mut steps, &mut out[row * width..(row + 1) * width]);
        }
    }
    Ok(out)
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
        *pixel = super::cast::single(pair[1] - pair[0]);
    }
    steps.fill(0.0);
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{Polygons, coverage, read_bent};

    /// The polygons with one outline per list of corners.
    fn shape(outlines: &[&[[f64; 2]]]) -> Polygons {
        let mut polygons = Polygons::default();
        for outline in outlines {
            polygons.begin(outline[0]);
            for corner in &outline[1..] {
                polygons.push(*corner);
            }
        }
        polygons
    }

    /// The rectangle from `(left, top)` to `(right, bottom)` as an outline, clockwise on a screen.
    fn rectangle(left: f64, top: f64, right: f64, bottom: f64) -> Vec<[f64; 2]> {
        vec![[left, top], [right, top], [right, bottom], [left, bottom]]
    }

    fn covered(height: usize, width: usize, shape: &Polygons, samples: usize) -> Vec<f32> {
        coverage(height, width, shape, samples, u64::MAX).unwrap_or_default()
    }

    /// Whether `actual` is `expected` to within `by`, with the first index at which it is not for the message.
    fn close(actual: &[f32], expected: &[f32], by: f32) -> Result<(), usize> {
        if actual.len() != expected.len() {
            return Err(actual.len().min(expected.len()));
        }
        match actual
            .iter()
            .zip(expected)
            .position(|(one, other)| (one - other).abs() > by)
        {
            Some(at) => Err(at),
            None => Ok(()),
        }
    }

    #[test]
    fn a_rectangle_covers_each_pixel_by_the_share_of_it_inside_measured_on_the_scanlines() {
        // x from 0.5 to 3.5 covers pixels 0 and 3 by a half and 1 and 2 fully; y from 0.5 to 2.25: with 4 scanlines a row
        // (at 1/8, 3/8, 5/8, 7/8 of it) row 0 has two of its four inside, row 1 all four, row 2 one (at 2.125)
        let rect = shape(&[&rectangle(0.5, 0.5, 3.5, 2.25)]);
        let rows = [[0.25, 0.5, 0.5, 0.25], [0.5, 1.0, 1.0, 0.5], [0.125, 0.25, 0.25, 0.125]].concat();
        assert_eq!(close(&covered(3, 4, &rect, 4), &rows, 1e-6), Ok(()));
        // the exact area is 3 x 1.75 = 5.25 and every scanline of a rectangle measures it exactly
        assert!((covered(3, 4, &rect, 4).iter().sum::<f32>() - 5.25).abs() < 1e-5);
    }

    #[test]
    fn an_edge_on_a_scanline_belongs_to_the_shape_below_it_not_above() {
        // one scanline a row, in the middle of it. The top at 0.5 is on the scanline of row 0: inside; the bottom at 2.5 is on
        // the scanline of row 2: outside. So rows 0 and 1 are covered, and row 2 is not
        let rect = shape(&[&rectangle(0.0, 0.5, 2.0, 2.5)]);
        assert_eq!(
            close(&covered(3, 2, &rect, 1), &[1.0, 1.0, 1.0, 1.0, 0.0, 0.0], 0.0),
            Ok(())
        );
    }

    #[test]
    fn the_direction_an_outline_is_drawn_does_not_matter_and_an_inner_one_the_other_way_round_is_a_hole() {
        let outer = rectangle(0.0, 0.0, 6.0, 6.0);
        let mut inner = rectangle(2.0, 2.0, 4.0, 4.0);
        let reversed: Vec<[f64; 2]> = outer.iter().rev().copied().collect();
        assert_eq!(
            covered(6, 6, &shape(&[&outer]), 2),
            covered(6, 6, &shape(&[&reversed]), 2)
        );
        // against the way round of the outer outline: a hole of 4 pixels
        inner.reverse();
        let ring = covered(6, 6, &shape(&[&outer, &inner]), 2);
        assert!((ring.iter().sum::<f32>() - 32.0).abs() < 1e-4);
        assert_eq!(ring[2 * 6 + 2].to_bits(), 0.0_f32.to_bits());
        // the way the outer goes round: it winds twice there, which is inside under the nonzero rule
        inner.reverse();
        let doubled = covered(6, 6, &shape(&[&outer, &inner]), 2);
        assert!((doubled.iter().sum::<f32>() - 36.0).abs() < 1e-4);
    }

    #[test]
    fn two_outlines_of_one_call_are_one_shape_and_the_parts_outside_the_picture_are_clipped() {
        let apart = shape(&[&rectangle(0.0, 0.0, 2.0, 2.0), &rectangle(4.0, 3.0, 6.0, 4.0)]);
        assert!((covered(4, 6, &apart, 3).iter().sum::<f32>() - 6.0).abs() < 1e-5);
        // a rectangle reaching beyond the picture on every side covers it all, and one entirely beyond covers nothing
        let big = shape(&[&rectangle(-5.0, -5.0, 10.0, 10.0)]);
        assert_eq!(close(&covered(3, 4, &big, 4), &[1.0; 12], 1e-6), Ok(()));
        let away = shape(&[&rectangle(10.0, 10.0, 12.0, 12.0)]);
        assert_eq!(close(&covered(3, 4, &away, 4), &[0.0; 12], 0.0), Ok(()));
        assert_eq!(close(&covered(3, 4, &Polygons::default(), 4), &[0.0; 12], 0.0), Ok(()));
    }

    #[test]
    fn filling_that_would_take_more_crossings_than_the_budget_is_refused_before_any_work() {
        // two vertical edges cross every scanline of 10 rows: 20 crossings with 1 scanline a row, 80 with 4
        let tall = shape(&[&rectangle(1.0, 0.0, 3.0, 10.0)]);
        assert!(coverage(10, 4, &tall, 1, 20).is_ok());
        let refused = coverage(10, 4, &tall, 1, 19);
        assert!(
            matches!(&refused, Err(said) if said.contains("20 crossings") && said.contains("19")),
            "{refused:?}"
        );
        assert!(coverage(10, 4, &tall, 4, 80).is_ok());
        assert!(coverage(10, 4, &tall, 4, 79).is_err());
        // only the rows of the picture count: the same edges over a picture of 5 rows cross 10 scanlines
        assert!(coverage(5, 4, &tall, 1, 10).is_ok());
    }

    #[test]
    fn a_map_read_at_moved_centres_is_interpolated_between_the_pixels_and_beyond_the_picture_is_its_edge() {
        let values = [0.0, 1.0, 0.0, 1.0, 0.0, 0.0]; // 2 rows of 3
        let still = vec![0.0; 6];
        assert_eq!(read_bent(&values, 2, 3, [&still, &still]), values);
        // moved right by half a pixel: the mean of the pixel and the one to its right; the last reads its own edge
        let right = vec![0.5; 6];
        let moved = read_bent(&values, 2, 3, [&right, &still]);
        assert_eq!(close(&moved, &[0.5, 0.5, 0.0, 0.5, 0.0, 0.0], 1e-7), Ok(()));
        // moved down by a quarter: three quarters of the pixel and a quarter of the one below (the last row has none below)
        let down = vec![0.25; 6];
        let moved = read_bent(&values, 2, 3, [&still, &down]);
        assert_eq!(close(&moved, &[0.25, 0.75, 0.0, 1.0, 0.0, 0.0], 1e-7), Ok(()));
        // moved far beyond the picture on either side: the edge pixel
        let far = vec![100.0; 6];
        let moved = read_bent(&values, 2, 3, [&far, &far]);
        assert_eq!(close(&moved, &[0.0; 6], 0.0), Ok(()));
        let before = vec![-100.0; 6];
        assert_eq!(
            close(&read_bent(&values, 2, 3, [&before, &before]), &[0.0; 6], 0.0),
            Ok(())
        );
    }

    /// The winding number of the outlines round the point `(x, y)` (all scaled to whole numbers), by Sunday's algorithm
    /// (an upward edge that the point is left of counts +1, a downward one that it is right of -1; an edge counts for the
    /// heights from its start up to but not including its end), in exact integer arithmetic.
    fn winding(outlines: &[Vec<[i64; 2]>], x: i64, y: i64) -> i64 {
        let mut turns = 0;
        for outline in outlines {
            for (at, start) in outline.iter().enumerate() {
                let end = outline[(at + 1) % outline.len()];
                let left = (end[0] - start[0]) * (y - start[1]) - (x - start[0]) * (end[1] - start[1]);
                if start[1] <= y {
                    if end[1] > y && left > 0 {
                        turns += 1;
                    }
                } else if end[1] <= y && left < 0 {
                    turns -= 1;
                }
            }
        }
        turns
    }

    /// The coverage of each pixel of a `size` x `size` picture by the outlines (corners in half pixels), measured with
    /// `samples` scanlines a row and 256 points along each, each decided by [`winding`] on whole numbers.
    fn dense(outlines: &[Vec<[i64; 2]>], size: usize, samples: usize) -> Vec<f32> {
        let (points, scale) = (256_i64, 2 * 256 * i64::try_from(samples).unwrap_or(1)); // scale: half pixels to whole numbers
        let scaled: Vec<Vec<[i64; 2]>> = outlines
            .iter()
            .map(|outline| {
                outline
                    .iter()
                    .map(|corner| corner.map(|half| half * scale / 2))
                    .collect()
            })
            .collect();
        let samples = i64::try_from(samples).unwrap_or(1);
        let mut out = Vec::new();
        for row in 0..i64::try_from(size).unwrap_or(0) {
            for column in 0..i64::try_from(size).unwrap_or(0) {
                let mut hits = 0;
                for scan in 0..samples {
                    // the scanline (row * samples + scan + 1/2) / samples, the point (column + (k + 1/2) / points)
                    let y = (row * samples + scan) * 2 * points + points;
                    for k in 0..points {
                        let x = column * scale + (2 * k + 1) * samples;
                        hits += i64::from(winding(&scaled, x, y) != 0);
                    }
                }
                out.push(
                    f32::from(u16::try_from(hits).unwrap_or(0))
                        / f32::from(u16::try_from(points * samples).unwrap_or(1)),
                );
            }
        }
        out
    }

    /// Outlines of three to six corners, in half pixels of an 8 x 8 picture: crossing themselves, nested, any way round.
    fn outlines() -> impl Strategy<Value = Vec<Vec<[i64; 2]>>> {
        proptest::collection::vec(proptest::collection::vec([0_i64..=16, 0_i64..=16], 3..=6), 1..=2)
    }

    proptest! {
        #[test]
        fn the_coverage_is_that_of_a_dense_point_by_point_winding_number_test_whatever_the_outlines(
            drawn in outlines(),
            samples in 1_usize..=3,
        ) {
            let mut polygons = Polygons::default();
            for outline in &drawn {
                polygons.begin([f64::from(i32::try_from(outline[0][0]).unwrap_or(0)) / 2.0, f64::from(i32::try_from(outline[0][1]).unwrap_or(0)) / 2.0]);
                for corner in &outline[1..] {
                    polygons.push([f64::from(i32::try_from(corner[0]).unwrap_or(0)) / 2.0, f64::from(i32::try_from(corner[1]).unwrap_or(0)) / 2.0]);
                }
            }
            let made = covered(8, 8, &polygons, samples);
            let expected = dense(&drawn, 8, samples);
            // 256 points a scanline: a border inside a pixel is placed to within 1/256 of it, and a pixel has a few
            prop_assert_eq!(close(&made, &expected, 0.03), Ok(()));
        }
    }
}

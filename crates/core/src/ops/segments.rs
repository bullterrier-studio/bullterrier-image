//! Anti-aliased line segments with soft ends, drawn into a layer of light: dust and scratches, particles.

use super::cast::single;
use super::length;

/// A point (x, y) of the picture, in px, a pixel's centre at whole numbers.
pub(crate) type Point = (f64, f64);

/// One segment: where it runs and how it looks.
#[derive(Clone, Copy)]
pub(crate) struct Segment {
    /// Where it starts; a segment whose ends are equal is a disc.
    pub(crate) start: Point,
    /// Where it ends.
    pub(crate) end: Point,
    /// Half its width, px.
    pub(crate) radius: f64,
    /// Its brightness along its centre line, 0..1.
    pub(crate) weight: f64,
    /// The width of its soft edge, px; under 1 it is taken as 1, a pixel of anti-aliasing, the finest edge a pixel grid draws.
    pub(crate) feather: f64,
}

/// Draws `segment` into `layer` (`height` x `width`, the rows from canvas row `first_row` on; the segment's points are canvas
/// places): each pixel takes the larger of its value and `weight` times how far the
/// segment covers it. That is 1 within `radius` - `feather` / 2 of the segment, 0 beyond `radius` + `feather` / 2 and falling
/// linearly between, so 0.5 at `radius`. Where segments overlap the brighter one shows; parts beyond the picture are cut off.
pub(crate) fn draw(layer: &mut [f32], (height, width): (usize, usize), first_row: usize, segment: &Segment) {
    let Segment {
        start,
        end,
        radius,
        weight,
        feather,
    } = *segment;
    let feather = feather.max(1.0); // constant: one pixel, the finest edge a pixel grid can draw
    let half = feather / 2.0; // constant: the edge ramp reaches half its width each side of the radius
    // shortcut: the box around the shape, so a long leaning segment visits its whole slanted box; a row-by-row span would
    // visit only the line, worth it once segments lean far on a very large picture
    let reach = radius + half + 1.0; // constant: a pixel beyond the ramp is surely uncovered
    // the pixels the shape can touch (`whole` of a negative number is 0)
    let first = |coordinate: f64| length::whole(coordinate.floor());
    let top = first(start.1.min(end.1) - reach).max(first_row);
    let bottom = (first(start.1.max(end.1) + reach) + 1).min(first_row + height);
    let left = first(start.0.min(end.0) - reach);
    let right = (first(start.0.max(end.0) + reach) + 1).min(width);
    let along = (end.0 - start.0, end.1 - start.1);
    let squared = along.0 * along.0 + along.1 * along.1;
    for row in top..bottom {
        for column in left..right {
            let (x, y) = (length::float(column) - start.0, length::float(row) - start.1);
            // how far along the segment (0..1) its nearest point to the pixel lies; a disc has no length to divide by
            let nearest = if squared > 0.0 {
                ((x * along.0 + y * along.1) / squared).clamp(0.0, 1.0)
            } else {
                0.0 // constant: a disc is its one point
            };
            let distance = (x - nearest * along.0).hypot(y - nearest * along.1);
            let covered = weight * ((radius + half - distance) / feather).clamp(0.0, 1.0);
            let cell = &mut layer[(row - first_row) * width + column];
            *cell = cell.max(single(covered));
        }
    }
}

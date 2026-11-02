//! Outlines: SVG path data turned into closed polygons in px, ready to fill (see [`raster`](super::raster)).

use svgtypes::{SimplePathSegment, SimplifyingPathParser};

use super::length;
use super::raster::Polygons;

/// A rectangle of the picture in px: `[left, top, right, bottom]`.
pub(crate) type Frame = [f64; 4]; // constant: left, top, right, bottom

/// The most a path may ask of the reader, so that a hostile or mistaken one costs a bounded amount of work and memory.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// The most chords one curve may need: a curve needing more reaches absurdly far outside the frame.
    pub(crate) chords: usize,
    /// The farthest a corner or control point may be from the origin, px.
    pub(crate) far: f64,
    /// The most corners all the outlines of one path may have.
    pub(crate) corners: usize,
    /// The most crossings of an edge with a scanline that filling may take (all the shapes of a drawing together).
    pub(crate) crossings: u64,
    /// The most pixels that painting all the shapes of a drawing may set, each shape counted by its box clipped to the
    /// picture.
    pub(crate) painted: u64,
    /// The most nodes (elements, texts, comments) an SVG file may have.
    pub(crate) nodes: u32,
    /// The most elements an SVG drawing may visit, each counted as often as a `use` repeats it.
    pub(crate) drawn: u64,
    /// The deepest nesting of elements, a `use` counting as the parent of what it uses.
    pub(crate) depth: usize,
}

impl Limits {
    /// The limits of a render.
    pub(crate) const STANDARD: Self = Self {
        chords: 1024,       // constant: a curve inside any picture needs a few hundred chords at most
        far: 1e7, // constant: three orders more than any picture side, which keeps every coordinate exact enough to fill
        corners: 1 << 20, // constant: a million corners is a path of 9 MB, far beyond a drawing
        crossings: 1 << 28, // constant: about a second of filling
        painted: 1 << 30, // constant: a billion pixels is a few seconds of painting, far beyond a drawing on a picture
        nodes: 1 << 21, // constant: two million nodes is an arena of about 100 MB, far beyond a drawing
        drawn: 1 << 22, // constant: four million visits is a few seconds of walking, far beyond a drawing
        depth: 64, // constant: drawings nest under 20 levels, and the walk below stays within a few frames of it
    };
}

/// How far a chord may stray from its curve, in px.
const FLATNESS: f64 = 0.05; // constant: a twentieth of a pixel, far below what a pixel shows

/// The four control points of a cubic Bézier.
type Cubic = [[f64; 2]; 4]; // constant: a cubic has four control points

/// The corners after the first of the chords that stand for the cubic Bézier through the control points `points`: as many
/// as keep the curve within [`FLATNESS`] of them. The distance of a curve to its chord is at most 1/8 of the curve's largest
/// second derivative divided by the number of chords squared, and the second derivative is at most 6 times the larger of the
/// two second differences of the control points.
fn chords(points: Cubic, limits: &Limits) -> Result<Vec<[f64; 2]>, String> {
    let second = |a: usize, b: usize, c: usize| {
        let (x, y) = (
            points[a][0] - 2.0 * points[b][0] + points[c][0],
            points[a][1] - 2.0 * points[b][1] + points[c][1],
        );
        x.hypot(y)
    };
    let bend = second(0, 1, 2).max(second(1, 2, 3)); // constant: the control points by threes
    // constant: the bound on the second derivative (6), the error of a chord (1/8)
    let wanted = (6.0 * bend / 8.0 / FLATNESS).sqrt().ceil();
    if wanted.is_nan() || wanted > length::float(limits.chords) {
        return Err(format!(
            "a curve reaches too far outside the frame (it would need more than {} chords)",
            limits.chords
        ));
    }
    let count = length::whole(wanted).max(1);
    Ok((1..=count)
        .map(|at| {
            let t = length::float(at) / length::float(count);
            let u = 1.0 - t;
            // constant: the Bernstein form of a cubic: weights 1, 3, 3, 1
            let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            [0, 1].map(|axis| {
                weights
                    .iter()
                    .zip(&points)
                    .map(|(weight, point)| weight * point[axis])
                    .sum()
            })
        })
        .collect())
}

/// The polygons of the SVG path data `d`, whose coordinates are shares of `frame` (so the unit square is stretched to the
/// frame, and an arc `A 0.1 0.1` is a circle only in a square frame): one outline for each subpath, not closed by a repeated
/// corner. Curves and arcs are replaced by chords that stay within a twentieth of a pixel of them. Drawing after a close goes
/// on from where the closed subpath began.
///
/// # Errors
/// As [`flatten`].
pub(crate) fn path(d: &str, frame: Frame, limits: &Limits) -> Result<Polygons, String> {
    let [left, top, right, bottom] = frame;
    flatten(d, limits, 0, |x, y| {
        [left + x * (right - left), top + y * (bottom - top)]
    })
}

/// The polygons of the SVG path data `d` with every point moved to px by `to_px`, as [`path`]; `spent` corners of other
/// outlines already count against `limits.corners`.
///
/// # Errors
/// A message naming the problem if `d` is empty, does not start with `M`, is not path data (the message gives the position),
/// or reaches beyond `limits`: a corner or control point not finite or farther than `far`, a curve that needs more than
/// `chords` chords, more than `corners` corners.
pub(crate) fn flatten(
    d: &str,
    limits: &Limits,
    spent: usize,
    to_px: impl Fn(f64, f64) -> [f64; 2],
) -> Result<Polygons, String> {
    if d.trim().is_empty() {
        return Err("path data is empty: write it like 'M 0.1 0.1 L 0.9 0.1 L 0.5 0.9 Z'".to_owned());
    }
    if !d.trim_start().starts_with(['M', 'm']) {
        return Err("path data must start with M (a move to the first point)".to_owned());
    }
    let to_px = |x: f64, y: f64| -> Result<[f64; 2], String> {
        let point = to_px(x, y);
        if point.iter().all(|value| value.abs() <= limits.far) {
            Ok(point)
        } else {
            Err(format!(
                "a coordinate is not finite or reaches too far outside the frame (over {:e} px)",
                limits.far
            ))
        }
    };
    let mut polygons = Polygons::default();
    let mut at = [0.0, 0.0]; // constant: the pen starts at the origin until the first move
    for segment in SimplifyingPathParser::from(d) {
        let segment = segment.map_err(|error| format!("path data is not valid SVG path data: {error}"))?;
        match segment {
            SimplePathSegment::MoveTo { x, y } => {
                at = to_px(x, y)?;
                polygons.begin(at);
            }
            SimplePathSegment::LineTo { x, y } => {
                at = to_px(x, y)?;
                polygons.push(at);
            }
            SimplePathSegment::CurveTo { x1, y1, x2, y2, x, y } => {
                let points = [at, to_px(x1, y1)?, to_px(x2, y2)?, to_px(x, y)?];
                at = points[3]; // constant: the end of a cubic is its fourth point
                for corner in chords(points, limits)? {
                    polygons.push(corner);
                }
            }
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                let (control, end) = (to_px(x1, y1)?, to_px(x, y)?);
                // constant: a quadratic is the cubic whose control points are 2/3 of the way from each end to its control
                let along = |from: [f64; 2]| [0, 1].map(|axis| from[axis] + 2.0 / 3.0 * (control[axis] - from[axis]));
                for corner in chords([at, along(at), along(end), end], limits)? {
                    polygons.push(corner);
                }
                at = end;
            }
            SimplePathSegment::ClosePath => {}
        }
        if spent + polygons.corners() > limits.corners {
            return Err(format!("path data has more than {} corners", limits.corners));
        }
    }
    Ok(polygons)
}

/// How many subpaths the path data `d` has, if it is path data a region may hold: what [`path`] accepts in the unit frame
/// with the standard limits.
///
/// # Errors
/// As [`path`].
pub(crate) fn check_syntax(d: &str) -> Result<usize, String> {
    path(d, [0.0, 0.0, 1.0, 1.0], &Limits::STANDARD).map(|polygons| polygons.outlines())
}

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

#[cfg(test)]
mod tests {
    use super::{Frame, Limits, check_syntax, path};
    use crate::ops::length;
    use crate::ops::raster::Polygons;

    const BOX: Frame = [10.0, 20.0, 110.0, 70.0]; // 100 px wide, 50 px tall

    /// The corners of each outline of the path `d` in `BOX`.
    fn outlines_of(d: &str) -> Result<Vec<Vec<[f64; 2]>>, String> {
        path(d, BOX, &Limits::STANDARD).map(|polygons| all(&polygons))
    }

    /// A small exponent.
    fn exponent(power: usize) -> i32 {
        i32::try_from(power).unwrap_or(0)
    }

    fn all(polygons: &Polygons) -> Vec<Vec<[f64; 2]>> {
        (0..polygons.outlines())
            .map(|at| polygons.outline(at).to_vec())
            .collect()
    }

    /// Whether the corners agree to `1e-9`.
    fn same(one: &[Vec<[f64; 2]>], other: &[Vec<[f64; 2]>]) -> bool {
        one.len() == other.len()
            && one.iter().zip(other).all(|(a, b)| {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|(p, q)| (p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9)
            })
    }

    /// The area enclosed by the outlines, by the shoelace formula (signed, outline by outline).
    fn area(outlines: &[Vec<[f64; 2]>]) -> f64 {
        outlines
            .iter()
            .map(|outline| {
                (0..outline.len())
                    .map(|at| {
                        let (p, q) = (outline[at], outline[(at + 1) % outline.len()]);
                        p[0] * q[1] - q[0] * p[1]
                    })
                    .sum::<f64>()
                    / 2.0
            })
            .sum::<f64>()
            .abs()
    }

    #[test]
    fn coordinates_are_shares_of_the_frame() -> Result<(), String> {
        let outlines = outlines_of("M 0 0 L 1 0 L 1 1 L 0.5 1 Z")?;
        assert_eq!(outlines, [[[10.0, 20.0], [110.0, 20.0], [110.0, 70.0], [60.0, 70.0]]]);
        Ok(())
    }

    #[test]
    fn relative_smooth_and_compact_commands_mean_what_the_absolute_ones_do() -> Result<(), String> {
        // (the command written another way, the absolute commands it stands for)
        let rows = [
            ("m 0.1 0.1 l 0.4 0 l 0 0.4 z", "M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 Z"),
            (
                "M 0.1 0.1 h 0.4 v 0.4 H 0.1 Z",
                "M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 L 0.1 0.5 Z",
            ),
            ("M.1.1L.5.1.5.5Z", "M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 Z"),
            (
                "M 0.1,0.1 c 0.1 0 0.3 0 0.4 0.4 z",
                "M 0.1 0.1 C 0.2 0.1 0.4 0.1 0.5 0.5 Z",
            ),
            (
                "M 0.1 0.1 C 0.2 0.1 0.4 0.1 0.5 0.5 s 0.2 0.2 0.3 0 Z",
                "M 0.1 0.1 C 0.2 0.1 0.4 0.1 0.5 0.5 C 0.6 0.9 0.7 0.7 0.8 0.5 Z",
            ),
            (
                "M 0.1 0.5 Q 0.3 0.1 0.5 0.5 T 0.9 0.5",
                "M 0.1 0.5 Q 0.3 0.1 0.5 0.5 Q 0.7 0.9 0.9 0.5",
            ),
            ("M1e-1 .1l4e-1 0", "M 0.1 0.1 L 0.5 0.1"),
        ];
        for (relative, absolute) in rows {
            let (got, want) = (outlines_of(relative)?, outlines_of(absolute)?);
            assert!(same(&got, &want), "{relative:?}");
        }
        Ok(())
    }

    #[test]
    fn an_arc_circle_has_the_area_of_a_circle_and_in_a_wide_frame_that_of_an_ellipse() -> Result<(), String> {
        let circle = "M 0.25 0.5 A 0.25 0.25 0 1 0 0.75 0.5 A 0.25 0.25 0 1 0 0.25 0.5 Z";
        let square = all(&path(circle, [0.0, 0.0, 200.0, 200.0], &Limits::STANDARD)?);
        assert!((area(&square) - std::f64::consts::PI * 50.0 * 50.0).abs() < 2e-3 * std::f64::consts::PI * 2500.0);
        // the unit square is stretched to the frame: the circle of radius 1/4 is an ellipse of 100 x 25 px radii
        let wide = all(&path(circle, [0.0, 0.0, 400.0, 100.0], &Limits::STANDARD)?);
        assert!((area(&wide) - std::f64::consts::PI * 100.0 * 25.0).abs() < 2e-3 * std::f64::consts::PI * 2500.0);
        Ok(())
    }

    #[test]
    fn arc_flags_choose_between_the_four_arcs_through_two_points() -> Result<(), String> {
        // From (0.2, 0.5) to (0.8, 0.5) with radius 0.5 in a 100 px square: the chord is 60 px, so the circle's centre is 40 px
        // above or below its middle, and the arc rises (clockwise, sweep 1) or sinks (sweep 0) by 50 - 40 = 10 px the short way
        // and 50 + 40 = 90 px the long way (SVG 1.1, F.6.2). (large, sweep, rise, sink)
        for (large, sweep, rise, sink) in [
            (0, 1, 10.0, 0.0),
            (1, 1, 90.0, 0.0),
            (0, 0, 0.0, 10.0),
            (1, 0, 0.0, 90.0),
        ] {
            let d = format!("M 0.2 0.5 A 0.5 0.5 0 {large} {sweep} 0.8 0.5");
            let outlines = all(&path(&d, [0.0, 0.0, 100.0, 100.0], &Limits::STANDARD)?);
            let heights: Vec<f64> = outlines[0].iter().map(|corner| corner[1]).collect();
            let (top, bottom) = (
                heights.iter().copied().fold(f64::MAX, f64::min),
                heights.iter().copied().fold(f64::MIN, f64::max),
            );
            assert!(
                (50.0 - top - rise).abs() < 0.2 && (bottom - 50.0 - sink).abs() < 0.2,
                "large {large}, sweep {sweep}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_curve_stays_within_a_twentieth_of_a_pixel_of_the_curve() -> Result<(), String> {
        // the curve's own Bernstein form against the chords that stand for it, in a 400 x 300 frame
        let rows: [(&str, &[[f64; 2]]); 2] = [
            (
                "M 0.05 0.9 C 0.1 -0.4 0.9 1.4 0.95 0.1",
                &[[0.05, 0.9], [0.1, -0.4], [0.9, 1.4], [0.95, 0.1]],
            ),
            (
                "M 0.05 0.9 Q 0.5 -0.6 0.95 0.1",
                &[[0.05, 0.9], [0.5, -0.6], [0.95, 0.1]],
            ),
        ];
        for (d, controls) in rows {
            let outlines = all(&path(d, [0.0, 0.0, 400.0, 300.0], &Limits::STANDARD)?);
            let chords = &outlines[0];
            let points: Vec<[f64; 2]> = controls
                .iter()
                .map(|point| [point[0] * 400.0, point[1] * 300.0])
                .collect();
            let degree = points.len() - 1;
            let binomial = |k: usize| -> f64 {
                (1..=k)
                    .map(|i| length::float(degree + 1 - i) / length::float(i))
                    .product()
            };
            let mut worst = 0.0_f64;
            for step in 0..=4000 {
                let t = f64::from(step) / 4000.0;
                let on_curve: [f64; 2] = [0, 1].map(|axis| {
                    (0..=degree)
                        .map(|k| {
                            binomial(k) * (1.0 - t).powi(exponent(degree - k)) * t.powi(exponent(k)) * points[k][axis]
                        })
                        .sum()
                });
                let nearest = chords
                    .windows(2)
                    .map(|pair| {
                        let (a, b) = (pair[0], pair[1]);
                        let ab = [b[0] - a[0], b[1] - a[1]];
                        let along = (((on_curve[0] - a[0]) * ab[0] + (on_curve[1] - a[1]) * ab[1])
                            / (ab[0] * ab[0] + ab[1] * ab[1]))
                            .clamp(0.0, 1.0);
                        (on_curve[0] - a[0] - along * ab[0]).hypot(on_curve[1] - a[1] - along * ab[1])
                    })
                    .fold(f64::MAX, f64::min);
                worst = worst.max(nearest);
            }
            assert!(worst <= 0.05, "{d}: {worst} px");
            assert_eq!(chords.first(), Some(&points[0]));
            assert_eq!(chords.last(), points.last());
        }
        Ok(())
    }

    #[test]
    fn drawing_after_a_close_goes_on_from_where_the_closed_subpath_began_and_each_subpath_is_an_outline()
    -> Result<(), String> {
        let outlines = outlines_of("M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 Z L 0.9 0.9")?;
        assert_eq!(
            outlines,
            [
                vec![[20.0, 25.0], [60.0, 25.0], [60.0, 45.0]],
                vec![[20.0, 25.0], [100.0, 65.0]]
            ]
        );
        let two = outlines_of("M 0 0 L 0.5 0 L 0.5 0.5 Z M 0.6 0.6 L 1 0.6 L 1 1 Z")?;
        assert_eq!(two.iter().map(Vec::len).collect::<Vec<_>>(), [3, 3]);
        Ok(())
    }

    #[test]
    fn path_data_that_is_no_path_is_refused_naming_the_problem() {
        let rows = [
            ("", "empty"),
            ("   ", "empty"),
            ("L 0.5 0.5", "start with M"),
            ("M 0 0 L foo", "position 9"),
            ("M 0 0 X 1 1", "position 7"),
            ("M 0 0 L 1", "valid SVG path data"),
            ("M 0 0 L 1e999 0", "valid SVG path data"),
            ("M 0 0 A 1 1 0 2 0 1 1", "valid SVG path data"),
            ("M 0 0 L 0.5 0.5 Z M", "valid SVG path data"),
            ("M 0 0 C 0 0 1e9 1e9 0 0", "too far"),
            ("M 0 0 C 0 0 1e999 1e999 0 0", "valid SVG path data"),
        ];
        for (d, mentions) in rows {
            let refused = check_syntax(d);
            assert!(
                matches!(&refused, Err(said) if said.contains(mentions)),
                "{d:?}: {refused:?}"
            );
        }
        assert_eq!(check_syntax("M 0 0 L 1 1 Z M 0.5 0.5 L 1 0"), Ok(2));
    }

    #[test]
    fn each_limit_is_met_at_its_value_and_refused_one_past_it() {
        let small = |limits: Limits, d: &str| path(d, [0.0, 0.0, 1.0, 1.0], &limits).map(|polygons| polygons.corners());
        // a cubic from (0,0) with control points (0,0), (x,0) and the end (0,0): bend is 2x, so it needs ceil(sqrt(30 x))
        // chords: x = 31 gives sqrt(930) = 30.5, so 31 chords
        let curve = "M 0 0 C 0 0 31 0 0 0";
        assert_eq!(
            small(
                Limits {
                    chords: 31,
                    ..Limits::STANDARD
                },
                curve
            ),
            Ok(32)
        );
        let refused = small(
            Limits {
                chords: 30,
                ..Limits::STANDARD
            },
            curve,
        );
        assert!(
            matches!(&refused, Err(said) if said.contains("30 chords")),
            "{refused:?}"
        );
        // a corner or control point may be `far` from the origin, and no farther
        let limits = Limits {
            far: 100.0,
            ..Limits::STANDARD
        };
        assert_eq!(small(limits, "M 0 0 L 100 -100"), Ok(2));
        assert!(matches!(small(limits, "M 0 0 L 100.5 0"), Err(said) if said.contains("too far")));
        assert!(matches!(small(limits, "M 0 0 C 0 0 0 -101 0 0"), Err(said) if said.contains("too far")));
        // so many corners
        let limits = Limits {
            corners: 4,
            ..Limits::STANDARD
        };
        assert_eq!(small(limits, "M 0 0 L 1 0 L 1 1 L 0 1 Z"), Ok(4));
        assert!(
            matches!(small(limits, "M 0 0 L 1 0 L 1 1 L 0 1 L 0 0.5 Z"), Err(said) if said.contains("more than 4 corners"))
        );
    }
}

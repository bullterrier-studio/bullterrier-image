//! Cuts: the pieces that straight and curved cuts divide a frame into.
//!
//! Every cut splits the picture's pixels in two, and a pixel's piece is the set of sides it is on, one bit for each cut. The
//! sets are numbered by the first pixel that has them (see [`pieces`](super::pieces)), so which side of a cut is which does
//! not matter.

use super::length;
use super::outlines::Frame;
use super::pieces;
use super::raster::{self, Polygons};

/// The corners of a frame, numbered clockwise from the top left; also the edges, numbered from the top.
const CORNERS: usize = 4; // constant: a rectangle has four corners

/// One cut.
pub(crate) enum Cut {
    /// A line through the point at the shares `x` and `y` of the frame, at `angle` degrees clockwise from the right as seen
    /// on the picture. It has no head: an angle and the one 180 further cut alike.
    Line { x: f64, y: f64, angle: f64 },
    /// An open curve in px, as one outline of the polygons, closed along the frame's border to cut off one side.
    Curve(Polygons),
}

/// Where on the border of `frame` the point nearest to `point` lies, as a position from 0 to 4 clockwise from the top left
/// corner: the top is 0 to 1, the right edge 1 to 2, the bottom 2 to 3 (right to left), the left edge 3 to 4.
fn along(point: [f64; 2], frame: Frame) -> f64 {
    let [left, top, right, bottom] = frame;
    let u = ((point[0] - left) / (right - left)).clamp(0.0, 1.0);
    let v = ((point[1] - top) / (bottom - top)).clamp(0.0, 1.0);
    // (distance, position) to the top, the right edge, the bottom and the left edge; the nearest, the earlier on a tie
    let [first, rest @ ..] = [(v, u), (1.0 - u, 1.0 + v), (1.0 - v, 3.0 - u), (u, 4.0 - v)]; // constant: the four edges' positions, 0 to 4
    rest.iter()
        .fold(first, |best, next| {
            if next.0.total_cmp(&best.0).then(next.1.total_cmp(&best.1)).is_lt() {
                *next
            } else {
                best
            }
        })
        .1
}

/// The corners of `frame` clockwise from the top left, in px.
fn frame_corners(frame: Frame) -> [[f64; 2]; CORNERS] {
    let [left, top, right, bottom] = frame;
    [[left, top], [right, top], [right, bottom], [left, bottom]]
}

/// The point on the border of `frame` at `position` (see [`along`]).
fn border_point(position: f64, frame: Frame) -> [f64; 2] {
    let corners = frame_corners(frame);
    let (from, share) = (length::whole(position.floor()) % CORNERS, position - position.floor());
    let (start, finish) = (corners[from], corners[(from + 1) % CORNERS]);
    [0, 1].map(|axis| start[axis] + share * (finish[axis] - start[axis]))
}

/// The curve `curve` (two corners at least) closed: each end is carried straight to the nearest border of `frame`, and the
/// border is walked clockwise from the last end to the first, round every corner of the frame in between. What it encloses
/// is one side of the cut.
fn closed(curve: &[[f64; 2]], frame: Frame) -> Polygons {
    let (first, last) = (curve[0], curve[curve.len() - 1]);
    let (end, start) = (along(last, frame), along(first, frame));
    // how far clockwise from the last end a position is
    let ahead = |position: f64| (position - end).rem_euclid(4.0); // constant: positions run round the border, 0 to 4
    let mut corners: Vec<(f64, [f64; 2])> = frame_corners(frame)
        .into_iter()
        .enumerate()
        .map(|(at, corner)| (ahead(length::float(at)), corner))
        .filter(|(distance, _)| 0.0 < *distance && *distance < ahead(start))
        .collect();
    corners.sort_by(|one, other| one.0.total_cmp(&other.0));
    let mut polygon = Polygons::default();
    polygon.begin(first);
    for corner in &curve[1..] {
        polygon.push(*corner);
    }
    polygon.push(border_point(end, frame));
    for (_, corner) in corners {
        polygon.push(corner);
    }
    polygon.push(border_point(start, frame));
    polygon
}

/// The pieces of a `height` x `width` picture that `cuts` divide the `frame` into, and their number. A line cuts the whole
/// picture, beyond the frame too; a curve's two sides are decided at the pixel centres. With `shift` (the shifts of the
/// pixel centres along x and along y, row after row) the centres are moved first, so the cuts wander.
///
/// There are at most 32 cuts.
///
/// # Errors
/// A message if painting a curve's side would take more than `budget` crossings of an edge with a scanline.
pub(crate) fn pieces(
    height: usize,
    width: usize,
    frame: Frame,
    cuts: &[Cut],
    shift: Option<[&[f32]; 2]>,
    budget: u64,
) -> Result<(Vec<i32>, usize), String> {
    let [left, top, right, bottom] = frame;
    let mut sides = vec![0_u32; height * width];
    for (bit, cut) in cuts.iter().enumerate() {
        match cut {
            Cut::Line { x, y, angle } => {
                let (sin, cos) = angle.to_radians().sin_cos();
                let (across, down) = (left + x * (right - left), top + y * (bottom - top));
                for (at, side) in sides.iter_mut().enumerate() {
                    let (mut x, mut y) = (length::float(at % width) + 0.5, length::float(at / width) + 0.5); // constant: a pixel's centre is half a pixel in
                    if let Some([along_x, along_y]) = shift {
                        x += f64::from(along_x[at]);
                        y += f64::from(along_y[at]);
                    }
                    // the line runs along (cos, sin) on the screen, where y points down, so its normal is (-sin, cos)
                    *side |= u32::from((y - down) * cos - (x - across) * sin > 0.0) << bit;
                }
            }
            Cut::Curve(curve) => {
                let mut beyond = vec![0_i32; height * width];
                raster::paint(&mut beyond, height, width, &closed(curve.outline(0), frame), 1, budget)?;
                if let Some(shift) = shift {
                    beyond = raster::read_floor(&beyond, height, width, shift);
                }
                for (side, on) in sides.iter_mut().zip(beyond) {
                    *side |= u32::from(on == 1) << bit;
                }
            }
        }
    }
    Ok(pieces::renumber_sparse(sides.into_iter().map(u64::from)))
}

#[cfg(test)]
mod tests {
    use super::{Frame, along, closed};

    const FRAME: Frame = [0.0, 0.0, 6.0, 4.0];

    #[test]
    fn a_point_is_carried_to_the_nearest_border_as_a_position_clockwise_from_the_top_left() {
        // (point, position): the middle of the top, the right edge a quarter down, the middle of the bottom, the left edge
        // three quarters down, a point inside nearer the top, a corner, and points beyond the frame
        for (point, position) in [
            ([3.0, 0.0], 0.5),
            ([6.0, 1.0], 1.25),
            ([3.0, 4.0], 2.5),
            ([0.0, 3.0], 3.25),
            ([3.0, 1.0], 0.5),
            ([0.0, 0.0], 0.0),
            ([9.0, 2.0], 1.5),
            ([-2.0, 1.0], 3.75),
        ] {
            assert!((along(point, FRAME) - position).abs() < 1e-12, "{point:?}");
        }
    }

    #[test]
    fn a_curve_is_closed_by_the_border_walked_clockwise_from_its_last_end_to_its_first() {
        let corners = |curve: &[[f64; 2]]| closed(curve, FRAME).outline(0).to_vec();
        // top to bottom: down the right, then along the bottom to the bottom left corner and up the left, back to the top
        assert_eq!(
            corners(&[[3.0, 0.0], [3.0, 4.0]]),
            [[3.0, 0.0], [3.0, 4.0], [3.0, 4.0], [0.0, 4.0], [0.0, 0.0], [3.0, 0.0]]
        );
        // from the left edge to the bottom: only the bottom left corner lies between
        assert_eq!(
            corners(&[[0.0, 2.0], [3.0, 4.0]]),
            [[0.0, 2.0], [3.0, 4.0], [3.0, 4.0], [0.0, 4.0], [0.0, 2.0]]
        );
        // the other way round the same cut: the border walked the long way, round three corners
        assert_eq!(
            corners(&[[3.0, 4.0], [0.0, 2.0]]),
            [
                [3.0, 4.0],
                [0.0, 2.0],
                [0.0, 2.0],
                [0.0, 0.0],
                [6.0, 0.0],
                [6.0, 4.0],
                [3.0, 4.0]
            ]
        );
        // an end inside the frame is carried straight up to the border, here the top
        assert_eq!(
            corners(&[[3.0, 1.0], [3.0, 4.0]]),
            [[3.0, 1.0], [3.0, 4.0], [3.0, 4.0], [0.0, 4.0], [0.0, 0.0], [3.0, 0.0]]
        );
    }
}

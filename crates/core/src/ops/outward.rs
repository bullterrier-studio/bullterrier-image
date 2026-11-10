//! How far each element of a shape is from its edge, and which way is out.
//!
//! For effects that grow things away from a shape's outline, on a mask of any size: a full-resolution matte or a coarse grid
//! of cells.

use super::blur::gaussian_filter_plane;
use super::distance::to_nearest_sampled;
use super::edge::Extension;

/// The distance of each element from the edge of a shape, and the direction out.
pub(crate) struct Outward {
    /// Distance from each element to the nearest edge of the shape, in the unit of the sampling; 0 outside it.
    pub(crate) distance: Vec<f32>,
    /// The unit vector `[x, y]` pointing out of the shape (x to the right, y down); `[0, -1]` (up) where the distance is
    /// flat, because up is where things drift by default.
    pub(crate) direction: Vec<[f32; 2]>,
}

/// How the shape is measured.
pub(crate) struct Measure {
    /// `(row_step, column_step)`: the size of one element, so that a grid of tall characters gives distances in px.
    pub(crate) sampling: (f64, f64),
    /// Whether the bottom border is no edge, as when a portrait is cropped at the chest. The left, right and top borders
    /// always are.
    pub(crate) open_bottom: bool,
    /// Sigma, in elements, of the Gaussian blur of the distance before its slope is read (0 = none): more turns the
    /// direction more gradually and lets it reach further beyond the shape.
    pub(crate) smoothing: f64,
    /// Where the blur's kernel is cut off, in sigmas.
    pub(crate) truncate: f64,
    /// What the blur sees beyond the shape's array.
    pub(crate) edge: Extension,
}

/// The distance from the edge and the outward direction for every element of `shape` (`height` x `width` flags row after
/// row, true = inside), measured as `measure` says. The edge is the boundary between true and false plus the borders of
/// the array. The direction is the negative slope (central differences, one-sided at the ends, per unit of sampling) of the
/// smoothed distance; a slope under 1e-6 is float noise or beyond the smoothing's reach, and has no direction.
pub(crate) fn outward(shape: &[bool], (height, width): (usize, usize), measure: &Measure) -> Outward {
    let (row_step, column_step) = measure.sampling;
    // A border is an edge by putting a row or column of "outside" next to it. Leaving the bottom unpadded makes it open:
    // nothing outside the array can be nearer than what is already in it.
    let padded_height = height + 1 + usize::from(!measure.open_bottom);
    let padded_width = width + 2; // constant: one column of "outside" at each side
    let mut targets = vec![true; padded_height * padded_width];
    for row in 0..height {
        for column in 0..width {
            targets[(row + 1) * padded_width + column + 1] = !shape[row * width + column];
        }
    }
    let padded = to_nearest_sampled(&targets, (padded_height, padded_width), measure.sampling);
    let distance: Vec<f32> = (0..height)
        .flat_map(|row| {
            let line = &padded[(row + 1) * padded_width + 1..][..width];
            line.iter().copied()
        })
        .collect();
    let smooth = gaussian_filter_plane(
        &distance,
        (height, width),
        measure.smoothing,
        measure.truncate,
        measure.edge,
    );
    let slope = |before: f32, after: f32, span: f64, step: f64| (f64::from(after) - f64::from(before)) / (span * step);
    let direction = (0..height * width)
        .map(|at| {
            let (row, column) = (at / width, at % width);
            let down = if height > 1 {
                let (first, last) = (row.saturating_sub(1), (row + 1).min(height - 1));
                slope(
                    smooth[first * width + column],
                    smooth[last * width + column],
                    super::length::float(last - first),
                    row_step,
                )
            } else {
                0.0
            };
            let across = if width > 1 {
                let (first, last) = (column.saturating_sub(1), (column + 1).min(width - 1));
                slope(
                    smooth[row * width + first],
                    smooth[row * width + last],
                    super::length::float(last - first),
                    column_step,
                )
            } else {
                0.0
            };
            let length = across.hypot(down);
            if length < FLAT {
                [0.0, -1.0] // constant: up, where things drift by default
            } else {
                #[expect(clippy::cast_possible_truncation, reason = "a unit vector in f32")]
                [(-across / length) as f32, (-down / length) as f32]
            }
        })
        .collect();
    Outward { distance, direction }
}

/// A slope under this per unit of sampling is float noise, or beyond the smoothing's reach: there is no direction.
const FLAT: f64 = 1e-6; // constant: far above f32 rounding of a distance of a few hundred, far below any real slope

#[cfg(test)]
mod tests {
    use super::{Measure, outward};
    use crate::ops::edge::Extension;

    fn measure(open_bottom: bool, smoothing: f64) -> Measure {
        Measure {
            sampling: (1.0, 1.0),
            open_bottom,
            smoothing,
            truncate: 4.0,
            edge: Extension::Reflect,
        }
    }

    /// A block of 5 x 5 inside a 7 x 7 mask, at 1..=5. Hand distances (to the nearest outside element): the ring is 1, the
    /// middle 2 in a 5 x 5 block, the centre 3.
    #[test]
    fn the_distance_inside_is_the_steps_to_the_nearest_outside_and_zero_outside() {
        let mut shape = vec![false; 49];
        for row in 1..6 {
            for column in 1..6 {
                shape[row * 7 + column] = true;
            }
        }
        let found = outward(&shape, (7, 7), &measure(false, 0.0));
        assert_eq!(found.distance[0], 0.0);
        assert_eq!(found.distance[7 + 1], 1.0);
        assert_eq!(found.distance[2 * 7 + 2], 2.0);
        assert_eq!(found.distance[3 * 7 + 3], 3.0);
    }

    #[test]
    fn a_border_is_an_edge_except_the_bottom_one_when_it_is_open() {
        let shape = vec![true; 3 * 3];
        let closed = outward(&shape, (3, 3), &measure(false, 0.0));
        let open = outward(&shape, (3, 3), &measure(true, 0.0));
        // closed: bottom row is 1 from the border below it; open: it is 3 from the top and 2 from the sides
        assert_eq!(closed.distance[6 + 1], 1.0);
        assert_eq!(open.distance[6 + 1], 2.0);
        assert_eq!(open.distance[6], 1.0);
    }

    /// Left of a wall of "inside" lies the outside: the way out of a shape that fills the right half points left; a flat
    /// distance points up.
    #[test]
    fn the_direction_points_out_of_the_shape_and_up_where_the_distance_is_flat() {
        let mut shape = vec![false; 5 * 8];
        for row in 0..5 {
            for column in 4..8 {
                shape[row * 8 + column] = true;
            }
        }
        let found = outward(&shape, (5, 8), &measure(true, 0.0));
        // at the middle row, one element inside the wall: the distance rises toward the right, so out is to the left
        assert_eq!(found.direction[2 * 8 + 5], [-1.0, 0.0]);
        // all outside: the distance is zero everywhere, no direction, so up
        let nothing = outward(&[false; 6], (2, 3), &measure(true, 0.0));
        assert!(
            nothing
                .direction
                .iter()
                .all(|direction| direction.as_slice() == [0.0, -1.0])
        );
    }
}

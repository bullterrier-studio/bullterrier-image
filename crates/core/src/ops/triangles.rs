//! A picture cut into triangles: the Delaunay triangles of some sites and of points round the picture's edge, and the
//! triangle each pixel's centre lies in.
//!
//! Coordinates: a point is `(x, y)` in px from the picture's top left corner, and the pixel in row `i`, column `j` has its
//! centre at `(j + 0.5, i + 0.5)`. The triangulation is the `delaunator` crate's; its types stop at this module.

use super::length;

/// The three corners of a triangle, `(x, y)` in px.
pub(crate) type Triangle = [[f64; 2]; 3];

/// How far outside a triangle a pixel's centre may lie and still be placed in it: float rounding must never leave a centre
/// on a shared side in neither triangle.
const SLACK: f64 = 1e-9; // constant: far above the rounding of coordinates of a few thousand px, far below a pixel

/// Points round the edge of a `height` x `width` picture, clockwise from the top left corner: each side cut into as many
/// equal parts as are about `spacing` px long (at least one), so the four corners are among them.
fn border(height: usize, width: usize, spacing: f64) -> Vec<[f64; 2]> {
    let (rows, columns) = (length::float(height), length::float(width));
    let across = length::whole(columns / spacing).max(1);
    let down = length::whole(rows / spacing).max(1);
    let mut points = Vec::with_capacity(2 * (across + down));
    points.extend((0..across).map(|k| [columns * length::float(k) / length::float(across), 0.0]));
    points.extend((0..down).map(|k| [columns, rows * length::float(k) / length::float(down)]));
    points.extend((0..across).map(|k| [columns - columns * length::float(k) / length::float(across), rows]));
    points.extend((0..down).map(|k| [0.0, rows - rows * length::float(k) / length::float(down)]));
    points
}

/// The Delaunay triangles of `sites` (inside a `height` x `width` picture) and of points round its edge, `spacing` px apart
/// or a little more or less (each side is cut into equal parts, at least one, so the four corners are among the points):
/// they tile the picture, and none fans out of a corner. A site that coincides with another point is left out.
pub(crate) fn triangles(height: usize, width: usize, sites: &[[f64; 2]], spacing: f64) -> Vec<Triangle> {
    let points: Vec<[f64; 2]> = sites.iter().copied().chain(border(height, width, spacing)).collect();
    let input: Vec<delaunator::Point> = points.iter().map(|&[x, y]| delaunator::Point { x, y }).collect();
    delaunator::triangulate(&input)
        .triangles
        .as_chunks::<3>()
        .0
        .iter()
        .map(|corners| corners.map(|corner| points[corner]))
        .collect()
}

/// The index in `triangles` (which tile a `height` x `width` picture) of the triangle that holds each pixel's centre, row
/// after row. A centre on a side shared by triangles goes to the first of them.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "a few thousand triangles at most"
)]
pub(crate) fn labels(height: usize, width: usize, triangles: &[Triangle]) -> Vec<i32> {
    let mut found = vec![0_i32; height * width];
    let mut placed = vec![false; height * width];
    for (index, triangle) in triangles.iter().enumerate() {
        let (top, bottom) = triangle
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), p| {
                (low.min(p[1]), high.max(p[1]))
            });
        // the rows whose centres lie between the triangle's highest and lowest corner
        let first = length::whole((top - 0.5 - SLACK).ceil());
        let last = length::whole((bottom - 0.5 + SLACK).floor() + 1.0).min(height);
        for row in first..last {
            let centre = length::float(row) + 0.5;
            // where the line through the row's centres crosses the triangle's sides
            let (mut left, mut right) = (f64::INFINITY, f64::NEG_INFINITY);
            for (a, b) in [(0, 1), (1, 2), (2, 0)].map(|(a, b)| (triangle[a], triangle[b])) {
                let (low, high) = (a[1].min(b[1]), a[1].max(b[1]));
                if centre < low - SLACK || centre > high + SLACK {
                    continue;
                }
                let across = if high - low < SLACK {
                    [a[0], b[0]]
                } else {
                    let at = ((centre - a[1]) / (b[1] - a[1])).clamp(0.0, 1.0);
                    [a[0] + at * (b[0] - a[0]); 2]
                };
                for x in across {
                    left = left.min(x);
                    right = right.max(x);
                }
            }
            if left > right {
                continue;
            }
            let from = length::whole((left - 0.5 - SLACK).ceil());
            let to = length::whole((right - 0.5 + SLACK).floor() + 1.0).min(width);
            for at in row * width + from..row * width + to.max(from) {
                if !placed[at] {
                    placed[at] = true;
                    found[at] = index as i32;
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::random::Rng;

    /// `count` random sites strictly inside a `height` x `width` picture.
    fn scattered(count: usize, (height, width): (usize, usize), seed: u32) -> Vec<[f64; 2]> {
        let mut rng = Rng::seeded(seed);
        (0..count)
            .map(|_| {
                [
                    0.5 + rng.uniform() * (length::float(width) - 1.0),
                    0.5 + rng.uniform() * (length::float(height) - 1.0),
                ]
            })
            .collect()
    }

    fn area(triangle: &Triangle) -> f64 {
        let [a, b, c] = *triangle;
        ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
    }

    /// Twice the signed area of the triangle `a`, `b`, `p`: which side of the line a-b the point is on.
    fn side(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    }

    /// Whether `p` is inside the triangle or within `slack` (twice-area units) of its sides.
    fn holds(triangle: &Triangle, p: [f64; 2], slack: f64) -> bool {
        let [a, b, c] = *triangle;
        let sides = [side(a, b, p), side(b, c, p), side(c, a, p)];
        sides.iter().all(|s| *s >= -slack) || sides.iter().all(|s| *s <= slack)
    }

    /// The centre of the circle through the triangle's corners, by the textbook formula.
    fn circumcentre(triangle: &Triangle) -> [f64; 2] {
        let [a, b, c] = *triangle;
        let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
        let (a2, b2, c2) = (
            a[0].powi(2) + a[1].powi(2),
            b[0].powi(2) + b[1].powi(2),
            c[0].powi(2) + c[1].powi(2),
        );
        [
            (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d,
            (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d,
        ]
    }

    /// The distinct corners of the triangles.
    fn vertices(triangles: &[Triangle]) -> Vec<[f64; 2]> {
        let mut all: Vec<[f64; 2]> = triangles.iter().flatten().copied().collect();
        all.sort_by(|p, q| p[0].total_cmp(&q[0]).then(p[1].total_cmp(&q[1])));
        all.dedup();
        all
    }

    const SIZES: [(usize, usize); 3] = [(23, 37), (40, 30), (9, 61)];

    #[test]
    fn no_corner_of_any_triangle_lies_inside_the_circle_through_another() {
        for (seed, (count, size)) in (0_u32..).zip([(3, SIZES[0]), (8, SIZES[1]), (25, SIZES[2])]) {
            let mesh = triangles(size.0, size.1, &scattered(count, size, seed), 9.0);
            let corners = vertices(&mesh);
            assert_ne!(mesh.len(), 0);
            for triangle in &mesh {
                let centre = circumcentre(triangle);
                let radius = (triangle[0][0] - centre[0]).hypot(triangle[0][1] - centre[1]);
                for corner in &corners {
                    let away = (corner[0] - centre[0]).hypot(corner[1] - centre[1]);
                    assert!(
                        away >= radius * (1.0 - 1e-9),
                        "{corner:?} is inside the circle of {triangle:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn n_corners_with_h_on_the_picture_edge_make_2n_minus_2_minus_h_triangles() {
        for (seed, size) in (0_u32..).zip(SIZES) {
            let (height, width) = (length::float(size.0), length::float(size.1));
            let mesh = triangles(size.0, size.1, &scattered(14, size, seed + 40), 7.0);
            let corners = vertices(&mesh);
            let on_edge = |p: &[f64; 2]| p[0] < 1e-9 || width - p[0] < 1e-9 || p[1] < 1e-9 || height - p[1] < 1e-9;
            let hull = corners.iter().filter(|p| on_edge(p)).count();
            assert_eq!(mesh.len(), 2 * corners.len() - 2 - hull, "{size:?}");
        }
    }

    #[test]
    fn with_no_sites_each_side_is_cut_into_equal_parts_about_the_spacing_long() {
        // 40 wide, 30 high, spacing 10: 4 parts across and 3 down, 14 points all on the hull, so 14 - 2 triangles
        let mesh = triangles(30, 40, &[], 10.0);
        assert_eq!(vertices(&mesh).len(), 14);
        assert_eq!(mesh.len(), 12);
        // a spacing longer than a side still cuts it in one part: the four corners
        assert_eq!(vertices(&triangles(30, 40, &[], 1000.0)).len(), 4);
    }

    #[test]
    fn the_four_corners_are_corners_and_the_triangles_tile_the_picture() {
        for (seed, size) in (0_u32..).zip(SIZES) {
            let (height, width) = (length::float(size.0), length::float(size.1));
            let mesh = triangles(size.0, size.1, &scattered(11, size, seed + 70), 8.0);
            let corners = vertices(&mesh);
            for corner in [[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]] {
                assert!(corners.contains(&corner), "{corner:?}");
            }
            let total: f64 = mesh.iter().map(area).sum();
            assert!(
                (total - height * width).abs() < 1e-6,
                "{total} against {}",
                height * width
            );
        }
    }

    #[test]
    fn every_pixel_goes_to_the_one_triangle_that_holds_its_centre() {
        for (seed, size) in (0_u32..).zip(SIZES) {
            let mesh = triangles(size.0, size.1, &scattered(9, size, seed + 90), 8.0);
            let found = labels(size.0, size.1, &mesh);
            for (at, label) in found.iter().enumerate() {
                let centre = [length::float(at % size.1) + 0.5, length::float(at / size.1) + 0.5];
                let inside: Vec<usize> = (0..mesh.len()).filter(|&t| holds(&mesh[t], centre, 1e-7)).collect();
                assert!(
                    inside.contains(&usize::try_from(*label).unwrap_or(usize::MAX)),
                    "pixel {at}: label {label}, held by {inside:?}"
                );
                // a centre clear of every side is held by exactly one triangle, the label's
                let clear: Vec<usize> = (0..mesh.len()).filter(|&t| holds(&mesh[t], centre, -1e-7)).collect();
                assert!(clear.len() <= 1, "pixel {at}: {clear:?}");
                assert!(
                    clear.is_empty() || clear == [usize::try_from(*label).unwrap_or(usize::MAX)],
                    "pixel {at}: {label}, {clear:?}"
                );
            }
        }
    }

    #[test]
    fn duplicate_and_collinear_sites_leave_a_tiling_without_panicking() {
        let size = (20, 30);
        let line: Vec<[f64; 2]> = (1..10).map(|k| [length::float(k) * 3.0, 10.0]).collect();
        for sites in [
            vec![[5.0, 5.0]; 6],
            line,
            vec![[0.0, 0.0], [30.0, 20.0], [15.0, 10.0], [15.0, 10.0]],
            vec![[2.5, 3.5], [2.5, 3.5], [17.0, 9.0]],
        ] {
            let mesh = triangles(size.0, size.1, &sites, 7.0);
            let total: f64 = mesh.iter().map(area).sum();
            assert!((total - 600.0).abs() < 1e-6, "{sites:?}: {total}");
            assert_eq!(labels(size.0, size.1, &mesh).len(), 600);
        }
    }
}

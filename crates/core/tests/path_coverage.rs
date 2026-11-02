//! What a `path` region selects, checked against geometry done independently of the crate: areas of shapes worked by hand
//! or by numeric integration, the coverage of a pixel by a rectangle, and a dense point-by-point winding-number test. The
//! path data is in shares of the picture, so a picture of known size turns a share into a known number of pixels.

mod common;

use bullterrier_image::Error;
use bullterrier_image::region::Path;
use common::ops::{float, selection, total};
use proptest::prelude::*;

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// The selection of the path `d` on a `height` x `width` picture, with `samples` scanlines a row.
fn covered(height: usize, width: usize, d: &str, samples: u32) -> Result<Vec<f32>, Error> {
    selection(
        (height, width),
        None,
        Path {
            d: d.to_owned(),
            samples,
            ..Path::default()
        },
    )
}

/// `actual` is `expected` to within `by`, with the first index at which it is not for the message.
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

/// The area of a polygon, by the shoelace formula.
fn shoelace(corners: &[[f64; 2]]) -> f64 {
    (0..corners.len())
        .map(|at| {
            let (p, q) = (corners[at], corners[(at + 1) % corners.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

#[test]
fn coordinates_are_shares_of_the_frame() -> Outcome {
    // 100 x 50 px: the polygon (0, 0), (100, 0), (100, 50), (50, 50) is a trapezoid of (100 + 50) / 2 x 50. A slanting edge
    // is measured exactly by scanlines at the middle of each share of a row, so the area is exact
    let values = covered(50, 100, "M 0 0 L 1 0 L 1 1 L 0.5 1 Z", 4)?;
    assert!((total(&values) - 3750.0).abs() < 0.5, "{}", total(&values));
    // pixel (row 0, column 99) is wholly inside it and pixel (row 49, column 0) wholly outside
    assert!((values[99] - 1.0).abs() < 1e-6 && values[49 * 100].abs() < 1e-6);
    Ok(())
}

#[test]
fn relative_smooth_and_compact_commands_mean_what_the_absolute_ones_do() -> Outcome {
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
            "M 0.1 0.5 Q 0.3 0.1 0.5 0.5 T 0.9 0.5 Z",
            "M 0.1 0.5 Q 0.3 0.1 0.5 0.5 Q 0.7 0.9 0.9 0.5 Z",
        ),
        ("M1e-1 .1l4e-1 0 0 4e-1z", "M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 Z"),
    ];
    for (relative, absolute) in rows {
        let (got, want) = (covered(40, 60, relative, 4)?, covered(40, 60, absolute, 4)?);
        assert!(total(&want) > 10.0, "{absolute:?} selects nothing");
        assert_eq!(close(&got, &want, 1e-4), Ok(()), "{relative:?}");
    }
    Ok(())
}

#[test]
fn an_arc_circle_has_the_area_of_a_circle_and_in_a_wide_frame_that_of_an_ellipse() -> Outcome {
    let circle = "M 0.25 0.5 A 0.25 0.25 0 1 0 0.75 0.5 A 0.25 0.25 0 1 0 0.25 0.5 Z";
    let square = total(&covered(200, 200, circle, 16)?);
    assert!(
        (square - std::f64::consts::PI * 50.0 * 50.0).abs() < 2e-3 * std::f64::consts::PI * 2500.0,
        "{square}"
    );
    // the unit square is stretched to the frame: the circle of radius 1/4 is an ellipse of 100 x 25 px radii
    let wide = total(&covered(100, 400, circle, 16)?);
    assert!(
        (wide - std::f64::consts::PI * 100.0 * 25.0).abs() < 2e-3 * std::f64::consts::PI * 2500.0,
        "{wide}"
    );
    Ok(())
}

#[test]
fn arc_flags_choose_between_the_four_arcs_through_two_points() -> Outcome {
    // From (0.35, 0.5) to (0.65, 0.5) with radius 0.25 in a 200 px square: the chord is 60 px, so the circle's centre is 40 px
    // above or below its middle, and the arc rises (clockwise, sweep 1) or sinks (sweep 0) by 50 - 40 = 10 px the short way
    // and 50 + 40 = 90 px the long way (SVG 1.1, F.6.2). Closed by the chord, the shape is a segment of the circle of
    // radius 50: with half angle a = asin(30 / 50), R^2 (a - sin a cos a) the short way and the circle less that the long
    let half_angle = (30.0_f64 / 50.0).asin();
    let short = 2500.0 * (half_angle - half_angle.sin() * half_angle.cos());
    let long = std::f64::consts::PI * 2500.0 - short;
    // the arc is drawn by chords within 0.05 px of it, which an area follows to 0.05 x the arc's length
    let (short_arc, long_arc) = (100.0 * half_angle, 100.0 * (std::f64::consts::PI - half_angle));
    // (large, sweep, rise, sink, area, arc length)
    for (large, sweep, rise, sink, area, arc) in [
        (0, 1, 10.0, 0.0, short, short_arc),
        (1, 1, 90.0, 0.0, long, long_arc),
        (0, 0, 0.0, 10.0, short, short_arc),
        (1, 0, 0.0, 90.0, long, long_arc),
    ] {
        let values = covered(
            200,
            200,
            &format!("M 0.35 0.5 A 0.25 0.25 0 {large} {sweep} 0.65 0.5 Z"),
            16,
        )?;
        let rows: Vec<usize> = (0..200)
            .filter(|&row| values[row * 200..(row + 1) * 200].iter().any(|&value| value > 0.0))
            .collect();
        let (top, bottom) = (rows[0], rows[rows.len() - 1] + 1);
        assert!(
            (float(top) - (100.0 - rise)).abs() <= 1.0 && (float(bottom) - (100.0 + sink)).abs() <= 1.0,
            "large {large}, sweep {sweep}: rows {top} to {bottom}"
        );
        assert!(
            (total(&values) - area).abs() < 0.05 * arc,
            "large {large}, sweep {sweep}: {}",
            total(&values)
        );
    }
    Ok(())
}

#[test]
fn a_curve_stays_within_a_twentieth_of_a_pixel_of_the_curve() -> Outcome {
    // the area between the curve and its chord, by the curve's own Bernstein form on 4000 points (de Casteljau), against
    // what the path selects. A polygon of chords within 0.05 px of a curve differs from it in area by at
    // most 0.05 x 500. The controls lie in the picture, and so does the curve
    let rows: [(&str, &[[f64; 2]]); 2] = [
        (
            "M 0.05 0.9 C 0.1 0.0 0.9 0.2 0.95 0.9 Z",
            &[[0.05, 0.9], [0.1, 0.0], [0.9, 0.2], [0.95, 0.9]],
        ),
        (
            "M 0.05 0.9 Q 0.5 0.0 0.95 0.1 Z",
            &[[0.05, 0.9], [0.5, 0.0], [0.95, 0.1]],
        ),
    ];
    for (d, controls) in rows {
        let points: Vec<[f64; 2]> = controls.iter().map(|p| [p[0] * 400.0, p[1] * 300.0]).collect();
        let on_curve: Vec<[f64; 2]> = (0..=4000)
            .map(|step| {
                let t = f64::from(step) / 4000.0;
                let mut level = points.clone();
                while level.len() > 1 {
                    level = level
                        .windows(2)
                        .map(|pair| [0, 1].map(|axis| pair[0][axis] * (1.0 - t) + pair[1][axis] * t))
                        .collect();
                }
                level[0]
            })
            .collect();
        let exact = shoelace(&on_curve);
        let length: f64 = on_curve
            .windows(2)
            .map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]))
            .sum();
        let values = covered(300, 400, d, 16)?;
        assert!(
            (total(&values) - exact).abs() < 0.05 * length,
            "{d}: {} against {exact}",
            total(&values)
        );
    }
    Ok(())
}

#[test]
fn drawing_after_a_close_goes_on_from_where_the_closed_subpath_began_and_each_subpath_is_an_outline() -> Outcome {
    // in 100 x 100 px: the triangle (10, 10), (50, 10), (50, 50) is 800 px; after the close the next line goes on from its
    // start (10, 10), so the second outline is (10, 10), (90, 90), (10, 90), 3200 px, with only the diagonal in common
    // with the first. Started from the last point (50, 50) instead it would be 1600 px
    let values = covered(100, 100, "M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 Z L 0.9 0.9 L 0.1 0.9 Z", 16)?;
    assert!((total(&values) - 4000.0).abs() < 4.0, "{}", total(&values));
    // two subpaths with an M each: 50 x 50 / 2 and 40 x 40 / 2
    let two = covered(100, 100, "M 0 0 L 0.5 0 L 0.5 0.5 Z M 0.6 0.6 L 1 0.6 L 1 1 Z", 16)?;
    assert!((total(&two) - 2050.0).abs() < 3.0, "{}", total(&two));
    Ok(())
}

#[test]
fn path_data_that_is_no_path_is_refused_naming_the_region_and_the_problem() {
    let rows = [
        ("   ", "empty"),
        ("M 0 0 A 1 1 0 2 0 1 1", "valid SVG path data"),
        ("M 0 0 L 0.5 0.5 Z M", "valid SVG path data"),
        ("M 0 0 C 0 0 1e999 1e999 0 0", "valid SVG path data"),
    ];
    for (d, mentions) in rows {
        let refused = covered(8, 8, d, 4);
        assert!(
            matches!(&refused, Err(error) if error.to_string().contains("region 'r'") && error.to_string().contains(mentions)),
            "{d:?}: {refused:?}"
        );
    }
    // and path data of two subpaths is accepted
    assert!(covered(8, 8, "M 0 0 L 1 1 Z M 0.5 0.5 L 1 0", 4).is_ok());
}

#[test]
fn a_rectangle_covers_each_pixel_by_the_share_of_it_inside_measured_on_the_scanlines() -> Outcome {
    // x from 0.5 to 3.5 covers pixels 0 and 3 by a half and 1 and 2 fully; y from 0.5 to 2.25: with 4 scanlines a row
    // (at 1/8, 3/8, 5/8, 7/8 of it) row 0 has two of its four inside, row 1 all four, row 2 one (at 2.125). A 4 x 3
    // picture: the shares are 0.125, 0.875, and 1/6 and 0.75 of the height
    let d = format!("M 0.125 {0} H 0.875 V 0.75 H 0.125 Z", 0.5 / 3.0);
    let values = covered(3, 4, &d, 4)?;
    let rows = [[0.25, 0.5, 0.5, 0.25], [0.5, 1.0, 1.0, 0.5], [0.125, 0.25, 0.25, 0.125]].concat();
    assert_eq!(close(&values, &rows, 1e-6), Ok(()), "{values:?}");
    // the exact area is 3 x 1.75 = 5.25 and every scanline of a rectangle measures it exactly
    assert!((total(&values) - 5.25).abs() < 1e-5);
    Ok(())
}

#[test]
fn an_edge_on_a_scanline_belongs_to_the_shape_below_it_not_above() -> Outcome {
    // one scanline a row, in the middle of it. In a picture 4 rows tall the top at 0.5 (a share of 0.125) is on the
    // scanline of row 0: inside; the bottom at 2.5 (0.625) is on the scanline of row 2: outside. So rows 0 and 1 are
    // covered, and rows 2 and 3 are not
    let values = covered(4, 2, "M 0 0.125 H 1 V 0.625 H 0 Z", 1)?;
    assert_eq!(
        close(&values, &[1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0], 0.0),
        Ok(()),
        "{values:?}"
    );
    Ok(())
}

#[test]
fn the_direction_an_outline_is_drawn_does_not_matter_and_an_inner_one_the_other_way_round_is_a_hole() -> Outcome {
    let outer = "M 0 0 H 1 V 1 H 0 Z";
    let reversed = "M 0 0 V 1 H 1 V 0 Z";
    assert_eq!(
        close(&covered(6, 6, outer, 2)?, &covered(6, 6, reversed, 2)?, 0.0),
        Ok(())
    );
    // against the way round of the outer outline: a hole of 4 pixels
    let ring = covered(
        6,
        6,
        &format!("{outer} M {0} {0} V {1} H {1} V {0} Z", 2.0 / 6.0, 4.0 / 6.0),
        2,
    )?;
    assert!((total(&ring) - 32.0).abs() < 1e-4, "{}", total(&ring));
    assert!(ring[2 * 6 + 2].abs() < f32::EPSILON);
    // the way the outer goes round: it winds twice there, which is inside under the nonzero rule
    let doubled = covered(
        6,
        6,
        &format!("{outer} M {0} {0} H {1} V {1} H {0} Z", 2.0 / 6.0, 4.0 / 6.0),
        2,
    )?;
    assert!((total(&doubled) - 36.0).abs() < 1e-4, "{}", total(&doubled));
    Ok(())
}

#[test]
fn two_outlines_of_one_call_are_one_shape_and_the_parts_outside_the_picture_are_clipped() -> Outcome {
    // a 4 x 6 picture: the rectangles (0, 0)-(2, 2) and (4, 3)-(6, 4) cover 4 + 2 pixels
    let apart = covered(
        4,
        6,
        &format!(
            "M 0 0 H {0} V 0.5 H 0 Z M {1} 0.75 H 1 V 1 H {1} Z",
            2.0 / 6.0,
            4.0 / 6.0
        ),
        3,
    )?;
    assert!((total(&apart) - 6.0).abs() < 1e-5, "{}", total(&apart));
    // a rectangle reaching beyond the picture on every side covers it all, and one entirely beyond covers nothing
    let big = covered(3, 4, "M -5 -5 H 10 V 10 H -5 Z", 4)?;
    assert_eq!(close(&big, &[1.0; 12], 1e-6), Ok(()));
    let away = covered(3, 4, "M 10 10 H 12 V 12 H 10 Z", 4)?;
    assert_eq!(close(&away, &[0.0; 12], 0.0), Ok(()));
    Ok(())
}

/// The winding number of the outlines round the point `(x, y)` (all scaled to whole numbers), by Sunday's algorithm (an
/// upward edge that the point is left of counts +1, a downward one that it is right of -1; an edge counts for the heights
/// from its start up to but not including its end), in exact integer arithmetic.
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
                f32::from(u16::try_from(hits).unwrap_or(0)) / f32::from(u16::try_from(points * samples).unwrap_or(1)),
            );
        }
    }
    out
}

/// Outlines of three to six corners, in half pixels of an 8 x 8 picture: crossing themselves, nested, any way round.
fn outlines() -> impl Strategy<Value = Vec<Vec<[i64; 2]>>> {
    proptest::collection::vec(proptest::collection::vec([0_i64..=16, 0_i64..=16], 3..=6), 1..=2)
}

/// The path data of `outlines` (half pixels of an 8 x 8 picture) as shares of it: a half pixel is 1/16, exactly.
fn data(outlines: &[Vec<[i64; 2]>]) -> String {
    outlines
        .iter()
        .map(|outline| {
            let corners: Vec<String> = outline
                .iter()
                .map(|corner| format!("{} {}", float_of(corner[0]) / 16.0, float_of(corner[1]) / 16.0))
                .collect();
            format!("M {} Z", corners.join(" L "))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A small whole number as a float.
fn float_of(number: i64) -> f64 {
    f64::from(i32::try_from(number).unwrap_or(0))
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn the_coverage_is_that_of_a_dense_point_by_point_winding_number_test_whatever_the_outlines(
        drawn in outlines(),
        samples in 1_usize..=3,
    ) {
        let made = covered(8, 8, &data(&drawn), u32::try_from(samples)?)?;
        let expected = dense(&drawn, 8, samples);
        // 256 points a scanline: a border inside a pixel is placed to within 1/256 of it, and a pixel has a few
        prop_assert_eq!(close(&made, &expected, 0.03), Ok(()));
    }
}

//! `kaleidoscope`: the picture compared with a point-by-point computation from the definition (every reading and edge
//! rule), hand-worked mirrors on a pixel lattice, the symmetries a fold into sectors has, and the properties of the fold.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "small picture sizes and offsets from a middle pixel; the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::kaleidoscope::Params;
use bullterrier_image::{Edge, Error, Interpolation};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;
use crate::common::sampling::warped;

/// The pictures are compared at this distance: the reference reads in f64 and the core stores f32.
const TOLERANCE: f32 = 1e-5;

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
/// The readings the comparison with the definition runs on: a 'nearest' reading flips on a tie that two ways of working
/// out the same place (a rotation, polar coordinates) tell apart by a rounding error, so it is checked on the lattice.
const SMOOTH: [Interpolation; 2] = [Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
/// The shapes the comparison runs on: a pixel, a row, a column, a few pixels, odd and even sides.
const SHAPES: [(usize, usize); 6] = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31), (16, 10)];

/// `picture` folded as `params` says, worked out from the definition: the centre is at fraction (`x`, `y`) of the way from
/// the first to the last pixel; a pixel at distance `d` and angle `a` (clockwise from pointing right) shows the picture at
/// distance `d` and the angle `a` folds to. With `start` the angle in radians and `sector` a turn over `segments`, the
/// fold takes `a - start` into the sector; the first half of it is kept and the second half runs back, to `sector` less.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let centre = (
        f64::from(params.y) * (height - 1) as f64,
        f64::from(params.x) * (width - 1) as f64,
    );
    let sector = std::f64::consts::TAU / f64::from(params.segments);
    let start = f64::from(params.angle).to_radians();
    warped(
        picture,
        height,
        width,
        params.interpolation,
        (params.edge, params.edge),
        |row, column| {
            let (down, across) = (row as f64 - centre.0, column as f64 - centre.1);
            let angle = down.atan2(across);
            let mut within = (angle - start) % sector;
            if within < 0.0 {
                within += sector;
            }
            let shown = start
                + if within <= sector / 2.0 {
                    within
                } else {
                    sector - within
                };
            let distance = down.hypot(across);
            (centre.0 + distance * shown.sin(), centre.1 + distance * shown.cos())
        },
    )
}

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // (segments, angle, x, y): one mirror and many wedges, axes at several angles, the centre in the middle, at a corner
    // and off the pixel grid
    let sets = [
        (6, 0.0, 0.5, 0.5),
        (1, 90.0, 0.5, 0.5),
        (2, 33.3, 0.3, 0.8),
        (7, 200.0, 0.0, 1.0),
        (64, 360.0, 1.0, 0.4),
        (3, 45.0, 0.37, 0.5),
    ];
    for interpolation in SMOOTH {
        for edge in EDGES {
            for (segments, angle, x, y) in sets {
                let params = Params {
                    segments,
                    angle,
                    x,
                    y,
                    interpolation,
                    edge,
                };
                for (height, width) in SHAPES {
                    let picture = varied(height, width);
                    let at = first_difference_within(
                        &run_picture(&params, height, width, &picture)?,
                        &by_definition(&params, height, width, &picture),
                        TOLERANCE,
                    );
                    assert_eq!(at, None, "{params:?}, {height} x {width}: first differs at {at:?}");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn the_comparison_tells_folds_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another number of segments, angle, centre, reading or edge rule is not this fold; and the fold
    // is not nothing, so the differences show
    let picture = varied(23, 31);
    let params = Params {
        segments: 5,
        angle: 20.0,
        x: 0.3,
        y: 0.6,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Wrap,
    };
    let ours = run_picture(&params, 23, 31, &picture)?;
    assert!(
        first_difference_within(&ours, &picture, 0.05).is_some(),
        "the fold changes the picture"
    );
    for other in [
        Params {
            segments: 6,
            ..params.clone()
        },
        Params {
            angle: 30.0,
            ..params.clone()
        },
        Params {
            x: 0.4,
            ..params.clone()
        },
        Params {
            y: 0.5,
            ..params.clone()
        },
        Params {
            interpolation: Interpolation::Nearest,
            ..params.clone()
        },
        Params {
            edge: Edge::Mirror,
            ..params.clone()
        },
    ] {
        assert!(
            first_difference_within(&ours, &by_definition(&other, 23, 31, &picture), TOLERANCE).is_some(),
            "{other:?}"
        );
    }
    Ok(())
}

/// `out` of a `size` x `size` picture at the offset (`down`, `across`) from its middle pixel.
fn at_offset(pixels: &[f32], size: usize, down: i64, across: i64) -> &[f32] {
    let middle = (size / 2) as i64;
    let at = (((middle + down) as usize) * size + (middle + across) as usize) * 3;
    &pixels[at..at + 3]
}

#[test]
fn one_segment_keeps_the_half_clockwise_from_the_angle_and_mirrors_it_over_the_other() -> Result<(), Error> {
    // by hand, on a 21 x 21 picture with the centre on the middle pixel and nearest reading: the half clockwise from
    // pointing right by `angle` is kept; the other half shows the kept half reflected in the line through the centre at
    // that angle. (angle, a pixel's offset (down, across) in the kept half, the offset that shows it in the other half)
    let size = 21;
    let picture = varied(size, size);
    // 0 keeps the bottom half, 90 the left, 180 the top and 270 the right, the reflection swapping the row or column sign
    let rows = [
        (0.0, (4, -7), (-4, -7)),
        (90.0, (-3, -8), (-3, 8)),
        (180.0, (-6, 2), (6, 2)),
        (270.0, (5, 9), (5, -9)),
    ];
    for (angle, kept, other) in rows {
        let params = Params {
            segments: 1,
            angle,
            interpolation: Interpolation::Nearest,
            ..Params::default()
        };
        let out = run_picture(&params, size, size, &picture)?;
        assert_eq!(
            at_offset(&out, size, kept.0, kept.1),
            at_offset(&picture, size, kept.0, kept.1),
            "angle {angle}: kept"
        );
        assert_eq!(
            at_offset(&out, size, other.0, other.1),
            at_offset(&picture, size, kept.0, kept.1),
            "angle {angle}: mirrored"
        );
    }
    Ok(())
}

#[test]
fn two_segments_copy_the_kept_quarter_into_the_other_three() -> Result<(), Error> {
    // by hand: angle 0, the quarter below and right of the centre (angle 0 to 90 degrees) is kept; the others reflect it
    let size = 21;
    let picture = varied(size, size);
    let params = Params {
        segments: 2,
        angle: 0.0,
        interpolation: Interpolation::Nearest,
        ..Params::default()
    };
    let out = run_picture(&params, size, size, &picture)?;
    for down in -10_i64..=10 {
        for across in -10_i64..=10 {
            assert_eq!(
                at_offset(&out, size, down, across),
                at_offset(&picture, size, down.abs(), across.abs()),
                "offset ({down}, {across})"
            );
        }
    }
    Ok(())
}

#[test]
fn four_segments_look_the_same_turned_by_a_quarter_and_across_each_axis() -> Result<(), Error> {
    // the pattern of n segments repeats every 360 / n degrees and each axis is a mirror: with 4 (angle 0) a square picture's
    // result is the same turned a quarter, flipped, or across either diagonal; with 2 only the halves turns
    let size = 21;
    let picture = varied(size, size);
    let fold = |segments| -> Result<Vec<f32>, Error> {
        run_picture(
            &Params {
                segments,
                angle: 0.0,
                interpolation: Interpolation::Nearest,
                ..Params::default()
            },
            size,
            size,
            &picture,
        )
    };
    let (two, four) = (fold(2)?, fold(4)?);
    let moved = |out: &[f32], map: &dyn Fn(i64, i64) -> (i64, i64)| -> bool {
        (-10..=10).all(|down| {
            (-10..=10).all(|across| {
                let (to_down, to_across) = map(down, across);
                at_offset(out, size, down, across) == at_offset(out, size, to_down, to_across)
            })
        })
    };
    let quarter: &dyn Fn(i64, i64) -> (i64, i64) = &|down, across| (across, -down);
    let transpose: &dyn Fn(i64, i64) -> (i64, i64) = &|down, across| (across, down);
    let flip: &dyn Fn(i64, i64) -> (i64, i64) = &|down, across| (-down, across);
    for (name, map) in [
        ("a quarter turn", quarter),
        ("the diagonal", transpose),
        ("the flip", flip),
    ] {
        assert!(moved(&four, map), "four segments, {name}");
    }
    assert!(moved(&two, flip), "two segments, the flip");
    assert!(!moved(&two, quarter), "two segments are not alike turned by a quarter");
    assert!(
        !moved(&two, transpose),
        "two segments are not alike across the diagonal"
    );
    Ok(())
}

#[test]
fn a_picture_that_depends_only_on_the_distance_from_the_centre_is_left_as_it_is() -> Result<(), Error> {
    // the fold keeps the distance from the centre, so rings stay rings (beyond reading between pixels) inside the disc the
    // picture holds; a ramp does not
    let size = 41;
    let rings: Vec<f32> = (0..size * size)
        .flat_map(|at| {
            let d = ((at / size) as f64 - 20.0).hypot((at % size) as f64 - 20.0);
            [(d / 30.0) as f32; 3]
        })
        .collect();
    let params = Params {
        segments: 5,
        angle: 17.0,
        interpolation: Interpolation::Bicubic,
        ..Params::default()
    };
    let out = run_picture(&params, size, size, &rings)?;
    for (at, (found, expected)) in out.chunks(3).zip(rings.chunks(3)).enumerate() {
        let d = ((at / size) as f64 - 20.0).hypot((at % size) as f64 - 20.0);
        if d <= 18.0 {
            assert!(
                (found[0] - expected[0]).abs() < 0.02,
                "pixel {at}: {} for {}",
                found[0],
                expected[0]
            );
        }
    }
    let slope = crate::common::pictures::ramp(size, size);
    assert!(
        first_difference_within(&run_picture(&params, size, size, &slope)?, &slope, 0.1).is_some(),
        "a ramp is folded"
    );
    Ok(())
}

#[test]
fn a_flat_picture_stays_as_flat_and_the_centre_keeps_its_pixel() -> Result<(), Error> {
    let flat = vec![0.3_f32; 5 * 7 * 3];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                segments: 3,
                angle: 100.0,
                x: 0.2,
                y: 0.9,
                interpolation,
                edge,
            };
            let at = first_difference_within(&run_picture(&params, 5, 7, &flat)?, &flat, TOLERANCE);
            assert_eq!(at, None, "{params:?}: first differs at {at:?}");
        }
    }
    // a centre on a pixel keeps that pixel whatever the fold
    let picture = varied(9, 11);
    for segments in [1, 3, 64] {
        let params = Params {
            segments,
            angle: 77.0,
            x: 0.5,
            y: 0.5,
            ..Params::default()
        };
        let out = run_picture(&params, 9, 11, &picture)?;
        let middle = (4 * 11 + 5) * 3;
        assert_eq!(
            out[middle..middle + 3],
            picture[middle..middle + 3],
            "{segments} segments"
        );
    }
    Ok(())
}

/// Parameters over their whole ranges, the ends included.
fn parameters() -> impl Strategy<Value = Params> {
    let fraction = prop_oneof![Just(0.0_f32), Just(1.0_f32), 0.0_f32..=1.0];
    (
        prop_oneof![Just(1), Just(64), 1..=64],
        prop_oneof![Just(0.0_f32), Just(360.0_f32), 0.0_f32..=360.0],
        fraction.clone(),
        fraction,
        0..SMOOTH.len(),
        0..EDGES.len(),
    )
        .prop_map(|(segments, angle, x, y, interpolation, edge)| Params {
            segments,
            angle,
            x,
            y,
            interpolation: SMOOTH[interpolation],
            edge: EDGES[edge],
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn at_every_end_of_every_range_the_picture_is_the_definitions_and_stays_in_range(
        params in parameters(),
        height in 1_usize..6,
        width in 1_usize..9,
        picture in prop::collection::vec(0.0_f32..=1.0, 5 * 8 * 3),
    ) {
        let picture = &picture[..height * width * 3];
        let out = run_picture(&params, height, width, picture)
            .map_err(|error: Error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(out.iter().all(|value| (0.0..=1.0).contains(value)), "within 0..1");
        let at = first_difference_within(&out, &by_definition(&params, height, width, picture), TOLERANCE);
        prop_assert_eq!(at, None, "{:?}: first differs at {:?}", params, at);
    }
}

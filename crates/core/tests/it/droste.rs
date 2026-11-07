//! `droste`: the picture compared with a point-by-point computation from the definition (every reading and edge rule), a
//! hand-worked halving, the self-similarity of the copies and the properties of what lies outside the first copy.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "small picture sizes and offsets from a middle pixel; the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::droste::Params;
use bullterrier_image::{Edge, Error, Interpolation};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;
use crate::common::sampling::warped;

/// The pictures are compared at this distance: the reference reads in f64 and the core stores f32.
const TOLERANCE: f32 = 1e-5;

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
/// The shapes the comparison runs on: a pixel, a row, a column, a few pixels, odd and even sides.
const SHAPES: [(usize, usize); 6] = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31), (16, 10)];

/// `picture` with its copies, worked out from the definition: a pixel's size is its distance from the middle over the
/// half-height or half-width, whichever is larger; the copies are the boxes of size `scale`, `scale`^2 and so on about the
/// middle, up to `repeats` of them, and a pixel in `n` of them shows the picture at its offset from the middle divided by
/// `scale`^n and turned by `n x turn` degrees, clockwise as the picture appears.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let (half_height, half_width) = (((height - 1).max(1) as f64) / 2.0, ((width - 1).max(1) as f64) / 2.0);
    let middle = ((height - 1) as f64 / 2.0, (width - 1) as f64 / 2.0);
    let (scale, turn) = (f64::from(params.scale), f64::from(params.turn).to_radians());
    warped(
        picture,
        height,
        width,
        params.interpolation,
        (params.edge, params.edge),
        |row, column| {
            let (down, across) = (row as f64 - middle.0, column as f64 - middle.1);
            let mut size = (down.abs() / half_height).max(across.abs() / half_width);
            let mut copies = 0;
            while size < scale && size > 0.0 && copies < params.repeats {
                size /= scale;
                copies += 1;
            }
            let (stretch, turned) = (scale.powi(-copies), turn * f64::from(copies));
            let (down, across) = (down * stretch, across * stretch);
            (
                middle.0 + down * turned.cos() - across * turned.sin(),
                middle.1 + down * turned.sin() + across * turned.cos(),
            )
        },
    )
}

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // (scale, turn, repeats): shallow and deep, both senses, a slow shrink, a quarter turn
    let sets = [
        (0.5, 20.0, 4),
        (0.9, -75.0, 12),
        (0.1, 180.0, 1),
        (0.7, 0.0, 6),
        (0.62, 90.0, 3),
    ];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for (scale, turn, repeats) in sets {
                let params = Params {
                    scale,
                    turn,
                    repeats,
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
fn the_comparison_tells_copies_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another scale, turn, depth, reading or edge rule is not this picture; and the copies are not
    // nothing, so the differences show
    let picture = varied(23, 31);
    let params = Params {
        scale: 0.6,
        turn: 35.0,
        repeats: 3,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Wrap,
    };
    let ours = run_picture(&params, 23, 31, &picture)?;
    assert!(
        first_difference_within(&ours, &picture, 0.05).is_some(),
        "the copies change the picture"
    );
    for other in [
        Params {
            scale: 0.55,
            ..params.clone()
        },
        Params {
            turn: -35.0,
            ..params.clone()
        },
        Params {
            repeats: 2,
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

#[test]
fn a_copy_half_the_size_shows_every_second_pixel_of_the_picture_and_the_outside_is_untouched() -> Result<(), Error> {
    // by hand: 41 x 41, the middle pixel (20, 20), half-width 20 px. Scale 0.5, no turn, one copy, nearest: a pixel d px
    // from the middle, d under 10 either way, shows the pixel 2 d from it; one 10 or more from it is as it was.
    let size = 41;
    let picture = varied(size, size);
    let params = Params {
        scale: 0.5,
        turn: 0.0,
        repeats: 1,
        interpolation: Interpolation::Nearest,
        ..Params::default()
    };
    let out = run_picture(&params, size, size, &picture)?;
    for row in 0..size {
        for column in 0..size {
            let (down, across) = (row as i64 - 20, column as i64 - 20);
            let (from_row, from_column) = if down.abs().max(across.abs()) < 10 {
                (20 + 2 * down, 20 + 2 * across)
            } else {
                (row as i64, column as i64)
            };
            let (at, from) = (
                (row * size + column) * 3,
                (from_row as usize * size + from_column as usize) * 3,
            );
            assert_eq!(out[at..at + 3], picture[from..from + 3], "({row}, {column})");
        }
    }
    Ok(())
}

#[test]
fn a_half_turn_per_copy_shows_the_picture_upside_down_in_the_copy() -> Result<(), Error> {
    // by hand: as above with a turn of 180 degrees, a pixel d px from the middle shows the pixel -2 d from it
    let size = 41;
    let picture = varied(size, size);
    let params = Params {
        scale: 0.5,
        turn: 180.0,
        repeats: 1,
        interpolation: Interpolation::Nearest,
        ..Params::default()
    };
    let out = run_picture(&params, size, size, &picture)?;
    for (down, across) in [(1_i64, 0_i64), (-3, 4), (9, -9), (0, 7), (-9, 0)] {
        let at = (((20 + down) as usize) * size + (20 + across) as usize) * 3;
        let from = (((20 - 2 * down) as usize) * size + (20 - 2 * across) as usize) * 3;
        assert_eq!(out[at..at + 3], picture[from..from + 3], "offset ({down}, {across})");
    }
    Ok(())
}

#[test]
fn each_copy_is_the_picture_scaled_by_the_ratio_to_the_one_around_it() -> Result<(), Error> {
    // self-similarity, with no turn and nearest: the picture at offset e from the middle is the picture at offset 2 e,
    // for every e inside the picture, as long as the copies do not run out (12 repeats, 41 px: 2 e is at most 5 copies deep)
    let size = 41;
    let picture = varied(size, size);
    let params = Params {
        scale: 0.5,
        turn: 0.0,
        repeats: 12,
        interpolation: Interpolation::Nearest,
        ..Params::default()
    };
    let out = run_picture(&params, size, size, &picture)?;
    for down in -9_i64..=9 {
        for across in -9_i64..=9 {
            let at = (((20 + down) as usize) * size + (20 + across) as usize) * 3;
            let twice = (((20 + 2 * down) as usize) * size + (20 + 2 * across) as usize) * 3;
            assert_eq!(out[at..at + 3], out[twice..twice + 3], "offset ({down}, {across})");
        }
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_as_flat_and_more_repeats_only_change_the_middle() -> Result<(), Error> {
    let flat = vec![0.3_f32; 5 * 7 * 3];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                scale: 0.8,
                turn: 130.0,
                repeats: 12,
                interpolation,
                edge,
            };
            let at = first_difference_within(&run_picture(&params, 5, 7, &flat)?, &flat, TOLERANCE);
            assert_eq!(at, None, "{params:?}: first differs at {at:?}");
        }
    }
    // one more copy lies inside the last: pixels farther out than the last copy's reach are unchanged
    let size = 41;
    let picture = varied(size, size);
    let (few, many) = (
        Params {
            scale: 0.5,
            turn: 0.0,
            repeats: 2,
            interpolation: Interpolation::Nearest,
            ..Params::default()
        },
        Params {
            scale: 0.5,
            turn: 0.0,
            repeats: 3,
            interpolation: Interpolation::Nearest,
            ..Params::default()
        },
    );
    let (few, many) = (
        run_picture(&few, size, size, &picture)?,
        run_picture(&many, size, size, &picture)?,
    );
    for row in 0..size {
        for column in 0..size {
            let (down, across) = (row as i64 - 20, column as i64 - 20);
            let at = (row * size + column) * 3;
            // the third copy is 0.5^3 = 1/8 of the picture: under 2.5 px from the middle
            if down.abs().max(across.abs()) >= 3 {
                assert_eq!(few[at..at + 3], many[at..at + 3], "({row}, {column})");
            }
        }
    }
    assert_ne!(few, many, "the third copy shows");
    Ok(())
}

/// Parameters over their whole ranges, the ends included.
fn parameters() -> impl Strategy<Value = Params> {
    (
        prop_oneof![Just(0.1_f32), Just(0.9_f32), 0.1_f32..=0.9],
        prop_oneof![Just(-180.0_f32), Just(180.0_f32), Just(0.0_f32), -180.0_f32..=180.0],
        1..=12,
        0..INTERPOLATIONS.len(),
        0..EDGES.len(),
    )
        .prop_map(|(scale, turn, repeats, interpolation, edge)| Params {
            scale,
            turn,
            repeats,
            interpolation: INTERPOLATIONS[interpolation],
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

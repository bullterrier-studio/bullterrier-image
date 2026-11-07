//! `perspective`: hand-worked readings of each step of the transform (keystone, skews, turn, zoom) on a pixel lattice and
//! on a picture a bilinear reading follows exactly, the transforms undone by their inverses, and the ranges.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "small picture sizes and offsets from a middle pixel"
)]

use bullterrier_image::effects::perspective::Params;
use bullterrier_image::{Edge, Error, Interpolation};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;

const SIZE: usize = 21;
const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

/// Every step at nothing (the default keystone is not), reading the closest pixel.
fn none() -> Params {
    Params {
        vertical: 0.0,
        interpolation: Interpolation::Nearest,
        ..Params::default()
    }
}

/// A `size` x `size` picture whose channels are the row over `size - 1`, the column over it and a half: linear in the row
/// and the column, so a bilinear reading of it is exact wherever it stays inside the picture.
fn grid(size: usize) -> Vec<f32> {
    (0..size * size)
        .flat_map(|at| {
            [
                (at / size) as f32 / (size - 1) as f32,
                (at % size) as f32 / (size - 1) as f32,
                0.5,
            ]
        })
        .collect()
}

/// The pixel at the offset (`down`, `across`) from the middle pixel of a `size` x `size` picture.
fn at_offset(pixels: &[f32], size: usize, down: i64, across: i64) -> &[f32] {
    let middle = (size / 2) as i64;
    let at = (((middle + down) as usize) * size + (middle + across) as usize) * 3;
    &pixels[at..at + 3]
}

/// Fails unless the pixel of `out` at `to` is the pixel of `picture` at `from`, both offsets (down, across) from the middle.
fn assert_shows(params: &Params, to: (i64, i64), from: (i64, i64)) -> Result<(), Error> {
    let picture = varied(SIZE, SIZE);
    let out = run_picture(params, SIZE, SIZE, &picture)?;
    assert_eq!(
        at_offset(&out, SIZE, to.0, to.1),
        at_offset(&picture, SIZE, from.0, from.1),
        "{params:?}: {to:?} shows {from:?}"
    );
    Ok(())
}

#[test]
fn every_step_at_nothing_leaves_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(9, 14);
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                interpolation,
                edge,
                ..none()
            };
            let at = first_difference_within(&run_picture(&params, 9, 14, &picture)?, &picture, 1e-6);
            assert_eq!(at, None, "{params:?}: first differs at {at:?}");
        }
    }
    Ok(())
}

#[test]
fn each_step_moves_the_lattice_where_its_definition_says() -> Result<(), Error> {
    // by hand, 21 x 21, offsets (down, across) from the middle pixel, nearest reading, one step at a time. Pixel `to` of
    // the result shows the pixel `from` of the picture.
    let rows = [
        // a quarter turn clockwise carries what is above the middle to the right of it
        (Params { rotate: 90.0, ..none() }, (0, 5), (-5, 0)),
        (
            Params {
                rotate: -90.0,
                ..none()
            },
            (0, 5),
            (5, 0),
        ),
        (
            Params {
                rotate: 180.0,
                ..none()
            },
            (3, -7),
            (-3, 7),
        ),
        // twice as big: a point twice as far out shows the one half as far
        (Params { scale: 1.0, ..none() }, (4, 6), (2, 3)),
        (Params { scale: -0.5, ..none() }, (2, 3), (4, 6)),
        // rows slide sideways by 1/2 px per px below the middle (and back above it): the source of a pixel is that much to its left
        (Params { shear_x: 0.5, ..none() }, (4, 5), (4, 3)),
        (
            Params {
                shear_x: -0.5,
                ..none()
            },
            (-6, 1),
            (-6, -2),
        ),
        // columns slide down by 1/2 px per px right of the middle
        (Params { shear_y: 0.5, ..none() }, (6, 4), (4, 4)),
        (
            Params {
                shear_y: -0.5,
                ..none()
            },
            (-2, -8),
            (-6, -8),
        ),
        // both, `shear_x` first: source (down 4, across 2) goes to across 2 + 4/2 = 4, then down 4 + 4/2 = 6
        (
            Params {
                shear_x: 0.5,
                shear_y: 0.5,
                ..none()
            },
            (6, 4),
            (4, 2),
        ),
    ];
    for (params, to, from) in rows {
        assert_shows(&params, to, from)?;
    }
    Ok(())
}

#[test]
fn a_vertical_keystone_shrinks_the_far_edge_and_keeps_the_near_edge_in_place() -> Result<(), Error> {
    // by hand: vertical 0.5 makes the top the far side. In the picture's units (half-height 10.5 px at the pixels' edges)
    // the keystone is x' = a x / w, y' = a y / w with a = 2/3 and w = 1 - y / 31.5, so the pixel 4 px below the middle
    // shows the row y with 4 = (2/3) y / (1 - y / 31.5), y = 4 / (2/3 + 4 / 31.5) = 5.04, and the column x with 3 =
    // (2/3) x / (1 - 5.04 / 31.5), x = 3 x 0.84 / (2/3) = 3.78: on a picture linear in row and column, that reading is exact.
    let picture = grid(SIZE);
    let params = Params {
        vertical: 0.5,
        interpolation: Interpolation::Bilinear,
        ..Params::default()
    };
    let out = run_picture(&params, SIZE, SIZE, &picture)?;
    let found = at_offset(&out, SIZE, 4, 3);
    assert!((found[0] - (10.0 + 5.04) / 20.0).abs() < 1e-5, "row {}", found[0]);
    assert!((found[1] - (10.0 + 3.78) / 20.0).abs() < 1e-5, "column {}", found[1]);
    // a negative keystone is the same upside down
    let flipped = run_picture(
        &Params {
            vertical: -0.5,
            ..params.clone()
        },
        SIZE,
        SIZE,
        &picture,
    )?;
    let found = at_offset(&flipped, SIZE, -4, 3);
    assert!((found[0] - (10.0 - 5.04) / 20.0).abs() < 1e-5, "row {}", found[0]);
    assert!((found[1] - (10.0 + 3.78) / 20.0).abs() < 1e-5, "column {}", found[1]);
    Ok(())
}

#[test]
fn a_horizontal_keystone_is_the_vertical_one_on_its_side() -> Result<(), Error> {
    // by hand, as above with rows and columns swapped: horizontal 0.5 makes the right side the far one and the left the
    // near one, so the pixel 4 px left of the middle shows the column -5.04 and, 3 px below the middle, the row 3.78
    let picture = grid(SIZE);
    let params = Params {
        vertical: 0.0,
        horizontal: 0.5,
        interpolation: Interpolation::Bilinear,
        ..Params::default()
    };
    let out = run_picture(&params, SIZE, SIZE, &picture)?;
    let found = at_offset(&out, SIZE, 3, -4);
    assert!((found[1] - (10.0 - 5.04) / 20.0).abs() < 1e-5, "column {}", found[1]);
    assert!((found[0] - (10.0 + 3.78) / 20.0).abs() < 1e-5, "row {}", found[0]);
    Ok(())
}

#[test]
fn a_turn_of_a_whole_circle_is_no_turn() -> Result<(), Error> {
    // (a nearest reading tells two ways of working out one place apart on a tie, so the smooth readings)
    let picture = varied(23, 31);
    for interpolation in [Interpolation::Bilinear, Interpolation::Bicubic] {
        let (turned, plain) = (
            Params {
                rotate: 360.0,
                interpolation,
                ..Params::default()
            },
            Params {
                rotate: 0.0,
                interpolation,
                ..Params::default()
            },
        );
        let at = first_difference_within(
            &run_picture(&turned, 23, 31, &picture)?,
            &run_picture(&plain, 23, 31, &picture)?,
            1e-5,
        );
        assert_eq!(at, None, "{interpolation:?}: first differs at {at:?}");
    }
    Ok(())
}

#[test]
fn a_skew_or_a_turn_undone_by_its_inverse_gives_the_picture_back_inside() -> Result<(), Error> {
    // the maps are inverses, so reading a picture linear in row and column twice returns it wherever both readings stay
    // inside the picture: the middle 17 x 17 of a 41 x 41, for a skew either way and a turn of 30 degrees
    let size = 41;
    let picture = grid(size);
    let pairs = [
        (
            Params { shear_x: 0.5, ..none() },
            Params {
                shear_x: -0.5,
                ..none()
            },
        ),
        (
            Params {
                shear_y: -1.2,
                ..none()
            },
            Params { shear_y: 1.2, ..none() },
        ),
        (
            Params { rotate: 30.0, ..none() },
            Params {
                rotate: -30.0,
                ..none()
            },
        ),
    ];
    for (there, back) in pairs {
        let (there, back) = (
            Params {
                interpolation: Interpolation::Bilinear,
                ..there
            },
            Params {
                interpolation: Interpolation::Bilinear,
                ..back
            },
        );
        let round = run_picture(&back, size, size, &run_picture(&there, size, size, &picture)?)?;
        for down in -8_i64..=8 {
            for across in -8_i64..=8 {
                let (got, expected) = (
                    at_offset(&round, size, down, across),
                    at_offset(&picture, size, down, across),
                );
                let gap = got.iter().zip(expected).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
                assert!(gap < 1e-5, "{there:?}: offset ({down}, {across}) is {gap} away");
            }
        }
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_as_flat_whatever_the_transform() -> Result<(), Error> {
    let flat = vec![0.3_f32; 5 * 7 * 3];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                vertical: -0.9,
                horizontal: 0.9,
                shear_x: 2.0,
                shear_y: -2.0,
                rotate: 133.0,
                scale: 4.0,
                interpolation,
                edge,
            };
            let at = first_difference_within(&run_picture(&params, 5, 7, &flat)?, &flat, 1e-5);
            assert_eq!(at, None, "{params:?}: first differs at {at:?}");
        }
    }
    Ok(())
}

#[test]
fn a_pixel_on_the_vanishing_line_still_shows_a_colour() -> Result<(), Error> {
    // found by scanning keystones, zooms and turns over small pictures: the line where the depth is nothing runs through a
    // pixel, which then reads a place at infinity (and was NaN). (vertical, horizontal, scale, rotate, height, width)
    let rows = [
        (-0.9, -0.75, 0.0, 0.0, 5, 9),
        (-0.9, -0.5, -0.5, 90.0, 10, 3),
        (0.9, 0.9, 0.0, 0.0, 5, 9),
    ];
    for (vertical, horizontal, scale, rotate, height, width) in rows {
        let picture = varied(height, width);
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                let params = Params {
                    vertical,
                    horizontal,
                    scale,
                    rotate,
                    interpolation,
                    edge,
                    ..none()
                };
                let out = run_picture(&params, height, width, &picture)?;
                assert!(
                    out.iter().all(|value| (0.0..=1.0).contains(value)),
                    "{params:?}, {height} x {width}"
                );
            }
        }
    }
    Ok(())
}

/// Parameters over their whole ranges, the ends included.
fn parameters() -> impl Strategy<Value = Params> {
    let keystone = prop_oneof![Just(-0.9_f32), Just(0.9_f32), Just(0.0_f32), -0.9_f32..=0.9];
    let skew = prop_oneof![Just(-2.0_f32), Just(2.0_f32), Just(0.0_f32), -2.0_f32..=2.0];
    (
        (keystone.clone(), keystone),
        (skew.clone(), skew),
        prop_oneof![Just(-180.0_f32), Just(180.0_f32), Just(0.0_f32), -180.0_f32..=180.0],
        prop_oneof![Just(-0.9_f32), Just(4.0_f32), Just(0.0_f32), -0.9_f32..=4.0],
        (0..INTERPOLATIONS.len(), 0..EDGES.len()),
    )
        .prop_map(
            |((vertical, horizontal), (shear_x, shear_y), rotate, scale, (interpolation, edge))| Params {
                vertical,
                horizontal,
                shear_x,
                shear_y,
                rotate,
                scale,
                interpolation: INTERPOLATIONS[interpolation],
                edge: EDGES[edge],
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn at_every_end_of_every_range_the_picture_is_finite_and_in_range(
        params in parameters(),
        height in 1_usize..9,
        width in 1_usize..9,
        picture in prop::collection::vec(0.0_f32..=1.0, 8 * 8 * 3),
    ) {
        let picture = &picture[..height * width * 3];
        let out = run_picture(&params, height, width, picture)
            .map_err(|error: Error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(out.iter().all(|value| (0.0..=1.0).contains(value)), "within 0..1");
    }
}

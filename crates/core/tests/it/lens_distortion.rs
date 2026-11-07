//! `lens_distortion`: hand-worked readings of the Brown-Conrady and fisheye curves on a ramp, the frame that keeps the
//! farthest corner, the border where the bend leaves the picture, the sign of the bend, and the picture compared with a
//! point-by-point computation from the definition.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "small picture sizes; the f64 reference narrowed to the f32 picture"
)]

use std::f64::consts::PI;

use bullterrier_image::effects::lens_distortion::{Fit, Model, Params};
use bullterrier_image::params::Color;
use bullterrier_image::{Edge, Error, Interpolation};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::pictures::{ramp, varied};
use crate::common::pixels::run_picture;
use crate::common::sampling::warped;

/// The pictures are compared at this distance: the reference reads in f64 and the core stores f32.
const TOLERANCE: f32 = 1e-5;

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
/// The shapes the comparison runs on: a pixel, a row, a column, a few pixels, odd and even sides.
const SHAPES: [(usize, usize); 6] = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31), (16, 10)];

/// A row of 21 pixels, each channel the pixel's column over 20: a picture a bilinear reading follows exactly. Its lens
/// axis (`x` 0.5) is on column 10 and the farthest corner, the radius 1, 10 px from it.
fn row() -> Vec<f32> {
    ramp(1, 21)
}

/// The first channel of `params` applied to the row, at column `column`.
fn at_column(params: &Params, column: usize) -> Result<f32, Error> {
    Ok(run_picture(params, 1, 21, &row())?[column * 3])
}

/// Every parameter but those a test sets: a bilinear reading, which a ramp keeps exact.
fn lens(model: Model, amount: f32, fit: Fit) -> Params {
    Params {
        model,
        amount,
        fit,
        interpolation: Interpolation::Bilinear,
        ..Params::default()
    }
}

#[test]
fn the_brown_conrady_curve_reads_a_pixel_at_its_distance_times_one_plus_amount_times_the_radius_squared()
-> Result<(), Error> {
    // by hand, fit 'centre' (the axis keeps its scale): the pixel 4 px right of the axis is at radius 4 / 10, so it reads
    // 1 + 0.2 x 0.16 = 1.032 times as far out: column 10 + 4.128, which the ramp gives as 14.128 / 20
    let found = at_column(&lens(Model::BrownConrady, 0.2, Fit::Centre), 14)?;
    assert!((f64::from(found) - 14.128 / 20.0).abs() < 1e-6, "{found}");
    // 'frame' divides the stretch by 1 + amount: 4 x 1.032 / 1.2 = 3.44 from the axis
    let found = at_column(&lens(Model::BrownConrady, 0.2, Fit::Frame), 14)?;
    assert!((f64::from(found) - 13.44 / 20.0).abs() < 1e-6, "{found}");
    Ok(())
}

#[test]
fn the_fisheye_curve_reads_a_pixel_at_the_tangent_of_its_angle_over_the_angle() -> Result<(), Error> {
    // by hand, fit 'centre'. With amount 4 / pi - 1 the corner (radius 1) stretches by tan(u) / u = 4 / pi at u = pi / 4,
    // as tan(pi / 4) = 1. The pixel at radius 1/2 (5 px from the axis) sees the angle pi / 8 and tan(pi / 8) = sqrt 2 - 1,
    // so it reads 5 x (sqrt 2 - 1) / (pi / 8) px out.
    let amount = 4.0 / PI - 1.0;
    let out = 5.0 * (2.0_f64.sqrt() - 1.0) / (PI / 8.0);
    let found = at_column(&lens(Model::Fisheye, amount as f32, Fit::Centre), 15)?;
    assert!((f64::from(found) - (10.0 + out) / 20.0).abs() < 1e-5, "{found}");
    // a negative amount is the inverse curve: with amount pi / 4 - 1 the corner stretches by atan(u) / u at u = 1, which
    // is pi / 4 as atan(1) = pi / 4; the pixel at radius 1/2 sees 1/2 and atan(1/2) = 0.4636476 (a table value)
    let amount = PI / 4.0 - 1.0;
    let out = 5.0 * 0.463_647_609_000_806_1 / 0.5;
    let found = at_column(&lens(Model::Fisheye, amount as f32, Fit::Centre), 15)?;
    assert!((f64::from(found) - (10.0 + out) / 20.0).abs() < 1e-5, "{found}");
    Ok(())
}

#[test]
fn with_fit_frame_the_farthest_corner_stays_in_its_corner_for_every_curve_and_amount() -> Result<(), Error> {
    // the first and last columns are the farthest from the axis: they read themselves, and the axis reads itself
    let picture = varied(1, 21);
    for model in [Model::BrownConrady, Model::Fisheye] {
        for amount in [-0.3, -0.1, 0.01, 0.4, 1.0, 3.5] {
            let out = run_picture(&lens(model, amount, Fit::Frame), 1, 21, &picture)?;
            for column in [0, 10, 20] {
                let at = column * 3;
                let gap = out[at..at + 3]
                    .iter()
                    .zip(&picture[at..at + 3])
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0, f32::max);
                assert!(gap < 1e-5, "{model:?} {amount}: column {column} moved by {gap}");
            }
        }
    }
    Ok(())
}

#[test]
fn a_barrel_reads_farther_out_and_a_pincushion_nearer_in_than_the_pixel_lies() -> Result<(), Error> {
    // on a ramp rising to the right, a pixel right of the axis that reads farther right is lighter than the input there,
    // one left of it darker; a pincushion the other way; at the axis, no change
    let picture = row();
    for model in [Model::BrownConrady, Model::Fisheye] {
        let barrel = run_picture(&lens(model, 0.3, Fit::Centre), 1, 21, &picture)?;
        let pincushion = run_picture(&lens(model, -0.25, Fit::Centre), 1, 21, &picture)?;
        for column in 1..10 {
            let (left, right) = (10 - column, 10 + column);
            assert!(
                barrel[right * 3] > picture[right * 3],
                "{model:?} barrel, right {column}"
            );
            assert!(barrel[left * 3] < picture[left * 3], "{model:?} barrel, left {column}");
            assert!(
                pincushion[right * 3] < picture[right * 3],
                "{model:?} pincushion, right {column}"
            );
            assert!(
                pincushion[left * 3] > picture[left * 3],
                "{model:?} pincushion, left {column}"
            );
        }
        assert!((barrel[30] - picture[30]).abs() < 1e-6, "{model:?} the axis stays");
    }
    Ok(())
}

#[test]
fn an_amount_of_nothing_leaves_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(6, 5);
    for model in [Model::BrownConrady, Model::Fisheye] {
        for fit in [Fit::Frame, Fit::Centre] {
            let params = Params {
                model,
                amount: 0.0,
                fit,
                border: Some(Color([1.0, 0.0, 1.0])),
                ..Params::default()
            };
            assert_eq!(run_picture(&params, 6, 5, &picture)?, picture, "{model:?} {fit:?}");
        }
    }
    // a one pixel picture has no distance to bend
    let params = Params::default();
    assert_eq!(run_picture(&params, 1, 1, &[0.2, 0.4, 0.6])?, vec![0.2, 0.4, 0.6]);
    Ok(())
}

#[test]
fn the_border_shows_exactly_where_the_bend_leaves_the_picture_and_blends_within_a_pixel_of_it() -> Result<(), Error> {
    // by hand, Brown-Conrady, amount 0.2, fit 'centre', a grey row of 21 with the border magenta. The pixel k px from the
    // axis reads 10 + k (1 + 0.2 k^2 / 100): k = 8 reads 19.024, inside the picture (last column 20); k = 9 reads 20.458,
    // 0.458 px past the last pixel, so the picture counts 1 - 0.458 = 0.542 and the border the rest; k = 10 reads 22,
    // beyond a whole pixel past it, all border. The left side is the same by symmetry.
    let grey = 0.4_f32;
    let magenta = [1.0_f32, 0.0, 1.0];
    let params = Params {
        border: Some(Color(magenta)),
        ..lens(Model::BrownConrady, 0.2, Fit::Centre)
    };
    let out = run_picture(&params, 1, 21, &vec![grey; 21 * 3])?;
    let pixel = |column: usize| &out[column * 3..column * 3 + 3];
    for column in [2, 18] {
        assert!(
            pixel(column).iter().all(|&level| (level - grey).abs() < 1e-6),
            "inside, column {column}"
        );
    }
    for (column, picture) in [(1, 0.541_6), (19, 0.541_6)] {
        for (channel, border) in magenta.iter().enumerate() {
            let expected = picture * grey + (1.0 - picture) * border;
            assert!(
                (pixel(column)[channel] - expected).abs() < 1e-3,
                "blended, column {column} channel {channel}"
            );
        }
    }
    for column in [0, 20] {
        assert_eq!(pixel(column), magenta, "all border, column {column}");
    }
    // the same bend with no border reads the edge rule instead: the last pixel of the grey row, so no magenta anywhere
    let plain = run_picture(&lens(Model::BrownConrady, 0.2, Fit::Centre), 1, 21, &vec![grey; 21 * 3])?;
    assert!(plain.iter().all(|&level| (level - grey).abs() < 1e-6));
    Ok(())
}

#[test]
fn a_barrel_with_fit_frame_reads_only_inside_the_picture_so_the_border_never_shows() -> Result<(), Error> {
    let picture = varied(9, 13);
    for model in [Model::BrownConrady, Model::Fisheye] {
        let bordered = Params {
            border: Some(Color([0.0, 1.0, 0.0])),
            ..lens(model, 0.8, Fit::Frame)
        };
        let unbordered = lens(model, 0.8, Fit::Frame);
        let at = first_difference_within(
            &run_picture(&bordered, 9, 13, &picture)?,
            &run_picture(&unbordered, 9, 13, &picture)?,
            1e-6,
        );
        assert_eq!(at, None, "{model:?}: first differs at {at:?}");
    }
    Ok(())
}

/// `picture` bent as `params` says, worked out from the definition: with the axis at fraction (`x`, `y`) of the way from
/// the first to the last pixel and `farthest` the distance to the farthest corner, a pixel at distance `d` shows the
/// picture on the same ray at `d` times the stretch of its radius `d / farthest`: `1 + amount x radius^2` for the
/// Brown-Conrady curve, `law(u radius) / (u radius)` for the fisheye (`law` the tangent, or the arctangent for a negative
/// amount) with `u` found by bisection, divided by `1 + amount` for the frame.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let (x, y, amount) = (f64::from(params.x), f64::from(params.y), f64::from(params.amount));
    let axis = (y * (height - 1) as f64, x * (width - 1) as f64);
    let farthest = (y.max(1.0 - y) * (height - 1) as f64).hypot(x.max(1.0 - x) * (width - 1) as f64);
    if amount == 0.0 || farthest == 0.0 {
        return picture.to_vec();
    }
    let barrel = amount > 0.0;
    let u = {
        let law = |u: f64| if barrel { u.tan() } else { u.atan() } / u;
        let (mut low, mut high) = (1e-9, if barrel { PI / 2.0 - 1e-12 } else { 2.0 / (1.0 + amount) });
        for _ in 0..200 {
            let middle = f64::midpoint(low, high);
            if (law(middle) < 1.0 + amount) == barrel {
                low = middle;
            } else {
                high = middle;
            }
        }
        f64::midpoint(low, high)
    };
    // where the pixel at (`row`, `column`) reads the picture
    let place = |row: usize, column: usize| {
        let (down, across) = (row as f64 - axis.0, column as f64 - axis.1);
        let radius = down.hypot(across) / farthest;
        let stretch = match params.model {
            Model::BrownConrady => 1.0 + amount * radius * radius,
            Model::Fisheye if radius == 0.0 => 1.0,
            Model::Fisheye => {
                (if barrel {
                    (u * radius).tan()
                } else {
                    (u * radius).atan()
                }) / (u * radius)
            }
        };
        let stretch = if params.fit == Fit::Frame {
            stretch / (1.0 + amount)
        } else {
            stretch
        };
        (axis.0 + down * stretch, axis.1 + across * stretch)
    };
    let mut out = warped(
        picture,
        height,
        width,
        params.interpolation,
        (params.edge, params.edge),
        place,
    );
    let Some(Color(border)) = params.border else {
        return out;
    };
    // the picture counts fully until a pixel past its first and last pixel, and falls to nothing there
    let count = |at: f64, size: usize| (at + 1.0).min(size as f64 - at).clamp(0.0, 1.0);
    for at in 0..height * width {
        let (row, column) = place(at / width, at % width);
        let shown = count(row, height) * count(column, width);
        for (channel, level) in out[at * 3..at * 3 + 3].iter_mut().enumerate() {
            *level = (f64::from(*level) * shown + f64::from(border[channel]) * (1.0 - shown)) as f32;
        }
    }
    out
}

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // (model, amount, fit, x, y, border): both curves, both senses, both fits, the axis in the middle, at a corner and off
    // the pixel grid, with and without a border
    let sets = [
        (Model::BrownConrady, 0.2, Fit::Frame, 0.5, 0.5, None),
        (Model::BrownConrady, -0.3, Fit::Centre, 0.0, 1.0, Some([0.1, 0.9, 0.5])),
        (Model::BrownConrady, 3.5, Fit::Centre, 0.37, 0.81, Some([1.0, 0.0, 0.0])),
        (Model::Fisheye, 0.46, Fit::Frame, 0.5, 0.5, None),
        (Model::Fisheye, -0.3, Fit::Centre, 0.2, 0.9, Some([0.0, 0.0, 0.0])),
        (Model::Fisheye, 3.5, Fit::Centre, 0.7, 0.4, None),
        (Model::Fisheye, 1.0, Fit::Frame, 1.0, 0.0, Some([0.3, 0.3, 1.0])),
    ];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for (model, amount, fit, x, y, border) in sets {
                let params = Params {
                    model,
                    amount,
                    fit,
                    x,
                    y,
                    border: border.map(Color),
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
fn the_comparison_tells_bends_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another curve, amount, fit, axis, border, reading or edge rule is not this bend; and the bend is
    // not nothing, so the differences show
    let picture = varied(23, 31);
    let params = Params {
        model: Model::Fisheye,
        amount: 0.7,
        fit: Fit::Centre,
        x: 0.3,
        y: 0.6,
        border: Some(Color([0.2, 0.4, 0.6])),
        interpolation: Interpolation::Bilinear,
        edge: Edge::Wrap,
    };
    let ours = run_picture(&params, 23, 31, &picture)?;
    assert!(
        first_difference_within(&ours, &picture, 0.05).is_some(),
        "the bend changes the picture"
    );
    for other in [
        Params {
            model: Model::BrownConrady,
            ..params.clone()
        },
        Params {
            amount: 0.6,
            ..params.clone()
        },
        Params {
            fit: Fit::Frame,
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
            border: None,
            ..params.clone()
        },
        Params {
            border: Some(Color([0.2, 0.4, 0.7])),
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

/// Parameters over their whole ranges, the ends included.
fn parameters() -> impl Strategy<Value = Params> {
    let fraction = prop_oneof![Just(0.0_f32), Just(1.0_f32), 0.0_f32..=1.0];
    (
        (0_usize..2, 0_usize..2),
        prop_oneof![Just(-0.3_f32), Just(3.5_f32), Just(0.0_f32), -0.3_f32..=3.5],
        (fraction.clone(), fraction),
        prop::option::of([0.0_f32..=1.0, 0.0..=1.0, 0.0..=1.0]),
        (0..INTERPOLATIONS.len(), 0..EDGES.len()),
    )
        .prop_map(|((model, fit), amount, (x, y), border, (interpolation, edge))| Params {
            model: [Model::BrownConrady, Model::Fisheye][model],
            amount,
            fit: [Fit::Frame, Fit::Centre][fit],
            x,
            y,
            border: border.map(Color),
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

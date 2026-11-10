//! `flow_warp`: with no variation the flow is one straight current, so the picture is what repeated shifts of the definition
//! give (every reading, every edge rule); with variation the places the pixels were read from, recovered through a picture
//! that says where each pixel was read from, show the flow the documentation describes: `amount` px long in one step, its
//! direction `angle` plus noise of mean 0 and deviation `variation`, smooth over `scale` px, decided by the seed.

#![expect(
    clippy::cast_precision_loss,
    reason = "small picture sizes; the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::flow_warp::Params;
use bullterrier_image::{Edge, Error, Interpolation};

use crate::common::compare::{first_bit_difference, first_difference_within};
use crate::common::pictures::{coordinates, places, varied};
use crate::common::pixels::run_picture;
use crate::common::stats::{correlation, deviation, mean};

/// The pictures are compared at this distance: the reference reads in f64 and the core stores f32.
const TOLERANCE: f32 = 1e-5;

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

/// `picture` carried along one straight current by the definition: `steps` times, every pixel shows what was
/// `amount / steps` px behind it in the direction `angle` degrees clockwise from pointing right.
fn straight(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let step = f64::from(params.amount) / f64::from(params.steps);
    let (sin, cos) = f64::from(params.angle).to_radians().sin_cos();
    let mut carried = picture.to_vec();
    for _ in 0..params.steps {
        carried = moved_with(params, height, width, &carried, step * sin, step * cos);
    }
    carried
}

/// The picture shifted down by `dy` and right by `dx` with the reading and the edge rule of `params`.
fn moved_with(params: &Params, height: usize, width: usize, picture: &[f32], dy: f64, dx: f64) -> Vec<f32> {
    crate::common::sampling::warped(
        picture,
        height,
        width,
        params.interpolation,
        (params.edge, params.edge),
        |row, column| (row as f64 - dy, column as f64 - dx),
    )
}

#[test]
fn with_no_variation_the_picture_is_carried_by_repeated_shifts_of_the_definition() -> Result<(), Error> {
    // (amount, steps, angle): one step and several, the four quarters of the angle, a slant, an amount beyond the picture
    let sets = [
        (6.0, 1, 0.0),
        (7.5, 3, 90.0),
        (9.0, 4, 33.0),
        (4.0, 2, 180.0),
        (11.0, 5, 270.0),
        (100.0, 3, 125.0),
        (3.0, 32, 360.0),
    ];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for (amount, steps, angle) in sets {
                let params = Params {
                    amount,
                    steps,
                    angle,
                    variation: 0.0,
                    interpolation,
                    edge,
                    ..Params::default()
                };
                for (height, width) in [(1, 9), (9, 1), (14, 19), (16, 10)] {
                    let picture = varied(height, width);
                    let at = first_difference_within(
                        &run_picture(&params, height, width, &picture)?,
                        &straight(&params, height, width, &picture),
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
fn no_amount_leaves_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(9, 11);
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                amount: 0.0,
                interpolation,
                edge,
                ..Params::default()
            };
            let out = run_picture(&params, 9, 11, &picture)?;
            assert_eq!(first_bit_difference(&out, &picture), None, "{params:?}");
        }
    }
    Ok(())
}

/// How far and which way `params` carries each pixel of a `size` x `size` picture in one step, recovered from where the
/// pixels were read: a picture that says its own coordinates is read linearly, so what a pixel shows is the place it was
/// read from. Only pixels at least `margin` px from every edge, where the reading does not meet an edge rule. Each is the
/// (length, angle in radians clockwise from pointing right) of the step.
fn flow(params: &Params, size: usize, margin: usize) -> Result<Vec<(f64, f64)>, Error> {
    let params = Params {
        steps: 1,
        interpolation: Interpolation::Bilinear,
        ..params.clone()
    };
    let out = run_picture(&params, size, size, &coordinates(size, size))?;
    let read = places(&out, size, size);
    let mut steps = Vec::new();
    for row in margin..size - margin {
        for column in margin..size - margin {
            let (from_row, from_column) = read[row * size + column];
            // the pixel shows what was a step behind it, so the step is the pixel's place less the place it read
            let (down, across) = (row as f64 - from_row, column as f64 - from_column);
            steps.push((down.hypot(across), down.atan2(across)));
        }
    }
    Ok(steps)
}

#[test]
fn every_pixel_is_carried_amount_px_in_one_step_whatever_the_flow() -> Result<(), Error> {
    let params = Params {
        amount: 3.0,
        variation: 360.0,
        scale: 12.0,
        seed: 5,
        ..Params::default()
    };
    for (length, _) in flow(&params, 160, 4)? {
        assert!((length - 3.0).abs() < 1e-3, "length {length}");
    }
    Ok(())
}

#[test]
fn the_direction_is_the_angle_turned_by_noise_of_mean_zero_and_deviation_variation() -> Result<(), Error> {
    // the noise has deviation 1 at every place, so over a large picture (some 500 independent cells) the directions have about
    // the mean `angle` and the deviation `variation`
    for (angle, variation) in [(0.0_f32, 30.0_f32), (90.0, 60.0), (200.0, 45.0)] {
        let params = Params {
            amount: 2.0,
            angle,
            variation,
            scale: 10.0,
            seed: 3,
            ..Params::default()
        };
        let directions: Vec<f64> = flow(&params, 220, 2)?
            .into_iter()
            .map(|(_, direction)| {
                // the angle, wrapped to within half a turn of the mean angle
                let turn = (direction - f64::from(angle).to_radians()).rem_euclid(std::f64::consts::TAU);
                if turn > std::f64::consts::PI {
                    turn - std::f64::consts::TAU
                } else {
                    turn
                }
            })
            .collect();
        let (centre, spread) = (mean(&directions).to_degrees(), deviation(&directions).to_degrees());
        assert!(
            centre.abs() < 0.2 * f64::from(variation),
            "angle {angle}: mean turn {centre}"
        );
        assert!(
            (spread - f64::from(variation)).abs() < 0.2 * f64::from(variation),
            "angle {angle}: deviation {spread} for {variation}"
        );
    }
    Ok(())
}

#[test]
fn no_variation_is_one_current_in_the_angle() -> Result<(), Error> {
    let params = Params {
        amount: 4.0,
        angle: 90.0,
        variation: 0.0,
        ..Params::default()
    };
    for (length, direction) in flow(&params, 40, 5)? {
        assert!((length - 4.0).abs() < 1e-3);
        assert!((direction - std::f64::consts::FRAC_PI_2).abs() < 1e-3, "{direction}");
    }
    Ok(())
}

#[test]
fn the_flow_is_smooth_over_the_scale() -> Result<(), Error> {
    // neighbouring pixels turn alike when the scale is wide and (nearly) independently when it is a pixel or two
    let direction_correlation = |scale: f32| -> Result<f64, Error> {
        let params = Params {
            amount: 2.0,
            // 30 degrees turn a direction by under 130 even at the noise's greatest, so no direction wraps at 180 degrees
            variation: 30.0,
            scale,
            octaves: 1,
            seed: 8,
            ..Params::default()
        };
        let directions: Vec<f64> = flow(&params, 160, 3)?
            .into_iter()
            .map(|(_, direction)| direction)
            .collect();
        let (here, next): (Vec<f64>, Vec<f64>) = directions
            .iter()
            .zip(directions.iter().skip(1))
            .enumerate()
            .filter(|(at, _)| (at + 1) % 154 != 0)
            .map(|(_, pair)| (*pair.0, *pair.1))
            .unzip();
        Ok(correlation(&here, &next))
    };
    let (wide, fine) = (direction_correlation(40.0)?, direction_correlation(1.0)?);
    assert!(wide > 0.98, "wide {wide}");
    assert!(fine < 0.5, "fine {fine}");
    Ok(())
}

#[test]
fn the_seed_decides_the_flow() -> Result<(), Error> {
    let picture = varied(40, 40);
    let params = |seed| Params {
        amount: 8.0,
        scale: 10.0,
        seed,
        ..Params::default()
    };
    let one = run_picture(&params(1), 40, 40, &picture)?;
    assert_eq!(
        first_bit_difference(&one, &run_picture(&params(1), 40, 40, &picture)?),
        None
    );
    assert!(first_difference_within(&one, &run_picture(&params(2), 40, 40, &picture)?, 0.05).is_some());
    Ok(())
}

#[test]
fn more_steps_follow_the_bends_and_soften_the_picture() -> Result<(), Error> {
    // the same flow, the same total path: with 1 step a pixel's content moves straight; with more it bends with the flow
    let picture = varied(60, 60);
    let params = |steps| Params {
        amount: 20.0,
        steps,
        variation: 180.0,
        scale: 15.0,
        seed: 2,
        ..Params::default()
    };
    let straight_path = run_picture(&params(1), 60, 60, &picture)?;
    let bent = run_picture(&params(8), 60, 60, &picture)?;
    assert!(first_difference_within(&straight_path, &bent, 0.05).is_some());
    Ok(())
}

#[test]
fn a_flat_picture_stays_flat_and_every_value_stays_in_range() -> Result<(), Error> {
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                amount: 500.0,
                variation: 360.0,
                interpolation,
                edge,
                ..Params::default()
            };
            let flat = vec![0.3_f32; 7 * 9 * 3];
            let at = first_difference_within(&run_picture(&params, 7, 9, &flat)?, &flat, TOLERANCE);
            assert_eq!(at, None, "{params:?}");
            let out = run_picture(&params, 7, 9, &varied(7, 9))?;
            assert!(out.iter().all(|value| (0.0..=1.0).contains(value)), "{params:?}");
        }
    }
    Ok(())
}

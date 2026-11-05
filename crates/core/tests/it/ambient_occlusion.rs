//! `ambient_occlusion`: the picture darkened where its field says it is sheltered. The oracle is a brute-force Gaussian
//! mean over the kernel's window, summed by hand.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "the pictures are a few dozen px: every count and index is small"
)]

use bullterrier_image::effects::ambient_occlusion;
use bullterrier_image::{Edge, Error};

use crate::common::pictures::varied;
use crate::common::pixels::run_with_field;

const SIZE: usize = 64;
const MIDDLE: usize = SIZE / 2;

fn field_of(at: impl Fn(f64, f64) -> f64) -> Vec<f32> {
    (0..SIZE * SIZE)
        .map(|index| at((index % SIZE) as f64, (index / SIZE) as f64) as f32)
        .collect()
}

/// `inside` within `radius` px of the middle, `outside` elsewhere: a pit if it is lower, a bump if higher.
fn disc(inside: f64, outside: f64, radius: f64) -> Vec<f32> {
    field_of(|x, y| {
        if (x - MIDDLE as f64).hypot(y - MIDDLE as f64) <= radius {
            inside
        } else {
            outside
        }
    })
}

/// A picture with no channel near zero, so that out / in is defined everywhere.
fn picture() -> Vec<f32> {
    varied(SIZE, SIZE).iter().map(|value| 0.05 + value * 0.95).collect()
}

fn run(params: &ambient_occlusion::Params, field: &[f32], picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_with_field(params, "field", field, SIZE, SIZE, picture)
}

/// 1 - out / in of the first channel at each pixel: 0 = unchanged, 1 = black.
fn darkening(picture: &[f32], out: &[f32]) -> Vec<f64> {
    (0..SIZE * SIZE)
        .map(|pixel| 1.0 - f64::from(out[pixel * 3]) / f64::from(picture[pixel * 3]))
        .collect()
}

/// The cavity at (`y`, `x`) by brute force: the Gaussian-weighted mean of the field over the square window of half-width
/// `int(truncate * sigma + 0.5)` (the separable kernel), minus the field there. The window lies inside the picture.
fn cavity(field: &[f32], y: usize, x: usize, sigma: f64, truncate: f64) -> f64 {
    let reach = (truncate * sigma + 0.5) as i64;
    let (mut total, mut weights) = (0.0, 0.0);
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let weight = (-0.5 * ((dy * dy + dx * dx) as f64) / (sigma * sigma)).exp();
            let at = (y as i64 + dy) as usize * SIZE + (x as i64 + dx) as usize;
            total += weight * f64::from(field[at]);
            weights += weight;
        }
    }
    total / weights - f64::from(field[y * SIZE + x])
}

#[test]
fn a_flat_or_evenly_sloping_field_leaves_the_picture_as_it_is() -> Result<(), Error> {
    let picture = picture();
    let slope = field_of(|x, y| 0.5 + 0.004 * (x - MIDDLE as f64) - 0.002 * (y - MIDDLE as f64));
    for (field, radius, inside) in [
        (field_of(|_, _| 0.0), 8.0, 0..SIZE),
        (field_of(|_, _| 0.4), 200.0, 0..SIZE),
        (field_of(|_, _| 1.0), 3.0, 0..SIZE),
        (slope, 3.0, 13..SIZE - 13), // a Gaussian keeps a ramp, away from the edge rule (its reach is 12)
    ] {
        let params = ambient_occlusion::Params {
            radius,
            gain: 100.0,
            ..Default::default()
        };
        let out = run(&params, &field, &picture)?;
        for row in inside.clone() {
            for column in inside.clone() {
                for channel in 0..3 {
                    let at = (row * SIZE + column) * 3 + channel;
                    assert!(
                        (out[at] - picture[at]).abs() < 2e-5,
                        "radius {radius}, ({row}, {column})"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn it_never_brightens_and_darkens_every_channel_by_one_grey_factor() -> Result<(), Error> {
    let picture = picture();
    let field = field_of(|x, y| ((x * 7.0 + y * 13.0 + x * y) % 17.0) / 17.0);
    for (radius, gain) in [(3.0, 4.0), (1.0, 100.0), (20.0, 100.0), (40.0, 1.0)] {
        let params = ambient_occlusion::Params {
            radius,
            gain,
            ..Default::default()
        };
        let out = run(&params, &field, &picture)?;
        let mut darkened = false;
        for (out, picture) in out.chunks(3).zip(picture.chunks(3)) {
            let factors = [0, 1, 2].map(|channel| out[channel] / picture[channel]);
            assert!(factors.iter().all(|&factor| factor <= 1.0), "radius {radius}");
            assert!(
                factors.iter().all(|&factor| (factor - factors[0]).abs() < 2e-6),
                "radius {radius}"
            );
            darkened |= factors[0] < 0.99;
        }
        assert!(
            darkened,
            "radius {radius}, gain {gain}: a field with hollows darkens something"
        );
    }
    Ok(())
}

#[test]
fn the_darkening_is_the_gain_times_the_brute_force_cavity() -> Result<(), Error> {
    let (pit, bump) = (disc(0.3, 0.8, 8.0), disc(1.0, 0.4, 8.0));
    let picture = picture();
    // (field, point, radius, gain, truncate): every row is a case the clip leaves alone
    let rows = [
        (&pit, (MIDDLE, MIDDLE), 2.0, 1.0, 4.0),
        (&pit, (MIDDLE, MIDDLE + 6), 2.0, 4.0, 4.0),
        (&pit, (MIDDLE, MIDDLE), 4.0, 1.0, 4.0),
        (&bump, (MIDDLE, MIDDLE + 10), 2.0, 2.0, 4.0),
        (&bump, (MIDDLE + 12, MIDDLE), 4.0, 1.0, 4.0),
        (&bump, (MIDDLE + 9, MIDDLE + 9), 3.0, 3.0, 4.0),
        (&bump, (MIDDLE, MIDDLE + 11), 4.0, 3.0, 1.5),
    ];
    for (field, (y, x), radius, gain, truncate) in rows {
        let params = ambient_occlusion::Params {
            radius: radius as f32,
            gain: gain as f32,
            truncate: truncate as f32,
            ..Default::default()
        };
        let expected = gain * cavity(field, y, x, radius, truncate);
        assert!(0.0 < expected && expected < 1.0, "{expected}: the clip would act");
        let got = darkening(&picture, &run(&params, field, &picture)?)[y * SIZE + x];
        assert!(
            (got - expected).abs() < 2e-5,
            "({y}, {x}), radius {radius}: {got} against {expected}"
        );
    }
    Ok(())
}

#[test]
fn a_bump_keeps_its_top_and_darkens_the_plane_beside_it() -> Result<(), Error> {
    let (bump, picture) = (disc(1.0, 0.4, 8.0), picture());
    for radius in [2.0, 6.0] {
        let params = ambient_occlusion::Params {
            radius,
            ..Default::default()
        };
        let out = run(&params, &bump, &picture)?;
        for (pixel, &depth) in bump.iter().enumerate() {
            if depth > 0.5 {
                assert_eq!(out[pixel * 3..][..3], picture[pixel * 3..][..3], "radius {radius}");
            }
        }
        assert!(
            darkening(&picture, &out)[MIDDLE * SIZE + MIDDLE + 10] > 0.05,
            "radius {radius}"
        );
    }
    Ok(())
}

#[test]
fn raising_the_radius_widens_the_darkened_area() -> Result<(), Error> {
    let (bump, picture) = (disc(1.0, 0.4, 6.0), picture());
    let mut areas = vec![];
    for radius in [1.5, 3.0, 6.0] {
        let params = ambient_occlusion::Params {
            radius,
            ..Default::default()
        };
        let dark = darkening(&picture, &run(&params, &bump, &picture)?);
        areas.push(dark.iter().filter(|&&darkening| darkening > 0.01).count());
    }
    assert!(areas[0] < areas[1] && areas[1] < areas[2], "{areas:?}");
    Ok(())
}

#[test]
fn with_the_wrap_edge_shifting_picture_and_field_shifts_the_result() -> Result<(), Error> {
    let picture = picture();
    let field = field_of(|x, y| ((x * 5.0 + y * 11.0 + x * y) % 13.0) / 13.0);
    let (dy, dx) = (SIZE / 3, SIZE / 2);
    let moved = |values: &[f32], channels: usize| -> Vec<f32> {
        let mut out = vec![0.0; values.len()];
        for (index, chunk) in values.chunks_exact(channels).enumerate() {
            let (y, x) = (((index / SIZE) + dy) % SIZE, ((index % SIZE) + dx) % SIZE);
            out[(y * SIZE + x) * channels..][..channels].copy_from_slice(chunk);
        }
        out
    };
    for radius in [3.0, 20.0] {
        let params = ambient_occlusion::Params {
            radius,
            edge: Edge::Wrap,
            ..Default::default()
        };
        let out = run(&params, &field, &picture)?;
        let out_moved = run(&params, &moved(&field, 1), &moved(&picture, 3))?;
        let expected = moved(&out, 3);
        let at = out_moved.iter().zip(&expected).position(|(a, b)| (a - b).abs() > 2e-5);
        assert_eq!(at, None, "radius {radius}");
    }
    Ok(())
}

#[test]
fn single_pixels_and_thin_pictures_are_darkened_without_leaving_the_range() -> Result<(), Error> {
    for (height, width) in [(1, 1), (1, 9), (9, 1), (2, 3)] {
        let picture = varied(height, width);
        let field: Vec<f32> = (0..height * width).map(|index| (index % 3) as f32 / 2.0).collect();
        let params = ambient_occlusion::Params {
            radius: 50.0,
            gain: 100.0,
            ..Default::default()
        };
        let out = run_with_field(&params, "field", &field, height, width, &picture)?;
        assert!(
            out.iter()
                .zip(&picture)
                .all(|(out, picture)| (0.0..=*picture).contains(out)),
            "{height} x {width}"
        );
    }
    Ok(())
}

#[test]
fn turning_the_picture_and_its_field_on_their_side_turns_the_result() -> Result<(), Error> {
    // a map of 5 rows and 120 columns: a transform made for the rows is too short for the columns (radius 6, reach 24:
    // 64 against 256 samples), and one made for the columns needlessly long for the rows; radius 2 is summed
    let (rows, columns) = (5, 120);
    let picture = varied(rows, columns);
    let field: Vec<f32> = (0..rows * columns)
        .map(|index| {
            (((index / columns) * 5 + (index % columns) * 7 + (index / columns) * (index % columns)) % 11) as f32 / 10.0
        })
        .collect();
    let turn = |values: &[f32], channels: usize| -> Vec<f32> {
        let mut out = vec![0.0; values.len()];
        for row in 0..rows {
            for column in 0..columns {
                let (from, to) = ((row * columns + column) * channels, (column * rows + row) * channels);
                out[to..to + channels].copy_from_slice(&values[from..from + channels]);
            }
        }
        out
    };
    for radius in [2.0, 6.0] {
        let params = ambient_occlusion::Params {
            radius,
            gain: 20.0,
            ..Default::default()
        };
        let upright = run_with_field(&params, "field", &field, rows, columns, &picture)?;
        let on_its_side = run_with_field(&params, "field", &turn(&field, 1), columns, rows, &turn(&picture, 3))?;
        let expected = turn(&upright, 3);
        let at = on_its_side
            .iter()
            .zip(&expected)
            .position(|(a, b)| (a - b).abs() > 2e-5);
        assert_eq!(at, None, "radius {radius}");
    }
    Ok(())
}

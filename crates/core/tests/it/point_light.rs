//! `point_light`: a lamp lights a relief. Every expected value is worked here from the geometry (a lamp, a surface and
//! its normal) and from the colour maths written out again, on pictures and fields whose slopes are known.

#![expect(
    clippy::cast_precision_loss,
    reason = "the pictures are a few dozen px: every count and index is small"
)]

use bullterrier_image::effects::point_light;
use bullterrier_image::params::Color;
use bullterrier_image::{Edge, Error};

use crate::common::compare::assert_close;
use crate::common::pictures::{ramp, varied};
use crate::common::pixels::run_with_field;

const SIZE: usize = 32;
/// Field gained per px along `plane`.
const RISE: f32 = 0.005;
/// Pixels clear of the first and last row and column, where the slope's edge rule decides.
const INSIDE: std::ops::Range<usize> = 2..SIZE - 2;

fn params(settings: impl FnOnce(&mut point_light::Params)) -> point_light::Params {
    let mut params = point_light::Params {
        // no smoothing and no picture relief unless a test asks: the slopes are exactly the field's
        blur: 0.0,
        detail: 0.0,
        height: 100.0,
        edge: Edge::Reflect,
        ..Default::default()
    };
    settings(&mut params);
    params
}

fn run(params: &point_light::Params, field: &[f32], picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_with_field(params, "field", field, SIZE, SIZE, picture)
}

fn flat() -> Vec<f32> {
    vec![0.0; SIZE * SIZE]
}

fn grey(level: f32) -> Vec<f32> {
    vec![level; SIZE * SIZE * 3]
}

/// A field of 0.5 in the middle rising by `RISE` per px towards `direction` (degrees clockwise from pointing right).
fn plane(direction: f32) -> Vec<f32> {
    let (cos, sin) = (direction.to_radians().cos(), direction.to_radians().sin());
    let middle = SIZE as f32 / 2.0;
    (0..SIZE * SIZE)
        .map(|index| {
            let (x, y) = ((index % SIZE) as f32, (index / SIZE) as f32);
            0.5 + RISE * ((x - middle) * cos + (y - middle) * sin)
        })
        .collect()
}

fn to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// What the lamp makes of one pixel of grey-or-coloured `picture`, from the geometry: the surface at the pixel stands
/// `height_px` high and rises `rise` (x, y) px per px; the lamp stands over the canvas point (x, y) at `z` px plus the
/// tallest relief there can be. The light is the unit normal against the unit vector to the lamp, times the fall-off
/// window (1 - (distance over the relief's top / reach)^2)^2, then added (screen) or taken away in linear light.
fn expected(
    params: &point_light::Params,
    (column, row): (usize, usize),
    (height_px, rise): (f32, (f32, f32)),
    picture: [f32; 3],
) -> [f32; 3] {
    let width = SIZE as f32;
    let lamp = (
        params.x * (width - 1.0),
        params.y * (width - 1.0),
        params.z + params.height + params.detail,
    );
    let to_lamp = [lamp.0 - column as f32, lamp.1 - row as f32, lamp.2 - height_px];
    let distance = to_lamp.iter().map(|v| v * v).sum::<f32>().sqrt();
    // the light fades with the distance from the lamp over the relief's top (`z` px up), not from the surface itself
    let fall = (to_lamp[0] * to_lamp[0] + to_lamp[1] * to_lamp[1] + params.z.powi(2)).sqrt();
    let normal_length = (1.0 + rise.0 * rise.0 + rise.1 * rise.1).sqrt();
    let facing = (-rise.0 * to_lamp[0] - rise.1 * to_lamp[1] + to_lamp[2]) / normal_length / distance;
    let window = (1.0 - (fall / params.radius).powi(2)).max(0.0);
    let lit = facing.max(0.0) * window * window;
    let mut out = [0.0; 3];
    for channel in 0..3 {
        let linear = to_linear(picture[channel]);
        let base = linear * params.ambient;
        let light = params.gain * lit * params.color.0[channel];
        let result = if light >= 0.0 {
            1.0 - (1.0 - base) * (1.0 - (linear * light).clamp(0.0, 1.0))
        } else {
            (base * (1.0 + light)).max(0.0)
        };
        out[channel] = to_srgb(result);
    }
    out
}

fn pixel(values: &[f32], column: usize, row: usize) -> [f32; 3] {
    let at = (row * SIZE + column) * 3;
    [values[at], values[at + 1], values[at + 2]]
}

#[test]
fn no_light_and_full_ambient_leave_the_picture_as_it_was() -> Result<(), Error> {
    let picture = varied(SIZE, SIZE);
    let disc: Vec<f32> = (0..SIZE * SIZE)
        .map(|i| (1.0 - ((i % SIZE) as f32 - 16.0).hypot((i / SIZE) as f32 - 16.0) / 10.0).clamp(0.0, 1.0))
        .collect();
    for field in [flat(), plane(30.0), disc] {
        for detail in [0.0, 6.0] {
            let settings = params(|p| (p.gain, p.ambient, p.detail) = (0.0, 1.0, detail));
            assert_close(&run(&settings, &field, &picture)?, &picture, 1e-5, "gain 0, ambient 1");
        }
    }
    Ok(())
}

#[test]
fn over_a_flat_relief_the_light_is_the_fall_off_window_of_the_distance() -> Result<(), Error> {
    let settings = params(|p| {
        (p.x, p.y, p.z, p.radius) = (0.0, 0.0, 6.4, 28.8);
        (p.gain, p.ambient, p.height) = (1.5, 0.4, 0.0);
    });
    let out = run(&settings, &flat(), &grey(0.4))?;
    // under the lamp, in the middle, and (beyond the reach of 28.8 px) in the far corner
    for (column, row) in [(0, 0), (10, 20), (SIZE - 1, SIZE - 1)] {
        let want = expected(&settings, (column, row), (0.0, (0.0, 0.0)), [0.4; 3]);
        assert_close(&pixel(&out, column, row), &want, 1e-5, &format!("pixel {column},{row}"));
    }
    let dimmed = to_srgb(to_linear(0.4) * 0.4);
    assert_close(
        &pixel(&out, SIZE - 1, SIZE - 1),
        &[dimmed; 3],
        1e-6,
        "beyond the reach: only the ambient",
    );
    Ok(())
}

#[test]
fn a_surface_is_lit_by_the_angle_between_its_normal_and_the_lamp() -> Result<(), Error> {
    let settings = params(|p| {
        (p.x, p.y, p.z, p.radius) = (1.0, 0.5, 4.8, 51.2);
        (p.gain, p.ambient) = (1.0, 0.5);
    });
    for direction in [0.0_f32, 90.0, 180.0, 300.0] {
        let (cos, sin) = (direction.to_radians().cos(), direction.to_radians().sin());
        let rise = (settings.height * RISE * cos, settings.height * RISE * sin);
        let (field, middle) = (plane(direction), SIZE as f32 / 2.0);
        let out = run(&settings, &field, &grey(0.5))?;
        for row in INSIDE {
            for column in INSIDE {
                let height_px = settings.height * field[row * SIZE + column];
                let want = expected(&settings, (column, row), (height_px, rise), [0.5; 3]);
                assert_close(
                    &pixel(&out, column, row),
                    &want,
                    1e-5,
                    &format!("direction {direction} at {column},{row} (middle {middle})"),
                );
            }
        }
    }
    // ground that falls toward the lamp (rises away from it) faces it and is brighter than ground that rises toward it
    let facing = run(&settings, &plane(180.0), &grey(0.5))?;
    let turned_away = run(&settings, &plane(0.0), &grey(0.5))?;
    assert!(pixel(&facing, 16, 16)[0] > pixel(&turned_away, 16, 16)[0]);
    Ok(())
}

#[test]
fn a_coloured_lamp_adds_only_its_own_colour() -> Result<(), Error> {
    let red = params(|p| {
        (p.x, p.y, p.gain, p.ambient, p.height) = (0.5, 0.5, 2.0, 1.0, 0.0);
        p.color = Color([1.0, 0.0, 0.0]);
    });
    let out = run(&red, &flat(), &grey(0.3))?;
    let at = (SIZE / 2, SIZE / 2);
    let lit = pixel(&out, at.0, at.1);
    assert!(lit[0] > 0.3 + 0.01, "red rises under a red lamp: {lit:?}");
    assert_close(&lit[1..], &[0.3, 0.3], 1e-6, "green and blue stay");

    let white = params(|p| (p.x, p.y, p.gain, p.ambient, p.height) = (0.5, 0.5, 2.0, 1.0, 0.0));
    let lit = pixel(&run(&white, &flat(), &grey(0.3))?, at.0, at.1);
    assert!(lit[0] > 0.31);
    assert_close(
        &[lit[1], lit[2]],
        &[lit[0]; 2],
        1e-6,
        "a white lamp lifts a grey equally",
    );
    Ok(())
}

#[test]
fn a_negative_gain_takes_the_light_away_and_never_adds() -> Result<(), Error> {
    let picture = varied(SIZE, SIZE);
    let settings = params(|p| {
        (p.x, p.y, p.z, p.radius) = (0.5, 0.5, 6.4, 32.0);
        (p.gain, p.ambient, p.height) = (-1.0, 1.0, 0.0);
    });
    let out = run(&settings, &flat(), &picture)?;
    for (index, (&after, &before)) in out.iter().zip(&picture).enumerate() {
        assert!(after <= before + 1e-6, "channel {index} darkened: {before} -> {after}");
    }
    for (column, row) in [(16, 16), (3, 29)] {
        let at = (row * SIZE + column) * 3;
        let want = expected(
            &settings,
            (column, row),
            (0.0, (0.0, 0.0)),
            [picture[at], picture[at + 1], picture[at + 2]],
        );
        assert_close(&pixel(&out, column, row), &want, 1e-5, "hand tone");
    }
    Ok(())
}

#[test]
fn the_lamp_is_brightest_at_the_pixel_nearest_its_place_on_the_canvas() -> Result<(), Error> {
    // the lamp's canvas point is (0.25 x 31, 0.75 x 31) = (7.75, 23.25): the pixel (8, 23)
    let settings = params(|p| {
        (p.x, p.y, p.z, p.radius) = (0.25, 0.75, 3.2, 38.4);
        (p.gain, p.ambient, p.height) = (1.0, 0.5, 0.0);
    });
    let out = run(&settings, &flat(), &grey(0.5))?;
    let brightest = (0..SIZE * SIZE)
        .max_by(|&a, &b| out[a * 3].total_cmp(&out[b * 3]))
        .unwrap_or(0);
    assert_eq!((brightest % SIZE, brightest / SIZE), (8, 23));
    Ok(())
}

#[test]
fn the_pictures_own_brightness_is_a_fine_relief_that_detail_makes_tall() -> Result<(), Error> {
    // a grey ramp from 0 to 1 over 31 px, lit with no field relief: it rises `detail / 31` px per px to the right
    let picture = ramp(SIZE, SIZE);
    let settings = params(|p| {
        (p.x, p.y, p.z, p.radius) = (1.0, 0.5, 4.8, 51.2);
        (p.gain, p.ambient, p.height, p.detail) = (1.0, 0.5, 0.0, 12.0);
    });
    let out = run(&settings, &flat(), &picture)?;
    let rise = (settings.detail / (SIZE as f32 - 1.0), 0.0);
    for row in INSIDE {
        for column in INSIDE {
            let level = column as f32 / (SIZE as f32 - 1.0);
            let want = expected(&settings, (column, row), (settings.detail * level, rise), [level; 3]);
            assert_close(
                &pixel(&out, column, row),
                &want,
                1e-5,
                &format!("detail at {column},{row}"),
            );
        }
    }
    // with no detail the same picture is lit as a flat surface: the ramp is not read as a shape
    let plain =
        params(|p| (p.x, p.y, p.z, p.radius, p.gain, p.ambient, p.height) = (1.0, 0.5, 4.8, 51.2, 1.0, 0.5, 0.0));
    let out = run(&plain, &flat(), &picture)?;
    for column in INSIDE {
        let level = column as f32 / (SIZE as f32 - 1.0);
        let want = expected(&plain, (column, 16), (0.0, (0.0, 0.0)), [level; 3]);
        assert_close(&pixel(&out, column, 16), &want, 1e-5, "no detail, flat");
    }
    Ok(())
}

#[test]
fn a_glint_rides_on_bright_parts_only_and_is_off_by_default() -> Result<(), Error> {
    let lit = |settings: &point_light::Params, level| run(settings, &flat(), &grey(level));
    let plain = params(|p| (p.x, p.y, p.z, p.gain, p.ambient, p.height) = (0.5, 0.5, 9.6, 1.0, 0.5, 0.0));
    let glossy = point_light::Params {
        specular: 0.8,
        ..plain.clone()
    };
    assert_eq!(point_light::Params::default().specular.to_bits(), 0.0_f32.to_bits());
    let (without, with) = (lit(&plain, 0.6)?, lit(&glossy, 0.6)?);
    assert!(with.iter().zip(&without).all(|(a, b)| a >= b));
    assert!(
        pixel(&with, 16, 16)[0] > pixel(&without, 16, 16)[0] + 0.01,
        "a glint under the lamp"
    );
    assert_close(&lit(&glossy, 0.0)?, &grey(0.0), 1e-7, "black has no glint");
    Ok(())
}

#[test]
fn blur_smooths_the_field_before_its_slopes_are_taken() -> Result<(), Error> {
    let picture = grey(0.3);
    let spike: Vec<f32> = (0..SIZE * SIZE)
        .map(|index| f32::from(u8::from((index % SIZE, index / SIZE) == (16, 16))))
        .collect();
    let lamp = |blur| {
        params(|p| {
            (p.x, p.y, p.z, p.radius) = (0.5, 0.5, 6.4, 64.0);
            (p.gain, p.ambient, p.height, p.blur) = (1.0, 0.5, 10.0, blur);
        })
    };
    let flat_relief = run(&lamp(0.0), &flat(), &picture)?;
    let (sharp, soft) = (run(&lamp(0.0), &spike, &picture)?, run(&lamp(2.0), &spike, &picture)?);
    let change = |lit: &[f32], row: usize, column: usize| {
        (lit[(row * SIZE + column) * 3] - flat_relief[(row * SIZE + column) * 3]).abs()
    };
    // a spike's slopes reach one pixel; its blur reaches several: three pixels away only the blurred field is lit
    assert!(change(&sharp, 16, 19) < 1e-6);
    assert!(change(&soft, 16, 19) > 1e-4);
    let worst = |lit: &[f32]| (0..SIZE * SIZE).fold(0.0_f32, |most, at| most.max(change(lit, at / SIZE, at % SIZE)));
    assert!(worst(&soft) < worst(&sharp), "blur lowers the steepest slope");
    Ok(())
}

#[test]
fn the_pictures_brightness_is_smoothed_a_third_as_much_as_the_field() -> Result<(), Error> {
    // one bright pixel among grey, read as a relief at a `blur` of 3 px: the picture's share is smoothed over 1 px, so six
    // pixels away no relief is left, while beside the pixel there is
    let mut picture = grey(0.3);
    let spot = (16 * SIZE + 16) * 3;
    picture[spot..spot + 3].fill(1.0);
    let settings = params(|p| {
        (p.x, p.y, p.z, p.radius) = (0.5, 0.5, 6.4, 64.0);
        (p.gain, p.ambient, p.height, p.detail, p.blur) = (1.0, 0.5, 0.0, 40.0, 3.0);
    });
    let (lit, level) = (run(&settings, &flat(), &picture)?, run(&settings, &flat(), &grey(0.3))?);
    let change = |row: usize, column: usize| (lit[(row * SIZE + column) * 3] - level[(row * SIZE + column) * 3]).abs();
    assert!(change(16, 22) < 1e-5, "six pixels away: {}", change(16, 22));
    assert!(change(16, 18) > 1e-3, "two pixels away: {}", change(16, 18));
    Ok(())
}

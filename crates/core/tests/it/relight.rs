//! `relight`: a field lit as a relief. Gains are worked from the geometry (a unit normal against a unit vector to the
//! light) on fields whose slopes are known.

#![expect(
    clippy::cast_precision_loss,
    reason = "the pictures are a few dozen px: every count and index is small"
)]

use bullterrier_image::effects::relight;
use bullterrier_image::{Edge, Error};

use crate::common::compare::assert_close;
use crate::common::pictures::varied;
use crate::common::pixels::run_with_field;

const SIZE: usize = 32;
/// Field gained per px along `plane`: 0.75 px per px under a relief 150 px tall.
const RISE: f32 = 0.005;
/// Pixels beyond the reach of the edge rule through a blur of 2 px (8 px at 4 sigmas).
const INSIDE: std::ops::Range<usize> = 10..SIZE - 10;

fn params(settings: impl FnOnce(&mut relight::Params)) -> relight::Params {
    let mut params = relight::Params {
        height: 150.0,
        ..Default::default()
    };
    settings(&mut params);
    params
}

fn run(params: &relight::Params, field: &[f32], picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_with_field(params, "field", field, SIZE, SIZE, picture)
}

fn field_of(at: impl Fn(f32, f32) -> f32) -> Vec<f32> {
    (0..SIZE * SIZE)
        .map(|index| at((index % SIZE) as f32, (index / SIZE) as f32))
        .collect()
}

/// A field of 0.5 in the middle rising by `RISE` per px towards `direction` (degrees clockwise from pointing right).
fn plane(direction: f32) -> Vec<f32> {
    let (cos, sin) = (direction.to_radians().cos(), direction.to_radians().sin());
    let middle = SIZE as f32 / 2.0;
    field_of(|x, y| 0.5 + RISE * ((x - middle) * cos + (y - middle) * sin))
}

/// The gain of a surface rising `slope_x` px per px to the right and `slope_y` down, from the geometry: its unit normal
/// (-dx, -dy, 1) / length dotted with the unit vector towards the light (x right, y down, z up), over what a
/// flat surface (normal (0, 0, 1)) catches.
fn hand_gain(slope_x: f32, slope_y: f32, angle: f32, elevation: f32) -> f32 {
    let (a, e) = (angle.to_radians(), elevation.to_radians());
    let light = [e.cos() * a.cos(), e.cos() * a.sin(), e.sin()];
    let length = (1.0 + slope_x * slope_x + slope_y * slope_y).sqrt();
    (-slope_x * light[0] - slope_y * light[1] + light[2]) / length / light[2]
}

fn grey(level: f32) -> Vec<f32> {
    vec![level; SIZE * SIZE * 3]
}

#[test]
fn a_flat_field_leaves_the_picture_as_it_is_and_height_0_flattens_any_field() -> Result<(), Error> {
    let picture = varied(SIZE, SIZE);
    let disc = field_of(|x, y| (1.0 - ((x - 16.0).hypot(y - 16.0) / 10.0)).clamp(0.0, 1.0));
    let rows = [
        (params(|_| {}), vec![0.4; SIZE * SIZE]),
        (params(|p| (p.angle, p.elevation) = (10.0, 1.0)), vec![1.0; SIZE * SIZE]),
        (params(|p| p.height = 0.0), disc),
        (params(|p| (p.height, p.elevation) = (0.0, 10.0)), plane(30.0)),
    ];
    for (settings, field) in rows {
        assert_close(&run(&settings, &field, &picture)?, &picture, 1e-6, "flat relief");
    }
    Ok(())
}

#[test]
fn a_plane_is_lit_by_the_geometry_of_its_normal() -> Result<(), Error> {
    // (direction the field rises towards, light angle, elevation); a grey of 0.3 stays below white
    let rows = [
        (0.0, 225.0, 45.0),
        (90.0, 225.0, 45.0),
        (200.0, 40.0, 60.0),
        (135.0, 135.0, 30.0),
        (315.0, 135.0, 30.0),
    ];
    for (direction, angle, elevation) in rows {
        for blur in [0.0, 2.0] {
            let settings = params(|p| (p.angle, p.elevation, p.blur) = (angle, elevation, blur));
            let out = run(&settings, &plane(direction), &grey(0.3))?;
            let (slope_x, slope_y) = (0.75 * direction.to_radians().cos(), 0.75 * direction.to_radians().sin());
            let expected = (0.3 * hand_gain(slope_x, slope_y, angle, elevation)).clamp(0.0, 1.0);
            for (row, column) in INSIDE.flat_map(|row| INSIDE.map(move |column| (row, column))) {
                let at = (row * SIZE + column) * 3;
                assert_close(
                    &out[at..at + 3],
                    &[expected; 3],
                    1e-5,
                    &format!("direction {direction} light {angle}/{elevation} blur {blur}"),
                );
            }
        }
    }
    Ok(())
}

#[test]
fn the_light_comes_from_the_angle_and_a_whole_turn_is_the_same_light() -> Result<(), Error> {
    let (picture, field) = (grey(0.3), plane(0.0));
    let at = |angle| run(&params(|p| p.angle = angle), &field, &picture);
    assert_close(&at(0.0)?, &at(360.0)?, 1e-5, "0 and 360 degrees");
    assert!(at(0.0)? != at(180.0)?, "the opposite side is lit differently");
    // light from the right (0) on a field rising to the right is grazed from the high side: darker than a flat 0.3
    assert!(at(0.0)?[(16 * SIZE + 16) * 3] < 0.3);
    assert!(at(180.0)?[(16 * SIZE + 16) * 3] > 0.3);
    Ok(())
}

#[test]
fn saturation_mixes_the_picture_into_mid_grey_before_the_one_grey_gain() -> Result<(), Error> {
    let picture = varied(SIZE, SIZE);
    let flat = vec![0.5; SIZE * SIZE];
    assert_close(
        &run(&params(|p| p.saturation = 0.0), &flat, &picture)?,
        &grey(0.5),
        1e-6,
        "relief alone, flat: mid grey",
    );
    assert_close(
        &run(&params(|p| p.saturation = 0.25), &flat, &picture)?,
        &picture.iter().map(|value| 0.25 * value + 0.375).collect::<Vec<_>>(),
        1e-6,
        "a quarter picture, three quarters mid grey",
    );
    // one gain for all channels: the ratios of a pixel's channels are kept
    let tinted = [0.1, 0.2, 0.3].repeat(SIZE * SIZE);
    let out = run(&params(|_| {}), &plane(30.0), &tinted)?;
    let at = (16 * SIZE + 16) * 3;
    assert_close(
        &[out[at + 1] / out[at], out[at + 2] / out[at]],
        &[2.0, 3.0],
        1e-4,
        "channel ratios",
    );
    assert!((out[at] - 0.1).abs() > 0.01, "the plane is lit, not flat");
    Ok(())
}

#[test]
fn blur_smooths_the_field_before_its_slopes_are_taken() -> Result<(), Error> {
    let picture = grey(0.3);
    let spike = field_of(|x, y| if (x, y) == (16.0, 16.0) { 1.0 } else { 0.0 });
    let sharp = run(&params(|p| (p.blur, p.height) = (0.0, 10.0)), &spike, &picture)?;
    let soft = run(&params(|p| (p.blur, p.height) = (2.0, 10.0)), &spike, &picture)?;
    let at = |picture: &[f32], row: usize, column: usize| picture[(row * SIZE + column) * 3];
    // a spike's slopes reach one pixel; its blur reaches several: three pixels away only the blurred field is lit
    assert!((at(&sharp, 16, 19) - 0.3).abs() < 1e-6);
    assert!((at(&soft, 16, 19) - 0.3).abs() > 1e-3);
    let range = |picture: &[f32]| {
        picture
            .iter()
            .fold(0.0_f32, |worst, value| worst.max((value - 0.3).abs()))
    };
    assert!(range(&soft) < range(&sharp), "blur lowers the steepest slope");
    Ok(())
}

#[test]
fn the_edge_rule_decides_the_first_column_only() -> Result<(), Error> {
    // straight above (elevation 90) the gain is 1 / sqrt(1 + dx^2 + dy^2); the field rises by 0.01 per column, so with a
    // height of 100 px dx is 1 in the interior; in column 0 the previous column is column 0 (reflect, nearest), column 1
    // (mirror) or the last one (wrap)
    let field = field_of(|x, _| 0.01 * x);
    let last = SIZE as f32 - 1.0;
    let rows = [
        (Edge::Reflect, 0.5),
        (Edge::Nearest, 0.5),
        (Edge::Mirror, 0.0),
        (Edge::Wrap, 100.0 * (0.01 - 0.01 * last) / 2.0),
    ];
    for (edge, dx) in rows {
        let settings = params(|p| (p.height, p.elevation, p.blur, p.edge) = (100.0, 90.0, 0.0, edge));
        let out = run(&settings, &field, &grey(0.5))?;
        let gain = |dx: f32| 0.5 / (1.0 + dx * dx).sqrt();
        for row in 0..SIZE {
            assert_close(
                &out[row * SIZE * 3..][..3],
                &[gain(dx); 3],
                1e-4,
                &format!("{edge:?}, column 0"),
            );
            assert_close(
                &out[(row * SIZE + 5) * 3..][..3],
                &[gain(1.0); 3],
                1e-4,
                &format!("{edge:?}, column 5"),
            );
        }
    }
    Ok(())
}

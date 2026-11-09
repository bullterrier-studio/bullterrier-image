//! `liquid_metal`: chrome from the picture's brightness. The expected pictures are worked by hand from the environment the
//! effect documents (black on the ground, the tint at the horizon, white in the sky), on grey ramps whose slopes are known.

#![expect(
    clippy::cast_precision_loss,
    reason = "the pictures are a few dozen px: every count and index is small"
)]

use bullterrier_image::effects::{gaussian_blur, liquid_metal};
use bullterrier_image::params::Color;
use bullterrier_image::{Edge, EdgeOrConstant, Error};

use crate::common::compare::assert_close;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;

const SIZE: usize = 24;
const TINT: [f32; 3] = [0.8, 0.5, 0.2];
/// Brightness gained per row by `down_ramp`.
const RISE: f32 = 0.005;

fn params(settings: impl FnOnce(&mut liquid_metal::Params)) -> liquid_metal::Params {
    let mut params = liquid_metal::Params {
        smoothing: 0.0,
        relief: 100.0,
        horizon: 0.5,
        tint: Color(TINT),
        ..Default::default()
    };
    settings(&mut params);
    params
}

fn run(params: &liquid_metal::Params, picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_picture(params, SIZE, SIZE, picture)
}

/// A grey picture whose brightness is `RISE` x row (rising downwards) when `down` and falls by it per row otherwise.
fn ramp(down: bool, across: bool) -> Vec<f32> {
    (0..SIZE * SIZE)
        .flat_map(|index| {
            let (row, column) = ((index / SIZE) as f32, (index % SIZE) as f32);
            let along = if across { column } else { row };
            [if down { 0.5 + RISE * along } else { 0.5 - RISE * along }; 3]
        })
        .collect()
}

/// The chrome the environment gives a surface looking at `t` (0 ground, 1 sky) under `horizon`, worked by hand: the
/// gradient's position is t / (2 horizon) below the horizon and 0.5 + (t - horizon) / (2 (1 - horizon)) above it.
fn chrome(t: f32, horizon: f32) -> [f32; 3] {
    let shine = if t < horizon {
        t / (2.0 * horizon)
    } else {
        0.5 + (t - horizon) / (2.0 * (1.0 - horizon))
    };
    let (toward_tint, toward_white) = ((2.0 * shine).min(1.0), (2.0 * shine - 1.0).max(0.0));
    TINT.map(|level| level * toward_tint + (1.0 - level) * toward_white)
}

/// The pixel at (`row`, 12) of `picture`.
fn at_row(picture: &[f32], row: usize) -> [f32; 3] {
    let start = (row * SIZE + SIZE / 2) * 3;
    [picture[start], picture[start + 1], picture[start + 2]]
}

#[test]
fn a_flat_mirror_is_the_tint_whatever_the_picture() -> Result<(), Error> {
    let out = run(&params(|p| p.relief = 0.0), &varied(SIZE, SIZE))?;
    assert_close(&out, &TINT.repeat(SIZE * SIZE), 1e-6, "relief 0 at the horizon 0.5");
    Ok(())
}

#[test]
fn the_metal_runs_black_to_tint_to_white_along_the_environment() -> Result<(), Error> {
    // brightness rising down by 0.005 per row under a relief of 100 px: dy = 0.5 px per px, dx = 0, so t = 0.5 +/- 0.4 (0.9, 0.1)
    // (rows: rising downwards, then upwards; interior rows 1..SIZE - 1)
    let rows = [
        (true, 0.5, [0.96, 0.9, 0.84]), // t = 0.9 at horizon 0.5: shine 0.9, white 0.8 of the way from tint
        (false, 0.5, [0.16, 0.1, 0.04]), // t = 0.1: shine 0.1, tint x 0.2
        (true, 0.8, [0.9, 0.75, 0.6]),  // t = 0.9 at horizon 0.8: shine 0.75, white 0.5 of the way
        (false, 0.2, [0.4, 0.25, 0.1]), // t = 0.1 at horizon 0.2: shine 0.25, tint x 0.5
    ];
    for (down, horizon, expected) in rows {
        let out = run(&params(|p| p.horizon = horizon), &ramp(down, false))?;
        for row in 1..SIZE - 1 {
            assert_close(
                &at_row(&out, row),
                &expected,
                1e-5,
                &format!("down {down} horizon {horizon} row {row}"),
            );
        }
    }
    Ok(())
}

#[test]
fn a_slope_across_the_picture_does_not_tilt_the_mirror() -> Result<(), Error> {
    // the viewer sees the environment's height only: a surface rising sideways looks at the horizon, as a flat one
    for down in [true, false] {
        let out = run(&params(|_| {}), &ramp(down, true))?;
        assert_close(&out, &TINT.repeat(SIZE * SIZE), 1e-5, "rising sideways");
    }
    Ok(())
}

#[test]
fn a_steeper_normal_tilts_less_than_the_slope_alone() -> Result<(), Error> {
    // relief 400: dy = 2, so t = 0.5 + 2 / (1 + 4) = 0.9, not 0.5 + 2
    let out = run(&params(|p| p.relief = 400.0), &ramp(true, false))?;
    assert_close(&at_row(&out, 10), &chrome(0.9, 0.5), 1e-5, "relief 400");
    Ok(())
}

#[test]
fn the_picture_enters_by_its_brightness_alone() -> Result<(), Error> {
    // adding (0.7152, -0.2126, 0) x a leaves the Rec. 709 brightness (0.2126, 0.7152, 0.0722) unchanged
    let grey: Vec<f32> = varied(SIZE, SIZE)
        .chunks_exact(3)
        .flat_map(|p| [p[0] * 0.5 + 0.25; 3])
        .collect();
    let shifted: Vec<f32> = grey
        .chunks_exact(3)
        .enumerate()
        .flat_map(|(index, pixel)| {
            let amount = 0.15 * (index % 7) as f32 / 7.0;
            [pixel[0] + 0.7152 * amount, pixel[1] - 0.2126 * amount, pixel[2]]
        })
        .collect();
    assert_ne!(grey, shifted);
    let settings = params(|p| {
        p.smoothing = 1.5;
        p.relief = 30.0;
    });
    assert_close(
        &run(&settings, &shifted)?,
        &run(&settings, &grey)?,
        1e-4,
        "same brightness, other hues",
    );
    Ok(())
}

#[test]
fn smoothing_is_a_gaussian_blur_of_the_brightness() -> Result<(), Error> {
    let grey: Vec<f32> = varied(SIZE, SIZE).chunks_exact(3).flat_map(|p| [p[1]; 3]).collect();
    let blurred = run_picture(
        &gaussian_blur::Params {
            radius: 2.0,
            edge: EdgeOrConstant::Mirror,
            ..Default::default()
        },
        SIZE,
        SIZE,
        &grey,
    )?;
    let smoothed = run(&params(|p| (p.smoothing, p.edge) = (2.0, Edge::Mirror)), &grey)?;
    assert_close(
        &smoothed,
        &run(&params(|p| p.edge = Edge::Mirror), &blurred)?,
        1e-4,
        "smoothing 2",
    );
    assert!(
        smoothed != run(&params(|p| p.edge = Edge::Mirror), &grey)?,
        "smoothing changes the picture"
    );
    Ok(())
}

#[test]
fn the_edge_rule_decides_the_first_row_only() -> Result<(), Error> {
    // rising downwards by 0.005 per row; in row 0 the previous row is row 0 (reflect, nearest), row 1 (mirror) or the last
    // row, SIZE - 1 (wrap), so dy = 100 x (next - previous) / 2 is 0.25, 0, or the wrapped difference
    let wrapped = 100.0 * (RISE - RISE * (SIZE - 1) as f32) / 2.0;
    let rows = [
        (Edge::Reflect, 0.25),
        (Edge::Nearest, 0.25),
        (Edge::Mirror, 0.0),
        (Edge::Wrap, wrapped),
    ];
    for (edge, dy) in rows {
        let out = run(&params(|p| p.edge = edge), &ramp(true, false))?;
        assert_close(
            &at_row(&out, 0),
            &chrome(0.5 + dy / (1.0 + dy * dy), 0.5),
            1e-4,
            &format!("{edge:?}, row 0"),
        );
        assert_close(
            &at_row(&out, 5),
            &chrome(0.9, 0.5),
            1e-5,
            &format!("{edge:?}, row 5 is no edge"),
        );
    }
    Ok(())
}

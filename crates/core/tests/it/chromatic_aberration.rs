//! `chromatic_aberration`: red, green and blue read at slightly different scales about a centre. Checked on pictures that
//! are affine in the row and the column, where a read at a scaled place has a closed form, with the spectrum's weights
//! worked out by hand for small numbers of copies.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "small picture sizes: every count and index is far below 2^52; the f64 references narrowed to the f32 picture"
)]

use bullterrier_image::effects::chromatic_aberration::Params;
use bullterrier_image::{Edge, Error, Interpolation};

use crate::common::compare::first_difference_within;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;
use crate::common::sampling::warped;

const HEIGHT: usize = 21;
const WIDTH: usize = 33;
const TOLERANCE: f32 = 1e-5;

/// The value of the affine picture at (`row`, `column`): 0.1 to 0.9 over the picture, a different slope along each axis, so
/// a mix-up of the axes shows. Every channel is this.
fn affine(row: f64, column: f64) -> f64 {
    0.1 + 0.3 * row / (HEIGHT - 1) as f64 + 0.5 * column / (WIDTH - 1) as f64
}

fn affine_picture() -> Vec<f32> {
    (0..HEIGHT * WIDTH)
        .flat_map(|at| [affine((at / WIDTH) as f64, (at % WIDTH) as f64) as f32; 3])
        .collect()
}

/// The farthest corner's distance from the centre (`x`, `y`), in px.
fn farthest(x: f64, y: f64) -> f64 {
    (y.max(1.0 - y) * (HEIGHT - 1) as f64).hypot(x.max(1.0 - x) * (WIDTH - 1) as f64)
}

/// For each pixel whose value differs from the centre's (at `x`, `y`) by more than 0.05 and whose farthest read, at (1 + `s`)
/// times its offset from the centre, lies inside the picture: how far each channel (red, green, blue) lies from the
/// picture's own value, in units of the difference between the pixel's own value and the centre's. On an affine picture a
/// read at (1 - k) times the offset from the centre gives `own - k x (own - centre)`, so this gives k.
fn coefficients(out: &[f32], x: f64, y: f64, s: f64) -> Vec<[f64; 3]> {
    let (centre_row, centre_column) = (y * (HEIGHT - 1) as f64, x * (WIDTH - 1) as f64);
    let centre = affine(centre_row, centre_column);
    (0..HEIGHT * WIDTH)
        .filter_map(|at| {
            let (row, column) = ((at / WIDTH) as f64, (at % WIDTH) as f64);
            let own = affine(row, column);
            let (far_row, far_column) = (
                centre_row + (row - centre_row) * (1.0 + s),
                centre_column + (column - centre_column) * (1.0 + s),
            );
            let reads_inside =
                (0.0..=(HEIGHT - 1) as f64).contains(&far_row) && (0.0..=(WIDTH - 1) as f64).contains(&far_column);
            (reads_inside && (own - centre).abs() > 0.05)
                .then(|| [0, 1, 2].map(|channel| (own - f64::from(out[at * 3 + channel])) / (own - centre)))
        })
        .collect()
}

#[test]
fn no_amount_and_a_picture_of_one_pixel_leave_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(7, 9);
    let params = Params {
        amount: 0.0,
        ..Params::default()
    };
    assert_eq!(run_picture(&params, 7, 9, &picture)?, picture);
    let one = vec![0.2, 0.5, 0.9];
    assert_eq!(run_picture(&Params::default(), 1, 1, &one)?, one);
    Ok(())
}

#[test]
fn a_flat_picture_and_the_centre_pixel_stay_as_they_are() -> Result<(), Error> {
    let flat = vec![0.37_f32; HEIGHT * WIDTH * 3];
    let busy = varied(HEIGHT, WIDTH);
    for samples in [3, 4, 7, 32] {
        for interpolation in [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic] {
            for edge in [Edge::Reflect, Edge::Wrap] {
                let params = Params {
                    amount: 30.0,
                    samples,
                    x: 0.3,
                    y: 0.7,
                    interpolation,
                    edge,
                };
                let at = first_difference_within(&run_picture(&params, HEIGHT, WIDTH, &flat)?, &flat, TOLERANCE);
                assert_eq!(at, None, "flat, {params:?}: first differs at {at:?}");
                // the middle of an odd picture is a pixel; every copy reads it where it is
                let middle = Params {
                    x: 0.5,
                    y: 0.5,
                    ..params
                };
                let out = run_picture(&middle, HEIGHT, WIDTH, &busy)?;
                let at = (HEIGHT / 2 * WIDTH + WIDTH / 2) * 3;
                assert_eq!(
                    first_difference_within(&out[at..at + 3], &busy[at..at + 3], TOLERANCE),
                    None,
                    "centre, {middle:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn with_three_copies_green_is_the_picture_red_is_pulled_outward_and_blue_inward_in_proportion_to_the_distance()
-> Result<(), Error> {
    // on an affine picture a read at the place `(1 - s)` of the way from the centre to the pixel gives
    // own - s x (own - centre): red is the copy with s = amount / farthest, green s = 0, blue s = -amount / farthest. So
    // the shift is in proportion to the distance from the centre, nothing at the centre, the whole amount (in px) at the
    // farthest corner
    for (amount, x, y) in [(4.0, 0.5, 0.5), (9.0, 0.5, 0.5), (6.0, 0.2, 0.9), (2.0, 1.0, 0.0)] {
        let params = Params {
            amount,
            x: x as f32,
            y: y as f32,
            ..Params::default()
        };
        let s = f64::from(amount) / farthest(x, y);
        let out = run_picture(&params, HEIGHT, WIDTH, &affine_picture())?;
        // reads that fall beyond the border are mirrored, so only the pixels whose farthest read stays inside count
        let found = coefficients(&out, x, y, s);
        assert!(found.len() > 100, "{params:?}: only {} pixels checked", found.len());
        for [red, green, blue] in found {
            assert!(
                (red - s).abs() < 1e-3 && green.abs() < 1e-3 && (blue + s).abs() < 1e-3,
                "{params:?}: red {red}, green {green}, blue {blue}, expected {s}, 0, {}",
                -s
            );
        }
    }
    Ok(())
}

#[test]
fn a_shift_longer_than_the_way_to_the_farthest_corner_is_held_to_it() -> Result<(), Error> {
    // s is capped at 1: red reads the centre for every pixel, a flat picture of the centre's value
    let params = Params {
        amount: 100.0,
        ..Params::default()
    };
    assert!(farthest(0.5, 0.5) < 100.0);
    let out = run_picture(&params, HEIGHT, WIDTH, &affine_picture())?;
    let centre = affine(((HEIGHT - 1) / 2) as f64, ((WIDTH - 1) / 2) as f64) as f32;
    assert!(
        (0..HEIGHT * WIDTH).all(|at| (out[at * 3] - centre).abs() < TOLERANCE),
        "red is not the centre's value everywhere"
    );
    Ok(())
}

#[test]
fn more_copies_mix_the_fringe_by_the_spectrum_with_green_still_unmoved() -> Result<(), Error> {
    // red's weights over the copies by hand (red from the red and yellow copies, summing to 1): 4 copies 0.6, 0.4, 0, 0
    // whose shifts are s x (1, 1/3, -1/3, -1) give 0.6 + 0.4 / 3 = 11/15 of s; 5 copies 1/2, 1/2, 0, 0, 0 with shifts
    // s x (1, 1/2, 0, -1/2, -1) give 3/4. The spectrum is symmetric, so green is unmoved whatever the number of copies and blue is red's mirror
    for (samples, red) in [(3, 1.0), (4, 11.0 / 15.0), (5, 0.75)] {
        let params = Params {
            amount: 5.0,
            samples,
            ..Params::default()
        };
        let s = f64::from(params.amount) / farthest(0.5, 0.5);
        let out = run_picture(&params, HEIGHT, WIDTH, &affine_picture())?;
        let found = coefficients(&out, 0.5, 0.5, s);
        assert!(
            found.len() > 100,
            "{samples} copies: only {} pixels checked",
            found.len()
        );
        for [r, g, b] in found {
            assert!(
                (r - red * s).abs() < 2e-3 && g.abs() < 2e-3 && (b + red * s).abs() < 2e-3,
                "{samples} copies: red {r}, green {g}, blue {b}, expected {}, 0, {}",
                red * s,
                -red * s
            );
        }
    }
    // many copies: a smooth rainbow pulls red less far than three do, and still symmetric
    let many = Params {
        amount: 5.0,
        samples: 32,
        ..Params::default()
    };
    let s = f64::from(many.amount) / farthest(0.5, 0.5);
    let out = run_picture(&many, HEIGHT, WIDTH, &affine_picture())?;
    for [r, g, b] in coefficients(&out, 0.5, 0.5, s) {
        assert!(
            r > 0.3 * s && r < 0.99 * s && g.abs() < 2e-3 && (r + b).abs() < 2e-3,
            "{r} {g} {b}"
        );
    }
    Ok(())
}

#[test]
fn a_varied_picture_is_what_each_channel_read_at_its_scaled_place_gives() -> Result<(), Error> {
    // three copies, point by point from the definition: red is the picture read at (1 - s) x the offset from the centre, green
    // where it is, blue at (1 + s) x the offset; each channel taken from its own copy only
    let picture = varied(HEIGHT, WIDTH);
    for (interpolation, edge) in [
        (Interpolation::Nearest, Edge::Reflect),
        (Interpolation::Bilinear, Edge::Wrap),
        (Interpolation::Bicubic, Edge::Mirror),
        (Interpolation::Bilinear, Edge::Nearest),
    ] {
        let (x, y, amount) = (0.3_f32, 0.8_f32, 7.0_f32);
        let params = Params {
            amount,
            x,
            y,
            interpolation,
            edge,
            ..Params::default()
        };
        let s = f64::from(amount) / farthest(f64::from(x), f64::from(y));
        let centre = (f64::from(y) * (HEIGHT - 1) as f64, f64::from(x) * (WIDTH - 1) as f64);
        let copy = |scale: f64| {
            warped(&picture, HEIGHT, WIDTH, interpolation, (edge, edge), |row, column| {
                (
                    centre.0 + (row as f64 - centre.0) * scale,
                    centre.1 + (column as f64 - centre.1) * scale,
                )
            })
        };
        let (red, green, blue) = (copy(1.0 - s), copy(1.0), copy(1.0 + s));
        let expected: Vec<f32> = (0..HEIGHT * WIDTH)
            .flat_map(|at| [red[at * 3], green[at * 3 + 1], blue[at * 3 + 2]])
            .collect();
        let at = first_difference_within(&run_picture(&params, HEIGHT, WIDTH, &picture)?, &expected, TOLERANCE);
        assert_eq!(at, None, "{params:?}: first differs at {at:?}");
    }
    Ok(())
}

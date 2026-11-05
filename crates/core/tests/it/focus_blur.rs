//! `focus_blur`: each pixel blurred by how far its field value lies from the focus. The blurs of the comparisons are the
//! `gaussian_blur` effect, itself held to its definition in `gaussian_blur.rs`; the mixing between them is worked out by
//! hand.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "the pictures are a few dozen px: every count and index is small"
)]

use bullterrier_image::effects::{focus_blur, gaussian_blur};
use bullterrier_image::{Edge, EdgeOrConstant, Error};

use crate::common::compare::first_difference_within;
use crate::common::pictures::varied;
use crate::common::pixels::{run_picture, run_with_field};

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
/// The same four rules by name, as `gaussian_blur` takes them (written out, not mapped by the code under test).
const BLUR_EDGES: [EdgeOrConstant; 4] = [
    EdgeOrConstant::Reflect,
    EdgeOrConstant::Nearest,
    EdgeOrConstant::Mirror,
    EdgeOrConstant::Wrap,
];
const HEIGHT: usize = 24;
const WIDTH: usize = 30;

fn field_of(height: usize, width: usize, at: impl Fn(usize, usize) -> f32) -> Vec<f32> {
    (0..height * width)
        .map(|index| at(index / width, index % width))
        .collect()
}

fn flat(value: f32) -> Vec<f32> {
    vec![value; HEIGHT * WIDTH]
}

/// Two planes: `left` in the left half, `right` in the right half.
fn planes(left: f32, right: f32) -> Vec<f32> {
    field_of(HEIGHT, WIDTH, |_, column| if column < WIDTH / 2 { left } else { right })
}

fn run(params: &focus_blur::Params, field: &[f32], picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_with_field(params, "field", field, HEIGHT, WIDTH, picture)
}

fn gaussian(radius: f32, edge: EdgeOrConstant, truncate: f32, picture: &[f32]) -> Result<Vec<f32>, Error> {
    let params = gaussian_blur::Params {
        radius,
        edge,
        truncate,
        ..Default::default()
    };
    run_picture(&params, HEIGHT, WIDTH, picture)
}

/// The pixels of columns `from..to` as one list of channel values.
fn columns(picture: &[f32], from: usize, to: usize) -> Vec<f32> {
    (0..HEIGHT)
        .flat_map(|row| picture[(row * WIDTH + from) * 3..(row * WIDTH + to) * 3].to_vec())
        .collect()
}

#[test]
fn pixels_within_the_sharp_band_are_returned_exactly_and_the_rest_is_blurred() -> Result<(), Error> {
    // (field of the left half, of the right half, focus, width): the left half lies 0, 0.05, width / 2 and 0 away from the
    // focus, the right half more than a band's half-width and a little feather beyond it
    let rows = [
        (0.8, 0.0, 0.8, 0.15),
        (0.55, 0.0, 0.5, 0.2),
        (0.75, 0.0, 0.5, 0.5),
        (0.0, 1.0, 0.0, 0.0),
    ];
    let picture = varied(HEIGHT, WIDTH);
    for (left, right, focus, width) in rows {
        let params = focus_blur::Params {
            focus,
            width,
            ..Default::default()
        };
        let out = run(&params, &planes(left, right), &picture)?;
        assert_eq!(
            columns(&out, 0, WIDTH / 2),
            columns(&picture, 0, WIDTH / 2),
            "left {left}, focus {focus}, width {width}"
        );
        assert!(
            first_difference_within(
                &columns(&out, WIDTH / 2, WIDTH),
                &columns(&picture, WIDTH / 2, WIDTH),
                0.05
            )
            .is_some(),
            "left {left}, focus {focus}, width {width}"
        );
    }
    Ok(())
}

#[test]
fn a_field_a_full_feather_beyond_the_band_gives_the_gaussian_at_the_radius() -> Result<(), Error> {
    // focus 0.8, band 0.15 (0.075 each side), feather 0.4: 0.325 and below is a full feather beyond the band
    let picture = varied(HEIGHT, WIDTH);
    let rows = [
        (6.0, 6, 4.0),
        (3.0, 1, 4.0),
        (10.0, 16, 4.0),
        (5.0, 3, 1.0),
        (0.3, 4, 4.0),
    ];
    for (radius, levels, truncate) in rows {
        for (edge, blur_edge) in EDGES.into_iter().zip(BLUR_EDGES) {
            let params = focus_blur::Params {
                radius,
                levels,
                truncate,
                edge,
                ..Default::default()
            };
            let out = run(&params, &flat(0.1), &picture)?;
            let expected = gaussian(radius, blur_edge, truncate, &picture)?;
            assert_eq!(
                first_difference_within(&out, &expected, 2e-6),
                None,
                "radius {radius}, {levels} levels, truncate {truncate}, {edge:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn between_the_levels_a_pixel_mixes_the_two_copies_either_side_of_its_weight() -> Result<(), Error> {
    // focus 0.8, band 0.15: the field 0.8 - 0.075 - feather w is at weight w. Level k is the Gaussian of sigma
    // radius k / levels (the sharp picture for k = 0); the weight w sits at k + t of `levels`, so the pixel is
    // (1 - t) of copy k and t of copy k + 1
    let picture = varied(HEIGHT, WIDTH);
    let radius = 8.0_f32;
    for (levels, weight, feather) in [
        (1, 0.25, 0.4),
        (2, 0.25, 0.4),
        (4, 0.25, 0.4),
        (3, 0.25, 0.4),
        (3, 0.5, 0.2),
        (6, 0.9, 0.7),
        (16, 0.37, 0.05),
    ] {
        let params = focus_blur::Params {
            radius,
            levels,
            feather,
            ..Default::default()
        };
        let out = run(&params, &flat(0.8 - 0.075 - feather * weight), &picture)?;
        let at = weight * levels as f32;
        let (lower, t) = (at.floor() as i32, at.fract());
        let copy = |level: i32| -> Result<Vec<f32>, Error> {
            if level == 0 {
                return Ok(picture.clone());
            }
            gaussian(
                radius * level as f32 / levels as f32,
                EdgeOrConstant::Reflect,
                4.0,
                &picture,
            )
        };
        let (below, above) = (copy(lower)?, copy(lower + 1)?);
        let expected: Vec<f32> = below
            .iter()
            .zip(&above)
            .map(|(below, above)| (1.0 - t) * below + t * above)
            .collect();
        assert_eq!(
            first_difference_within(&out, &expected, 3e-5),
            None,
            "{levels} levels, weight {weight}, feather {feather}"
        );
    }
    Ok(())
}

#[test]
fn the_field_decides_which_pixels_are_blurred() -> Result<(), Error> {
    let picture = varied(HEIGHT, WIDTH);
    let params = focus_blur::Params {
        focus: 1.0,
        ..Default::default()
    };
    let left_sharp = run(&params, &planes(1.0, 0.0), &picture)?;
    let right_sharp = run(&params, &planes(0.0, 1.0), &picture)?;
    assert_eq!(columns(&left_sharp, 0, WIDTH / 2), columns(&picture, 0, WIDTH / 2));
    assert_eq!(
        columns(&right_sharp, WIDTH / 2, WIDTH),
        columns(&picture, WIDTH / 2, WIDTH)
    );
    assert!(
        first_difference_within(
            &columns(&left_sharp, WIDTH / 2, WIDTH),
            &columns(&picture, WIDTH / 2, WIDTH),
            0.05
        )
        .is_some()
    );
    assert!(
        first_difference_within(
            &columns(&right_sharp, 0, WIDTH / 2),
            &columns(&picture, 0, WIDTH / 2),
            0.05
        )
        .is_some()
    );
    // a field at the focus everywhere leaves the picture as it is
    assert_eq!(run(&params, &flat(1.0), &picture)?, picture);
    Ok(())
}

#[test]
fn radius_zero_returns_the_picture_whatever_the_field() -> Result<(), Error> {
    let picture = varied(HEIGHT, WIDTH);
    let params = focus_blur::Params {
        radius: 0.0,
        ..Default::default()
    };
    for field in [flat(0.0), planes(0.1, 0.9), flat(0.8)] {
        assert_eq!(run(&params, &field, &picture)?, picture);
    }
    Ok(())
}

#[test]
fn the_blur_stays_within_the_range_of_the_picture() -> Result<(), Error> {
    let picture: Vec<f32> = varied(HEIGHT, WIDTH).iter().map(|value| value * 0.5 + 0.3).collect();
    let ramp = field_of(HEIGHT, WIDTH, |_, column| 1.0 - column as f32 / (WIDTH - 1) as f32);
    for levels in [1, 3, 16] {
        let params = focus_blur::Params {
            radius: 20.0,
            levels,
            ..Default::default()
        };
        let out = run(&params, &ramp, &picture)?;
        let (low, high) = picture
            .iter()
            .fold((1.0_f32, 0.0_f32), |(low, high), &v| (low.min(v), high.max(v)));
        assert!(
            out.iter().all(|&value| (low..=high).contains(&value)),
            "{levels} levels"
        );
    }
    Ok(())
}

/// The standard deviation down column `column`, over the rows, of the first channel.
fn spread(picture: &[f32], column: usize) -> f32 {
    let values: Vec<f32> = (0..HEIGHT).map(|row| picture[(row * WIDTH + column) * 3]).collect();
    let mean = values.iter().sum::<f32>() / HEIGHT as f32;
    (values.iter().map(|value| (value - mean).powi(2)).sum::<f32>() / HEIGHT as f32).sqrt()
}

#[test]
fn the_blur_grows_with_the_distance_of_the_field_from_the_focus() -> Result<(), Error> {
    // stripes 2 px wide, the same in every column, on a field falling from 1 at the left to 0 at the right
    let stripes: Vec<f32> = (0..HEIGHT * WIDTH * 3)
        .map(|index| f32::from(u8::try_from(index / 3 / WIDTH / 2 % 2).unwrap_or(0)))
        .collect();
    let ramp = field_of(HEIGHT, WIDTH, |_, column| 1.0 - column as f32 / (WIDTH - 1) as f32);
    for focus in [1.0_f32, 0.5, 0.2] {
        let params = focus_blur::Params {
            focus,
            width: 0.1,
            feather: 0.5,
            radius: 3.0,
            ..Default::default()
        };
        let out = run(&params, &ramp, &stripes)?;
        let contrast: Vec<f32> = (0..WIDTH).map(|column| spread(&out, column)).collect();
        let centre = ((1.0 - focus) * (WIDTH - 1) as f32).round() as usize;
        assert!(
            contrast[centre..].windows(2).all(|pair| pair[1] <= pair[0] + 1e-6),
            "{focus}"
        );
        assert!(
            contrast[..=centre].windows(2).all(|pair| pair[0] <= pair[1] + 1e-6),
            "{focus}"
        );
        assert!(contrast[0].min(contrast[WIDTH - 1]) < 0.5 * contrast[centre], "{focus}");
    }
    Ok(())
}

#[test]
fn single_pixels_lines_and_pictures_smaller_than_the_blur_are_blurred_without_leaving_the_range() -> Result<(), Error> {
    for (height, width) in [(1, 1), (1, 9), (9, 1), (3, 2)] {
        let picture = varied(height, width);
        let field = field_of(height, width, |row, column| ((row + column) % 3) as f32 / 2.0);
        for edge in EDGES {
            let params = focus_blur::Params {
                radius: 20.0,
                edge,
                ..Default::default()
            };
            let out = run_with_field(&params, "field", &field, height, width, &picture)?;
            let (low, high) = picture
                .iter()
                .fold((1.0_f32, 0.0_f32), |(low, high), &v| (low.min(v), high.max(v)));
            assert!(
                out.iter().all(|&value| (low..=high).contains(&value)),
                "{height} x {width}, {edge:?}"
            );
        }
    }
    Ok(())
}

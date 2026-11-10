//! `rim_light`: light along the edge of a shape that faces it. A square on a grey picture, lit white with no smoothing:
//! along its middle row the signed distance runs 3, 2, 1, -1, -2, ... across the left edge, so the edge's outward
//! direction (the central difference of the distance) points straight left, and the light's strength at each depth is
//! worked by hand from the band (1 - (depth - 1) / width)^falloff, screened over the grey.

use bullterrier_image::Error;
use bullterrier_image::effects::rim_light;

use crate::common::pixels::run_with_field;

const SIZE: usize = 21;
/// The square's rows and columns: 4..17.
const SQUARE: std::ops::Range<usize> = 4..17;
const GREY: f32 = 0.2;
const MIDDLE: usize = 10;

fn square(level: f32) -> Vec<f32> {
    (0..SIZE * SIZE)
        .map(|at| {
            if SQUARE.contains(&(at / SIZE)) && SQUARE.contains(&(at % SIZE)) {
                level
            } else {
                0.0
            }
        })
        .collect()
}

fn params(angle: f32) -> rim_light::Params {
    rim_light::Params {
        angle,
        color: bullterrier_image::params::Color([1.0; 3]),
        width: 4.0,
        falloff: 1.0,
        smoothing: 0.0,
        ..Default::default()
    }
}

/// The light (the share of the way from grey to white) of every pixel, row after row.
fn light(params: &rim_light::Params, field: &[f32]) -> Result<Vec<f32>, Error> {
    let out = run_with_field(params, "field", field, SIZE, SIZE, &vec![GREY; SIZE * SIZE * 3])?;
    Ok(out
        .chunks_exact(3)
        .map(|pixel| (pixel[0] - GREY) / (1.0 - GREY))
        .collect())
}

fn near(got: f32, wanted: f32) -> bool {
    (got - wanted).abs() < 1e-5
}

#[test]
fn the_edge_facing_the_light_is_lit_by_the_band_at_each_depth_and_nothing_else_is() -> Result<(), Error> {
    // light from the left (180 degrees): the left edge faces it head-on
    let lit = light(&params(180.0), &square(1.0))?;
    let row = &lit[MIDDLE * SIZE..][..SIZE];
    // depths 1 to 4 at columns 4 to 7: the band of width 4 falls as 1, 0.75, 0.5, 0.25; at depth 5 it is spent
    for (column, wanted) in [(4, 1.0), (5, 0.75), (6, 0.5), (7, 0.25), (8, 0.0)] {
        assert!(near(row[column], wanted), "column {column}: {}", row[column]);
    }
    // the right edge faces away, the top and bottom edges side-on (in their middle), and outside the shape is never lit
    assert!(near(row[16], 0.0) && near(lit[4 * SIZE + MIDDLE], 0.0) && near(lit[16 * SIZE + MIDDLE], 0.0));
    for (at, value) in lit.iter().enumerate() {
        let outside = !(SQUARE.contains(&(at / SIZE)) && SQUARE.contains(&(at % SIZE)));
        assert!(
            !outside || *value == 0.0,
            "pixel {at}, outside the shape, is lit {value}"
        );
    }
    Ok(())
}

#[test]
fn the_angle_turns_clockwise_from_the_right_and_an_edge_at_an_angle_to_the_light_gets_its_cosine() -> Result<(), Error>
{
    // (angle, the outermost pixel of the middle of the edge lit fully, its cosine to the light at 45 degrees)
    let cases = [
        (0.0, MIDDLE * SIZE + 16, MIDDLE * SIZE + 4), // from the right: the right edge
        (90.0, 16 * SIZE + MIDDLE, 4 * SIZE + MIDDLE), // from below: the bottom edge
        (270.0, 4 * SIZE + MIDDLE, 16 * SIZE + MIDDLE), // from above: the top edge
    ];
    for (angle, lit_pixel, dark_pixel) in cases {
        let lit = light(&params(angle), &square(1.0))?;
        assert!(near(lit[lit_pixel], 1.0) && near(lit[dark_pixel], 0.0), "angle {angle}");
    }
    // from the upper left (225 degrees) the left and the top edge both face it at 45 degrees
    let lit = light(&params(225.0), &square(1.0))?;
    let half = std::f32::consts::FRAC_1_SQRT_2;
    assert!(near(lit[MIDDLE * SIZE + 4], half) && near(lit[4 * SIZE + MIDDLE], half));
    Ok(())
}

#[test]
fn amount_and_a_soft_shape_scale_the_light() -> Result<(), Error> {
    let soft = light(&params(180.0), &square(0.8))?;
    let half = light(
        &rim_light::Params {
            amount: 0.5,
            ..params(180.0)
        },
        &square(1.0),
    )?;
    let at = MIDDLE * SIZE + 4;
    assert!(near(soft[at], 0.8) && near(half[at], 0.5), "{} {}", soft[at], half[at]);
    Ok(())
}

#[test]
fn a_shape_with_no_edge_lights_nothing() -> Result<(), Error> {
    for level in [0.0, 1.0] {
        let lit = light(&params(180.0), &vec![level; SIZE * SIZE])?;
        assert!(lit.iter().all(|value| *value == 0.0), "a field of {level}");
    }
    Ok(())
}

//! `drop_shadow`: copies of a shape behind it. On a flat grey picture with hard copies (no blur) a pixel a copy covers
//! fully is that copy's colour, worked by hand: each copy takes its opacity of what the nearer copies left clear, and
//! the shape itself is never changed.

use bullterrier_image::effects::drop_shadow;
use bullterrier_image::params::Color;
use bullterrier_image::{Edge, Error};

use crate::common::pixels::run_with_field;

const GREY: f32 = 0.8;
const RED: Color = Color([1.0, 0.0, 0.0]);
const BLUE: Color = Color([0.0, 0.0, 1.0]);

fn hard(offset: f32, angle: f32) -> drop_shadow::Params {
    drop_shadow::Params {
        offset,
        angle,
        blur: 0.0,
        ..Default::default()
    }
}

/// The pixels of `params` laid over a grey picture of `height` x `width` with the shape `field`.
fn run(params: &drop_shadow::Params, field: &[f32], height: usize, width: usize) -> Result<Vec<[f32; 3]>, Error> {
    let out = run_with_field(params, "field", field, height, width, &vec![GREY; height * width * 3])?;
    Ok(out.as_chunks::<3>().0.to_vec())
}

/// A shape of the pixels `(row, column)` in a picture `width` wide.
fn pixels_of(height: usize, width: usize, inside: impl Fn(usize, usize) -> bool) -> Vec<f32> {
    (0..height * width)
        .map(|at| f32::from(u8::from(inside(at / width, at % width))))
        .collect()
}

fn near(got: [f32; 3], wanted: [f32; 3]) -> bool {
    got.iter().zip(wanted).all(|(a, b)| (a - b).abs() < 1e-5)
}

/// Where a copy lands, (row, column), and the share of it there.
type Landing = ((usize, usize), f32);

#[test]
fn a_copy_lands_offset_away_along_the_angle_and_nowhere_else() -> Result<(), Error> {
    let lone = pixels_of(32, 32, |row, column| (row, column) == (10, 12));
    // (offset, angle, where the black copy lands with what share of it)
    let cases: [(f32, f32, &[Landing]); 6] = [
        (5.0, 0.0, &[((10, 17), 1.0)]),
        (5.0, 90.0, &[((15, 12), 1.0)]),
        (5.0, 180.0, &[((10, 7), 1.0)]),
        (5.0, 270.0, &[((5, 12), 1.0)]),
        (3.0 * std::f32::consts::SQRT_2, 45.0, &[((13, 15), 1.0)]),
        // half a pixel down shares the copy between the shape's own pixel (where it is hidden) and the one below
        (0.5, 90.0, &[((11, 12), 0.5)]),
    ];
    for (offset, angle, lands) in cases {
        let out = run(&hard(offset, angle), &lone, 32, 32)?;
        for (at, pixel) in out.iter().enumerate() {
            let share = lands
                .iter()
                .find(|(place, _)| *place == (at / 32, at % 32))
                .map_or(0.0, |(_, share)| *share);
            assert!(
                near(*pixel, [GREY * (1.0 - share); 3]),
                "offset {offset}, angle {angle}: pixel {at} is {pixel:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn the_shape_is_never_changed_and_with_no_offset_and_no_blur_nothing_is() -> Result<(), Error> {
    let disc = pixels_of(24, 24, |row, column| {
        (row.abs_diff(12).pow(2) + column.abs_diff(12).pow(2)) <= 25
    });
    let settings = [
        drop_shadow::Params::default(),
        drop_shadow::Params {
            copies: 5,
            offset: 6.0,
            color: RED,
            ..Default::default()
        },
        hard(0.0, 30.0),
    ];
    for (number, params) in settings.iter().enumerate() {
        let out = run(params, &disc, 24, 24)?;
        for (pixel, inside) in out.iter().zip(&disc) {
            assert!(*inside == 0.0 || *pixel == [GREY; 3], "setting {number}: {pixel:?}");
        }
    }
    let hidden = run(&hard(0.0, 30.0), &disc, 24, 24)?;
    assert!(hidden.iter().all(|pixel| *pixel == [GREY; 3]));
    Ok(())
}

#[test]
fn a_blurred_copy_spreads_the_shape_and_keeps_its_amount() -> Result<(), Error> {
    // one black pixel blurred by a sigma of 2 px: what it takes from the grey sums to one pixel's worth, about its middle
    let lone = pixels_of(41, 41, |row, column| (row, column) == (20, 14));
    let params = drop_shadow::Params {
        offset: 6.0,
        angle: 0.0,
        blur: 2.0,
        ..Default::default()
    };
    let out = run(&params, &lone, 41, 41)?;
    let taken: Vec<f32> = out.iter().map(|pixel| (GREY - pixel[0]) / GREY).collect();
    let total: f32 = taken.iter().sum::<f32>() - taken[20 * 41 + 14]; // the shape's own pixel shows none
    assert!((total - 1.0).abs() < 1e-3, "{total}");
    let middle = 20 * 41 + 20;
    assert!(taken[middle] > taken[middle + 1] && (taken[middle + 1] - taken[middle - 1]).abs() < 1e-6);
    Ok(())
}

#[test]
fn copies_run_from_color_to_end_color_each_taking_its_opacity_of_what_the_nearer_left_clear() -> Result<(), Error> {
    // a block 4 columns wide; two copies 2 and 4 columns to the right: columns 14 and 15 under both, 16 and 17 under the far
    let block = pixels_of(12, 40, |row, column| {
        (4..8).contains(&row) && (10..14).contains(&column)
    });
    let stack = |fade| drop_shadow::Params {
        copies: 2,
        copy_opacity: 0.5,
        fade,
        color: RED,
        end_color: BLUE,
        ..hard(2.0, 0.0)
    };
    let out = run(&stack(0.0), &block, 12, 40)?;
    // under both: red takes half, blue half of the rest, a quarter of the grey is left
    assert!(near(out[5 * 40 + 14], [0.7, 0.2, 0.45]) && near(out[5 * 40 + 15], [0.7, 0.2, 0.45]));
    assert!(near(out[5 * 40 + 16], [0.4, 0.4, 0.9]) && near(out[5 * 40 + 17], [0.4, 0.4, 0.9]));
    // a fade of 1: the farthest copy vanishes
    let faded = run(&stack(1.0), &block, 12, 40)?;
    assert!(near(faded[5 * 40 + 14], [0.9, 0.4, 0.4]) && near(faded[5 * 40 + 16], [GREY; 3]));
    Ok(())
}

#[test]
fn a_shape_cut_off_by_the_border_casts_its_copy_from_what_the_edge_rule_puts_beyond_it() -> Result<(), Error> {
    // the shape is the first column; its copy 3 columns to the right reads columns -3, -2, -1 beyond the border at 1, 2, 3
    let first_column = pixels_of(8, 12, |row, column| (2..6).contains(&row) && column == 0);
    for (edge, dark) in [
        (Edge::Nearest, [1, 2, 3].as_slice()),
        (Edge::Reflect, &[2, 3]),
        (Edge::Mirror, &[3]),
        (Edge::Wrap, &[3]),
    ] {
        let out = run(&drop_shadow::Params { edge, ..hard(3.0, 0.0) }, &first_column, 8, 12)?;
        let row: Vec<bool> = out[3 * 12..][..12].iter().map(|pixel| pixel[0] < 0.5).collect();
        let wanted: Vec<bool> = (0..12).map(|column| dark.contains(&column)).collect();
        assert_eq!(row, wanted, "{edge:?}");
    }
    Ok(())
}

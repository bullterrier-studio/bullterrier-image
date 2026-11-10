//! `defringe`: a square subject (green) on a white backdrop, its outermost ring of pixels pink (a matte's fringe). The
//! signed distance of every ring pixel to the backdrop is 1 (a corner too: its nearest backdrop pixel is beside it), so
//! with a band of `width` the ring takes `amount * (1 - 1 / width)` of the interior colour, worked by hand.

use bullterrier_image::Error;
use bullterrier_image::effects::defringe;

use crate::common::pixels::run_with_field;

const SIZE: usize = 15;
/// The subject's rows and columns: 3..12.
const SUBJECT: std::ops::Range<usize> = 3..12;
const WHITE: [f32; 3] = [1.0; 3];
const PINK: [f32; 3] = [1.0, 0.4, 0.7];
const GREEN: [f32; 3] = [0.1, 0.6, 0.2];

fn inside(row: usize, column: usize) -> bool {
    SUBJECT.contains(&row) && SUBJECT.contains(&column)
}

fn ring(row: usize, column: usize) -> bool {
    inside(row, column)
        && !(inside(row - 1, column) && inside(row + 1, column) && inside(row, column - 1) && inside(row, column + 1))
}

fn field() -> Vec<f32> {
    (0..SIZE * SIZE)
        .map(|at| f32::from(u8::from(inside(at / SIZE, at % SIZE))))
        .collect()
}

fn photo() -> Vec<[f32; 3]> {
    (0..SIZE * SIZE)
        .map(|at| match (inside(at / SIZE, at % SIZE), ring(at / SIZE, at % SIZE)) {
            (false, _) => WHITE,
            (true, true) => PINK,
            (true, false) => GREEN,
        })
        .collect()
}

fn run(params: &defringe::Params, field: &[f32], photo: &[[f32; 3]]) -> Result<Vec<[f32; 3]>, Error> {
    let values: Vec<f32> = photo.iter().flatten().copied().collect();
    let out = run_with_field(params, "field", field, SIZE, SIZE, &values)?;
    Ok(out.as_chunks::<3>().0.to_vec())
}

fn params(width: f32, amount: f32) -> defringe::Params {
    defringe::Params {
        width,
        amount,
        ..Default::default()
    }
}

fn near(got: [f32; 3], wanted: [f32; 3]) -> bool {
    got.iter().zip(wanted).all(|(a, b)| (a - b).abs() < 1e-5)
}

fn mix(from: [f32; 3], to: [f32; 3], share: f32) -> [f32; 3] {
    [0, 1, 2].map(|channel| from[channel] + (to[channel] - from[channel]) * share)
}

#[test]
fn the_ring_takes_the_interior_colour_by_depth_and_nothing_else_changes() -> Result<(), Error> {
    for (width, amount) in [(2.0, 1.0), (4.0, 1.0), (4.0, 0.5)] {
        let before = photo();
        let after = run(&params(width, amount), &field(), &before)?;
        for (at, (got, was)) in after.iter().zip(&before).enumerate() {
            let (row, column) = (at / SIZE, at % SIZE);
            let wanted = if ring(row, column) {
                mix(PINK, GREEN, amount * (1.0 - 1.0 / width)) // depth 1
            } else {
                *was // the backdrop, and the interior, which is at least 2 deep (and its own colour at any depth)
            };
            assert!(
                near(*got, wanted),
                "width {width} amount {amount}, pixel ({row}, {column}): {got:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn pixels_as_deep_as_the_width_or_deeper_are_exactly_as_they_were() -> Result<(), Error> {
    // a second, grey ring (depth 2) under the pink one: with width 2 it is out of the band
    let mut before = photo();
    for (at, pixel) in before.iter_mut().enumerate() {
        let (row, column) = (at / SIZE, at % SIZE);
        if inside(row, column) && !ring(row, column) && !(SUBJECT.start + 2..SUBJECT.end - 2).contains(&row)
            || inside(row, column) && !ring(row, column) && !(SUBJECT.start + 2..SUBJECT.end - 2).contains(&column)
        {
            *pixel = [0.5; 3];
        }
    }
    let after = run(&params(2.0, 1.0), &field(), &before)?;
    for (at, (got, was)) in after.iter().zip(&before).enumerate() {
        let (row, column) = (at / SIZE, at % SIZE);
        if !ring(row, column) {
            assert_eq!(got, was, "pixel ({row}, {column})");
        }
    }
    Ok(())
}

#[test]
fn nothing_changes_with_no_amount_no_width_no_interior_or_no_edge() -> Result<(), Error> {
    let before = photo();
    let thin: Vec<f32> = (0..SIZE * SIZE)
        .map(|at| f32::from(u8::from((6..9).contains(&(at / SIZE)) && (6..9).contains(&(at % SIZE)))))
        .collect();
    let cases = [
        (params(3.0, 0.0), field()),
        (params(0.0, 1.0), field()),
        (params(3.0, 1.0), thin), // 3 x 3: no pixel is deeper than 2
        (params(3.0, 1.0), vec![0.0; SIZE * SIZE]),
        (params(3.0, 1.0), vec![1.0; SIZE * SIZE]),
    ];
    for (index, (params, field)) in cases.iter().enumerate() {
        assert_eq!(run(params, field, &before)?, before, "case {index}");
    }
    Ok(())
}

#[test]
fn the_field_threshold_decides_which_pixels_are_inside() -> Result<(), Error> {
    // the square covers 0.4 of each pixel: it is the subject at a threshold of 0.4 or less, nothing at 0.5
    let soft: Vec<f32> = field().iter().map(|value| value * 0.4).collect();
    let before = photo();
    let unchanged = run(&params(4.0, 1.0), &soft, &before)?;
    assert_eq!(unchanged, before);
    let lowered = defringe::Params {
        field_threshold: 0.4,
        ..params(4.0, 1.0)
    };
    assert_ne!(run(&lowered, &soft, &before)?, before);
    Ok(())
}

#[test]
fn the_result_stays_in_range_on_any_picture() -> Result<(), Error> {
    let noisy: Vec<[f32; 3]> = (0..SIZE * SIZE)
        .map(|at| [0, 1, 2].map(|c| f32::from(u8::try_from((at * 7 + c * 13) % 11).unwrap_or(0)) / 10.0))
        .collect();
    for width in [1.0, 3.0, 6.0] {
        for pixel in run(&params(width, 1.0), &field(), &noisy)? {
            assert!(pixel.iter().all(|c| (0.0..=1.0).contains(c)), "{pixel:?}");
        }
    }
    Ok(())
}

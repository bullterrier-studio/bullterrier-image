//! `light_wrap`: a square subject (grey) on a flat backdrop colour. The backdrop's blurred colour is that colour
//! everywhere it can be seen from, so a subject pixel at depth d (1 on the outermost ring, corners too) is the screen of
//! the colour at weight `amount * (1 - (d - 1) / width)`: `base + weight * colour * (1 - base)`, worked by hand.

use bullterrier_image::Error;
use bullterrier_image::effects::light_wrap;

use crate::common::pixels::run_with_field;

const SIZE: usize = 15;
/// The subject's rows and columns: 3..12; its depth is 1 on the outer ring, 2 inside it, 3, then 4 at the centre.
const SUBJECT: std::ops::Range<usize> = 3..12;
const SUBJECT_COLOUR: [f32; 3] = [0.2, 0.3, 0.4];
const BACKDROP: [f32; 3] = [0.8, 0.5, 0.0];

fn inside(row: usize, column: usize) -> bool {
    SUBJECT.contains(&row) && SUBJECT.contains(&column)
}

/// 1 for the outer ring up to 4 at the centre of the 9 x 9 square, 0 outside.
fn depth(row: usize, column: usize) -> f32 {
    if !inside(row, column) {
        return 0.0;
    }
    let edge = |at: usize| (at - SUBJECT.start).min(SUBJECT.end - 1 - at);
    f32::from(u8::try_from(edge(row).min(edge(column)) + 1).unwrap_or(u8::MAX))
}

fn field() -> Vec<f32> {
    (0..SIZE * SIZE)
        .map(|at| f32::from(u8::from(inside(at / SIZE, at % SIZE))))
        .collect()
}

fn photo(backdrop: [f32; 3]) -> Vec<[f32; 3]> {
    (0..SIZE * SIZE)
        .map(|at| {
            if inside(at / SIZE, at % SIZE) {
                SUBJECT_COLOUR
            } else {
                backdrop
            }
        })
        .collect()
}

fn run(params: &light_wrap::Params, field: &[f32], photo: &[[f32; 3]]) -> Result<Vec<[f32; 3]>, Error> {
    let values: Vec<f32> = photo.iter().flatten().copied().collect();
    let out = run_with_field(params, "field", field, SIZE, SIZE, &values)?;
    Ok(out.as_chunks::<3>().0.to_vec())
}

fn params(width: f32, amount: f32) -> light_wrap::Params {
    light_wrap::Params {
        width,
        amount,
        blur: 2.0,
        ..Default::default()
    }
}

fn near(got: [f32; 3], wanted: [f32; 3]) -> bool {
    got.iter().zip(wanted).all(|(a, b)| (a - b).abs() < 1e-5)
}

#[test]
fn the_backdrop_colour_is_screened_over_the_edge_by_depth_and_nothing_else_changes() -> Result<(), Error> {
    for (width, amount) in [(4.0, 0.7), (3.0, 1.0), (4.0, 0.3)] {
        let before = photo(BACKDROP);
        let after = run(&params(width, amount), &field(), &before)?;
        for (at, (got, was)) in after.iter().zip(&before).enumerate() {
            let (row, column) = (at / SIZE, at % SIZE);
            let d = depth(row, column);
            let wanted = if d == 0.0 || d > width {
                *was // the backdrop, and the subject deeper than the width: exactly as it was
            } else {
                let weight = amount * (1.0 - (d - 1.0) / width); // d = 1: amount
                [0, 1, 2].map(|c| SUBJECT_COLOUR[c] + weight * BACKDROP[c] * (1.0 - SUBJECT_COLOUR[c]))
            };
            if d == 0.0 || d > width {
                assert_eq!(got, &wanted, "width {width} amount {amount}, pixel ({row}, {column})");
            } else {
                assert!(
                    near(*got, wanted),
                    "width {width} amount {amount}, pixel ({row}, {column}): {got:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_width_of_one_screens_the_backdrop_over_the_outermost_ring_at_full_amount() -> Result<(), Error> {
    // d = 1 = width: weight 1 * (1 - 0 / 1) = 1; the ring is base + colour * (1 - base), and d = 2 is untouched
    let before = photo(BACKDROP);
    let after = run(&params(1.0, 1.0), &field(), &before)?;
    for (at, (got, was)) in after.iter().zip(&before).enumerate() {
        let (row, column) = (at / SIZE, at % SIZE);
        if depth(row, column) == 1.0 {
            let wanted = [0, 1, 2].map(|c| SUBJECT_COLOUR[c] + BACKDROP[c] * (1.0 - SUBJECT_COLOUR[c]));
            assert!(near(*got, wanted), "ring pixel ({row}, {column}): {got:?}");
        } else {
            assert_eq!(got, was, "pixel ({row}, {column})");
        }
    }
    Ok(())
}

#[test]
fn a_black_backdrop_adds_no_light_and_the_subject_never_darkens() -> Result<(), Error> {
    let black = photo([0.0; 3]);
    assert_eq!(run(&params(4.0, 1.0), &field(), &black)?, black);
    let bright = photo([1.0, 0.9, 0.2]);
    for (after, before) in run(&params(6.0, 1.0), &field(), &bright)?.iter().zip(&bright) {
        assert!(
            (0..3).all(|c| after[c] >= before[c] && after[c] <= 1.0),
            "{after:?} from {before:?}"
        );
    }
    Ok(())
}

#[test]
fn nothing_changes_with_no_amount_no_width_no_backdrop_or_no_subject() -> Result<(), Error> {
    let before = photo(BACKDROP);
    let cases = [
        (params(4.0, 0.0), field()),
        (params(0.0, 1.0), field()),
        (params(4.0, 1.0), vec![1.0; SIZE * SIZE]), // everything is subject: no backdrop to wrap
        (params(4.0, 1.0), vec![0.0; SIZE * SIZE]), // no subject: nothing to wrap onto
    ];
    for (index, (params, field)) in cases.iter().enumerate() {
        assert_eq!(run(params, field, &before)?, before, "case {index}");
    }
    Ok(())
}

#[test]
fn the_light_is_only_the_backdrops_not_the_subjects_and_blur_softens_its_detail() -> Result<(), Error> {
    // a backdrop red on the left half and blue on the right: a sharp wrap gives the left edge red and the right blue, a
    // blur wide enough to mix both gives them a share of the other
    let split: Vec<[f32; 3]> = (0..SIZE * SIZE)
        .map(|at| match (inside(at / SIZE, at % SIZE), at % SIZE < SIZE / 2) {
            (true, _) => SUBJECT_COLOUR,
            (false, true) => [1.0, 0.0, 0.0],
            (false, false) => [0.0, 0.0, 1.0],
        })
        .collect();
    let pixel_at = |blur: f32| -> Result<[f32; 3], Error> {
        let p = light_wrap::Params {
            blur,
            ..params(4.0, 1.0)
        };
        Ok(run(&p, &field(), &split)?[7 * SIZE + 3]) // the middle of the left edge
    };
    let (sharp, soft) = (pixel_at(1.0)?, pixel_at(8.0)?);
    assert!(sharp[2] - SUBJECT_COLOUR[2] < 1e-3, "blue on the red side: {sharp:?}");
    assert!(
        soft[2] - SUBJECT_COLOUR[2] > 0.05,
        "no blue mixed in by a wide blur: {soft:?}"
    );
    Ok(())
}

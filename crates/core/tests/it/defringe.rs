//! `defringe`: a square subject (green) on a white backdrop, its outermost ring of pixels pink (a matte's fringe). The
//! signed distance of every ring pixel to the backdrop is 1 (a corner too: its nearest backdrop pixel is beside it). A
//! pixel at depth d takes `amount * (1 - (d - 1) / width)` of the interior colour for d up to `width`, none deeper, so
//! the ring (d = 1) takes `amount`, worked by hand.

use bullterrier_image::Error;
use bullterrier_image::effects::defringe;

use crate::common::pixels::run_with_field;

const SIZE: usize = 15;
/// The subject's rows and columns: 3..12.
const SUBJECT: std::ops::Range<usize> = 3..12;
const WHITE: [f32; 3] = [1.0; 3];
const PINK: [f32; 3] = [1.0, 0.4, 0.7];
const GREEN: [f32; 3] = [0.1, 0.6, 0.2];
const GREY: [f32; 3] = [0.5; 3];

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
                mix(PINK, GREEN, amount) // depth 1: 1 - (1 - 1) / width = 1
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

/// The 9 x 9 square with a grey ring (depth 2) under the pink one (depth 1); the core (depth 3 and more) is green.
fn three_rings() -> Vec<[f32; 3]> {
    let mut before = photo();
    for (at, pixel) in before.iter_mut().enumerate() {
        let (row, column) = (at / SIZE, at % SIZE);
        let core = |at: usize| (SUBJECT.start + 2..SUBJECT.end - 2).contains(&at);
        if inside(row, column) && !ring(row, column) && !(core(row) && core(column)) {
            *pixel = GREY;
        }
    }
    before
}

#[test]
fn a_width_of_one_recolours_the_outermost_ring_fully_and_nothing_deeper() -> Result<(), Error> {
    // d = 1 = width: 1 - (1 - 1) / 1 = 1, so the pink ring becomes the interior colour (here the mean of the grey and
    // green pixels deeper than 1, which is neither pink); d = 2 = width + 1 is not in the band
    let before = three_rings();
    let after = run(&params(1.0, 1.0), &field(), &before)?;
    for (at, (got, was)) in after.iter().zip(&before).enumerate() {
        let (row, column) = (at / SIZE, at % SIZE);
        if ring(row, column) {
            assert!(
                !near(*got, PINK) && got[1] > 0.4,
                "ring pixel ({row}, {column}) unchanged: {got:?}"
            );
        } else {
            assert_eq!(got, was, "pixel ({row}, {column})");
        }
    }
    Ok(())
}

#[test]
fn the_band_is_width_rings_and_a_pixel_just_beyond_it_is_exactly_as_it_was() -> Result<(), Error> {
    // width 2, amount 1: d = 1 takes 1 - 0 / 2 = 1 of the interior colour (the green core, depth > 2), d = 2 takes
    // 1 - 1 / 2 = 1/2, d = 3 is out of the band
    let before = three_rings();
    let after = run(&params(2.0, 1.0), &field(), &before)?;
    for (at, (got, was)) in after.iter().zip(&before).enumerate() {
        let (row, column) = (at / SIZE, at % SIZE);
        if ring(row, column) {
            assert!(near(*got, GREEN), "depth 1, pixel ({row}, {column}): {got:?}");
        } else if *was == GREY {
            assert!(
                near(*got, mix(GREY, GREEN, 0.5)),
                "depth 2, pixel ({row}, {column}): {got:?}"
            );
        } else {
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

#[test]
#[expect(clippy::cast_precision_loss, reason = "small indices")]
fn the_interior_colour_is_a_gaussian_mean_of_sigma_half_the_width() -> Result<(), Error> {
    // A subject of columns 10..120 over every row of a 101 x 140 picture whose red grows along the columns. At width 8
    // its interior is the columns 18 on (depth over 8), and the edge pixel of row 50, column 10, takes all of the interior
    // colour: the mean of the red of columns 18 to 26 (the blur reaches 4 sigma = 16 columns, rounded half up, from
    // column 10) weighted by exp(-d^2 / (2 sigma^2)), d the distance from column 10, sigma = 8 / 2 = 4. The rows weigh
    // alike, being all interior, and drop out.
    let (rows, columns, width) = (101_usize, 140_usize, 8.0_f32);
    let sigma = f64::from(width) / 2.0;
    let field: Vec<f32> = (0..rows * columns)
        .map(|at| f32::from(u8::from((10..120).contains(&(at % columns)))))
        .collect();
    let picture: Vec<f32> = (0..rows * columns * 3)
        .map(|at| {
            if at % 3 == 0 {
                ((at / 3) % columns) as f32 / 140.0
            } else {
                0.5
            }
        })
        .collect();
    let out = run_with_field(&params(width, 1.0), "field", &field, rows, columns, &picture)?;
    let (mut sum, mut weights) = (0.0_f64, 0.0_f64);
    for column in 18..=26_u32 {
        let weight = (-f64::from((column - 10).pow(2)) / (2.0 * sigma * sigma)).exp();
        sum += weight * f64::from(column) / 140.0;
        weights += weight;
    }
    let got = f64::from(out[(50 * columns + 10) * 3]);
    assert!(
        (got - sum / weights).abs() < 1e-4,
        "got {got}, wanted {}",
        sum / weights
    );
    Ok(())
}

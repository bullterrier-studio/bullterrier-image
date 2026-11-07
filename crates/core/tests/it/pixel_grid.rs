//! `pixel_grid`: the stripes and gaps of a cell worked out by hand, what `amount` and `keep_brightness` do to them, and the
//! defaults.

use bullterrier_image::Error;
use bullterrier_image::effects::pixel_grid;

use crate::common::compare::assert_close;
use crate::common::pixels::{run_picture, run_sized};

fn grid(cell: i32, gap: f32, amount: f32, keep_brightness: bool) -> pixel_grid::Params {
    pixel_grid::Params {
        cell,
        gap,
        amount,
        keep_brightness,
    }
}

/// The grid over a white `size` x `size` picture, row after row.
fn on_white(params: &pixel_grid::Params, size: usize) -> Result<Vec<[f32; 3]>, Error> {
    run_sized(params, size, size, 1.0)
}

/// Row `row` of a `size` x `size` picture.
fn row_of(picture: &[[f32; 3]], size: usize, row: usize) -> &[[f32; 3]] {
    &picture[row * size..(row + 1) * size]
}

#[test]
fn amount_blends_linearly_from_the_picture_to_the_full_grid() -> Result<(), Error> {
    let picture: Vec<f32> = (0..10 * 10 * 3_u16)
        .map(|index| f32::from(index * 7 % 31) / 31.0)
        .collect();
    let with = |amount| run_picture(&grid(5, 0.3, amount, false), 10, 10, &picture);
    let (full, half) = (with(1.0)?, with(0.5)?);
    let expected: Vec<f32> = picture
        .iter()
        .zip(&full)
        .map(|(shown, lit)| 0.5 * shown + 0.5 * lit)
        .collect();
    assert_close(&half, &expected, 1e-6, "half the grid");
    Ok(())
}

#[test]
fn cells_without_a_gap_are_three_stripes_each_showing_one_channel() -> Result<(), Error> {
    // cell 6 = stripes 2 px wide: red in columns 0-1, green in 2-3, blue in 4-5, the same in every row
    let out = on_white(&grid(6, 0.0, 1.0, false), 12)?;
    let stripes = [
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
    ];
    for row in 0..12 {
        assert_eq!(row_of(&out, 12, row), [stripes, stripes].concat(), "row {row}");
    }
    Ok(())
}

#[test]
fn a_gap_darkens_the_edges_of_each_stripe_and_the_edge_between_rows_of_cells() -> Result<(), Error> {
    // cell 6, gap 0.5: a stripe is 2 px wide and its dark gap 1 px (half on each side), vertically too: the first red
    // stripe lights half of column 0 and of column 1, and half of the cell's first row and last row
    let out = on_white(&grid(6, 0.5, 1.0, false), 12)?;
    let half = 0.5;
    assert_close(
        row_of(&out, 12, 2)[..6].as_flattened(),
        [
            [half, 0.0, 0.0],
            [half, 0.0, 0.0],
            [0.0, half, 0.0],
            [0.0, half, 0.0],
            [0.0, 0.0, half],
            [0.0, 0.0, half],
        ]
        .as_flattened(),
        1e-6,
        "a middle row",
    );
    for row in [0, 5, 6] {
        assert_close(
            row_of(&out, 12, row)[..2].as_flattened(),
            [[half * half, 0.0, 0.0]; 2].as_flattened(),
            1e-6,
            &format!("row {row}, the first or last of a cell"),
        );
    }
    Ok(())
}

#[test]
fn a_gap_of_1_lights_nothing_whatever_the_cell_so_the_picture_is_black() -> Result<(), Error> {
    // a third of a cell is not a whole number of pixels for most cells, which is where a width computed by subtraction rounds
    for cell in 3..=32 {
        for keep_brightness in [false, true] {
            let out = on_white(&grid(cell, 1.0, 1.0, keep_brightness), 48)?;
            assert!(
                out.as_flattened().iter().all(|&level| level == 0.0),
                "cell {cell}, keep_brightness {keep_brightness}"
            );
        }
    }
    Ok(())
}

#[test]
fn keep_brightness_lifts_a_lit_stripe_by_the_mean_darkening_and_clips_at_white() -> Result<(), Error> {
    // cell 3, no gap, amount 0.5: a lit stripe passes all the light and an unlit one half, the mean gain is
    // (1 + 0.5 + 0.5) / 3 = 2/3, so the gains are 1.5 and 0.75: a grey of 0.4 shows 0.6 on the lit stripe, 0.3 elsewhere
    // and white stays 1 there and is 0.75 on the other stripes
    let kept = grid(3, 0.0, 0.5, true);
    let grey = run_sized(&kept, 3, 6, 0.4)?;
    let white = run_sized(&kept, 3, 6, 1.0)?;
    for column in 0..6 {
        let (mut expected_grey, mut expected_white) = ([0.3; 3], [0.75; 3]);
        expected_grey[column % 3] = 0.6;
        expected_white[column % 3] = 1.0;
        assert_close(&grey[column], &expected_grey, 1e-6, &format!("grey, column {column}"));
        assert_close(
            &white[column],
            &expected_white,
            1e-6,
            &format!("white, column {column}"),
        );
    }
    Ok(())
}

#[test]
fn keep_brightness_keeps_the_average_of_a_dim_picture_over_whole_cells() -> Result<(), Error> {
    // over whole cells the gains average 1 - amount x (1 - the lit share), which they are divided by; a grey of 0.2 stays
    // below 1 whatever it is lifted by, so nothing clips and the picture averages 0.2
    for (cell, size, gap, amount) in [(6, 24, 0.25, 0.8), (4, 16, 0.3, 0.6)] {
        let out = run_sized(&grid(cell, gap, amount, true), size, size, 0.2)?;
        let mean = out.as_flattened().iter().sum::<f32>() / f32::from(u16::try_from(3 * size * size).unwrap_or(1));
        assert!(
            (mean - 0.2).abs() <= 1e-5,
            "cell {cell} gap {gap} amount {amount}: {mean}"
        );
    }
    Ok(())
}

#[test]
fn with_no_settings_a_cell_of_4_px_lights_each_stripe_as_the_documented_gap_and_amount_leave_it() -> Result<(), Error> {
    // cell 4, gap 0.15, amount 0.4 on white: a stripe is 4 / 3 px wide and the gap leaves 0.15 x (4 / 3) / 2 = 0.1 px free
    // on each side of it, so red covers 0.1 to 1.2333 px of the cell, green 1.4333 to 2.5667 and blue 2.7667 to 3.9; a
    // pixel's lit share is the length of that it spans, and a row of cells likewise covers 0.1 to 3.9 of its 4 px. The
    // result is the picture mixed with the grid by amount: 1 - 0.4 x (1 - lit)
    let out = on_white(&pixel_grid::Params::default(), 8)?;
    let lit_columns = [
        [0.9, 0.0, 0.0],
        [0.233_333_3, 0.566_666_7, 0.0],
        [0.0, 0.566_666_7, 0.233_333_3],
        [0.0, 0.0, 0.9],
    ];
    // row 4 starts the next cell
    for (row, lit_row) in [(0, 0.9), (1, 1.0), (2, 1.0), (3, 0.9), (4, 0.9)] {
        let expected: Vec<[f32; 3]> = lit_columns
            .iter()
            .map(|lit| lit.map(|share: f32| 1.0 - 0.4 * (1.0 - lit_row * share)))
            .collect();
        assert_close(
            row_of(&out, 8, row)[..4].as_flattened(),
            expected.as_flattened(),
            1e-6,
            &format!("row {row}"),
        );
    }
    Ok(())
}

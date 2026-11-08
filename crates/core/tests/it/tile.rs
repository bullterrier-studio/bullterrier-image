//! `tile`: the picture averaged down by area to a cell, the cell slid with wrapping, the cell repeated. Checked by the
//! average of a block worked out by brute force and by hand, by the repetition, and by the slide of a cell by index.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "small picture sizes; the f64 references narrowed to the f32 picture"
)]

use bullterrier_image::Error;
use bullterrier_image::effects::tile::Params;

use crate::common::compare::{first_difference, first_difference_within};
use crate::common::pictures::varied;
use crate::common::pixels::{channel, run_picture};

const TOLERANCE: f32 = 1e-5;

/// `params` with the offsets zero.
fn grid(columns: i32, rows: i32) -> Params {
    Params {
        columns,
        rows,
        ..Params::default()
    }
}

#[test]
fn one_column_and_one_row_give_the_picture_and_whole_cells_of_offset_change_nothing() -> Result<(), Error> {
    let picture = varied(9, 14);
    let plain = Params {
        columns: 1,
        rows: 1,
        offset_x: 0.0,
        offset_y: 0.0,
    };
    assert_eq!(first_difference(&run_picture(&plain, 9, 14, &picture)?, &picture), None);
    for (offset_x, offset_y) in [(1.0, 0.0), (-1.0, 1.0), (0.0, -1.0)] {
        let params = Params {
            offset_x,
            offset_y,
            ..plain.clone()
        };
        let at = first_difference_within(&run_picture(&params, 9, 14, &picture)?, &picture, TOLERANCE);
        assert_eq!(at, None, "{params:?}");
    }
    Ok(())
}

#[test]
fn a_cell_is_the_mean_of_the_block_of_the_picture_it_covers() -> Result<(), Error> {
    // 12 x 16 in 4 columns and 4 rows: a 3 x 4 cell, each pixel of it the mean of a 4 x 4 block (brute force)
    let (height, width) = (12, 16);
    let picture = varied(height, width);
    let out = run_picture(&grid(4, 4), height, width, &picture)?;
    for (i, j, channel) in (0..3).flat_map(|i| (0..4).flat_map(move |j| (0..3).map(move |c| (i, j, c)))) {
        let mean: f32 = (0..4)
            .flat_map(|down| (0..4).map(move |across| (down, across)))
            .map(|(down, across)| picture[((i * 4 + down) * width + (j * 4 + across)) * 3 + channel])
            .sum::<f32>()
            / 16.0;
        assert!(
            (out[(i * width + j) * 3 + channel] - mean).abs() < TOLERANCE,
            "cell ({i}, {j}) channel {channel}"
        );
    }
    Ok(())
}

#[test]
fn a_side_the_grid_does_not_divide_is_averaged_with_part_pixels_and_the_leftover_shows_the_next_copy()
-> Result<(), Error> {
    // a row of 5 in 2 columns: a cell of 2, each cell pixel covers 2.5 source pixels:
    // (a + b + c / 2) / 2.5 and (c / 2 + d + e) / 2.5; the fifth pixel is the start of the next copy
    let row = [0.1, 0.2, 0.4, 0.6, 0.8];
    let grey: Vec<f32> = row.iter().flat_map(|&level| [level; 3]).collect();
    let out = run_picture(&grid(2, 1), 1, 5, &grey)?;
    let red = channel(&out, 0);
    assert_eq!(
        first_difference_within(&red, &[0.2, 0.64, 0.2, 0.64, 0.2], 1e-6),
        None,
        "{red:?}"
    );
    Ok(())
}

#[test]
fn the_cell_repeats_across_the_picture_to_its_last_pixel() -> Result<(), Error> {
    // 14 x 17 in a grid of columns x rows: a cell of 17 // columns wide and 14 // rows high (different for the two, so a
    // mix-up shows), the leftover pixels showing the next copy's start
    let (height, width) = (14, 17);
    let picture = varied(height, width);
    for (columns, rows, cell_width, cell_height) in [(3, 3, 5, 4), (4, 2, 4, 7), (2, 5, 8, 2), (100, 100, 1, 1)] {
        for (offset_x, offset_y) in [(0.0, 0.0), (0.3, -0.6)] {
            let params = Params {
                offset_x,
                offset_y,
                ..grid(columns, rows)
            };
            let out = run_picture(&params, height, width, &picture)?;
            for at in 0..height * width {
                let (row, column) = (at / width, at % width);
                let first = ((row % cell_height) * width + column % cell_width) * 3;
                assert_eq!(
                    first_difference(&out[at * 3..at * 3 + 3], &out[first..first + 3]),
                    None,
                    "{params:?}: pixel ({row}, {column})"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_flat_and_a_grid_that_divides_keeps_the_mean() -> Result<(), Error> {
    let flat = vec![0.37_f32; 9 * 16 * 3];
    let busy = varied(12, 16);
    for params in [
        grid(3, 3),
        Params {
            offset_x: 0.37,
            offset_y: -0.8,
            ..grid(4, 2)
        },
        grid(100, 100),
    ] {
        let out = run_picture(&params, 9, 16, &flat)?;
        assert_eq!(first_difference_within(&out, &flat, TOLERANCE), None, "{params:?}");
    }
    for params in [
        grid(4, 3),
        Params {
            offset_x: 0.37,
            offset_y: -0.8,
            ..grid(4, 3)
        },
    ] {
        let (before, after) = (
            busy.iter().sum::<f32>(),
            run_picture(&params, 12, 16, &busy)?.iter().sum::<f32>(),
        );
        assert!((before - after).abs() < 1e-2, "{params:?}: {before} became {after}");
    }
    Ok(())
}

#[test]
fn a_positive_offset_slides_the_cell_right_and_down_with_what_leaves_coming_back() -> Result<(), Error> {
    // 8 x 8 in 2 x 2: a 4 x 4 cell; an offset of 0.25 is one pixel, 0.5 two. Compared with the cell not slid, by index
    let picture = varied(8, 8);
    let base = run_picture(&grid(2, 2), 8, 8, &picture)?;
    for (offset_x, offset_y, along, down) in [
        (0.25, 0.0, 1, 0),
        (0.5, 0.0, 2, 0),
        (-0.25, 0.0, -1, 0),
        (0.0, 0.25, 0, 1),
        (0.0, -0.5, 0, -2),
    ] {
        let params = Params {
            offset_x,
            offset_y,
            ..grid(2, 2)
        };
        let out = run_picture(&params, 8, 8, &picture)?;
        for at in 0..8 * 8 {
            let (row, column) = ((at / 8) as i32, (at % 8) as i32);
            let source = ((row - down).rem_euclid(4) as usize) * 8 + (column - along).rem_euclid(4) as usize;
            let (found, expected) = (&out[at * 3..at * 3 + 3], &base[source * 3..source * 3 + 3]);
            assert_eq!(
                first_difference_within(found, expected, TOLERANCE),
                None,
                "{params:?}: pixel {at}"
            );
        }
    }
    Ok(())
}

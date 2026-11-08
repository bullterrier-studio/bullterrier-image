//! `dead_pixels`: which pixels of a grey picture change, found from the output alone. The oracles are counts worked out by
//! hand (how many cells, how many stuck, rounding halves to even) and the promises of the documentation: cells are
//! whole squares of `size`, the picture elsewhere is untouched, a larger count keeps the pixels a smaller one had.

use bullterrier_image::Error;
use bullterrier_image::effects::dead_pixels::Params;
use bullterrier_image::params::Color;

use crate::common::compare::first_bit_difference;
use crate::common::pixels::run_picture;

const GREY: f32 = 0.5;

fn grey(height: usize, width: usize) -> Vec<f32> {
    vec![GREY; height * width * 3]
}

fn pixels(values: &[f32]) -> Vec<[f32; 3]> {
    values.as_chunks::<3>().0.to_vec()
}

/// The pixels of a grey picture that `params` changed, row after row.
fn changed(params: &Params, height: usize, width: usize) -> Result<Vec<bool>, Error> {
    let out = run_picture(params, height, width, &grey(height, width))?;
    Ok(pixels(&out).iter().map(|pixel| *pixel != [GREY; 3]).collect())
}

fn count_of(mask: &[bool]) -> usize {
    mask.iter().filter(|on| **on).count()
}

#[test]
fn exactly_count_pixels_change_and_the_rest_are_untouched() -> Result<(), Error> {
    for count in [0, 1, 7, 100] {
        for stuck in [0.0, 0.5, 1.0] {
            let params = Params {
                count,
                size: 1,
                stuck,
                seed: 3,
                ..Params::default()
            };
            let picture = grey(40, 50);
            let out = run_picture(&params, 40, 50, &picture)?;
            let touched: Vec<bool> = pixels(&out).iter().map(|pixel| *pixel != [GREY; 3]).collect();
            assert_eq!(
                count_of(&touched),
                usize::try_from(count).unwrap_or(0),
                "count {count}, stuck {stuck}"
            );
        }
    }
    Ok(())
}

#[test]
fn every_pixel_is_the_picture_as_it_was_the_stuck_colour_or_black() -> Result<(), Error> {
    let picture = crate::common::pictures::varied(20, 30);
    let params = Params {
        count: 25,
        size: 1,
        seed: 8,
        ..Params::default()
    };
    let out = pixels(&run_picture(&params, 20, 30, &picture)?);
    let mut differing = 0;
    for (after, before) in out.iter().zip(pixels(&picture)) {
        if *after != before {
            differing += 1;
            assert!(*after == [1.0; 3] || *after == [0.0; 3], "{after:?}");
        }
    }
    assert!((12..=25).contains(&differing), "{differing}"); // 25 faulty pixels, a few of which were already as they show
    Ok(())
}

#[test]
fn stuck_is_the_share_of_the_defects_that_are_the_colour_and_the_rest_are_black() -> Result<(), Error> {
    // (stuck, expected stuck of ten): count x stuck rounded, halves to the even number (2.5 -> 2, 7.5 -> 8; 0.25 and 0.75 are exact in f32)
    let rows = [(0.0, 0), (1.0, 10), (0.5, 5), (0.3, 3), (0.26, 3), (0.25, 2), (0.75, 8)];
    for (stuck, bright) in rows {
        let params = Params {
            count: 10,
            size: 1,
            stuck,
            color: Color([1.0, 128.0 / 255.0, 0.0]),
            seed: 1,
            ..Params::default()
        };
        let out = pixels(&run_picture(&params, 40, 50, &grey(40, 50))?);
        let stuck_count = out.iter().filter(|pixel| **pixel == [1.0, 128.0 / 255.0, 0.0]).count();
        let dead_count = out.iter().filter(|pixel| **pixel == [0.0; 3]).count();
        assert_eq!((stuck_count, dead_count), (bright, 10 - bright), "stuck {stuck}");
    }
    Ok(())
}

#[test]
fn a_defect_fills_one_square_cell_of_a_grid_of_size_px() -> Result<(), Error> {
    // a 30 x 30 picture has 15, 10 or 6 cells a side for sizes 2, 3 and 5; each cell is all one defect or untouched
    for size in [2_usize, 3, 5] {
        let params = Params {
            count: 6,
            size: i32::try_from(size).unwrap_or(0),
            seed: 2,
            ..Params::default()
        };
        let mask = changed(&params, 30, 30)?;
        let cells = 30 / size;
        let mut hit = 0;
        for cell_row in 0..cells {
            for cell_column in 0..cells {
                let inside = (0..size * size)
                    .filter(|at| mask[(cell_row * size + at / size) * 30 + cell_column * size + at % size])
                    .count();
                assert!(
                    inside == 0 || inside == size * size,
                    "size {size}: a cell has {inside} changed pixels"
                );
                hit += usize::from(inside > 0);
            }
        }
        assert_eq!(hit, 6, "size {size}");
    }
    Ok(())
}

#[test]
fn a_count_beyond_the_cells_fills_every_cell_including_the_clipped_ones_at_the_edge() -> Result<(), Error> {
    // a 7 x 5 picture with size 3 has 3 x 2 cells; the last row and column of cells are 1 and 2 px wide
    let params = Params {
        count: 10_000,
        size: 3,
        stuck: 1.0,
        color: Color([1.0; 3]),
        ..Params::default()
    };
    let out = run_picture(&params, 7, 5, &grey(7, 5))?;
    assert!(out.iter().all(|level| (*level - 1.0).abs() < f32::EPSILON));
    Ok(())
}

#[test]
fn seed_and_count_alone_choose_where_the_defects_are() -> Result<(), Error> {
    let red = Params {
        count: 20,
        stuck: 1.0,
        color: Color([1.0, 0.0, 0.0]),
        seed: 4,
        ..Params::default()
    };
    let dead = Params {
        stuck: 0.0,
        color: Color([0.0, 1.0, 0.0]),
        ..red.clone()
    };
    assert!(
        changed(&red, 40, 50)? == changed(&dead, 40, 50)?,
        "the colour and the share only paint"
    );
    assert!(
        changed(&red, 40, 50)? != changed(&Params { seed: 5, ..red }, 40, 50)?,
        "another seed, other places"
    );
    Ok(())
}

#[test]
fn more_defects_keep_the_ones_there_were() -> Result<(), Error> {
    let at = |count| {
        changed(
            &Params {
                count,
                size: 1,
                seed: 5,
                ..Params::default()
            },
            40,
            50,
        )
    };
    let (few, many) = (at(10)?, at(25)?);
    assert!(few.iter().zip(&many).all(|(before, after)| !before || *after));
    assert_eq!((count_of(&few), count_of(&many)), (10, 25));
    Ok(())
}

#[test]
fn every_cell_is_equally_likely_to_be_the_one_faulty_pixel() -> Result<(), Error> {
    // one faulty pixel on a 2 x 3 grid of cells over 600 seeds: each cell is hit 100 times on average, a standard deviation
    // of sqrt(600 x 1/6 x 5/6) = 9.1 (the bound is 3.8 of them)
    let mut hits = [0_usize; 6];
    for seed in 0..600 {
        let mask = changed(
            &Params {
                count: 1,
                size: 1,
                seed,
                ..Params::default()
            },
            2,
            3,
        )?;
        hits[mask.iter().position(|on| *on).unwrap_or(0)] += 1;
    }
    assert!(hits.iter().all(|hit| hit.abs_diff(100) <= 35), "{hits:?}");
    Ok(())
}

#[test]
fn a_count_of_zero_leaves_the_picture_as_it_is() -> Result<(), Error> {
    let picture = crate::common::pictures::varied(9, 11);
    let out = run_picture(
        &Params {
            count: 0,
            ..Params::default()
        },
        9,
        11,
        &picture,
    )?;
    assert_eq!(first_bit_difference(&out, &picture), None);
    Ok(())
}

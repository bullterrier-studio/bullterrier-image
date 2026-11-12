//! `jpeg_crush`: JPEG's lossy steps as maths, judged against values worked out by hand from ITU-T T.81 (flat blocks that
//! survive exactly, one cosine rounded to its Annex K step) and by properties (blocky at the lowest quality, a hit stays
//! where JPEG lets it reach, the damage at a size ratio is the design-size damage repeated).

use bullterrier_image::effects::jpeg_crush::Params;
use bullterrier_image::params::{Declared, EffectParams};
use bullterrier_image::{Error, Inputs, Picture, apply};
use serde_json::Value;

use crate::common::compare::assert_close;

/// `params` applied to the `height` x `width` picture `values` at size ratio `ratio`.
fn run(params: &Params, height: usize, width: usize, values: &[f32], ratio: f64) -> Result<Vec<f32>, Error> {
    let image = apply(
        Params::EFFECT,
        &Picture::new(height, width, values)?,
        &Value::Object(params.values()),
        &Inputs::default().with_size_ratio(ratio),
    )?;
    Ok(image.into_values())
}

fn crush(quality: i32, height: usize, width: usize, values: &[f32]) -> Result<Vec<f32>, Error> {
    run(
        &Params {
            quality,
            ..Params::default()
        },
        height,
        width,
        values,
        1.0,
    )
}

/// A picture whose red, green and blue are all the 8-bit `levels` (row after row).
fn grey(levels: &[u8]) -> Vec<f32> {
    levels.iter().flat_map(|&level| [f32::from(level) / 255.0; 3]).collect()
}

/// A picture of flat 8 x 8 blocks of the 8-bit grey `levels` (one per entry, `across` to a row of blocks).
fn blocks_of_grey(levels: &[u8], across: usize) -> (Vec<f32>, usize, usize) {
    let rows = levels.len() / across;
    let picture = (0..rows * 8)
        .flat_map(|row| (0..across * 8).map(move |column| (row, column)))
        .map(|(row, column)| levels[(row / 8) * across + column / 8])
        .collect::<Vec<_>>();
    (grey(&picture), rows * 8, across * 8)
}

/// A gentle colour gradient inside 0.3..0.7, so that damage never runs into the ends of the range and hides.
#[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
fn smooth(height: usize, width: usize) -> Vec<f32> {
    let (h, w) = (height as f32, width as f32);
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (y as f32, x as f32)))
        .flat_map(|(y, x)| [x / w, y / h, (x + y) / (w + h)].map(|level| 0.3 + 0.4 * level))
        .collect()
}

fn mean_error(left: &[f32], right: &[f32]) -> f32 {
    #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
    let count = left.len() as f32;
    left.iter().zip(right).map(|(l, r)| (l - r).abs()).sum::<f32>() / count
}

/// The 8 x 8 block of samples whose orthonormal DCT is `coefficient` at (`vertical`, `horizontal`) and zero elsewhere, by
/// the definition: coefficient a(v) a(u) cos((2y + 1) v pi / 16) cos((2x + 1) u pi / 16), a(0) = sqrt(1/8), a(k) = 1/2.
fn cosine_block(vertical: usize, horizontal: usize, coefficient: f64) -> Vec<f64> {
    let scale = |k: usize| if k == 0 { (1.0_f64 / 8.0).sqrt() } else { 0.5 };
    let wave = |index: usize, k: usize| {
        #[expect(clippy::cast_precision_loss, reason = "indices below 16")]
        let angle = (2 * index + 1) as f64 * k as f64 * std::f64::consts::PI / 16.0;
        angle.cos()
    };
    (0..64)
        .map(|at| {
            let (y, x) = (at / 8, at % 8);
            coefficient * scale(vertical) * scale(horizontal) * wave(y, vertical) * wave(x, horizontal)
        })
        .collect()
}

#[test]
fn flat_blocks_survive_exactly_when_their_dc_value_is_a_whole_number_of_steps() -> Result<(), Error> {
    // a flat block of 8-bit value v has DC 8 (v - 128); the DC steps at these qualities are 40, 16 and 8, so offsets from
    // 128 that are multiples of 5 make every DC a whole number of steps
    let (picture, height, width) = blocks_of_grey(&[128, 138, 188, 78], 2);
    for quality in [20, 50, 75] {
        assert_close(
            &crush(quality, height, width, &picture)?,
            &picture,
            1e-5,
            &format!("quality {quality}"),
        );
    }
    Ok(())
}

#[test]
fn a_flat_picture_that_does_not_fill_its_blocks_keeps_its_size_and_its_colour() -> Result<(), Error> {
    let picture = grey(&[138; 13 * 21]);
    assert_close(&crush(50, 13, 21, &picture)?, &picture, 1e-5, "13 x 21");
    Ok(())
}

#[test]
fn a_flat_colour_survives_when_its_chroma_dc_is_a_whole_number_of_chroma_steps() -> Result<(), Error> {
    // Cb = 128 + 17 (in 8-bit levels), Y and Cr = 128: the block's Cb DC is 8 x 17 = 136, exactly 8 of table K.2's step 17
    // at quality 50 (luma's first step is 16, which would round 8.5 to 8 and move the colour)
    let (luma, blue) = (128.0 / 255.0, 145.0 / 255.0);
    let (offset_blue, offset_red) = (blue - 128.0 / 255.0, 0.0);
    // the inverse JFIF matrix of ITU-T T.871
    let colour = [
        luma + 1.402 * offset_red,
        luma - 0.344_136 * offset_blue - 0.714_136 * offset_red,
        luma + 1.772 * offset_blue,
    ];
    let picture: Vec<f32> = colour.repeat(16 * 16);
    assert_close(&crush(50, 16, 16, &picture)?, &picture, 1e-4, "chroma on its step");
    // at quality 80 the step is 7: 136 / 7 = 19.4 rounds to 19 steps, and the colour moves
    assert!(mean_error(&crush(80, 16, 16, &picture)?, &picture) > 1e-3);
    Ok(())
}

#[test]
fn one_cosine_pattern_comes_back_with_its_coefficient_rounded_to_the_annex_k_step() -> Result<(), Error> {
    // at quality 50 the steps are table K.1 itself: (v, u, coefficient, coefficient rounded to whole steps)
    let rows = [
        (0, 1, 96.0, 99.0),   // step 11: 8.7 steps -> 9
        (1, 0, 50.0, 48.0),   // step 12: 4.2 -> 4
        (1, 1, 130.0, 132.0), // step 12: 10.8 -> 11
        (3, 2, 200.0, 198.0), // step 22: 9.1 -> 9
    ];
    for (vertical, horizontal, coefficient, kept) in rows {
        let (before, after) = (
            cosine_block(vertical, horizontal, coefficient),
            cosine_block(vertical, horizontal, kept),
        );
        // the top-left block carries the pattern, the other three are flat grey 128
        let level = |block: &[f64], row: usize, column: usize| {
            128.0
                + if row < 8 && column < 8 {
                    block[row * 8 + column]
                } else {
                    0.0
                }
        };
        let grid = |block: &[f64]| -> Vec<f32> {
            (0..16 * 16)
                .flat_map(|at| {
                    #[expect(clippy::cast_possible_truncation, reason = "levels are small")]
                    let value = (level(block, at / 16, at % 16) / 255.0) as f32;
                    [value; 3]
                })
                .collect()
        };
        assert_close(
            &crush(50, 16, 16, &grid(&before))?,
            &grid(&after),
            1e-4,
            &format!("cosine ({vertical}, {horizontal})"),
        );
    }
    Ok(())
}

#[test]
fn a_block_mean_moves_in_jumps_that_change_only_on_the_8_px_block_grid() -> Result<(), Error> {
    // quality 10 has a DC step of 80, so a block's mean moves in jumps of 10 grey levels (80 / 8): blocks at 132 and 134
    // round to 0 and 1 steps, 128 and 138, with the jump exactly between column 7 and column 8
    let (picture, height, width) = blocks_of_grey(&[132, 134], 2);
    let out = crush(10, height, width, &picture)?;
    let expected: Vec<f32> = (0..8 * 16)
        .flat_map(|at| [if at % 16 < 8 { 128.0 } else { 138.0 } / 255.0; 3])
        .collect();
    assert_close(&out, &expected, 1e-5, "two blocks");
    Ok(())
}

#[test]
fn at_the_lowest_quality_every_block_of_a_gentle_picture_is_flat() -> Result<(), Error> {
    // every step is 255, so a gentle gradient keeps only block means: luma is flat over each 8 x 8 block and chroma over
    // each 16 x 16 one, so the colour is flat over each 8 x 8 block
    let (height, width) = (32, 32);
    let out = crush(1, height, width, &smooth(height, width))?;
    for block_row in 0..height / 8 {
        for block_column in 0..width / 8 {
            let at = |y: usize, x: usize| {
                let pixel = ((block_row * 8 + y) * width + block_column * 8 + x) * 3;
                [out[pixel], out[pixel + 1], out[pixel + 2]]
            };
            for y in 0..8 {
                for x in 0..8 {
                    assert_close(
                        &at(y, x),
                        &at(0, 0),
                        1e-5,
                        &format!("block ({block_row}, {block_column})"),
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn a_smooth_picture_comes_back_within_a_grey_level_or_two_at_quality_100() -> Result<(), Error> {
    let (height, width) = (24, 40);
    let picture = smooth(height, width);
    let out = crush(100, height, width, &picture)?;
    assert!(mean_error(&picture, &out) < 1.0 / 255.0);
    assert_close(&out, &picture, 3.0 / 255.0, "quality 100");
    Ok(())
}

#[test]
fn the_lower_the_quality_the_further_the_picture_is_from_the_original() -> Result<(), Error> {
    let (height, width) = (32, 32);
    let mut state = 7_u32;
    let noisy: Vec<f32> = (0..height * width * 3)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); // Numerical Recipes' constants
            f32::from(u16::try_from(state >> 16).unwrap_or(0)) / 65_535.0
        })
        .collect();
    let errors =
        [95, 75, 50, 25, 5].map(|quality| crush(quality, height, width, &noisy).map(|out| mean_error(&noisy, &out)));
    let errors = errors.into_iter().collect::<Result<Vec<_>, _>>()?;
    assert!(errors.windows(2).all(|pair| pair[0] < pair[1]), "{errors:?}");
    Ok(())
}

#[test]
fn crushing_a_crushed_picture_changes_it_far_less_than_the_first_time() -> Result<(), Error> {
    // quantising is a projection: what survived the first rounding mostly survives the second
    let (height, width) = (32, 32);
    let picture = smooth(height, width);
    let once = crush(20, height, width, &picture)?;
    let twice = crush(20, height, width, &once)?;
    assert!(mean_error(&once, &twice) < 0.25 * mean_error(&picture, &once));
    Ok(())
}

const HEIGHT: usize = 64;
const WIDTH: usize = 96;

fn hit(seed: u32, dc_share: f32) -> Params {
    Params {
        quality: 30,
        hits: 1,
        seed,
        dc_share,
        magnitude: 100,
    }
}

/// The (row, column) of every pixel `damaged` differs from `plain` in.
fn changed(plain: &[f32], damaged: &[f32]) -> Vec<(usize, usize)> {
    (0..HEIGHT * WIDTH)
        .filter(|at| (0..3).any(|channel| plain[at * 3 + channel].to_bits() != damaged[at * 3 + channel].to_bits()))
        .map(|at| (at / WIDTH, at % WIDTH))
        .collect()
}

#[test]
fn without_hits_the_damage_settings_change_nothing() -> Result<(), Error> {
    let picture = smooth(HEIGHT, WIDTH);
    let plain = crush(30, HEIGHT, WIDTH, &picture)?;
    for damage in [
        Params {
            quality: 30,
            seed: 7,
            ..Params::default()
        },
        Params {
            quality: 30,
            dc_share: 1.0,
            ..Params::default()
        },
        Params {
            quality: 30,
            magnitude: 100,
            ..Params::default()
        },
    ] {
        assert_eq!(run(&damage, HEIGHT, WIDTH, &picture, 1.0)?, plain, "{damage:?}");
    }
    Ok(())
}

#[test]
fn a_dc_hit_carries_on_to_every_later_block_of_its_row_and_to_no_other_row() -> Result<(), Error> {
    let picture = smooth(HEIGHT, WIDTH);
    let plain = crush(30, HEIGHT, WIDTH, &picture)?;
    for seed in 0..6 {
        let out = run(&hit(seed, 1.0), HEIGHT, WIDTH, &picture, 1.0)?;
        let moved = changed(&plain, &out);
        assert!(!moved.is_empty(), "seed {seed}");
        // one row of MCUs: a luma block row is 8 px, a chroma one 16, and both lie in a 16-row stripe
        let (top, bottom) = (moved.iter().map(|m| m.0).min(), moved.iter().map(|m| m.0).max());
        assert_eq!(
            top.map(|row| row / 16),
            bottom.map(|row| row / 16),
            "seed {seed}: more than one stripe"
        );
        // from the first changed column on, every column of the stripe has a changed pixel, up to the right edge
        let first = moved.iter().map(|m| m.1).min().unwrap_or(0);
        for column in first..WIDTH {
            assert!(
                moved.iter().any(|m| m.1 == column),
                "seed {seed}: column {column} untouched"
            );
        }
    }
    Ok(())
}

#[test]
fn an_ac_hit_stays_inside_one_block() -> Result<(), Error> {
    let picture = smooth(HEIGHT, WIDTH);
    let plain = crush(30, HEIGHT, WIDTH, &picture)?;
    for seed in 0..6 {
        let out = run(&hit(seed, 0.0), HEIGHT, WIDTH, &picture, 1.0)?;
        let moved = changed(&plain, &out);
        assert!(!moved.is_empty(), "seed {seed}");
        // a luma block is 8 x 8 px, a chroma block 16 x 16: all changes are in one 16 x 16 cell
        let cell = (moved[0].0 / 16, moved[0].1 / 16);
        assert!(
            moved.iter().all(|m| (m.0 / 16, m.1 / 16) == cell),
            "seed {seed}: {moved:?}"
        );
    }
    Ok(())
}

#[test]
fn a_detail_hit_never_moves_the_mean_of_a_block_of_brightness() -> Result<(), Error> {
    // a detail hit on luma leaves a block's mean brightness (Rec. 601 weights) as it is and a hit on chroma leaves brightness
    // alone; a hit on a luma DC value would shift a block's mean, so none may land there (a high quality keeps the hits small
    // enough to stay clear of the ends of the range)
    let picture = smooth(32, 32);
    let brightness = |values: &[f32], block_row: usize, block_column: usize| -> f32 {
        (0..64)
            .map(|at| {
                let pixel = ((block_row * 8 + at / 8) * 32 + block_column * 8 + at % 8) * 3;
                0.299 * values[pixel] + 0.587 * values[pixel + 1] + 0.114 * values[pixel + 2]
            })
            .sum::<f32>()
            / 64.0
    };
    let plain = crush(95, 32, 32, &picture)?;
    let out = run(
        &Params {
            quality: 95,
            hits: 400,
            dc_share: 0.0,
            magnitude: 1,
            ..Params::default()
        },
        32,
        32,
        &picture,
        1.0,
    )?;
    assert!(mean_error(&plain, &out) > 1e-3, "nothing was hit");
    for (row, column) in (0..4).flat_map(|row| (0..4).map(move |column| (row, column))) {
        let (before, after) = (brightness(&plain, row, column), brightness(&out, row, column));
        assert!(
            (before - after).abs() < 1e-4,
            "block ({row}, {column}): {before} -> {after}"
        );
    }
    Ok(())
}

#[test]
fn more_hits_damage_more_of_the_picture() -> Result<(), Error> {
    let picture = smooth(HEIGHT, WIDTH);
    let plain = crush(30, HEIGHT, WIDTH, &picture)?;
    let damaged = |hits| {
        run(
            &Params {
                quality: 30,
                hits,
                dc_share: 0.0,
                ..Params::default()
            },
            HEIGHT,
            WIDTH,
            &picture,
            1.0,
        )
        .map(|out| changed(&plain, &out).len())
    };
    assert!(damaged(3)? < damaged(60)?);
    Ok(())
}

#[test]
fn a_larger_magnitude_changes_the_pixels_further() -> Result<(), Error> {
    let picture = smooth(HEIGHT, WIDTH);
    let plain = crush(30, HEIGHT, WIDTH, &picture)?;
    let far = |magnitude| {
        run(
            &Params {
                quality: 30,
                hits: 30,
                magnitude,
                ..Params::default()
            },
            HEIGHT,
            WIDTH,
            &picture,
            1.0,
        )
        .map(|out| mean_error(&plain, &out))
    };
    assert!(far(1)? < far(100)?);
    Ok(())
}

fn damage_params() -> Params {
    Params {
        quality: 20,
        hits: 6,
        seed: 3,
        ..Params::default()
    }
}

/// The mean of the input pixels (a `side` x `side` picture) whose centres lie in each box of the `design` x `design` grid
/// laid over it, a box taking the pixel centred on its right or lower edge and not the one on its left or upper edge (the
/// averaging the shrink to the design size is meant to be).
fn boxes(picture: &[f32], side: usize, design: usize) -> Vec<f32> {
    #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
    let (scale, side_f) = (side as f64 / design as f64, side as f64);
    // the pixels whose centre p + 0.5 lies in (j scale, (j + 1) scale]
    let members = |box_index: usize| -> Vec<usize> {
        (0..side)
            .filter(|&pixel| {
                #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
                let (centre, low) = (pixel as f64 + 0.5, box_index as f64 * scale);
                centre > low && centre <= (low + scale).min(side_f)
            })
            .collect()
    };
    (0..design * design * 3)
        .map(|at| {
            let (box_at, channel) = (at / 3, at % 3);
            let (rows, columns) = (members(box_at / design), members(box_at % design));
            #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
            let count = (rows.len() * columns.len()) as f32;
            rows.iter()
                .flat_map(|&row| {
                    columns
                        .iter()
                        .map(move |&column| picture[(row * side + column) * 3 + channel])
                })
                .sum::<f32>()
                / count
        })
        .collect()
}

#[test]
fn at_a_size_ratio_the_damage_is_the_design_size_damage_repeated_over_the_pixels_it_covers() -> Result<(), Error> {
    // (side, design side): a whole ratio of 4, and 1.5 whose boxes hold two pixels or one
    for (side, design) in [(64, 16), (48, 32)] {
        #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
        let ratio = side as f64 / design as f64;
        let picture = smooth(side, side);
        let small = boxes(&picture, side, design);
        let crushed = run(&damage_params(), design, design, &small, 1.0)?;
        let expected: Vec<f32> = (0..side * side * 3)
            .map(|at| {
                let (pixel, channel) = (at / 3, at % 3);
                let near = (((pixel / side) * design / side) * design + (pixel % side) * design / side) * 3 + channel;
                (picture[at] + (crushed[near] - small[near])).clamp(0.0, 1.0)
            })
            .collect();
        assert_close(
            &run(&damage_params(), side, side, &picture, ratio)?,
            &expected,
            1e-5,
            &format!("ratio {ratio}"),
        );
    }
    Ok(())
}

#[test]
fn a_ratio_that_is_not_whole_gives_a_picture_of_the_same_size_in_range() -> Result<(), Error> {
    let picture = smooth(40, 56);
    for ratio in [1.5, 2.5, 0.5] {
        let out = run(&damage_params(), 40, 56, &picture, ratio)?;
        assert_eq!(out.len(), picture.len(), "ratio {ratio}");
        assert!(out.iter().all(|level| (0.0..=1.0).contains(level)), "ratio {ratio}");
    }
    Ok(())
}

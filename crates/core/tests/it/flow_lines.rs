//! `flow_lines`: lines drawn along a noise flow, judged by what a stamped disc, a straight line and a count of lines must
//! look like, worked out by hand. With `twist` 0 the flow runs right everywhere, so a line is a horizontal run whose length is
//! `length` rounded to whole steps; a line of no steps is one disc, whose total ink is its area.

use bullterrier_image::Error;
use bullterrier_image::effects::flow_lines::Params;
use bullterrier_image::params::Color;

use crate::common::compare::first_bit_difference;
use crate::common::numbers::float;
use crate::common::pixels::{is_pixel, run_sized};

const WHITE: Color = Color([1.0; 3]);
const BLACK: Color = Color([0.0; 3]);

/// The ink of a `height` x `width` picture: white lines on black, so a pixel's level is its ink.
fn ink(params: &Params, height: usize, width: usize) -> Result<Vec<f32>, Error> {
    let out = run_sized(
        &Params {
            line: WHITE,
            background: BLACK,
            ..params.clone()
        },
        height,
        width,
        0.5,
    )?;
    Ok(out.iter().map(|pixel| pixel[0]).collect())
}

/// Lines per 1000 px² that make exactly `lines` lines in a `height` x `width` picture.
#[expect(clippy::cast_possible_truncation, reason = "a small density")]
fn density_for(lines: usize, height: usize, width: usize) -> f32 {
    (1000.0 * float(lines) / float(height * width)) as f32
}

/// The rows and the columns that hold any ink of a `height` x `width` picture.
fn extent(ink: &[f32], height: usize, width: usize) -> (Vec<usize>, Vec<usize>) {
    let rows = (0..height)
        .filter(|&row| (0..width).any(|column| ink[row * width + column] > 0.0))
        .collect();
    let columns = (0..width)
        .filter(|&column| (0..height).any(|row| ink[row * width + column] > 0.0))
        .collect();
    (rows, columns)
}

/// The number of 4-connected groups of inked pixels.
fn groups(ink: &[f32], width: usize) -> usize {
    let mut seen = vec![false; ink.len()];
    let mut count = 0;
    for start in 0..ink.len() {
        if ink[start] <= 0.0 || seen[start] {
            continue;
        }
        count += 1;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(at) = stack.pop() {
            let (row, column) = (at / width, at % width);
            let mut near = Vec::new();
            if row > 0 {
                near.push(at - width);
            }
            if at + width < ink.len() {
                near.push(at + width);
            }
            if column > 0 {
                near.push(at - 1);
            }
            if column + 1 < width {
                near.push(at + 1);
            }
            for next in near {
                if ink[next] > 0.0 && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
    }
    count
}

#[test]
fn no_density_draws_no_lines_and_the_picture_is_the_background() -> Result<(), Error> {
    let background = Color([0.25, 0.5, 0.75]);
    let out = run_sized(
        &Params {
            density: 0.0,
            background,
            ..Params::default()
        },
        20,
        30,
        1.0,
    )?;
    assert!(out.iter().all(|pixel| is_pixel(*pixel, background.0)));
    Ok(())
}

#[test]
fn a_line_of_no_steps_is_one_disc_whose_ink_is_its_area() -> Result<(), Error> {
    // length 1 in steps of 16 rounds to no step: the start alone is stamped, a disc whose edge fades over 1 px: coverage 1 out
    // to radius r - 1/2, then falling linearly to 0 at r + 1/2. Its area is pi (r - 1/2)^2 + 2 pi (c d^2 / 2 - d^3 / 3) from
    // r - 1/2 to r + 1/2, c = r + 1/2: pi/3 for r = 1/2, 7.3304 for r = 3/2, 78.80 for r = 5. The ink of one disc on the pixel
    // grid depends on where it falls, but averaged over starts it is the area
    for (thickness, area) in [(1.0, std::f64::consts::FRAC_PI_3), (3.0, 7.3304), (10.0, 78.80)] {
        let (mut total, mut judged) = (0.0, 0);
        for seed in 0..60 {
            let params = Params {
                density: density_for(1, 100, 100),
                length: 1.0,
                step: 16.0,
                thickness,
                seed,
                ..Params::default()
            };
            let ink = ink(&params, 100, 100)?;
            let on_edge = (0..100).any(|at| {
                [at, 99 * 100 + at, at * 100, at * 100 + 99]
                    .iter()
                    .any(|&place| ink[place] > 0.0)
            });
            if on_edge {
                continue; // part of the disc lies off the picture
            }
            assert!(ink.iter().all(|level| (0.0..=1.0).contains(level)));
            total += ink.iter().map(|level| f64::from(*level)).sum::<f64>();
            judged += 1;
        }
        assert!(
            judged >= 30,
            "thickness {thickness}: only {judged} seeds had a whole disc"
        );
        let found = total / float(judged);
        assert!(
            (found / area - 1.0).abs() < 0.02,
            "thickness {thickness}: {found} against {area}"
        );
    }
    Ok(())
}

#[test]
fn a_straight_line_runs_right_for_its_length_rounded_to_whole_steps_or_to_the_edge() -> Result<(), Error> {
    // twist 0: every move is one step to the right. A disc of diameter 1 reaches 1 px each way from a point, so the inked
    // columns span the line's reach (steps x step) plus 1 to 3 px, in at most two rows; at the right edge the line stops
    for (length, step, reach) in [
        (30.0, 1.0, 30.0),
        (30.0, 4.0, 32.0),
        (12.0, 5.0, 10.0),
        (7.0, 16.0, 0.0),
    ] {
        let (mut free, mut cut) = (0, 0);
        for seed in 0..40 {
            let params = Params {
                density: density_for(1, 70, 200),
                length,
                step,
                thickness: 1.0,
                twist: 0.0,
                seed,
                ..Params::default()
            };
            let (rows, columns) = extent(&ink(&params, 70, 200)?, 70, 200);
            assert!(rows.len() <= 2, "length {length}, seed {seed}: rows {rows:?}");
            let span = float(columns.last().map_or(0, |last| last + 1 - columns[0]));
            if step <= 1.0 {
                assert_eq!(
                    columns.last().map(|last| last + 1 - columns[0]),
                    Some(columns.len()),
                    "a solid run"
                );
            }
            let last = columns.last().copied().unwrap_or(0);
            // a line whose next step would leave the picture may have been cut short
            if float(last) + f64::from(step) + 2.0 >= 200.0 {
                cut += 1;
                assert!(
                    span <= reach + 3.0,
                    "length {length} step {step} seed {seed}: span {span}"
                );
            } else {
                free += 1;
                assert!(
                    (reach + 1.0..=reach + 3.0).contains(&span),
                    "length {length} step {step} seed {seed}: span {span}"
                );
            }
        }
        assert!(
            free >= 10 && cut >= 1,
            "length {length}: {free} free lines, {cut} cut by the edge"
        );
    }
    Ok(())
}

#[test]
fn each_line_started_is_one_group_of_ink_and_a_denser_start_makes_more() -> Result<(), Error> {
    for lines in [0, 1, 5, 12] {
        let params = Params {
            density: density_for(lines, 150, 150),
            length: 1.0,
            step: 16.0,
            thickness: 3.0,
            seed: 4,
            ..Params::default()
        };
        let found = groups(&ink(&params, 150, 150)?, 150);
        // lines that start close together join up, so there can be fewer groups than lines but never more
        assert!(
            found <= lines && found + 2 >= lines,
            "{lines} lines made {found} groups"
        );
    }
    Ok(())
}

#[test]
fn where_lines_cross_the_stronger_ink_counts_and_a_line_is_never_bolder_than_one() -> Result<(), Error> {
    let out = run_sized(
        &Params {
            density: 50.0,
            length: 200.0,
            thickness: 4.0,
            line: Color([1.0, 0.0, 0.0]),
            background: Color([0.0, 0.0, 1.0]),
            ..Params::default()
        },
        100,
        100,
        0.0,
    )?;
    // every pixel lies on the segment from the background to the line colour
    assert!(
        out.iter()
            .all(|[red, green, blue]| *green == 0.0 && (red + blue - 1.0).abs() < 1e-6 && *red >= 0.0 && *blue >= 0.0)
    );
    assert!(out.iter().any(|pixel| pixel[0] >= 1.0 - 1e-6), "a solid line pixel");
    Ok(())
}

#[test]
fn a_thicker_line_covers_everything_a_thinner_one_does() -> Result<(), Error> {
    let at = |thickness| {
        ink(
            &Params {
                density: 3.0,
                thickness,
                seed: 7,
                ..Params::default()
            },
            90,
            110,
        )
    };
    let (thin, thick) = (at(1.0)?, at(4.0)?);
    assert!(thin.iter().zip(&thick).all(|(thin, thick)| thin <= thick));
    assert!(thick.iter().sum::<f32>() > 2.0 * thin.iter().sum::<f32>());
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_lines_and_another_seed_others() -> Result<(), Error> {
    let run = |seed| {
        ink(
            &Params {
                seed,
                ..Params::default()
            },
            80,
            100,
        )
    };
    assert_eq!(first_bit_difference(&run(5)?, &run(5)?), None);
    assert!(first_bit_difference(&run(5)?, &run(6)?).is_some());
    assert!(first_bit_difference(&run(2_147_483_647)?, &run(2_147_483_646)?).is_some());
    Ok(())
}

#[test]
fn a_twist_bends_the_lines_away_from_straight() -> Result<(), Error> {
    // one line of 60 px: straight at twist 0 (within two rows, above); with a full turn of twist per deviation it wanders over
    // many rows, for most starts
    let mut wandering = 0;
    for seed in 0..20 {
        let params = Params {
            density: density_for(1, 200, 200),
            length: 60.0,
            thickness: 1.0,
            twist: 360.0,
            scale: 40.0,
            seed,
            ..Params::default()
        };
        wandering += usize::from(extent(&ink(&params, 200, 200)?, 200, 200).0.len() > 8);
    }
    assert!(wandering >= 10, "{wandering} of 20 lines bend over 8 rows");
    Ok(())
}

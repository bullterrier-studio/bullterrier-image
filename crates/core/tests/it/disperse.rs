//! `disperse`: the picture cut into character cells that detach, drift and go. The oracles are hand-worked: with the
//! wind-up, the ragged front and the fade switched off a cell's journey is `(t + a t^2) / (1 + a)` of the wind, so a cell
//! caught half way through its flight has moved by a number of px worked on paper, and the order cells leave in is a
//! plain ramp (height or side) whose cut by `reach` is counted by hand. Cells are `CELL` px wide and 7 high (the font set
//! 4 px: ascent ceil(1.02 * 4) = 5 and descent ceil(0.3 * 4) = 2, see `ops/glyphs.rs`).

#![expect(clippy::cast_precision_loss, reason = "small indices and cell numbers")]

use bullterrier_image::Error;
use bullterrier_image::effects::disperse::Params;

use crate::common::compare::first_bit_difference;
use crate::common::pixels::run_with_field;

const CELL: usize = 2;
const CELL_HEIGHT: usize = 7;
const ROWS: usize = 8;
const COLUMNS: usize = 30;
const HEIGHT: usize = ROWS * CELL_HEIGHT;
const WIDTH: usize = COLUMNS * CELL;

/// A clean front and a straight flight: no jitter, no wind-up, no glow, no fade, no spread, no wobble, no floating, and all
/// pieces travelling the whole wind, to the right.
fn plain(progress: f32) -> Params {
    Params {
        cell: CELL,
        progress,
        top: 1.0,
        outline: 0.0,
        sweep: 0.0,
        jitter: 0.0,
        pullback: 0.0,
        flare: 0.0,
        falloff: 0.0,
        angle: 0.0,
        wind: 48.0,
        angle_spread: 0.0,
        sway: 0.0,
        distance_minimum: 1.0,
        flight_variation: 0.0,
        ..Params::default()
    }
}

fn solid(level: f32) -> Vec<f32> {
    vec![level; HEIGHT * WIDTH * 3]
}

/// A picture of `level` in the cell at (`row`, `column`) and black elsewhere.
fn one_cell(row: usize, column: usize, level: f32) -> Vec<f32> {
    let mut values = vec![0.0; HEIGHT * WIDTH * 3];
    for y in row * CELL_HEIGHT..(row + 1) * CELL_HEIGHT {
        for x in column * CELL..(column + 1) * CELL {
            values[(y * WIDTH + x) * 3..][..3].fill(level);
        }
    }
    values
}

/// The picture with every px moved by (`dy`, `dx`); what moves off the picture is gone.
fn moved(values: &[f32], dy: isize, dx: isize) -> Vec<f32> {
    let mut out = vec![0.0; values.len()];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if let (Some(to_y), Some(to_x)) = (y.checked_add_signed(dy), x.checked_add_signed(dx))
                && to_y < HEIGHT
                && to_x < WIDTH
            {
                out[(to_y * WIDTH + to_x) * 3..][..3].copy_from_slice(&values[(y * WIDTH + x) * 3..][..3]);
            }
        }
    }
    assert!(
        out.iter().any(|&level| level > 0.0),
        "the moved picture left the frame, so a comparison with it proves nothing"
    );
    out
}

/// Whether the cell at (`row`, `column`) is exactly as in `before`.
fn cell_unchanged(before: &[f32], after: &[f32], row: usize, column: usize) -> bool {
    (row * CELL_HEIGHT..(row + 1) * CELL_HEIGHT).all(|y| {
        let at = (y * WIDTH + column * CELL) * 3;
        first_bit_difference(&before[at..at + CELL * 3], &after[at..at + CELL * 3]).is_none()
    })
}

/// Whether the cell at (`row`, `column`) is black.
fn cell_black(after: &[f32], row: usize, column: usize) -> bool {
    (row * CELL_HEIGHT..(row + 1) * CELL_HEIGHT).all(|y| {
        let at = (y * WIDTH + column * CELL) * 3;
        after[at..at + CELL * 3].iter().all(|&level| level == 0.0)
    })
}

/// A varied picture with no black cell: every cell is lit.
fn varied() -> Vec<f32> {
    (0..HEIGHT * WIDTH * 3)
        .map(|at| 0.3 + 0.7 * ((at * 37 % 101) as f32 / 100.0))
        .collect()
}

/// `params` applied to `picture` with the whole picture as the figure.
fn run(params: &Params, picture: &[f32]) -> Result<Vec<f32>, Error> {
    with_field(params, "field", &vec![1.0; HEIGHT * WIDTH], picture)
}

/// `params` applied to `picture` with `field` as the figure.
fn with_field(params: &Params, parameter: &str, field: &[f32], picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_with_field(params, parameter, field, HEIGHT, WIDTH, picture)
}

/// A field of 1 where `inside(row, column)` of the picture's px, else 0.
fn field(inside: impl Fn(usize, usize) -> bool) -> Vec<f32> {
    (0..HEIGHT * WIDTH)
        .map(|at| f32::from(u8::from(inside(at / WIDTH, at % WIDTH))))
        .collect()
}

#[test]
fn progress_0_leaves_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied();
    let out = run(&plain(0.0), &picture)?;
    assert_eq!(first_bit_difference(&out, &picture), None);
    Ok(())
}

#[test]
fn progress_1_leaves_nothing_of_a_figure_that_goes_wholly() -> Result<(), Error> {
    for params in [
        plain(1.0),
        Params {
            jitter: 0.5,
            ..plain(1.0)
        },
        Params {
            outline: 1.0,
            sweep: -0.5,
            ..plain(1.0)
        },
    ] {
        let out = run(&params, &varied())?;
        assert!(out.iter().all(|&level| level == 0.0));
    }
    Ok(())
}

#[test]
fn reach_leaves_the_last_cells_in_the_order_where_they_were_and_the_first_go() -> Result<(), Error> {
    // top only, on a lit picture: the order of row r is r / 7 once spread to 0..1, so reach 0.5 takes rows 0 to 3
    let picture = varied();
    let out = run(
        &Params {
            reach: 0.5,
            ..plain(1.0)
        },
        &picture,
    )?;
    for column in [0, 7, 29] {
        for row in 0..4 {
            assert!(
                cell_black(&out, row, column),
                "row {row} column {column} should be gone"
            );
        }
        for row in 4..ROWS {
            assert!(
                cell_unchanged(&picture, &out, row, column),
                "row {row} column {column} should stay"
            );
        }
    }
    Ok(())
}

#[test]
fn sweep_sends_the_side_it_points_from_first() -> Result<(), Error> {
    // side only: the order of column c is c / 29; reach 0.5 takes columns 0 to 14 (a positive sweep) or 15 to 29 (negative)
    let picture = varied();
    for (sweep, gone, stays) in [(1.0, 0..15, 15..COLUMNS), (-1.0, 15..COLUMNS, 0..15)] {
        let params = Params {
            top: 0.0,
            sweep,
            reach: 0.5,
            ..plain(1.0)
        };
        let out = run(&params, &picture)?;
        for row in [0, 3, 7] {
            assert!(
                gone.clone().all(|column| cell_black(&out, row, column)),
                "sweep {sweep} row {row}"
            );
            assert!(
                stays.clone().all(|column| cell_unchanged(&picture, &out, row, column)),
                "sweep {sweep} row {row}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_cell_not_yet_reached_by_the_front_is_untouched() -> Result<(), Error> {
    // row r detaches at 0.06 + r / 7 x 0.64 and starts to wind up 0.06 earlier: at progress 0.1 rows 0 and 1 have begun
    // (r / 7 x 0.64 < 0.1 for r < 1.09), the rest has not; the wind lifts a cell up by at most 17 px, so rows 3 and below
    // cannot have been written over
    let picture = varied();
    let out = run(
        &Params {
            angle: 350.0,
            ..plain(0.1)
        },
        &picture,
    )?;
    assert!(first_bit_difference(&out, &picture).is_some());
    for row in 3..ROWS {
        for column in 0..COLUMNS {
            assert!(cell_unchanged(&picture, &out, row, column), "row {row} column {column}");
        }
    }
    Ok(())
}

#[test]
fn a_cell_caught_half_way_through_its_flight_has_moved_the_path_it_has_gone() -> Result<(), Error> {
    // one lit cell (order 0): it detaches at the anticipation 0.06 and flies for 0.2; at 0.16 it is half way, t = 0.5, and
    // has gone (0.5 + 0.5 x 0.25) / 1.5 = 5/12 of the wind 48 px: 20 px
    let picture = one_cell(3, 15, 0.8);
    for (angle, wind, (dy, dx)) in [
        (0.0, 48.0, (0, 20)),
        (0.0, -48.0, (0, -20)),
        (90.0, 48.0, (20, 0)),
        (180.0, 48.0, (0, -20)),
    ] {
        let out = run(
            &Params {
                angle,
                wind,
                ..plain(0.16)
            },
            &picture,
        )?;
        assert_eq!(
            first_bit_difference(&out, &moved(&picture, dy, dx)),
            None,
            "angle {angle}, wind {wind}"
        );
    }
    Ok(())
}

#[test]
fn a_flying_cell_fades_by_one_minus_its_share_of_the_flight_to_the_falloff() -> Result<(), Error> {
    // the same flight, half way: (1 - 0.5)^falloff of the cell's brightness, with falloff 1 and 2
    let picture = one_cell(3, 15, 0.8);
    for falloff in [1.0_f32, 2.0] {
        let out = run(&Params { falloff, ..plain(0.16) }, &picture)?;
        let wanted: Vec<f32> = moved(&picture, 0, 20)
            .iter()
            .map(|level| level * 0.5_f32.powf(falloff))
            .collect();
        assert!(
            out.iter().zip(&wanted).all(|(got, want)| (got - want).abs() < 1e-6),
            "falloff {falloff}"
        );
    }
    Ok(())
}

#[test]
fn the_glow_brightens_a_leaving_cell_and_stops_at_white() -> Result<(), Error> {
    // half way through a flight of span 0.5, the glow is at its peak: 1 + flare
    let picture = one_cell(3, 15, 0.4);
    let out = run(
        &Params {
            flare: 1.0,
            flare_span: 1.0,
            ..plain(0.16)
        },
        &picture,
    )?;
    let wanted: Vec<f32> = moved(&picture, 0, 20)
        .iter()
        .map(|level| (level * 2.0).min(1.0))
        .collect();
    assert!(out.iter().zip(&wanted).all(|(got, want)| (got - want).abs() < 1e-6));
    let bright = run(
        &Params {
            flare: 1.5,
            flare_span: 1.0,
            ..plain(0.16)
        },
        &one_cell(3, 15, 0.9),
    )?;
    let most = bright.iter().copied().fold(0.0, f32::max);
    assert!(most <= 1.0 && most > 0.99, "{most}");
    Ok(())
}

#[test]
fn a_cell_just_inside_a_wall_is_thrown_away_from_it_by_outward() -> Result<(), Error> {
    // the figure fills the right half (px column 20 on); the cell at column 10 is its first column, 2 px from the outside,
    // and the distance rises to the right there, so out is to the left; no wind, no blur of the direction. Half way through
    // the flight it has gone 5/12 of outward x outward_distance 20 px = 8.33 px: 8 px left
    let wall = field(|_, x| x >= 20);
    let picture = one_cell(3, 10, 0.8);
    let params = Params {
        wind: 0.0,
        outward: 1.0,
        outward_distance: 20.0,
        normal_smoothing: 0.0,
        ..plain(0.16)
    };
    let out = with_field(&params, "field", &wall, &picture)?;
    assert_eq!(first_bit_difference(&out, &moved(&picture, 0, -8)), None);
    // with the cell on the other side of the wall's edge the way out is the other way: the cell at column 9 is outside
    Ok(())
}

#[test]
fn the_figure_matters_only_while_outline_has_weight() -> Result<(), Error> {
    let picture = varied();
    let (left, right) = (field(|_, x| x < 20), field(|_, x| x >= 20));
    let run = |params: &Params, field: &[f32]| with_field(params, "field", field, &picture);
    let none = plain(0.5);
    assert_eq!(first_bit_difference(&run(&none, &left)?, &run(&none, &right)?), None);
    let some = Params {
        outline: 1.0,
        ..plain(0.5)
    };
    assert!(first_bit_difference(&run(&some, &left)?, &run(&some, &right)?).is_some());
    Ok(())
}

#[test]
fn outline_sends_the_cells_near_the_edge_before_the_middle_of_the_figure() -> Result<(), Error> {
    // a figure of the middle columns, rows 8 to 47 px; outline only, reach 0.5, finished: everything outside the figure (its
    // distance is 0) goes, the centre of the figure stays and its corner, nearer the edge, goes
    let picture = varied();
    let middle = field(|y, x| (8..48).contains(&y) && (10..30).contains(&x));
    let params = Params {
        top: 0.0,
        outline: 1.0,
        reach: 0.5,
        ..plain(1.0)
    };
    let out = with_field(&params, "field", &middle, &picture)?;
    assert!(
        cell_black(&out, 0, 0) && cell_black(&out, 7, 29),
        "outside the figure goes"
    );
    assert!(cell_unchanged(&picture, &out, 4, 10), "the middle stays");
    assert!(cell_black(&out, 2, 6), "the corner goes");
    Ok(())
}

#[test]
fn hover_loosens_the_cells_at_the_outline_long_before_they_would_otherwise_go() -> Result<(), Error> {
    // the whole picture is outside the figure (depth 0), so with hover every cell floats from the start; the bottom row goes
    // last (detaches at 0.7) and without hover is untouched at progress 0.3
    let picture = varied();
    let nowhere = field(|_, _| false);
    let at = |hover: f32| with_field(&Params { hover, ..plain(0.3) }, "field", &nowhere, &picture);
    assert!(cell_unchanged(&picture, &at(0.0)?, 7, 10));
    assert!(!cell_unchanged(&picture, &at(0.3)?, 7, 10));
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_pieces_and_another_seed_others() -> Result<(), Error> {
    let params = |seed| Params {
        jitter: 1.0,
        flight_variation: 0.9,
        angle_spread: 90.0,
        seed,
        ..plain(0.4)
    };
    let (one, again, other) = (
        run(&params(3), &varied())?,
        run(&params(3), &varied())?,
        run(&params(4), &varied())?,
    );
    assert_eq!(first_bit_difference(&one, &again), None);
    assert!(first_bit_difference(&one, &other).is_some());
    Ok(())
}

#[test]
fn black_is_background_that_never_moves_and_a_dim_cell_is_dropped_once_it_starts() -> Result<(), Error> {
    let black = solid(0.0);
    assert_eq!(first_bit_difference(&run(&plain(0.5), &black)?, &black), None);
    // a picture no brighter than the background threshold (0.02) is all background: black once the dispersal has begun
    let dim = solid(0.02);
    assert!(run(&plain(0.5), &dim)?.iter().all(|&level| level == 0.0));
    assert_eq!(first_bit_difference(&run(&plain(0.0), &dim)?, &dim), None);
    Ok(())
}

#[test]
fn the_shimmer_of_the_cells_that_stay_follows_a_sine_wave_against_the_wind() -> Result<(), Error> {
    // nothing has left at progress 0; wavelength 40 px, depth 0.1 on grey 0.5: a cell's pixels are 0.5 (1 + 0.1 sin(2 pi
    // (phase + x / 40))) with x the px of the cell's middle, 2 c + 1
    let params = Params {
        ambient: 0.1,
        ambient_wavelength: 40.0,
        ..plain(0.0)
    };
    for (phase, wind, side) in [(0.0, 96.0, 1.0_f64), (0.25, 96.0, 1.0), (0.0, -96.0, -1.0)] {
        let out = run(
            &Params {
                ambient_phase: phase,
                wind,
                ..params.clone()
            },
            &solid(0.5),
        )?;
        for column in 0..COLUMNS {
            let x = (2 * column + 1) as f64;
            let wanted = 0.5 + 0.05 * (std::f64::consts::TAU * (f64::from(phase) + side * x / 40.0)).sin();
            let got = f64::from(out[column * CELL * 3]);
            assert!(
                (got - wanted).abs() < 1e-5,
                "phase {phase}, wind {wind}, column {column}: {got} for {wanted}"
            );
        }
    }
    // a turn of the wave is the same picture, and the glow of the shimmer stops at white
    let turn = |phase| {
        run(
            &Params {
                ambient_phase: phase,
                ..params.clone()
            },
            &solid(0.5),
        )
    };
    assert_eq!(first_bit_difference(&turn(0.0)?, &turn(1.0)?), None);
    let white = run(&Params { ambient: 1.0, ..params }, &solid(1.0))?;
    assert!(white.iter().all(|&level| (0.0..=1.0).contains(&level)));
    Ok(())
}

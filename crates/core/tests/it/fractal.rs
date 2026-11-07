//! `fractal`: the escape count of points worked out offline in plain double precision (`z -> z^2 + c` with Python's
//! complex numbers, smooth count `n + 1 - log2(log|z| / log 256)`), where the picture puts each point, and the colours
//! a count takes. The core iterates in single precision, so the points are chosen away from the edge of the set.

use bullterrier_image::Error;
use bullterrier_image::effects::fractal::{self, Kind};
use bullterrier_image::params::Color;

use crate::common::pixels::run_sized;

const TOLERANCE: f32 = 1e-4;
const RED: [f32; 3] = [1.0, 0.0, 0.0];

/// Black to white over a period long enough that a count is its own position (count / 64), interior red.
fn count_as_grey(kind: Kind) -> fractal::Params {
    fractal::Params {
        kind,
        colors: vec![Color([0.0; 3]), Color([1.0; 3])],
        interior: Color(RED),
        period: 64.0,
        iterations: 100,
        zoom: 1.0,
        ..fractal::Params::default()
    }
}

fn assert_grey(actual: [f32; 3], count: f32, context: &str) {
    let expected = [count / 64.0; 3];
    assert!(
        actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= TOLERANCE),
        "{context}: {actual:?}, not {expected:?}"
    );
}

/// The Mandelbrot points (real, imaginary) that escape, with their smooth counts, and some that never do.
const ESCAPING: [(f32, f32, f32); 6] = [
    (1.0, 0.0, 5.766_877),
    (2.0, 0.0, 4.607_978),
    (1.5, 0.0, 5.012_874),
    (0.5, 0.5, 8.098_52),
    (-1.5, 0.5, 6.916_931),
    (-0.75, 0.125, 30.249_464),
];
const STAYING: [(f32, f32); 3] = [(-1.75, 0.0), (0.25, 0.5), (0.375, 0.25)];

#[test]
fn a_mandelbrot_point_takes_the_colour_of_its_smooth_escape_count_and_one_that_stays_the_interior() -> Result<(), Error>
{
    // a one-pixel picture is its centre point (x, y)
    for (x, y, count) in ESCAPING {
        let params = fractal::Params {
            x,
            y,
            ..count_as_grey(Kind::Mandelbrot)
        };
        assert_grey(run_sized(&params, 1, 1, 0.0)?[0], count, &format!("c = {x} + {y}i"));
    }
    for (x, y) in STAYING {
        let params = fractal::Params {
            x,
            y,
            ..count_as_grey(Kind::Mandelbrot)
        };
        assert_eq!(run_sized(&params, 1, 1, 0.0)?[0], RED, "c = {x} + {y}i");
    }
    Ok(())
}

#[test]
fn the_colours_run_forward_and_back_over_the_period() -> Result<(), Error> {
    // c = 1 has count 5.766877: over a period of 4 that is 1.4417 periods, a wave 0.4417 of the way back down; over 2
    // it is 2.8834, 0.8834 into the second pass; over 12 it is 0.4806 of the way up
    for (period, position) in [(4.0, 1.0 - 0.441_72), (12.0, 0.480_57), (2.0, 0.883_44)] {
        let params = fractal::Params {
            x: 1.0,
            period,
            ..count_as_grey(Kind::Mandelbrot)
        };
        let pixel = run_sized(&params, 1, 1, 0.0)?[0];
        assert!(
            (pixel[0] - position).abs() <= 1e-3,
            "period {period}: {pixel:?}, not {position}"
        );
    }
    Ok(())
}

#[test]
fn a_julia_point_is_the_start_and_the_constant_is_the_one_given() -> Result<(), Error> {
    // c = 0: 2 -> 4 -> 16 -> 256 (not past it) -> 65536 on the fourth step, where log|z| / log 256 is exactly 2 and the
    // count is 5 - log2(2) = 4; 1 stays at 1 for ever
    let julia = |x| fractal::Params {
        x,
        c_real: 0.0,
        c_imag: 0.0,
        period: 8.0,
        ..count_as_grey(Kind::Julia)
    };
    assert_eq!(run_sized(&julia(2.0), 1, 1, 0.0)?[0], [0.5; 3]);
    assert_eq!(run_sized(&julia(1.0), 1, 1, 0.0)?[0], RED);
    Ok(())
}

#[test]
fn the_real_axis_runs_left_to_right_and_the_imaginary_axis_upwards_in_steps_of_four_over_zoom_and_the_shorter_side()
-> Result<(), Error> {
    // zoom 8 on a picture one pixel high: a step is 4 / 8 / 1 = 0.5 plane units, so three columns are x - 0.5, x, x + 0.5
    let params = fractal::Params {
        x: 1.0,
        zoom: 8.0,
        ..count_as_grey(Kind::Mandelbrot)
    };
    let across = run_sized(&params, 1, 3, 0.0)?;
    for (pixel, count) in across.iter().zip([8.239_629, 5.766_877, 5.012_874]) {
        assert_grey(*pixel, count, "columns at 0.5, 1 and 1.5");
    }
    // three rows of one column: the top row is the highest imaginary part, 1, then 0.5, then 0
    let params = fractal::Params {
        x: 0.5,
        y: 0.5,
        zoom: 8.0,
        ..count_as_grey(Kind::Mandelbrot)
    };
    let down = run_sized(&params, 3, 1, 0.0)?;
    for (pixel, count) in down.iter().zip([6.139_887, 8.098_52, 8.239_629]) {
        assert_grey(*pixel, count, "rows at 0.5 + 1i, 0.5 + 0.5i and 0.5");
    }
    // a wider picture takes its step from the shorter side, 3 rows: 4 / 8 / 3 = 1/6, so the top row of the middle
    // column is 0.5 + (0.5 + 1/6)i
    let wide = run_sized(&params, 3, 5, 0.0)?;
    assert_grey(
        wide[2],
        7.428_154,
        "the middle column of the top row of a wider picture",
    );
    Ok(())
}

#[test]
fn a_point_that_escapes_on_the_last_iteration_it_is_given_is_outside_and_one_fewer_leaves_it_inside()
-> Result<(), Error> {
    // c = 1 has the orbit 1, 2, 5, 26, 677: past 256 on the fifth step
    let at = |iterations| {
        run_sized(
            &fractal::Params {
                x: 1.0,
                iterations,
                ..count_as_grey(Kind::Mandelbrot)
            },
            1,
            1,
            0.0,
        )
    };
    assert_grey(at(5)?[0], 5.766_877, "five steps");
    assert_eq!(at(4)?[0], RED, "four steps");
    Ok(())
}

#[test]
fn a_julia_set_takes_its_constant_as_real_part_then_imaginary_part() -> Result<(), Error> {
    // from z = 0 a Julia set is the orbit of the Mandelbrot point c: c = 1 escapes (count 5.766877), c = i never does
    let from_origin = |c_real, c_imag| fractal::Params {
        c_real,
        c_imag,
        ..count_as_grey(Kind::Julia)
    };
    assert_grey(run_sized(&from_origin(1.0, 0.0), 1, 1, 0.0)?[0], 5.766_877, "c = 1");
    assert_eq!(run_sized(&from_origin(0.0, 1.0), 1, 1, 0.0)?[0], RED, "c = i");
    Ok(())
}

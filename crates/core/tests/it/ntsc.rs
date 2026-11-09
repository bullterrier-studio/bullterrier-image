//! `ntsc`: colour smeared along the rows and leaking into brightness. The oracle is the FCC YIQ matrix typed in the test and
//! the Gaussian worked out from its definition: with no crawl the picture's brightness is kept and its colour is the
//! row-wise blur of the input's; the crawl adds `crawl` x (I cos t + Q sin t) to brightness, with t worked out by hand.

#![expect(clippy::cast_precision_loss, reason = "small picture sizes")]

use bullterrier_image::effects::ntsc::Params;
use bullterrier_image::{Edge, Error};

use crate::common::compare::assert_close;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;
use crate::common::yiq::{smoothed, yiq, yiq_of};

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

/// Fails unless the YIQ of every pixel of `picture` is `expected`, to within 3e-5.
fn assert_yiq(picture: &[f32], expected: &[[f64; 3]], context: &str) {
    for (at, (got, want)) in yiq_of(picture).iter().zip(expected).enumerate() {
        for axis in 0..3 {
            assert!(
                (got[axis] - want[axis]).abs() < 3e-5,
                "{context}: pixel {at}, axis {axis}: {} against {}",
                got[axis],
                want[axis]
            );
        }
    }
}

#[test]
fn no_smear_and_no_crawl_leave_the_picture_as_it_is_whatever_the_edge_rule() -> Result<(), Error> {
    let picture = varied(9, 11);
    for edge in EDGES {
        let params = Params {
            sigma: 0.0,
            crawl: 0.0,
            edge,
            ..Params::default()
        };
        assert_close(&run_picture(&params, 9, 11, &picture)?, &picture, 1e-5, &format!("{edge:?}"));
    }
    Ok(())
}

#[test]
fn a_grey_picture_has_no_colour_to_smear_or_leak_so_it_is_unchanged() -> Result<(), Error> {
    let grey: Vec<f32> = varied(8, 12).chunks_exact(3).flat_map(|pixel| [pixel[0]; 3]).collect();
    for (sigma, crawl, phase, edge) in [
        (4.0, 0.3, 0.0, Edge::Reflect),
        (20.0, 1.0, 0.3, Edge::Wrap),
        (0.0, 1.0, 0.77, Edge::Nearest),
        (2.5, 0.0, 0.5, Edge::Mirror),
    ] {
        let params = Params {
            sigma,
            crawl,
            phase,
            edge,
            ..Params::default()
        };
        assert_close(
            &run_picture(&params, 8, 12, &grey)?,
            &grey,
            1e-5,
            &format!("sigma {sigma}, crawl {crawl}, phase {phase}, {edge:?}"),
        );
    }
    Ok(())
}

/// Rows of 20 pixels: the first 8 one colour, the rest another, the colours different from row to row, all mild enough
/// for blurred colour to stay inside the gamut.
fn two_colours_per_row(rows: usize) -> Vec<f32> {
    (0..rows)
        .flat_map(|row| {
            let tilt = row as f32 * 0.03;
            (0..20).flat_map(move |column| {
                if column < 8 {
                    [0.7 - tilt, 0.45, 0.3 + tilt]
                } else {
                    [0.3 + tilt, 0.5, 0.6 - tilt]
                }
            })
        })
        .collect()
}

#[test]
fn colour_is_blurred_along_each_row_alone_while_brightness_is_kept() -> Result<(), Error> {
    let (rows, width) = (5, 20);
    let picture = two_colours_per_row(rows);
    let before = yiq_of(&picture);
    for edge in EDGES {
        for (sigma, truncate) in [(2.0, 4.0), (3.5, 2.0)] {
            let params = Params {
                sigma: sigma as f32,
                crawl: 0.0,
                truncate: truncate as f32,
                edge,
                ..Params::default()
            };
            let out = run_picture(&params, rows, width, &picture)?;
            let mut expected = before.clone();
            for row in 0..rows {
                for axis in 1..3 {
                    let line: Vec<f64> = (0..width).map(|column| before[row * width + column][axis]).collect();
                    for (column, value) in smoothed(&line, sigma, truncate, edge).into_iter().enumerate() {
                        expected[row * width + column][axis] = value;
                    }
                }
            }
            assert_yiq(&out, &expected, &format!("{edge:?}, sigma {sigma}, truncate {truncate}"));
        }
    }
    Ok(())
}

#[test]
fn the_crawl_adds_the_subcarrier_to_brightness_a_quarter_cycle_a_column_and_half_a_cycle_a_row() -> Result<(), Error> {
    let (rows, width) = (4, 9);
    let colour = [0.7_f32, 0.3, 0.4];
    let picture = colour.repeat(rows * width);
    let [luma, i, q] = yiq(&colour);
    // by hand: I = 0.5959 x 0.7 - 0.2746 x 0.3 - 0.3213 x 0.4 = 0.20623
    assert!((i - 0.206_23).abs() < 1e-5, "the test's own I");
    for phase in [0.0, 0.125, 0.4] {
        let params = Params {
            sigma: 0.0,
            crawl: 0.5,
            phase,
            ..Params::default()
        };
        let out = run_picture(&params, rows, width, &picture)?;
        let expected: Vec<[f64; 3]> = (0..rows * width)
            .map(|at| {
                let (row, column) = ((at / width) as f64, (at % width) as f64);
                let angle = std::f64::consts::TAU * (column / 4.0 + row / 2.0 + f64::from(phase));
                [luma + 0.5 * (i * angle.cos() + q * angle.sin()), i, q]
            })
            .collect();
        assert_yiq(&out, &expected, &format!("phase {phase}"));
    }
    Ok(())
}

#[test]
fn a_full_cycle_of_phase_is_none_and_half_a_cycle_turns_light_dots_to_dark() -> Result<(), Error> {
    let picture = [0.7_f32, 0.3, 0.4].repeat(3 * 8);
    let at = |phase| {
        run_picture(
            &Params {
                sigma: 0.0,
                crawl: 0.6,
                phase,
                ..Params::default()
            },
            3,
            8,
            &picture,
        )
    };
    let (start, half, full) = (at(0.0)?, at(0.5)?, at(1.0)?);
    assert_close(&full, &start, 1e-5, "phase 1 against phase 0");
    let (flat, shifted) = (yiq_of(&picture), yiq_of(&half));
    for ((from, one), other) in flat.iter().zip(yiq_of(&start)).zip(shifted) {
        // the two crawls cancel: the brightness is on either side of the picture's by the same amount
        assert!((one[0] + other[0] - 2.0 * from[0]).abs() < 3e-5);
    }
    Ok(())
}

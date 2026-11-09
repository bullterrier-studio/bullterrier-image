//! `reaction_diffusion`: the Gray-Scott model grown from the picture's bright parts. The oracles are one step worked by
//! hand on a single seeded cell, a naive scalar model written in the test (every edge rule, several steps), and
//! properties: a field with nothing to react to stays flat, a uniform field stays uniform, cells show as blocks.

#![expect(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the pictures are a few dozen px: every count and index is small"
)]

use bullterrier_image::effects::reaction_diffusion::Params;
use bullterrier_image::params::Color;
use bullterrier_image::{Edge, Error, Interpolation};

use crate::common::compare::assert_close;
use crate::common::pictures::{impulse, varied};
use crate::common::pixels::{channel, run_picture};

/// Black at 1 - u = 0 (nothing grown), white at 1 - u = 1: a pixel's grey is 1 - u.
fn greys() -> Vec<Color> {
    vec![Color([0.0; 3]), Color([1.0; 3])]
}

/// One simulation cell per pixel, read exactly: what a naive model on the pixel grid can be compared with.
fn exact() -> Params {
    Params {
        colors: greys(),
        cell: 1.0,
        resampling: Interpolation::Nearest,
        ..Params::default()
    }
}

fn run(params: &Params, height: usize, width: usize, picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_picture(params, height, width, picture)
}

/// A grey picture whose pixel at (`row`, `column`) is `bright(row, column)` in every channel.
fn grey_picture(height: usize, width: usize, bright: impl Fn(usize, usize) -> f32) -> Vec<f32> {
    (0..height * width)
        .flat_map(|at| [bright(at / width, at % width); 3])
        .collect()
}

#[test]
fn one_step_around_a_seeded_cell_is_worked_by_hand() -> Result<(), Error> {
    // u = 1/2, v = 1/4 at the seed, u = 1, v = 0 elsewhere. Five-point Laplacian of u: 4 x 1 - 4 x 1/2 = 2 at the seed,
    // 3 x 1 + 1/2 - 4 = -1/2 at a neighbour. u' = u + 0.2097 lap u - u v^2 + feed (1 - u).
    let params = Params {
        feed: 0.04,
        kill: 0.06,
        steps: 1,
        threshold: 0.5,
        ..exact()
    };
    let (height, width) = (5, 7);
    let out = channel(&run(&params, height, width, &impulse(height, width, 2, 3))?, 0);
    let seed = 1.0 - (0.5 + 0.2097 * 2.0 - 0.5 * 0.25 * 0.25 + 0.04 * 0.5);
    let neighbour = 1.0 - (1.0 + 0.2097 * -0.5);
    let mut expected = vec![0.0; height * width];
    expected[2 * width + 3] = seed;
    for at in [(1, 3), (3, 3), (2, 2), (2, 4)] {
        expected[at.0 * width + at.1] = neighbour;
    }
    assert_close(&out, &expected, 1e-6, "one step");
    Ok(())
}

#[test]
fn zero_steps_show_the_seed_read_at_each_cells_centre() -> Result<(), Error> {
    // cells of 4 px read the picture at pixel 4i + 2 (nearest); the right half of a 16 px picture is bright
    let params = Params {
        colors: greys(),
        cell: 4.0,
        steps: 0,
        resampling: Interpolation::Nearest,
        ..Params::default()
    };
    let picture = grey_picture(8, 16, |_, column| if column >= 8 { 0.9 } else { 0.1 });
    let out = channel(&run(&params, 8, 16, &picture)?, 0);
    let expected: Vec<f32> = (0..8 * 16).map(|at| if at % 16 >= 8 { 0.5 } else { 0.0 }).collect();
    assert_close(&out, &expected, 1e-6, "seed only");
    Ok(())
}

#[test]
fn a_threshold_nothing_exceeds_leaves_the_first_colour() -> Result<(), Error> {
    let params = Params {
        colors: greys(),
        threshold: 1.0,
        steps: 40,
        ..Params::default()
    };
    let out = run(&params, 24, 30, &grey_picture(24, 30, |_, _| 0.9))?;
    assert!(out.iter().all(|&level| level == 0.0));
    Ok(())
}

#[test]
fn a_uniform_field_stays_uniform_whatever_lies_beyond_the_edge() -> Result<(), Error> {
    // all seeded (u = 1/2, v = 1/4 everywhere): the Laplacian is 0 under every rule, so only the reaction acts and every
    // pixel stays alike
    for edge in [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap] {
        let params = Params {
            colors: greys(),
            edge,
            steps: 30,
            threshold: 0.2,
            ..Params::default()
        };
        let out = run(&params, 20, 28, &grey_picture(20, 28, |_, _| 0.9))?;
        let first = out[0];
        assert!(
            out.iter().all(|&level| (level - first).abs() < 1e-5),
            "{edge:?} made the uniform field uneven"
        );
    }
    Ok(())
}

#[test]
fn a_dark_picture_grows_nothing() -> Result<(), Error> {
    let params = Params {
        colors: greys(),
        steps: 60,
        ..Params::default()
    };
    let out = run(&params, 24, 30, &grey_picture(24, 30, |_, _| 0.1))?;
    assert!(out.iter().all(|&level| level == 0.0));
    Ok(())
}

/// The model on a `height` x `width` grid in plain f64: `seeded` cells start at (1/2, 1/4), the rest at (1, 0).
fn naive(
    seeded: &[bool],
    (height, width): (usize, usize),
    (feed, kill): (f64, f64),
    steps: usize,
    edge: Edge,
) -> Vec<f64> {
    // the cell a neighbour at `index` (-1 or the length included) is, written out for each rule
    let beyond = |index: isize, len: usize| -> usize {
        let last = len as isize - 1;
        let at = if index < 0 {
            match edge {
                Edge::Reflect | Edge::Nearest => 0,
                Edge::Mirror => 1.min(last),
                Edge::Wrap => last,
            }
        } else if index > last {
            match edge {
                Edge::Reflect | Edge::Nearest => last,
                Edge::Mirror => (last - 1).max(0),
                Edge::Wrap => 0,
            }
        } else {
            index
        };
        at as usize
    };
    let mut u: Vec<f64> = seeded.iter().map(|&on| if on { 0.5 } else { 1.0 }).collect();
    let mut v: Vec<f64> = seeded.iter().map(|&on| if on { 0.25 } else { 0.0 }).collect();
    for _ in 0..steps {
        let (mut next_u, mut next_v) = (u.clone(), v.clone());
        for row in 0..height {
            for column in 0..width {
                let around = |plane: &[f64]| {
                    let at = |r: isize, c: isize| plane[beyond(r, height) * width + beyond(c, width)];
                    let (r, c) = (row as isize, column as isize);
                    at(r - 1, c) + at(r + 1, c) + at(r, c - 1) + at(r, c + 1) - 4.0 * plane[row * width + column]
                };
                let here = row * width + column;
                let reaction = u[here] * v[here] * v[here];
                next_u[here] = u[here] + 0.2097 * around(&u) - reaction + feed * (1.0 - u[here]);
                next_v[here] = v[here] + 0.105 * around(&v) + reaction - (feed + kill) * v[here];
            }
        }
        (u, v) = (next_u, next_v);
    }
    u
}

#[test]
fn several_steps_match_a_naive_model_under_every_edge_rule() -> Result<(), Error> {
    let (height, width) = (9, 11);
    let picture = varied(height, width);
    // the brightness of each pixel, Rec. 709
    let seeded: Vec<bool> = picture
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| f64::from(p[0]) * 0.2126 + f64::from(p[1]) * 0.7152 + f64::from(p[2]) * 0.0722 > 0.6)
        .collect();
    for edge in [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap] {
        let params = Params {
            feed: 0.055,
            kill: 0.062,
            steps: 9,
            threshold: 0.6,
            edge,
            ..exact()
        };
        let out = channel(&run(&params, height, width, &picture)?, 0);
        let expected: Vec<f32> = naive(&seeded, (height, width), (0.055, 0.062), 9, edge)
            .into_iter()
            .map(|u| (1.0 - u) as f32)
            .collect();
        assert_close(&out, &expected, 1e-4, &format!("{edge:?}"));
    }
    Ok(())
}

#[test]
fn cells_show_as_blocks_when_read_back_nearest() -> Result<(), Error> {
    let params = Params {
        colors: greys(),
        cell: 4.0,
        steps: 12,
        threshold: 0.4,
        resampling: Interpolation::Nearest,
        ..Params::default()
    };
    let out = channel(&run(&params, 16, 16, &varied(16, 16))?, 0);
    for block in 0..16 {
        let (top, left) = (block / 4 * 4, block % 4 * 4);
        let first = out[top * 16 + left];
        for (row, column) in (0..16).map(|at| (at / 4, at % 4)) {
            assert!(
                (out[(top + row) * 16 + left + column] - first).abs() < 1e-6,
                "block {block} is not one value"
            );
        }
    }
    Ok(())
}

#[test]
fn a_cell_larger_than_the_picture_gives_one_flat_value() -> Result<(), Error> {
    let params = Params {
        colors: greys(),
        cell: 64.0,
        steps: 5,
        threshold: 0.3,
        ..Params::default()
    };
    let out = channel(&run(&params, 20, 30, &varied(20, 30))?, 0);
    assert!(out.iter().all(|&level| (level - out[0]).abs() < 1e-6));
    Ok(())
}

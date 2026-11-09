//! `edges`: the picture's edges drawn in a colour. The oracle is a naive reference written in the test from the
//! definitions (a Gaussian from its weights, Sobel from its 3x3 kernels, Canny's suppression by the angle of the gradient
//! and its hysteresis by a flood fill), plus values worked out by hand for a step.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "small picture sizes and indices"
)]

use std::collections::VecDeque;

use bullterrier_image::effects::edges::{Method, Params};
use bullterrier_image::params::Color;
use bullterrier_image::{Edge, Error};

use crate::common::compare::assert_close;
use crate::common::edge::folded;
use crate::common::gaussian::weights;
use crate::common::pictures::varied;
use crate::common::pixels::{is_pixel, run_picture};

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
const METHODS: [Method; 3] = [Method::Sobel, Method::Canny, Method::Dog];
/// The ratio of the two Gaussians of `dog` (Marr and Hildreth), written out here.
const RATIO: f64 = 1.6;

/// Grey values as a picture of three equal channels.
fn grey_picture(grey: &[f32]) -> Vec<f32> {
    grey.iter().flat_map(|&level| [level; 3]).collect()
}

/// The red channel of `params` applied to a grey picture: white edges on black unless the params say otherwise.
fn drawn(params: &Params, height: usize, width: usize, grey: &[f32]) -> Result<Vec<f32>, Error> {
    let out = run_picture(params, height, width, &grey_picture(grey))?;
    Ok(out.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect())
}

/// A picture with a bright disc, some texture and no symmetry (so no two neighbours tie).
fn scene(height: usize, width: usize) -> Vec<f32> {
    let texture = varied(height, width);
    (0..height * width)
        .map(|at| {
            let (row, column) = ((at / width) as f32, (at % width) as f32);
            let inside = (row - 8.0).powi(2) + (column - 11.0).powi(2) < 30.0;
            0.25 + if inside { 0.5 } else { 0.0 } + 0.12 * texture[at * 3]
        })
        .collect()
}

/// Black on the left, white from column `at` on, `height` rows.
fn step_rows(height: usize, width: usize, at: usize) -> Vec<f32> {
    (0..height * width)
        .map(|index| f32::from(index % width >= at))
        .collect()
}

// --- the reference -------------------------------------------------------------------------------------------------

/// `plane` blurred by the Gaussian, summed over every tap of the 2D kernel.
fn blurred(plane: &[f64], (height, width): (usize, usize), sigma: f64, truncate: f64, edge: Edge) -> Vec<f64> {
    let (kernel, reach) = weights(sigma, truncate);
    let (reach, h, w) = (reach as i64, height as i64, width as i64);
    (0..height * width)
        .map(|at| {
            let (row, column) = ((at / width) as i64, (at % width) as i64);
            let mut sum = 0.0;
            for dr in -reach..=reach {
                for dc in -reach..=reach {
                    let (r, c) = (folded(edge, row + dr, h), folded(edge, column + dc, w));
                    sum += kernel[(dr + reach) as usize] * kernel[(dc + reach) as usize] * plane[r * width + c];
                }
            }
            sum
        })
        .collect()
}

/// The Sobel gradient (right minus left, down minus up) of `plane`, in value per px, from the 3x3 kernels.
fn sobel(plane: &[f64], (height, width): (usize, usize), edge: Edge) -> (Vec<f64>, Vec<f64>) {
    const ACROSS: [[f64; 3]; 3] = [[-1.0, 0.0, 1.0], [-2.0, 0.0, 2.0], [-1.0, 0.0, 1.0]];
    let (h, w) = (height as i64, width as i64);
    let (mut gx, mut gy) = (vec![0.0; height * width], vec![0.0; height * width]);
    for at in 0..height * width {
        let (row, column) = ((at / width) as i64, (at % width) as i64);
        for (i, kernel_row) in ACROSS.iter().enumerate() {
            for (j, &weight) in kernel_row.iter().enumerate() {
                let value = |dr: i64, dc: i64| plane[folded(edge, row + dr, h) * width + folded(edge, column + dc, w)];
                gx[at] += weight / 8.0 * value(i as i64 - 1, j as i64 - 1);
                gy[at] += weight / 8.0 * value(j as i64 - 1, i as i64 - 1);
            }
        }
    }
    (gx, gy)
}

/// The strongest the method reads on a step from black to white at this radius: found by running the reference on one.
fn steepest(params: &Params) -> f64 {
    let (radius, truncate) = (f64::from(params.radius), f64::from(params.truncate));
    let reach = (truncate * radius * RATIO + 0.5) as usize + 3;
    let dims = (3, 2 * reach);
    let step: Vec<f64> = (0..3 * 2 * reach)
        .map(|at| f64::from(u8::from(at % (2 * reach) >= reach)))
        .collect();
    let narrow = blurred(&step, dims, radius, truncate, Edge::Nearest);
    if params.method == Method::Dog {
        let wide = blurred(&step, dims, radius * RATIO, truncate, Edge::Nearest);
        return wide
            .iter()
            .zip(&narrow)
            .map(|(wide, narrow)| wide - narrow)
            .fold(0.0, f64::max);
    }
    let (gx, gy) = sobel(&narrow, dims, Edge::Nearest);
    gx.iter().zip(&gy).map(|(x, y)| x.hypot(*y)).fold(0.0, f64::max)
}

/// How fully each pixel of the grey picture is drawn, by the definition.
fn reference(params: &Params, (height, width): (usize, usize), grey: &[f32]) -> Vec<f64> {
    let (radius, truncate) = (f64::from(params.radius), f64::from(params.truncate));
    let plane: Vec<f64> = grey.iter().map(|&level| f64::from(level)).collect();
    let narrow = blurred(&plane, (height, width), radius, truncate, params.edge);
    let (mut gx, mut gy) = (Vec::new(), Vec::new());
    let steepness: Vec<f64> = if params.method == Method::Dog {
        let wide = blurred(&plane, (height, width), radius * RATIO, truncate, params.edge);
        wide.iter()
            .zip(&narrow)
            .map(|(wide, narrow)| (wide - narrow) / steepest(params))
            .collect()
    } else {
        (gx, gy) = sobel(&narrow, (height, width), params.edge);
        gx.iter()
            .zip(&gy)
            .map(|(x, y)| x.hypot(*y) / steepest(params))
            .collect()
    };
    let (threshold, softness) = (f64::from(params.threshold), f64::from(params.softness));
    let (lower, upper) = (threshold * (1.0 - softness), threshold * (1.0 + softness));
    let ramp = |steep: f64| {
        if upper > lower {
            ((steep - lower) / (upper - lower)).clamp(0.0, 1.0)
        } else {
            f64::from(u8::from(steep > upper))
        }
    };
    let mut out: Vec<f64> = steepness.iter().map(|&steep| ramp(steep)).collect();
    if params.method == Method::Canny {
        let kept = flood(
            &crest_flags(&steepness, &gx, &gy, (height, width), lower),
            &steepness,
            upper,
            (height, width),
        );
        for (value, kept) in out.iter_mut().zip(kept) {
            *value *= f64::from(u8::from(kept));
        }
    }
    out
}

/// The pixels where the steepness is at its crest across the edge and above `lower`: by the angle of the gradient, rounded
/// to the nearest of eight directions; nothing lies beyond the picture.
fn crest_flags(steepness: &[f64], gx: &[f64], gy: &[f64], (height, width): (usize, usize), lower: f64) -> Vec<bool> {
    let strength_at = |row: i64, column: i64| {
        if (0..height as i64).contains(&row) && (0..width as i64).contains(&column) {
            steepness[row as usize * width + column as usize]
        } else {
            0.0
        }
    };
    (0..height * width)
        .map(|at| {
            let (row, column) = ((at / width) as i64, (at % width) as i64);
            let octant = (gy[at].atan2(gx[at]) / std::f64::consts::FRAC_PI_4).round_ties_even() as i64;
            let (down, across) = [(0, 1), (1, 1), (1, 0), (1, -1)][octant.rem_euclid(4) as usize];
            let strength = steepness[at];
            strength > lower
                && strength >= strength_at(row + down, column + across)
                && strength > strength_at(row - down, column - across)
        })
        .collect()
}

/// The crests that are above `upper`, or reached from one through crests (diagonals too), by a flood fill.
fn flood(crests: &[bool], steepness: &[f64], upper: f64, (height, width): (usize, usize)) -> Vec<bool> {
    let mut kept = vec![false; height * width];
    let mut queue: VecDeque<usize> = (0..height * width)
        .filter(|&at| crests[at] && steepness[at] > upper)
        .collect();
    for &at in &queue {
        kept[at] = true;
    }
    while let Some(at) = queue.pop_front() {
        let (row, column) = ((at / width) as i64, (at % width) as i64);
        for (dr, dc) in (-1..=1).flat_map(|dr| (-1..=1).map(move |dc| (dr, dc))) {
            let (r, c) = (row + dr, column + dc);
            if (0..height as i64).contains(&r) && (0..width as i64).contains(&c) {
                let next = r as usize * width + c as usize;
                if crests[next] && !kept[next] {
                    kept[next] = true;
                    queue.push_back(next);
                }
            }
        }
    }
    kept
}

// --- the properties ------------------------------------------------------------------------------------------------

#[test]
fn every_method_edge_rule_and_radius_draws_what_the_definitions_say() -> Result<(), Error> {
    let (height, width) = (20, 23);
    let grey = scene(height, width);
    for method in METHODS {
        for edge in EDGES {
            for (radius, truncate, threshold, softness) in
                [(0.5, 1.0, 0.1, 0.5), (1.5, 4.0, 0.05, 0.9), (2.2, 2.5, 0.2, 0.0)]
            {
                let params = Params {
                    method,
                    edge,
                    radius,
                    truncate,
                    threshold,
                    softness,
                    ..Params::default()
                };
                let expected: Vec<f32> = reference(&params, (height, width), &grey)
                    .iter()
                    .map(|&value| value as f32)
                    .collect();
                let context = format!(
                    "{method:?} {edge:?} radius {radius} truncate {truncate} threshold {threshold} softness {softness}"
                );
                assert!(
                    expected.iter().any(|&value| value > 0.0),
                    "{context}: the picture has edges"
                );
                assert_close(&drawn(&params, height, width, &grey)?, &expected, 1e-4, &context);
            }
        }
    }
    Ok(())
}

#[test]
fn a_flat_picture_is_all_background_and_an_edge_is_in_the_edge_colour() -> Result<(), Error> {
    let (height, width) = (12, 15);
    for method in METHODS {
        let params = Params {
            method,
            color: Color([1.0, 0.0, 0.5]),
            background: Color([0.0, 0.25, 0.5]),
            threshold: 0.01,
            ..Params::default()
        };
        let flat = run_picture(&params, height, width, &vec![0.4; height * width * 3])?;
        assert!(
            flat.as_chunks::<3>()
                .0
                .iter()
                .all(|pixel| is_pixel(*pixel, [0.0, 0.25, 0.5])),
            "{method:?}"
        );
        let marked = run_picture(&params, height, width, &grey_picture(&step_rows(height, width, 8)))?;
        assert!(
            marked
                .as_chunks::<3>()
                .0
                .iter()
                .any(|pixel| is_pixel(*pixel, [1.0, 0.0, 0.5])),
            "{method:?}: the step is drawn in full in the colour"
        );
    }
    Ok(())
}

#[test]
fn a_step_reads_as_hand_worked_for_sobel_and_canny_keeps_one_pixel_of_it() -> Result<(), Error> {
    // radius 0.5 cut at 1 sigma is the three taps [a, b, a] with a = e^-2 / (1 + 2 e^-2): the step from black to white at
    // column s blurs to a at s - 1, 1 - a at s and 0 or 1 elsewhere, so the Sobel steepness (right minus left over 2) is
    // (1 - a) / 2 at s - 1 and s, the most there is (read as 1), and a / 2 at s - 2 and s + 1: a / (1 - a) of it. With
    // threshold 1 and softness 1 the ramp is steepness / 2.
    let (height, width, at) = (7, 30, 15);
    let a = (-2.0_f64).exp() / (1.0 + 2.0 * (-2.0_f64).exp());
    let side = a / (1.0 - a) / 2.0;
    let grey = step_rows(height, width, at);
    let sobel = Params {
        method: Method::Sobel,
        radius: 0.5,
        truncate: 1.0,
        threshold: 1.0,
        softness: 1.0,
        ..Params::default()
    };
    let out = drawn(&sobel, height, width, &grey)?;
    for row in 0..height {
        let line = &out[row * width..][..width];
        for (column, &value) in line.iter().enumerate() {
            let expected = match column {
                c if c == at - 1 || c == at => 0.5,
                c if c == at - 2 || c == at + 1 => side,
                _ => 0.0,
            };
            assert!(
                (f64::from(value) - expected).abs() < 1e-5,
                "row {row} column {column}: {value} against {expected}"
            );
        }
    }
    // the two equal pixels of the crest make one line: of two equal neighbours one stays (which one is the last bit's)
    let canny = Params {
        method: Method::Canny,
        radius: 0.5,
        truncate: 1.0,
        ..Params::default()
    };
    let lines = drawn(&canny, height, width, &grey)?;
    for row in lines.chunks_exact(width) {
        let lit: Vec<usize> = (0..width).filter(|&column| row[column] > 0.0).collect();
        assert!(lit == [at - 1] || lit == [at], "{lit:?}");
        assert!((row[lit[0]] - 1.0).abs() < 1e-5);
    }
    Ok(())
}

#[test]
fn dog_draws_one_line_on_the_dark_side_of_a_step_and_on_dark_strokes_only() -> Result<(), Error> {
    let (height, width, at) = (5, 60, 30);
    let params = Params {
        method: Method::Dog,
        radius: 1.5,
        threshold: 0.3,
        ..Params::default()
    };
    let out = drawn(&params, height, width, &step_rows(height, width, at))?;
    let row = &out[2 * width..][..width];
    let lit: Vec<usize> = (0..width).filter(|&column| row[column] > 0.0).collect();
    assert!(
        row.iter().any(|&value| (value - 1.0).abs() < 1e-5),
        "somewhere fully drawn"
    );
    assert!(lit.windows(2).all(|pair| pair[1] == pair[0] + 1), "one run");
    assert!(lit.iter().all(|&column| column < at), "only the dark side");
    assert!(
        lit.iter().all(|&column| column + 12 >= at),
        "within the reach of the Gaussians (4 x 1.5 x 1.6 px and 3 more)"
    );
    let dark_line: Vec<f32> = (0..height * width)
        .map(|index| if index % width == 30 { 0.0 } else { 1.0 })
        .collect();
    let light_line: Vec<f32> = dark_line.iter().map(|level| 1.0 - level).collect();
    let on_light = drawn(&params, height, width, &dark_line)?;
    let on_dark = drawn(&params, height, width, &light_line)?;
    assert!(on_light[2 * width + 30] > 0.9, "a dark line on light is drawn");
    assert!(
        on_dark[2 * width + 30].abs() < f32::EPSILON,
        "a light line on dark is not: only the dark ground beside it is"
    );
    Ok(())
}

/// A vertical step at column 8 of a picture `height` rows high whose contrast follows `contrast` row by row.
fn fading_step(contrast: &[f32]) -> Vec<f32> {
    contrast
        .iter()
        .flat_map(|&level| (0..24).map(move |column| if column >= 8 { level } else { 0.0 }))
        .collect()
}

/// The `n` values from `from` to `to`, evenly.
fn linear(from: f32, to: f32, n: usize) -> impl Iterator<Item = f32> {
    (0..n).map(move |index| from + (to - from) * index as f32 / (n - 1) as f32)
}

#[test]
fn canny_keeps_a_weak_line_only_where_it_joins_a_strong_one() -> Result<(), Error> {
    // contrast is steepness for radius 1: the thresholds are 0.05 and 0.15 and 0.12 is between them
    let params = |method| Params {
        method,
        radius: 1.0,
        ..Params::default()
    };
    let lit = |out: &[f32], rows: std::ops::Range<usize>| {
        rows.into_iter()
            .any(|row| out[row * 24 + 7] > 0.0 || out[row * 24 + 8] > 0.0)
    };
    // 0.12 all the way down: nothing strong to join, though sobel draws it
    let weak = fading_step(&[0.12; 60]);
    assert!(
        !lit(&drawn(&params(Method::Canny), 60, 24, &weak)?, 5..55),
        "alone, a weak line goes"
    );
    assert!(
        lit(&drawn(&params(Method::Sobel), 60, 24, &weak)?, 5..55),
        "sobel draws it"
    );
    // from 1 down to 0.12: joined to its strong top, the weak end stays
    let fading: Vec<f32> = linear(1.0, 0.12, 60).collect();
    assert!(
        lit(&drawn(&params(Method::Canny), 60, 24, &fading_step(&fading))?, 50..55),
        "joined, it stays"
    );
    // strong, then a stretch under the lower threshold, then weak: the gap cuts the weak part off
    let gapped: Vec<f32> = [
        vec![1.0; 12],
        linear(1.0, 0.02, 12).collect(),
        vec![0.02; 12],
        linear(0.02, 0.12, 12).collect(),
        vec![0.12; 24],
    ]
    .concat();
    let out = drawn(&params(Method::Canny), gapped.len(), 24, &fading_step(&gapped))?;
    assert!(lit(&out, 2..10), "the strong top is drawn");
    assert!(!lit(&out, 60..70), "the weak end is cut off");
    Ok(())
}

#[test]
fn softness_zero_draws_each_pixel_fully_or_not_at_all_and_a_higher_threshold_never_draws_more() -> Result<(), Error> {
    let (height, width) = (20, 23);
    let grey = scene(height, width);
    for method in METHODS {
        let hard = drawn(
            &Params {
                method,
                softness: 0.0,
                ..Params::default()
            },
            height,
            width,
            &grey,
        )?;
        assert!(
            hard.iter().all(|&value| value == 0.0 || (value - 1.0).abs() < 1e-6),
            "{method:?}"
        );
        let low = drawn(
            &Params {
                method,
                threshold: 0.05,
                ..Params::default()
            },
            height,
            width,
            &grey,
        )?;
        let high = drawn(
            &Params {
                method,
                threshold: 0.6,
                ..Params::default()
            },
            height,
            width,
            &grey,
        )?;
        assert!(
            low.iter().zip(&high).all(|(low, high)| high <= &(low + 1e-6)),
            "{method:?}: never more"
        );
        assert!(
            high.iter().sum::<f32>() < low.iter().sum::<f32>(),
            "{method:?}: and fewer somewhere"
        );
    }
    Ok(())
}

#[test]
fn canny_is_much_thinner_than_sobel() -> Result<(), Error> {
    let (height, width) = (20, 23);
    let grey = scene(height, width);
    let count = |method| -> Result<usize, Error> {
        Ok(drawn(
            &Params {
                method,
                ..Params::default()
            },
            height,
            width,
            &grey,
        )?
        .iter()
        .filter(|&&value| value > 0.0)
        .count())
    };
    let (canny, sobel) = (count(Method::Canny)?, count(Method::Sobel)?);
    assert!(canny > 0 && canny * 10 < sobel * 6, "{canny} against {sobel}");
    Ok(())
}

#[test]
fn a_wrapped_picture_has_a_shifted_edge_map_when_shifted_and_a_turned_one_when_turned() -> Result<(), Error> {
    let (height, width) = (16, 16);
    let grey = scene(height, width);
    let rolled: Vec<f32> = (0..height * width)
        .map(|at| grey[at / width * width + (at % width + 5) % width])
        .collect();
    let turned: Vec<f32> = (0..height * width)
        .map(|at| grey[(at % width) * width + (width - 1 - at / width)])
        .collect();
    for method in [Method::Sobel, Method::Dog] {
        let params = Params {
            method,
            edge: Edge::Wrap,
            ..Params::default()
        };
        let plain = drawn(&params, height, width, &grey)?;
        let shifted: Vec<f32> = (0..height * width)
            .map(|at| plain[at / width * width + (at % width + 5) % width])
            .collect();
        assert_close(
            &drawn(&params, height, width, &rolled)?,
            &shifted,
            1e-4,
            &format!("{method:?} rolled"),
        );
        // turning the picture a quarter turn to the left turns its edges the same way
        let expected: Vec<f32> = (0..height * width)
            .map(|at| plain[(at % width) * width + (width - 1 - at / width)])
            .collect();
        assert_close(
            &drawn(&params, height, width, &turned)?,
            &expected,
            1e-4,
            &format!("{method:?} turned"),
        );
    }
    Ok(())
}

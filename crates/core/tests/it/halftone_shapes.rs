//! `halftone`'s dot shapes: every shape keeps the tone, and each has its own geometry. See `halftone.rs` for the oracles.

#![expect(clippy::cast_precision_loss, reason = "small picture sizes")]

use std::f32::consts::PI;

use bullterrier_image::effects::halftone::{Params, Shape};
use bullterrier_image::params::{Declared, PictureName};
use bullterrier_image::{Error, Image, Inputs, Picture, apply};
use serde_json::Value;

use crate::halftone::*;

// ---- dot shapes -------------------------------------------------------------------------------------------------------

/// The screens of every new shape, each as the coverage oracle runs it (the ellipse at aspect 2 and the matrix at a picture
/// of its own, see `coverage_of_a_matrix`).
fn new_shapes() -> Vec<(&'static str, Params)> {
    vec![
        ("square", oracle(Shape::Square)),
        ("diamond", oracle(Shape::Diamond)),
        ("euclidean", oracle(Shape::Euclidean)),
        (
            "ellipse 2",
            Params {
                aspect: 2.0,
                ..oracle(Shape::Ellipse)
            },
        ),
        (
            "ellipse 4",
            Params {
                aspect: 4.0,
                ..oracle(Shape::Ellipse)
            },
        ),
    ]
}

#[test]
fn ink_grows_with_tone_in_every_new_shape() -> Result<(), Error> {
    for (name, params) in new_shapes() {
        let mut last = -1.0;
        for tone in [0.0, 0.001, 0.01, 0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let ink = coverage_with(&params, tone)?;
            assert!(ink > last, "{name} {tone}: {ink} after {last}");
            last = ink;
        }
    }
    Ok(())
}

#[test]
fn every_new_shape_keeps_the_tone() -> Result<(), Error> {
    // A straight edge keeps its area (the ramp is centred on it and a pixel row samples it at whole pixels). A curve gains
    // pi f^2 / 12 px^2 a dot at feather f = 1 where the ramp is a pixel wide (see `a_dot_prints_about_its_own_area_however_small`),
    // 0.0013 of a cell of 20 px; a value that changes only 0.7 a cell at the edge (the diamond's diagonals, the cosine
    // screen's) widens the ramp by 1 / 0.7 and so doubles that gain, 0.0026. The level is read off a grid of 65536 places, within
    // 0.0003 of tone. About 0.003 together, and the cosine screen's flat saddles at the tips of its diamond, where the value does
    // not change and the ramp spreads, add a little more (measured 0.0041 at most): the bound is 0.005.
    for (name, params) in new_shapes() {
        for tone in [0.1, 0.25, 0.5, 0.75, 0.9] {
            let ink = coverage_with(&params, tone)?;
            assert!((ink - tone).abs() < 0.005, "{name} tone {tone}: ink {ink}");
        }
    }
    Ok(())
}

#[test]
fn a_square_at_full_tone_fills_its_cell_and_at_a_quarter_is_a_centred_half_cell_square() -> Result<(), Error> {
    let params = oracle(Shape::Square);
    assert!(ink_map(&params, 1.0)?.iter().all(|&ink| ink > 1.0 - 1e-6));
    // side sqrt(tone) cells: 10 px of a cell of 20 (pixels 5..15), its edges on whole pixels
    let ink = ink_map(&params, 0.25)?;
    for (x, y) in [(10, 10), (6, 6), (14, 14), (6, 14), (14, 6), (26, 26)] {
        assert!(ink[y * 80 + x] > 0.99, "({x}, {y}) is inside");
    }
    for (x, y) in [(0, 0), (4, 10), (10, 4), (16, 10), (10, 16), (19, 19), (20, 20)] {
        assert!(ink[y * 80 + x] < 0.01, "({x}, {y}) is outside");
    }
    Ok(())
}

#[test]
fn a_diamond_at_half_tone_touches_its_neighbours_at_the_middle_of_the_cell_sides() -> Result<(), Error> {
    // |du| + |dv| < 1 / 2 is a diamond of area 1 / 2 whose tips are the middles of the cell's sides: the diamond at (x, y)
    // = (10, 10) has its right tip at (20, 10) where the next diamond's left tip is
    let params = oracle(Shape::Diamond);
    let touching = ink_map(&params, 0.5)?;
    let apart = ink_map(&params, 0.4)?;
    let past = ink_map(&params, 0.6)?;
    let at = |map: &[f32], x: usize, y: usize| map[y * 80 + x];
    // the tip pixels on the side between two cells (x = 19 is in the left cell, 20 in the right), and a corner
    assert!(
        at(&touching, 19, 10) > 0.9 && at(&touching, 20, 10) > 0.4,
        "the tips meet"
    );
    assert!(at(&apart, 20, 10) < 0.01, "a smaller diamond leaves a gap");
    assert!(
        at(&past, 20, 10) > 0.99 && at(&past, 19, 10) > 0.99,
        "a larger one overlaps"
    );
    assert!(
        at(&touching, 20, 20) < 0.01 && at(&touching, 19, 19) < 0.01,
        "the corners stay paper"
    );
    // and the centre is ink, the square's own side middle a little further out is the paper between diamonds
    assert!(at(&touching, 10, 10) > 0.99);
    Ok(())
}

#[test]
fn an_ellipse_of_aspect_one_is_the_round_dot() -> Result<(), Error> {
    let round = oracle(Shape::Dot);
    let ellipse = Params {
        aspect: 1.0,
        ..oracle(Shape::Ellipse)
    };
    for tone in [0.02, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
        let (want, got) = (ink_map(&round, tone)?, ink_map(&ellipse, tone)?);
        // a level read off a grid differs from the circle's by 0.0003 of tone, 0.0003 / (2 pi r) cells at a dot of radius
        // r cells (the smallest here, 0.08): 0.0006 cells, 0.012 px, a coverage of that much
        let worst = want.iter().zip(&got).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(worst < 0.01, "tone {tone}: pixels differ by {worst}");
    }
    Ok(())
}

#[test]
fn an_ellipse_is_aspect_times_as_long_along_the_rows_as_across_them() -> Result<(), Error> {
    for aspect in [2.0_f32, 4.0] {
        let params = Params {
            aspect,
            ..oracle(Shape::Ellipse)
        };
        // the centre cell's ink along its middle row and its middle column (pixels 10, centred on the dot at 10)
        let ink = ink_map(&params, 0.05)?;
        let along: f32 = (0..20).map(|x| ink[10 * 80 + x]).sum();
        let across: f32 = (0..20).map(|y| ink[y * 80 + 10]).sum();
        // axes r sqrt a and r / sqrt a; the ramp adds about a pixel to each length, so allow 1.2 px on the ratio's terms
        let want = across * aspect;
        assert!(
            (along - want).abs() < 1.5 * aspect,
            "aspect {aspect}: {along} along, {across} across"
        );
        assert!(along > across * 1.5, "aspect {aspect}");
    }
    Ok(())
}

#[test]
fn a_euclidean_dot_is_round_in_highlights_half_the_cell_at_half_and_mirrors_about_half() -> Result<(), Error> {
    let params = oracle(Shape::Euclidean);
    // highlights: round but for the cosine's fourth-order squareness (the radius differs by about 10 % between axis and
    // diagonal at this size), of the radius that holds the tone (0.1: 3.57 px of 20); inside 0.85 of it all ink, past 1.25 none
    let ink = ink_map(&params, 0.1)?;
    let radius = (0.1_f32 / PI).sqrt() * 20.0;
    for y in 0..20 {
        for x in 0..20 {
            let distance = ((x as f32 - 10.0).powi(2) + (y as f32 - 10.0).powi(2)).sqrt();
            if distance < 0.85 * radius {
                assert!(ink[y * 80 + x] > 0.99, "({x}, {y}) inside");
            } else if distance > 1.25 * radius {
                assert!(ink[y * 80 + x] < 0.01, "({x}, {y}) outside");
            }
        }
    }
    // the ink lights half at 50 %
    assert!((coverage_with(&params, 0.5)? - 0.5).abs() < 0.005);
    // swapping ink and paper about 50 %: the picture of tone 1 - t is the paper of tone t, half a cell on in both ways
    for tone in [0.1, 0.3, 0.5] {
        let (a, b) = (ink_map(&params, tone)?, ink_map(&params, 1.0 - tone)?);
        for y in 0..70 {
            for x in 0..70 {
                let (here, there) = (a[y * 80 + x], 1.0 - b[(y + 10) * 80 + x + 10]);
                assert!(
                    (here - there).abs() < 0.01,
                    "tone {tone} ({x}, {y}): {here} against {there}"
                );
            }
        }
    }
    Ok(())
}

/// The round dot's spot value at each pixel of one cell of `side` px: the picture whose ranking is the dot's, the
/// centre brightest (the brightest cells are inked first).
fn dot_matrix(side: usize) -> Result<Image, Error> {
    let greys: Vec<f32> = (0..side * side)
        .flat_map(|index| {
            let (x, y) = (
                (index % side) as f32 / side as f32 - 0.5,
                (index / side) as f32 / side as f32 - 0.5,
            );
            let distance = (x * x + y * y).sqrt();
            [1.0 - distance; 3]
        })
        .collect();
    Image::new(side, side, &greys)
}

/// The ink map of the oracle screen with shape 'matrix' and the cell picture `matrix`.
fn matrix_ink(matrix: &Image, tone: f32) -> Result<Vec<f32>, Error> {
    let params = Params {
        matrix: PictureName::new("m"),
        ..oracle(Shape::Matrix)
    };
    let values = flat(80, 80, 1.0 - tone);
    let out = apply(
        "halftone",
        &Picture::new(80, 80, &values)?,
        &Value::Object(params.values()),
        &Inputs::default().with_image("matrix", matrix),
    )?;
    Ok(out
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| 1.0 - pixel[0])
        .collect())
}

#[test]
fn a_matrix_of_the_round_dots_values_reproduces_the_round_dot() -> Result<(), Error> {
    // a picture of 20 x 20, one place a pixel of the cell, ranks the same pixels as the dot's own distance does; the dot
    // inks the places nearer its centre than the radius, the matrix its k nearest of 400 (ties in reading order), and
    // its soft edge is a ramp of 1 / 400 of tone a rank, 0.05 of a pixel's coverage: the two differ at the edge pixels
    // alone (a ring about 2 pi r px, up to half a coverage each) and the tone each keeps is within 1 / 400
    let matrix = dot_matrix(20)?;
    let round = oracle(Shape::Dot);
    for tone in [0.1, 0.25, 0.5, 0.75] {
        let (want, got) = (ink_map(&round, tone)?, matrix_ink(&matrix, tone)?);
        let mean = |map: &[f32]| map.iter().sum::<f32>() / map.len() as f32;
        assert!(
            (mean(&want) - mean(&got)).abs() < 0.005,
            "tone {tone}: {} against {}",
            mean(&want),
            mean(&got)
        );
        let ring = 2.0 * PI * (tone / PI).sqrt() * 20.0; // px of edge a cell
        let bound = 0.5 * ring / 400.0; // half a coverage each, at most
        let mean_difference = want.iter().zip(&got).map(|(a, b)| (a - b).abs()).sum::<f32>() / want.len() as f32;
        assert!(
            mean_difference < bound,
            "tone {tone}: pixels differ by {mean_difference} (bound {bound})"
        );
    }
    Ok(())
}

#[test]
fn a_matrix_inks_a_tone_of_k_of_its_n_places_brightest_first_with_ties_in_reading_order() -> Result<(), Error> {
    // 2 x 2 places of a cell of 20 px: greys 0.2 0.9 / 0.5 0.5 rank (bright first) top right, then the two 0.5 in reading
    // order (bottom left, bottom right), then the top left; a flat tone of k / 4 inks k places, whole blocks of 10 x 10 px
    // but for the soft edge of 0.5 / 4 * 20 = 2.5 of coverage's ramp, so the middle pixel of each block is whole
    let matrix = Image::new(
        2,
        2,
        &[0.2, 0.9, 0.5, 0.5].iter().flat_map(|&g| [g; 3]).collect::<Vec<_>>(),
    )?;
    let block = |ink: &[f32], column: usize, row: usize| ink[(row * 10 + 5) * 80 + column * 10 + 5];
    for (tone, inked) in [
        (0.25, [[0, 1], [0, 0]]),
        (0.5, [[0, 1], [1, 0]]),
        (0.75, [[0, 1], [1, 1]]),
        (1.0, [[1, 1], [1, 1]]),
    ] {
        let ink = matrix_ink(&matrix, tone)?;
        for (row, line) in inked.iter().enumerate() {
            for (column, &want) in line.iter().enumerate() {
                assert!(
                    (block(&ink, column, row) - want as f32).abs() < 0.01,
                    "tone {tone} place ({column}, {row})"
                );
            }
        }
    }
    Ok(())
}

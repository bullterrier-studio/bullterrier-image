//! The soft borders of an `each` step, seen through its picture: a look is mixed in by a weight map, the pieces that have it
//! blurred by a Gaussian of `feather` design px (a sigma of `feather` x longest side / 480 px). A photo of grey 0 under a
//! look that paints 1 and a look that changes nothing shows that weight map itself, so the blur is checked against a
//! brute-force Gaussian over the pieces' indicator, written here in double precision.
//!
//! A sigma can be at most 24 x longest side / 480, a twentieth of the longest side, so a blur is never wider than the map.

use crate::common::Outcome;
use crate::common::gaussian;
use crate::common::numbers::{float, total};
use crate::common::ops::level;
use bullterrier_image::region::{Grid, Shares};
use bullterrier_image::{Each, Error, Options, Photo, Picture, Presets, Recipe};

/// The weight map of the pieces of `grid` that take the painting look (every other piece changes nothing), for a feather
/// of `feather` design px, on a grey-0 photo of `height` x `width` pixels: the picture the step makes.
fn weights(height: usize, width: usize, grid: Grid, feather: f64) -> Result<Vec<f32>, Error> {
    let mut each = Each::new("cells", [Some(level(1.0)?), None]);
    each.feather = feather;
    let recipe = Recipe::new([each]).with_region("cells", grid);
    let values = vec![0.0; height * width * 3];
    let image =
        Photo::new(Picture::new(height, width, &values)?).render(&recipe, &Presets::new(), Options::default())?;
    Ok(image
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| pixel[0])
        .collect())
}

/// The grid of `columns` x `rows` shares, and the indicator, for each pixel of a picture the shares divide whole, of the
/// pieces with an even number: the ones the painting look has.
fn pieces(columns: &[f64], rows: &[f64], height: usize, width: usize) -> (Grid, Vec<f64>) {
    let boundaries = |shares: &[f64], size: usize| -> Vec<f64> {
        let scale = float(size) / shares.iter().sum::<f64>();
        shares
            .iter()
            .scan(0.0, |edge, share| {
                *edge += share * scale;
                Some(*edge)
            })
            .collect()
    };
    let (across, down) = (boundaries(columns, width), boundaries(rows, height));
    // the piece of a pixel centre: how many borders lie at or before it
    let place = |edges: &[f64], centre: f64| edges.iter().filter(|&&edge| edge <= centre).count();
    let indicator = (0..height * width)
        .map(|at| {
            let (row, column) = (
                place(&down, float(at / width) + 0.5),
                place(&across, float(at % width) + 0.5),
            );
            f64::from(u8::from((row * columns.len() + column).is_multiple_of(2)))
        })
        .collect();
    (
        Grid {
            columns: Shares(columns.to_vec()),
            rows: Shares(rows.to_vec()),
            ..Grid::default()
        },
        indicator,
    )
}

/// The blur of `indicator` (`height` x `width`) by brute force: every output the sum over `-radius..=radius` of the
/// normalised `exp(-x^2 / 2 sigma^2)` times the value at the clamped position, along the rows and then down the columns.
fn gaussian(indicator: &[f64], height: usize, width: usize, sigma: f64) -> Vec<f64> {
    let (kernel, reach) = gaussian::weights(sigma, 4.0);
    let radius = i64::try_from(reach).unwrap_or(0);
    let along = |read: &dyn Fn(usize, usize) -> f64, rows: usize, columns: usize| -> Vec<f64> {
        let mut out = Vec::new();
        for row in 0..rows {
            for column in 0..columns {
                out.push(
                    (-radius..=radius)
                        .zip(&kernel)
                        .map(|(offset, weight)| {
                            let last = i64::try_from(columns).unwrap_or(1) - 1;
                            let source = usize::try_from((i64::try_from(column).unwrap_or(0) + offset).clamp(0, last))
                                .unwrap_or(0);
                            weight * read(row, source)
                        })
                        .sum(),
                );
            }
        }
        out
    };
    let across = along(&|row, column| indicator[row * width + column], height, width);
    let flipped = along(&|column, row| across[row * width + column], width, height);
    // `flipped` is the columns as rows: back to row after row
    (0..height * width)
        .map(|index| flipped[(index % width) * height + index / width])
        .collect()
}

/// The most the weights differ from the brute-force Gaussian of the indicator of the pieces, at a sigma in px.
fn worst_gap(height: usize, width: usize, grid: &Grid, indicator: &[f64], sigma: f64) -> Result<f64, Error> {
    let feather = sigma * 480.0 / float(height.max(width));
    let made = weights(height, width, grid.clone(), feather)?;
    let expected = gaussian(indicator, height, width, sigma);
    Ok(made
        .iter()
        .zip(&expected)
        .map(|(got, want)| (f64::from(*got) - want).abs())
        .fold(0.0, f64::max))
}

#[test]
fn the_weights_are_within_a_few_hundredths_of_the_brute_force_gaussian_over_blocks_at_any_sigma() -> Outcome {
    // three boxes are not a Gaussian: measured gaps are 0.03 at most from sigma 1.5, and 0.11 at sigma 1, where
    // whole-pixel boxes cannot meet the variance. Blocks of 15 pieces, of which the even ones are painted
    let (grid, indicator) = pieces(&[9.0, 11.0, 12.0, 5.0, 3.0], &[24.0, 16.0, 20.0], 60, 80);
    for (sigma, most) in [(1.0, 0.12), (1.5, 0.035), (2.0, 0.035), (3.0, 0.035), (4.0, 0.035)] {
        let gap = worst_gap(60, 80, &grid, &indicator, sigma)?;
        assert!(gap <= most, "sigma {sigma}: {gap}");
    }
    // larger pictures reach wider blurs: a sigma of 5, 12 and 15 on a map 300 wide
    let (grid, indicator) = pieces(&[9.0, 11.0, 12.0, 5.0, 3.0], &[24.0, 16.0, 20.0], 150, 300);
    for sigma in [5.0, 12.0, 15.0] {
        let gap = worst_gap(150, 300, &grid, &indicator, sigma)?;
        assert!(gap <= 0.035, "sigma {sigma}: {gap}");
    }
    Ok(())
}

#[test]
fn a_row_a_column_and_maps_a_few_pixels_wide_are_blurred_as_the_gaussian_is() -> Outcome {
    // the widest blur a map allows, sigma a twentieth of its longest side (10 px of 200)
    for (height, width) in [(1, 200), (200, 1), (3, 200), (200, 3)] {
        // a piece for each pixel across a map so narrow that wider pieces would have no pixel to be numbered by
        let columns = if width <= 3 {
            vec![1.0; width]
        } else {
            vec![9.0, 11.0, 12.0, 5.0, 3.0]
        };
        let rows = if height <= 3 {
            vec![1.0; height]
        } else {
            vec![24.0, 16.0, 20.0]
        };
        let (grid, indicator) = pieces(&columns, &rows, height, width);
        let gap = worst_gap(height, width, &grid, &indicator, 10.0)?;
        assert!(gap <= 0.035, "{height} x {width}: {gap}");
    }
    Ok(())
}

#[test]
fn a_feather_too_small_to_reach_a_neighbour_leaves_the_borders_as_hard_as_none() -> Outcome {
    let (grid, _) = pieces(&[9.0, 11.0, 12.0, 5.0, 3.0], &[24.0, 16.0, 20.0], 60, 80);
    let hard = weights(60, 80, grid.clone(), 0.0)?;
    // a feather of 0.5 design px is a sigma of 0.08 px, 1.8 a sigma of 0.3 px, on a map 80 wide
    for feather in [0.5, 1.0, 1.8] {
        assert_eq!(weights(60, 80, grid.clone(), feather)?, hard, "feather {feather}");
    }
    assert_ne!(weights(60, 80, grid, 6.0)?, hard);
    Ok(())
}

#[test]
fn one_piece_that_has_the_look_is_given_the_look_everywhere_and_an_impulse_keeps_its_total() -> Outcome {
    // a single piece with the painting look: the weights are 1 wherever the blur stays inside it, whatever the feather
    for feather in [2.4, 12.0, 24.0] {
        let flat = weights(30, 40, Grid::default(), feather)?;
        assert!(
            flat.iter().all(|weight| (weight - 1.0).abs() <= 1e-6),
            "feather {feather}"
        );
    }
    // a pixel of its own in the middle of a map larger than the kernel: the weights add up to 1 and none is below 0.
    // Nine pieces, the middle one (number 4) with the painting look and the rest changing nothing
    let mut looks = vec![None; 9];
    looks[4] = Some(level(1.0)?);
    let mut each = Each::new("cells", looks);
    each.feather = 24.0;
    let recipe = Recipe::new([each]).with_region(
        "cells",
        Grid {
            columns: Shares(vec![20.0, 1.0, 20.0]),
            rows: Shares(vec![20.0, 1.0, 20.0]),
            ..Grid::default()
        },
    );
    let values = vec![0.0; 41 * 41 * 3];
    let image = Photo::new(Picture::new(41, 41, &values)?).render(&recipe, &Presets::new(), Options::default())?;
    let impulse: Vec<f32> = image
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| pixel[0])
        .collect();
    assert!((total(&impulse) - 1.0).abs() <= 1e-4, "{}", total(&impulse));
    assert!(impulse.iter().all(|weight| *weight >= -1e-6));
    Ok(())
}

//! `lens_drops`: the drops are found by what they do to a picture that says where each pixel was read from (`coordinates`):
//! inside a drop a pixel shows the place `middle - refraction x (pixel - middle)`, so the middle and the radius of a drop can be
//! read off the output. Through them the whole picture is compared with a point-by-point computation from the definition
//! (the blur from the `gaussian_blur` effect, every reading, every edge rule, the shine), and the laws of the radius, the
//! count and the places of the drops are checked by statistics over seeds.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "small picture sizes; the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::gaussian_blur;
use bullterrier_image::effects::lens_drops::Params;
use bullterrier_image::{Edge, Error, Interpolation};

use crate::common::compare::{first_bit_difference, first_difference_within};
use crate::common::pictures::{coordinates, places, varied};
use crate::common::pixels::{run_picture, run_sized};
use crate::common::sampling::read_at;

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

/// The drop of a one-drop `params` (blur and highlight 0) on a `height` x `width` picture, found from what it does to
/// `coordinates`: the middle (row, column), and how many pixels show the drop in full (the radius is read off those).
struct Found {
    middle: (f64, f64),
    full: usize,
    changed: usize,
}

/// The middle that the most of `each` (one middle worked out from each changed pixel) agree on, within a thousandth of a pixel,
/// and how many agree: the pixels that read the picture's own places agree on one middle; the others (read from beyond the
/// picture, and the antialiased ring) scatter.
fn consensus(each: &[(f64, f64)]) -> ((f64, f64), usize) {
    let agreeing = |candidate: &(f64, f64)| {
        each.iter()
            .filter(|pair| (pair.0 - candidate.0).abs() < 1e-3 && (pair.1 - candidate.1).abs() < 1e-3)
            .count()
    };
    each.iter()
        .map(|candidate| (*candidate, agreeing(candidate)))
        .max_by_key(|(_, count)| *count)
        .unwrap_or(((0.0, 0.0), 0))
}

/// The middle of the drop, by `place = (1 + refraction) x middle - refraction x pixel` for every pixel that shows a place of
/// the picture's own: the one the most changed pixels agree on.
fn find(params: &Params, height: usize, width: usize) -> Result<Found, Error> {
    let params = Params {
        count: 1,
        blur: 0.0,
        highlight: 0.0,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Nearest,
        ..params.clone()
    };
    let original = coordinates(height, width);
    let out = run_picture(&params, height, width, &original)?;
    let read = places(&out, height, width);
    let rho = f64::from(params.refraction);
    let mut each = Vec::new();
    for (at, place) in read.iter().enumerate() {
        let (row, column) = ((at / width) as f64, (at % width) as f64);
        let moved = (0..2).any(|channel| (out[at * 3 + channel] - original[at * 3 + channel]).abs() > 1e-6);
        if moved {
            each.push((
                (place.0 + rho * row) / (1.0 + rho),
                (place.1 + rho * column) / (1.0 + rho),
            ));
        }
    }
    let (middle, full) = consensus(&each);
    Ok(Found {
        middle,
        full,
        changed: each.len(),
    })
}

/// A smooth picture with a different colour everywhere, so that a small error of a drop's place shows as a small error.
fn smooth(height: usize, width: usize) -> Vec<f32> {
    (0..height * width)
        .flat_map(|at| {
            let (row, column) = ((at / width) as f32, (at % width) as f32);
            [
                0.5 + 0.4 * (row * 0.21 + column * 0.13).sin(),
                0.5 + 0.4 * (row * 0.07 - column * 0.19).cos(),
                0.1 + 0.8 * (row * 0.05 + 0.02 * column) % 1.0,
            ]
        })
        .collect()
}

/// `picture` with the one drop at `middle` of radius `reach`, from the definition.
fn by_definition(
    params: &Params,
    middle: (f64, f64),
    reach: f64,
    size: usize,
    picture: &[f32],
) -> Result<Vec<f32>, Error> {
    let scene = run_picture(
        &gaussian_blur::Params {
            radius: params.blur,
            truncate: params.truncate,
            edge: params.edge.into(),
            ..gaussian_blur::Params::default()
        },
        size,
        size,
        picture,
    )?;
    let rho = f64::from(params.refraction);
    let mut out = picture.to_vec();
    for at in 0..size * size {
        let (row, column) = ((at / size) as f64, (at % size) as f64);
        let (down, across) = (row - middle.0, column - middle.1);
        let distance = down.hypot(across);
        let cover = (reach + 0.5 - distance).clamp(0.0, 1.0);
        if cover > 0.0 {
            let shown = read_at(
                &scene,
                size,
                size,
                params.interpolation,
                params.edge,
                middle.0 - rho * down,
                middle.1 - rho * across,
            );
            let shine = f64::from(params.highlight) * distance * (distance - down) / (2.0 * reach * reach);
            for channel in 0..3 {
                let shown = shown[channel].clamp(0.0, 1.0);
                let colour = shown + shine * (1.0 - shown);
                let was = f64::from(picture[at * 3 + channel]);
                out[at * 3 + channel] = (was * (1.0 - cover) + colour * cover).clamp(0.0, 1.0) as f32;
            }
        }
    }
    Ok(out)
}

/// One drop of exactly `radius` (no variation) at a place the seed chooses.
fn one_drop(radius: f32, seed: u32) -> Params {
    Params {
        count: 1,
        radius,
        size_variation: 0.0,
        seed,
        ..Params::default()
    }
}

#[test]
fn a_drop_shows_the_picture_turned_round_about_its_middle_and_as_wide_as_the_refraction() -> Result<(), Error> {
    // the pixels of the drop show `middle - refraction x (pixel - middle)`: the middle recovered from each is one place, and
    // as many pixels agree on it as there are in the disc of the drop's radius less half a pixel (the antialiased ring
    // is left out), within the error of counting a circle's lattice points
    let size = 200;
    let mut tested = 0;
    for refraction in [0.5_f32, 1.0, 2.0] {
        for seed in 0..24 {
            let params = Params {
                refraction,
                ..one_drop(14.0, seed)
            };
            let found = find(&params, size, size)?;
            let reach = f64::from(params.radius) * f64::from(refraction) + 2.0;
            let room = |middle: f64| middle > reach && middle < size as f64 - 1.0 - reach;
            if !(room(found.middle.0) && room(found.middle.1)) {
                continue; // reads from beyond the picture, where the places are not the picture's own
            }
            tested += 1;
            let disc = std::f64::consts::PI * 13.5_f64.powi(2);
            assert!(
                (found.full as f64 - disc).abs() < 0.08 * disc,
                "refraction {refraction}, seed {seed}: {} pixels for a disc of {disc}",
                found.full
            );
            assert!(found.changed >= found.full, "the changed pixels include the full ones");
        }
    }
    assert!(tested >= 20, "{tested} drops were away from the border");
    Ok(())
}

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // a small picture, so that drops near the border and strong refraction read from beyond it, under every edge rule; the
    // blur and the shine are in the definition too. The middle comes from `find` on the same drop
    let size = 48;
    let picture = smooth(size, size);
    let mut beyond = 0;
    for (blur, highlight, refraction) in [(0.0, 0.0, 1.0), (2.0, 0.6, 1.5), (1.5, 1.0, 4.0), (0.0, 0.5, 0.25)] {
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                for seed in 0..6 {
                    let params = Params {
                        blur,
                        highlight,
                        refraction,
                        interpolation,
                        edge,
                        ..one_drop(14.0, seed)
                    };
                    let found = find(&params, size, size)?;
                    // the drop reads beyond the picture when its reach exceeds the distance to a border
                    let reach = f64::from(refraction) * 14.0;
                    let near = |middle: f64| middle < reach || middle > size as f64 - 1.0 - reach;
                    beyond += usize::from(near(found.middle.0) || near(found.middle.1));
                    let at = first_difference_within(
                        &run_picture(&params, size, size, &picture)?,
                        &by_definition(&params, found.middle, 14.0, size, &picture)?,
                        2e-3,
                    );
                    assert_eq!(
                        at, None,
                        "{params:?}: first differs at {at:?} middle {:?} changed {} full {}",
                        found.middle, found.changed, found.full
                    );
                }
            }
        }
    }
    assert!(beyond > 100, "{beyond} of the drops read from beyond the picture");
    Ok(())
}

#[test]
fn the_comparison_tells_drops_that_differ_apart() -> Result<(), Error> {
    let size = 48;
    let picture = smooth(size, size);
    let params = Params {
        blur: 2.0,
        highlight: 0.6,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Reflect,
        ..one_drop(14.0, 3)
    };
    let found = find(&params, size, size)?;
    let ours = run_picture(&params, size, size, &picture)?;
    assert!(
        first_difference_within(&ours, &picture, 0.02).is_some(),
        "the drop changes the picture"
    );
    let (row, column) = found.middle;
    for (other, middle, reach) in [
        (params.clone(), (row + 0.3, column), 14.0),
        (params.clone(), (row, column), 13.0),
        (
            Params {
                highlight: 0.3,
                ..params.clone()
            },
            (row, column),
            14.0,
        ),
        (
            Params {
                blur: 1.0,
                ..params.clone()
            },
            (row, column),
            14.0,
        ),
        (
            Params {
                refraction: 1.4,
                ..params.clone()
            },
            (row, column),
            14.0,
        ),
        (
            Params {
                edge: Edge::Wrap,
                ..params.clone()
            },
            (row, column),
            14.0,
        ),
    ] {
        assert!(
            first_difference_within(&ours, &by_definition(&other, middle, reach, size, &picture)?, 2e-3).is_some(),
            "{other:?} {middle:?} {reach}"
        );
    }
    Ok(())
}

#[test]
fn the_radius_of_a_drop_is_between_radius_less_the_variation_and_radius() -> Result<(), Error> {
    // a single drop's radius, read from the pixels that show it in full (the disc of radius R - 1/2 has pi (R - 1/2)^2)
    let size = 200;
    let radius_of = |params: &Params| -> Result<Option<f64>, Error> {
        let found = find(params, size, size)?;
        let room = |middle: f64| middle > 20.0 && middle < size as f64 - 21.0;
        Ok((room(found.middle.0) && room(found.middle.1))
            .then(|| (found.full as f64 / std::f64::consts::PI).sqrt() + 0.5))
    };
    for (variation, expected_mean) in [(0.0_f32, 1.0), (0.5, 0.75), (1.0, 0.5)] {
        let mut shares = Vec::new();
        for seed in 0..90 {
            let params = Params {
                size_variation: variation,
                refraction: 1.0,
                ..one_drop(16.0, seed)
            };
            if let Some(radius) = radius_of(&params)? {
                shares.push(radius / 16.0);
            }
        }
        assert!(shares.len() >= 20, "{} drops away from the border", shares.len());
        let most = shares.iter().copied().fold(0.0, f64::max);
        let least = shares.iter().copied().fold(f64::INFINITY, f64::min);
        let mean = shares.iter().sum::<f64>() / shares.len() as f64;
        assert!(most < 1.06, "variation {variation}: largest share {most}");
        assert!(
            least > f64::from(1.0 - variation) - 0.1,
            "variation {variation}: smallest share {least}"
        );
        assert!(
            (mean - expected_mean).abs() < 0.07,
            "variation {variation}: mean share {mean} for {expected_mean}"
        );
        if variation > 0.0 {
            assert!(
                most - least > 0.3 * f64::from(variation),
                "variation {variation}: all drops alike"
            );
        }
    }
    Ok(())
}

#[test]
fn the_drops_are_anywhere_in_the_picture_whatever_its_shape() -> Result<(), Error> {
    // the middles of one-drop pictures over many seeds spread over a picture that is wider than high: down the rows and across
    // the columns the mean is near the middle and every quarter has some
    let (height, width) = (120, 300);
    let middles: Vec<(f64, f64)> = (0..80)
        .map(|seed| {
            let params = Params {
                refraction: 1.0,
                ..one_drop(3.0, seed)
            };
            find(&params, height, width).map(|found| found.middle)
        })
        .collect::<Result<_, _>>()?;
    for (long, axis) in [
        (height, (|m: &(f64, f64)| m.0) as fn(&(f64, f64)) -> f64),
        (width, |m| m.1),
    ] {
        let long = long as f64;
        let values: Vec<f64> = middles.iter().map(axis).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        assert!((mean - long / 2.0).abs() < 0.09 * long, "mean {mean} of {long}");
        for quarter in 0..4 {
            let low = f64::from(quarter) * long / 4.0;
            assert!(
                values.iter().any(|v| (low..low + long / 4.0).contains(v)),
                "nothing in the quarter from {low} of {long}"
            );
        }
        assert!(
            values.iter().all(|v| (0.0..long).contains(v)),
            "a drop outside the picture"
        );
    }
    Ok(())
}

#[test]
fn a_later_drop_is_laid_over_the_earlier_ones() -> Result<(), Error> {
    // the last of `count` drops is what `count` shows over what `count - 1` drops showed: where they differ, the pixels show
    // one drop whole, and as many of them agree on its middle as the disc has pixels that read the picture's own places,
    // overlapped or not (an earlier drop on top would take its share of them)
    let size = 100;
    let original = coordinates(size, size);
    let mut overlapped = 0;
    for seed in 0..8 {
        let params = |count| Params {
            count,
            refraction: 1.0,
            blur: 0.0,
            ..one_drop(14.0, seed)
        };
        let (before, after) = (
            run_picture(&params(7), size, size, &original)?,
            run_picture(&params(8), size, size, &original)?,
        );
        // the last drop alone, as `find` reads one: its middle by consensus of the pixels `after` changed
        let last: Vec<usize> = (0..size * size)
            .filter(|at| (0..2).any(|channel| (after[at * 3 + channel] - before[at * 3 + channel]).abs() > 1e-6))
            .collect();
        let read = places(&after, size, size);
        let each: Vec<(f64, f64)> = last
            .iter()
            .map(|&at| {
                (
                    f64::midpoint(read[at].0, (at / size) as f64),
                    f64::midpoint(read[at].1, (at % size) as f64),
                )
            })
            .collect();
        let (middle, agree) = consensus(&each);
        // the pixels of the disc (radius 14 less half a pixel) whose place, the reflection of the pixel in the middle, is on
        // the picture
        let on_picture = |place: f64| (0.0..=(size - 1) as f64).contains(&place);
        let disc: Vec<usize> = (0..size * size)
            .filter(|at| {
                let (row, column) = ((at / size) as f64, (at % size) as f64);
                (row - middle.0).hypot(column - middle.1) <= 13.5
                    && on_picture(2.0 * middle.0 - row)
                    && on_picture(2.0 * middle.1 - column)
            })
            .collect();
        overlapped += disc
            .iter()
            .filter(|at| !last.contains(at) || before_changed(&before, &original, **at))
            .count();
        assert!(
            disc.len() > 50,
            "seed {seed}: the last drop has {} pixels on the picture",
            disc.len()
        );
        assert!(
            (agree as f64 - disc.len() as f64).abs() <= 0.03 * disc.len() as f64 + 3.0,
            "seed {seed}: {agree} pixels agree, the disc has {}",
            disc.len()
        );
    }
    assert!(overlapped > 0, "no drop of these seeds overlapped an earlier one");
    Ok(())
}

/// Whether the pixel `at` of `picture` differs from `original`.
fn before_changed(picture: &[f32], original: &[f32], at: usize) -> bool {
    (0..2).any(|channel| (picture[at * 3 + channel] - original[at * 3 + channel]).abs() > 1e-6)
}

#[test]
fn about_count_drops_are_made() -> Result<(), Error> {
    // small drops far apart on a big flat-coordinate picture: the pixels changed are about count x the disc of radius + 1/2
    // (the antialiased ring is partly lost where the colour barely differs; overlaps and the border take a little more)
    let size = 400;
    let original = coordinates(size, size);
    for count in [1, 5, 20] {
        let params = Params {
            count,
            radius: 6.0,
            size_variation: 0.0,
            refraction: 1.0,
            blur: 0.0,
            seed: 7,
            ..Params::default()
        };
        let out = run_picture(&params, size, size, &original)?;
        let changed = (0..size * size)
            .filter(|at| (0..2).any(|channel| (out[at * 3 + channel] - original[at * 3 + channel]).abs() > 1e-6))
            .count();
        let disc = std::f64::consts::PI * 6.5_f64.powi(2);
        let expected = f64::from(count) * disc;
        assert!(
            (changed as f64 - expected).abs() < 0.2 * expected,
            "{count} drops: {changed} pixels changed for about {expected}"
        );
    }
    Ok(())
}

#[test]
fn no_drops_leave_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(9, 11);
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                count: 0,
                interpolation,
                edge,
                ..Params::default()
            };
            assert_eq!(
                first_bit_difference(&run_picture(&params, 9, 11, &picture)?, &picture),
                None
            );
        }
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_flat_without_highlight_and_values_stay_in_range_with_it() -> Result<(), Error> {
    let flat = run_sized(&Params::default(), 60, 80, 0.4)?;
    assert!(
        flat.iter()
            .all(|pixel| pixel.iter().all(|level| (level - 0.4).abs() < 1e-6))
    );
    for fill in [0.0_f32, 0.4, 1.0] {
        let lit = run_sized(
            &Params {
                count: 200,
                radius: 30.0,
                highlight: 1.0,
                ..Params::default()
            },
            60,
            80,
            fill,
        )?;
        assert!(
            lit.iter()
                .all(|pixel| pixel.iter().all(|level| (0.0..=1.0).contains(level))),
            "fill {fill}"
        );
        if fill < 1.0 {
            assert!(
                lit.iter().any(|pixel| pixel[0] > fill + 0.2),
                "the shine lights the drops"
            );
        }
    }
    Ok(())
}

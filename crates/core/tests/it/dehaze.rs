//! `dehaze`: the picture compared with the effect worked out from its definition (the darkest value of each patch found
//! by looking at every value in it, a guided filter window by window), and the limits the doc promises.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "small picture sizes, and the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::dehaze::Params;
use bullterrier_image::params::{Declared, EffectParams};
use bullterrier_image::{Edge, Error, Inputs, Picture, apply};
use serde_json::Value;

use crate::common::compare::first_difference_within;
use crate::common::edge::folded;
use crate::common::guided::guided_filter;

/// The pictures are compared at this distance: the reference is made in f64 where the effect makes it in f32, and the
/// colours are stretched by at most 1 / `transmission_floor`, 100 at the smallest.
const TOLERANCE: f32 = 1e-4;

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

/// `params` applied to the `height` x `width` picture `values` (row after row, three channels a pixel).
fn dehazed(params: &Params, height: usize, width: usize, values: &[f32]) -> Result<Vec<f32>, Error> {
    let image = apply(
        Params::EFFECT,
        &Picture::new(height, width, values)?,
        Value::Object(params.values()),
        &Inputs::default(),
    )?;
    Ok(image.into_values())
}

/// A different value in nearly every channel of every pixel, lifted toward grey as haze lifts it.
fn hazy(height: usize, width: usize) -> Vec<f32> {
    (0..height * width * 3)
        .map(|index| {
            let value = f32::from(u16::try_from((index * 7919 + index / 13) % 1000).unwrap_or(0)) / 1000.0;
            0.55 * value + 0.4
        })
        .collect()
}

/// The smallest value in the `patch` x `patch` window around every place (it spans `patch / 2` before the place and the
/// rest after it), by looking at all of them.
fn darkest(values: &[f32], height: usize, width: usize, patch: usize, rule: Edge) -> Vec<f32> {
    let (h, w, p) = (height as i64, width as i64, patch as i64);
    let mut out = Vec::new();
    for row in 0..h {
        for column in 0..w {
            let mut smallest = f32::INFINITY;
            for y in row - p / 2..row + p - p / 2 {
                for x in column - p / 2..column + p - p / 2 {
                    smallest = smallest.min(values[folded(rule, y, h) * width + folded(rule, x, w)]);
                }
            }
            out.push(smallest);
        }
    }
    out
}

/// The smallest channel of each pixel.
fn smallest(pixels: &[[f32; 3]]) -> Vec<f32> {
    pixels
        .iter()
        .map(|pixel| pixel[0].min(pixel[1]).min(pixel[2]))
        .collect()
}

/// The effect by its definition (He, Sun and Tang, 2009): see the doc of `Params`.
fn reference(params: &Params, height: usize, width: usize, values: &[f32]) -> Vec<f32> {
    let pixels: Vec<[f32; 3]> = values.as_chunks::<3>().0.to_vec();
    let patch = params.patch as usize;
    let dark = darkest(&smallest(&pixels), height, width, patch, params.edge);

    // the airlight: of the `count` pixels with the highest dark channel (those above the count-th highest value by reading
    // order, then those equal to it), the colour of the one with the largest red + green + blue, the first on a tie
    let count = ((f64::from(params.airlight_fraction) * pixels.len() as f64) as usize).max(1);
    let mut sorted = dark.clone();
    sorted.sort_by(|a, b| b.total_cmp(a));
    let threshold = sorted[count - 1];
    let above: Vec<usize> = (0..dark.len()).filter(|&at| dark[at] > threshold).collect();
    let tied: Vec<usize> = (0..dark.len())
        .filter(|&at| dark[at].total_cmp(&threshold).is_eq())
        .collect();
    let haziest = [above.as_slice(), &tied[..count - above.len()]].concat();
    let sum = |at: usize| pixels[at][0] + pixels[at][1] + pixels[at][2];
    let brightest = haziest
        .iter()
        .fold(haziest[0], |best, &at| if sum(at) > sum(best) { at } else { best });
    let airlight = pixels[brightest].map(|channel| channel.max(1e-3));

    // the transmission: from the dark channel of the picture over the airlight, refined along the edges of the luma
    let over_airlight: Vec<[f32; 3]> = pixels
        .iter()
        .map(|pixel| [0, 1, 2].map(|channel| pixel[channel] / airlight[channel]))
        .collect();
    let rough: Vec<f64> = darkest(&smallest(&over_airlight), height, width, patch, params.edge)
        .iter()
        .map(|&dark| 1.0 - f64::from(params.amount) * f64::from(dark))
        .collect();
    let luma: Vec<f64> = pixels
        .iter()
        .map(|p| 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]))
        .collect();
    let radius = f64::from(params.radius).round_ties_even() as usize;
    let epsilon = f64::from(params.edge_threshold).powi(2);
    let refined = guided_filter(&luma, &rough, height, width, radius, epsilon);

    pixels
        .iter()
        .zip(refined)
        .flat_map(|(pixel, refined)| {
            let transmission = refined.clamp(f64::from(params.transmission_floor), 1.0);
            [0, 1, 2].map(|channel| {
                let light = f64::from(airlight[channel]);
                ((f64::from(pixel[channel]) - light) / transmission + light).clamp(0.0, 1.0) as f32
            })
        })
        .collect()
}

#[test]
fn the_picture_is_the_dehazed_picture_of_the_definition_whatever_lies_beyond_the_edge() -> Result<(), Error> {
    // pictures from one pixel to a few; patches of 1, odd and even, smaller and larger than the picture; a guided filter
    // radius that is none, small and larger than the picture; every other parameter at a value of its own
    let shapes = [(1, 1), (1, 9), (9, 1), (6, 7), (13, 11), (20, 17)];
    for edge in EDGES {
        for (patch, radius, amount, edge_threshold, transmission_floor, airlight_fraction) in [
            (1, 0.0, 1.0, 0.05, 0.2, 0.1),
            (4, 2.0, 0.9, 0.03, 0.1, 0.02),
            (5, 40.0, 0.6, 0.5, 0.01, 0.1),
            (6, 0.0, 0.8, 0.001, 0.4, 0.3),
            (15, 3.0, 1.0, 0.2, 1.0, 0.05),
            (31, 2.5, 0.7, 0.02, 0.15, 0.0001),
        ] {
            let params = Params {
                amount,
                patch,
                radius,
                edge_threshold,
                transmission_floor,
                airlight_fraction,
                edge,
            };
            for (height, width) in shapes {
                let picture = hazy(height, width);
                let at = first_difference_within(
                    &dehazed(&params, height, width, &picture)?,
                    &reference(&params, height, width, &picture),
                    TOLERANCE,
                );
                assert_eq!(at, None, "{params:?}, {height} x {width}: first differs at {at:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn the_comparison_tells_another_look_apart() -> Result<(), Error> {
    // the known-bad pair: the reference of another patch, another edge rule and another amount is not the dehazed picture
    let picture = hazy(13, 11);
    let params = Params {
        patch: 5,
        edge: Edge::Wrap,
        radius: 2.0,
        ..Params::default()
    };
    let found = dehazed(&params, 13, 11, &picture)?;
    assert_eq!(
        first_difference_within(&found, &reference(&params, 13, 11, &picture), TOLERANCE),
        None
    );
    for other in [
        Params {
            patch: 9,
            ..params.clone()
        },
        Params {
            edge: Edge::Nearest,
            ..params.clone()
        },
        Params {
            amount: 0.3,
            ..params.clone()
        },
    ] {
        assert!(first_difference_within(&found, &reference(&other, 13, 11, &picture), TOLERANCE).is_some());
    }
    Ok(())
}

#[test]
fn a_picture_with_no_haze_and_an_amount_of_0_come_out_as_they_went_in() -> Result<(), Error> {
    // one channel of every pixel is black, so the dark channel is 0 and the transmission 1 whatever the patch and edge
    let mut clear = hazy(12, 14);
    for (at, pixel) in clear.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        pixel[at % 3] = 0.0;
    }
    for edge in EDGES {
        for patch in [1, 4, 15] {
            let params = Params {
                amount: 1.0,
                patch,
                edge,
                ..Params::default()
            };
            let found = dehazed(&params, 12, 14, &clear)?;
            assert_eq!(
                first_difference_within(&found, &clear, TOLERANCE),
                None,
                "{edge:?}, patch {patch}"
            );
        }
    }
    let veiled = hazy(12, 14);
    let off = dehazed(
        &Params {
            amount: 0.0,
            ..Params::default()
        },
        12,
        14,
        &veiled,
    )?;
    assert_eq!(first_difference_within(&off, &veiled, TOLERANCE), None);
    Ok(())
}

#[test]
fn nearest_and_reflect_give_the_same_picture_and_the_other_rules_do_not() -> Result<(), Error> {
    // the doc: the two look only at pixels of the patch inside the picture; wrap sees the far side's. Mirror sees the
    // same pixels as they do for an odd patch (the window of a border pixel holds the pixels its mirror image would);
    // for an even one, which reaches further before a pixel than after it, it sees one more at the start. A dark pixel
    // in each corner and a veil elsewhere.
    let (height, width) = (12, 14);
    let mut picture = hazy(height, width);
    for corner in [0, width - 1, (height - 1) * width, height * width - 1] {
        picture[corner * 3..corner * 3 + 3].fill(0.02);
    }
    let run = |edge| {
        let params = Params {
            patch: 4,
            radius: 0.0,
            edge,
            ..Params::default()
        };
        dehazed(&params, height, width, &picture)
    };
    let nearest = run(Edge::Nearest)?;
    assert!(
        run(Edge::Reflect)?
            .iter()
            .zip(&nearest)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
    for other in [Edge::Mirror, Edge::Wrap] {
        assert!(
            first_difference_within(&run(other)?, &nearest, TOLERANCE).is_some(),
            "{other:?}"
        );
    }
    Ok(())
}

#[test]
fn every_value_stays_in_the_range_at_the_extremes_of_the_parameters() -> Result<(), Error> {
    let picture = hazy(15, 13);
    for (patch, radius, edge_threshold, airlight_fraction, edge) in [
        (201, 500.0, 1.0, 0.0001, Edge::Wrap),
        (1, 0.0, 0.001, 0.1, Edge::Mirror),
    ] {
        let params = Params {
            amount: 1.0,
            patch,
            radius,
            edge_threshold,
            transmission_floor: 0.01,
            airlight_fraction,
            edge,
        };
        let found = dehazed(&params, 15, 13, &picture)?;
        assert!(found.iter().all(|value| (0.0..=1.0).contains(value)), "{params:?}");
    }
    Ok(())
}

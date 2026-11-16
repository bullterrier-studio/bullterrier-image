//! `stipple`: the picture as dots whose number follows the tone, spread evenly by Lloyd relaxation. What it does is
//! documented once, on `Params`.

use std::f64::consts::PI;

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::color::luminance;
use crate::ops::hatch::Tone;
use crate::ops::length;
use crate::ops::nearest::Buckets;
use crate::ops::par;
use crate::ops::random::Rng;
use crate::ops::raster::centre;
use crate::ops::sites::scatter;
use crate::params::{Color, Params};
use crate::{Footprint, Image, Inputs, Picture};

/// Where `stipple` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[],
};

/// A dot's place is the relaxation of every dot's cell, which reads the whole picture.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Dots paint where the picture's tone asks, over the whole canvas: not worked out.
pub(crate) fn spread(_params: &Params) -> crate::Spread {
    crate::Spread::Unbounded
}

/// The picture as dots of ink on paper, as a pen-and-ink stipple drawing.
///
/// Dots of `radius` px in `color` on `paper`. Darkness is how far a pixel's luminance lies from the `paper`'s towards the
/// `color`'s, and the number of dots is the picture's total darkness over the area of one dot, so the ink covers the tone
/// (the mean ink is the tone) until the dots crowd: they cannot pack past about 0.9 of the area (hexagonal packing), so a
/// tone darker than that prints at it. The dots start scattered at random in proportion to darkness, from `seed`, and
/// `iterations` rounds of Lloyd relaxation (Secord's weighted version) move each dot to the darkness-weighted centre of the
/// pixels nearer to it than to any other dot, which spreads them evenly and keeps them dense where the picture is dark.
/// More rounds make an evener, more regular pattern; 0 leaves the random scatter, clumped. A dot's edge is a ramp one px
/// wide; a dot in the border is cut by it.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Radius of every dot in px. Larger = fewer, bigger dots for the same tone.
    #[param(default = 1.5, unit = px, min = 1, max = 20)]
    pub radius: f32,
    /// Rounds of Lloyd relaxation: 0 = the random scatter, more = dots spread more evenly. Work grows with it.
    #[param(default = 10, unit = count, min = 0, max = 50)]
    pub iterations: i32,
    /// The ink.
    #[param(default = "#000000")]
    pub color: Color,
    /// The paper between dots. Tones darker than it get ink.
    #[param(default = "#ffffff")]
    pub paper: Color,
    /// Random seed for where the dots start: the same seed always gives the same dots, another seed a different pattern.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// The area of ink an anti-aliased dot of radius `radius` paints, in px: its ramp one px wide integrates to
/// `pi (r^2 + 1/12)` (a full disc to `r - 1/2`, a linear fall to nothing at `r + 1/2`).
fn dot_area(radius: f64) -> f64 {
    PI * (radius * radius + 1.0 / 12.0) // constant: the integral of a one px ramp round a disc
}

/// One round of Lloyd relaxation: every site moves to the centre of the darkness (`weights`, row after row) of the pixels
/// nearer to it than to any other site; a site whose pixels hold no darkness stays.
fn relax(sites: &mut [[f64; 2]], (height, width): (usize, usize), weights: &[f64]) {
    let buckets = Buckets::new(sites);
    let mut owner = vec![0_usize; height * width];
    par::rows(&mut owner, width, |top, band| {
        for (at, slot) in band.iter_mut().enumerate() {
            let [x, y] = centre(top * width + at, width, None);
            *slot = buckets.nearest(x, y);
        }
    });
    // per site: the darkness and its first moments over x and y
    let mut sums = vec![[0.0; 3]; sites.len()]; // constant: nothing accumulated yet
    for (at, (&site, &weight)) in owner.iter().zip(weights).enumerate() {
        let [x, y] = centre(at, width, None);
        let sum = &mut sums[site];
        sum[0] += weight;
        sum[1] += weight * x;
        sum[2] += weight * y;
    }
    for (site, sum) in sites.iter_mut().zip(&sums) {
        if sum[0] > 0.0 {
            *site = [sum[1] / sum[0], sum[2] / sum[0]];
        }
    }
}

/// The ink (0..1) of `sites` as dots of `radius` px, row after row: where dots overlap the ink of the strongest.
fn paint(sites: &[[f64; 2]], (height, width): (usize, usize), radius: f64) -> Vec<f32> {
    let mut cover = vec![0.0_f32; height * width]; // constant: no ink yet
    let reach = radius + 0.5; // constant: half a px of ramp each side of the edge
    for &[x, y] in sites {
        let rows = length::whole((y - reach).floor().max(0.0))..length::whole((y + reach).ceil() + 1.0).min(height);
        let columns = length::whole((x - reach).floor().max(0.0))..length::whole((x + reach).ceil() + 1.0).min(width);
        for row in rows {
            for column in columns.clone() {
                let distance = (length::float(column) + 0.5 - x).hypot(length::float(row) + 0.5 - y);
                #[expect(clippy::cast_possible_truncation, reason = "a share")]
                let ink = (reach - distance).clamp(0.0, 1.0) as f32;
                let slot = &mut cover[row * width + column];
                *slot = slot.max(ink);
            }
        }
    }
    cover
}

/// The stipple.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let (paper, color) = (params.paper.0, params.color.0);
    let tone = Tone::new(paper, color);
    let weights: Vec<f64> = picture
        .pixels()
        .iter()
        .map(|&pixel| tone.of_light(luminance(pixel)))
        .collect();
    let radius = f64::from(params.radius);
    let total: f64 = weights.iter().sum();
    let count = length::whole(total / dot_area(radius));
    let mut sites = if count == 0 {
        Vec::new()
    } else {
        scatter(count, (height, width), Some(&weights), &mut Rng::seeded(params.seed))
    };
    if !sites.is_empty() {
        for _ in 0..params.iterations {
            relax(&mut sites, (height, width), &weights);
        }
    }
    let cover = paint(&sites, (height, width), radius);
    let out = cover
        .iter()
        .map(|&share| [0, 1, 2].map(|c| paper[c] + (color[c] - paper[c]) * share))
        .collect();
    Image::from_pixels(height, width, out)
}

//! `reaction_diffusion`: a Gray-Scott pattern grown from the picture's bright parts. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::cast::single;
use crate::ops::color::luminance;
use crate::ops::edge::Edge;
use crate::ops::length::{float, whole};
use crate::ops::par;
use crate::ops::ramp;
use crate::ops::reaction::grow;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `reaction_diffusion` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[],
};

/// What `reaction_diffusion` reads around a pixel: a pattern grown over many steps depends on the whole picture.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Grow a Gray-Scott reaction-diffusion pattern (spots, worms, mazes) from the picture's bright parts.
///
/// The model has two chemicals, u (starting at 1 everywhere) and v (starting at 0). Where the picture's brightness is above
/// `threshold` they start at u = 1/2, v = 1/4. Each step both diffuse (v half as fast as u, by the five-point Laplacian) and
/// react: u += Du lap u - u v^2 + feed (1 - u), v += Dv lap v + u v^2 - (feed + kill) v, with Du = 0.2097 and Dv = 0.105
/// (Pearson 1993) and a time step of 1. The result is drawn through `colors` at 1 - u, so untouched areas take the first
/// colour. The simulation runs on cells of `cell` px: the picture is read at each cell's centre, and the cells are read
/// back over the picture, both by `resampling`.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The feed rate F of the Gray-Scott model: how fast chemical u is replenished. Together with `kill` it decides the kind
    /// of pattern: about 0.03 to 0.04 with a kill of 0.06 gives spots and worms, higher feeds give mazes and fingerprints,
    /// the extremes leave nothing but the seed.
    #[param(default = 0.037, unit = ratio, min = 0.01, max = 0.08)]
    pub feed: f32,
    /// The kill rate k of the Gray-Scott model: how fast chemical v is removed. Raising it makes the pattern sparser
    /// (isolated spots); lowering it fills the picture.
    #[param(default = 0.06, unit = ratio, min = 0.04, max = 0.07)]
    pub kill: f32,
    /// How many simulation steps run. 0 shows only the seed; more lets the pattern grow outward from it and costs time in
    /// proportion (and more cells, so smaller `cell`, cost more per step).
    #[param(default = 300, unit = count, min = 0, max = 5000)]
    pub steps: usize,
    /// The size of one simulation cell in px: the model runs on a coarse grid and is read back over the picture by
    /// `resampling`. 1 simulates every pixel (slow on large pictures); larger = a coarser, larger-scale pattern that runs
    /// faster. Tuned for a picture about 480 px high.
    #[param(default = 8.0, unit = px, min = 1, max = 64)]
    pub cell: f32,
    /// Brightness (0..1) above which the picture seeds the pattern: only the brighter parts start it. Lower seeds more of
    /// the picture; at 1 nothing seeds and the result is a flat first colour.
    #[param(default = 0.6, unit = fraction, min = 0, max = 1)]
    pub threshold: f32,
    /// The colour ramp, evenly spaced; at least two. Untouched areas take the first colour and the strongest pattern the
    /// last.
    #[param(default = ["#12002b", "#6a1bff", "#ff2bd6", "#ffd84a"], min_length = 2)]
    pub colors: Vec<Color>,
    /// How the picture is read at each cell and how the cells are read back over the picture: 'nearest' takes the closest
    /// cell (blocky), 'bilinear' blends the four around (smooth), 'bicubic' fits a cubic spline (the smoothest, may
    /// overshoot slightly).
    #[param(default = Bilinear, advanced)]
    pub resampling: Interpolation,
    /// What lies beyond the picture's border for the diffusion and for reading: 'reflect' (mirrored, edge pixel repeated),
    /// 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge, so the
    /// pattern tiles).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture with the pattern drawn over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let cell = f64::from(params.cell);
    let (rows, columns) = (
        whole((float(height) / cell).ceil()), // cells down, rounded up
        whole((float(width) / cell).ceil()),  // and across
    );
    let edge = params.edge.extension();
    // a pixel's centre is at whole numbers, a cell's centre is at (index + 1/2) cells: the picture is read there
    let seen = Sampler::new(picture.pixels(), height, width, params.resampling, edge);
    let mut seeded = vec![false; rows * columns];
    par::rows(&mut seeded, columns, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(columns).enumerate() {
            let row = (float(first_row + offset) + 0.5) * cell - 0.5; // constant: a centre is half a unit in
            for (column, on) in line.iter_mut().enumerate() {
                let at = seen.at(row, (float(column) + 0.5) * cell - 0.5); // constant: as above
                // reading is linear in the pixels, so the brightness of the reading is the reading of the brightness
                *on = luminance(at.map(single)) > params.threshold;
            }
        }
    });
    let chemical = grow(&seeded, (rows, columns), (params.feed, params.kill), params.steps, edge);
    let dropped: Vec<[f32; 3]> = chemical.iter().map(|&u| [1.0 - u; 3]).collect();
    let back = Sampler::new(&dropped, rows, columns, params.resampling, edge);
    let stops: Vec<[f32; 3]> = params.colors.iter().map(|colour| colour.0).collect();
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = (float(first_row + offset) + 0.5) / cell - 0.5; // constant: a centre is half a unit in
            for (column, pixel) in line.iter_mut().enumerate() {
                let at = back.at(row, (float(column) + 0.5) / cell - 0.5); // constant: as above
                *pixel = ramp::at(&stops, single(at[0]).clamp(0.0, 1.0));
            }
        }
    });
    Image::from_pixels(height, width, out)
}

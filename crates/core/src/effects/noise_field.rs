//! `noise_field`: a generated noise picture (value, perlin, worley or sine) coloured through a ramp. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::edge::Edge;
use crate::ops::gauss::ndtr;
use crate::ops::nearest::Buckets;
use crate::ops::noise::{Fractal, field, standardised};
use crate::ops::random::{Rng, mix, unit};
use crate::ops::sample::{Interpolation, Sampler};
use crate::ops::{cast, length, par, ramp};
use crate::params::{Choice, Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `noise_field` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Backgrounds,
    opacity: 1.0,
    inputs: &[],
};

/// What `noise_field` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// The noises `noise_field` can make.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Kind {
    /// Random values on a lattice, smoothly interpolated: soft blotches.
    Value,
    /// Gradient noise: random slopes on a lattice, finer and more even, zero at the lattice points.
    Perlin,
    /// Cellular noise: the distance to the nearest of a scatter of random points.
    Worley,
    /// Crossed sine waves: smooth, regular swirls; the classic plasma.
    Sine,
}

/// Replace the picture with a noise field of `kind` (value, perlin, worley, the Voronoi-like one, or sine, the classic
/// plasma), coloured through `colors`.
///
/// A generated background: the input is used only for its height and width, its content is ignored. Followed by `shatter`
/// with `fill` 1 it makes flat Voronoi cells (see the `voronoi-cells` recipe). The noise is standardised to zero mean and
/// unit deviation over the picture and passed through the normal cumulative distribution, so a pixel's place on the colour
/// ramp is about its rank among all pixels. 'value' is fractal noise; 'perlin' sums octaves of gradient noise (a Gaussian
/// random slope at each lattice point, blended with Perlin's fade curve); 'worley' sums octaves of F1 (the distance to the
/// nearest feature point, one point per cell of `scale` / `lacunarity`^i, measured in those cells), weight
/// `persistence`^i; 'sine' sums octaves of two plane sine waves sin(2 pi (x cos a + y sin a) / w + p) of wavelength w =
/// `scale` / `lacunarity`^i, random directions a and phases p, weight `persistence`^i, layer i evaluated at positions
/// pushed along x and y by `warp` px times the two waves of layer i - 1.
///
/// For the other kinds `warp` reads the finished picture at points shifted by two further fields of the kind (the seed's
/// streams 1 and 2, standardised, so their deviation is `warp` px), bicubically, mirrored beyond the edge.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The noise: 'value' (random values on a lattice, smoothly interpolated: soft blotches), 'perlin' (gradient noise:
    /// random slopes on a lattice, finer and more even, zero at the lattice points) or 'worley' (cellular noise: the distance
    /// to the nearest of a scatter of random points, one per cell of `scale` px, dark at each point and brightening away
    /// from it; each layer is a finer scatter) or 'sine' (crossed sine waves: smooth, regular swirls; the classic plasma).
    #[param(default = Perlin)]
    pub kind: Kind,
    /// Size of the coarsest features in pixels: the spacing of the first lattice (value, perlin), the size of a cell
    /// (worley) or the wavelength from crest to crest of the first waves (sine); tuned for a picture about 480 px high.
    /// Larger = broad, slow variation; small = fine grain.
    #[param(default = 120.0, unit = px, min = 2, max = 1024)]
    pub scale: f32,
    /// How many layers of noise are summed (all kinds), each finer than the last (by `lacunarity`) and weaker (by
    /// `persistence`). 1 = smooth, more = more fine detail. Each sine layer is two waves crossing at random angles. Layers
    /// finer than one pixel (sine: two pixels) are dropped.
    #[param(default = 4, unit = count, min = 1, max = 8)]
    pub octaves: i32,
    /// How much weaker each finer layer is than the one before (all kinds): each layer's weight is this to the power of its
    /// number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough).
    #[param(default = 0.5, unit = ratio, min = 0, max = 1)]
    pub persistence: f32,
    /// How much finer each layer is than the one before (all kinds): the lattice spacing, cell size or wavelength is
    /// divided by this per layer. 1 = every layer the same size, 2 = each half as coarse, larger = detail drops off faster.
    #[param(default = 2.0, unit = ratio, min = 1, max = 4)]
    pub lacunarity: f32,
    /// How far in pixels the pattern is pushed about, the typical shift of a point: the swirling, folded look of a domain
    /// warp. 0 = no warp, and a little is nearly the same picture. In 'sine' the first layer is never pushed, so with one
    /// octave `warp` changes nothing.
    #[param(default = 0.0, unit = px, min = 0, max = 500)]
    pub warp: f32,
    /// The colours the noise is shown in, from its lowest values to its highest, evenly spaced along the ramp; at least two.
    /// Each pixel's place on the ramp is its rank in the noise, so every colour is used about equally.
    #[param(default = ["#0b1d3a", "#2a7f9e", "#f4e9c8"], min_length = 2)]
    pub colors: Vec<Color>,
    /// Random seed: the same seed always gives the same picture, another seed a different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// One layer of noise: where it is in the sum, how coarse and how strong.
struct Layer {
    octave: u32,
    /// Lattice spacing, cell size or wavelength in px.
    spacing: f64,
    weight: f64,
}

/// The layers of `params` that the pixel grid can show: those whose `spacing` is at least `finest`; the first one under it
/// ends the list.
fn layers(params: &Params, finest: f64) -> Vec<Layer> {
    let mut weight = 1.0; // constant: the coarsest layer counts in full
    (0..params.octaves.cast_unsigned())
        .map(|octave| {
            let layer = Layer {
                octave,
                spacing: f64::from(params.scale) / f64::from(params.lacunarity).powi(octave.cast_signed()),
                weight,
            };
            weight *= f64::from(params.persistence);
            layer
        })
        .take_while(|layer| layer.spacing >= finest)
        .collect()
}

/// A pair of independent standard normal numbers drawn by the hash of `words` (Box-Muller).
// constant: the four words are stream, octave, row and column
fn normals(seed: u32, words: [u64; 4]) -> [f64; 2] {
    let draw = |part| unit(mix(seed, &[words[0], words[1], words[2], words[3], part])); // constant: the same four words
    // 1 - u lies in (0, 1], so the logarithm is finite
    let radius = (-2.0 * (1.0 - draw(0)).ln()).sqrt(); // constant: Box-Muller (1958)
    let angle = std::f64::consts::TAU * draw(1);
    [radius * angle.cos(), radius * angle.sin()]
}

/// Gradient noise of `layers` (Perlin's, with his fade curve), summed, not standardised: each lattice point has a Gaussian
/// random slope, a pixel blends the four corners of its cell.
fn perlin(height: usize, width: usize, params: &Params, stream: u64) -> Vec<f32> {
    let mut sum = vec![0.0_f32; height * width];
    for Layer {
        octave,
        spacing,
        weight,
    } in layers(params, 1.0)
    {
        // lattice points to reach the last row and column, and the one past each
        let rows = length::whole(((length::float(height) - 1.0) / spacing).floor()) + 2;
        let columns = length::whole(((length::float(width) - 1.0) / spacing).floor()) + 2;
        let slopes: Vec<[f64; 2]> = (0..rows * columns)
            .map(|at| {
                normals(
                    params.seed,
                    [stream, u64::from(octave), (at / columns) as u64, (at % columns) as u64],
                )
            })
            .collect();
        par::rows(&mut sum, width, |first_row, band| {
            for (at, value) in band.iter_mut().enumerate() {
                let (row, column) = (first_row + at / width, at % width);
                let (down, across) = (length::float(row) / spacing, length::float(column) / spacing);
                let (corner_row, corner_column) = (length::whole(down.floor()), length::whole(across.floor()));
                let (fy, fx) = (down - length::float(corner_row), across - length::float(corner_column));
                let corner = |dr: usize, dc: usize| {
                    let [slope_down, slope_across] = slopes[(corner_row + dr) * columns + corner_column + dc];
                    slope_down * (fy - length::float(dr)) + slope_across * (fx - length::float(dc))
                };
                let (fade_y, fade_x) = (fade(fy), fade(fx));
                let top = corner(0, 0) + (corner(0, 1) - corner(0, 0)) * fade_x;
                let bottom = corner(1, 0) + (corner(1, 1) - corner(1, 0)) * fade_x;
                *value += cast::single(weight * (top + (bottom - top) * fade_y));
            }
        });
    }
    sum
}

/// Perlin's improved-noise fade curve 6t^5 - 15t^4 + 10t^3 (SIGGRAPH 2002): zero slope at both ends.
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0) // constant: Perlin's improved-noise fade coefficients
}

/// Cellular noise of `layers` summed, not standardised: per layer the distance of each pixel to the nearest point of a
/// jittered grid (one random point in each cell, and a ring of cells round the picture), in cells.
fn worley(height: usize, width: usize, params: &Params, stream: u64) -> Vec<f32> {
    let mut sum = vec![0.0_f32; height * width];
    for Layer {
        octave,
        spacing: cell,
        weight,
    } in layers(params, 1.0)
    {
        let rows = length::whole((length::float(height) / cell).ceil()) + 2;
        let columns = length::whole((length::float(width) / cell).ceil()) + 2;
        // the cells are numbered from -1, so a point just outside the picture can be the nearest
        let points: Vec<[f64; 2]> = (0..rows * columns)
            .map(|at| {
                let (row, column) = (at / columns, at % columns);
                let jitter = |part| {
                    unit(mix(
                        params.seed,
                        &[stream, u64::from(octave), row as u64, column as u64, part],
                    ))
                };
                [
                    (length::float(column) - 1.0 + jitter(0)) * cell,
                    (length::float(row) - 1.0 + jitter(1)) * cell,
                ]
            })
            .collect();
        let buckets = Buckets::new(&points);
        par::rows(&mut sum, width, |first_row, band| {
            for (at, value) in band.iter_mut().enumerate() {
                let (row, column) = (length::float(first_row + at / width), length::float(at % width));
                let [x, y] = points[buckets.nearest(column, row)];
                *value += cast::single(weight * (x - column).hypot(y - row) / cell);
            }
        });
    }
    sum
}

/// One plane wave: its direction (cosine and sine of the angle), the phase and the wavelength's angular frequency.
struct Wave {
    along: [f64; 2],
    phase: f64,
    frequency: f64,
}

impl Wave {
    /// A wave of `wavelength` px in a random direction and phase.
    fn drawn(rng: &mut Rng, wavelength: f64) -> Self {
        let (angle, phase) = (
            std::f64::consts::TAU * rng.uniform(),
            std::f64::consts::TAU * rng.uniform(),
        );
        Self {
            along: [angle.cos(), angle.sin()],
            phase,
            frequency: std::f64::consts::TAU / wavelength,
        }
    }

    /// The wave at (`x`, `y`) pushed along by (`push_x`, `push_y`) px.
    fn at(&self, x: f64, y: f64, push_x: f64, push_y: f64) -> f64 {
        (self.frequency * ((x + push_x) * self.along[0] + (y + push_y) * self.along[1]) + self.phase).sin()
    }
}

/// Sine waves summed, not standardised: per layer two plane waves, each layer pushed about by the one before.
fn sine(height: usize, width: usize, params: &Params) -> Vec<f32> {
    let mut rng = Rng::seeded(params.seed);
    // a wave needs at least 2 px a period to show on a pixel grid (Nyquist)
    let waves: Vec<(f64, [Wave; 2])> = layers(params, 2.0)
        .into_iter()
        .map(|layer| (layer.weight, [(); 2].map(|()| Wave::drawn(&mut rng, layer.spacing))))
        .collect();
    let warp = f64::from(params.warp);
    let mut sum = vec![0.0_f32; height * width];
    par::rows(&mut sum, width, |first_row, band| {
        for (at, value) in band.iter_mut().enumerate() {
            let (y, x) = (length::float(first_row + at / width), length::float(at % width));
            let (mut push_x, mut push_y) = (0.0, 0.0);
            let mut total = 0.0; // constant: the sum starts at zero
            for (weight, [first, second]) in &waves {
                let (one, two) = (first.at(x, y, push_x, push_y), second.at(x, y, push_x, push_y));
                total += weight * (one + two);
                (push_x, push_y) = (warp * one, warp * two);
            }
            *value = cast::single(total);
        }
    });
    sum
}

/// The noise of `params.kind` over the picture, standardised: independent stream `stream` of the seed.
fn noise(height: usize, width: usize, params: &Params, stream: u64) -> Vec<f32> {
    match params.kind {
        Kind::Value => field(
            height,
            width,
            &Fractal {
                spacing: f64::from(params.scale),
                octaves: params.octaves.cast_unsigned(),
                persistence: f64::from(params.persistence),
                lacunarity: f64::from(params.lacunarity),
                seed: params.seed,
                origin: 0.0, // constant: a lattice point on the first pixel
            },
            stream,
        ),
        Kind::Perlin => standardised(perlin(height, width, params, stream)),
        Kind::Worley => standardised(worley(height, width, params, stream)),
        Kind::Sine => standardised(sine(height, width, params)),
    }
}

/// The noise picture, in the picture's size: the picture is used only for its height and width.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let mut place: Vec<f32> = noise(height, width, params, 0)
        .into_iter()
        .map(|z| cast::single(ndtr(f64::from(z))))
        .collect();
    if params.kind != Kind::Sine && params.warp > 0.0 {
        // constant: streams 1 and 2 of the seed are the independent fields that push rows and columns; 0 is the picture
        let (push_down, push_across) = (noise(height, width, params, 1), noise(height, width, params, 2));
        let pixels: Vec<[f32; 3]> = place.iter().map(|&level| [level; 3]).collect();
        let sampler = Sampler::new(&pixels, height, width, Interpolation::Bicubic, Edge::Mirror.extension());
        let warp = f64::from(params.warp);
        par::rows(&mut place, width, |first_row, band| {
            for (at, level) in band.iter_mut().enumerate() {
                let index = first_row * width + at;
                let (row, column) = (first_row + at / width, at % width);
                let read = sampler.at(
                    length::float(row) + warp * f64::from(push_down[index]),
                    length::float(column) + warp * f64::from(push_across[index]),
                );
                // the cubic can overshoot beside a sharp fold; a place on the ramp lies in 0..1
                *level = cast::single(read[0]).clamp(0.0, 1.0);
            }
        });
    }
    let stops: Vec<[f32; 3]> = params.colors.iter().map(|colour| colour.0).collect();
    picture.map_at(|row, column, _| ramp::at(&stops, place[row * width + column]))
}

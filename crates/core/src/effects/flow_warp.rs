//! `flow_warp`: the picture carried along a field of noise-driven currents, smearing into streaks. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::noise::{Fractal, field_rows};
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `flow_warp` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `flow_warp` reads around a pixel: the content that reaches it travelled `amount` px, and each of the `steps`
/// resamplings reads up to two rows beyond the place it is asked for (the footprint's own margin counts one of them).
pub(crate) fn footprint(params: &Params, canvas: &Canvas) -> Footprint {
    let taps = 2.0 * f64::from(params.steps - 1); // constant: a bicubic window's extra rows, for each step but the first
    Footprint::reach(
        f64::from(params.amount) + taps / canvas.ratio,
        params.edge == Edge::Wrap,
    )
}

/// Carry the picture `amount` px along a flow whose direction is `angle` plus `variation` degrees of fractal noise.
///
/// The flow at each pixel points `angle` degrees clockwise from pointing right, turned by a noise field (`scale`,
/// `octaves`, `persistence`, `lacunarity`, of standard deviation 1 at every place, then scaled to `variation`
/// degrees of spread). The picture is moved along it in `steps` equal steps of `amount` / `steps` px, each step moving what
/// is at a pixel one step along the flow there, so a pixel's content follows the bends of the flow and piles up into
/// streaks where currents meet. The path of any content is `amount` px long. The result never leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far in pixels the picture is carried along the flow in all, the length of the path each pixel's content travels
    /// in `steps` equal steps (the path bends with the flow, so it ends nearer than this where it turns). Raising it makes
    /// longer streaks; 0 leaves the picture unchanged.
    #[param(default = 20.0, unit = px, min = 0, max = 500)]
    pub amount: f32,
    /// How many equal steps the journey is made in, the picture being resampled once per step: with 1 every pixel moves
    /// straight `amount` px in the direction of the flow where it stands; more steps follow the bends of the flow and
    /// smear the picture into streaks along it, a little softer each time. Raising it costs time in proportion.
    #[param(default = 6, unit = count, min = 1, max = 32, advanced)]
    pub steps: i32,
    /// Size of the coarsest swirls of the flow in pixels, the spacing of the first noise lattice. Larger = broad, sweeping
    /// currents; small = fine, busy eddies.
    #[param(default = 80.0, unit = px, min = 1, max = 1024)]
    pub scale: f32,
    /// How many layers of noise are summed to make the flow, each finer than the last (by `lacunarity`) and weaker (by
    /// `persistence`). 1 = smooth currents, more = more fine, ragged detail. Layers finer than one pixel are dropped.
    #[param(default = 3, unit = count, min = 1, max = 8)]
    pub octaves: i32,
    /// How much weaker each finer layer is than the one before: each layer's weight is this to the power of its number. 0 =
    /// only the coarsest layer, 1 = every layer equally strong (very rough).
    #[param(default = 0.5, unit = ratio, min = 0, max = 1, advanced)]
    pub persistence: f32,
    /// How much finer each layer is than the one before: the lattice spacing is divided by this per layer. 1 = every layer
    /// the same size, 2 = each half as coarse, larger = detail drops off faster in size.
    #[param(default = 2.0, unit = ratio, min = 1, max = 4, advanced)]
    pub lacunarity: f32,
    /// Random seed: the same seed always gives the same flow, another seed a different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
    /// The direction the flow runs in on average, in degrees clockwise from pointing right: 0 carries the picture to the
    /// right, 90 down, 180 to the left, 270 up. With `variation` 0 the whole picture is carried this way.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// How far the noise turns the flow away from `angle`: the typical turn, in degrees, either way (the noise's standard
    /// deviation). 0 = one straight current; 60 = streaks that wander about the angle; 360 = currents running every way,
    /// with eddies where they meet.
    #[param(default = 60.0, unit = degrees, min = 0, max = 360)]
    pub variation: f32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft),
    /// 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What a pixel near the border sees when the flow carries it in from beyond the picture: 'reflect' (mirrored, edge
    /// pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the
    /// opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture carried along the flow.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // the flow is that of the canvas place, so a band of the picture is carried as the same rows of the whole are
    let place = inputs.place(picture);
    let noise = field_rows(
        place.first_row,
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
        0,
    );
    let (angle, variation) = (
        f64::from(params.angle).to_radians(),
        f64::from(params.variation).to_radians(),
    );
    let step = f64::from(params.amount) / f64::from(params.steps);
    // a pixel shows what was a step behind it: the offsets down and across, the same in every step
    let behind: Vec<(f64, f64)> = noise
        .iter()
        .map(|&z| {
            let (sin, cos) = (angle + variation * f64::from(z)).sin_cos();
            (step * sin, step * cos)
        })
        .collect();
    let carry = |pixels: &[[f32; 3]]| {
        let sampler = Sampler::new(pixels, height, width, params.interpolation, params.edge.extension());
        Centre::new(height, width, 0.5, 0.5).warp(&sampler, |spot| {
            let (down, across) = behind[spot.index];
            (spot.row - down, spot.column - across)
        })
    };
    let mut carried = carry(picture.pixels());
    for _ in 1..params.steps {
        carried = carry(carried.as_picture().pixels());
    }
    carried
}

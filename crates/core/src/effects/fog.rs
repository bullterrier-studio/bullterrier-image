//! `fog`: a veil of drifting noise-shaped cloud, thicker toward the bottom, mixed in as a colour. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::edge::Edge;
use crate::ops::gauss::ndtr;
use crate::ops::noise::{Fractal, field_rows, standardised};
use crate::ops::sample::{Interpolation, Sampler};
use crate::ops::{cast, length, par};
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `fog` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Atmosphere,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `fog` reads around a pixel: the clouds are noise of the canvas place and the seed, so none of the picture but the
/// pixel itself. Read straight off (no `rise`, no `turbulence`) the noise is of deviation 1 at every pixel. Read between
/// pixels it is smoothed by an amount that depends on where, and the clouds are ranked again over the picture to hold their
/// `coverage`: that measures the whole picture.
pub(crate) fn footprint(params: &Params) -> Footprint {
    if reads_in_place(params) {
        Footprint::Pixel
    } else {
        Footprint::Whole
    }
}

/// Whether the clouds are read at the pixel itself.
#[expect(
    clippy::float_cmp,
    reason = "1 and 0 are the parameters' own values, at which a read is exactly in place"
)]
fn reads_in_place(params: &Params) -> bool {
    params.rise == 1.0 && params.turbulence == 0.0
}

/// Fog or smoke: mix the picture toward `color` by an opacity that is cloudy and heavier toward the bottom. Fog, mist, haze,
/// steam and smoke are settings of it.
///
/// The clouds are the high parts of a fractal noise field (`scale`, `octaves`, `persistence`, `lacunarity`), warped when it is
/// read (`resampling`, `edge`): a pixel the height d above the bottom edge reads the field at d / `rise` (so the clouds are
/// drawn out `rise` times upward from the bottom edge), pushed `turbulence` px along two more noise fields. The warped field
/// is standardised (over the picture, if it is warped) and passed through the normal cumulative distribution, giving u spread evenly over 0..1; the cloud shape
/// is smoothstep((u - (1 - `coverage`)) / `feather`), so about `coverage` of the picture has cloud and the shape climbs from
/// 0 to 1 across the `feather` share of pixels above the cut. The opacity at a pixel is `amount` x (1 - `gradient` x (1 - t))
/// x (`floor` + (1 - `floor`) x shape), where t is how far down the frame the pixel's row centre is (0 at the top edge, 1 at
/// the bottom edge). Each pixel becomes picture + opacity x (color - picture), so the fog lowers contrast as well as tinting.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How opaque the fog is where it is thickest (the bottom of the frame, in the densest cloud): the share of the fog
    /// colour mixed into the picture there. 0 = no fog, 0.5 = see-through, 1 = the colour itself, hiding the picture. Fog also
    /// lowers contrast, since every pixel moves toward the same colour.
    #[param(default = 0.77, unit = fraction, min = 0, max = 1, drivable)]
    pub amount: f32,
    /// The colour of the fog: '#rrggbb', '#rgb' or [r, g, b] 0..1. A pale blue-grey is a cool morning mist, white a thick
    /// cloud, a warm grey smog, a dark grey or black a heavy, sooty plume.
    #[param(default = "#cfd8e3")]
    pub color: Color,
    /// How much thicker the fog is at the bottom of the frame than at the top, as a share of its opacity: the opacity falls in
    /// a straight line from the bottom row up, to 1 - gradient of its value at the top row. 0 = the same all the way up, 1 =
    /// no fog at the very top, as for mist lying on the ground.
    #[param(default = 0.7, unit = fraction, min = 0, max = 1)]
    pub gradient: f32,
    /// About the share of the picture that has any cloud in it: 0 = none, 0.45 = clouds with clear air between them, 1 =
    /// cloud everywhere, in thicker and thinner places. The clouds are the highest parts of a noise field, so raising it
    /// widens them and joins them up.
    #[param(default = 0.93, unit = fraction, min = 0, max = 1)]
    pub coverage: f32,
    /// How soft the edge of a cloud is, as the share of the picture over which the fog thickens from its thinnest to full
    /// `amount` just above the coverage cut. Small = crisp, billowing outlines; 1 = clouds that melt into the air with no
    /// clear outline.
    #[param(default = 1.0, unit = fraction, min = 0.01, max = 1, advanced)]
    pub feather: f32,
    /// How much of the `amount` is left between the clouds, as a share: the opacity where there is no cloud is `amount` x
    /// `floor`. 0 = open gaps of clear air, like plumes of smoke; 1 = an even veil with no clouds at all; 0.5 = a haze that
    /// thins between drifts but never clears.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub floor: f32,
    /// Size of the biggest clouds across, in pixels, the spacing of the first noise lattice before `rise` stretches it; tuned
    /// for a picture about 480 px high. Larger = broad banks of fog; small = fine, wispy mottling.
    #[param(default = 150.0, unit = px, min = 1, max = 1024)]
    pub scale: f32,
    /// How much the clouds are drawn out upward, anchored at the bottom edge: 1 = round puffs, 2 = twice as tall as wide, 8 =
    /// long columns, like smoke from a chimney.
    #[param(default = 1.0, unit = ratio, min = 1, max = 8, advanced)]
    pub rise: f32,
    /// How far in pixels two more noise fields (with the same `scale`, `octaves`, `persistence` and `lacunarity`) push the
    /// clouds sideways and up and down: the typical displacement (their standard deviation). 0 = smooth, regular clouds;
    /// larger = twisting, curling smoke; much larger than `scale` tears the clouds into scattered wisps.
    #[param(default = 0.0, unit = px, min = 0, max = 500, advanced)]
    pub turbulence: f32,
    /// How many layers of noise are summed to make the clouds, each finer than the last (by `lacunarity`) and weaker (by
    /// `persistence`). 1 = smooth blobs, more = more wispy detail. Layers finer than one pixel are dropped.
    #[param(default = 4, unit = count, min = 1, max = 8, advanced)]
    pub octaves: i32,
    /// How much weaker each finer layer is than the one before: each layer's weight is this to the power of its number. 0 =
    /// only the coarsest layer, 1 = every layer equally strong (very rough).
    #[param(default = 0.5, unit = ratio, min = 0, max = 1, advanced)]
    pub persistence: f32,
    /// How much finer each layer is than the one before: the lattice spacing is divided by this per layer. 1 = every layer
    /// the same size, 2 = each half as coarse, larger = detail drops off faster in size.
    #[param(default = 2.0, unit = ratio, min = 1, max = 4, advanced)]
    pub lacunarity: f32,
    /// How the noise is read where `rise` and `turbulence` have moved it between pixels: 'nearest' takes the closest value
    /// (fastest, a little jagged), 'bilinear' blends the four around it (smooth), 'bicubic' fits a cubic spline through the
    /// neighbours (smoothest, a little slower). It matters only where something moves. Named `resampling`, not
    /// `interpolation`, because it reads a generated noise field, never the picture.
    #[param(default = Bilinear, advanced)]
    pub resampling: Interpolation,
    /// What the noise is beyond the picture's border when `turbulence` pushes a cloud in from outside: 'reflect' (mirrored,
    /// edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the
    /// opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
    /// Random seed: the same seed always gives the same clouds, another seed a different bank of fog.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// The picture with the fog mixed in.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let place = inputs.place(picture);
    let first_row = place.first_row;
    let lattice = Fractal {
        spacing: f64::from(params.scale),
        octaves: params.octaves.cast_unsigned(),
        persistence: f64::from(params.persistence),
        lacunarity: f64::from(params.lacunarity),
        seed: params.seed,
        origin: 0.0, // constant: a lattice point on the first pixel
    };
    // constant: one stream per field, numbered from 0 (the seed itself)
    let cloud = field_rows(first_row, height, width, &lattice, 0);
    let ranked = if reads_in_place(params) {
        cloud
    } else {
        // `Whole`: the picture is the canvas
        let sway_down = field_rows(0, height, width, &lattice, 1);
        let sway_across = field_rows(0, height, width, &lattice, 2);
        // the cloud field as a grey picture for the sampler to read between its pixels
        let cloud_pixels: Vec<[f32; 3]> = cloud.iter().map(|&z| [z; 3]).collect();
        let sampler = Sampler::new(&cloud_pixels, height, width, params.resampling, params.edge.extension());
        let (rise, turbulence) = (f64::from(params.rise), f64::from(params.turbulence));
        let mut warped = vec![0.0_f32; height * width];
        par::rows(&mut warped, width, |first, band| {
            for (at, out) in band.iter_mut().enumerate() {
                let index = first * width + at;
                let (row, column) = (index / width, index % width);
                // a pixel d above the bottom edge reads the field d / rise above it
                let read_row = (length::float(height) - 1.0) * (1.0 - 1.0 / rise) + length::float(row) / rise;
                *out = cast::single(
                    sampler.at(
                        read_row + turbulence * f64::from(sway_down[index]),
                        length::float(column) + turbulence * f64::from(sway_across[index]),
                    )[0],
                );
            }
        });
        standardised(warped)
    };
    let (coverage, feather, floor) = (
        f64::from(params.coverage),
        f64::from(params.feather),
        f64::from(params.floor),
    );
    let color = params.color.0;
    picture.map_at(|row, column, pixel| {
        let lift = ((ndtr(f64::from(ranked[row * width + column])) - (1.0 - coverage)) / feather).clamp(0.0, 1.0);
        let shape = lift * lift * (3.0 - 2.0 * lift); // constant: 3t^2 - 2t^3 is the standard smoothstep Hermite curve
        let down = (length::float(first_row + row) + 0.5) / length::float(place.canvas_height);
        let opacity = cast::single(
            f64::from(params.amount)
                * (1.0 - f64::from(params.gradient) * (1.0 - down))
                * (floor + (1.0 - floor) * shape),
        );
        [0, 1, 2].map(|channel| pixel[channel] + opacity * (color[channel] - pixel[channel]))
    })
}

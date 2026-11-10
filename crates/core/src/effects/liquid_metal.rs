//! `liquid_metal`: the picture's brightness as a relief, polished into tinted chrome. What it does is documented once,
//! on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter_plane;
use crate::ops::color::luminance;
use crate::ops::edge::Edge;
use crate::ops::relief::shade_slopes;
use crate::params::{Color, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `liquid_metal` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[],
};

/// What `liquid_metal` reads around a pixel: the smoothing's reach and a pixel on either side for the slope.
pub(crate) fn footprint(params: &Params, _canvas: &Canvas) -> Footprint {
    // constant: the kernel reaches int(truncate x smoothing + 0.5) px and the slope one more
    Footprint::reach(
        f64::from(params.smoothing * params.truncate) + 1.5,
        params.edge == Edge::Wrap,
    )
}

/// Chrome that follows the picture's brightness: its relief, `relief` px tall, mirrors a sky above a ground.
///
/// The height is the Rec. 709 luminance smoothed by `smoothing`; only brightness matters, the picture's hues are gone. A
/// viewer straight above sees, in a surface with slopes (dy, dx) down and across the picture, the environment at
/// t = 0.5 + dy / (1 + dx^2 + dy^2) from the ground (0) to the sky (1), 0.5 where the surface is flat. The
/// environment runs linearly from 0 on the ground to 0.5 at `horizon` to 1 at the sky, and the metal colours it from
/// black through `tint` to white.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Gaussian sigma in px with which the brightness is smoothed before it is read as height: 0 = every pixel is a bump
    /// and the metal is rough with the picture's texture and noise, larger = broad, flowing swells.
    #[param(default = 3.0, unit = px, min = 0, max = 20)]
    pub smoothing: f32,
    /// How tall the relief is: a white pixel stands this many px above a black one, so the slope between neighbours is
    /// their brightness difference times this. 0 = a flat mirror (one tint-coloured picture when the horizon is at 0.5),
    /// larger = steeper relief and a more contrasty, more broken-up reflection. Sizes are in px, so the defaults suit a
    /// picture about 480 px high: scale `smoothing` and `relief` with the picture's size.
    #[param(default = 35.0, unit = px, min = 0, max = 200)]
    pub relief: f32,
    /// Where the environment's horizon is, as a fraction of the way from the ground (0, looking straight down) to the sky
    /// (1, looking straight up), kept off the extremes so there is always some of both; a flat surface looks at 0.5. The
    /// metal is exactly `tint` at the horizon, darker towards the ground and lighter towards the sky: raising it turns
    /// more of the reflections dark, lowering it more of them light.
    #[param(default = 0.65, unit = fraction, min = 0.05, max = 0.95)]
    pub horizon: f32,
    /// The metal's colour as it looks at the horizon ('#rrggbb', '#rgb' or [r, g, b] 0..1): the metal runs from black on
    /// the ground through this to white in the sky. A light bluish silver by default; gold is '#e6b84d'.
    #[param(default = "#c9d6ff")]
    pub tint: Color,
    /// Where the smoothing Gaussian is cut off, in sigmas (only matters when `smoothing` > 0). Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the smoothing and the slope see beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated),
    /// 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge, so a
    /// tileable picture polishes without a seam).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The chrome of the picture's brightness relief.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let edge = params.edge.extension();
    let brightness: Vec<f32> = picture.pixels().iter().map(|&pixel| luminance(pixel)).collect();
    let surface = gaussian_filter_plane(
        &brightness,
        (height, width),
        f64::from(params.smoothing),
        f64::from(params.truncate),
        edge,
    );
    let (horizon, tint) = (params.horizon, params.tint.0);
    let mut pixels = vec![[0.0; 3]; height * width];
    shade_slopes(
        &surface,
        (height, width),
        params.relief,
        edge,
        &mut pixels,
        |_, rise_y, rise_x| {
            // The reflected ray's upward part, 2 n_y n_z in -1..1, as a height in 0..1.
            let looks_up = 0.5 + rise_y / (1.0 + rise_x * rise_x + rise_y * rise_y); // constant: n_y n_z of a unit normal, 0.5 = the flat surface
            // The metal's place on its black (0), tint (0.5), white (1) gradient: linear ground to horizon to sky.
            let shine = if looks_up < horizon {
                0.5 * looks_up / horizon // constant: the horizon sits at the gradient's middle
            } else {
                0.5 + 0.5 * (looks_up - horizon) / (1.0 - horizon) // constant: the horizon sits at the gradient's middle
            };
            let (towards_tint, towards_white) = ((2.0 * shine).min(1.0), (2.0 * shine - 1.0).max(0.0)); // constant: two halves of the gradient
            tint.map(|level| (level * towards_tint + (1.0 - level) * towards_white).clamp(0.0, 1.0))
        },
    );
    Image::from_pixels(height, width, pixels)
}

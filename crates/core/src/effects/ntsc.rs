//! `ntsc`: colour smeared along the scan lines and leaking into brightness, as in composite video. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::cast;
use crate::ops::chroma::smeared;
use crate::ops::colorspace::{rgb_to_yiq, yiq_to_rgb};
use crate::ops::edge::Edge;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `ntsc` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Glitch,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `ntsc` reads around a pixel: the dot crawl follows the parity of the row, so a row is not the same in another frame
/// of reference.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

/// Smear the colour of the picture along its rows and let a share of it crawl into brightness, like an NTSC signal.
///
/// In YIQ (FCC coefficients) the colour axes I and Q are blurred by a Gaussian along each row only, while brightness Y is
/// kept. Then the colour, as a modulated subcarrier, leaks into brightness: `crawl` of I cos t + Q sin t is added to Y, where
/// t = 2 pi (x / 4 + y / 2 + `phase`), because the NTSC subcarrier is sampled four times per cycle and flips phase on every
/// line. Greys have no colour, so they are not touched. The result is clipped to 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far the colour smears sideways, as the standard deviation in px of a Gaussian along each row, 0 to 100, tuned for
    /// a picture about 480 px high: 0 keeps the colour where it is, raising it bleeds colour further past the edges of what
    /// it belongs to. Brightness is not smeared.
    #[param(default = 4.0, unit = px, min = 0, max = 100)]
    pub sigma: f32,
    /// How much of the colour signal leaks into brightness as dot crawl, 0 to 1: a fine diagonal chequer on coloured areas,
    /// as strong as the colour is. 0 = none, 1 = the full colour signal; grey areas show none.
    #[param(default = 0.3, unit = fraction, min = 0, max = 1)]
    pub crawl: f32,
    /// Where the chequer of the dot crawl is in its cycle, 0 to 1 (1 is the same as 0): moving it sideways through a full
    /// cycle makes the dots crawl; half a cycle swaps light dots and dark dots.
    #[param(default = 0.0, unit = cycles, min = 0, max = 1)]
    pub phase: f32,
    /// Where the smear's Gaussian is cut off, in sigmas, 1 to 10: the colour spreads over about this many sigmas on either
    /// side. Lower is faster and gives a slightly sharper tail.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What colour is smeared in from beyond the left and right edges: 'reflect' the picture mirrored (edge pixel repeated),
    /// 'nearest' its edge pixel repeated, 'mirror' mirrored about the edge pixel, 'wrap' the opposite side.
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture with its colour smeared and crawling.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let planes = smeared(
        picture.pixels(),
        (height, width),
        0.0, // constant: the colour is smeared in place, not moved
        (f64::from(params.sigma), f64::from(params.truncate)),
        params.edge.extension(),
    );
    // The subcarrier is sampled four times per cycle (SMPTE 244M), so t advances a quarter cycle with each column, and half
    // a cycle with each row: the (cos t, sin t) of the four columns are those of column 0 turned by a quarter each time, and
    // an odd row has them negated.
    // the lines alternate from the top of the canvas, whichever row of it the picture starts at
    let first_row = inputs.place(picture).first_row;
    let turn = std::f64::consts::TAU * f64::from(params.phase);
    let (cos, sin) = (cast::single(turn.cos()), cast::single(turn.sin()));
    let subcarrier = [[cos, sin], [-sin, cos], [-cos, -sin], [sin, -cos]];
    picture.map_at(|row, column, pixel| {
        let [luma, _, _] = rgb_to_yiq(pixel);
        let at = row * 2 * width + column;
        let (i, q) = (planes[at], planes[at + width]);
        let [cos, sin] = subcarrier[column % subcarrier.len()];
        let line = if (first_row + row) % 2 == 0 { 1.0 } else { -1.0 }; // constant: the subcarrier flips phase on every line
        let crawl = params.crawl * line * (i * cos + q * sin);
        yiq_to_rgb([luma + crawl, i, q]).map(|level| level.clamp(0.0, 1.0))
    })
}

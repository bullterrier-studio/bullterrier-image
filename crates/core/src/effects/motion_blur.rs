//! `motion_blur`: the picture smeared along a straight line, as by a moving camera or subject. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::convolve::{Kernel, convolve};
use crate::ops::edge::Edge;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `motion_blur` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `motion_blur` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Smear the picture along a straight line of `length` px at `angle` degrees, as a moving camera would.
///
/// Every pixel becomes the average of the picture along that line, each colour channel on its own, so flat areas stay
/// flat, the mean is kept and the result never leaves the picture's own range. The cost hardly grows with the length.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The direction of the motion, in degrees clockwise from pointing right on the screen, 0 to 360: 0 = horizontal,
    /// 90 = vertical, 45 = falling to the lower right. Opposite directions (0 and 180) give the same blur.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// How far the smear reaches along the line, end to end, in px, tuned for a picture about 480 px high: larger is a
    /// faster motion; 0 leaves the picture unchanged.
    #[param(default = 24.0, unit = px, min = 0, max = 500, drivable)]
    pub length: f32,
    /// What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The weights of a straight line `length` px long through the middle of a square of pixels: each pixel weighs the length
/// of line inside it (a 1 x 1 square). Rows count downwards, columns to the right; `angle` is clockwise from the columns'
/// direction, in degrees.
fn line(angle: f64, length: f64) -> Kernel {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a reach in px, small and positive"
    )]
    let reach = (length / 2.0).ceil() as usize;
    let (right, down) = (angle.to_radians().cos(), angle.to_radians().sin());
    Kernel::from_fn((reach, reach), |row, col| {
        // The stretch of the line, as distance t from its middle, inside the pixel: where it is within half a pixel of the
        // pixel's centre along both axes. A step of 0 divides to infinity, which is the right limit: the line is then
        // inside the span of the pixel's row or column at every t, or never.
        let (mut low, mut high) = (-length / 2.0, length / 2.0);
        for (centre, step) in [(col, right), (row, down)] {
            let ends = [(centre - 0.5) / step, (centre + 0.5) / step]; // constant: half a pixel, centre to edge
            low = low.max(ends[0].min(ends[1]));
            high = high.min(ends[0].max(ends[1]));
        }
        (high - low).max(0.0)
    })
}

/// The picture smeared along the line.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    if params.length == 0.0 {
        return picture.map(|pixel| pixel);
    }
    let kernel = line(f64::from(params.angle), f64::from(params.length));
    let pixels = convolve(picture.pixels(), height, width, &kernel, params.edge.extension());
    Image::from_pixels(height, width, pixels)
}

//! `ripple`: a wave travelling across the picture that moves each pixel sideways to its own direction. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `ripple` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `ripple` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Move every pixel by a sine wave of `wavelength` and `amplitude` that travels in the direction `angle`.
///
/// A pixel at distance s px from the picture's centre measured along the wave's direction (`angle` degrees clockwise from
/// pointing right) is moved by amplitude x sin(2 pi s / wavelength + phase) px, at a right angle to that direction (down
/// for angle 0, left for angle 90), and the point between pixels is read by `interpolation`. The wave is at its phase
/// along the line through the centre, so turning `angle` turns the whole pattern about the centre. The result never
/// leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The distance in pixels from one crest of the wave to the next, along the wave's direction. Raising it gives long,
    /// lazy swells; lowering it gives tight, fine ripples (below about 4 px the wave is too fine for the pixels to show it).
    #[param(default = 40.0, unit = px, min = 2, max = 4000)]
    pub wavelength: f32,
    /// How far in pixels the wave moves a pixel at its crest, perpendicular to the wave's direction. Raising it makes
    /// taller waves and a more broken-up picture; 0 leaves the picture unchanged.
    #[param(default = 6.0, unit = px, min = 0, max = 500)]
    pub amplitude: f32,
    /// Where the wave starts, in degrees of one cycle: 0 has the wave at zero along the line through the picture's centre
    /// and rising away from it in the wave's direction, 90 has a crest there, and 360 is a whole cycle and so the same as
    /// 0. Changing it slides the wave along.
    #[param(default = 0.0, unit = degrees, min = -180, max = 180, cyclic)]
    pub phase: f32,
    /// The direction the wave travels in, in degrees clockwise from pointing right: 0 sends the wave along the picture's
    /// width, so each column is moved up or down and the crests are horizontal ripples running across; 90 sends it down
    /// the picture, so each row is moved left or right. Pixels are always moved at a right angle to this direction.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft),
    /// 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What the wave sees beyond the picture's edge (it moves pixels in from outside along the edges): 'reflect'
    /// (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated),
    /// 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture with the wave passed through it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let (sin, cos) = f64::from(params.angle).to_radians().sin_cos();
    let phase = f64::from(params.phase).to_radians();
    let (amplitude, wavelength) = (f64::from(params.amplitude), f64::from(params.wavelength));
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    // the wave is measured from the middle of the picture, between the middle two pixels of a side with an even number
    Centre::new(height, width, 0.5, 0.5).warp(&sampler, |spot| {
        let along = spot.across * cos + spot.down * sin;
        // a pixel shows what is `moved` px behind it
        let moved = amplitude * (std::f64::consts::TAU * along / wavelength + phase).sin();
        (spot.row - cos * moved, spot.column + sin * moved)
    })
}

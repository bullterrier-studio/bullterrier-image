//! `lens_distortion`: barrel or pincushion distortion, or a fisheye, with the picture scaled back. What it does is
//! documented once, on `Params`.

#![expect(
    clippy::doc_markdown,
    reason = "the choice 'brown_conrady' is spelt as the user writes it, in the help text"
)]

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::{Choice, Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `lens_distortion` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `lens_distortion` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// The curve of the bend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Model {
    /// The radial polynomial of the Brown-Conrady lens model (k1 only), which bends evenly from the axis out.
    BrownConrady,
    /// An equidistant fisheye, which keeps the middle almost flat and bends the edges hardest.
    Fisheye,
}

/// How the picture is scaled back after the bend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Fit {
    /// The farthest corner stays in its corner.
    Frame,
    /// The scale at the lens axis stays.
    Centre,
}

/// Bend the picture as a lens does, barrel, pincushion or fisheye, with the curve `model` and strength `amount` about the
/// axis `x`, `y`.
///
/// Lens distortion: the picture bent about the lens axis, by the radial polynomial of the Brown-Conrady model or as through
/// a fisheye lens, so straight lines bow outward (barrel) or inward (pincushion). Each pixel at distance d from the axis
/// shows the picture on the same ray at d x s(d / R), R the distance of the farthest corner. s is 1 at the axis and 1 +
/// `amount` at the farthest corner: for 'brown_conrady' 1 + `amount` x (d / R)^2 (the Brown-Conrady radial term, k1 only);
/// for 'fisheye' tan(u d / R) / (u d / R), the equidistant fisheye for a half field of view u (the arctangent, its
/// inverse, for a negative `amount`). With `fit` 'frame' s is divided by 1 + `amount` so that the corner stays put (a
/// barrel then reads only inside the picture). The axis never moves, and the result never leaves 0..1. What the bend reads
/// from beyond the picture is the `edge` rule's, or `border` where that is set.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The curve of the bend, with the same `amount` giving the same stretch at the farthest corner: 'brown_conrady' is the
    /// radial polynomial of the Brown-Conrady lens model (k1 only), which bends evenly from the axis out, as an ordinary
    /// lens does; 'fisheye' is an equidistant fisheye, a pixel's distance from the axis proportional to the angle it sees,
    /// which keeps the middle almost flat and bends the edges hardest, and for a negative `amount` is its inverse, an
    /// arctangent curve.
    #[param(default = BrownConrady)]
    pub model: Model,
    /// How strongly the lens bends: at the farthest corner each pixel's source lies 1 + amount times as far from the axis as
    /// the pixel's own place, where at the axis it lies at its own place (0.2 is 20% farther; 'fit' then scales it back).
    /// The distance from the axis is measured in units of the farthest corner's. Positive is barrel: straight lines bow
    /// outward, the middle swells and the picture looks wrapped around a ball. Negative is pincushion: lines bow inward and
    /// the corners are pulled out; -0.3 is the most the 'brown_conrady' curve can have without folding the picture over. 0
    /// leaves the picture unchanged. For a fisheye, 0.4 is about a 103 degree lens, 1 a 134 degree one and 3.5 a 162 degree
    /// one.
    #[param(default = 0.2, unit = ratio, min = -0.3, max = 3.5)]
    pub amount: f32,
    /// How the picture is scaled back after the bend: 'frame' keeps the farthest corner in its corner, so the picture fills
    /// the frame (a barrel is zoomed in and loses its sides, and nothing beyond the picture is shown; a pincushion is zoomed
    /// out toward the middle and pulls in what lies beyond the middle of each edge, which the edge rule fills); 'centre'
    /// keeps the scale at the lens axis, so a barrel shrinks toward the middle, and the edge rule fills the corners it
    /// exposes.
    #[param(default = Frame)]
    pub fit: Fit,
    /// The lens axis's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 =
    /// the middle, 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub x: f32,
    /// The lens axis's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the
    /// middle, 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub y: f32,
    /// The colour shown where the bend reads from beyond the picture (with fit 'centre', or a pincushion): '#rrggbb',
    /// '#rgb' or [r, g, b] 0..1; none (the default) fills it by the `edge` rule instead. A source less than a pixel beyond
    /// the picture is blended into the border, so its edge is smooth. A black border is a bezel round a curved screen.
    #[param()]
    pub border: Option<Color>,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but lines turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft),
    /// 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What shows where the bend reads from beyond the picture (with fit 'centre', or a pincushion): 'reflect' (mirrored,
    /// edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the
    /// opposite edge). With a `border`, only what the bend reads from within a pixel of the edge, which blends into the
    /// border.
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The `u` for which the fisheye curve stretches the farthest corner by `amount` (not 0): tan(u) / u = 1 + `amount` for a
/// positive amount, atan(u) / u = 1 + `amount` for a negative one. Tangent rises and arctangent falls steadily in u, so
/// halving a bracket finds it.
fn fisheye_reach(amount: f64) -> f64 {
    let barrel = amount > 0.0;
    let law = |u: f64| if barrel { u.tan() } else { u.atan() };
    // constant: tan reaches infinity at pi / 2; atan(u) / u is 1 + amount at a u below 2 / (1 + amount) (at most 2 / 0.7)
    let (mut low, mut high) = (
        0.0,
        if barrel {
            std::f64::consts::FRAC_PI_2
        } else {
            2.0 / (1.0 + amount)
        },
    );
    // constant: 60 halvings shrink a bracket of a few radians below double precision
    for _ in 0..60 {
        let middle = f64::midpoint(low, high);
        // the stretch rises with u for tan, falls for atan
        if (law(middle) / middle < 1.0 + amount) == barrel {
            low = middle;
        } else {
            high = middle;
        }
    }
    f64::midpoint(low, high)
}

/// The picture bent as a lens does.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let (x, y, amount) = (f64::from(params.x), f64::from(params.y), f64::from(params.amount));
    // the farthest corner, in pixels: where the radius is 1
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let farthest = (y.max(1.0 - y) * (height - 1) as f64).hypot(x.max(1.0 - x) * (width - 1) as f64);
    if params.amount == 0.0 || farthest == 0.0 {
        return picture.map(|pixel| pixel);
    }
    let reach = match params.model {
        Model::BrownConrady => 0.0,
        Model::Fisheye => fisheye_reach(amount),
    };
    // the scale back: the corner stays put in the frame, or the axis keeps its scale
    let refit = match params.fit {
        Fit::Frame => 1.0 / (1.0 + amount),
        Fit::Centre => 1.0,
    };
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    let source = |spot: crate::ops::centre::Spot| {
        let radius = spot.distance() / farthest;
        let stretch = match params.model {
            Model::BrownConrady => 1.0 + amount * radius * radius,
            Model::Fisheye => {
                let angle = reach * radius;
                // at the axis itself, the limit 1 of law(angle) / angle
                if angle > 0.0 {
                    (if amount > 0.0 { angle.tan() } else { angle.atan() }) / angle
                } else {
                    1.0
                }
            }
        };
        let scale = stretch * refit;
        spot.place_at(spot.down * scale, spot.across * scale)
    };
    let centre = Centre::new(height, width, x, y);
    match params.border {
        None => centre.warp(&sampler, source),
        Some(Color(border)) => centre.warp_bordered(&sampler, border, source),
    }
}

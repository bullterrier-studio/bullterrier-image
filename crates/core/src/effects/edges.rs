//! `edges`: an edge map of the picture, drawn as a picture. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur::{gaussian_filter_owned, gaussian_filter_rows};
use crate::ops::color::luminance;
use crate::ops::edge::{Edge, Extension};
use crate::ops::edges::{at, crests, sobel, sobel_of_line};
use crate::ops::length::whole;
use crate::ops::par;
use crate::params::{Choice, Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `edges` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[],
};

/// The second Gaussian of the difference is this many times the first.
const DOG_RATIO: f32 = 1.6; // constant: Marr and Hildreth (1980) show this ratio makes the difference of Gaussians closely match the Laplacian of a Gaussian

/// What `edges` reads around a pixel: the smoothing and the gradient, and for `canny` lines that may join across the
/// whole picture.
pub(crate) fn footprint(params: &Params) -> Footprint {
    let reach = f64::from(params.radius * params.truncate);
    match params.method {
        Method::Sobel => Footprint::reach(reach, params.edge == Edge::Wrap),
        Method::Dog => Footprint::reach(reach * f64::from(DOG_RATIO), params.edge == Edge::Wrap),
        Method::Canny => Footprint::Whole,
    }
}

/// How far `edges` paints beyond the picture's edge: as far as it reads.
pub(crate) fn spread(params: &Params) -> crate::Spread {
    crate::Spread::of_footprint(footprint(params))
}

/// How edges are found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Method {
    /// The steepness of the brightness: wide, soft bands that follow the picture's gradients.
    Sobel,
    /// The same steepness reduced to thin connected lines: the weakest lines are dropped unless they join a strong one.
    Canny,
    /// Sketch ink lines: the difference of Gaussians itself, so one line along each dark stroke and on the dark side of
    /// each edge.
    Dog,
}

/// Edge detection and sketch ink lines in `color` on `background`, by Sobel, Canny or difference of Gaussians.
///
/// The picture's brightness (Rec. 709 luminance) is smoothed by `radius`, and how strongly a pixel reads as an edge (a
/// black-to-white step reads 1, whichever method and radius) decides how fully it is drawn: not at all below `threshold` x
/// (1 - `softness`), fully from threshold x (1 + softness), faded between. 'sobel' and 'canny' read the steepness (the
/// Sobel gradient); 'canny' keeps only the crest of each edge, one or two pixels wide, and drops the weak ones that join
/// no strong one. 'dog' reads the difference between two Gaussians (`radius` and 1.6 times that) itself, not its
/// steepness, so it draws one ink line along each dark stroke and on the dark side of each edge, of features of about
/// that size. Flat areas are always `background`. Screened over the picture (a step's `blend_mode = "screen"`) it lights
/// up the edges; a dark `color` on a light `background` is line art.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How edges are found: 'sobel' (the steepness of the brightness: wide, soft bands that follow the picture's
    /// gradients), 'canny' (the same steepness reduced to thin connected lines: the weakest lines are dropped unless they
    /// join a strong one) or 'dog' (sketch ink lines: the difference of Gaussians itself, so one line along each dark
    /// stroke and on the dark side of each edge, from features between `radius` and 1.6 times `radius` in size; slow
    /// gradients and fine texture fall away).
    #[param(default = Canny)]
    pub method: Method,
    /// How far the picture is smoothed before edges are looked for, as the Gaussian sigma in px at the recipe's size
    /// ('dog': the finer of its two Gaussians). Raising it drops fine detail and noise and makes broader, smoother edges.
    #[param(default = 1.5, unit = px, min = 0.5, max = 50)]
    pub radius: f32,
    /// How strong an edge must be to show, as a share of the strongest edge there can be after smoothing (black straight
    /// to white = 1; a faint edge is near 0.05). Raising it keeps only the strongest edges.
    #[param(default = 0.1, unit = fraction, min = 0.01, max = 1)]
    pub threshold: f32,
    /// How gradually edges fade in around `threshold`: they are fully drawn from threshold x (1 + softness) and not at all
    /// below threshold x (1 - softness), and 'canny' also keeps a line down to that lower value if it touches one above
    /// the upper. 0 = every edge fully drawn or not at all; raising it adds weaker edges as faint lines.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub softness: f32,
    /// The colour of the edges: '#rrggbb', '#rgb' or [r, g, b] 0..1. Laid over a picture with the blend mode 'screen' only
    /// the edges light up; dark on a light `background` it is line art.
    #[param(default = "#ffffff")]
    pub color: Color,
    /// The colour where there is no edge: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black suits 'screen' blending, a light
    /// colour with a dark `color` gives a drawing.
    #[param(default = "#000000")]
    pub background: Color,
    /// Where the Gaussians are cut off, in sigmas: they reach truncate x radius px. Larger is more exact and slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the smoothing and the gradient see beyond the picture's border: 'reflect' (mirrored, edge pixel repeated),
    /// 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The strength of the strongest edge the method can see at `radius`: a step from black straight to white, found by
/// running the method on a row of one wide enough that its ends cannot touch the step. For 'dog' it is the difference of
/// the two blurs, for the others the Sobel gradient.
fn steepest(params: &Params) -> f32 {
    let (radius, truncate) = (f64::from(params.radius), f64::from(params.truncate));
    let reach = whole(truncate * radius * f64::from(DOG_RATIO)) + 3; // constant: 3 px of margin beyond the smoothing
    let mut narrow: Vec<f32> = (0..2 * reach).map(|column| f32::from(column >= reach)).collect();
    let step = narrow.clone();
    gaussian_filter_rows(&mut narrow, 2 * reach, radius, truncate, Extension::Nearest);
    if params.method == Method::Dog {
        let mut wide = step;
        gaussian_filter_rows(
            &mut wide,
            2 * reach,
            radius * f64::from(DOG_RATIO),
            truncate,
            Extension::Nearest,
        );
        wide.iter()
            .zip(&narrow)
            .map(|(wide, narrow)| wide - narrow)
            .fold(0.0, f32::max)
    } else {
        sobel_of_line(&narrow)
    }
}

/// The edge map of the picture, drawn in `color` on `background`.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let (radius, truncate) = (f64::from(params.radius), f64::from(params.truncate));
    let mut grey = vec![0.0_f32; height * width];
    par::rows(&mut grey, width, |first, band| {
        let from = &picture.pixels()[first * width..];
        for (out, &pixel) in band.iter_mut().zip(from) {
            *out = luminance(pixel);
        }
    });
    let extension = params.edge.extension();
    let scale = 1.0 / steepest(params);
    let lower = params.threshold * (1.0 - params.softness);
    let upper = params.threshold * (1.0 + params.softness);
    // how fully a pixel of this steepness is drawn: a ramp from `lower` to `upper`, or a step at both when they meet
    let drawn = |steepness: f32| {
        if upper > lower {
            ((steepness - lower) / (upper - lower)).clamp(0.0, 1.0)
        } else {
            f32::from(steepness > upper)
        }
    };
    let (background, color) = (params.background.0, params.color.0);
    let paint =
        |amount: f32| [0, 1, 2].map(|channel| background[channel] + (color[channel] - background[channel]) * amount);
    let mut pixels = vec![[0.0; 3]; height * width];
    match params.method {
        Method::Dog => {
            let narrow = gaussian_filter_owned(grey.clone(), (height, width), radius, truncate, extension);
            let wide = gaussian_filter_owned(
                grey,
                (height, width),
                radius * f64::from(DOG_RATIO),
                truncate,
                extension,
            );
            par::rows(&mut pixels, width, |first, band| {
                for (index, out) in band.iter_mut().enumerate() {
                    let cell = first * width + index;
                    *out = paint(drawn((wide[cell] - narrow[cell]) * scale));
                }
            });
        }
        Method::Sobel | Method::Canny => {
            let smooth = gaussian_filter_owned(grey, (height, width), radius, truncate, extension);
            let slopes = sobel(&smooth, (height, width), params.edge, scale);
            let kept = (params.method == Method::Canny).then(|| crests(&slopes, (height, width), lower, upper));
            par::rows(&mut pixels, width, |first, band| {
                for (offset, line) in band.chunks_exact_mut(width).enumerate() {
                    for (column, out) in line.iter_mut().enumerate() {
                        let cell = at(width, first + offset, column);
                        let amount = drawn(slopes[cell].strength);
                        *out = paint(if kept.as_ref().is_none_or(|kept| kept[cell]) {
                            amount
                        } else {
                            0.0
                        });
                    }
                }
            });
        }
    }
    Image::from_pixels(height, width, pixels)
}

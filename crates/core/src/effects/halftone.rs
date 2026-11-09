//! `halftone`: a print-style dot screen, where tone becomes dot size. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter;
use crate::ops::cast;
use crate::ops::color::luminance;
use crate::ops::edge::EdgeOrConstant;
use crate::ops::length::{float, whole};
use crate::ops::par;
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `halftone` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[],
};

/// What `halftone` reads around a pixel: the grid of dots is anchored at the picture's corner, so a row is not the same
/// in another frame of reference.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// A print-style dot screen: tone becomes dot size.
///
/// A rotated square grid of dots, `cell` px apart, turned `angle` degrees. Each dot looks up the picture, blurred by a
/// Gaussian of `sigma` cells, at its centre, and is as large as the ink that tone needs: the area of a dot is
/// proportional to the ink, so the screen keeps the picture's tones, dots touch at half tone and fill their cell at full
/// tone. Ink is the Rec. 709 luminance on a dark `paper` (below `paper_threshold`) and its complement on a light one. The
/// dot's edge is a ramp `feather` px wide, centred on its radius and faded in with the dot so that a dot narrower than
/// the ramp prints about its own area, and bare paper for zero ink. A dot is `ink`, or with no `ink` the colour of the
/// photo under it, scaled up until its strongest channel is full but by at most 1 / `ink_floor`.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Distance between neighbouring dots in px (the screen's pitch). Larger = fewer, bigger dots, a coarser print.
    #[param(default = 14, unit = px, min = 1, max = 200)]
    pub cell: i32,
    /// Rotation of the dot grid in degrees, clockwise on screen: rows of dots run that many degrees below the x axis (340
    /// tilts them 20 degrees above it). The grid is square, so 90 degrees on looks the same; 45 is the classic screen
    /// angle for one ink.
    #[param(default = 45.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// The ink: one colour for every dot ('#rrggbb', '#rgb' or three numbers 0..1); leave it out to give each dot the
    /// colour of the photo under it, brightened so small dots stay visible (by at most 1 / `ink_floor`).
    #[param()]
    pub ink: Option<Color>,
    /// With no `ink`: the most a dot's colour is brightened, as a floor 0.01 to 1 on its strongest channel: a colour whose
    /// strongest channel is below it is scaled by 1 / `ink_floor` instead of up to full, so dark areas stay dark. Lowering it
    /// brightens dark colours more; 1 = colours as they are.
    #[param(default = 0.35, unit = fraction, min = 0.01, max = 1)]
    pub ink_floor: f32,
    /// The paper colour between the dots. On a dark paper (Rec. 709 luminance below `paper_threshold`) bright tones get the
    /// ink; on a light one, dark tones.
    #[param(default = "#000000")]
    pub paper: Color,
    /// The Rec. 709 luminance, 0 to 1, below which `paper` counts as dark (bright tones get the ink) rather than light
    /// (dark tones get it); 0.5 = mid grey. Raising it lets lighter paper count as dark: 0 treats every paper as light, 1
    /// every paper but white as dark.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub paper_threshold: f32,
    /// Gaussian sigma, in cells, of the blur that averages the tone each dot stands for: 0 = each dot sees only the pixel at
    /// its centre (detail, but noisy), larger = smoother tone that spreads into neighbouring dots.
    #[param(default = 0.5, unit = ratio, min = 0, max = 5)]
    pub sigma: f32,
    /// Width of each dot's soft edge in px (anti-aliasing): small = crisp dots, larger = blurrier dots. The ink stays the
    /// same.
    #[param(default = 1.0, unit = px, min = 0.1, max = 50)]
    pub feather: f32,
    /// Where the tone blur's Gaussian is cut off, in sigmas: it reaches truncate x sigma cells. Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the tone blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant'
    /// (`constant_value`).
    #[param(default = Reflect, advanced)]
    pub edge: EdgeOrConstant,
    /// The brightness (0..1) assumed beyond the edge when edge is 'constant'; ignored otherwise.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub constant_value: f32,
}

/// `value` rounded to a pixel index no further than `last`.
fn pixel(value: f32, last: usize) -> usize {
    whole(f64::from(value)).min(last)
}

/// The dot screen.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let cell = cast::single(f64::from(params.cell));
    let blurred;
    let smooth = if params.sigma > 0.0 {
        blurred = gaussian_filter(
            picture,
            f64::from(cell) * f64::from(params.sigma),
            f64::from(params.truncate),
            params.edge.extension(params.constant_value),
        );
        &blurred
    } else {
        picture.pixels()
    };
    // a full turn is exactly the first, so min and max of the angle give the same screen
    let turn = f64::from(params.angle.rem_euclid(360.0)).to_radians(); // constant: a full turn in degrees
    let (sin, cos) = (cast::single(turn.sin()), cast::single(turn.cos()));
    let paper = params.paper.0;
    let ink_is_bright = luminance(paper) < params.paper_threshold;
    let (inv_cell, inv_feather) = (1.0 / cell, 1.0 / params.feather);
    let mut pixels = vec![[0.0; 3]; height * width];
    par::rows(&mut pixels, width, |first, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let y = cast::single(float(first + offset));
                        for (column, out) in line.iter_mut().enumerate() {
                let x = cast::single(float(column));
                // the pixel in the rotated grid, in cells, and the centre of the dot it belongs to
                let (u, v) = ((x * cos + y * sin) * inv_cell, (y * cos - x * sin) * inv_cell);
                let (cu, cv) = (u.floor() + 0.5, v.floor() + 0.5);
                let distance = ((u - cu) * (u - cu) + (v - cv) * (v - cv)).sqrt();
                // the tone at the dot's centre, mapped back to the picture
                let column = pixel((cu * cos - cv * sin) * cell, width - 1);
                let row = pixel((cu * sin + cv * cos) * cell, height - 1);
                let sampled = smooth[row * width + column];
                let tone = luminance(sampled);
                // the radius of a dot in cells: its area follows the ink, and a dot of full ink reaches the corners of its cell
                let radius = (if ink_is_bright { tone } else { 1.0 - tone }).clamp(0.0, 1.0).sqrt()
                    * std::f32::consts::FRAC_1_SQRT_2;
                let fade_in = (radius * cell * 2.0 * inv_feather).min(1.0); // constant: the ramp is half a feather on each side
                let coverage = ((radius - distance) * cell * inv_feather + 0.5).clamp(0.0, 1.0) * fade_in; // constant: the ramp is centred on the edge
                *out = if coverage > 0.0 {
                    let dot = params.ink.map_or_else(
                        || {
                            let strongest = sampled[0].max(sampled[1]).max(sampled[2]).max(params.ink_floor);
                            sampled.map(|level| (level / strongest).clamp(0.0, 1.0))
                        },
                        |ink| ink.0,
                    );
                    [0, 1, 2].map(|channel| paper[channel] * (1.0 - coverage) + dot[channel] * coverage)
                } else {
                    paper
                };
            }
        }
    });
    Image::from_pixels(height, width, pixels)
}

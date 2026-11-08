//! `rgb_split`: red pushed one way along an angle, blue (and for a red-cyan pair green) the other way. What it does is
//! documented once, on `Params`.

#![expect(clippy::doc_markdown, reason = "the choices 'red_blue' and 'red_cyan' are spelt as the user writes them, in the help text")]

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::edge::Edge;
use crate::ops::shift::shifted;
use crate::params::{Choice, Params};
use crate::{Image, Inputs, Picture};

/// Where `rgb_split` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Glitch,
    opacity: 1.0,
    inputs: &[],
};

/// What `rgb_split` reads around a pixel.
pub(crate) fn footprint(params: &Params) -> Footprint {
    Footprint::reach(f64::from(params.amount), params.edge == Edge::Wrap)
}

/// Which channels part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Pair {
    /// Red moves one way and blue the other; green stays in place.
    RedBlue,
    /// Red moves one way and green and blue together the other.
    RedCyan,
}

/// Push red `amount` px along `angle` and the opposite channels `amount` px the other way: an RGB split, a glitch fringe or
/// a red-cyan 3D anaglyph.
///
/// Red moves `amount` px in the direction `angle` (degrees clockwise from pointing right: 0 right, 90 down); blue moves
/// `amount` px the opposite way. Green stays with 'red_blue' and moves with blue with 'red_cyan'. On a grey picture the
/// channels end up exactly twice `amount` px from each other in a row, which shows as a red fringe on one side of an edge and a
/// blue (or cyan) one on the other. Whole or fractional distances are allowed; what a channel shows where it has left the
/// picture is set by `edge`.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Which channels part: 'red_blue' moves red one way and blue the other and leaves green in place (colour fringes on a
    /// colour picture); 'red_cyan' moves red one way and green and blue together the other, as in 3D glasses (grey the
    /// picture first, e.g. with `grade` at saturation 0, for the classic anaglyph).
    #[param(default = RedBlue)]
    pub pair: Pair,
    /// The direction red moves, in degrees clockwise from pointing right on the screen (as CSS `rotate`), 0 to 360: 0 =
    /// right, 90 = down, 180 = left, 270 = up. Blue (and green with 'red_cyan') moves the opposite way.
    #[param(default = 330.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// How far red and its opposite channels each move from where they were, in pixels, 0 to 500; they end up twice this
    /// far apart. 0 leaves the picture unchanged. A fraction of a pixel blends the two pixels it lies between. Raising it
    /// pulls the colour fringes further apart.
    #[param(default = 8.0, unit = px, min = 0, max = 500)]
    pub amount: f32,
    /// What a moved channel shows where it has left the picture's edge: 'nearest' repeats the channel's edge pixels (a
    /// coloured smear along the edge), 'wrap' brings back what left on the other side, 'reflect' mirrors the channel (the
    /// edge line repeated once), 'mirror' mirrors it about the edge line.
    #[param(default = Nearest, advanced)]
    pub edge: Edge,
}

/// The picture with its channels moved apart.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let radians = f64::from(params.angle).to_radians();
    let dx = f64::from(params.amount) * radians.cos();
    // rows count down, so a positive angle (clockwise) moves toward more rows
    let dy = f64::from(params.amount) * radians.sin();
    let edge = params.edge.extension();
    let pixels = picture.pixels();
    let red = shifted(pixels, height, width, dy, dx, edge);
    let opposite = shifted(pixels, height, width, -dy, -dx, edge);
    let green_moves = params.pair == Pair::RedCyan;
    let split = pixels.iter().zip(&red).zip(&opposite).map(|((own, red), opposite)| {
        [red[0], if green_moves { opposite[1] } else { own[1] }, opposite[2]]
    });
    Image::from_pixels(height, width, split.collect())
}

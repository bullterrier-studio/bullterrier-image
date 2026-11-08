//! `flow_lines`: thin lines that follow a smooth noise flow field, in place of the picture. What it does is documented once,
//! on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::noise::{Fractal, field};
use crate::ops::random::Rng;
use crate::ops::{cast, length};
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `flow_lines` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Backgrounds,
    opacity: 1.0,
    inputs: &[],
};

/// Replace the picture with streamlines of a noise flow: lines that start at random points and follow the flow.
///
/// A generated background: the input is used only for its height and width, its content is ignored. The flow's direction
/// at each pixel is `twist` times a fractal noise field. Each of the `density` per 1000 px² lines starts at a random point
/// and follows the flow for `length` px in steps of `step` px (the direction read at the pixel the line is in; the steps are
/// `length` / `step` rounded), ending early where it would leave the picture. A soft disc `thickness` px across is stamped
/// at every point; where discs overlap the strongest counts, so lines stay as bold as one.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many lines: lines started per 1000 design px² (a 32 x 32 px area at the recipe's size), so any picture size looks
    /// alike; a 480 x 384 picture at 2 has about 370. 0 draws none. More = a denser weave; lines cross freely.
    #[param(default = 2.0, unit = density, min = 0, max = 50)]
    pub density: f32,
    /// How far each line follows the flow, in px along the line, tuned for a picture about 480 px high. It is drawn in steps
    /// of `step`. A line also ends where it reaches the edge of the picture.
    #[param(default = 120.0, unit = px, min = 1, max = 2000)]
    pub length: f32,
    /// The distance in px a line moves each step. Keep it at or below the line's thickness for a solid line; larger steps
    /// leave dotted lines.
    #[param(default = 1.0, unit = px, min = 0.5, max = 16, advanced)]
    pub step: f32,
    /// The width of the lines in px (a soft disc of this diameter is stamped at every step). Tuned for a picture about 480 px
    /// high.
    #[param(default = 1.5, unit = px, min = 0.5, max = 16)]
    pub thickness: f32,
    /// The size of the flow's coarsest swirls in px, the spacing of the first noise lattice. Larger = broader, gentler bends.
    /// Tuned for a picture about 480 px high.
    #[param(default = 120.0, unit = px, min = 4, max = 1024)]
    pub scale: f32,
    /// How many layers of noise are summed for the flow, each finer than the last (by `lacunarity`) and weaker (by
    /// `persistence`). 1 = smooth sweeping lines, more = wigglier ones.
    #[param(default = 2, unit = count, min = 1, max = 8)]
    pub octaves: i32,
    /// How much weaker each finer layer of the flow is than the one before. 0 = only the coarsest layer.
    #[param(default = 0.5, unit = ratio, min = 0, max = 1, advanced)]
    pub persistence: f32,
    /// How much finer each layer of the flow is than the one before: the lattice spacing is divided by this.
    #[param(default = 2.0, unit = ratio, min = 1, max = 4, advanced)]
    pub lacunarity: f32,
    /// How far the flow turns for each standard deviation of the noise: a line's direction is the noise times this, measured
    /// clockwise on screen from the right. 0 = every line runs straight to the right; larger = tighter, more varied swirls.
    #[param(default = 180.0, unit = degrees, min = 0, max = 720)]
    pub twist: f32,
    /// Seeds the flow and the line starts: the same seed gives the same picture. The starts and the flow are independent
    /// random streams of it.
    #[param(default = 0, unit = count, min = 0, max = 2147483647)]
    pub seed: u32,
    /// The colour of the lines.
    #[param(default = "#ffd84a")]
    pub line: Color,
    /// The colour behind the lines.
    #[param(default = "#12002b")]
    pub background: Color,
}

/// Stamps a soft disc of `radius` px at (`x`, `y`) onto `ink` (`height` x `width`): each pixel keeps the strongest disc over
/// it, coverage 1 within `radius` - 1/2 of the pixel's centre and falling to 0 at `radius` + 1/2 (a pixel's own width).
fn stamp(ink: &mut [f32], (height, width): (usize, usize), radius: f64, (x, y): (f64, f64)) {
    let reach = length::whole(radius.ceil()).cast_signed();
    let (corner_column, corner_row) = (length::whole(x.floor()).cast_signed(), length::whole(y.floor()).cast_signed());
    for row in corner_row - reach..=corner_row + reach {
        for column in corner_column - reach..=corner_column + reach {
            if !(0..height.cast_signed()).contains(&row) || !(0..width.cast_signed()).contains(&column) {
                continue;
            }
            let (centre_x, centre_y) = (length::float(column.cast_unsigned()) + 0.5, length::float(row.cast_unsigned()) + 0.5);
            let covers = (radius - (centre_x - x).hypot(centre_y - y) + 0.5).clamp(0.0, 1.0);
            let at = &mut ink[row.cast_unsigned() * width + column.cast_unsigned()];
            *at = at.max(cast::single(covers));
        }
    }
}

/// The lines, in the picture's size: the picture is used only for its height and width.
///
/// Sequential: every line is drawn one after the other into one picture of ink, and the strongest disc over a pixel wins, which
/// is the same whatever the order; the work (the stamps) is a share of the lines, not of the pixels.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // constant: density is counted per 1000 square px
    let lines = length::whole(f64::from(params.density) * length::float(height * width) / 1000.0);
    let moves = length::whole(f64::from(params.length) / f64::from(params.step));
    // constant: the flow is the lattice's stream 0 of the seed, independent of the starts' separate generator
    let flow = field(
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
    let twist = f64::from(params.twist).to_radians();
    let step = f64::from(params.step);
    let radius = f64::from(params.thickness) / 2.0;
    let mut starts = Rng::seeded(params.seed);
    let mut ink = vec![0.0_f32; height * width];
    for _ in 0..lines {
        let mut at = (starts.uniform() * length::float(width), starts.uniform() * length::float(height));
        for _ in 0..=moves {
            // a line ends where it leaves the picture
            if !(0.0..length::float(width)).contains(&at.0) || !(0.0..length::float(height)).contains(&at.1) {
                break;
            }
            stamp(&mut ink, (height, width), radius, at);
            let heading = twist * f64::from(flow[length::whole(at.1.floor()) * width + length::whole(at.0.floor())]);
            at = (at.0 + step * heading.cos(), at.1 + step * heading.sin());
        }
    }
    let (background, line) = (params.background.0, params.line.0);
    picture.map_at(|row, column, _| {
        let share = ink[row * width + column];
        [0, 1, 2].map(|channel| background[channel] + (line[channel] - background[channel]) * share)
    })
}

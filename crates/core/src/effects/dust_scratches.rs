//! `dust_scratches`: light and dark specks, hairs and scratches drawn on the picture, like a worn print. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::length;
use crate::ops::random::{mix, unit};
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `dust_scratches` sits in the catalogue: a worn print shows its marks partway, so a step lays them at 0.8.
pub(crate) const META: Meta = Meta {
    category: Category::Texture,
    opacity: 0.8,
    inputs: &[],
};

/// What `dust_scratches` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Draw light and dark specks, hairs and scratches at random places over the picture.
///
/// `density` marks per 1000 px² are specks (random radius up to `size` / 2, mostly small) or, for a share `hairs` of them,
/// one-pixel hairs up to `size` px long; `scratches` long lines lean up to `angle` from vertical and are `width` wide. Each
/// mark is `light` or `dark` with equal chance, solid inside, with antialiased edges (where a light mark crosses a dark one
/// the light one lies on top). A higher `density` keeps every mark of a lower one and adds more; the same `seed` always
/// gives the same marks.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many specks and hairs there are: marks per 1000 design px² (a 32 x 32 px area at the recipe's size), so any
    /// picture size looks alike. 0 = none, 0.6 is a light dusting, 3 is filthy.
    #[param(default = 0.6, unit = density, min = 0, max = 5)]
    pub density: f32,
    /// The size of the biggest dust mark in px: a speck's diameter or a hair's length. Most marks are smaller (speck radii
    /// are random, with many tiny ones); larger = bigger blotches and longer hairs.
    #[param(default = 8.0, unit = px, min = 1, max = 50)]
    pub size: f32,
    /// The share of the dust marks drawn as hairs (short thin lines in any direction) instead of round specks: 0 = only
    /// specks, 1 = only hairs.
    #[param(default = 0.15, unit = fraction, min = 0, max = 1, advanced)]
    pub hairs: f32,
    /// How many scratches: thin, long, nearly vertical lines, each running over half to all of the picture's height. 0 =
    /// none. A count, not a density: their width is in design px, so the same count looks alike at any picture size.
    #[param(default = 2, unit = count, min = 0, max = 100)]
    pub scratches: i32,
    /// The most a scratch may lean away from vertical, in degrees: each scratch gets a random lean up to this, to either
    /// side. 0 = all exactly vertical.
    #[param(default = 4.0, unit = degrees, min = 0, max = 45, advanced)]
    pub angle: f32,
    /// How wide a scratch is in px. Larger = bolder scratches.
    #[param(default = 1.0, unit = px, min = 0.5, max = 10, advanced)]
    pub width: f32,
    /// The colour of the light marks (dust on a dark print, scratches into the emulsion): '#rrggbb', '#rgb' or [r, g, b]
    /// 0..1. Each mark is light or dark with equal chance.
    #[param(default = "#ffffff")]
    pub light: Color,
    /// The colour of the dark marks (dust on the scanner glass): '#rrggbb', '#rgb' or [r, g, b] 0..1.
    #[param(default = "#000000")]
    pub dark: Color,
    /// Random seed: the same seed always puts the same marks in the same places, another seed others.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// A point (x, y) of the picture, in px.
type Point = (f64, f64);

/// Draws the segment from `start` to `end` (a speck has equal ends), `radius` px wide each side, into `cover` (`height` x
/// `width`): each pixel takes the larger of its value and how far the shape covers it, 0..1, soft by one pixel.
fn stamp(cover: &mut [f32], (height, width): (usize, usize), start: Point, end: Point, radius: f64) {
    // shortcut: the box around the shape, so a long leaning scratch visits its whole slanted box; a row-by-row span would
    // visit only the line, worth it once scratches lean far on a very large picture
    let reach = radius + 1.0;
    // the pixels the shape can touch (`whole` of a negative number is 0)
    let first = |coordinate: f64| length::whole(coordinate.floor());
    let top = first(start.1.min(end.1) - reach);
    let bottom = (first(start.1.max(end.1) + reach) + 1).min(height);
    let left = first(start.0.min(end.0) - reach);
    let right = (first(start.0.max(end.0) + reach) + 1).min(width);
    let along = (end.0 - start.0, end.1 - start.1);
    let length = along.0 * along.0 + along.1 * along.1;
    for row in top..bottom {
        for column in left..right {
            let (x, y) = (length::float(column) - start.0, length::float(row) - start.1);
            // how far along the segment (0..1) its nearest point to the pixel lies; a speck has no length to divide by
            let nearest = if length > 0.0 {
                ((x * along.0 + y * along.1) / length).clamp(0.0, 1.0)
            } else {
                0.0 // constant: a speck is its one point
            };
            let distance = (x - nearest * along.0).hypot(y - nearest * along.1);
            // a pixel is a unit box: covered fully within `radius` - 0.5 of the shape, not at all beyond `radius` + 0.5
            let covered = (radius + 0.5 - distance).clamp(0.0, 1.0);
            let cell = &mut cover[row * width + column];
            *cell = cell.max(crate::ops::cast::single(covered));
        }
    }
}

/// The picture with the marks drawn on it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let (mut light, mut dark) = (vec![0.0_f32; height * width], vec![0.0_f32; height * width]);
    // mark number `mark` of `stream`, its `part`-th number: a hash, so a higher count keeps every mark of a lower one
    let draw = |stream: u64, mark: usize, part: u64| unit(mix(params.seed, &[stream, mark as u64, part]));
    let size = f64::from(params.size);
    // constant: density is counted per 1000 square px
    let (rows, columns) = (length::float(height), length::float(width));
    let dust = length::whole(f64::from(params.density) * rows * columns / 1000.0);
    for mark in 0..dust {
        let [x, y, extent, tone, kind, direction] = [0, 1, 2, 3, 4, 5].map(|part| draw(0, mark, part));
        let centre = (x * columns, y * rows);
        let cover = if tone < 0.5 { &mut light } else { &mut dark };
        if kind < f64::from(params.hairs) {
            // a hair, up to `size` long, in any direction, one pixel wide: half a pixel each side of its line
            let (reach, turn) = (size * (0.5 + extent / 2.0), std::f64::consts::TAU * direction);
            let end = (centre.0 + reach * turn.cos(), centre.1 + reach * turn.sin());
            stamp(cover, (height, width), centre, end, 0.5); // constant: half a pixel each side
        } else {
            // a speck: many small, few large
            stamp(cover, (height, width), centre, centre, size / 2.0 * extent * extent);
        }
    }
    for mark in 0..params.scratches.cast_unsigned() as usize {
        let [x, extent, tone, lean] = [0, 1, 2, 3].map(|part| draw(1, mark, part));
        let reach = rows * (0.5 + extent / 2.0);
        let turn = f64::from(params.angle).to_radians() * (2.0 * lean - 1.0);
        let start = (x * columns, (rows - reach) * extent);
        let end = (start.0 + reach * turn.tan(), start.1 + reach);
        let cover = if tone < 0.5 { &mut light } else { &mut dark };
        stamp(cover, (height, width), start, end, f64::from(params.width) / 2.0);
    }
    picture.map_at(|row, column, pixel| {
        let at = row * width + column;
        let mut out = pixel;
        for (cover, colour) in [(&dark, params.dark.0), (&light, params.light.0)] {
            let weight = cover[at];
            out = [0, 1, 2].map(|channel| out[channel] * (1.0 - weight) + colour[channel] * weight);
        }
        out
    })
}

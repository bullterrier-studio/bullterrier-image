//! `lens_flare`: a halo, a ring and ghosts of one light, as stray light inside a lens draws them. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::light::screen;
use crate::params::{Color, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `lens_flare` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `lens_flare` reads around a pixel.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Pixel
}

/// Add a halo and a ring round the light at (`x`, `y`) and `ghosts` soft discs along the line from it through the
/// picture's centre, tinted `color` and screened over the picture by `amount`.
///
/// Every feature is a smooth bump of brightness, 1 where it is strongest and falling (1 - t^2)^2 to nothing at its
/// radius (`halo`, `size`) or half-width (`ring_width`), in px, drawn straight from its formula with no blur or warp.
/// Ghost i is centred at light + i x `spacing` x (picture centre - light) and has the brightness (ghosts - i + 1) /
/// ghosts. The picture only ever gets brighter.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The light's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 =
    /// the middle, 1 = the last.
    #[param(default = 0.75, unit = fraction, min = 0, max = 1)]
    pub x: f32,
    /// The light's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the
    /// middle, 1 = the last.
    #[param(default = 0.25, unit = fraction, min = 0, max = 1)]
    pub y: f32,
    /// How many ghosts (soft discs, the lens's reflections of the light) lie along the line from the light through
    /// the picture's centre; each is dimmer than the one before, the last 1/ghosts as bright as the first. 0 = none.
    #[param(default = 5, unit = count, min = 0, max = 12)]
    pub ghosts: i32,
    /// Where the ghosts lie on that line, as multiples of the distance from the light to the picture's centre: ghost
    /// number i sits i x spacing of the way from the light toward the centre. 1 puts the first on the centre, 2
    /// mirrors the light through it, more goes beyond; 0 piles them on the light. A light at the centre gives them
    /// no line, so they sit on it.
    #[param(default = 0.45, unit = ratio, min = 0, max = 3, advanced)]
    pub spacing: f32,
    /// The radius in px of each ghost, tuned for a picture about 480 px high: the disc is brightest at its middle and
    /// falls smoothly to nothing at this distance. Raising it makes bigger, softer ghosts.
    #[param(default = 30.0, unit = px, min = 1, max = 300)]
    pub size: f32,
    /// The radius in px of the soft glow round the light, tuned for a picture about 480 px high: brightest at the
    /// light and falling smoothly to nothing at this distance. 0 = no halo.
    #[param(default = 120.0, unit = px, min = 0, max = 600)]
    pub halo: f32,
    /// The radius in px of the thin ring round the light, tuned for a picture about 480 px high: the distance from the
    /// light at which the ring is brightest. 0 = no ring.
    #[param(default = 72.0, unit = px, min = 0, max = 600)]
    pub ring: f32,
    /// How far each side of its radius the ring reaches, in px tuned for a picture about 480 px high: it falls
    /// smoothly from full brightness on the radius to nothing this far off. Raising it makes a wider, softer ring.
    #[param(default = 12.0, unit = px, min = 1, max = 300, advanced)]
    pub ring_width: f32,
    /// How strongly the flare is screened over the picture: 0 = off, 1 = the light at full brightness where it is
    /// brightest (the light itself and the first ghost); above 1 it saturates to white sooner.
    #[param(default = 0.8, unit = ratio, min = 0, max = 10)]
    pub amount: f32,
    /// The tint of the flare: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is a warm white; white keeps the flare
    /// colourless.
    #[param(default = "#ffe2b8")]
    pub color: Color,
}

/// One feature of the flare: a bump of peak `weight` round `centre` (row, column), or round the circle of radius
/// `ring` there, reaching `reach` px each side.
struct Bump {
    centre: (f64, f64),
    reach: f64,
    ring: f64,
    weight: f64,
}

impl Bump {
    /// The bump's brightness at pixel (`row`, `column`): `weight` x (1 - t^2)^2 within `reach` of the circle, else 0.
    fn at(&self, row: usize, column: usize) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
        let (down, right) = (row as f64 - self.centre.0, column as f64 - self.centre.1);
        let outer = self.reach + self.ring;
        if down.abs() >= outer || right.abs() >= outer {
            return 0.0; // outside the square that holds the bump: skip the root
        }
        let away = (down.hypot(right) - self.ring) / self.reach;
        let bell = (1.0 - away * away).max(0.0);
        self.weight * bell * bell
    }
}

/// The picture with the flare screened over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    // the light lies on the canvas, of which the picture holds the rows from `first_row`
    let place = inputs.place(picture);
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let (last_row, last_column) = ((place.canvas_height - 1) as f64, (place.canvas_width - 1) as f64);
    let light = (f64::from(params.y) * last_row, f64::from(params.x) * last_column);
    let middle = (last_row / 2.0, last_column / 2.0);
    let (halo, ring, size) = (f64::from(params.halo), f64::from(params.ring), f64::from(params.size));
    let bump = |centre, reach, ring, weight| Bump {
        centre,
        reach,
        ring,
        weight,
    };
    // constant: a feature's peak brightness, before `amount` scales it
    let full = 1.0;
    let mut bumps = Vec::new();
    if params.halo > 0.0 {
        bumps.push(bump(light, halo, 0.0, full));
    }
    if params.ring > 0.0 {
        bumps.push(bump(light, f64::from(params.ring_width), ring, full));
    }
    for ghost in 1..=params.ghosts {
        let along = f64::from(ghost) * f64::from(params.spacing);
        let centre = (
            light.0 + along * (middle.0 - light.0),
            light.1 + along * (middle.1 - light.1),
        );
        let fade = f64::from(params.ghosts - ghost + 1) / f64::from(params.ghosts);
        bumps.push(bump(centre, size, 0.0, fade));
    }
    let tint = params.color.0.map(|channel| params.amount * channel);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the flare is summed in f64 and stored in the f32 picture"
    )]
    picture.map_at(|row, column, pixel| {
        let flare = bumps
            .iter()
            .map(|bump| bump.at(place.first_row + row, column))
            .sum::<f64>() as f32;
        screen(pixel, tint.map(|channel| channel * flare))
    })
}

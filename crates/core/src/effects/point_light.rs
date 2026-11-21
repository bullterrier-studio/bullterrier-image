//! `point_light`: a lamp at a place on the picture lights it, or takes light away. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::blur::{gaussian_filter_plane, is_fourier};
use crate::ops::color::luminance;
use crate::ops::colorspace::{linear_to_srgb_fast, srgb_to_linear_fast};
use crate::ops::edge::Edge;
use crate::ops::relief::shade_slopes;
use crate::params::{Color, FieldName, Params};
use crate::{Footprint, Image, Inputs, Picture, Spread};

/// constant: the sharpness of the glint, the exponent of a Blinn-Phong highlight; 60 is a tight glint, as on oily skin
const SHININESS: i32 = 60;
/// constant: the picture's brightness is smoothed a third as much as the field, because its detail is finer
const DETAIL_SMOOTHING: f32 = 3.0;

/// Where `point_light` sits in the catalogue: the lamp lies on the canvas, of which the picture holds the rows from
/// `first_row`.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `point_light` reads around a pixel: the smoothing's reach and a pixel on either side for the slope. A blur summed
/// by the Fourier transform follows the length of its lines: the whole.
pub(crate) fn footprint(params: &Params) -> Footprint {
    if is_fourier(f64::from(params.blur), f64::from(params.truncate)) {
        return Footprint::Whole;
    }
    // constant: the kernel reaches int(truncate x blur + 0.5) px and the slope one more
    Footprint::reach(
        f64::from(params.blur * params.truncate) + 1.5,
        params.edge == Edge::Wrap,
    )
}

/// How far `point_light` paints beyond the picture's edge: as far as it reads.
pub(crate) fn spread(params: &Params) -> Spread {
    Spread::of_footprint(footprint(params))
}

/// A lamp lights the picture: only the picture's own pixels are brightened (or darkened), so what is in it stays as it was.
///
/// The lamp stands over the picture at (`x`, `y`) and `z` high, and shines on a relief made of the picture's own shape:
/// the `field` (the depth, by default) `height` px tall plus the picture's brightness `detail` px tall. A surface that faces
/// the lamp gets more light than one that turns away; the light fades with its distance from the lamp to nothing at `radius`. The light is
/// added to the picture in linear light, in the lamp's `color`, so a bright part cannot burn out; where the lamp does not
/// reach, the picture keeps `ambient` of its brightness. `gain` below 0 takes that much light away instead, a lamp that
/// darkens. A `specular` glint rides on the picture's bright parts, as bright as the lamp's positive `gain`. With `gain` 0 and `ambient` 1 the picture is returned
/// unchanged.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The relief the lamp shines on, the higher the value the higher the ground: `depth` (the nearest stands highest),
    /// `light` (the picture's own brightness), `subject`, or a region the recipe declares.
    #[param(default = "depth")]
    pub field: FieldName,
    /// The lamp's horizontal place as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the
    /// middle, 1 = the last. Beyond 0 to 1 the lamp stands outside the picture.
    #[param(default = 0.3, unit = fraction, min = -0.5, max = 1.5)]
    pub x: f32,
    /// The lamp's vertical place as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle,
    /// 1 = the last.
    #[param(default = 0.3, unit = fraction, min = -0.5, max = 1.5)]
    pub y: f32,
    /// The colour of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1. A coloured lamp adds only that colour (a red lamp
    /// leaves green and blue as they were); for a negative `gain` it is the colour taken away.
    #[param(default = "#ffffff")]
    pub color: Color,
    /// How much light the lamp gives, where it is brightest: 0 = none, 1 = a full lamp, more burns the picture toward
    /// the lamp's colour. Below 0 it takes light away: -1 darkens the picture to black under the lamp.
    #[param(default = 1.0, unit = ratio, min = -4, max = 4)]
    pub gain: f32,
    /// How much of the picture's brightness is left where the lamp does not reach: 1 = all of it (the lamp only adds
    /// light), 0 = none (the picture is lit by the lamp alone, in a dark room). Lowering it makes the lamp's light
    /// stand out; it also dims the lit parts' surroundings.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub ambient: f32,
    /// How high the lamp stands above the tallest the relief can be, in px at the recipe's size. A low lamp rakes across
    /// the surface and shows its shape; a high one lights it flat.
    #[param(default = 144.0, unit = px, min = 5, max = 1000, advanced)]
    pub z: f32,
    /// How far the lamp reaches, in px at the recipe's size: the light falls smoothly from the lamp to nothing at this
    /// distance, and past it the picture keeps only its `ambient`. Raising it lights more of the picture.
    #[param(default = 432.0, unit = px, min = 50, max = 2000, advanced)]
    pub radius: f32,
    /// How much of a bright, glossy glint the lamp leaves on the picture's bright parts, 0 = none: dark parts get none,
    /// and a surface turned toward the lamp more. Raising it makes a stronger glint.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub specular: f32,
    /// How tall the whole range of the `field` stands as a relief, in px at the recipe's size: a field value of 1
    /// stands this many px above 0. 0 = flat (only `detail` and the distance shape the light); larger = steeper shapes
    /// that catch the light from the side.
    #[param(default = 400.0, unit = px, min = 0, max = 1000, advanced)]
    pub height: f32,
    /// How tall the picture's own brightness stands as a fine relief on top of the field's, in px at the recipe's size:
    /// bright parts stand proud, so pores, creases and fabric catch the light. 0 = the picture is not read as a shape.
    #[param(default = 6.0, unit = px, min = 0, max = 100, advanced)]
    pub detail: f32,
    /// Gaussian sigma in px with which the field is smoothed before its slopes are taken (the picture's brightness is
    /// smoothed a third as much, to keep its finer detail): 0 = every ripple of the field is lit, larger = softer
    /// lighting that follows only the broad shapes. A sharp step in the field (a subject's outline in depth) lights as a
    /// cliff and leaves a bright fringe beside it; the default softens it.
    #[param(default = 6.0, unit = px, min = 0, max = 50, advanced)]
    pub blur: f32,
    /// Where the smoothing Gaussian is cut off, in sigmas (only matters when `blur` > 0). Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the smoothing and the slope see beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated),
    /// 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture lit by the lamp.
#[expect(
    clippy::cast_precision_loss,
    reason = "a picture side and an index are far below 2**24 in the lamp's geometry"
)]
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let pixels = picture.pixels();
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(height, width, pixels.to_vec());
    };
    let edge = params.edge.extension();
    let (blur, truncate) = (f64::from(params.blur), f64::from(params.truncate));

    // the relief in px: the field `height` tall, then the picture's own brightness `detail` tall on top of it
    let mut relief = gaussian_filter_plane(field.values(), (height, width), blur, truncate, edge);
    for at in &mut relief {
        *at *= params.height;
    }
    if params.detail > 0.0 {
        let brightness: Vec<f32> = pixels.iter().map(|&pixel| luminance(pixel)).collect();
        let fine = gaussian_filter_plane(
            &brightness,
            (height, width),
            blur / f64::from(DETAIL_SMOOTHING),
            truncate,
            edge,
        );
        for (at, brightness) in relief.iter_mut().zip(fine) {
            *at += params.detail * brightness;
        }
    }

    // the lamp lies on the canvas, of which the picture holds the rows from `first_row`; it stands `z` px above the
    // tallest the relief can be
    let place = inputs.place(picture);
    let (canvas_width, canvas_height) = (place.canvas_width as f32, place.canvas_height as f32);
    let lamp_height = params.z;
    let lamp = [
        params.x * (canvas_width - 1.0),
        params.y * (canvas_height - 1.0),
        lamp_height + params.height + params.detail,
    ];
    let reach = params.radius;
    let mut out = vec![[0.0; 3]; height * width];
    shade_slopes(
        &relief,
        (height, width),
        1.0,
        edge,
        &mut out,
        |index, rise_y, rise_x| {
            let (column, row) = ((index % width) as f32, (place.first_row + index / width) as f32);
            let to_lamp = [lamp[0] - column, lamp[1] - row, lamp[2] - relief[index]];
            let distance = to_lamp.iter().map(|part| part * part).sum::<f32>().sqrt();
            let unit = to_lamp.map(|part| part / distance);
            let normal_length = (1.0 + rise_x * rise_x + rise_y * rise_y).sqrt();
            // the surface's unit normal (-rise_x, -rise_y, 1) / its length against the unit vector to the lamp
            let facing = (-rise_x * unit[0] - rise_y * unit[1] + unit[2]) / normal_length;
            // the light fades with the distance from the lamp over the relief's top, not from the surface itself: how tall the
            // relief is drawn (`height`) shapes the shadows, never how far the lamp reaches
            let fall = (to_lamp[0] * to_lamp[0] + to_lamp[1] * to_lamp[1] + lamp_height * lamp_height).sqrt();
            let window = (1.0 - (fall / reach) * (fall / reach)).max(0.0);
            let lit = facing.max(0.0) * window * window;
            // the half way vector between the lamp and the eye (straight above the picture)
            let half = [unit[0], unit[1], unit[2] + 1.0];
            let half_length = half.iter().map(|part| part * part).sum::<f32>().sqrt();
            let glint = ((-rise_x * half[0] - rise_y * half[1] + half[2]) / half_length / normal_length)
                .max(0.0)
                .powi(SHININESS)
                * window
                * window;
            let pixel = pixels[index];
            let gloss = params.specular * params.gain.max(0.0) * glint * luminance(pixel);
            let mut lit_pixel = [0.0; 3];
            for channel in 0..3 {
                let linear = srgb_to_linear_fast(pixel[channel]);
                let base = linear * params.ambient;
                let light = params.gain * lit * params.color.0[channel];
                let lamp_result = if light >= 0.0 {
                    // screen: the light is added, and a bright part cannot burn out
                    1.0 - (1.0 - base) * (1.0 - (linear * light).clamp(0.0, 1.0))
                } else {
                    (base * (1.0 + light)).max(0.0)
                };
                lit_pixel[channel] =
                    linear_to_srgb_fast((lamp_result + gloss * params.color.0[channel]).clamp(0.0, 1.0));
            }
            lit_pixel
        },
    );
    Image::from_pixels(height, width, out)
}

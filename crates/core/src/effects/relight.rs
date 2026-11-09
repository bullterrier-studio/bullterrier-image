//! `relight`: a field read as a relief and lit from one side. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter_plane;
use crate::ops::edge::Edge;
use crate::ops::relief::shade_slopes;
use crate::params::{FieldName, Params};
use crate::{Image, Inputs, Picture};

/// Where `relight` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `relight` reads around a pixel: the blur's reach and a pixel on either side for the slope.
pub(crate) fn footprint(params: &Params) -> Footprint {
    // constant: the kernel reaches int(truncate x blur + 0.5) px and the slope one more
    Footprint::reach(f64::from(params.blur * params.truncate) + 1.5, params.edge == Edge::Wrap)
}

/// Light the picture from the shape of a field, read as a relief `height` px tall, from `angle` at `elevation`: bump
/// lighting, emboss, relighting by depth.
///
/// The lighting comes from the shape of `field`, not from the picture's brightness unless that is the field: the height
/// is the field smoothed by `blur`; the surface normal is (-dx, -dy, 1) / its length with the slopes taken per
/// pixel by central differences. The light's strength is the normal's dot product with the light's direction in
/// proportion to what a flat surface gets, so flat areas are unchanged and a slope is brightened or darkened. The lit
/// surface is the picture mixed into mid grey by `saturation`; all channels are multiplied by the one grey gain, so the
/// colour ratios are kept, and the result is clipped to 0..1. A low `elevation` with `saturation` 0 is an emboss: the
/// relief in grey. A sharp step in the field (a subject's outline in depth) lights as a steep cliff and can leave a dark
/// or bright fringe beside it; raise `blur` or lower `height` to soften it.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The field read as the relief, the higher the value the higher the ground: `depth` (the nearest stands highest; the
    /// subject rises from its background), `light` (the picture's own brightness: bright parts stand proud), `subject`, or
    /// a region the recipe declares.
    #[param(default = "depth")]
    pub field: FieldName,
    /// Where the light comes from, in degrees clockwise from pointing right, on screen: 0 = the right, 90 = below, 180 =
    /// the left, 225 = the upper left, 270 = above. Sides that face it are brightened, those that face away darkened.
    #[param(default = 225.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// How high the light stands above the picture's plane, in degrees: 90 = straight above (every slope only dims a
    /// little, whatever `angle`), 45 = the usual raking light, 1 = grazing, so the shapes are dramatic and the lit side is
    /// clipped to white.
    #[param(default = 45.0, unit = degrees, min = 1, max = 90)]
    pub elevation: f32,
    /// How tall the whole range of the field stands as a relief, in px at the recipe's size: a field value of 1 stands this
    /// many px above 0, so the slope between neighbours is their difference times this. 0 = flat (the relief is nothing:
    /// the picture at `saturation` 1, mid grey at 0), larger = steeper shapes that catch more of the light. A field of the
    /// picture's own brightness wants far less (about 8) than a depth (about 200).
    #[param(default = 200.0, unit = px, min = 0, max = 1000)]
    pub height: f32,
    /// Gaussian sigma in px with which the field is smoothed before its slopes are taken: 0 = every ripple of the field is
    /// lit (its noise shows), larger = softer lighting that follows only the broad shapes.
    #[param(default = 2.0, unit = px, min = 0, max = 50)]
    pub blur: f32,
    /// How much of the picture's colour is lit: 1 = the picture itself, lit; 0 = the relief alone, a grey surface lit (a
    /// flat field is mid grey); between, the picture mixed into mid grey.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub saturation: f32,
    /// Where the smoothing Gaussian is cut off, in sigmas (only matters when `blur` > 0). Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the smoothing and the slope see beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated),
    /// 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge, so a
    /// tileable picture lights without a seam).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture multiplied by the grey gain of the field's lighting.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(height, width, picture.pixels().to_vec());
    };
    let edge = params.edge.extension();
    let smooth = gaussian_filter_plane(
        field.values(),
        (height, width),
        f64::from(params.blur),
        f64::from(params.truncate),
        edge,
    );
    let (angle, elevation) = (f64::from(params.angle).to_radians(), f64::from(params.elevation).to_radians());
    #[expect(clippy::cast_possible_truncation, reason = "a direction, 0..1 in each part")]
    let (to_x, to_y, level, lean) = (angle.cos() as f32, angle.sin() as f32, elevation.sin() as f32, elevation.cos() as f32);
    let (saturation, pixels_in) = (params.saturation, picture.pixels());
    let mut pixels = vec![[0.0; 3]; height * width];
    shade_slopes(&smooth, (height, width), params.height, edge, &mut pixels, |index, rise_y, rise_x| {
        // Lambert's law: the unit normal (-rise_x, -rise_y, 1) dotted with the direction to the light, over what a flat
        // surface (sin of the elevation) catches
        let toward = rise_x * to_x + rise_y * to_y;
        let gain = (level - lean * toward) / ((1.0 + rise_x * rise_x + rise_y * rise_y).sqrt() * level);
        // constant: mid grey, the neutral surface the relief is lit on when the picture's colours are left out
        pixels_in[index].map(|value| ((saturation * value + (1.0 - saturation) * 0.5) * gain).clamp(0.0, 1.0))
    });
    Image::from_pixels(height, width, pixels)
}

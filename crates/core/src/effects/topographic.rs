//! `topographic`: contour lines where a field (the picture's brightness, its depth...) crosses a level. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter_plane;
use crate::ops::edge::Edge;
use crate::ops::par;
use crate::ops::ramp;
use crate::params::{Choice, Color, FieldName, Params};
use crate::{Image, Inputs, Picture};

/// Where `topographic` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[],
};

/// What `topographic` reads around a pixel: the field is blurred and its slope read, and the picture's border changes the
/// slope there.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// What lies between the contour lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Fill {
    /// The photo itself, with the lines drawn over it.
    None,
    /// The photo replaced by flat colour, each band coloured from `colors` by its height, like a hypsometric map.
    Bands,
}

/// Draw contour lines where the smoothed `field` crosses `levels` evenly spaced heights.
///
/// The field (`light` is the Rec. 709 luminance) is blurred by `smoothing` px and scaled to h = field x `levels`; the
/// contours are h = 1, 2, ..., `levels` - 1. A pixel's distance to the nearest contour in px is |h - k| divided by the slope
/// of h there (central differences, halved on the outermost rows and columns, which have one neighbour: lines come out
/// thinner there), so a line is `thickness` px wide however steep or shallow the field runs; a pixel at distance d is
/// covered by clip(`thickness` / 2 + 1/2 - d, 0, 1) of the `line` colour. Where the field does not vary there is no
/// contour. With fill 'bands' the photo is first replaced by the colour of its band floor(h) (the last band for 1) taken
/// from `colors`.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// What the contours follow: 'light' (the picture's own brightness, a plain contour map), 'depth' (how near things are,
    /// a `LiDAR` look), 'subject', 'background' or a region of the recipe. Its range 0..1 is cut into `levels` bands.
    #[param(default = "light")]
    pub field: FieldName,
    /// How many bands the field's range 0..1 is cut into; there is a contour line at each of the levels - 1 heights between
    /// them. 2 = a single line at the middle of the range, 8 = a clear contour map, 14 = a busy one, 64 = a dense engraving.
    #[param(default = 8, unit = count, min = 2, max = 64)]
    pub levels: i32,
    /// How wide the lines are, in pixels across the line however steep the field's slope there; tuned for a picture about
    /// 480 px high. Edges are anti-aliased, so fractions of a pixel show as partial strength.
    #[param(default = 1.5, unit = px, min = 0.5, max = 20)]
    pub thickness: f32,
    /// The colour of the contour lines: '#rrggbb', '#rgb' or [r, g, b] 0..1.
    #[param(default = "#ffffff")]
    pub line: Color,
    /// What lies between the lines: 'none' (the photo itself, with the lines drawn over it) or 'bands' (the photo is
    /// replaced by flat colour, each band coloured from `colors` by its height, like a hypsometric map).
    #[param(default = None)]
    pub fill: Fill,
    /// The colours of the bands for fill 'bands', from the lowest band (the field near 0) to the highest, evenly spaced along
    /// the ramp; at least two. Ignored by fill 'none'.
    #[param(default = ["#16324f", "#3f7d6a", "#d9c97a", "#9c6b43", "#f4f1ea"], min_length = 2)]
    pub colors: Vec<Color>,
    /// Sigma in pixels of the Gaussian blur the field gets before the contours are found, so that grain and fine texture do
    /// not break the lines into specks; tuned for a picture about 480 px high. 0 = the field as it is, raising it gives
    /// smoother, simpler contours.
    #[param(default = 5.0, unit = px, min = 0, max = 50)]
    pub smoothing: f32,
    /// Where the smoothing blur is cut off, in sigmas: it reaches truncate x `smoothing` px. Larger is more exact and
    /// slower; below 3 the blur turns visibly boxy.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the smoothing sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture with contour lines drawn over it.
#[expect(clippy::cast_precision_loss, reason = "levels is from 2 to 64")]
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(height, width, picture.pixels().to_vec());
    };
    let smooth = gaussian_filter_plane(
        field.values(),
        (height, width),
        f64::from(params.smoothing),
        f64::from(params.truncate),
        params.edge.extension(),
    );
    let levels = params.levels as f32;
    let top = levels - 1.0; // constant: the highest contour lies one level below the top of the range
    let high = |row: usize, column: usize| smooth[row * width + column] * levels;
    let stops: Vec<[f32; 3]> = params.colors.iter().map(|colour| colour.0).collect();
    let line = params.line.0;
    let reach = params.thickness / 2.0 + 0.5; // constant: a pixel is a unit wide, so a line covers half a pixel beyond its edge
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (offset, pixels) in band.chunks_exact_mut(width).enumerate() {
            let row = first_row + offset;
            let (above, below) = (row.saturating_sub(1), (row + 1).min(height - 1));
            for (column, pixel) in pixels.iter_mut().enumerate() {
                let (before, after) = (column.saturating_sub(1), (column + 1).min(width - 1));
                let here = high(row, column);
                // central differences; a border pixel's missing neighbour is itself, so there the slope is half the
                // one-sided one
                let slope = ((high(below, column) - high(above, column)) / 2.0) // constant: the mean of two steps
                    .hypot((high(row, after) - high(row, before)) / 2.0); // constant: as above
                let nearest = here.round_ties_even().clamp(1.0, top);
                let cover = if slope > 0.0 {
                    (reach - (here - nearest).abs() / slope).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let base = match params.fill {
                    Fill::None => picture.pixels()[row * width + column],
                    Fill::Bands => ramp::at(&stops, here.floor().clamp(0.0, top) / top),
                };
                *pixel = [0, 1, 2].map(|channel| base[channel] + cover * (line[channel] - base[channel]));
            }
        }
    });
    Image::from_pixels(height, width, out)
}

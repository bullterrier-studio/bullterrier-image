//! `pixel_sort`: runs of pixels between two thresholds put in order of brightness or hue. What it does is documented once,
//! on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::color::luminance;
use crate::ops::colorspace::rgb_to_hsv;
use crate::ops::par;
use crate::params::{Choice, Params};
use crate::{Image, Inputs, Picture};

/// Where `pixel_sort` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `pixel_sort` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// How far `pixel_sort` paints beyond the picture's edge: not worked out.
pub(crate) fn spread(_params: &Params) -> crate::Spread {
    crate::Spread::Unbounded
}

/// What the pixels are measured and put in order by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum By {
    /// Perceived brightness.
    Brightness,
    /// The position on the colour circle.
    Hue,
}

/// The direction the pixels are sorted along.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Axis {
    /// Along every row.
    Horizontal,
    /// Along every column.
    Vertical,
}

/// Which way a run is ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Order {
    /// The lowest key first.
    Ascending,
    /// The highest key first.
    Descending,
}

/// A pixel sort: along every row (or column) the pixels whose key lies between two thresholds are put in order.
///
/// A run is a stretch of consecutive pixels in one row (column) that all have a key (`by`) from `low` to `high`; a pixel
/// outside that range ends the run and never moves. Each run keeps exactly the pixels it had, only put in `order` of their
/// key, so every row (column) keeps its pixels and no colour is invented. Equal keys keep their order.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// What the pixels are measured and put in order by: 'brightness' (perceived brightness, Rec. 709 weights; 0 = black,
    /// 1 = white) or 'hue' (the position on the colour circle, 0 = red, 1/3 = green, 2/3 = blue; a grey has hue 0).
    /// Brightness gives smooth light-to-dark smears, hue rainbow streaks.
    #[param(default = Brightness)]
    pub by: By,
    /// The lowest key, 0 to 1, a pixel may have and still be sorted: pixels below it stay where they are and break a run in
    /// two. Raising it leaves more of the dark (or low-hue) pixels alone. If it is above `high`, nothing is sorted.
    #[param(default = 0.25, unit = fraction, min = 0, max = 1)]
    pub low: f32,
    /// The highest key, 0 to 1, a pixel may have and still be sorted: pixels above it stay where they are and break a run in
    /// two. Lowering it leaves more of the bright (or high-hue) pixels alone, which protects the highlights. If it is below
    /// `low`, nothing is sorted.
    #[param(default = 0.75, unit = fraction, min = 0, max = 1)]
    pub high: f32,
    /// The direction the pixels are sorted along: 'horizontal' sorts along every row (the streaks run across the picture),
    /// 'vertical' along every column (the streaks run down it).
    #[param(default = Horizontal)]
    pub axis: Axis,
    /// Which way a run is ordered: 'ascending' puts the lowest key first (the left or top end of the run), 'descending' the
    /// highest key first.
    #[param(default = Ascending)]
    pub order: Order,
}

/// A pixel and the key it is sorted by.
type Keyed = (f32, [f32; 3]);

/// The `length` pixels of one row or column, `(key, pixel)` pairs read by `at`, with every run of keys within `low..=high`
/// put in order, written back by `put`. `scratch` holds a run while it is gathered.
fn sort_line(
    length: usize,
    params: &Params,
    scratch: &mut Vec<Keyed>,
    at: impl Fn(usize) -> Keyed,
    mut put: impl FnMut(usize, [f32; 3]),
) {
    // the run in `scratch` ends at `end`: put in order and written back where it was read from
    let mut flush = |scratch: &mut Vec<Keyed>, end: usize| {
        // a stable sort, so equal keys keep their order
        match params.order {
            Order::Ascending => scratch.sort_by(|a, b| a.0.total_cmp(&b.0)),
            Order::Descending => scratch.sort_by(|a, b| b.0.total_cmp(&a.0)),
        }
        for (offset, &(_, pixel)) in scratch.iter().enumerate() {
            put(end - scratch.len() + offset, pixel);
        }
        scratch.clear();
    };
    for index in 0..length {
        let keyed = at(index);
        if keyed.0 >= params.low && keyed.0 <= params.high {
            scratch.push(keyed);
        } else {
            flush(scratch, index);
        }
    }
    flush(scratch, length);
}

/// The picture with its runs sorted.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let pixels = picture.pixels();
    let key = |pixel: [f32; 3]| match params.by {
        By::Brightness => luminance(pixel),
        By::Hue => rgb_to_hsv(pixel)[0],
    };
    let mut out = pixels.to_vec();
    match params.axis {
        Axis::Horizontal => par::rows(&mut out, width, |first_row, band| {
            let mut scratch = Vec::new();
            for (offset, row) in band.chunks_exact_mut(width).enumerate() {
                let source = &pixels[(first_row + offset) * width..][..width];
                sort_line(
                    width,
                    params,
                    &mut scratch,
                    |column| (key(source[column]), source[column]),
                    |column, pixel| row[column] = pixel,
                );
            }
        }),
        Axis::Vertical => {
            let mut scratch = Vec::new();
            for column in 0..width {
                sort_line(
                    height,
                    params,
                    &mut scratch,
                    |row| {
                        let pixel = pixels[row * width + column];
                        (key(pixel), pixel)
                    },
                    |row, pixel| out[row * width + column] = pixel,
                );
            }
        }
    }
    Image::from_pixels(height, width, out)
}

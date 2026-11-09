//! `jpeg_crush`: the picture put through JPEG's lossy steps at a low quality, optionally damaged. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::jpeg::{Coefficients, Plane, decode, encode};
use crate::ops::length::{float, whole};
use crate::ops::par;
use crate::ops::random::Rng;
use crate::ops::resample::Resize;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `jpeg_crush` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Glitch,
    opacity: 1.0,
    inputs: &[Input::SizeRatio],
};

/// What `jpeg_crush` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Blocky JPEG artefacts, or a corrupted file: compress the picture with a JPEG codec written as maths at `quality`, damage
/// `hits` coefficients, and decode it.
///
/// The steps are the standard's (ITU-T T.81 with JFIF): sRGB to YCbCr, the two colour planes halved in each direction
/// (4:2:0), 8 x 8 blocks through the DCT, every coefficient rounded to a multiple of its quantisation step, then all of it
/// undone. There is no file and no byte: the result does not depend on any image library, so it is alike on every platform.
/// A picture whose sides are not multiples of 16 is extended by repeating its last row and column and cropped back.
///
/// Each hit picks a block at random from all those of the brightness and the two colour planes, and a coefficient in it (the
/// DC value with chance `dc_share`, otherwise one of the 63 others), and adds or subtracts between 1 and `magnitude`
/// quantisation steps before the decoding. A hit on a DC value also moves the DC value of every later block of its row in
/// that plane, as a damaged difference does in the file.
///
/// The blocks are 8 design px, so the damage looks the same at any size while the fine detail stays. The picture is shrunk
/// to its design size by averaging, crushed and damaged there, and the damage (crushed minus shrunk) is added back to the
/// full picture, each design pixel's damage repeated over the working pixels it covers so block edges stay hard.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The JPEG quality, 1 to 100, on the scale JPEG tools use (the Independent JPEG Group's): 100 keeps almost everything,
    /// about 75 is a normal photo, about 6 shows blocks 8 px wide, ringing round edges and colour bleed, 1 is a few flat
    /// blocks. Lowering it coarsens the quantisation of every 8 x 8 block, so each hit is also a larger jump in the picture.
    #[param(default = 6, unit = count, min = 1, max = 100)]
    pub quality: i32,
    /// How many coefficients are damaged after the compression, 0 to 5000, each at a random place in a random 8 x 8 block of
    /// the brightness or one of the two colour planes, as a corrupted file shows. 0 leaves the plain crushed picture, and
    /// then `seed`, `dc_share` and `magnitude` do nothing. Raising it spreads more damage over the picture.
    #[param(default = 0, unit = count, min = 0, max = 5000)]
    pub hits: i32,
    /// Random seed for where the hits land and how large they are: the same seed always gives the same damage, any other a
    /// different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
    /// The share of hits, 0 to 1, that land on a block's DC value (its average) rather than on one of its 63 detail
    /// coefficients. A DC hit shifts the block and, as JPEG stores each average as a difference from the block before,
    /// every later block of its row: a streak to the right edge. A detail hit only marks its own block with a pattern.
    #[param(default = 0.3, unit = fraction, min = 0, max = 1, advanced)]
    pub dc_share: f32,
    /// The largest change a hit makes to a coefficient, 1 to 100 quantisation steps: each hit adds or subtracts a whole
    /// number of steps between 1 and this. Raising it makes the damage louder.
    #[param(default = 8, unit = count, min = 1, max = 100, advanced)]
    pub magnitude: i32,
}

/// `coded` with `params.hits` coefficients damaged, at places and by amounts drawn from `params.seed`.
fn damage(coded: &mut Coefficients, params: &Params) {
    let mut rng = Rng::seeded(params.seed);
    let planes = [Plane::Luma, Plane::Blue, Plane::Red];
    let grids = planes.map(|plane| coded.blocks(plane));
    let total: usize = grids.iter().map(|(rows, columns)| rows * columns).sum();
    for _ in 0..params.hits.unsigned_abs() {
        // the pick numbers the blocks of the planes one after the other
        let mut pick = rng.below(total);
        let on_dc = rng.uniform() < f64::from(params.dc_share);
        let coefficient = 1 + rng.below(63); // constant: the 63 coefficients of a block other than its DC value
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "a change is at most `magnitude`, 100"
        )]
        let steps = (1 + rng.below(params.magnitude.unsigned_abs() as usize)) as i32;
        let change = steps * if rng.uniform() < 0.5 { -1 } else { 1 }; // constant: a hit is as likely down as up
        let mut which = 0;
        while pick >= grids[which].0 * grids[which].1 {
            pick -= grids[which].0 * grids[which].1;
            which += 1;
        }
        let (plane, columns) = (planes[which], grids[which].1);
        let (row, column) = (pick / columns, pick % columns);
        if on_dc {
            // the later blocks' values are differences from this one
            for later in column..columns {
                *coded.at(plane, row, later, 0) += change;
            }
        } else {
            *coded.at(plane, row, column, coefficient) += change;
        }
    }
}

/// The JPEG round trip of the `height` x `width` picture `pixels` at `params.quality`, with `params.hits` coefficients
/// damaged.
fn crush(pixels: &[[f32; 3]], height: usize, width: usize, params: &Params) -> Vec<[f32; 3]> {
    let mut coded = encode(pixels, height, width, params.quality.unsigned_abs());
    damage(&mut coded, params);
    decode(&coded)
}

/// The picture, crushed.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let ratio = inputs.size_ratio();
    let design = (
        whole(float(height) / ratio).max(1),
        whole(float(width) / ratio).max(1),
    );
    if design == (height, width) {
        return Image::from_pixels(height, width, crush(picture.pixels(), height, width, params));
    }
    let small = Resize::averaging(height, width, design).apply(picture.pixels());
    let crushed = crush(&small, design.0, design.1, params);
    let pixels = picture.pixels();
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (offset, row) in band.chunks_exact_mut(width).enumerate() {
            let line = first_row + offset;
            let near_row = line * design.0 / height;
            for (column, pixel) in row.iter_mut().enumerate() {
                // the design pixel this one falls in: its damage is added to the picture
                let near = near_row * design.1 + column * design.1 / width;
                let original = pixels[line * width + column];
                *pixel = std::array::from_fn(|channel| {
                    (crushed[near][channel] + (original[channel] - small[near][channel])).clamp(0.0, 1.0)
                });
            }
        }
    });
    Image::from_pixels(height, width, out)
}

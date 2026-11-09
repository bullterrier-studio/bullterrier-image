//! `dither`: the picture redrawn in a few colours, its tones kept as a dot pattern. What it does is documented once, on
//! `Params`.

use std::borrow::Cow;

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::cluster::{Point, Start, kmeans, mean, nearest};
use crate::ops::colorspace::rgb_to_oklab;
use crate::ops::dither::{ATKINSON, FLOYD_STEINBERG, Tile, diffuse, ordered};
use crate::ops::inks::{Inks, MOST_INKS, shift};
use crate::ops::length::whole;
use crate::ops::par;
use crate::ops::random::{Rng, mix};
use crate::params::{Choice, Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `dither` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[],
};

/// What `dither` reads around a pixel: error diffusion carries errors across the picture, and the colours of the `photo`
/// palette are searched in all of it.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// How tones become a pattern of dots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Method {
    /// Passes each pixel's error to its neighbours (Floyd and Steinberg, 1976); keeps every tone.
    FloydSteinberg,
    /// Passes three quarters of the error on (Atkinson), so highlights and shadows go clean.
    Atkinson,
    /// A regular crosshatch of an 8 x 8 matrix.
    Bayer,
    /// An even, grainless scatter from a repeating 64 x 64 tile.
    BlueNoise,
}

/// Where the colours come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Palette {
    /// Colours found in the picture itself (k-means).
    Photo,
    /// The 4 greens widely used to draw a Game Boy screen.
    Gameboy,
    /// IBM CGA graphics mode 4, palette 1, high intensity: black, cyan, magenta, white.
    Cga,
    /// The 16 default colours of the IBM EGA.
    Ega,
    /// The 16 colours of the PICO-8 fantasy console.
    Pico8,
    /// Every mix of 2 evenly spaced levels of red, green and blue: 8 colours.
    Cube2,
    /// Every mix of 3 levels: 27 colours.
    Cube3,
    /// Every mix of 4 levels: 64 colours.
    Cube4,
    /// Every mix of 5 levels: 125 colours.
    Cube5,
    /// Every mix of 6 levels: 216 colours.
    Cube6,
    /// The colours of `custom`.
    Custom,
}

/// How the colours are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Separation {
    /// From `palette`.
    None,
    /// One layer of dots per ink in `inks`, printed over `paper`.
    Inks,
}

/// The colour space in which dots are matched to palette colours, the `photo` colours are searched, and errors are measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Space {
    /// Where equal distances look equally different.
    Oklab,
    /// The picture's own red, green and blue values.
    Rgb,
}

/// How the search for colours starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Init {
    /// Spreads the starting colours out (k-means++, Arthur and Vassilvitskii, 2007).
    Spread,
    /// Starts from random pixels.
    Random,
}

/// Redraw the picture in a few colours, its tones kept as dots of `cell` size: ordered or error-diffusion dither, 1-bit
/// black and white, retro palettes, and the presets `posterize`, `threshold` and `palette_quantize`.
///
/// The picture is averaged onto a grid of `cell`-sized squares, each of which becomes one palette colour, so seen from afar
/// the dots add up to the original tones; the result is shown with hard square dots. `palette` picks the colours (found in
/// the photo, a named set or your own), `method` the pattern and `pattern` how much of it is used: 0 gives plain flat
/// colours. Colours are matched, and errors measured, in `space`. `separation` 'inks' prints the `inks` instead (one to four
/// colours): each ink is a layer of dots that cover the picture as much as the ink is needed there (the photo's colours
/// split into amounts of the inks, as in `halftone`), each layer dithered with `method` and `cell`, and the layers
/// printed subtractively over `paper` (inks overlap like transparent layers, so use a light `paper`). `palette`,
/// `custom`, `colors`, `space`, `init`, `iterations`, `tolerance`, `sample` and `seed` are used only with no
/// `separation`. `offset` px moves the second ink's layer that far from the first along `offset_angle`, as ink
/// misregistered on a press.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How tones become a pattern of dots. `floyd_steinberg` and 'atkinson' pass each pixel's error on to its neighbours
    /// (organic grain; Floyd-Steinberg keeps every tone, Atkinson drops a quarter of the error, so highlights and shadows go
    /// clean and the contrast is higher). 'bayer' is a regular crosshatch of an 8 x 8 matrix. `blue_noise` is an even,
    /// grainless scatter from a repeating 64 x 64 tile, the one with no visible pattern. Both of those mix colours of any
    /// palette, in steps of a sixteenth.
    #[param(default = FloydSteinberg)]
    pub method: Method,
    /// Where the colours come from. 'photo' finds `colors` colours in the picture itself (k-means); 'gameboy' (the 4 greens
    /// widely used to draw a Game Boy screen), 'cga' (black, cyan, magenta, white), 'ega' (the 16 IBM EGA colours), 'pico8'
    /// (the 16 PICO-8 colours) and 'cube2', 'cube3', 'cube4', 'cube5' and 'cube6' (every mix of 2 to 6 evenly spaced levels
    /// of red, green and blue: 8 to 216 colours) are fixed palettes; 'custom' takes your own list, `custom`.
    #[param(default = Photo)]
    pub palette: Palette,
    /// Your own colours, in any order ('#rrggbb', '#rgb' or three numbers 0 to 1), at least two, used when `palette` is
    /// 'custom' (ignored for any other palette).
    #[param(default = ["#000000", "#ffffff"], min_length = 2, max_length = 256)]
    pub custom: Vec<Color>,
    /// How many colours to find when `palette` is 'photo' (ignored for any other palette). Raising it keeps more detail and
    /// gradation; fewer than this many are used if the picture has fewer distinct colours. 'bayer' and `blue_noise` take
    /// longer with more colours.
    #[param(default = 8, unit = count, min = 2, max = 256)]
    pub colors: i32,
    /// The size of one dot in px at the recipe's size (rounded to whole pixels): the picture is dithered on a grid of this
    /// size and shown with hard square dots. 0, the default, is one dot per pixel at any render size (so a `pattern` of 0
    /// recolours pixel by pixel); 4 is a chunky dot on a 480 px picture and 10 or more a pixel-art look. Raising it gives
    /// coarser, larger dots.
    #[param(default = 0.0, unit = px, min = 0, max = 48)]
    pub cell: f32,
    /// How much of the dot pattern is used: 1 = the method as designed; lower passes less error on (`floyd_steinberg`,
    /// atkinson) or mixes fewer colours per dot (bayer, `blue_noise`), so flatter areas; 0 gives each dot its nearest palette
    /// colour, plain flat colours with no pattern, whatever the method.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub pattern: f32,
    /// The colour space in which dots are matched to palette colours, the 'photo' colours are searched, and errors are
    /// measured. 'oklab' is where equal distances look equally different, so colours match as the eye sees them; 'rgb' is
    /// the picture's own red, green and blue values, where each channel is matched on its own: with a 'cube' palette and
    /// `pattern` 0 it snaps every channel to a few levels, hard colour separations.
    #[param(default = Oklab, advanced)]
    pub space: Space,
    /// How the search for colours starts (palette 'photo'): 'spread' spreads the starting colours out (k-means++, Arthur and
    /// Vassilvitskii, 2007) and usually finds a better set; 'random' starts from `colors` random pixels.
    #[param(default = Spread, advanced)]
    pub init: Init,
    /// The most k-means rounds to run when `palette` is 'photo'. Each round moves every colour to the average of the pixels
    /// nearest to it; raising it lets the search settle further.
    #[param(default = 20, unit = count, min = 1, max = 100, advanced)]
    pub iterations: i32,
    /// Stop the k-means rounds early once no colour has moved farther than this in `space` (in Oklab black to white is 1). 0
    /// runs all `iterations`; raising it stops sooner with a rougher result.
    #[param(default = 0.001, unit = fraction, min = 0, max = 1, advanced)]
    pub tolerance: f32,
    /// How many randomly chosen dots the colours are searched on when `palette` is 'photo'; every dot is still repainted. A
    /// picture with fewer dots is searched whole. Raising it costs time and can find rare colours; lowering it is faster but
    /// may miss small areas of colour.
    #[param(default = 10000, unit = count, min = 1, max = 1000000, advanced)]
    pub sample: i32,
    /// Random seed for the choice of sample dots and starting colours when `palette` is 'photo': the same seed always gives
    /// the same picture, another seed may find slightly different colours.
    #[param(default = 0, unit = count, min = 0, max = 4294967295, advanced)]
    pub seed: u32,
    /// How the colours are chosen: 'none' (from `palette`) or 'inks' (one layer of dots per colour of `inks`, printed over
    /// `paper` like transparent layers).
    #[param(default = None)]
    pub separation: Separation,
    /// With 'inks' separation: the one to four ink colours ('#rrggbb', '#rgb' or three numbers 0..1), in the order they are
    /// offset; ignored otherwise. Each pixel's colour is split into amounts of these inks.
    #[param(default = ["#ff4d6d", "#2b59ff"], min_length = 1, max_length = 4)]
    pub inks: Vec<Color>,
    /// With 'inks' separation: the paper colour the inks are printed over, where no ink lands. Use a light one: inks only
    /// darken it.
    #[param(default = "#ffffff")]
    pub paper: Color,
    /// With 'inks' separation: how far in px the second ink's layer is moved from the first (the third twice as far, the
    /// fourth three times), as ink misregistered on a press; 0 = all layers in register.
    #[param(default = 0.0, unit = px, min = 0, max = 100)]
    pub offset: f32,
    /// With an `offset`: the direction of the move, in degrees clockwise on screen from the x axis (90 is downward).
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub offset_angle: f32,
}

/// The name of the stream that picks the sample dots; the k-means starts from the stream of the seed itself.
const SAMPLING: u64 = 1; // constant: names the stream of the sample, any number but the seed's own

/// The colours of the fixed palettes as 0xrrggbb, as their authors publish them (sRGB):
/// - gameboy: the four greens widely used to draw a Game Boy screen, darkest to lightest (a community palette; the real DMG
///   panel's colours differ and depend on the display).
/// - cga: IBM CGA graphics mode 4, palette 1, high intensity: black, cyan, magenta, white.
/// - ega: the 16 default colours of the IBM EGA (and VGA text mode).
/// - pico8: the 16 colours of the PICO-8 fantasy console.
// constant: the published colours of these palettes
const GAMEBOY: [u32; 4] = [0x0f_380f, 0x30_6230, 0x8b_ac0f, 0x9b_bc0f];
// constant: the published colours of these palettes
const CGA: [u32; 4] = [0x00_0000, 0x55_ffff, 0xff_55ff, 0xff_ffff];
// constant: the published colours of these palettes
const EGA: [u32; 16] = [
    0x00_0000, 0x00_00aa, 0x00_aa00, 0x00_aaaa, 0xaa_0000, 0xaa_00aa, 0xaa_5500, 0xaa_aaaa, 0x55_5555, 0x55_55ff,
    0x55_ff55, 0x55_ffff, 0xff_5555, 0xff_55ff, 0xff_ff55, 0xff_ffff,
];
// constant: the published colours of these palettes
const PICO8: [u32; 16] = [
    0x00_0000, 0x1d_2b53, 0x7e_2553, 0x00_8751, 0xab_5236, 0x5f_574f, 0xc2_c3c7, 0xff_f1e8, 0xff_004d, 0xff_a300,
    0xff_ec27, 0x00_e436, 0x29_adff, 0x83_769c, 0xff_77a8, 0xff_ccaa,
];

/// The colours of 0xrrggbb codes, as 0..1 sRGB.
fn hex(codes: &[u32]) -> Vec<Point> {
    codes
        .iter()
        .map(|code| {
            // constant: the bytes of 0xrrggbb are its red, green and blue, each of 255 steps
            [code >> 16, code >> 8, *code].map(|byte| f32::from(u8::try_from(byte & 255).unwrap_or(u8::MAX)) / 255.0)
        })
        .collect()
}

/// Every combination of `levels` evenly spaced levels (0, 1/(levels - 1) .. 1) per channel, red varying slowest.
fn cube(levels: u8) -> Vec<Point> {
    let level = |step: u8| f32::from(step) / f32::from(levels - 1);
    (0..levels)
        .flat_map(|red| (0..levels).flat_map(move |green| (0..levels).map(move |blue| [red, green, blue])))
        .map(|steps| steps.map(level))
        .collect()
}

/// A count a parameter's range keeps positive.
fn count(value: i32) -> usize {
    usize::try_from(value).unwrap_or_default()
}

/// `colours` in `space`: Oklab, or sRGB as it is.
fn in_space(colours: &[Point], space: Space) -> Cow<'_, [Point]> {
    match space {
        Space::Rgb => Cow::Borrowed(colours),
        Space::Oklab => {
            let mut matched = vec![[0.0; 3]; colours.len()];
            par::rows(&mut matched, 1, |first, band| {
                for (out, colour) in band.iter_mut().zip(&colours[first..]) {
                    *out = rgb_to_oklab(*colour);
                }
            });
            Cow::Owned(matched)
        }
    }
}

/// The picture averaged onto squares `cell` px on a side (the last ones in a row or column are smaller): `rows` x `columns`
/// dots, row after row.
fn averaged(pixels: &[Point], (height, width): (usize, usize), cell: usize) -> Cow<'_, [Point]> {
    if cell == 1 {
        return Cow::Borrowed(pixels);
    }
    let columns = width.div_ceil(cell);
    let mut dots = vec![[0.0_f32; 3]; height.div_ceil(cell) * columns];
    par::rows(&mut dots, columns, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(columns).enumerate() {
            let (top, bottom) = (
                (first_row + offset) * cell,
                ((first_row + offset + 1) * cell).min(height),
            );
            for row in pixels[top * width..bottom * width].chunks_exact(width) {
                for (dot, run) in line.iter_mut().zip(row.chunks(cell)) {
                    for pixel in run {
                        for (sum, value) in dot.iter_mut().zip(pixel) {
                            *sum += value;
                        }
                    }
                }
            }
            for (column, dot) in line.iter_mut().enumerate() {
                let across = ((column + 1) * cell).min(width) - column * cell;
                let area = crate::ops::length::float((bottom - top) * across);
                #[expect(clippy::cast_possible_truncation, reason = "an area of at most 2304 px")]
                let area = area as f32;
                *dot = dot.map(|sum| sum / area);
            }
        }
    });
    Cow::Owned(dots)
}

/// The palette as sRGB: the list given, a fixed palette, or `colors` found among the dots, each the average of the dots
/// nearest to its k-means centre (the dots are then painted with the palette colour nearest to them, which can differ for a
/// few dots at the border between two colours). `matched` is `dots` in `params.space`.
fn colours(dots: &[Point], matched: &[Point], params: &Params) -> Vec<Point> {
    match params.palette {
        Palette::Custom => params.custom.iter().map(|colour| colour.0).collect(),
        Palette::Gameboy => hex(&GAMEBOY),
        Palette::Cga => hex(&CGA),
        Palette::Ega => hex(&EGA),
        Palette::Pico8 => hex(&PICO8),
        Palette::Cube2 => cube(2), // constant: the cubes offered run from 2 levels to 6
        Palette::Cube3 => cube(3), // constant: the cubes offered run from 2 levels to 6
        Palette::Cube4 => cube(4), // constant: the cubes offered run from 2 levels to 6
        Palette::Cube5 => cube(5), // constant: the cubes offered run from 2 levels to 6
        Palette::Cube6 => cube(6), // constant: the cubes offered run from 2 levels to 6
        Palette::Photo => found(dots, matched, params),
    }
}

/// The `photo` palette: see [`colours`].
fn found(dots: &[Point], matched: &[Point], params: &Params) -> Vec<Point> {
    let sample = count(params.sample);
    let searched: Cow<'_, [Point]> = if sample < matched.len() {
        let stream = (mix(params.seed, &[SAMPLING]) >> 32) as u32; // constant: the hash's upper half
        let chosen = Rng::seeded(stream).sample(matched.len(), sample);
        Cow::Owned(chosen.into_iter().map(|at| matched[at]).collect())
    } else {
        Cow::Borrowed(matched)
    };
    let start = match params.init {
        Init::Spread => Start::Spread,
        Init::Random => Start::Random,
    };
    let centres = kmeans(
        &searched,
        count(params.colors),
        start,
        count(params.iterations),
        params.tolerance,
        params.seed,
    );
    let mut sums = vec![[0.0_f64; 3]; centres.len()];
    let mut members = vec![0_usize; centres.len()];
    for (dot, label) in dots.iter().zip(nearest(matched, &centres)) {
        for (total, &value) in sums[usize::from(label)].iter_mut().zip(dot) {
            *total += f64::from(value);
        }
        members[usize::from(label)] += 1;
    }
    // a centre nobody is nearest to has no colour to give
    sums.iter()
        .zip(&members)
        .filter(|(_, count)| **count > 0)
        .map(|(sum, &count)| mean(sum, count))
        .collect()
}

/// The `chosen` method on `values` (`size` dots), as a pattern of palette indices.
fn pattern(method: Method, values: &[Point], size: (usize, usize), palette: &[Point], amount: f32) -> Vec<u8> {
    match method {
        Method::FloydSteinberg => diffuse(values, size, palette, &FLOYD_STEINBERG, amount),
        Method::Atkinson => diffuse(values, size, palette, &ATKINSON, amount),
        Method::Bayer => ordered(values, size, palette, Tile::Bayer, amount),
        Method::BlueNoise => ordered(values, size, palette, Tile::BlueNoise, amount),
    }
}

/// The inks printed: each ink's coverage of the dots is dithered to a layer of 0 and 1 (the layers in the order of the
/// inks), moved by its share of the offset, and every pixel gets the print of the inks that land on it.
fn printed(
    dots: &[Point],
    (rows, columns): (usize, usize),
    cell: usize,
    (height, width): (usize, usize),
    params: &Params,
) -> Image {
    let colours: Vec<[f32; 3]> = params.inks.iter().map(|ink| ink.0).collect();
    let inks = Inks::new(params.paper.0, &colours);
    let count = colours.len().min(MOST_INKS);
    let mut needed = vec![[0.0; MOST_INKS]; dots.len()];
    par::rows(&mut needed, 1, |first, band| {
        for (out, dot) in band.iter_mut().zip(&dots[first..]) {
            *out = inks.coverages(*dot);
        }
    });
    // an ink is the first channel of a dot; its two choices are no ink and full ink
    let (none, full) = ([0.0; 3], [1.0, 0.0, 0.0]);
    let mut plane = vec![none; dots.len()];
    let layers: Vec<Vec<u8>> = (0..count)
        .map(|ink| {
            for (value, coverage) in plane.iter_mut().zip(&needed) {
                value[0] = coverage[ink];
            }
            pattern(params.method, &plane, (rows, columns), &[none, full], params.pattern)
        })
        .collect();
    // the print of every combination of inks, by the bits of the inks that land
    let table: Vec<[f32; 3]> = (0..1_usize << count)
        .map(|mask| inks.print(&std::array::from_fn(|ink| if mask >> ink & 1 == 1 { 1.0 } else { 0.0 })))
        .collect();
    // whole pixels: the nearest to the move
    let whole_move = |away: f32| {
        let pixels = whole(f64::from(away.abs())).cast_signed();
        if away < 0.0 { -pixels } else { pixels }
    };
    let moves: Vec<[isize; 2]> = (0..count)
        .map(|ink| shift(params.offset, params.offset_angle, ink).map(whole_move))
        .collect();
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let y = first_row + offset;
            for (x, pixel) in line.iter_mut().enumerate() {
                let mut mask = 0; // constant: no ink has landed yet
                for (ink, (layer, by)) in layers.iter().zip(&moves).enumerate() {
                    // where the layer comes from; none where that is off the picture
                    let from =
                        |at: usize, away: isize, size: usize| at.checked_add_signed(-away).filter(|&at| at < size);
                    if let (Some(row), Some(column)) = (from(y, by[1], height), from(x, by[0], width)) {
                        mask |= usize::from(layer[row / cell * columns + column / cell]) << ink;
                    }
                }
                *pixel = table[mask];
            }
        }
    });
    Image::from_pixels(height, width, out)
}

/// The picture redrawn.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let cell = whole(f64::from(params.cell)).max(1);
    let (rows, columns) = (height.div_ceil(cell), width.div_ceil(cell));
    let dots = averaged(picture.pixels(), (height, width), cell);
    if params.separation == Separation::Inks {
        return printed(&dots, (rows, columns), cell, (height, width), params);
    }
    let matched = in_space(&dots, params.space);
    let palette = colours(&dots, &matched, params);
    let matched_palette = in_space(&palette, params.space);
    let size = (rows, columns);
    let chosen = pattern(params.method, &matched, size, &matched_palette, params.pattern);
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = (first_row + offset) / cell;
            for (column, run) in line.chunks_mut(cell).enumerate() {
                run.fill(palette[usize::from(chosen[row * columns + column])]);
            }
        }
    });
    Image::from_pixels(height, width, out)
}

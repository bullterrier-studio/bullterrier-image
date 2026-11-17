//! `dither`: the picture redrawn in a few colours, its tones kept as a dot pattern. What it does is documented once, on
//! `Params`.

use std::borrow::Cow;

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::attributes::{Blocks, Choices, Shape};
use crate::ops::cluster::{Point, Start};
use crate::ops::colorspace::rgb_to_oklab;
use crate::ops::dither::{
    ATKINSON, BURKES, FLOYD_STEINBERG, JARVIS_JUDICE_NINKE, OSTROMOUKHOV, SIERRA, SIERRA_LITE, SIERRA_TWO_ROW,
    STEVENSON_ARCE, STUCKI, Thresholds, Tile, check_matrix, diffuse, dot_diffusion, from_greys, matrix_greys, noise,
    ordered, riemersma, shared,
};
use crate::ops::inks::{Inks, MOST_INKS, shift};
use crate::ops::length::whole;
use crate::ops::par;
use crate::ops::realism::Realism;
use crate::ops::tiles::{self, Strip};
use crate::palette::{self, Search};
use crate::params::{Choice, Color, Params, PictureName};
use crate::{Data, Error, Image, Inputs, Labels, Layer, Picture};

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

/// How far `dither` paints beyond the picture's edge: not worked out.
pub(crate) fn spread(_params: &Params) -> crate::Spread {
    crate::Spread::Unbounded
}

/// How tones become a pattern of dots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Method {
    /// Passes each pixel's error to its neighbours (Floyd and Steinberg, 1976); keeps every tone.
    FloydSteinberg,
    /// Passes three quarters of the error on (Atkinson), so highlights and shadows go clean.
    Atkinson,
    /// Spreads the error over twelve pixels in three rows (Jarvis, Judice and Ninke, 1976): smoother, less crawling grain.
    JarvisJudiceNinke,
    /// Jarvis-Judice-Ninke's twelve pixels with weights that favour the near ones (Stucki, 1981): sharper edges.
    Stucki,
    /// Stucki without its third row (Burkes, 1988): twice as fast, nearly as smooth.
    Burkes,
    /// Spreads the error over ten pixels in three rows (Sierra, 1989): close to Stucki, softer.
    Sierra,
    /// Sierra's first two rows, seven pixels (Sierra, 1989): faster, a little grainier.
    SierraTwoRow,
    /// Three pixels, the lightest kernel (Sierra Lite, 1989): near Floyd-Steinberg in grain.
    SierraLite,
    /// A wide hexagonal kernel on twelve pixels (Stevenson and Arce, 1985): a fine, even grain.
    StevensonArce,
    /// Weights that change with the pixel's level (Ostromoukhov, 2001), tuned for an even grain at every grey; the level is
    /// taken from the pixel's lightness (Oklab) or Rec. 709 luma (rgb); the paper scans serpentine (see `serpentine`).
    Ostromoukhov,
    /// Knuth's dot diffusion (1987): pixels taken in the order of an 8 x 8 class matrix, each passing its error to its
    /// neighbours of a higher class; no worms, as a tile's pixels of one class do not depend on each other.
    DotDiffusion,
    /// Riemersma's dither (1998): the pixels taken along a Hilbert curve, each shifted by the weighted errors of the last
    /// 16; a grain with no row direction.
    Riemersma,
    /// A regular crosshatch of Bayer's 2 x 2 matrix: coarse, four levels.
    Bayer2,
    /// A regular crosshatch of Bayer's 4 x 4 matrix.
    Bayer4,
    /// A regular crosshatch of Bayer's 8 x 8 matrix.
    Bayer,
    /// Round dots in a square grid, as a halftone screen (an 8 x 8 clustered-dot matrix): dots grow with the tone.
    Clustered,
    /// Round dots in a grid turned 45 degrees, as a newspaper screen (an 8 x 8 clustered-dot matrix).
    Clustered45,
    /// An even, grainless scatter from a repeating 64 x 64 tile.
    BlueNoise,
    /// A random threshold for every pixel (`seed`): grain with no structure, noisier than blue noise.
    WhiteNoise,
    /// The Game Boy Camera's 4 x 4 dither matrix, a fine crosshatch as the camera draws it.
    GameboyCamera,
    /// A pattern of your own, the picture `matrix`: its grey values, ranked, are the thresholds.
    Matrix,
    /// Patterned tiles of your own, the picture `tiles`: each dot is one tile, chosen by its tone, drawn between the two
    /// palette colours that bracket the dot.
    Tiles,
}

include!(concat!(env!("OUT_DIR"), "/palettes.rs"));

/// How the colours are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Separation {
    /// From `palette`.
    None,
    /// The `inks` printed over `paper`, dithered once against their overprints.
    Spot,
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
/// colours. Colours are matched, and errors measured, in `space`. `separation` 'spot' prints the `inks` instead (one to four
/// colours): the photo's colours are split into amounts of the inks (as in `halftone`), mixed
/// back as printed subtractively over `paper` (inks overlap like transparent layers and only darken it, so use a light `paper`), and dithered
/// once with `method` and `cell` against the paper, each ink and every overlap. `palette`,
/// `custom`, `colors`, `space`, `init`, `iterations`, `tolerance` and `sample` are used only with no
/// `separation`; `seed` also seeds `white_noise`. `offset` px moves the second ink's layer that far from the first along `offset_angle`, as ink
/// misregistered on a press.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How tones become a pattern of dots. `floyd_steinberg`, 'atkinson', `jarvis_judice_ninke`, 'stucki', 'burkes',
    /// 'sierra', `sierra_two_row`, `sierra_lite`, `stevenson_arce` and `ostromoukhov` pass each pixel's error on to its neighbours
    /// (organic grain; they differ in how many pixels and how far the error goes: wider kernels are smoother, narrower ones
    /// sharper and faster; all keep every tone except Atkinson, which drops a quarter of the error, so highlights and
    /// shadows go clean and the contrast is higher; with several `inks` too, as with one, a quarter of the error is dropped, so
    /// highlights and shadows clip and an ink mix prints off-tone: two inks at one half each print green 0.366 where 0.439 is the mix). `dot_diffusion` (Knuth's 8 x 8 class matrix sets the order, pixels of one class independent) and `riemersma` (a Hilbert curve and a memory of the last 16 errors) pass error on in an order of their own. The rest are ordered: a repeating matrix of thresholds decides each
    /// pixel. `bayer2`, `bayer4` and 'bayer' are regular crosshatches of Bayer's 2 x 2, 4 x 4 and 8 x 8 matrices;
    /// 'clustered' and `clustered45` are halftone screens (8 x 8, dots in a square grid or turned 45 degrees);
    /// `blue_noise` is an even, grainless scatter from a repeating 64 x 64 tile, the one with no visible pattern;
    /// `white_noise` is a random threshold for every pixel (`seed`); `gameboy_camera` is the Game Boy Camera's 4 x 4
    /// matrix; 'matrix' uses the picture `matrix`; 'tiles' draws every dot as one tile of the picture `tiles`, picked by its tone, between the
    /// two palette colours nearest to the dot (it uses no `pattern`, `phase` or `cell`). The ordered methods mix colours of any palette, in steps of a
    /// sixteenth, or of one over the cells of a matrix of fewer than 16 (`bayer2` mixes in quarters); with 'spot' separation they
    /// give each overprint its exact share of the pixel, to one over the cells of the matrix.
    #[param(default = FloydSteinberg)]
    pub method: Method,
    /// Scan odd rows right to left, with the kernel mirrored, so the grain does not slant one way (`floyd_steinberg`,
    /// 'atkinson', `jarvis_judice_ninke`, 'stucki', 'burkes', 'sierra', `sierra_two_row`, `sierra_lite`,
    /// `stevenson_arce` and `ostromoukhov`; the other methods have no scan direction). Off, every row is scanned left to right.
    #[param(default = false)]
    pub serpentine: bool,
    /// With method 'matrix': the picture whose grey values are the thresholds, the path of an image file in a recipe file
    /// (relative to the file) or in `fx`, or, in Python, a Picture or an array; in Rust, the name of a picture the photo
    /// was given or the path of an image file. At most 256 x 256 pixels, a side of any length; it repeats across the
    /// picture from its top left corner. Its cells are ranked by brightness and the brightest are lit first, equal ones
    /// in reading order (a flat grey at k of the n levels of a matrix of n cells lights k cells). Ignored with any other method; none is refused with 'matrix'.
    #[param()]
    pub matrix: PictureName,
    /// With method 'tiles': a strip of n square tiles side by side, as a picture (the path of an image file in a recipe file
    /// (relative to the file) or in `fx`, or, in Python, a Picture or an array; in Rust, the name of a picture the photo was
    /// given or the path of an image file), n x its height wide: the height is the tile's side in px, which is the size of one
    /// dot (`cell` is not used), at most 256 px and 256 tiles. Each dot takes the tile its tone picks, the first tile for the
    /// dark end and the last for the light end; the tile's pixels above half grey show the lighter of the dot's two colours and the others the
    /// darker. Ignored with any other method; none is refused with 'tiles', and so is 'spot' separation.
    #[param()]
    pub tiles: PictureName,
    /// Where the pattern sits: the tile of an ordered method, of 'matrix' or of the noise is shifted by this share of its
    /// side (rounded down to whole dots), right and down, so 0 and 1 are the same picture; `white_noise`, which has no
    /// tile, shifts by a share of the picture and wraps. The tone of every area stays the same, only where the dots fall
    /// moves. Used by the ordered, noise and 'matrix' methods, with 'spot' separation too; the error-diffusion methods
    /// (`floyd_steinberg` and the other kernels, `dot_diffusion`, `riemersma`) and 'tiles' ignore it.
    #[param(default = 0.0, unit = cycles, min = 0, max = 1, drivable)]
    pub phase: f32,
    /// Colour attributes: the width in dots of a block that may use only two of the palette's colours, as the colour
    /// attributes of the ZX Spectrum, the MSX and the C64 allow. Every block takes the pair of colours whose line lies nearest to
    /// its dots (the least summed distance in `space`) and is dithered between those two with `method`; error and thresholds
    /// carry across block borders, each dot limited to its block's pair. With a palette longer than 16 colours the pair's
    /// ends are tried among the 16 that are nearest to most of the block's dots. 0 = off, every dot takes any colour. Not with 'tiles' or 'spot' separation.
    #[param(default = 0, unit = count, min = 0, max = 256)]
    pub block: i32,
    /// With a `block`: the height of a block in dots (8 x 8 for the Spectrum and the C64, 8 x 1 for the MSX). Ignored without a `block`.
    #[param(default = 8, unit = count, min = 1, max = 256)]
    pub block_height: i32,
    /// With a `block`: 0, or the palette is read in runs of this many colours (at least 2) and a block's two colours come from
    /// one run, as the Spectrum's two colours of a block share their brightness (a run of 8 normal colours, then of 8 bright
    /// ones, black in both). 0 = any two colours. Ignored without a `block`.
    #[param(default = 0, unit = count, min = 0, max = 256)]
    pub block_group: i32,
    /// Where the colours come from. 'photo' finds `colors` colours in the picture itself (k-means); 'custom' takes your own
    /// list, `custom`; 'cube2', 'cube3', 'cube4', 'cube5' and 'cube6' are every mix of 2 to 6 evenly spaced levels of red,
    /// green and blue (8 to 216 colours). Every other choice is a named palette of 2 to 16 colours (hardware, greys, colour
    /// schemes), listed with its description and colours by `Palette::NAMED` in Rust and `named_palettes()` in Python.
    #[param(default = Photo)]
    pub palette: Palette,
    /// Your own colours, in any order ('#rrggbb', '#rgb' or three numbers 0 to 1), at least two, used when `palette` is
    /// 'custom' (ignored for any other palette).
    #[param(default = ["#000000", "#ffffff"], min_length = 2, max_length = 256)]
    pub custom: Vec<Color>,
    /// How many colours to find when `palette` is 'photo' (ignored for any other palette). Raising it keeps more detail and
    /// gradation; fewer than this many are used if the picture has fewer distinct colours. The ordered methods take
    /// longer with more colours.
    #[param(default = 8, unit = count, min = 2, max = 256)]
    pub colors: i32,
    /// The size of one dot in px at the recipe's size (rounded to whole pixels; with 'tiles' the tile's side instead): the picture is dithered on a grid of this
    /// size and shown with hard square dots. 0, the default, is one dot per pixel at any render size (so a `pattern` of 0
    /// recolours pixel by pixel); 4 is a chunky dot on a 480 px picture and 10 or more a pixel-art look. Raising it gives
    /// coarser, larger dots.
    #[param(default = 0.0, unit = px, min = 0, max = 48)]
    pub cell: f32,
    /// How much of the dot pattern is used: 1 = the method as designed; lower passes less error on (the error-diffusion
    /// methods) or mixes fewer colours per dot (the ordered methods), so flatter areas; 0 gives each dot its nearest palette
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
    /// Random seed for the choice of sample dots and starting colours when `palette` is 'photo', for the thresholds of
    /// `white_noise`, and for `ink_noise` and `specks`: the same seed always gives the same picture, another seed finds slightly different colours or other
    /// noise.
    #[param(default = 0, unit = count, min = 0, max = 4294967295, advanced)]
    pub seed: u32,
    /// How the colours are chosen: 'none' (from `palette`) or 'spot' (the `inks` printed over
    /// `paper` like transparent layers). The ordered and noise methods print each overprint (paper, each ink and every overlap) in its exact
    /// share of the pixel, picked by the pixel's own threshold; the error-diffusion ones dither the colour mixed from the inks once against the overprints (Atkinson
    /// passes on only three quarters of the error, with several inks as with one, so highlights and shadows clip).
    #[param(default = None)]
    pub separation: Separation,
    /// With 'spot' separation: the one to four ink colours ('#rrggbb', '#rgb' or three numbers 0..1), in the order they are
    /// offset; ignored otherwise. Each pixel's colour is split into amounts of these inks.
    #[param(default = ["#ff4d6d", "#2b59ff"], min_length = 1, max_length = 4)]
    pub inks: Vec<Color>,
    /// With 'spot' separation: the paper colour the inks are printed over, where no ink lands. The inks print
    /// subtractively and only darken it, so a light paper shows them best.
    #[param(default = "#ffffff")]
    pub paper: Color,
    /// With 'spot' separation: how far in px the second ink's layer is moved from the first (the third twice as far, the
    /// fourth three times), as ink misregistered on a press; 0 = all inks in register.
    #[param(default = 0.0, unit = px, min = 0, max = 100)]
    pub offset: f32,
    /// With an `offset`: the direction of the move, in degrees clockwise on screen from the x axis (90 is downward).
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub offset_angle: f32,
    /// With 'spot' separation: how much the press fattens dots, as presses quote it: the gain at 50 % tone, in tone (0.2 = a
    /// 50 % tint prints as 70 %). An ink's coverage `a` becomes `a + 4 x dot_gain x a x (1 - a)`: the simplest curve through 0
    /// and 1 that is symmetric and gains `dot_gain` at 50 % (ISO 12647-2 quotes dot gain at 50 %), so highlights and shadows
    /// gain less. 0 = none.
    #[param(default = 0.0, unit = fraction, min = 0, max = 0.25, advanced)]
    pub dot_gain: f32,
    /// With 'spot' separation: how uneven the ink is from dot to dot: each dot's coverage strays by a random share, up to this
    /// one, of the nearer of its ink and its paper (the mean ink stays the same). 0 = even. It varies the coverage of each
    /// dot, not each pixel: a press leaves one dot fat and the next thin, not grain.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub ink_noise: f32,
    /// With 'spot' separation: specks of bare paper in the ink: marks per 1000 design px², so any picture size looks alike; they
    /// show only where there is ink. 0 = none.
    #[param(default = 0.0, unit = density, min = 0, max = 5, advanced)]
    pub specks: f32,
    /// With 'spot' separation: the diameter in px of a paper speck.
    #[param(default = 3.0, unit = px, min = 1, max = 10, advanced)]
    pub speck_size: f32,
}

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
        Palette::Cube2 => cube(2), // constant: the cubes offered run from 2 levels to 6
        Palette::Cube3 => cube(3), // constant: the cubes offered run from 2 levels to 6
        Palette::Cube4 => cube(4), // constant: the cubes offered run from 2 levels to 6
        Palette::Cube5 => cube(5), // constant: the cubes offered run from 2 levels to 6
        Palette::Cube6 => cube(6), // constant: the cubes offered run from 2 levels to 6
        Palette::Photo => found(dots, matched, params),
        named => hex(named.named().unwrap_or_default()), // every variant left is a named palette: it has colours
    }
}

/// The `photo` palette: see [`colours`].
fn found(dots: &[Point], matched: &[Point], params: &Params) -> Vec<Point> {
    let search = Search {
        colors: count(params.colors),
        start: match params.init {
            Init::Spread => Start::Spread,
            Init::Random => Start::Random,
        },
        rounds: count(params.iterations),
        tolerance: params.tolerance,
        sample: count(params.sample),
        seed: params.seed,
    };
    palette::found(dots, matched, &search)
        .into_iter()
        .map(|(colour, _)| colour)
        .collect()
}

/// The `chosen` method on `values` (`size` dots), as a pattern of palette indices.
fn pattern(
    method: Method,
    values: &[Point],
    size: (usize, usize),
    palette: &Choices<'_>,
    params: &Params,
    inputs: &Inputs<'_>,
    space: Space,
) -> Vec<u8> {
    let (amount, serpentine) = (params.pattern, params.serpentine);
    let lightness = if space == Space::Rgb {
        [0.2126, 0.7152, 0.0722] // constant: Rec. 709 luma weights, ITU-R BT.709
    } else {
        [1.0, 0.0, 0.0]
    };
    let by = |kernel| diffuse(values, size, palette, kernel, amount, serpentine, lightness);
    if let Some(thresholds) = thresholds(method, params, size, inputs) {
        return ordered(values, size, palette, &thresholds, amount);
    }
    match method {
        Method::FloydSteinberg => by(&FLOYD_STEINBERG),
        Method::Atkinson => by(&ATKINSON),
        Method::JarvisJudiceNinke => by(&JARVIS_JUDICE_NINKE),
        Method::Stucki => by(&STUCKI),
        Method::Burkes => by(&BURKES),
        Method::Sierra => by(&SIERRA),
        Method::SierraTwoRow => by(&SIERRA_TWO_ROW),
        Method::SierraLite => by(&SIERRA_LITE),
        Method::StevensonArce => by(&STEVENSON_ARCE),
        Method::Ostromoukhov => by(&OSTROMOUKHOV),
        Method::DotDiffusion => dot_diffusion(values, size, palette, amount),
        _ => riemersma(values, size, palette, amount), // the ordered methods returned above
    }
}

/// The thresholds of an ordered or noise `method` on a picture of `size` dots, moved by `phase`; none for a method that
/// passes error on.
fn thresholds(
    method: Method,
    params: &Params,
    size: (usize, usize),
    inputs: &Inputs<'_>,
) -> Option<Thresholds<'static>> {
    let thresholds = match method {
        Method::Bayer2 => Tile::Bayer2.thresholds(),
        Method::Bayer4 => Tile::Bayer4.thresholds(),
        Method::Bayer => Tile::Bayer.thresholds(),
        Method::Clustered => Tile::Clustered.thresholds(),
        Method::Clustered45 => Tile::Clustered45.thresholds(),
        Method::BlueNoise => Tile::BlueNoise.thresholds(),
        Method::WhiteNoise => noise(params.seed, size),
        Method::GameboyCamera => Tile::GameboyCamera.thresholds(),
        Method::Matrix => matrix_of(inputs),
        _ => return None,
    };
    Some(thresholds.phased(params.phase))
}

/// What `dither` refuses: method 'matrix' with no `matrix`, or one more than 256 pixels wide or high; method 'tiles' with
/// no `tiles`, a strip that is not a whole number of tiles wide or has too large a tile or too many, or 'spot' separation.
///
/// # Errors
/// [`Error::Recipe`] naming `matrix`, `tiles` or `separation`.
pub(crate) fn check(params: &Params, inputs: &Inputs<'_>) -> Result<(), Error> {
    if params.block > 0 {
        check_block(params)?;
    }
    match params.method {
        Method::Matrix => check_matrix("dither", "method 'matrix'", inputs),
        Method::Tiles if params.separation == Separation::Spot => Err(Error::Recipe(
            "dither: method 'tiles' cannot be used with `separation` 'spot': a tile is drawn between two palette colours, and \
             the spot inks print as overlapping layers of ink, not as a palette"
                .to_owned(),
        )),
        Method::Tiles => tiles::check_strip("dither", "method 'tiles'", inputs),
        _ => Ok(()),
    }
}

/// What a `block` refuses: 'tiles' (it draws between a pair of its own) and 'spot' separation (overlapping inks, not a
/// palette), and a `block_group` of 1 (a pair needs two colours of a run).
fn check_block(params: &Params) -> Result<(), Error> {
    let cause = if params.method == Method::Tiles {
        "method 'tiles' draws each dot between a pair of palette colours of its own"
    } else if params.separation == Separation::Spot {
        "`separation` 'spot' prints overlapping inks, not a palette to pick pairs from"
    } else if params.block_group == 1 {
        "`block_group` 1 leaves a run of one colour, and a block needs two: use 0 or at least 2"
    } else {
        return Ok(());
    };
    Err(Error::Recipe(format!("dither: `block` cannot be used: {cause}")))
}

/// The thresholds of the picture `matrix`, which `check` has seen.
fn matrix_of(inputs: &Inputs<'_>) -> Thresholds<'static> {
    let (greys, size) = matrix_greys(inputs);
    from_greys(&greys, size)
}

/// The inks printed, decided: which inks land on which cell, and what each combination of them prints.
struct Inked {
    /// For each cell, the bits of the inks that land on it.
    layers: Vec<u8>,
    /// The whole pixels each ink's layer is moved by, (x, y).
    moves: Vec<[isize; 2]>,
    /// The print of every combination of inks, by the bits of the inks that land: the overprint palette.
    table: Vec<[f32; 3]>,
    real: Realism,
    columns: usize,
    cell: usize,
    size: (usize, usize),
}

impl Inked {
    /// Whether `ink` lands on the pixel (`x`, `y`): its layer moved by its share of the offset, none from off the picture.
    fn lands(&self, ink: usize, x: usize, y: usize) -> bool {
        let (height, width) = self.size;
        let by = self.moves[ink];
        // where the layer comes from; none where that is off the picture
        let from = |at: usize, away: isize, size: usize| at.checked_add_signed(-away).filter(|&at| at < size);
        match (from(y, by[1], height), from(x, by[0], width)) {
            (Some(row), Some(column)) => {
                usize::from(self.layers[row / self.cell * self.columns + column / self.cell]) >> ink & 1 == 1
            }
            _ => false,
        }
    }
}

/// The inks decided: the dots' colours are split into amounts of the inks (after the press's dot gain and uneven ink),
/// mixed in the print model into one target colour per dot, and that is dithered once with `method` against the overprint
/// palette, the print of every combination of inks. The inks of the colour chosen for a pixel (the bits of its index) are
/// moved by their share of the offset.
fn inked(
    dots: &[Point],
    (rows, columns): (usize, usize),
    cell: usize,
    (height, width): (usize, usize),
    params: &Params,
    inputs: &Inputs<'_>,
) -> Inked {
    let colours: Vec<[f32; 3]> = params.inks.iter().map(|ink| ink.0).collect();
    let inks = Inks::new(params.paper.0, &colours);
    let count = colours.len().min(MOST_INKS);
    let real = Realism {
        dot_gain: params.dot_gain,
        ink_noise: params.ink_noise,
        specks: params.specks,
        speck_size: params.speck_size,
        seed: params.seed,
    };
    let mut amounts = vec![[0.0; MOST_INKS]; dots.len()];
    par::rows(&mut amounts, 1, |first, band| {
        for (offset, (out, dot)) in band.iter_mut().zip(&dots[first..]).enumerate() {
            let at = first + offset;
            let cell = [(at / columns) as u64, (at % columns) as u64]; // constant: a usize is at most 64 bits wide here, so it fits
            *out = inks.coverages(*dot);
            for (ink, amount) in out.iter_mut().enumerate() {
                *amount = real.coverage(*amount, ink, cell);
            }
        }
    });
    // the print of every combination of inks, by the bits of the inks that land: the overprint palette
    let table: Vec<[f32; 3]> = (0..1_usize << count)
        .map(|mask| inks.print(&std::array::from_fn(|ink| if mask >> ink & 1 == 1 { 1.0 } else { 0.0 })))
        .collect();
    let layers = if let Some(thresholds) = thresholds(params.method, params, (rows, columns), inputs) {
        shared(&amounts, count, (rows, columns), &thresholds, params.pattern)
    } else {
        let target: Vec<[f32; 3]> = amounts.iter().map(|amounts| inks.print(amounts)).collect();
        let table = Choices::new(&table, None);
        // in sRGB, where the print model mixes: distances in Oklab would pull the mean of the dots off the mix of the inks
        pattern(
            params.method,
            &target,
            (rows, columns),
            &table,
            params,
            inputs,
            Space::Rgb,
        )
    };
    // whole pixels: the nearest to the move
    let whole_move = |away: f32| {
        let pixels = whole(f64::from(away.abs())).cast_signed();
        if away < 0.0 { -pixels } else { pixels }
    };
    let moves: Vec<[isize; 2]> = (0..count)
        .map(|ink| shift(params.offset, params.offset_angle, ink).map(whole_move))
        .collect();
    Inked {
        layers,
        moves,
        table,
        real,
        columns,
        cell,
        size: (height, width),
    }
}

/// The inks printed: every pixel gets the print of the inks that land on it.
fn printed(print: &Inked) -> Image {
    let (height, width) = print.size;
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let y = first_row + offset;
            for (x, pixel) in line.iter_mut().enumerate() {
                let mask =
                    (0..print.moves.len()).fold(0, |mask, ink| mask | usize::from(print.lands(ink, x, y)) << ink);
                *pixel = if mask == 0 {
                    print.table[0]
                } else {
                    Realism::specked(print.table[mask], print.table[0], print.real.speck(x, y))
                };
            }
        }
    });
    Image::from_pixels(height, width, out)
}

/// One plane for each ink: 1 where it lands on the pixel, else 0.
fn planes(print: &Inked) -> Vec<Layer> {
    let (height, width) = print.size;
    (0..print.moves.len())
        .map(|ink| {
            let mut plane = vec![0.0_f32; height * width];
            par::rows(&mut plane, width, |first_row, band| {
                for (offset, line) in band.chunks_exact_mut(width).enumerate() {
                    for (x, level) in line.iter_mut().enumerate() {
                        *level = f32::from(u8::from(print.lands(ink, x, first_row + offset)));
                    }
                }
            });
            Layer::from_checked(height, width, plane)
        })
        .collect()
}

/// What `dither` chooses for a picture drawn from a palette: the palette and the index of its colour for each dot.
struct Chosen {
    palette: Vec<Point>,
    /// The palette index of each dot, row after row of `columns`.
    indices: Vec<u8>,
    columns: usize,
    cell: usize,
}

impl Chosen {
    /// The index of the colour of the pixel (`row`, `column`).
    fn at(&self, row: usize, column: usize) -> u8 {
        self.indices[row / self.cell * self.columns + column / self.cell]
    }
}

/// The dots' colours chosen from a palette by `params.method` (not `tiles`, which draws between two colours of its own).
fn chosen(dots: &[Point], size: (usize, usize), cell: usize, params: &Params, inputs: &Inputs<'_>) -> Chosen {
    let matched = in_space(dots, params.space);
    let palette = colours(dots, &matched, params);
    let matched_palette = in_space(&palette, params.space);
    let blocks = (params.block > 0).then(|| {
        let shape = Shape {
            width: count(params.block),
            height: count(params.block_height),
            group: count(params.block_group),
        };
        Blocks::choose(&matched, size, &matched_palette, shape)
    });
    let indices = pattern(
        params.method,
        &matched,
        size,
        &Choices::new(&matched_palette, blocks.as_ref()),
        params,
        inputs,
        params.space,
    );
    Chosen {
        palette,
        indices,
        columns: size.1,
        cell,
    }
}

/// The picture painted with the colours `chosen` gave its dots.
fn painted(chosen: &Chosen, (height, width): (usize, usize)) -> Image {
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first_row + offset;
            for (column, pixel) in line.iter_mut().enumerate() {
                *pixel = chosen.palette[usize::from(chosen.at(row, column))];
            }
        }
    });
    Image::from_pixels(height, width, out)
}

/// The dots of `picture` and the grid they make: the cell side, the rows and columns of dots.
fn dots_of<'a>(
    picture: &Picture<'a>,
    params: &Params,
    inputs: &Inputs<'_>,
) -> (usize, (usize, usize), Cow<'a, [Point]>) {
    let (height, width) = (picture.height(), picture.width());
    let cell = if params.method == Method::Tiles {
        Strip::of(inputs).side()
    } else {
        whole(f64::from(params.cell)).max(1)
    };
    let dots = averaged(picture.pixels(), (height, width), cell);
    (cell, (height.div_ceil(cell), width.div_ceil(cell)), dots)
}

/// The picture redrawn.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let size = (picture.height(), picture.width());
    let (cell, grid, dots) = dots_of(picture, params, inputs);
    if params.separation == Separation::Spot {
        return printed(&inked(&dots, grid, cell, size, params, inputs));
    }
    if params.method == Method::Tiles {
        let matched = in_space(&dots, params.space);
        let palette = colours(&dots, &matched, params);
        let matched_palette = in_space(&palette, params.space);
        let out = tiles::draw((&matched, &matched_palette), &palette, &Strip::of(inputs), size);
        return Image::from_pixels(size.0, size.1, out);
    }
    painted(&chosen(&dots, grid, cell, params, inputs), size)
}

/// What `dither` makes beside its picture: with 'spot' `separation` an ink plane for each ink, else the index of each
/// pixel's palette colour and the palette.
///
/// # Errors
/// [`Error::Recipe`] naming `method` for 'tiles', which draws between two palette colours, so has no one colour for a pixel.
pub(crate) fn data(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Result<Data, Error> {
    let (height, width) = (picture.height(), picture.width());
    let (cell, grid, dots) = dots_of(picture, params, inputs);
    if params.separation == Separation::Spot {
        return Ok(Data::Planes(planes(&inked(
            &dots,
            grid,
            cell,
            (height, width),
            params,
            inputs,
        ))));
    }
    if params.method == Method::Tiles {
        return Err(Error::Recipe(
            "dither: `method` 'tiles' draws each dot between two palette colours, so it has no palette index map"
                .to_owned(),
        ));
    }
    let made = chosen(&dots, grid, cell, params, inputs);
    let labels = (0..height)
        .flat_map(|row| (0..width).map(move |column| (row, column)))
        .map(|(row, column)| i32::from(made.at(row, column)))
        .collect();
    let map = Labels::new(height, width, labels, made.palette.len())?;
    Ok(Data::Palette {
        map,
        colours: made.palette,
    })
}

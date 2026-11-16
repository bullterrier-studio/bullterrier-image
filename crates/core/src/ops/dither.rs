//! Dithering: choosing a palette colour for every pixel so that, seen from afar, the pixels average to the picture.
//!
//! Error diffusion ([`diffuse`]) passes each pixel's error on to the pixels after it; ordered dithering ([`ordered`])
//! shifts each pixel by a repeating pattern ([`Tile`]) before choosing. Both return the index of the chosen palette colour
//! per pixel (a palette has at most 256 colours); the caller paints them.

use std::borrow::Cow;
use std::sync::OnceLock;

use crate::ops::cluster::{Point, nearest_one};
use crate::ops::length::whole;
use crate::ops::par;
use crate::ops::random::{Rng, mix};

/// Where an error-diffusion kernel sends a pixel's error: (columns right, rows down, weight) for the pixels it reaches,
/// and the weights' divisor. Every tap lies after the pixel in reading order.
pub(crate) struct Kernel {
    taps: &'static [(isize, usize, f32)],
    divisor: f32,
    /// For a kernel whose weights depend on the pixel's level: the weights of each tap for levels 0 to 127, replacing the
    /// taps' own weights, with their sum as the divisor.
    levels: Option<&'static [[u16; 3]; 128]>,
}

/// Floyd and Steinberg (1976): 7/16 right, 3/16 below left, 5/16 below, 1/16 below right.
pub(crate) const FLOYD_STEINBERG: Kernel = Kernel {
    taps: &[(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)], // constant: Floyd and Steinberg (1976)
    levels: None,
    divisor: 16.0, // constant: Floyd and Steinberg (1976)
};

/// Atkinson (Apple MacPaint): 1/8 to each of six pixels, so a quarter of the error is dropped.
pub(crate) const ATKINSON: Kernel = Kernel {
    taps: &[
        (1, 0, 1.0),
        (2, 0, 1.0),
        (-1, 1, 1.0),
        (0, 1, 1.0),
        (1, 1, 1.0),
        (0, 2, 1.0),
    ], // constant: Atkinson's published error weights
    levels: None,
    divisor: 8.0, // constant: Atkinson's published error weights
};

/// Jarvis, Judice and Ninke (1976): 7/48 and 5/48 right; 3 5 7 5 3 below; 1 3 5 3 1 two below.
pub(crate) const JARVIS_JUDICE_NINKE: Kernel = Kernel {
    taps: &[
        (1, 0, 7.0),
        (2, 0, 5.0),
        (-2, 1, 3.0),
        (-1, 1, 5.0),
        (0, 1, 7.0),
        (1, 1, 5.0),
        (2, 1, 3.0),
        (-2, 2, 1.0),
        (-1, 2, 3.0),
        (0, 2, 5.0),
        (1, 2, 3.0),
        (2, 2, 1.0),
    ], // constant: Jarvis, Judice and Ninke (1976)
    levels: None,
    divisor: 48.0, // constant: Jarvis, Judice and Ninke (1976)
};

/// Stucki (1981): JJN's shape with weights 8 4 / 2 4 8 4 2 / 1 2 4 2 1 over 42.
pub(crate) const STUCKI: Kernel = Kernel {
    taps: &[
        (1, 0, 8.0),
        (2, 0, 4.0),
        (-2, 1, 2.0),
        (-1, 1, 4.0),
        (0, 1, 8.0),
        (1, 1, 4.0),
        (2, 1, 2.0),
        (-2, 2, 1.0),
        (-1, 2, 2.0),
        (0, 2, 4.0),
        (1, 2, 2.0),
        (2, 2, 1.0),
    ], // constant: Stucki (1981)
    levels: None,
    divisor: 42.0, // constant: Stucki (1981)
};

/// Burkes (1988): Stucki without its third row, 8 4 / 2 4 8 4 2 over 32.
pub(crate) const BURKES: Kernel = Kernel {
    taps: &[
        (1, 0, 8.0),
        (2, 0, 4.0),
        (-2, 1, 2.0),
        (-1, 1, 4.0),
        (0, 1, 8.0),
        (1, 1, 4.0),
        (2, 1, 2.0),
    ], // constant: Burkes (1988)
    levels: None,
    divisor: 32.0, // constant: Burkes (1988)
};

/// Sierra (1989), three rows: 5 3 / 2 4 5 4 2 / 2 3 2 over 32.
pub(crate) const SIERRA: Kernel = Kernel {
    taps: &[
        (1, 0, 5.0),
        (2, 0, 3.0),
        (-2, 1, 2.0),
        (-1, 1, 4.0),
        (0, 1, 5.0),
        (1, 1, 4.0),
        (2, 1, 2.0),
        (-1, 2, 2.0),
        (0, 2, 3.0),
        (1, 2, 2.0),
    ], // constant: Sierra (1989)
    levels: None,
    divisor: 32.0, // constant: Sierra (1989)
};

/// Sierra (1989), two rows: 4 3 / 1 2 3 2 1 over 16.
pub(crate) const SIERRA_TWO_ROW: Kernel = Kernel {
    taps: &[
        (1, 0, 4.0),
        (2, 0, 3.0),
        (-2, 1, 1.0),
        (-1, 1, 2.0),
        (0, 1, 3.0),
        (1, 1, 2.0),
        (2, 1, 1.0),
    ], // constant: Sierra (1989), two-row
    levels: None,
    divisor: 16.0, // constant: Sierra (1989), two-row
};

/// Sierra (1989), Lite: 2 right, 1 below left, 1 below, over 4.
pub(crate) const SIERRA_LITE: Kernel = Kernel {
    taps: &[(1, 0, 2.0), (-1, 1, 1.0), (0, 1, 1.0)], // constant: Sierra (1989), Filter Lite
    levels: None,
    divisor: 4.0, // constant: Sierra (1989), Filter Lite
};

/// Stevenson and Arce (1985), a hexagonal lattice's weights on the square grid: 32 two right; 12 26 30 16 below; 12 26 12; 5 12 12 5, over 200.
pub(crate) const STEVENSON_ARCE: Kernel = Kernel {
    taps: &[
        (2, 0, 32.0),
        (-3, 1, 12.0),
        (-1, 1, 26.0),
        (1, 1, 30.0),
        (3, 1, 16.0),
        (-2, 2, 12.0),
        (0, 2, 26.0),
        (2, 2, 12.0),
        (-3, 3, 5.0),
        (-1, 3, 12.0),
        (1, 3, 12.0),
        (3, 3, 5.0),
    ], // constant: Stevenson and Arce (1985)
    levels: None,
    divisor: 200.0, // constant: Stevenson and Arce (1985)
};

/// Ostromoukhov (2001), variable coefficients: the weights of the right, below-left and below pixels depend on the level of
/// the pixel (its value plus the error passed to it, as lightness in Oklab or the Rec. 709 luma in rgb; clamped to 0 to 1
/// and taken in 256 levels), from the paper's appendix table for levels 0 to 127, mirrored for 128 to 255.
pub(crate) const OSTROMOUKHOV: Kernel = Kernel {
    taps: &[(1, 0, 0.0), (-1, 1, 0.0), (0, 1, 0.0)], // constant: Ostromoukhov (2001), the weights come from `levels`
    divisor: 0.0,                                    // constant: Ostromoukhov (2001), the sum of a level's weights
    levels: Some(&OSTROMOUKHOV_LEVELS),
};

/// The paper's appendix table: (right, below left, below) for levels 0 to 127.
// constant: Ostromoukhov, "A Simple and Efficient Error-Diffusion Algorithm", SIGGRAPH 2001, Appendix I
// (https://perso.liris.cnrs.fr/victor.ostromoukhov/publications/pdf/SIGGRAPH01_varcoeffED.pdf), agreeing with libpipi's
// pipi/dither/ostromoukhov.c (https://gitea.zoy.org/cacalabs/libpipi)
const OSTROMOUKHOV_LEVELS: [[u16; 3]; 128] = [
    [13, 0, 5],
    [13, 0, 5],
    [21, 0, 10],
    [7, 0, 4],
    [8, 0, 5],
    [47, 3, 28],
    [23, 3, 13],
    [15, 3, 8],
    [22, 6, 11],
    [43, 15, 20],
    [7, 3, 3],
    [501, 224, 211],
    [249, 116, 103],
    [165, 80, 67],
    [123, 62, 49],
    [489, 256, 191],
    [81, 44, 31],
    [483, 272, 181],
    [60, 35, 22],
    [53, 32, 19],
    [237, 148, 83],
    [471, 304, 161],
    [3, 2, 1],
    [481, 314, 185],
    [354, 226, 155],
    [1389, 866, 685],
    [227, 138, 125],
    [267, 158, 163],
    [327, 188, 220],
    [61, 34, 45],
    [627, 338, 505],
    [1227, 638, 1075],
    [20, 10, 19],
    [1937, 1000, 1767],
    [977, 520, 855],
    [657, 360, 551],
    [71, 40, 57],
    [2005, 1160, 1539],
    [337, 200, 247],
    [2039, 1240, 1425],
    [257, 160, 171],
    [691, 440, 437],
    [1045, 680, 627],
    [301, 200, 171],
    [177, 120, 95],
    [2141, 1480, 1083],
    [1079, 760, 513],
    [725, 520, 323],
    [137, 100, 57],
    [2209, 1640, 855],
    [53, 40, 19],
    [2243, 1720, 741],
    [565, 440, 171],
    [759, 600, 209],
    [1147, 920, 285],
    [2311, 1880, 513],
    [97, 80, 19],
    [335, 280, 57],
    [1181, 1000, 171],
    [793, 680, 95],
    [599, 520, 57],
    [2413, 2120, 171],
    [405, 360, 19],
    [2447, 2200, 57],
    [11, 10, 0],
    [158, 151, 3],
    [178, 179, 7],
    [1030, 1091, 63],
    [248, 277, 21],
    [318, 375, 35],
    [458, 571, 63],
    [878, 1159, 147],
    [5, 7, 1],
    [172, 181, 37],
    [97, 76, 22],
    [72, 41, 17],
    [119, 47, 29],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [65, 18, 17],
    [95, 29, 26],
    [185, 62, 53],
    [30, 11, 9],
    [35, 14, 11],
    [85, 37, 28],
    [55, 26, 19],
    [80, 41, 29],
    [155, 86, 59],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [305, 176, 119],
    [155, 86, 59],
    [105, 56, 39],
    [80, 41, 29],
    [65, 32, 23],
    [55, 26, 19],
    [335, 152, 113],
    [85, 37, 28],
    [115, 48, 37],
    [35, 14, 11],
    [355, 136, 109],
    [30, 11, 9],
    [365, 128, 107],
    [185, 62, 53],
    [25, 8, 7],
    [95, 29, 26],
    [385, 112, 103],
    [65, 18, 17],
    [395, 104, 101],
    [4, 1, 1],
];

/// The widest sideways reach of a kernel, in columns; the error rows are padded by it on both sides.
const REACH: usize = 3; // constant: Stevenson and Arce's taps reach three columns
/// The rows an error is passed down to, and so how many error rows are alive at once, with the one being read.
const ROWS_ALIVE: usize = 4; // constant: Stevenson and Arce's taps reach three rows down

/// The row of a level table for `value` (0 to 1, clamped): its level of 256, turned about 127 for the upper half.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a clamped value of 0 to 256 truncates to a level"
)]
fn folded_level(value: f32) -> usize {
    let level = (value.clamp(0.0, 1.0) * 255.9999) as usize; // constant: Ostromoukhov's 256 levels
    if level > 127 { 255 - level } else { level }
}

/// The level of `value` (before folding): its channels weighted by `lightness`.
fn level_of(value: [f32; 3], lightness: [f32; 3]) -> f32 {
    value.iter().zip(lightness).map(|(channel, weight)| channel * weight).sum()
}

/// The palette colour chosen for each pixel of `values` (`height` x `width`, row after row) by error diffusion: indices
/// into `palette`.
///
/// Pixels are taken in reading order; each gets the palette colour closest to its value plus the error passed to it, and
/// `amount` (0 to 1) of the difference between the two is passed on by `kernel`. An error that falls off the picture is
/// lost. With `serpentine`, odd rows are taken right to left and the kernel is mirrored for them. A kernel whose weights
/// depend on the pixel's level takes it as the sum of the value's channels times `lightness`.
pub(crate) fn diffuse(
    values: &[Point],
    (height, width): (usize, usize),
    palette: &[Point],
    kernel: &Kernel,
    amount: f32,
    serpentine: bool,
    lightness: [f32; 3],
) -> Vec<u8> {
    let stride = width + 2 * REACH;
    // the errors waiting for the rows from the current one down; a row's slot is cleared once it is read
    let mut waiting = vec![[0.0_f32; 3]; ROWS_ALIVE * stride];
    let mut chosen = vec![0; height * width];
    let fixed_scale = amount / kernel.divisor;
    for row in 0..height {
        let backwards = serpentine && row % 2 == 1; // constant: odd rows are the ones turned back
        for step in 0..width {
            let column = if backwards { width - 1 - step } else { step };
            let at = (row % ROWS_ALIVE) * stride + REACH + column;
            let pixel = values[row * width + column];
            let value = [0, 1, 2].map(|channel| pixel[channel] + waiting[at][channel]);
            let pick = nearest_one(&value, palette);
            chosen[row * width + column] = pick;
            let colour = palette[usize::from(pick)];
            let level = kernel.levels.map(|table| table[folded_level(level_of(value, lightness))].map(f32::from));
            let scale = level.map_or(fixed_scale, |row| amount / row.iter().sum::<f32>());
            let error = [0, 1, 2].map(|channel| (value[channel] - colour[channel]) * scale);
            for (tap, &(right, down, weight)) in kernel.taps.iter().enumerate() {
                let weight = level.map_or(weight, |row| row[tap]);
                let right = if backwards { -right } else { right };
                let to = ((row + down) % ROWS_ALIVE) * stride + (REACH + column).wrapping_add_signed(right);
                for (sum, part) in waiting[to].iter_mut().zip(error) {
                    *sum += part * weight;
                }
            }
        }
        let done = (row % ROWS_ALIVE) * stride;
        waiting[done..done + stride].fill([0.0; 3]);
    }
    chosen
}

/// The repeating pattern of an ordered dither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tile {
    /// The 2 x 2 matrix of Bayer's recursive construction.
    Bayer2,
    /// The 4 x 4 matrix of Bayer's recursive construction.
    Bayer4,
    /// The 8 x 8 matrix of Bayer's recursive construction, a regular crosshatch.
    Bayer,
    /// An 8 x 8 clustered-dot matrix, dots in a square grid.
    Clustered,
    /// An 8 x 8 clustered-dot matrix, dots on a 45 degree grid.
    Clustered45,
    /// A 64 x 64 void-and-cluster tile: the points below any level are spread evenly, with no clumps or lines.
    BlueNoise,
    /// The Game Boy Camera's 4 x 4 matrix.
    GameboyCamera,
}

/// The ranks of the straight clustered-dot matrix, 8 x 8, row after row: `ImageMagick`'s `h8x8o` (`config/thresholds.xml`,
/// from Gernot Hoffmann's "Dithering & Halftoning"), levels 1 to 64 turned round (63 - level + 1) so that the cell lit last
/// is the dot's centre, as `ordered` lights the highest rank first.
// constant: ImageMagick h8x8o, https://github.com/ImageMagick/ImageMagick/blob/main/config/thresholds.xml
const CLUSTERED: [u16; 64] = [
    57, 43, 31, 21, 28, 45, 55, 60, 48, 37, 13, 9, 15, 35, 50, 53, 33, 17, 7, 3, 5, 19, 29, 41, 23, 11, 4, 0, 2, 12,
    24, 26, 27, 20, 6, 1, 8, 18, 34, 42, 49, 36, 16, 10, 14, 38, 47, 54, 56, 46, 30, 22, 32, 44, 58, 62, 59, 51, 39,
    25, 40, 52, 61, 63,
];

/// The ranks of the 45 degree clustered-dot matrix, 8 x 8, row after row: the Go dither library's
/// `ClusteredDotDiagonal8x8` (from the libcaca study, part 2), turned round (63 - rank) as `CLUSTERED` is.
// constant: https://github.com/makeworld-the-better-one/dither/blob/master/ordered_ditherers.go, http://caca.zoy.org/study/part2.html
const CLUSTERED_45: [u16; 64] = [
    39, 53, 51, 37, 28, 16, 14, 26, 55, 63, 61, 49, 18, 4, 2, 12, 41, 57, 59, 47, 20, 6, 0, 10, 33, 43, 45, 35, 30, 22,
    8, 24, 29, 17, 15, 27, 38, 52, 50, 36, 19, 5, 3, 13, 54, 62, 60, 48, 21, 7, 1, 11, 40, 56, 58, 46, 31, 23, 9, 25,
    32, 42, 44, 34,
];

/// The Game Boy Camera's matrix, 4 x 4, row after row: the order of the 16 cells' thresholds (all three levels order them
/// alike), as the camera's registers A006 on hold them (cell x + 4 y at 3 registers a cell); the camera lights the lowest
/// threshold first, so the ranks are turned round (15 - order).
// constant: https://github.com/HerrZatacke/dither-pattern-gen/blob/master/reference/patterns.mjs (first Low Light pattern), the DashBoy camera's SD/config.json
const GAMEBOY_CAMERA: [u16; 16] = [15, 3, 12, 0, 7, 11, 4, 8, 13, 1, 14, 2, 5, 9, 6, 10];

/// The thresholds of an ordered dither: a rank for every pixel, out of `count` ranks, the highest lit first.
pub(crate) enum Thresholds<'a> {
    /// A tile of `rows` x `columns` ranks, row after row, repeating from the picture's top left corner.
    Tiled {
        ranks: Cow<'a, [u16]>,
        rows: usize,
        columns: usize,
        count: usize,
    },
    /// A rank drawn for each pixel from `seed` and the pixel's place, `row * width + column`.
    Noise { seed: u32 },
}

/// The ranks of white noise: a pixel's rank is one of this many levels.
const NOISE_LEVELS: usize = 1 << 16; // constant: a rank is 16 bits

impl Thresholds<'_> {
    fn count(&self) -> usize {
        match self {
            Self::Tiled { count, .. } => *count,
            Self::Noise { .. } => NOISE_LEVELS,
        }
    }

    fn rank(&self, (row, column): (usize, usize), width: usize) -> usize {
        match self {
            Self::Tiled {
                ranks, rows, columns, ..
            } => usize::from(ranks[(row % rows) * columns + column % columns]),
            Self::Noise { seed } => {
                // the top 16 bits of the word hashed from the seed and the pixel's place
                usize::try_from(mix(*seed, &[u64::try_from(row * width + column).unwrap_or_default()]) >> 48)
                    .unwrap_or_default()
            }
        }
    }
}

/// A caller's matrix as thresholds: `greys` (`rows` x `columns`, row after row) ranked, the brightest cell highest and so
/// lit first; of equal cells the earlier in reading order is lit first.
pub(crate) fn from_greys(greys: &[f32], (rows, columns): (usize, usize)) -> Thresholds<'static> {
    let mut order: Vec<usize> = (0..greys.len()).collect();
    order.sort_by(|&a, &b| greys[b].total_cmp(&greys[a]));
    let mut ranks = vec![0_u16; greys.len()];
    for (place, &cell) in order.iter().enumerate() {
        ranks[cell] = u16::try_from(greys.len() - 1 - place).unwrap_or(u16::MAX);
    }
    Thresholds::Tiled {
        ranks: Cow::Owned(ranks),
        rows,
        columns,
        count: greys.len(),
    }
}

/// White noise from `seed`.
pub(crate) const fn noise(seed: u32) -> Thresholds<'static> {
    Thresholds::Noise { seed }
}

/// The blue-noise tile's side.
const BLUE_SIDE: usize = 64; // constant: Ulichney (1993), void-and-cluster: 64 x 64, so 4096 levels
/// The standard deviation in px of the Gaussian that measures how crowded a spot is.
const BLUE_SIGMA: f64 = 1.5; // constant: Ulichney (1993), void-and-cluster
/// The share of the points the first pattern starts with, as a divisor of the tile's cell count.
const BLUE_START: usize = 10; // constant: Ulichney (1993): the first pattern holds a tenth of the points
/// The seed of the first pattern: any seed gives a good tile; this one is fixed, so the pattern is the same everywhere.
const BLUE_SEED: u32 = 0; // constant: any fixed seed makes the same tile everywhere

impl Tile {
    /// The tile's thresholds, its ranks 0 to cells - 1 each once. Blue noise is built once.
    pub(crate) fn thresholds(self) -> Thresholds<'static> {
        static BLUE: OnceLock<Vec<u16>> = OnceLock::new();
        let (ranks, side): (Cow<'static, [u16]>, usize) = match self {
            Self::Bayer2 => (Cow::Owned(bayer(2)), 2),         // constant: a 2 x 2 matrix
            Self::Bayer4 => (Cow::Owned(bayer(4)), 4),         // constant: a 4 x 4 matrix
            Self::Bayer => (Cow::Owned(bayer(8)), 8),          // constant: Bayer (1973), the 8 x 8 matrix, 64 levels
            Self::Clustered => (Cow::Borrowed(&CLUSTERED), 8), // constant: an 8 x 8 matrix
            Self::Clustered45 => (Cow::Borrowed(&CLUSTERED_45), 8), // constant: an 8 x 8 matrix
            Self::BlueNoise => (Cow::Borrowed(BLUE.get_or_init(blue_noise).as_slice()), BLUE_SIDE),
            Self::GameboyCamera => (Cow::Borrowed(&GAMEBOY_CAMERA), 4), // constant: a 4 x 4 matrix
        };
        Thresholds::Tiled {
            count: side * side,
            ranks,
            rows: side,
            columns: side,
        }
    }
}

/// The ranks of the `side` x `side` Bayer matrix: from [[0]], each doubling is [[4M, 4M + 2], [4M + 3, 4M + 1]].
fn bayer(wanted: usize) -> Vec<u16> {
    let mut side = 1; // constant: Bayer (1973) starts from the 1 x 1 matrix [[0]]
    let mut ranks = vec![0_u16];
    while side < wanted {
        let mut doubled = vec![0; 4 * side * side]; // constant: a doubling has four quarters
        for row in 0..side {
            for column in 0..side {
                let base = 4 * ranks[row * side + column]; // constant: Bayer (1973), the recursive construction
                let at = |down: usize, right: usize| (row + down * side) * 2 * side + column + right * side;
                doubled[at(0, 0)] = base;
                doubled[at(0, 1)] = base + 2; // constant: Bayer (1973), the recursive construction
                doubled[at(1, 0)] = base + 3; // constant: Bayer (1973), the recursive construction
                doubled[at(1, 1)] = base + 1;
            }
        }
        ranks = doubled;
        side *= 2;
    }
    ranks
}

/// How much a point at (`row`, `column`) crowds every spot of a tile, added (`sign` 1) to or taken (`sign` -1) from
/// `crowded`, from the doubled Gaussian `crowd`, so that a window of the tile's size places it anywhere, wrapping.
fn crowd_by(crowded: &mut [f64], crowd: &[f64], (row, column): (usize, usize), sign: f64) {
    let size = BLUE_SIDE;
    for (line, spots) in crowded.chunks_exact_mut(size).enumerate() {
        let from = (size - row + line) * 2 * size + size - column;
        for (spot, near) in spots.iter_mut().zip(&crowd[from..from + size]) {
            *spot += sign * near;
        }
    }
}

/// The spot of `crowded` among those `counts` that is most crowded (the first of equals).
fn tightest(crowded: &[f64], counts: impl Fn(usize) -> bool) -> usize {
    let mut best = (f64::NEG_INFINITY, 0);
    for (at, &value) in crowded.iter().enumerate() {
        if counts(at) && value > best.0 {
            best = (value, at);
        }
    }
    best.1
}

/// The spot of `crowded` among those `counts` that is least crowded (the first of equals).
fn largest_void(crowded: &[f64], counts: impl Fn(usize) -> bool) -> usize {
    let mut best = (f64::INFINITY, 0);
    for (at, &value) in crowded.iter().enumerate() {
        if counts(at) && value < best.0 {
            best = (value, at);
        }
    }
    best.1
}

/// The spot's (row, column).
const fn at(spot: usize) -> (usize, usize) {
    (spot / BLUE_SIDE, spot % BLUE_SIDE)
}

/// Ranks 0..4095 of the blue-noise tile by void-and-cluster (Ulichney, 1993): the points of every prefix of the ranking
/// are spread as evenly as the tile allows, wrapping around its edges.
fn blue_noise() -> Vec<u16> {
    let size = BLUE_SIDE;
    let total = size * size;
    let wrapped = |index: usize| index.min(size - index);
    // The Gaussian crowding of a point on every spot, twice each way, so that a window of the tile's size onto it places
    // the Gaussian anywhere, wrapping.
    let mut crowd = vec![0.0; 4 * total]; // constant: two copies each way, so four tiles
    for row in 0..2 * size {
        for column in 0..2 * size {
            let (down, right) = (wrapped(row % size), wrapped(column % size));
            #[expect(clippy::cast_precision_loss, reason = "a distance of at most 32 px")]
            let squares = (down * down + right * right) as f64;
            crowd[row * 2 * size + column] = (-squares / (2.0 * BLUE_SIGMA * BLUE_SIGMA)).exp();
        }
    }
    let mut points = vec![false; total];
    for spot in Rng::seeded(BLUE_SEED).sample(total, total / BLUE_START) {
        points[spot] = true;
    }
    let mut crowded = vec![0.0; total];
    for spot in (0..total).filter(|&spot| points[spot]) {
        crowd_by(&mut crowded, &crowd, at(spot), 1.0);
    }
    // move the tightest cluster's point to the largest void until that is where it already is
    loop {
        let cluster = tightest(&crowded, |spot| points[spot]);
        points[cluster] = false;
        crowd_by(&mut crowded, &crowd, at(cluster), -1.0);
        let void = largest_void(&crowded, |spot| !points[spot]);
        points[void] = true;
        crowd_by(&mut crowded, &crowd, at(void), 1.0);
        if void == cluster {
            break;
        }
    }
    let mut ranks = vec![0_u16; total];
    let ones = points.iter().filter(|&&point| point).count();
    let (start_points, start_crowded) = (points.clone(), crowded.clone());
    #[expect(clippy::cast_possible_truncation, reason = "a rank is below 4096")]
    let rank_of = |rank: usize| rank as u16;
    // take the points away, tightest cluster first, counting down
    for rank in (0..ones).rev() {
        let cluster = tightest(&crowded, |spot| points[spot]);
        ranks[cluster] = rank_of(rank);
        points[cluster] = false;
        crowd_by(&mut crowded, &crowd, at(cluster), -1.0);
    }
    points = start_points;
    crowded = start_crowded;
    // add points into the largest voids, counting up to half the tile
    for rank in ones..total / 2 {
        let void = largest_void(&crowded, |spot| !points[spot]);
        ranks[void] = rank_of(rank);
        points[void] = true;
        crowd_by(&mut crowded, &crowd, at(void), 1.0);
    }
    // for the rest the empty spots are the points: the tightest cluster of them is next
    crowded.fill(0.0);
    for spot in (0..total).filter(|&spot| !points[spot]) {
        crowd_by(&mut crowded, &crowd, at(spot), 1.0);
    }
    for rank in total / 2..total {
        let cluster = tightest(&crowded, |spot| !points[spot]);
        ranks[cluster] = rank_of(rank);
        points[cluster] = true;
        crowd_by(&mut crowded, &crowd, at(cluster), -1.0);
    }
    ranks
}

/// Most candidate colours an ordered dither mixes a pixel from, as many as the common form of Knoll's algorithm uses; more
/// would resolve the tile's levels finer and cost more.
const CANDIDATES: usize = 16; // constant: the common form of Knoll's mixing uses 16 candidates

/// The palette colour chosen for each pixel of `values` (`height` x `width`, row after row) by ordered dithering: indices
/// into `palette`, for any palette (not only one that runs from dark to light).
///
/// `values` and `palette` are in a space where distance is the Euclidean one (Oklab or RGB) and whose first channel orders
/// the candidates (lightness, or red). Each pixel first gets a mix that averages to its value: candidate colours picked
/// one after another, each the palette colour closest to the pixel's value plus the error of the candidates so far (Knoll's
/// mixing; the error is what the colours chosen for it still owe it). There are n candidates, n = 1 + round(15 `amount`)
/// with `amount` from 0 to 1: 16 at the full pattern, fewer for a weaker one, and at `amount` 0 only the first, the palette
/// colour nearest to the value. The candidates are put in order of the first channel (ties by palette order) and the
/// `thresholds` (a repeating tile, or a rank for every pixel) pick one of them: a pixel with level T takes the
/// candidate at position floor(n T), the level of a rank being (rank + 0.5) / count.
pub(crate) fn ordered(
    values: &[Point],
    (height, width): (usize, usize),
    palette: &[Point],
    thresholds: &Thresholds<'_>,
    amount: f32,
) -> Vec<u8> {
    let count = thresholds.count();
    #[expect(clippy::cast_precision_loss, reason = "a small constant")]
    let extra = (CANDIDATES - 1) as f64;
    let steps = 1 + whole(f64::from(amount) * extra);
    // palette indices, darkest first (ties in palette order), and the place of each in that order
    let mut darkest_first: Vec<u8> = (0..=u8::MAX).take(palette.len()).collect();
    darkest_first.sort_by(|&a, &b| palette[usize::from(a)][0].total_cmp(&palette[usize::from(b)][0]));
    let mut place = vec![0_u8; palette.len()];
    for (at, &index) in darkest_first.iter().enumerate() {
        place[usize::from(index)] = u8::try_from(at).unwrap_or(u8::MAX);
    }
    let mut chosen = vec![0; height * width];
    par::rows(&mut chosen, width, |first_row, band| {
        let mut mix = [0_u8; CANDIDATES];
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first_row + offset;
            for (column, pick) in line.iter_mut().enumerate() {
                let goal = values[row * width + column];
                let mut owed = [0.0_f32; 3];
                for candidate in &mut mix[..steps] {
                    let probe = [0, 1, 2].map(|channel| goal[channel] + owed[channel]);
                    let index = nearest_one(&probe, palette);
                    *candidate = place[usize::from(index)];
                    let colour = palette[usize::from(index)];
                    for channel in 0..3 {
                        owed[channel] += goal[channel] - colour[channel];
                    }
                }
                mix[..steps].sort_unstable();
                // floor(steps x (rank + 0.5) / count), worked in whole numbers
                let rank = thresholds.rank((row, column), width);
                let position = (2 * rank + 1) * steps / (2 * count);
                *pick = darkest_first[usize::from(mix[position])];
            }
        }
    });
    chosen
}

#[cfg(test)]
mod tests {
    //! Inline: the kernel tables are private, and no public route reads their weights.
    use super::*;

    #[test]
    fn a_kernels_weights_sum_to_its_divisor_and_every_tap_lies_ahead_inside_the_padding() {
        let kernels = [
            (&FLOYD_STEINBERG, "floyd_steinberg"),
            (&JARVIS_JUDICE_NINKE, "jarvis_judice_ninke"),
            (&STUCKI, "stucki"),
            (&BURKES, "burkes"),
            (&SIERRA, "sierra"),
            (&SIERRA_TWO_ROW, "sierra_two_row"),
            (&SIERRA_LITE, "sierra_lite"),
            (&STEVENSON_ARCE, "stevenson_arce"),
        ];
        for (kernel, name) in kernels {
            let total: f32 = kernel.taps.iter().map(|tap| tap.2).sum();
            assert!(
                (total - kernel.divisor).abs() < f32::EPSILON,
                "{name}: {total} of {}",
                kernel.divisor
            );
        }
        // Atkinson's stated exception: six eighths
        assert!((ATKINSON.taps.iter().map(|tap| tap.2).sum::<f32>() - 6.0).abs() < f32::EPSILON);
        for kernel in kernels.map(|(kernel, _)| kernel).into_iter().chain([&ATKINSON]) {
            for &(right, down, _) in kernel.taps {
                assert!(down > 0 || (down == 0 && right > 0), "a tap behind the pixel");
                assert!(right.unsigned_abs() <= REACH && down < ROWS_ALIVE);
            }
        }
    }
}

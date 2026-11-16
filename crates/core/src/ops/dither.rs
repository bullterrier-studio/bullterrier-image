//! Dithering: choosing a palette colour for every pixel so that, seen from afar, the pixels average to the picture.
//!
//! Error diffusion ([`diffuse`]) passes each pixel's error on to the pixels after it; ordered dithering ([`ordered`])
//! shifts each pixel by a repeating pattern ([`Tile`]) before choosing. Both return the index of the chosen palette colour
//! per pixel (a palette has at most 256 colours); the caller paints them.

use std::borrow::Cow;
use std::sync::OnceLock;

use crate::ops::attributes::Choices;
use crate::ops::blue_noise;
use crate::ops::cluster::Point;
use crate::ops::color::luminance;
use crate::ops::inks::MOST_INKS;
use crate::ops::length::whole;
use crate::ops::par;
use crate::ops::random::mix;
use crate::{Error, Inputs};

/// Where an error-diffusion kernel sends a pixel's error: (columns right, rows down, weight) for the pixels it reaches,
/// and the weights' divisor. Every tap lies after the pixel in reading order.
pub(crate) struct Kernel {
    taps: &'static [(isize, usize, f32)],
    divisor: f32,
    /// For a kernel whose weights depend on the pixel's level: the weights of each tap for levels 0 to 127, replacing the
    /// taps' own weights, with their sum as the divisor.
    levels: Option<&'static [[u16; 3]; 128]>, // constant: Ostromoukhov's table has 128 rows, levels 0 to 127
}

/// Floyd and Steinberg (1976): 7/16 right, 3/16 below left, 5/16 below, 1/16 below right.
pub(crate) const FLOYD_STEINBERG: Kernel = Kernel {
    taps: &[(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)], // constant: Floyd and Steinberg (1976)
    levels: None,
    divisor: 16.0, // constant: Floyd and Steinberg (1976)
};

/// Atkinson (Apple MacPaint): 1/8 to each of six pixels, so a quarter of the error is dropped.
pub(crate) const ATKINSON: Kernel = Kernel {
    // constant: Atkinson's published error weights, the taps and the divisor
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
    // constant: Jarvis, Judice and Ninke (1976), the taps and the divisor
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
    // constant: Stucki (1981), the taps and the divisor
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
    // constant: Burkes (1988), the taps and the divisor
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
    // constant: Sierra (1989), the taps and the divisor
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
    // constant: Sierra (1989), two-row, the taps and the divisor
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
    // constant: Stevenson and Arce (1985), the taps and the divisor
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
    if level > 127 { 255 - level } else { level } // constant: Ostromoukhov's table is turned about level 127
}

/// The palette colour nearest to `value` for the dot `at` (an index) and what is left over: `value` less that colour.
fn settle(value: Point, palette: &Choices<'_>, at: (usize, usize)) -> (u8, [f32; 3]) {
    let pick = palette.nearest(&value, at);
    let colour = palette[usize::from(pick)];
    (pick, [0, 1, 2].map(|channel| value[channel] - colour[channel]))
}

/// The level of `value` (before folding): its channels weighted by `lightness`.
fn level_of(value: [f32; 3], lightness: [f32; 3]) -> f32 {
    value
        .iter()
        .zip(lightness)
        .map(|(channel, weight)| channel * weight)
        .sum()
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
    palette: &Choices<'_>,
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
        let backwards = serpentine && row % 2 == 1;
        for step in 0..width {
            let column = if backwards { width - 1 - step } else { step };
            let at = (row % ROWS_ALIVE) * stride + REACH + column;
            let pixel = values[row * width + column];
            let value = [0, 1, 2].map(|channel| pixel[channel] + waiting[at][channel]);
            let (pick, miss) = settle(value, palette, (row, column));
            chosen[row * width + column] = pick;
            let level = kernel
                .levels
                .map(|table| table[folded_level(level_of(value, lightness))].map(f32::from));
            let scale = level.map_or(fixed_scale, |row| amount / row.iter().sum::<f32>());
            let error = miss.map(|part| part * scale);
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

/// The class of each cell of Knuth's 8 x 8 matrix, row after row: the pixels of class 0 are decided first.
// constant: Knuth, "Digital halftones by dot diffusion", ACM Transactions on Graphics 6 (1987), the class matrix as printed in
// US patent 8,077,355, Table 2 (https://patents.google.com/patent/US8077355B2/en)
const KNUTH_CLASSES: [u8; 64] = [
    34, 48, 40, 32, 29, 15, 23, 31, 42, 58, 56, 53, 21, 5, 7, 10, 50, 62, 61, 45, 13, 1, 2, 18, 38, 46, 54, 37, 25, 17,
    9, 26, 28, 14, 22, 30, 35, 49, 41, 33, 20, 4, 6, 11, 43, 59, 57, 52, 12, 0, 3, 19, 51, 63, 60, 44, 24, 16, 8, 27,
    39, 47, 55, 36,
];
/// The number of classes.
const CLASS_COUNT: u8 = 64; // constant: Knuth's class matrix has 64 classes
/// The class matrix's side.
const CLASS_SIDE: usize = 8; // constant: Knuth's class matrix is 8 x 8

/// The palette colour chosen for each pixel of `values` (`height` x `width`, row after row) by dot diffusion (Knuth, 1987).
///
/// The class matrix is tiled over the picture from its top left corner and the pixels are taken class by class, 0 first;
/// each gets the palette colour closest to its value plus the error passed to it, and `amount` (0 to 1) of the difference
/// is divided among its neighbours (of the eight around it, those inside the picture) of a higher class, in proportion to
/// 2 for a side-on neighbour and 1 for a diagonal one. A pixel with no such neighbour loses its error. The pixels of one
/// class are never neighbours, so their order does not matter.
pub(crate) fn dot_diffusion(
    values: &[Point],
    (height, width): (usize, usize),
    palette: &Choices<'_>,
    amount: f32,
) -> Vec<u8> {
    let class_at = |row: usize, column: usize| KNUTH_CLASSES[(row % CLASS_SIDE) * CLASS_SIDE + column % CLASS_SIDE];
    let mut waiting = vec![[0.0_f32; 3]; height * width];
    let mut chosen = vec![0; height * width];
    for class in 0..CLASS_COUNT {
        let at = KNUTH_CLASSES.iter().position(|&each| each == class).unwrap_or_default();
        let (first_row, first_column) = (at / CLASS_SIDE, at % CLASS_SIDE);
        for row in (first_row..height).step_by(CLASS_SIDE) {
            for column in (first_column..width).step_by(CLASS_SIDE) {
                let pixel = values[row * width + column];
                let value = [0, 1, 2].map(|channel| pixel[channel] + waiting[row * width + column][channel]);
                let (pick, miss) = settle(value, palette, (row, column));
                chosen[row * width + column] = pick;
                // the higher-class neighbours inside the picture, with their weights
                let mut heirs = [(0_usize, 0_usize, 0.0_f32); 8]; // constant: structural, a pixel has eight neighbours, not tunable
                let mut count = 0; // constant: structural, a counter that starts empty, not tunable
                for (down, right) in [
                    (-1, -1),
                    (-1, 0),
                    (-1, 1),
                    (0, -1),
                    (0, 1),
                    (1, -1),
                    (1, 0),
                    (1, 1_isize),
                ] {
                    let (Some(near_row), Some(near_column)) =
                        (row.checked_add_signed(down), column.checked_add_signed(right))
                    else {
                        continue;
                    };
                    if near_row < height && near_column < width && class_at(near_row, near_column) > class {
                        let weight = if down == 0 || right == 0 { 2.0 } else { 1.0 }; // constant: Knuth, side-on 2, diagonal 1
                        heirs[count] = (near_row, near_column, weight);
                        count += 1;
                    }
                }
                let total: f32 = heirs[..count].iter().map(|heir| heir.2).sum();
                for &(near_row, near_column, weight) in &heirs[..count] {
                    for (sum, part) in waiting[near_row * width + near_column].iter_mut().zip(miss) {
                        *sum += part * amount * weight / total;
                    }
                }
            }
        }
    }
    chosen
}

/// How many errors Riemersma's dither remembers.
const MEMORY: usize = 16; // constant: Riemersma (1998), "a list of 16 as a minimum", his program's SIZE
/// The weight of the newest error in memory against the oldest.
const MEMORY_RATIO: f32 = 16.0; // constant: Riemersma (1998), his program's MAX

/// Calls `visit(row, column)` for every cell of a `height` x `width` rectangle once, along the generalized Hilbert curve
/// ("gilbert"): each step goes to a neighbouring cell (a diagonal one at some odd sizes), in O(`height` x `width`) steps,
/// and on a power-of-two square it is the Hilbert curve (first down, then right, from the top left corner).
///
/// Reimplemented from Jakub Cervený's `gilbert2d` (BSD-2-Clause, <https://github.com/jakubcerveny/gilbert>): the algorithm
/// only, no code or dependency taken. It splits the rectangle in two or three and recurses, to a depth of log2 of the
/// longer side.
fn gilbert(height: usize, width: usize, visit: &mut impl FnMut(usize, usize)) {
    type Vector = (isize, isize); // (column, row)
    // `origin` is the corner, `along` the long edge and `across` the other.
    fn walk(origin: Vector, along: Vector, across: Vector, visit: &mut impl FnMut(usize, usize)) {
        let (x, y) = origin;
        let (long, short) = (along.0.abs() + along.1.abs(), across.0.abs() + across.1.abs());
        let (along_unit, across_unit) = (
            (along.0.signum(), along.1.signum()),
            (across.0.signum(), across.1.signum()),
        );
        if short == 1 || long == 1 {
            let (unit, count) = if short == 1 {
                (along_unit, long)
            } else {
                (across_unit, short)
            };
            for step in 0..count {
                visit((y + step * unit.1).unsigned_abs(), (x + step * unit.0).unsigned_abs());
            }
            return;
        }
        // floor halves, as the original's `//`
        let mut half_along = (along.0.div_euclid(2), along.1.div_euclid(2));
        let mut half_across = (across.0.div_euclid(2), across.1.div_euclid(2));
        let (long_half, short_half) = (
            half_along.0.abs() + half_along.1.abs(),
            half_across.0.abs() + half_across.1.abs(),
        );
        let wide = 2 * long > 3 * short; // constant: gilbert2d (Cerveny): split the long side when over 1.5 x the short
        if wide {
            if long_half % 2 == 1 && long > 2 {
                half_along = (half_along.0 + along_unit.0, half_along.1 + along_unit.1);
            }
            walk(origin, half_along, across, visit);
            walk(
                (x + half_along.0, y + half_along.1),
                (along.0 - half_along.0, along.1 - half_along.1),
                across,
                visit,
            );
        } else {
            if short_half % 2 == 1 && short > 2 {
                half_across = (half_across.0 + across_unit.0, half_across.1 + across_unit.1);
            }
            walk(origin, half_across, half_along, visit);
            walk(
                (x + half_across.0, y + half_across.1),
                along,
                (across.0 - half_across.0, across.1 - half_across.1),
                visit,
            );
            walk(
                (
                    x + (along.0 - along_unit.0) + (half_across.0 - across_unit.0),
                    y + (along.1 - along_unit.1) + (half_across.1 - across_unit.1),
                ),
                (-half_across.0, -half_across.1),
                (-(along.0 - half_along.0), -(along.1 - half_along.1)),
                visit,
            );
        }
    }
    let (width, height) = (
        isize::try_from(width).unwrap_or_default(),
        isize::try_from(height).unwrap_or_default(),
    );
    if width >= height {
        walk((0, 0), (width, 0), (0, height), visit);
    } else {
        walk((0, 0), (0, height), (width, 0), visit);
    }
}

/// The palette colour chosen for each pixel of `values` (`height` x `width`, row after row) by Riemersma's dither (1998).
///
/// The pixels are taken along the generalized Hilbert curve over the picture (`gilbert`). A pixel gets the palette colour closest to its value plus `amount` (0 to 1) of the
/// weighted sum of the last 16 errors, the newest weighing 1 and each older one less by the same factor, down to 1/16; its
/// own error is its value (without the sum) less the colour chosen.
pub(crate) fn riemersma(
    values: &[Point],
    (height, width): (usize, usize),
    palette: &Choices<'_>,
    amount: f32,
) -> Vec<u8> {
    #[expect(clippy::cast_precision_loss, reason = "a small constant")]
    let fall = MEMORY_RATIO.powf(-1.0 / (MEMORY - 1) as f32);
    // the weights from the oldest error to the newest
    let weights: [f32; MEMORY] =
        std::array::from_fn(|age| fall.powi(i32::try_from(MEMORY - 1 - age).unwrap_or_default()));
    let mut recent = [[0.0_f32; 3]; MEMORY]; // the oldest first
    let mut chosen = vec![0; height * width];
    gilbert(height, width, &mut |row, column| {
        let pixel = values[row * width + column];
        let value = [0, 1, 2].map(|channel| {
            let sum: f32 = recent
                .iter()
                .zip(&weights)
                .map(|(error, weight)| error[channel] * weight)
                .sum();
            pixel[channel] + amount * sum
        });
        let pick = palette.nearest(&value, (row, column));
        chosen[row * width + column] = pick;
        recent.rotate_left(1);
        let colour = palette[usize::from(pick)];
        recent[MEMORY - 1] = [0, 1, 2].map(|channel| pixel[channel] - colour[channel]);
    });
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
// constant: Go dither library ClusteredDotDiagonal8x8, https://github.com/makeworld-the-better-one/dither/blob/master/ordered_ditherers.go and http://caca.zoy.org/study/part2.html
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

/// Where a pixel's rank comes from.
enum Source<'a> {
    /// A tile of `rows` x `columns` ranks, row after row, repeating from the picture's top left corner.
    Tiled {
        ranks: Cow<'a, [u16]>,
        rows: usize,
        columns: usize,
        count: usize,
    },
    /// A rank drawn for each pixel from `seed` and the pixel's row and column, which wrap at the picture's `size`
    /// (`height`, `width`).
    Noise { seed: u32, size: (usize, usize) },
}

/// The thresholds of an ordered dither: a rank for every pixel, out of `count` ranks, the highest lit first, the pattern
/// moved right and down by `shift` (rows, columns) dots.
pub(crate) struct Thresholds<'a> {
    source: Source<'a>,
    shift: (usize, usize),
}

/// The ranks of white noise: a pixel's rank is one of this many levels.
const NOISE_LEVELS: usize = 1 << 16; // constant: a rank is 16 bits

impl Thresholds<'_> {
    fn count(&self) -> usize {
        match &self.source {
            Source::Tiled { count, .. } => *count,
            Source::Noise { .. } => NOISE_LEVELS,
        }
    }

    /// The pattern moved right and down by `phase` (0 to 1) of its side, rounded down to whole dots: of the tile, or, for
    /// noise, which has none, of the picture. A shift of a whole side is none. The one place a pattern is shifted, for every method that
    /// reads thresholds.
    pub(crate) fn phased(mut self, phase: f32) -> Self {
        let (rows, columns) = match &self.source {
            Source::Tiled { rows, columns, .. } => (*rows, *columns),
            Source::Noise { size, .. } => *size,
        };
        let by = |side: usize| whole((f64::from(phase) * crate::ops::length::float(side)).floor());
        self.shift = (by(rows), by(columns));
        self
    }

    /// The pixel's level in 0..1, (rank + 0.5) / count: each rank once per tile, so a share s of the levels is below s.
    fn level(&self, at: (usize, usize)) -> f64 {
        let (twice, whole) = (2 * self.rank(at) + 1, 2 * self.count());
        f64::from(u32::try_from(twice).unwrap_or(u32::MAX)) / f64::from(u32::try_from(whole).unwrap_or(u32::MAX))
    }

    fn rank(&self, (row, column): (usize, usize)) -> usize {
        // the pattern moved by `shift`: the rank here is the one that many dots up and to the left, round the edges
        let back = |at: usize, shift: usize, side: usize| (at + side - shift) % side;
        match &self.source {
            Source::Tiled {
                ranks, rows, columns, ..
            } => usize::from(ranks[back(row, self.shift.0, *rows) * columns + back(column, self.shift.1, *columns)]),
            Source::Noise { seed, size } => {
                let place = |at: usize| u64::try_from(at).unwrap_or_default();
                // constant: a rank is the top 16 bits of the 64-bit hash of the seed and the pixel's row and column
                usize::try_from(
                    mix(
                        *seed,
                        &[
                            place(back(row, self.shift.0, size.0)),
                            place(back(column, self.shift.1, size.1)),
                        ],
                    ) >> 48,
                )
                .unwrap_or_default()
            }
        }
    }
}

/// The most pixels along a side of a caller's `matrix`.
const MATRIX_SIDE: usize = 256; // constant: 256 x 256 cells are 65536 ranks, the most a 16-bit rank holds

/// What `effect` refuses of the picture `matrix` that `needs` (a setting of it): none given, or one more than 256 pixels
/// wide or high.
///
/// # Errors
/// [`Error::Recipe`] naming `matrix`.
pub(crate) fn check_matrix(effect: &str, needs: &str, inputs: &Inputs<'_>) -> Result<(), Error> {
    let Some(image) = inputs.image("matrix") else {
        return Err(Error::Recipe(format!(
            "{effect}: `matrix` is needed with {needs}: a picture name or path"
        )));
    };
    if image.height() > MATRIX_SIDE || image.width() > MATRIX_SIDE {
        return Err(Error::Recipe(format!(
            "{effect}: `matrix` is {} x {} pixels, at most {MATRIX_SIDE} x {MATRIX_SIDE}",
            image.width(),
            image.height()
        )));
    }
    Ok(())
}

/// The grey values of the caller's picture `matrix` (its luminance, row after row) and its (rows, columns), which
/// [`check_matrix`] has seen; one 0 where there is none.
pub(crate) fn matrix_greys(inputs: &Inputs<'_>) -> (Vec<f32>, (usize, usize)) {
    inputs.image("matrix").map_or_else(
        || (vec![0.0], (1, 1)),
        |image| {
            let greys = image
                .as_picture()
                .pixels()
                .iter()
                .map(|&pixel| luminance(pixel))
                .collect();
            (greys, (image.height(), image.width()))
        },
    )
}

/// The rank of each of `greys`: the brightest the highest, so lit first, of equal ones the earlier in reading order higher.
pub(crate) fn rank_greys(greys: &[f32]) -> Vec<u16> {
    let mut order: Vec<usize> = (0..greys.len()).collect();
    order.sort_by(|&a, &b| greys[b].total_cmp(&greys[a]));
    let mut ranks = vec![0_u16; greys.len()];
    for (place, &cell) in order.iter().enumerate() {
        ranks[cell] = u16::try_from(greys.len() - 1 - place).unwrap_or(u16::MAX);
    }
    ranks
}

/// A caller's matrix as thresholds: `greys` (`rows` x `columns`, row after row) ranked by [`rank_greys`].
pub(crate) fn from_greys(greys: &[f32], (rows, columns): (usize, usize)) -> Thresholds<'static> {
    Thresholds {
        source: Source::Tiled {
            ranks: Cow::Owned(rank_greys(greys)),
            rows,
            columns,
            count: greys.len(),
        },
        shift: (0, 0),
    }
}

/// White noise from `seed` over a picture of `size` (`height`, `width`) dots.
pub(crate) const fn noise(seed: u32, size: (usize, usize)) -> Thresholds<'static> {
    Thresholds {
        source: Source::Noise { seed, size },
        shift: (0, 0),
    }
}

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
            Self::BlueNoise => (
                Cow::Borrowed(BLUE.get_or_init(blue_noise::tile).as_slice()),
                blue_noise::SIDE,
            ),
            Self::GameboyCamera => (Cow::Borrowed(&GAMEBOY_CAMERA), 4), // constant: a 4 x 4 matrix
        };
        Thresholds {
            source: Source::Tiled {
                count: side * side,
                ranks,
                rows: side,
                columns: side,
            },
            shift: (0, 0),
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
    palette: &Choices<'_>,
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
                    let index = palette.nearest(&probe, (row, column));
                    *candidate = place[usize::from(index)];
                    let colour = palette[usize::from(index)];
                    for channel in 0..3 {
                        owed[channel] += goal[channel] - colour[channel];
                    }
                }
                mix[..steps].sort_unstable();
                // floor(steps x (rank + 0.5) / count), worked in whole numbers
                let rank = thresholds.rank((row, column));
                let position = (2 * rank + 1) * steps / (2 * count);
                *pick = darkest_first[usize::from(mix[position])];
            }
        }
    });
    chosen
}

/// A pattern of overprints with their exact shares. `amounts` holds each pixel's coverage of its first `inks` inks; the
/// inks overlap independently, so the overprint of the set S (bit k of the index is ink k) covers
/// `prod_(k in S) a_k prod_(k not in S) (1 - a_k)` of the pixel. The pixel's own level (its rank in the `thresholds`, or its
/// noise) picks the overprint whose cumulative share holds it, in index order: exact in expectation for noise and per tile
/// for a matrix, where `ordered` mixes in sixteenths. With `pattern` below 1 the level takes only `1 + round(15 pattern)`
/// steps, as `ordered`'s candidates do, so the dot is flatter; at 0 it is the middle one.
pub(crate) fn shared(
    amounts: &[[f32; MOST_INKS]],
    inks: usize,
    (height, width): (usize, usize),
    thresholds: &Thresholds<'_>,
    pattern: f32,
) -> Vec<u8> {
    #[expect(clippy::cast_precision_loss, reason = "a small constant")]
    let steps = 1 + whole(f64::from(pattern) * (CANDIDATES - 1) as f64);
    let steps_f = f64::from(u32::try_from(steps).unwrap_or(u32::MAX));
    let mut chosen = vec![0; height * width];
    par::rows(&mut chosen, width, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first_row + offset;
            for (column, pick) in line.iter_mut().enumerate() {
                let level = thresholds.level((row, column));
                let level = if steps < CANDIDATES {
                    ((level * steps_f).floor() + 0.5) / steps_f
                } else {
                    level
                };
                let a = &amounts[row * width + column];
                let (mut below, mut last) = (0.0, 0);
                for mask in 0..1_usize << inks {
                    let share: f64 = (0..inks)
                        .map(|k| f64::from(if mask >> k & 1 == 1 { a[k] } else { 1.0 - a[k] }))
                        .product();
                    below += share;
                    last = mask;
                    if level < below {
                        break;
                    }
                }
                *pick = u8::try_from(last).unwrap_or(u8::MAX);
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

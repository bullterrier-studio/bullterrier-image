//! JPEG's lossy core written as maths on a float picture: no file, no bytes, no entropy coding.
//!
//! [`encode`] takes a picture to what a JPEG stores before it is packed into bytes, the quantised DCT coefficients of JFIF
//! YCbCr (ITU-T T.81, T.871); [`decode`] takes such coefficients back to a picture. Damage done to the coefficients between
//! the two is what a damaged JPEG shows.
//!
//! The steps, as the standard has them: sRGB to JFIF YCbCr; the two chroma planes halved in each direction by averaging 2 x 2
//! patches (4:2:0); every plane shifted by 128 and cut into 8 x 8 blocks; each block through the orthonormal 2-D DCT; each
//! coefficient divided by its quantisation step and rounded. A picture whose sides do not fit is first extended to a multiple
//! of 16 by repeating its last row and column, and cropped back by [`decode`].
//!
//! The work is done one MCU (a 16 x 16 patch: four luma blocks and one block of each chroma plane) at a time, so no plane
//! is ever held whole; a row of MCUs is the unit the threads share.

use std::f64::consts::PI;

use super::colorspace::{rgb_to_ycbcr, ycbcr_to_rgb};
use super::length::float;
use super::par;

/// Samples along a block's side.
const BLOCK: usize = 8; // constant: JPEG transforms blocks of 8 x 8 samples (ITU-T T.81)
/// Coefficients in a block.
const AREA: usize = BLOCK * BLOCK;
/// Pixels along an MCU's side: the chroma planes hold one sample for every 2 x 2 luma samples (4:2:0, ITU-T T.871).
const SIDE: usize = 2 * BLOCK;
/// Blocks in an MCU: four of luma, one of Cb, one of Cr.
const BLOCKS: usize = 6; // constant: 4:2:0 has four luma blocks to each pair of chroma blocks
/// Coefficients in an MCU.
const MCU: usize = BLOCKS * AREA;
const LEVEL: f32 = 128.0; // constant: samples are shifted by half the 8-bit range before the DCT (ITU-T T.81)
const LEVELS: f32 = 255.0; // constant: the 8-bit range the standard's tables and quantiser are written for

/// The example quantisation table of ITU-T T.81 Annex K, table K.1 (luma).
#[rustfmt::skip]
const LUMA_TABLE: [u32; AREA] = [
    16, 11, 10, 16, 24, 40, 51, 61,
    12, 12, 14, 19, 26, 58, 60, 55,
    14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62,
    18, 22, 37, 56, 68, 109, 103, 77,
    24, 35, 55, 64, 81, 104, 113, 92,
    49, 64, 78, 87, 103, 121, 120, 101,
    72, 92, 95, 98, 112, 100, 103, 99,
];

/// The example quantisation table of ITU-T T.81 Annex K, table K.2 (chroma).
#[rustfmt::skip]
const CHROMA_TABLE: [u32; AREA] = [
    17, 18, 24, 47, 99, 99, 99, 99,
    18, 21, 26, 66, 99, 99, 99, 99,
    24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
];

/// One of the three planes a JPEG stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plane {
    /// Brightness (Y).
    Luma,
    /// The blue difference (Cb), a quarter of the luma plane's blocks.
    Blue,
    /// The red difference (Cr), a quarter of the luma plane's blocks.
    Red,
}

/// The quantised coefficients of a picture, as a JPEG stores them before packing them into bytes.
pub(crate) struct Coefficients {
    height: usize,
    width: usize,
    /// MCU rows and columns.
    mcus: (usize, usize),
    /// The quantisation steps, 8 x 8 each, row after row: luma, then chroma.
    steps: [[f32; AREA]; 2],
    /// The blocks of each MCU in turn (luma top-left, top-right, bottom-left, bottom-right, Cb, Cr), each 64 coefficients
    /// row after row, entry 0 the block's DC value (its mean).
    values: Vec<i32>,
}

impl Coefficients {
    /// The block rows and columns of `plane`.
    pub(crate) const fn blocks(&self, plane: Plane) -> (usize, usize) {
        match plane {
            Plane::Luma => (2 * self.mcus.0, 2 * self.mcus.1),
            Plane::Blue | Plane::Red => self.mcus,
        }
    }

    /// Coefficient `index` (0 the DC value, then row after row) of the block at (`row`, `column`) of `plane`.
    pub(crate) fn at(&mut self, plane: Plane, row: usize, column: usize, index: usize) -> &mut i32 {
        let (mcu, slot) = match plane {
            Plane::Luma => ((row / 2) * self.mcus.1 + column / 2, (row % 2) * 2 + column % 2),
            Plane::Blue => (row * self.mcus.1 + column, 4), // constant: Cb is the fifth block of an MCU
            Plane::Red => (row * self.mcus.1 + column, 5),  // constant: Cr is the sixth block of an MCU
        };
        &mut self.values[(mcu * BLOCKS + slot) * AREA + index]
    }
}

/// The orthonormal 1-D DCT-II matrix of size `BLOCK` (ITU-T T.81 A.3.3): row k holds the k-th cosine.
#[expect(clippy::cast_possible_truncation, reason = "matrix entries are stored as f32")]
fn dct_matrix() -> [[f32; BLOCK]; BLOCK] {
    std::array::from_fn(|k| {
        // the first row, the mean, is scaled by 1 / sqrt(2) to stay orthonormal
        let scale = (if k == 0 { 1.0 } else { 2.0 } / float(BLOCK)).sqrt();
        std::array::from_fn(|n| (scale * (PI * float(k) * (2.0 * float(n) + 1.0) / (2.0 * float(BLOCK))).cos()) as f32)
    })
}

/// The 2-D DCT of `block` (row after row): `D block Dᵀ`.
fn forward(block: &[f32; AREA], d: &[[f32; BLOCK]; BLOCK]) -> [f32; AREA] {
    let mut rows = [0.0; AREA];
    for k in 0..BLOCK {
        for n in 0..BLOCK {
            rows[k * BLOCK + n] = (0..BLOCK).map(|m| d[k][m] * block[m * BLOCK + n]).sum();
        }
    }
    std::array::from_fn(|at| {
        let (k, l) = (at / BLOCK, at % BLOCK);
        (0..BLOCK).map(|n| rows[k * BLOCK + n] * d[l][n]).sum()
    })
}

/// The inverse 2-D DCT of `coefficients`: `Dᵀ coefficients D`.
fn inverse(coefficients: &[f32; AREA], d: &[[f32; BLOCK]; BLOCK]) -> [f32; AREA] {
    let mut columns = [0.0; AREA];
    for m in 0..BLOCK {
        for l in 0..BLOCK {
            columns[m * BLOCK + l] = (0..BLOCK).map(|k| d[k][m] * coefficients[k * BLOCK + l]).sum();
        }
    }
    std::array::from_fn(|at| {
        let (m, n) = (at / BLOCK, at % BLOCK);
        (0..BLOCK).map(|l| columns[m * BLOCK + l] * d[l][n]).sum()
    })
}

/// The quantisation steps of `table` at JPEG `quality` 1 to 100: the Independent JPEG Group's rule scales the table by 5000 /
/// quality below 50 and 200 - 2 quality from 50 up; each step is (table x scale + 50) / 100 rounded down, held to 1..255.
#[expect(clippy::cast_precision_loss, reason = "steps are at most 255")]
fn steps(table: &[u32; AREA], quality: u32) -> [f32; AREA] {
    let scale = if quality < 50 {
        5000 / quality
    } else {
        200 - 2 * quality
    }; // constant: the IJG quality rule
    table.map(|entry| ((entry * scale + 50) / 100).clamp(1, 255) as f32) // constant: the IJG rule holds a step to 1..255
}

/// A coefficient as the float it is multiplied as.
#[expect(clippy::cast_precision_loss, reason = "a coefficient is far below 2^24")]
const fn level(coefficient: i32) -> f32 {
    coefficient as f32
}

/// The quantised DCT coefficients of the `height` x `width` picture `pixels` at JPEG `quality` 1 to 100.
pub(crate) fn encode(pixels: &[[f32; 3]], height: usize, width: usize, quality: u32) -> Coefficients {
    let mcus = (height.div_ceil(SIDE), width.div_ceil(SIDE));
    let steps = [steps(&LUMA_TABLE, quality), steps(&CHROMA_TABLE, quality)];
    let d = dct_matrix();
    // `quantise(block, step)` rounds each coefficient of the DCT of `block` to a multiple of its step
    let quantise = |block: &[f32; AREA], step: &[f32; AREA], out: &mut [i32]| {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a quantised coefficient is far below 2^31"
        )]
        for ((out, coefficient), step) in out.iter_mut().zip(forward(block, &d)).zip(step) {
            *out = (coefficient / step).round_ties_even() as i32;
        }
    };
    let mut values = vec![0; mcus.0 * mcus.1 * MCU];
    par::rows(&mut values, mcus.1 * MCU, |first, band| {
        let mut patch = [[0.0_f32; 3]; SIDE * SIDE];
        for (offset, strip) in band.chunks_exact_mut(mcus.1 * MCU).enumerate() {
            for (column, mcu) in strip.as_chunks_mut::<MCU>().0.iter_mut().enumerate() {
                // the patch in samples (YCbCr shifted by 128), the last row and column repeated past the picture
                for (at, sample) in patch.iter_mut().enumerate() {
                    let row = ((first + offset) * SIDE + at / SIDE).min(height - 1);
                    let across = (column * SIDE + at % SIDE).min(width - 1);
                    *sample = rgb_to_ycbcr(pixels[row * width + across]).map(|level| level * LEVELS - LEVEL);
                }
                for slot in 0..4 {
                    let (top, left) = ((slot / 2) * BLOCK, (slot % 2) * BLOCK);
                    let block = std::array::from_fn(|at| patch[(top + at / BLOCK) * SIDE + left + at % BLOCK][0]);
                    quantise(&block, &steps[0], &mut mcu[slot * AREA..][..AREA]);
                }
                for (slot, channel) in [(4, 1), (5, 2)] {
                    // each chroma sample is the mean of a 2 x 2 patch
                    let block = std::array::from_fn(|at| {
                        let first = (2 * (at / BLOCK)) * SIDE + 2 * (at % BLOCK);
                        (patch[first][channel]
                            + patch[first + 1][channel]
                            + patch[first + SIDE][channel]
                            + patch[first + SIDE + 1][channel])
                            / 4.0 // constant: the mean of a 2 x 2 patch
                    });
                    quantise(&block, &steps[1], &mut mcu[slot * AREA..][..AREA]);
                }
            }
        }
    });
    Coefficients {
        height,
        width,
        mcus,
        steps,
        values,
    }
}

/// The picture, 0..1, that `coefficients` describe.
///
/// Each coefficient is multiplied by its step, each block goes through the inverse DCT, the chroma planes are doubled in
/// each direction by repeating samples, the planes are shifted back by 128, converted from YCbCr to sRGB and cropped to the
/// original size. The result is clamped to 0..1 here, the one place it is done.
pub(crate) fn decode(coefficients: &Coefficients) -> Vec<[f32; 3]> {
    let (height, width) = (coefficients.height, coefficients.width);
    let columns = coefficients.mcus.1;
    let d = dct_matrix();
    let samples = |mcu: &[i32], slot: usize, step: &[f32; AREA]| {
        let block: [f32; AREA] = std::array::from_fn(|at| level(mcu[slot * AREA + at]) * step[at]);
        inverse(&block, &d)
    };
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first, band| {
        let last = first + band.len() / width;
        // one row of MCUs at a time, as wide as the MCUs are
        let mut strip = vec![[0.0_f32; 3]; SIDE * columns * SIDE];
        for mcu_row in first / SIDE..=(last - 1) / SIDE {
            for column in 0..columns {
                let mcu = &coefficients.values[(mcu_row * columns + column) * MCU..][..MCU];
                let luma: [[f32; AREA]; 4] = std::array::from_fn(|slot| samples(mcu, slot, &coefficients.steps[0]));
                let blue = samples(mcu, 4, &coefficients.steps[1]);
                let red = samples(mcu, 5, &coefficients.steps[1]);
                for at in 0..SIDE * SIDE {
                    let (y, x) = (at / SIDE, at % SIDE);
                    let block = &luma[(y / BLOCK) * 2 + x / BLOCK];
                    let chroma = (y / 2) * BLOCK + x / 2; // constant: 4:2:0 repeats a chroma sample over 2 x 2 pixels
                    let rgb = ycbcr_to_rgb([
                        (block[(y % BLOCK) * BLOCK + x % BLOCK] + LEVEL) / LEVELS,
                        (blue[chroma] + LEVEL) / LEVELS,
                        (red[chroma] + LEVEL) / LEVELS,
                    ]);
                    strip[y * columns * SIDE + column * SIDE + x] = rgb.map(|level| level.clamp(0.0, 1.0));
                }
            }
            for y in 0..SIDE {
                let row = mcu_row * SIDE + y;
                if (first..last).contains(&row) {
                    band[(row - first) * width..][..width].copy_from_slice(&strip[y * columns * SIDE..][..width]);
                }
            }
        }
    });
    out
}

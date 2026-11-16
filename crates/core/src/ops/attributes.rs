//! Attribute blocks: every block of dots draws from two palette colours only, as the colour attributes of the ZX Spectrum,
//! the MSX and the C64 do. [`Blocks`] picks the pair of each block; [`Choices`] is the palette a dither picks from, which
//! gives each dot the nearest colour of its own block's pair, so error and thresholds carry across block borders.

use std::ops::Deref;

use crate::ops::cluster::{Point, nearest_one, squared};
use crate::ops::par;
use crate::ops::tiles::on_segment;

/// The most colours the pair search tries as ends: a palette of this many or fewer is searched whole, a longer one by the
/// colours that are the nearest of most dots of the block.
const SEARCHED: usize = 16; // constant: 120 pairs of 64 dots a block is under 8k distances; every platform palette (16 colours) is exact

/// The size of a block in dots and how a palette is read in runs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shape {
    /// Dots across and down.
    pub(crate) width: usize,
    pub(crate) height: usize,
    /// 0, or the length of the runs of palette colours (colour i is in run i / group); a pair lies in one run.
    pub(crate) group: usize,
}

/// The pair of palette colours of every block of a picture of dots.
pub(crate) struct Blocks {
    pairs: Vec<[u8; 2]>,
    shape: Shape,
    across: usize,
}

impl Blocks {
    /// The pair for each block of `dots` (`rows` x `columns`, in the space of `palette`): the pair of palette colours
    /// minimising the summed distance of the block's dots to the segment between them. Of equal fits the shorter segment wins (a
    /// dither between colours further apart is the coarser), then the lower indices.
    // shortcut: a palette longer than SEARCHED tries only the colours nearest to most dots as ends (a pair with an unused
    // colour as an end is missed), and a block whose candidates share no run gets one colour; upgrade if a real palette
    // shows it
    pub(crate) fn choose(dots: &[Point], (rows, columns): (usize, usize), palette: &[Point], shape: Shape) -> Self {
        let across = columns.div_ceil(shape.width);
        let mut pairs = vec![[0, 0]; rows.div_ceil(shape.height) * across];
        par::rows(&mut pairs, 1, |first, band| {
            for (offset, pair) in band.iter_mut().enumerate() {
                let at = first + offset;
                let (top, left) = (at / across * shape.height, at % across * shape.width);
                let block: Vec<Point> = (top..(top + shape.height).min(rows))
                    .flat_map(|row| {
                        dots[row * columns + left..row * columns + (left + shape.width).min(columns)].iter()
                    })
                    .copied()
                    .collect();
                *pair = best_pair(&block, palette, shape.group);
            }
        });
        Self { pairs, shape, across }
    }
}

/// The valid pair of the candidate colours with the least summed distance of `block` to its segment.
fn best_pair(block: &[Point], palette: &[Point], group: usize) -> [u8; 2] {
    let mut candidates: Vec<usize> = (0..palette.len()).collect();
    if palette.len() > SEARCHED {
        let mut votes = vec![0_usize; palette.len()];
        for dot in block {
            votes[usize::from(nearest_one(dot, palette))] += 1;
        }
        candidates.sort_by_key(|&colour| (std::cmp::Reverse(votes[colour]), colour));
        candidates.truncate(SEARCHED);
        candidates.sort_unstable();
    }
    let mut best = (f32::INFINITY, f32::INFINITY, [candidates[0], candidates[0]]);
    for (place, &first) in candidates.iter().enumerate() {
        for &second in &candidates[place + 1..] {
            if group > 0 && first / group != second / group {
                continue;
            }
            let cost: f32 = block
                .iter()
                .map(|dot| on_segment(dot, &palette[first], &palette[second]).0.sqrt())
                .sum();
            let length = squared(&palette[first], &palette[second]);
            if cost.total_cmp(&best.0).then(length.total_cmp(&best.1)).is_lt() {
                best = (cost, length, [first, second]);
            }
        }
    }
    best.2.map(|colour| u8::try_from(colour).unwrap_or(u8::MAX))
}

/// A palette a dither picks from: all of it, or for a dot only the two colours of its block.
pub(crate) struct Choices<'a> {
    colours: &'a [Point],
    blocks: Option<&'a Blocks>,
}

impl<'a> Choices<'a> {
    /// Every colour of `colours` for every dot, or with `blocks` the pair of the dot's block.
    pub(crate) const fn new(colours: &'a [Point], blocks: Option<&'a Blocks>) -> Self {
        Self { colours, blocks }
    }

    /// The index of the colour nearest to `value`, for the dot at (`row`, `column`); the lower index of a tie.
    pub(crate) fn nearest(&self, value: &Point, (row, column): (usize, usize)) -> u8 {
        let Some(blocks) = self.blocks else {
            return nearest_one(value, self.colours);
        };
        let shape = blocks.shape;
        let [first, second] = blocks.pairs[row / shape.height * blocks.across + column / shape.width];
        let away = |colour: u8| squared(value, &self.colours[usize::from(colour)]);
        if away(second) < away(first) { second } else { first }
    }
}

impl Deref for Choices<'_> {
    type Target = [Point];

    fn deref(&self) -> &[Point] {
        self.colours
    }
}

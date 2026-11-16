//! Print realism shared by `halftone` and `dither`'s spot separation: dot gain, uneven ink and paper specks.
//!
//! Every setting at 0 leaves a coverage and a pixel exactly as they were. Random numbers come from [`mix`], numbered by
//! position (ink, cell, lattice square), so a band of rows draws what the whole picture draws.

use super::cast;
use super::length::float;
use super::random::{mix, unit};

/// What the press does to each ink's coverage and to the paper.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Realism {
    /// The gain at 50 % tone, 0 to 0.25.
    pub(crate) dot_gain: f32,
    /// How far a cell's coverage may stray, as a share of the nearer of its ink and its paper, 0 to 1.
    pub(crate) ink_noise: f32,
    /// Specks per 1000 px².
    pub(crate) specks: f32,
    /// A speck's diameter in px.
    pub(crate) speck_size: f32,
    pub(crate) seed: u32,
}

const NOISE_STREAM: u64 = 1; // constant: names the draw of a cell's coverage
const SPECK_STREAM: u64 = 2; // constant: names the draws of a speck's presence and place

impl Realism {
    /// The coverage a press prints where `amount` (0..1) of `ink` is wanted in the cell `cell`: first the dot gain, a' = a + 4
    /// g a (1 - a) (the simplest curve through 0 and 1 that gains `g` at 50 % and is symmetric; presses quote dot gain at
    /// 50 %, ISO 12647-2), then the cell's ink, uneven: a' moves by up to `ink_noise` of min(a', 1 - a') either way, from
    /// a draw that is the same for the same seed. The move is symmetric and cannot leave 0..1, so the mean stays a'.
    pub(crate) fn coverage(&self, amount: f32, ink: usize, cell: [u64; 2]) -> f32 {
        if self.dot_gain == 0.0 && self.ink_noise == 0.0 {
            return amount;
        }
        let a = amount.clamp(0.0, 1.0);
        let gained = a + 4.0 * self.dot_gain * a * (1.0 - a); // constant: 4 makes the gain at a = 1/2 exactly g
        let draw = cast::single(unit(mix(self.seed, &[NOISE_STREAM, ink as u64, cell[0], cell[1]])) * 2.0 - 1.0); // constant: 0..1 to -1..1
        gained + self.ink_noise * gained.min(1.0 - gained) * draw
    }

    /// How much of the pixel at (`x`, `y`) of the canvas a speck covers, 0 to 1. Specks are discs of `speck_size` px
    /// placed one at random in a square of the lattice of that side with the chance density x size^2 / 1000, so the
    /// density is as asked (none of this changes with the picture's rows).
    pub(crate) fn speck(&self, x: usize, y: usize) -> f32 {
        if self.specks == 0.0 {
            return 0.0;
        }
        let side = f64::from(self.speck_size);
        let chance = f64::from(self.specks) * side * side / 1000.0; // constant: density counts marks per 1000 px^2
        let (px, py) = (float(x) + 0.5, float(y) + 0.5); // constant: the pixel's centre
        let (cx, cy) = ((px / side).floor(), (py / side).floor());
        let mut covered = 0.0_f64; // constant: none yet
        for (dx, dy) in (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (dx, dy))) {
            let (sx, sy) = (cx + f64::from(dx), cy + f64::from(dy));
            let at = |part: u64| mix(self.seed, &[SPECK_STREAM, sx.to_bits(), sy.to_bits(), part]);
            if unit(at(0)) >= chance {
                continue;
            }
            let (ex, ey) = ((sx + unit(at(1))) * side, (sy + unit(at(2))) * side);
            let distance = (px - ex).hypot(py - ey);
            // a ramp one pixel wide across the edge, as the halftone's dots
            covered = covered.max((side / 2.0 - distance + 0.5).clamp(0.0, 1.0)); // constant: half a pixel each side of the edge
        }
        cast::single(covered)
    }

    /// `printed`, with a speck over it of bare paper `bare` where `speck` says (see [`Realism::speck`]) and ink was.
    pub(crate) fn specked(printed: [f32; 3], bare: [f32; 3], speck: f32) -> [f32; 3] {
        if speck == 0.0 {
            return printed;
        }
        [0, 1, 2].map(|channel| printed[channel] * (1.0 - speck) + bare[channel] * speck)
    }
}

#[cfg(test)]
mod tests {
    use super::Realism;

    /// Inks in the same cell draw their own numbers: two layers of one dot do not stray together.
    #[test]
    fn two_inks_in_one_cell_stray_differently() {
        let real = Realism {
            dot_gain: 0.0,
            ink_noise: 1.0,
            specks: 0.0,
            speck_size: 3.0,
            seed: 1,
        };
        let differ = (0..100)
            .filter(|&cell| real.coverage(0.5, 0, [cell, 7]).to_bits() != real.coverage(0.5, 1, [cell, 7]).to_bits())
            .count();
        assert!(differ > 95, "{differ}");
    }
}

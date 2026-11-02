//! A seeded random stream: the same seed always draws the same numbers, on every platform.
//!
//! The generator is `xoshiro256++` (Blackman and Vigna, <https://prng.di.unimi.it/xoshiro256plusplus.c>), its four words
//! of state filled from the seed by `SplitMix64` (<https://prng.di.unimi.it/splitmix64.c>), as its authors advise. It is
//! not the stream of numpy's `default_rng(seed)`: a seed draws other numbers than that does.

use std::collections::BTreeSet;

/// A random stream.
pub(crate) struct Rng {
    state: [u64; 4], // constant: xoshiro256++ keeps four words of state
}

/// The next word of the `SplitMix64` stream that `state` is at.
fn splitmix(state: &mut u64) -> u64 {
    // the increment, multipliers and shifts of the reference code (prng.di.unimi.it/splitmix64.c)
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15); // constant: the SplitMix64 increment (the golden ratio)
    let mut word = *state;
    word = (word ^ (word >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9); // constant: SplitMix64 first shift and multiplier
    word = (word ^ (word >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb); // constant: SplitMix64 second shift and multiplier
    word ^ (word >> 31) // constant: SplitMix64 final shift
}

/// A hash of `seed` and `words` (a position in a lattice, say): one `SplitMix64` step, which is a bijection of the 64 bits,
/// for each word xored into the state. The same arguments give the same word on every platform; it keeps no state, so
/// the value at a position never depends on which others were asked for.
pub(crate) fn mix(seed: u32, words: &[u64]) -> u64 {
    words.iter().fold(u64::from(seed), |state, word| {
        let mut moved = state ^ word;
        splitmix(&mut moved)
    })
}

/// `word` as a number from 0 up to but not including 1: its top 53 bits (a float's whole mantissa) over 2^53 (Vigna's
/// recipe for a double from 64 random bits).
#[expect(clippy::cast_precision_loss, reason = "53 bits fit a float's mantissa exactly")]
pub(crate) const fn unit(word: u64) -> f64 {
    (word >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0) // constant: 2^53, the values a double mantissa holds
}

impl Rng {
    /// The stream of `seed`.
    pub(crate) fn seeded(seed: u32) -> Self {
        let mut expanding = u64::from(seed);
        Self {
            state: std::array::from_fn(|_| splitmix(&mut expanding)),
        }
    }

    /// The next 64 random bits.
    fn next_word(&mut self) -> u64 {
        // the rotations and the shift of the reference code (prng.di.unimi.it/xoshiro256plusplus.c)
        let [first, second, third, fourth] = self.state;
        let word = first.wrapping_add(fourth).rotate_left(23).wrapping_add(first); // constant: xoshiro256++ output rotation
        let moved = second << 17; // constant: xoshiro256++ state shift
        let third = third ^ first;
        let fourth = fourth ^ second;
        let second = second ^ third;
        let first = first ^ fourth;
        self.state = [first, second, third ^ moved, fourth.rotate_left(45)]; // constant: xoshiro256++ state rotation
        word
    }

    /// The next number from 0 up to but not including 1 (see [`unit`]).
    pub(crate) fn uniform(&mut self) -> f64 {
        unit(self.next_word())
    }

    /// `count` different numbers below `bound`, every choice equally likely, in ascending order (Floyd's algorithm, "Sampling
    /// with the rule of Bentley and Floyd", 1987: `count` draws, whatever the bound). `count` is at most `bound`.
    pub(crate) fn sample(&mut self, bound: usize, count: usize) -> Vec<usize> {
        let mut chosen = BTreeSet::new();
        for last in bound.saturating_sub(count)..bound {
            let drawn = self.below(last + 1);
            // a number drawn before stands for `last`, which no draw has been able to reach yet
            if !chosen.insert(drawn) {
                chosen.insert(last);
            }
        }
        chosen.into_iter().collect()
    }

    /// A number from 0 to `bound` - 1, every one equally likely (Lemire's unbiased bounded draw, "Fast Random Integer
    /// Generation in an Interval", 2019). `bound` is at least 1; 0 gives 0.
    pub(crate) fn below(&mut self, bound: usize) -> usize {
        let bound = bound as u64; // constant: a usize is at most 64 bits wide here, so it fits
        let mut product = u128::from(self.next_word()) * u128::from(bound);
        // the low 64 bits below `bound` would favour some results: draw again until they are not
        if halves(product).1 < bound {
            let least = bound.wrapping_neg() % bound;
            while halves(product).1 < least {
                product = u128::from(self.next_word()) * u128::from(bound);
            }
        }
        halves(product).0
    }
}

/// The high and the low 64 bits of `product`. The high half of a draw below `bound` is below `bound`, so it is a `usize`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "each half is 64 bits and the high one is below the bound"
)]
const fn halves(product: u128) -> (usize, u64) {
    ((product >> 64) as usize, product as u64) // constant: the halves of a 128-bit product
}

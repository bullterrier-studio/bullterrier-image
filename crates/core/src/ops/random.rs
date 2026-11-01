//! A seeded random stream: the same seed always draws the same numbers, on every platform.
//!
//! The generator is `xoshiro256++` (Blackman and Vigna, <https://prng.di.unimi.it/xoshiro256plusplus.c>), its four words
//! of state filled from the seed by `SplitMix64` (<https://prng.di.unimi.it/splitmix64.c>), as its authors advise. It is
//! not the stream of numpy's `default_rng(seed)`: a seed draws other numbers than that does.

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

#[cfg(test)]
mod tests {
    use super::{Rng, splitmix};

    #[test]
    fn splitmix64_from_zero_gives_the_published_words() {
        // the first four outputs of the reference splitmix64.c started at x = 0, as published (and recomputed from a
        // transcription of the reference code)
        let mut state = 0_u64;
        let words: Vec<u64> = (0..4).map(|_| splitmix(&mut state)).collect();
        assert_eq!(
            words,
            [
                0xe220_a839_7b1d_cdaf,
                0x6e78_9e6a_a1b9_65f4,
                0x06c4_5d18_8009_454f,
                0xf88b_b8a8_724c_81ec
            ]
        );
    }

    #[test]
    fn xoshiro256_plus_plus_from_the_state_1_2_3_4_gives_the_published_words() {
        // the published test vector of the generator (the first output is rotl(1 + 4, 23) + 1 = 5 << 23 | ... = 41943041
        // by hand); the rest recomputed from a transcription of the reference xoshiro256plusplus.c
        let mut rng = Rng { state: [1, 2, 3, 4] };
        let words: Vec<u64> = (0..6).map(|_| rng.next_word()).collect();
        assert_eq!(
            words,
            [
                41_943_041,
                58_720_359,
                3_588_806_011_781_223,
                3_591_011_842_654_386,
                9_228_616_714_210_784_205,
                9_973_669_472_204_895_162,
            ]
        );
    }

    #[test]
    fn a_seed_is_expanded_by_splitmix64_into_the_state_of_the_generator() {
        // seed 0 starts the four words at the four SplitMix64 words above; its first xoshiro256++ output is
        // rotl(s0 + s3, 23) + s0, worked out here from those published words
        let (first, fourth) = (0xe220_a839_7b1d_cdaf_u64, 0xf88b_b8a8_724c_81ec_u64);
        let expected = first.wrapping_add(fourth).rotate_left(23).wrapping_add(first);
        assert_eq!(Rng::seeded(0).next_word(), expected);
    }

    #[test]
    fn below_draws_a_number_under_the_bound_and_the_same_seed_the_same_numbers() {
        // from a transcription of the reference generator with Lemire's rule: seed 7 below 10, seed 0 below 3
        let mut rng = Rng::seeded(7);
        let drawn: Vec<usize> = (0..12).map(|_| rng.below(10)).collect();
        assert_eq!(drawn, [0, 1, 7, 4, 9, 4, 7, 3, 9, 0, 1, 1]);
        let mut rng = Rng::seeded(0);
        let drawn: Vec<usize> = (0..12).map(|_| rng.below(3)).collect();
        assert_eq!(drawn, [0, 1, 1, 0, 1, 0, 2, 2, 0, 0, 0, 0]);
    }

    #[test]
    fn below_redraws_where_the_bound_would_favour_some_numbers() {
        // two thirds of 2^64 is a bound that rejects a third of the draws, the first of seed 0 among them: the numbers are
        // those of the transcription of the reference generator with Lemire's rule, which skips that draw
        let bound = usize::MAX / 3 * 2;
        let mut rng = Rng::seeded(0);
        let drawn: Vec<usize> = (0..8).map(|_| rng.below(bound)).collect();
        assert_eq!(
            drawn,
            [
                4_700_713_651_777_080_836,
                4_422_511_062_648_552_786,
                140_877_894_367_766_886,
                6_090_746_802_919_456_582,
                252_907_807_315_440_571,
                10_542_282_251_666_238_536,
                912_914_496_555_681_806,
                3_867_938_380_237_749_146,
            ]
        );
    }

    #[test]
    fn below_reaches_every_number_under_each_bound_and_never_one_beyond() {
        for bound in 1..=7_usize {
            let mut rng = Rng::seeded(11);
            let mut seen = [false; 7];
            for _ in 0..1000 {
                let number = rng.below(bound);
                assert!(number < bound, "bound {bound}: {number}");
                seen[number] = true;
            }
            assert!(seen[..bound].iter().all(|&reached| reached), "bound {bound}: {seen:?}");
        }
    }

    #[test]
    fn below_is_fair_within_a_wide_margin_over_a_fixed_seed() {
        // 60,000 draws of 6: each number about 10,000 times; a modulo bias of a 2^64 stream is far below this margin,
        // so the margin only catches a stream that favours a number
        let mut rng = Rng::seeded(2024);
        let mut counts = [0_u32; 6];
        for _ in 0..60_000 {
            counts[rng.below(6)] += 1;
        }
        assert!(
            counts.iter().all(|count| (9_500..=10_500).contains(count)),
            "{counts:?}"
        );
    }

    #[test]
    fn a_bound_of_one_always_gives_zero_and_a_huge_bound_stays_under_it() {
        let mut rng = Rng::seeded(5);
        assert!((0..100).all(|_| rng.below(1) == 0));
        let huge = usize::MAX / 3 * 2;
        assert!((0..100).all(|_| rng.below(huge) < huge));
    }
}

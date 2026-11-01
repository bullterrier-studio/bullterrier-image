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

    #[test]
    fn uniform_gives_the_top_53_bits_as_a_fraction_of_one_the_published_way() {
        // seed 0, from a transcription of the reference generators with Vigna's recipe (word >> 11) * 2^-53
        let mut rng = Rng::seeded(0);
        let drawn: Vec<f64> = (0..3).map(|_| rng.uniform()).collect();
        assert_eq!(
            drawn,
            [
                0.324_575_268_031_406_7,
                0.382_239_296_511_673_43,
                0.359_617_207_647_355_3
            ]
        );
    }

    #[test]
    fn uniform_stays_in_zero_to_one_and_averages_a_half() {
        let mut rng = Rng::seeded(3);
        let draws: Vec<f64> = (0..100_000).map(|_| rng.uniform()).collect();
        assert!(draws.iter().all(|draw| (0.0..1.0).contains(draw)));
        let mean = draws.iter().sum::<f64>() / 100_000.0;
        assert!((mean - 0.5).abs() < 0.01, "{mean}");
    }

    #[test]
    fn sample_gives_the_numbers_of_a_transcription_of_the_algorithm() {
        // 3 of 10 from seed 5, all 10 of 10, and 4 of 1000 from seed 9, from a transcription of Floyd's algorithm over the
        // reference generator
        assert_eq!(Rng::seeded(5).sample(10, 3), [0, 2, 5]);
        assert_eq!(Rng::seeded(5).sample(10, 10), (0..10).collect::<Vec<_>>());
        assert_eq!(Rng::seeded(9).sample(1000, 4), [198, 428, 597, 883]);
    }

    #[test]
    fn sample_is_ascending_different_in_range_and_every_number_is_as_likely() {
        let mut counts = [0_u32; 10];
        for seed in 0..3000 {
            let drawn = Rng::seeded(seed).sample(10, 3);
            assert!(drawn.windows(2).all(|pair| pair[0] < pair[1]) && drawn.len() == 3 && drawn[2] < 10);
            for number in drawn {
                counts[number] += 1;
            }
        }
        // 9000 picks over 10 numbers: 900 each when fair; the margin of 20 % is far wider than chance
        assert!(counts.iter().all(|count| (720..=1080).contains(count)), "{counts:?}");
        assert_eq!(Rng::seeded(1).sample(7, 0), Vec::<usize>::new());
    }
}

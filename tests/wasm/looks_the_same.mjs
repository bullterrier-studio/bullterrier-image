// The one tolerance of "looks the same" in this repository, in levels of 8-bit output: every sample is within MOST_LEVELS
// levels and all but SHARE_BEYOND_ONE_LEVEL of them within one. The numbers are those of
// crates/core/tests/it/common/compare.rs (the Rust source of them); tests/conftest.py states them for Python.
// A mismatch is reported by a count and the first sample, never by a diff of the whole buffer (never deepEqual on one).
export const SHARE_BEYOND_ONE_LEVEL = 0.001;
export const MOST_LEVELS = 2;

/** Why `actual` does not look like `expected` (two byte arrays), or null if it does. */
export function differenceInLooks(actual, expected) {
  if (actual.length !== expected.length) return `${actual.length} samples against ${expected.length}`;
  let beyondOne = 0;
  for (let index = 0; index < actual.length; index++) {
    const gap = Math.abs(actual[index] - expected[index]);
    if (gap > MOST_LEVELS) return `sample ${index} is ${gap} levels apart`;
    if (gap > 1) beyondOne++;
  }
  const allowed = Math.floor(SHARE_BEYOND_ONE_LEVEL * actual.length);
  return beyondOne > allowed ? `${beyondOne} samples are 2 levels apart; ${allowed} allowed` : null;
}

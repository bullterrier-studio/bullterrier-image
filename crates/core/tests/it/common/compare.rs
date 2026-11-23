//! How the tests tell two pictures, maps or label lists apart. A mismatch is reported by the index of its first value,
//! never printed whole (lesson L1: no whole pictures in a failure message).

/// The first index at which `left` and `right` differ, or none; if one is shorter, the length of the shorter.
pub(crate) fn first_difference<T: PartialEq>(left: &[T], right: &[T]) -> Option<usize> {
    if left.len() != right.len() {
        return Some(left.len().min(right.len()));
    }
    left.iter().zip(right).position(|(one, other)| one != other)
}

/// The first index at which `left` and `right` differ in a single bit, or none; if one is shorter, the length of the
/// shorter. (`-0.0` and `0.0` differ, and a NaN equals the same NaN.)
pub(crate) fn first_bit_difference(left: &[f32], right: &[f32]) -> Option<usize> {
    if left.len() != right.len() {
        return Some(left.len().min(right.len()));
    }
    left.iter()
        .zip(right)
        .position(|(one, other)| one.to_bits() != other.to_bits())
}

/// The first index at which `left` and `right` are further apart than `tolerance` (a NaN is further apart than any
/// tolerance), or none; if one is shorter, the length of the shorter.
pub(crate) fn first_difference_within(left: &[f32], right: &[f32], tolerance: f32) -> Option<usize> {
    if left.len() != right.len() {
        return Some(left.len().min(right.len()));
    }
    left.iter().zip(right).position(|(one, other)| {
        let gap = (one - other).abs();
        gap.is_nan() || gap > tolerance
    })
}

/// Fails unless `actual` is `expected` to within `tolerance`, value by value, naming the first value that is not and
/// `context`.
pub(crate) fn assert_close(actual: &[f32], expected: &[f32], tolerance: f32, context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}");
    let at = first_difference_within(actual, expected, tolerance);
    assert!(
        at.is_none(),
        "{context}: value {at:?} is {:?}, not {:?}",
        at.map(|index| actual[index]),
        at.map(|index| expected[index])
    );
}

/// The one tolerance of "looks the same" in this repository: two renders of one look (any thread count, a band or a window
/// against the whole, a cached run against a cold one, one platform against another) are compared in levels of 8-bit
/// output, after rounding. Every sample is within [`MOST_LEVELS`] levels, and all but [`SHARE_BEYOND_ONE_LEVEL`] of them
/// within one. The WebAssembly tests state the same numbers in `tests/wasm/looks_the_same.mjs`, the Python tests in
/// `tests/conftest.py`.
pub(crate) const SHARE_BEYOND_ONE_LEVEL: f64 = 0.001;
pub(crate) const MOST_LEVELS: i32 = 2;

#[allow(clippy::cast_possible_truncation)]
fn level(value: f32) -> Option<i32> {
    value
        .is_finite()
        .then(|| (value.clamp(0.0, 1.0) * 255.0).round() as i32)
}

/// Why `actual` does not look like `expected` (naming the first sample out of tolerance, never printing the values whole),
/// or nothing if it does.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub(crate) fn difference_in_looks(actual: &[f32], expected: &[f32]) -> Option<String> {
    if actual.len() != expected.len() {
        return Some(format!("{} samples against {}", actual.len(), expected.len()));
    }
    let (mut beyond_one, mut first_beyond_one) = (0_usize, None);
    for (at, (one, other)) in actual.iter().zip(expected).enumerate() {
        let (Some(one), Some(other)) = (level(*one), level(*other)) else {
            return Some(format!("sample {at} is {:?} against {:?}", actual[at], expected[at]));
        };
        let gap = (one - other).abs();
        if gap > MOST_LEVELS {
            return Some(format!("sample {at} is {gap} levels apart"));
        }
        if gap > 1 {
            beyond_one += 1;
            first_beyond_one.get_or_insert(at);
        }
    }
    let allowed = (SHARE_BEYOND_ONE_LEVEL * actual.len() as f64) as usize;
    (beyond_one > allowed)
        .then(|| format!("{beyond_one} samples are 2 levels apart (first at {first_beyond_one:?}); {allowed} allowed"))
}

/// Whether `actual` looks like `expected` ([`SHARE_BEYOND_ONE_LEVEL`], [`MOST_LEVELS`]).
pub(crate) fn looks_the_same(actual: &[f32], expected: &[f32]) -> bool {
    difference_in_looks(actual, expected).is_none()
}

/// Fails unless `actual` looks like `expected`, saying how it does not and `context`.
pub(crate) fn assert_looks_the_same(actual: &[f32], expected: &[f32], context: &str) {
    if let Some(why) = difference_in_looks(actual, expected) {
        panic!("{context}: {why}");
    }
}

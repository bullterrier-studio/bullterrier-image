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

/// The first index at which `left` and `right` are further apart than `tolerance`, or none; if one is shorter, the
/// length of the shorter.
pub(crate) fn first_difference_within(left: &[f32], right: &[f32], tolerance: f32) -> Option<usize> {
    if left.len() != right.len() {
        return Some(left.len().min(right.len()));
    }
    left.iter()
        .zip(right)
        .position(|(one, other)| (one - other).abs() > tolerance)
}

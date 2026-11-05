//! The comparators the other tests judge with (`common::compare`), each shown a pair they must call equal and one they
//! must tell apart.

use crate::common::compare::{assert_close, first_bit_difference, first_difference, first_difference_within};

#[test]
fn values_further_apart_than_the_tolerance_are_told_apart_at_the_first_such_index() {
    type Row<'a> = (&'a [f32], &'a [f32], f32, Option<usize>);
    let rows: [Row; 7] = [
        (&[0.1, 0.2, 0.3], &[0.1, 0.2, 0.3], 0.0, None),
        (&[0.1, 0.2, 0.3], &[0.1, 0.2005, 0.3], 1e-3, None),
        (&[0.1, 0.2, 0.3], &[0.1, 0.202, 0.31], 1e-3, Some(1)),
        (&[0.5, 0.5], &[0.5, 0.5, 0.5], 1.0, Some(2)),
        (&[0.5, 0.5, 0.5], &[0.5], 1.0, Some(1)),
        (&[0.5, f32::NAN], &[0.5, 0.5], 1.0, Some(1)),
        (&[0.5, f32::NAN], &[0.5, f32::NAN], 1.0, Some(1)),
    ];
    for (a, b, tolerance, first) in rows {
        assert_eq!(
            first_difference_within(a, b, tolerance),
            first,
            "{a:?} and {b:?} at {tolerance}"
        );
    }
}

#[test]
fn exact_comparison_is_by_value_and_bit_comparison_by_representation() {
    assert_eq!(first_difference(&[1, 2, 3], &[1, 2, 3]), None);
    assert_eq!(first_difference(&[1, 2, 3], &[1, 5, 3]), Some(1));
    assert_eq!(first_difference(&[1, 2], &[1, 2, 3]), Some(2));
    assert_eq!(first_bit_difference(&[0.5, f32::NAN], &[0.5, f32::NAN]), None);
    assert_eq!(first_bit_difference(&[0.5, 0.0], &[0.5, -0.0]), Some(1));
    assert_eq!(first_bit_difference(&[0.5, 0.25], &[0.5]), Some(1));
}

#[test]
fn assert_close_accepts_values_within_the_tolerance() {
    assert_close(&[0.5, 1.0], &[0.5, 1.000_000_5], 1e-6, "within");
}

#[test]
#[should_panic(expected = "beyond: value Some(1)")]
fn assert_close_names_the_first_value_beyond_the_tolerance() {
    assert_close(&[0.5, 1.0], &[0.5, 1.001], 1e-6, "beyond");
}

#[test]
#[should_panic(expected = "shorter")]
fn assert_close_refuses_lists_of_different_lengths() {
    assert_close(&[0.5], &[0.5, 0.5], 1.0, "shorter");
}

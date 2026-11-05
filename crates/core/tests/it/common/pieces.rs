//! Reading the labels of a partition (`common::ops` makes them).

/// A number of pieces as a label bound, saturating.
fn bound(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// The pieces of a picture as one line of digits for each row, rows joined by `/`: labels below 10 only.
pub(crate) fn drawn(labels: &[i32], width: usize) -> String {
    labels
        .chunks_exact(width)
        .map(|row| row.iter().map(ToString::to_string).collect::<String>())
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether the labels are the pieces' numbers in reading order: each label first appears right after the greatest before it,
/// and all `count` appear.
pub(crate) fn numbered_by_first_pixel(labels: &[i32], count: usize) -> bool {
    let mut greatest = -1;
    for &label in labels {
        if label > greatest + 1 || !(0..bound(count)).contains(&label) {
            return false;
        }
        greatest = greatest.max(label);
    }
    greatest + 1 == bound(count)
}

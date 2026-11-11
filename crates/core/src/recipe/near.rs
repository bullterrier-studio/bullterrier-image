//! The names closest to one a person mistyped.

/// The number of single-letter edits (insert, delete, change) between `one` and `other`: Levenshtein's distance.
fn edits(one: &str, other: &str) -> usize {
    let other: Vec<char> = other.chars().collect();
    let mut row: Vec<usize> = (0..=other.len()).collect();
    for (i, a) in one.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, b) in other.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(a != *b)).min(above + 1).min(row[j] + 1);
            diagonal = above;
        }
    }
    row[other.len()]
}

/// Up to three of `known` a few edits from `name` (about one in three letters, at least two), the nearest first.
pub(crate) fn closest<'a>(name: &str, known: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let reach = (name.chars().count() / 3).max(2); // constant: about one edit in three letters, at least two
    let mut near: Vec<(usize, &str)> = known
        .into_iter()
        .map(|candidate| (edits(name, candidate), candidate))
        .filter(|(distance, _)| *distance <= reach)
        .collect();
    near.sort_unstable();
    near.into_iter().take(3).map(|(_, candidate)| candidate).collect()
}

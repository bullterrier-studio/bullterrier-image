//! What a filter sees beyond the picture's edge, worked out from the definition of each rule by folding the index back
//! into the line one bounce at a time. (The core finds it by modular arithmetic.) The reference of every test that
//! compares a filter with its definition.

use bullterrier_image::{Edge, EdgeOrConstant};

/// Where the sample at `index` of a line of `len` samples (`len` at least 1) comes from under `rule`, always inside the
/// line.
pub(crate) fn folded(rule: Edge, mut index: i64, len: i64) -> usize {
    let inside = |index: i64| (0..len).contains(&index);
    match rule {
        Edge::Nearest => index = index.clamp(0, len - 1),
        Edge::Wrap => {
            while !inside(index) {
                index += if index < 0 { len } else { -len };
            }
        }
        Edge::Reflect => {
            while !inside(index) {
                index = if index < 0 { -index - 1 } else { 2 * len - 1 - index };
            }
        }
        Edge::Mirror => {
            while len > 1 && !inside(index) {
                index = if index < 0 { -index } else { 2 * (len - 1) - index };
            }
            index = index.clamp(0, len - 1);
        }
    }
    usize::try_from(index).unwrap_or(0)
}

/// Where the sample at `index` of a line of `len` samples (`len` at least 1) comes from under `rule`; none for the
/// constant rule beyond the line, where there is no sample but a value.
pub(crate) fn source(rule: EdgeOrConstant, index: i64, len: i64) -> Option<usize> {
    let edge = match rule {
        EdgeOrConstant::Constant => return (0..len).contains(&index).then(|| folded(Edge::Nearest, index, len)),
        EdgeOrConstant::Nearest => Edge::Nearest,
        EdgeOrConstant::Wrap => Edge::Wrap,
        EdgeOrConstant::Reflect => Edge::Reflect,
        EdgeOrConstant::Mirror => Edge::Mirror,
    };
    Some(folded(edge, index, len))
}

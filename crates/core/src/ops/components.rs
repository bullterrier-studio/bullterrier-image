//! Connected components of a selection: its pieces, 4-connected (pixels sharing an edge, not a corner).

/// The pixels of `inside` (a map `width` wide, row after row) grouped into connected components, in the order of their first
/// pixel in the raster scan, and how many components there are. A component for which `keep(area)` is false is left
/// out. Returns each pixel's component, counted from 1 (0 for a pixel outside every kept component), and that count.
///
/// An explicit stack, not recursion: a component can be every pixel of the picture.
pub(crate) fn label(inside: &[bool], width: usize, keep: impl Fn(usize) -> bool) -> (Vec<usize>, usize) {
    let mut component = vec![0; inside.len()];
    let mut seen = vec![false; inside.len()];
    let (mut kept, mut stack, mut members) = (0, Vec::new(), Vec::new());
    for start in 0..inside.len() {
        if !inside[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        members.clear();
        while let Some(at) = stack.pop() {
            members.push(at);
            let column = at % width;
            let neighbours = [
                at.checked_sub(width),
                (at + width < inside.len()).then_some(at + width),
                (column > 0).then(|| at - 1),
                (column + 1 < width).then_some(at + 1),
            ];
            for next in neighbours.into_iter().flatten() {
                if inside[next] && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if keep(members.len()) {
            kept += 1;
            for &at in &members {
                component[at] = kept;
            }
        }
    }
    (component, kept)
}

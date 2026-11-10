//! The clean-up of a subject's matte: its edge pulled in, then softened.

use crate::Layer;
use crate::ops::blur::gaussian_filter_owned;
use crate::ops::edge::Extension;
use crate::ops::length::{float, whole};
use crate::ops::rank::minimum_along_rows;

/// How far a Gaussian reaches, in sigmas: the cut-off scipy's `gaussian_filter` has, which the matte was tuned with.
const TRUNCATE: f64 = 4.0; // constant: scipy.ndimage.gaussian_filter's default `truncate`
/// Slack on the disc's radius so a pixel exactly on its rim counts, as it does in floating point.
const RIM: f64 = 1e-9; // constant: tolerance of `x^2 + y^2 <= shrink^2` for a radius that is not a whole number

/// `matte` with its edge cleaned up: a new map of the same size.
///
/// A segmenter's edge is soft and slightly generous, which shows as a coloured halo once the background is replaced.
/// `shrink` (px) pulls the edge in: a pixel keeps the lowest value within a disc of that radius around it (pixels `x`
/// across and `y` down with `x^2 + y^2 <= shrink^2`), so a halo up to `shrink` px wide is cut away; the picture's own
/// border does not count as an edge. Then `feather` (px) blurs the edge with a Gaussian of that sigma (cut off at 4
/// sigmas), which softens it without changing the matte's total. 0 turns either off, and so does a negative value or
/// one that is not a number.
#[must_use]
pub fn refine_edge(matte: &Layer, shrink: f64, feather: f64) -> Layer {
    let (height, width) = (matte.height(), matte.width());
    let mut values = matte.values().to_vec();
    if shrink > 0.0 {
        values = eroded(&values, height, width, shrink);
    }
    if feather > 0.0 {
        values = gaussian_filter_owned(values, (height, width), feather, TRUNCATE, Extension::Reflect);
    }
    for value in &mut values {
        *value = value.clamp(0.0, 1.0);
    }
    // the values came from a map within 0..1 and were only ever mixed or minimised, so they are still a map
    Layer::from_checked(height, width, values)
}

/// `values` with each replaced by the smallest within the disc of radius `radius` around it, `Nearest` beyond the edge.
fn eroded(values: &[f32], height: usize, width: usize, radius: f64) -> Vec<f32> {
    let reach = whole(radius.ceil()); // the disc's extent in whole pixels
    let limit = radius * radius + RIM;
    let mut out = vec![f32::INFINITY; values.len()];
    for up in 0..=2 * reach {
        let dy = float(up) - float(reach);
        let rest = limit - dy * dy;
        if rest < 0.0 {
            continue;
        }
        let half = whole(rest.sqrt().floor());
        let row_minima = minimum_along_rows(values, width, 2 * half + 1, Extension::Nearest);
        for row in 0..height {
            let source = (row + up).saturating_sub(reach).min(height - 1);
            for column in 0..width {
                let found = row_minima[source * width + column];
                let slot = &mut out[row * width + column];
                *slot = slot.min(found);
            }
        }
    }
    out
}

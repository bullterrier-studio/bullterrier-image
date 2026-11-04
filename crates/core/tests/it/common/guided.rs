//! The guided filter (He, Sun and Tang, 2010, algorithm 1, for a grey guide) worked out window by window, in f64: the
//! reference an effect that uses the filter is compared with.

/// The mean of `values` (`height` x `width`, row after row) over the (2 x `radius` + 1) square around each place, cut
/// off at the picture's edge, found by adding up every value in the window.
fn window_mean(values: &[f64], height: usize, width: usize, radius: usize) -> Vec<f64> {
    let mut out = Vec::new();
    for row in 0..height {
        for column in 0..width {
            let (mut sum, mut count) = (0.0, 0.0);
            for y in row.saturating_sub(radius)..=(row + radius).min(height - 1) {
                for x in column.saturating_sub(radius)..=(column + radius).min(width - 1) {
                    sum += values[y * width + x];
                    count += 1.0;
                }
            }
            out.push(sum / count);
        }
    }
    out
}

/// `source` smoothed to keep the edges of `guide`, both `height` x `width`.
pub(crate) fn guided_filter(
    guide: &[f64],
    source: &[f64],
    height: usize,
    width: usize,
    radius: usize,
    epsilon: f64,
) -> Vec<f64> {
    let mean = |values: &[f64]| window_mean(values, height, width, radius);
    let product = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(a, b)| a * b).collect::<Vec<_>>();
    let (mean_guide, mean_source) = (mean(guide), mean(source));
    let (mean_both, mean_square) = (mean(&product(guide, source)), mean(&product(guide, guide)));
    let slope: Vec<f64> = (0..guide.len())
        .map(|at| {
            (mean_both[at] - mean_guide[at] * mean_source[at])
                / (mean_square[at] - mean_guide[at] * mean_guide[at] + epsilon)
        })
        .collect();
    let offset: Vec<f64> = (0..guide.len())
        .map(|at| mean_source[at] - slope[at] * mean_guide[at])
        .collect();
    let (mean_slope, mean_offset) = (mean(&slope), mean(&offset));
    (0..guide.len())
        .map(|at| mean_slope[at] * guide[at] + mean_offset[at])
        .collect()
}

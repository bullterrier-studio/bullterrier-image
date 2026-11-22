// `ops/gauss.rs` `ndtr` on the GPU: the standard normal's cumulative distribution at `x`, from the Chebyshev fit of erfc of Numerical Recipes
// (fractional error under 1.2e-7: a few f32 steps; the CPU's is `libm::erfc` in f64).
fn ndtr(x: f32) -> f32 {
    let z = abs(x) * 0.7071067811865476;
    let t = 1.0 / (1.0 + 0.5 * z);
    let poly = -z * z - 1.26551223 + t * (1.00002368 + t * (0.37409196 + t * (0.09678418 + t * (-0.18628806
        + t * (0.27886807 + t * (-1.13520398 + t * (1.48851587 + t * (-0.82215223 + t * 0.17087277))))))));
    let tail = t * exp(poly); // erfc(z), z >= 0
    return select(0.5 * tail, 1.0 - 0.5 * tail, x >= 0.0);
}

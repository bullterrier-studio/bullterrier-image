// `ops/hatch.rs` on the GPU: how much ink a line of `width` px paints on the pixel that lies `across` a family of lines
// `spacing` apart (`Lines::cover`), and how dark a pixel of brightness `light` is between a paper and an ink (`Tone::of_light`).

// the share of the pixel, taken one px across, that lines of `width` cover (a pixel one px across, `off` from a line)
fn overlap(off: f32, half: f32) -> f32 {
    return max(min(off + 0.5, half) - max(off - 0.5, -half), 0.0);
}

fn hatch_cover(across: f32, width: f32, spacing: f32) -> f32 {
    let place = across / spacing;
    let off = (place - floor(place) - 0.5) * spacing;
    let half = width / 2.0;
    return min(overlap(off, half) + overlap(off - spacing, half) + overlap(off + spacing, half), 1.0);
}

// `paper_light` is the paper's luminance and `reach` the ink's less the paper's (0: no tone is asked of the ink)
fn hatch_darkness(light: f32, paper_light: f32, reach: f32) -> f32 {
    if abs(reach) < 2.220446049250313e-16 {
        return 0.0;
    }
    return clamp((light - paper_light) / reach, 0.0, 1.0);
}

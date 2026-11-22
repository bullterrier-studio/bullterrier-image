// `curves` on the GPU, as `ops/tone.rs` `Curve::apply`: the master curve, then each channel's, each a monotone cubic
// through five points evaluated piece by piece from the table (`data`: four curves, four pieces, four coefficients,
// highest power first). The CPU evaluates in f64.

struct Block {
    inputs: vec4<f32>, // the first four points' inputs
    last: vec4<f32>,   // x: the last point's input
}

// the curve `curve` (0 master, 1 red, 2 green, 3 blue) at `value`
fn curve_at(curve: u32, value: f32) -> f32 {
    let inputs = settings.block.inputs;
    let x = clamp(value, inputs.x, settings.block.last.x);
    // the last piece whose first input is at or below x
    var piece = 0u;
    for (var k = 1u; k < 4u; k = k + 1u) {
        if inputs[k] <= x {
            piece = k;
        }
    }
    let at = (curve * 4u + piece) * 4u;
    let along = x - inputs[piece];
    return ((data[at] * along + data[at + 1u]) * along + data[at + 2u]) * along + data[at + 3u];
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let p = source[i].rgb;
    store(i, vec3<f32>(curve_at(1u, curve_at(0u, p.x)), curve_at(2u, curve_at(0u, p.y)), curve_at(3u, curve_at(0u, p.z))));
}

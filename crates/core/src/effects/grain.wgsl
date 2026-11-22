// `grain` on the GPU, for specks of one pixel: the CPU's draw for the canvas place (`ops/random.rs`, random.wgsl) times the
// deviation at the pixel's brightness, added to every channel (or each its own), clipped.

struct Block {
    grain: vec4<f32>, // amount, balance, per-channel noise (1) or one noise (0), uniform (1) or gaussian (0)
    seed: vec4<u32>,  // x: the seed
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let head = settings.header;
    let at = (head.first_row + id.y) * head.canvas_width + id.x;
    let b = settings.block;
    let pixel = source[i].rgb;
    let deviation = b.grain.x * (1.0 + b.grain.y * (2.0 * luminance(pixel) - 1.0));
    var noise = vec3<f32>(0.0);
    for (var channel = 0u; channel < 3u; channel++) {
        let stream = select(0u, channel, b.grain.z > 0.5);
        var draw: f32;
        if b.grain.w > 0.5 {
            draw = (2.0 * unit53(mix3(b.seed.x, stream, at, 0u)) - 1.0) * 1.7320508075688772;
        } else {
            draw = standard_normal(b.seed.x, stream, at);
        }
        noise[channel] = draw;
    }
    store(i, clamp(pixel + deviation * noise, vec3<f32>(0.0), vec3<f32>(1.0)));
}

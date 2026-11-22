// `light_leak` on the GPU: the sum of the patches' Gaussians (`data`: depth, along, sigma, sigma along, weight for each), squashed
// by 1 - exp(-sum) and screened over the pixel.

struct Block {
    tint: vec4<f32>,
    side: vec4<f32>, // x: sin, y: cos of the side's turn, z: how far the side is from the centre
    count: vec4<u32>, // x: how many patches
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let head = settings.header;
    let count = b.count.x;
    let right = f32(id.x) - (f32(head.canvas_width) - 1.0) / 2.0;
    let up = (f32(head.canvas_height) - 1.0) / 2.0 - f32(head.first_row + id.y);
    let depth = b.side.z - (right * b.side.y + up * b.side.x);
    let along = up * b.side.y - right * b.side.x;
    var wash = 0.0;
    for (var k = 0u; k < count; k++) {
        let at = 5u * k;
        let in_from = (depth - data[at]) / data[at + 2u];
        let across = (along - data[at + 1u]) / data[at + 3u];
        wash += data[at + 4u] * exp(-0.5 * (in_from * in_from + across * across));
    }
    let light = 1.0 - exp(-wash);
    let base = source[i].rgb;
    store(i, base + clamp(b.tint.rgb * light, vec3<f32>(0.0), vec3<f32>(1.0)) * (1.0 - base));
}

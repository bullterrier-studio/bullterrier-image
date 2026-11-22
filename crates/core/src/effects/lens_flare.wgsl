// `lens_flare` on the GPU: the sum of the flare's bumps (`data`: centre row, centre column, reach, ring, weight for each),
// screened over the pixel as `ops/light.rs` does. A bump is `weight x (1 - t^2)^2` within its reach of its ring's circle.

struct Block {
    tint: vec4<f32>,
    count: vec4<u32>, // x: how many bumps
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let row = f32(settings.header.first_row + id.y);
    let column = f32(id.x);
    var flare = 0.0;
    for (var k = 0u; k < b.count.x; k++) {
        let at = 5u * k;
        let down = row - data[at];
        let right = column - data[at + 1u];
        let reach = data[at + 2u];
        let outer = reach + data[at + 3u];
        if abs(down) >= outer || abs(right) >= outer {
            continue;
        }
        let away = (length(vec2<f32>(down, right)) - data[at + 3u]) / reach;
        let bell = max(1.0 - away * away, 0.0);
        flare += data[at + 4u] * bell * bell;
    }
    let base = source[i].rgb;
    store(i, base + clamp(b.tint.rgb * flare, vec3<f32>(0.0), vec3<f32>(1.0)) * (1.0 - base));
}

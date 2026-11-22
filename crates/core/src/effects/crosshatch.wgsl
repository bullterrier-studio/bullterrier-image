// `crosshatch` on the GPU: the CPU's loop over the families of lines (each asked for what the ones before left), one pixel a
// thread. The lines' directions (sines and cosines) are worked out in f64 on the CPU.

struct Block {
    tone: vec4<f32>,  // x: the paper's luminance, y: the ink's less the paper's, z: spacing, w: thickness
    layers: vec4<u32>, // x: how many families
    paper: vec4<f32>,
    ink: vec4<f32>,
    turns: array<vec4<f32>, 6>, // x: sin, y: cos of each family's direction
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let spacing = b.tone.z;
    let darkness = hatch_darkness(luminance(source[i].rgb), b.tone.x, b.tone.y);
    let x = f32(id.x) + 0.5;
    let y = f32(settings.header.first_row + id.y) + 0.5;
    var bare = 1.0;
    var left = 1.0;
    for (var k = 0u; k < b.layers.x; k++) {
        if left <= 2.220446049250313e-16 {
            break;
        }
        let weight = clamp(1.0 - (1.0 - darkness) / left, 0.0, b.tone.w);
        left *= 1.0 - weight;
        let across = y * b.turns[k].y - x * b.turns[k].x;
        bare *= 1.0 - hatch_cover(across, weight * spacing, spacing);
    }
    let cover = 1.0 - bare;
    store(i, b.paper.rgb + (b.ink.rgb - b.paper.rgb) * cover);
}

// `engraving` on the GPU for straight lines (no `bend`): one family, as wide as the pixel's tone asks.

struct Block {
    tone: vec4<f32>, // x: the paper's luminance, y: the ink's less the paper's, z: spacing
    turn: vec4<f32>, // x: sin, y: cos of the lines' direction
    paper: vec4<f32>,
    ink: vec4<f32>,
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let x = f32(id.x) + 0.5;
    let y = f32(settings.header.first_row + id.y) + 0.5;
    let across = y * b.turn.y - x * b.turn.x;
    let width = hatch_darkness(luminance(source[i].rgb), b.tone.x, b.tone.y) * b.tone.z;
    let cover = hatch_cover(across, width, b.tone.z);
    store(i, b.paper.rgb + (b.ink.rgb - b.paper.rgb) * cover);
}

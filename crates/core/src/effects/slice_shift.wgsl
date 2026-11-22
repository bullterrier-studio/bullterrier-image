// `slice_shift` on the GPU: every row turns round to the right by the number of pixels the CPU worked out for it (`data`, a whole number
// a row), what leaves one side coming back on the other. The pixels are copied, not mixed.

struct Block {
    none: vec4<f32>,
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let width = settings.header.width;
    let moved = u32(data[id.y]);
    let column = (id.x + width - moved) % width;
    store(pixel_index(id), source[id.y * width + column].rgb);
}

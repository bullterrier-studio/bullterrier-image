// `scanlines` on the GPU: one gain a row, worked out on the CPU in f64 (`data`, a row after row from the picture's first
// row), multiplied into every channel.

struct Block {
    unused: vec4<f32>,
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    store(i, source[i].rgb * data[id.y]);
}

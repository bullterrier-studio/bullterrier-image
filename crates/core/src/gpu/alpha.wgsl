// A `frame.transparent` cut on the GPU: the picture's colour with the alpha the CPU worked out (`data`, one a pixel).

struct Block {
    unused: vec4<f32>,
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    dest[i] = vec4<f32>(source[i].rgb, data[i]);
}

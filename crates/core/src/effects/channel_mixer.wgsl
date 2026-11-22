// `channel_mixer` on the GPU: each output channel the weighted sum of the input's, summed in the CPU's order, clipped.

struct Block {
    red: vec4<f32>,   // the weights of input red, green, blue in output red
    green: vec4<f32>,
    blue: vec4<f32>,
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let p = source[i].rgb;
    let b = settings.block;
    let mixed = vec3<f32>(
        b.red.x * p.x + b.red.y * p.y + b.red.z * p.z,
        b.green.x * p.x + b.green.y * p.y + b.green.z * p.z,
        b.blue.x * p.x + b.blue.y * p.y + b.blue.z * p.z,
    );
    store(i, clamp(mixed, vec3<f32>(0.0), vec3<f32>(1.0)));
}

// An upload: the picture's values as they were sent (`data`, 3 or 4 floats a pixel, row after row) spread into one vec4
// a pixel; a picture without alpha gets 1.

struct Block {
    channels: vec4<f32>, // x: 3 or 4
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    if settings.block.channels.x > 3.5 {
        dest[i] = vec4<f32>(data[4u * i], data[4u * i + 1u], data[4u * i + 2u], data[4u * i + 3u]);
    } else {
        dest[i] = vec4<f32>(data[3u * i], data[3u * i + 1u], data[3u * i + 2u], 1.0);
    }
}

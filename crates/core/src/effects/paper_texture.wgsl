// `paper_texture` on the GPU: each pixel is multiplied by 1 + amount x texture, the texture the fibres' value noise (`ops/noise.wgsl`) mixed
// with a weave of two cosines. `data`: a number for each row (the weave's, already scaled to variance 1 with the other axis), a number
// for each column, then the noise's octaves.

struct Block {
    mix: vec4<f32>,   // x: amount, y: the weave's share, z: the fibres' share
    shape: vec4<u32>, // x: the seed, y: 1 if there is a weave
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let head = settings.header;
    let fibres = value_noise(b.shape.x, 0u, head.height + head.width, head.first_row + id.y, id.x);
    var texture = fibres;
    if b.shape.y == 1u {
        texture = b.mix.y * (data[id.y] + data[head.height + id.x]) + b.mix.z * fibres;
    }
    store(i, clamp(source[i].rgb * (1.0 + b.mix.x * texture), vec3<f32>(0.0), vec3<f32>(1.0)));
}

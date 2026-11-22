// `noise_field` ('value', unwarped) on the GPU: the noise at the pixel (`ops/noise.wgsl`), its place on the normal curve, looked up along the
// colours. `data`: the noise's octaves, then the colour stops.

struct Block {
    shape: vec4<u32>, // x: the seed, y: how many colours
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let b = settings.block;
    let octaves = u32(data[0]);
    let z = value_noise(b.shape.x, 0u, 0u, settings.header.first_row + id.y, id.x);
    let colour = ramp_at(1u + 2u * octaves, b.shape.y, ndtr(z));
    store(pixel_index(id), colour);
}

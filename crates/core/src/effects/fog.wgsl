// `fog` on the GPU for clouds read in place: the cloud's value noise at the pixel, its rank on the normal curve, thresholded by the coverage
// and feathered, mixed in by an opacity heavier toward the bottom. `data`: the noise's octaves.

struct Block {
    cloud: vec4<f32>,  // x: amount, y: gradient, z: coverage, w: feather
    colour: vec4<f32>, // the fog's colour; w: floor
    seed: vec4<u32>,   // x: the seed
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let head = settings.header;
    let row = head.first_row + id.y;
    let z = value_noise(b.seed.x, 0u, 0u, row, id.x);
    let lift = clamp((ndtr(z) - (1.0 - b.cloud.z)) / b.cloud.w, 0.0, 1.0);
    let shape = lift * lift * (3.0 - 2.0 * lift);
    let down = (f32(row) + 0.5) / f32(head.canvas_height);
    let opacity = b.cloud.x * (1.0 - b.cloud.y * (1.0 - down)) * (b.colour.w + (1.0 - b.colour.w) * shape);
    let pixel = source[i].rgb;
    store(i, pixel + opacity * (b.colour.rgb - pixel));
}

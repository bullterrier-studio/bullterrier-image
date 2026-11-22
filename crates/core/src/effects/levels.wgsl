// `levels` on the GPU: `ops/tone.rs` `Remap::apply` for each channel, with the numbers `Remap::new` works out.

struct Block {
    remap: vec4<f32>, // black, span, exponent (1 / gamma), output black
    reach: vec4<f32>, // x: output white - output black
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let r = settings.block.remap;
    let normalised = clamp((source[i].rgb - r.x) / r.y, vec3<f32>(0.0), vec3<f32>(1.0));
    // WGSL's pow is undefined at 0; the CPU's powf gives 0 for a positive exponent
    let raised = select(pow(normalised, vec3<f32>(r.z)), vec3<f32>(0.0), normalised <= vec3<f32>(0.0));
    store(i, clamp(r.w + raised * settings.block.reach.x, vec3<f32>(0.0), vec3<f32>(1.0)));
}

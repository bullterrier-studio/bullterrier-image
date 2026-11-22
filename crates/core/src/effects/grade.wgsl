// `grade` on the GPU, step for step as `grade.rs` `render`: colour balance (when the block says so), exposure, contrast
// around the pivot, saturation, vibrance, with the clips where the CPU clips.

struct Block {
    balance: vec4<f32>, // the linear-light gains of red, green, blue; w: 1 if the balance runs
    tone: vec4<f32>,    // exposure's factor, pivot, contrast, saturation
    more: vec4<f32>,    // x: vibrance
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    var c = source[i].rgb;
    if b.balance.w > 0.5 {
        c = linear_to_srgb(srgb_to_linear(c) * b.balance.xyz);
    }
    let pivot = b.tone.y;
    let contrasted = pivot + (c * b.tone.x - pivot) * b.tone.z;
    let grey = luminance(clamp(contrasted, vec3<f32>(0.0), vec3<f32>(1.0)));
    let saturated = clamp(grey + (contrasted - grey) * b.tone.w, vec3<f32>(0.0), vec3<f32>(1.0));
    // vibrance: HSV saturation s to s + amount * s * (1 - s), the strongest channel kept
    let strongest = max(max(saturated.x, saturated.y), saturated.z);
    let weakest = min(min(saturated.x, saturated.y), saturated.z);
    var chroma = 0.0;
    if strongest > 0.0 {
        chroma = (strongest - weakest) / strongest;
    }
    let spread = 1.0 + b.more.x * (1.0 - chroma);
    store(i, clamp(strongest - spread * (strongest - saturated), vec3<f32>(0.0), vec3<f32>(1.0)));
}

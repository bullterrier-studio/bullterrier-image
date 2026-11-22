// `color_balance` on the GPU, as `color_balance.rs` `render`: the luma, the two smoothstep hand-overs, each range's push
// added in the CPU's order, the luma restored if asked, then the clip.

struct Block {
    shadows: vec4<f32>,    // the shadows' colour less mid grey; w: amount
    midtones: vec4<f32>,   // w: the first hand-over's centre (balance / 2)
    highlights: vec4<f32>, // w: a hand-over's width (softness / 2)
    keep: vec4<f32>,       // x: 1 if the luma is kept
}

// the cubic Hermite smoothstep, 3x^2 - 2x^3, of the hand-over's argument held to 0..1 (a standard constant)
fn handover(luma: f32, centre: f32, width: f32) -> f32 {
    let x = clamp((luma - centre) / width + 0.5, 0.0, 1.0);
    return x * x * (3.0 - 2.0 * x);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let pixel = source[i].rgb;
    let luma = luminance(pixel);
    let centre = b.midtones.w;
    let width = b.highlights.w;
    let lower = handover(luma, centre, width);
    let upper = handover(luma, centre + 0.5, width);
    let amount = b.shadows.w;
    var out = pixel;
    out = out + amount * (1.0 - lower) * b.shadows.xyz;
    out = out + amount * (lower - upper) * b.midtones.xyz;
    out = out + amount * upper * b.highlights.xyz;
    if b.keep.x > 0.5 {
        out = out + (luma - luminance(out));
    }
    store(i, clamp(out, vec3<f32>(0.0), vec3<f32>(1.0)));
}

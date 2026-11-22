// A step's result laid over its input on the GPU: `ops/blend.rs` `lay`, formula for formula. `source` is the base (the
// step's input), `other` the result, `data` the mask (when the block says there is one).

struct Block {
    lay: vec4<f32>, // x: the blend mode (the order of `mode` below), y: opacity, z: 1 if `data` is a mask
}

fn multiply(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return b * s;
}

fn screen(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return b + s - b * s;
}

fn overlay(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return select(screen(s, 2.0 * b - 1.0), multiply(s, 2.0 * b), b <= vec3<f32>(0.5));
}

fn soft_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    let curve = select(sqrt(b), ((16.0 * b - 12.0) * b + 4.0) * b, b <= vec3<f32>(0.25));
    let darker = b - (1.0 - 2.0 * s) * b * (1.0 - b);
    let lighter = b + (2.0 * s - 1.0) * (curve - b);
    return select(lighter, darker, s <= vec3<f32>(0.5));
}

// `Lum` of section 10.2 (the W3C weights, not Rec. 709's)
fn lum(c: vec3<f32>) -> f32 {
    return c.x * 0.3 + c.y * 0.59 + c.z * 0.11;
}

fn clip_color(c: vec3<f32>, l: f32) -> vec3<f32> {
    let low = min(min(c.x, c.y), c.z);
    let high = max(max(c.x, c.y), c.z);
    var clipped = c;
    if low < 0.0 {
        clipped = l + (clipped - l) * l / (l - low);
    }
    if high > 1.0 {
        clipped = l + (clipped - l) * (1.0 - l) / (high - l);
    }
    return clipped;
}

fn colour(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    let l = lum(b);
    return clip_color(s + (l - lum(s)), l);
}

fn mode(m: f32, b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    switch u32(m) {
        case 1u: { return multiply(b, s); }
        case 2u: { return screen(b, s); }
        case 3u: { return overlay(b, s); }
        case 4u: { return soft_light(b, s); }
        case 5u: { return abs(b - s); }
        case 6u: { return colour(b, s); }
        default: { return s; }
    }
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let l = settings.block.lay;
    let base = source[i].rgb;
    var alpha = l.y;
    if l.z > 0.5 {
        alpha = l.y * data[i];
    }
    let blended = mode(l.x, base, other[i].rgb);
    store(i, clamp(base * (1.0 - alpha) + blended * alpha, vec3<f32>(0.0), vec3<f32>(1.0)));
}

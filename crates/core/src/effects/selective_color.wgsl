// `selective_color` on the GPU: the CPU's HSV conversions (`ops/colorspace.rs`) and the weight of a hue in the chosen range,
// the same f32 maths.

struct Block {
    range: vec4<f32>, // hue, width, feather, hue shift (degrees)
    tone: vec4<f32>,  // saturation, lightness
}

// `rem_euclid` for a positive modulus: the remainder is never negative
fn modulo(x: f32, m: f32) -> f32 {
    return x - m * floor(x / m);
}

fn to_hsv(c: vec3<f32>) -> vec3<f32> {
    let value = max(c.x, max(c.y, c.z));
    let chroma = value - min(c.x, min(c.y, c.z));
    if chroma == 0.0 {
        return vec3<f32>(0.0, 0.0, value);
    }
    var sector: f32;
    if c.x >= c.y && c.x >= c.z {
        sector = modulo((c.y - c.z) / chroma, 6.0);
    } else if c.y >= c.z {
        sector = (c.z - c.x) / chroma + 2.0;
    } else {
        sector = (c.x - c.y) / chroma + 4.0;
    }
    return vec3<f32>(sector / 6.0, chroma / value, value);
}

fn from_hsv(hsv: vec3<f32>) -> vec3<f32> {
    var out: vec3<f32>;
    for (var channel = 0u; channel < 3u; channel++) {
        let offset = array<f32, 3>(5.0, 3.0, 1.0)[channel];
        let k = modulo(offset + hsv.x * 6.0, 6.0);
        let ramp = clamp(min(k, 4.0 - k), 0.0, 1.0);
        out[channel] = hsv.z - hsv.z * hsv.y * ramp;
    }
    return out;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let pixel = source[i].rgb;
    let hsv = to_hsv(pixel);
    let degrees = hsv.x * 360.0;
    let distance = abs(modulo(degrees - b.range.x + 180.0, 360.0) - 180.0);
    let outside_by = distance - b.range.y / 2.0;
    let weight = clamp(1.0 - outside_by / max(b.range.z, 1e-6), 0.0, 1.0) * hsv.y;
    if weight <= 0.0 {
        store(i, pixel);
        return;
    }
    let room = select(hsv.z, 1.0 - hsv.z, b.tone.y > 0.0);
    store(i, from_hsv(vec3<f32>(
        (degrees + weight * b.range.w) / 360.0,
        clamp(hsv.y * (1.0 + weight * (b.tone.x - 1.0)), 0.0, 1.0),
        hsv.z + weight * b.tone.y * room,
    )));
}

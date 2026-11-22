// The part of every kernel's WGSL before its own: the one binding layout, the uniform header, index helpers, and the
// colour maths of the CPU's `ops/colorspace.rs` and `ops/color.rs`. A kernel adds `struct Block` (its parameters, vec4s
// only) and `fn main`, `@compute @workgroup_size(16, 16)`, one invocation a pixel. No texture is sampled anywhere.

struct Header {
    width: u32,
    height: u32,
    canvas_height: u32,
    canvas_width: u32,
    first_row: u32, // the canvas row of the picture's row 0
    ratio: f32,     // working pixels per design pixel
    _pad0: u32,
    _pad1: u32,
}

struct Settings {
    header: Header,
    block: Block,
}

@group(0) @binding(0) var<uniform> settings: Settings;
@group(0) @binding(1) var<storage, read> source: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dest: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> data: array<f32>;
@group(0) @binding(4) var<storage, read> other: array<vec4<f32>>;

// whether the invocation `id` is a pixel of the picture (the last workgroups reach past its edges)
fn inside(id: vec3<u32>) -> bool {
    return id.x < settings.header.width && id.y < settings.header.height;
}

// the index of the pixel of invocation `id`, row after row
fn pixel_index(id: vec3<u32>) -> u32 {
    return id.y * settings.header.width + id.x;
}

// writes `rgb` as the pixel `i` of the destination, with the source's alpha: effects never see alpha, a picture keeps it
fn store(i: u32, rgb: vec3<f32>) {
    dest[i] = vec4<f32>(rgb, source[i].a);
}

// the Rec. 709 weights of red, green and blue in brightness (`ops/color.rs`, `LUMA`)
fn luminance(c: vec3<f32>) -> f32 {
    return c.x * 0.2126 + c.y * 0.7152 + c.z * 0.0722;
}

// the sRGB transfer (IEC 61966-2-1), mirrored about zero as `ops/colorspace.rs` does it; the CPU works the power in f64,
// WGSL has none, so a kernel that uses these names a bound
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let m = abs(c);
    let linear = select(pow((m + 0.055) / 1.055, vec3<f32>(2.4)), m / 12.92, m <= vec3<f32>(0.04045));
    return linear * sign_of(c);
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let m = abs(c);
    let encoded = select(1.055 * pow(m, vec3<f32>(1.0 / 2.4)) - 0.055, m * 12.92, m <= vec3<f32>(0.0031308));
    return encoded * sign_of(c);
}

// 1 for a value at or above zero, -1 below (`copysign`, for the values pictures hold)
fn sign_of(c: vec3<f32>) -> vec3<f32> {
    return select(vec3<f32>(1.0), vec3<f32>(-1.0), c < vec3<f32>(0.0));
}

// the index of the sample of a line of `len` that lies at `index`, however far outside it, by the CPU's rule for what lies
// beyond an edge (`Extension::locate`, `ops/edge.rs`): 0 reflect (the edge sample repeated), 1 nearest, 2 mirror (about the
// edge sample), 3 wrap
fn locate(index: i32, len: i32, rule: i32) -> i32 {
    if rule == 1 {
        return clamp(index, 0, len - 1);
    }
    if rule == 3 {
        return ((index % len) + len) % len;
    }
    if rule == 0 {
        let at = ((index % (2 * len)) + 2 * len) % (2 * len);
        return select(2 * len - 1 - at, at, at < len);
    }
    if len == 1 {
        return 0;
    }
    let period = 2 * (len - 1);
    let at = ((index % period) + period) % period;
    return select(period - at, at, at < len);
}

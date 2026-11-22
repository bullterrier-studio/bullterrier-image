// `lut` on the GPU: the CPU's `Lut::lookup` (trilinear between the eight entries around the colour, the domain's edge beyond
// it), then mixed in by `amount` and kept in 0 to 1. `data` is the table's entries, red changing fastest, then green, then
// blue; no table (size 0) is the identity.

struct Block {
    size: vec4<u32>,     // x: entries per axis (0: no table)
    domain_min: vec4<f32>,
    domain_max: vec4<f32>,
    mixing: vec4<f32>,   // x: amount
}

fn entry(red: u32, green: u32, blue: u32, size: u32) -> vec3<f32> {
    let at = 3u * (red + size * (green + size * blue));
    return vec3<f32>(data[at], data[at + 1u], data[at + 2u]);
}

fn lookup(colour: vec3<f32>, size: u32) -> vec3<f32> {
    let b = settings.block;
    let top = f32(size - 1u);
    let position = clamp((colour - b.domain_min.xyz) / (b.domain_max.xyz - b.domain_min.xyz), vec3<f32>(0.0), vec3<f32>(1.0)) * top;
    let cell = min(vec3<u32>(position), vec3<u32>(size - 2u));
    let fraction = position - vec3<f32>(cell);
    var mixed = vec3<f32>(0.0);
    for (var corner = 0u; corner < 8u; corner++) {
        let red = corner & 1u;
        let green = (corner >> 1u) & 1u;
        let blue = (corner >> 2u) & 1u;
        let share_red = select(1.0 - fraction.x, fraction.x, red == 1u);
        let share_green = select(1.0 - fraction.y, fraction.y, green == 1u);
        let share_blue = select(1.0 - fraction.z, fraction.z, blue == 1u);
        let weight = share_red * share_green * share_blue;
        mixed += weight * entry(cell.x + red, cell.y + green, cell.z + blue, size);
    }
    return mixed;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let pixel = source[i].rgb;
    var graded = pixel;
    if b.size.x > 0u {
        graded = lookup(pixel, b.size.x);
    }
    store(i, clamp(pixel + b.mixing.x * (graded - pixel), vec3<f32>(0.0), vec3<f32>(1.0)));
}

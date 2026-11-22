// `gradient_map` on the GPU: the brightness stretched between the CPU's clip points, then the colour at that position along
// the stops (`ramp::at`), mixed in Oklab if asked.

struct Block {
    stretch: vec4<f32>, // the brightness at the first stop, the span to the last, 1 to mix in Oklab, 0
    stops: vec4<u32>,   // x: the number of stops
}

fn from_oklab(c: vec3<f32>) -> vec3<f32> {
    let roots = vec3<f32>(
        c.x + 0.39633778 * c.y + 0.21580376 * c.z,
        c.x - 0.105561346 * c.y - 0.06385417 * c.z,
        c.x - 0.08948418 * c.y - 1.2914855 * c.z,
    );
    let cones = roots * roots * roots;
    return linear_to_srgb(vec3<f32>(
        4.0767417 * cones.x - 3.3077116 * cones.y + 0.23096993 * cones.z,
        -1.268438 * cones.x + 2.6097574 * cones.y - 0.3413194 * cones.z,
        -0.004196086 * cones.x - 0.7034186 * cones.y + 1.7076147 * cones.z,
    ));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let position = clamp((luminance(source[i].rgb) - b.stretch.x) / b.stretch.y, 0.0, 1.0);
    let count = b.stops.x;
    let scaled = position * f32(count - 1u);
    let below = min(u32(scaled), max(count, 2u) - 2u);
    let share = scaled - f32(below);
    let lower = vec3<f32>(data[3u * below], data[3u * below + 1u], data[3u * below + 2u]);
    let upper = vec3<f32>(data[3u * below + 3u], data[3u * below + 4u], data[3u * below + 5u]);
    var colour = lower * (1.0 - share) + upper * share;
    if b.stretch.z > 0.5 {
        colour = clamp(from_oklab(colour), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    store(i, colour);
}

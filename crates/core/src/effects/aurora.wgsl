// `aurora` on the GPU: the curtain's base wobbles by the column's ripple, its light fades up to its top and is broken by the column's
// ray. `data`: the ripple of each column, then its ray, then the colour stops (red, green, blue each).

struct Block {
    curtain: vec4<f32>, // x: the base's row (a share of the canvas's height), y: waviness, z: height of the curtain, w: falloff
    sky: vec4<f32>,
    stops: vec4<u32>,   // x: how many colour stops
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let head = settings.header;
    let above = b.curtain.x + b.curtain.y * data[id.x] - f32(head.first_row + id.y);
    let up = above / b.curtain.z;
    let fade = clamp(1.0 - up, 0.0, 1.0);
    // WGSL leaves pow(0, y) undefined: nothing is left of the light past the top
    let tail = select(pow(fade, b.curtain.w), 0.0, fade <= 0.0);
    let brightness = clamp(above + 0.5, 0.0, 1.0) * tail * data[head.width + id.x];
    // the colour at `up` along the stops, evenly spaced (`ops/ramp.rs`)
    let last = b.stops.x - 1u;
    let position = clamp(up, 0.0, 1.0) * f32(last);
    let below = min(u32(position), last - 1u);
    let share = position - f32(below);
    let from_at = 2u * head.width + 3u * below;
    let lower = vec3<f32>(data[from_at], data[from_at + 1u], data[from_at + 2u]);
    let upper = vec3<f32>(data[from_at + 3u], data[from_at + 4u], data[from_at + 5u]);
    let colour = lower * (1.0 - share) + upper * share;
    store(i, b.sky.rgb + brightness * (colour - b.sky.rgb));
}

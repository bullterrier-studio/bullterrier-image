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
    let colour = ramp_at(2u * head.width, b.stops.x, clamp(up, 0.0, 1.0));
    store(i, b.sky.rgb + brightness * (colour - b.sky.rgb));
}

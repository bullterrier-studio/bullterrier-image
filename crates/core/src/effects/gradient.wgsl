// `gradient` on the GPU: the position along the gradient from each pixel's offsets from its centre (the CPU's own, worked
// out in f64 once for each column and row: `data` holds the columns, then the rows, then the colour stops), then the
// CPU's f32 repeat and ramp.

struct Block {
    shape: vec4<f32>, // kind (0 linear, 1 radial, 2 conic), the conic's start angle in radians, repeat, reflect (1 or 0)
    stops: vec4<u32>, // x: the number of colour stops
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let across = data[id.x];
    let down = data[settings.header.width + id.y];
    var t: f32;
    if b.shape.x == 0.0 {
        t = clamp(across + down + 0.5, 0.0, 1.0);
    } else if b.shape.x == 1.0 {
        t = clamp(length(vec2<f32>(across, down)), 0.0, 1.0);
    } else {
        // atan2 of the point itself is undefined in WGSL; the CPU's is 0
        let angle = select(atan2(down, across), 0.0, across == 0.0 && down == 0.0);
        let turns = (angle - b.shape.y) / 6.283185307179586;
        t = turns - floor(turns);
    }
    // the position among the colours (`along_colours`): which repeat it is in, backwards in every second if reflected
    let repeat = b.shape.z;
    let cycles = t * repeat;
    let run = min(floor(cycles), repeat - 1.0);
    let within = cycles - run;
    let backwards = b.shape.w > 0.5 && (u32(run) % 2u) == 1u;
    let position = select(within, 1.0 - within, backwards);
    // the ramp (`ramp::at`)
    let base = settings.header.width + settings.header.height;
    let count = b.stops.x;
    var colour: vec3<f32>;
    if count == 1u {
        colour = vec3<f32>(data[base], data[base + 1u], data[base + 2u]);
    } else {
        let scaled = position * f32(count - 1u);
        let below = min(u32(scaled), count - 2u);
        let share = scaled - f32(below);
        let at = base + 3u * below;
        let lower = vec3<f32>(data[at], data[at + 1u], data[at + 2u]);
        let upper = vec3<f32>(data[at + 3u], data[at + 4u], data[at + 5u]);
        colour = lower * (1.0 - share) + upper * share;
    }
    store(i, colour);
}

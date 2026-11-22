// `pattern` on the GPU: the CPU's f32 maths for a pixel centre, its position along the pattern's axes and the signed distance
// to the shape's border.

struct Block {
    turn: vec4<f32>,  // cos, sin of the angle, cell, half the line width in px
    shape: vec4<f32>, // kind (0 stripes, 1 checks, 2 dots, 3 grid), the dot's radius in px, feather
    foreground: vec4<f32>,
    background: vec4<f32>,
}

// Rust's `rem_euclid` on f32
fn rem_euclid(a: f32, b: f32) -> f32 {
    let r = a % b;
    return select(r, r + abs(b), r < 0.0);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let cell = b.turn.z;
    let x = f32(id.x) + 0.5;
    let y = f32(settings.header.first_row + id.y) + 0.5;
    let u = x * b.turn.x + y * b.turn.y;
    let v = y * b.turn.x - x * b.turn.y;
    let in_u = rem_euclid(u, cell);
    let in_v = rem_euclid(v, cell);
    let near_u = min(in_u, cell - in_u);
    let near_v = min(in_v, cell - in_v);
    var edge: f32;
    if b.shape.x == 0.0 {
        edge = b.turn.w - abs(in_u - cell / 2.0);
    } else if b.shape.x == 3.0 {
        edge = b.turn.w - min(near_u, near_v);
    } else if b.shape.x == 2.0 {
        edge = b.shape.y - length(vec2<f32>(in_u - cell / 2.0, in_v - cell / 2.0));
    } else {
        let even = rem_euclid(floor(u / cell) + floor(v / cell), 2.0) == 0.0;
        edge = min(near_u, near_v) * select(-1.0, 1.0, even);
    }
    var coverage: f32;
    if b.shape.z == 0.0 {
        coverage = select(0.0, 1.0, edge > 0.0);
    } else {
        coverage = clamp(edge / b.shape.z + 0.5, 0.0, 1.0);
    }
    store(i, b.background.rgb + (b.foreground.rgb - b.background.rgb) * coverage);
}

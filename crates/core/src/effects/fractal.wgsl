// `fractal` on the GPU: the CPU's orbit of z -> z^2 + c in single precision, each product rounded on its own (`times`), from the start
// points the CPU worked out (`data`: the point of each column, then of each canvas row, then the colour stops). A point that has not
// escaped after `iterations` steps is the interior; one that has takes the colour at its smooth escape count over the period, going
// forward and back along the colours.

struct Block {
    shape: vec4<f32>,   // x, y: the Julia constant, z: period
    interior: vec4<f32>,
    count: vec4<u32>,   // x: iterations, y: 1 for a Julia set, z: colour stops
}

const ESCAPE_SQUARED: f32 = 65536.0; // 256 ^ 2
const LOG_ESCAPE: f32 = 5.545177444479562; // ln 256

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let head = settings.header;
    let point = vec2<f32>(data[id.x], data[head.canvas_width + head.first_row + id.y]);
    var z = select(vec2<f32>(0.0), point, b.count.y == 1u);
    let c = select(point, b.shape.xy, b.count.y == 1u);
    var out = b.interior.rgb;
    for (var n = 1u; n <= b.count.x; n++) {
        z = vec2<f32>(times(z.x, z.x) - times(z.y, z.y) + c.x, times(times(2.0, z.x), z.y) + c.y);
        let squared = times(z.x, z.x) + times(z.y, z.y);
        if squared > ESCAPE_SQUARED {
            let log_radius = 0.5 * log(squared);
            let count = f32(n) + 1.0 - log2(log_radius / LOG_ESCAPE);
            let r = count / b.shape.z;
            let wrapped = r - 2.0 * floor(r / 2.0);
            let position = 1.0 - abs(1.0 - wrapped);
            out = ramp_at(head.canvas_width + head.canvas_height, b.count.z, clamp(position, 0.0, 1.0));
            break;
        }
    }
    store(i, out);
}

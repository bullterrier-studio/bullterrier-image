// `ntsc` on the GPU: the colour axes I and Q of each pixel (YIQ, the CPU's own matrices) are smeared along the row by the CPU's weights
// (`data`, at distance 0, 1, ...), the brightness stays, and the colour's subcarrier crawls into it: a quarter turn a column, a half
// turn a line.

struct Block {
    crawl: vec4<f32>,    // x: crawl, y: cos, z: sin of the phase
    smear: vec4<u32>,    // x: reach of the weights, y: the edge rule (`locate`)
    to_yiq: array<vec4<f32>, 3>,   // the columns of the matrix from rgb
    to_rgb: array<vec4<f32>, 3>,   // and back
}

fn yiq_of(c: vec3<f32>) -> vec3<f32> {
    let b = settings.block;
    return b.to_yiq[0].xyz * c.x + b.to_yiq[1].xyz * c.y + b.to_yiq[2].xyz * c.z;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let head = settings.header;
    let reach = i32(b.smear.x);
    var colour = vec2<f32>(0.0);
    for (var tap = -reach; tap <= reach; tap++) {
        let column = u32(locate(i32(id.x) + tap, i32(head.width), i32(b.smear.y)));
        colour += data[u32(abs(tap))] * yiq_of(source[id.y * head.width + column].rgb).yz;
    }
    let luma = yiq_of(source[i].rgb).x;
    let cos_t = b.crawl.y;
    let sin_t = b.crawl.z;
    var turn = vec2<f32>(cos_t, sin_t);
    switch id.x % 4u {
        case 1u: { turn = vec2<f32>(-sin_t, cos_t); }
        case 2u: { turn = vec2<f32>(-cos_t, -sin_t); }
        case 3u: { turn = vec2<f32>(sin_t, -cos_t); }
        default: {}
    }
    let line = select(-1.0, 1.0, (head.first_row + id.y) % 2u == 0u);
    let crawl = b.crawl.x * line * (colour.x * turn.x + colour.y * turn.y);
    let yiq = vec3<f32>(luma + crawl, colour);
    let rgb = b.to_rgb[0].xyz * yiq.x + b.to_rgb[1].xyz * yiq.y + b.to_rgb[2].xyz * yiq.z;
    store(i, clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)));
}

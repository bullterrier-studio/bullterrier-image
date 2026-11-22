// `vignette` on the GPU: the CPU's superellipse distance from the middle of the canvas, a smoothstep rise, a mix toward the tint.

struct Block {
    shape: vec4<f32>, // amount, radius, feather, exponent (2 / roundness; 0 = a rectangle)
    reach: vec4<f32>, // x: 1 / exponent (0 for a rectangle)
    tint: vec4<f32>,
}

// how far the pixel `index` of an axis of `length` pixels is from its middle: 0 in the middle, near 1 at the edge
fn offset(index: f32, length: f32) -> f32 {
    return abs(2.0 * index + 1.0 - length) / length;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let head = settings.header;
    let down = offset(f32(head.first_row + id.y), f32(head.canvas_height));
    let across = offset(f32(id.x), f32(head.canvas_width));
    let far = max(down, across);
    let near = min(down, across);
    let b = settings.block;
    var distance = far;
    if b.shape.w > 0.0 {
        if far > 0.0 {
            // the CPU divides by the larger offset first so the power cannot underflow; pow(0, y) is undefined in WGSL
            let ratio = near / far;
            let raised = select(pow(ratio, b.shape.w), 0.0, ratio <= 0.0);
            distance = far * pow((raised + 1.0) * 0.5, b.reach.x);
        } else {
            distance = 0.0;
        }
    }
    var t = 0.0;
    if b.shape.z > 0.0 {
        t = clamp((distance - b.shape.y) / b.shape.z, 0.0, 1.0);
    } else if distance > b.shape.y {
        t = 1.0;
    }
    let shade = b.shape.x * t * t * (3.0 - 2.0 * t);
    store(i, source[i].rgb * (1.0 - shade) + b.tint.rgb * shade);
}

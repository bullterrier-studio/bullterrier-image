// `pixel_grid` on the GPU: the share of each row and of each column's stripes a pixel lies in, worked out on the CPU in f64
// (`data`: the rows, then red, green, blue for each column), mixed in with `amount`.

struct Block {
    mix: vec4<f32>, // x: amount, y: the divisor of keep_brightness (1 when off)
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let row = data[id.y];
    let at = settings.header.height + 3u * id.x;
    let share = row * vec3<f32>(data[at], data[at + 1u], data[at + 2u]);
    let gain = (1.0 - settings.block.mix.x * (1.0 - share)) / settings.block.mix.y;
    store(i, clamp(source[i].rgb * gain, vec3<f32>(0.0), vec3<f32>(1.0)));
}

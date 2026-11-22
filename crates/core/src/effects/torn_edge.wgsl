// `torn_edge` on the GPU: a pixel closer to a side than that side's distance (the CPU's noise, in `data`: top and bottom a value a
// column, then left and right a value a canvas row) shows the fill. The comparisons are the CPU's, in f32, on whole numbers.

struct Block {
    fill: vec4<f32>,
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let head = settings.header;
    let row = head.first_row + id.y;
    let column = id.x;
    let width = head.canvas_width;
    let height = head.canvas_height;
    let top = data[column];
    let bottom = data[width + column];
    let left = data[2u * width + row];
    let right = data[2u * width + height + row];
    let torn = f32(row) < top || f32(height - 1u - row) < bottom || f32(column) < left || f32(width - 1u - column) < right;
    store(i, select(source[i].rgb, settings.block.fill.rgb, torn));
}

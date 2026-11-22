// `dead_pixels` on the GPU: the cell of the canvas the pixel lies in (`size` px square, row after row of `columns` cells) is looked up
// in `data`, which the CPU's own shuffle filled: 0 sound (the pixel stays), 1 stuck (the colour), 2 dead (black).

struct Block {
    color: vec4<f32>,
    cells: vec4<u32>, // x: the cell's size in px, y: cells in a row
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let row = settings.header.first_row + id.y;
    let state = u32(data[(row / b.cells.x) * b.cells.y + id.x / b.cells.x]);
    var out = source[i].rgb;
    if state == 1u {
        out = b.color.rgb;
    } else if state == 2u {
        out = vec3<f32>(0.0);
    }
    store(i, out);
}

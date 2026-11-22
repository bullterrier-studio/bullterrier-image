// The last pass of a histogram: thread b adds, in group order, the counts of bin b that histogram.wgsl wrote for each group
// (`source`), and writes the total as two exact floats: its high 16 bits in x and its low 16 in y of `dest[b]` (a float holds only 2^24 exactly, a picture may have more pixels).

struct Block {
    how: vec4<u32>, // x: bins, y: groups
}

@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) t: u32) {
    let bins = settings.block.how.x;
    if t < bins {
        var total = 0u;
        for (var g = 0u; g < settings.block.how.y; g++) {
            total += u32(source[g * bins + t].x);
        }
        dest[t] = vec4<f32>(f32(total >> 16u), f32(total & 0xffffu), 0.0, 0.0);
    }
}

// `interlace` on the GPU: the rows of one field slid sideways by a distance that may lie between two pixels, read linearly
// between them (`ops/shift.rs`, a move of 0 down: its vertical weights are 1 and 0, so a row reads only itself), with the
// rule for what lies beyond the edge (`ops/edge.rs` `locate`). The source column of pixel x is x + floor(-offset), the share of
// the next one the fraction of -offset, both worked out once on the CPU in f64.

struct Block {
    reading: vec4<f32>, // x: the share of the next pixel along
    ints: vec4<i32>,    // the whole part of the source column's offset; the edge rule (0 reflect, 1 nearest, 2 mirror, 3 wrap); the remainder of a slid row's canvas row by 2
}

// the column of the row that lies at `index`, however far outside it (`Extension::locate`)
fn locate(index: i32, len: i32, rule: i32) -> i32 {
    if rule == 1 {
        return clamp(index, 0, len - 1);
    }
    if rule == 3 {
        return ((index % len) + len) % len;
    }
    if rule == 0 {
        let at = ((index % (2 * len)) + 2 * len) % (2 * len);
        return select(2 * len - 1 - at, at, at < len);
    }
    if len == 1 {
        return 0;
    }
    let period = 2 * (len - 1);
    let at = ((index % period) + period) % period;
    return select(period - at, at, at < len);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let i = pixel_index(id);
    let b = settings.block;
    let row = (settings.header.first_row + id.y) % 2u;
    if i32(row) != b.ints.z {
        store(i, source[i].rgb);
        return;
    }
    let len = i32(settings.header.width);
    let at = i32(id.x) + b.ints.x;
    let line = id.y * settings.header.width;
    let near = source[line + u32(locate(at, len, b.ints.y))].rgb;
    let t = b.reading.x;
    if t == 0.0 {
        store(i, near);
        return;
    }
    let far = source[line + u32(locate(at + 1, len, b.ints.y))].rgb;
    store(i, near * (1.0 - t) + far * t);
}

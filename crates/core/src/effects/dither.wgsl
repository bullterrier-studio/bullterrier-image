// `dither`'s ordered and noise methods on the GPU (`ops/dither.rs` `ordered`): each dot (the average of a `cell` square of
// pixels, summed in the CPU's order) is matched to the palette in its space, Knoll's mix of up to 16 candidates is put in
// order of lightness, and the pixel's rank in the screen picks one. `data` holds, by the palette's size n: the palette in the
// matching space (3 n), the palette as sRGB (3 n), the palette indices darkest first (n), the place of each in that order
// (n), then the screen's ranks (the tile, row after row). All indices are exact whole numbers in f32 (at most 65535).

struct Block {
    grid: vec4<u32>,  // cell, dot rows, dot columns, palette size n
    pick: vec4<u32>,  // candidates, ranks per screen, noise seed, 1 if the screen is noise
    tile: vec4<u32>,  // screen rows, screen columns, shift down, shift right (in dots)
    mode: vec4<u32>,  // x: 1 to match in Oklab
}

// Ottosson's Oklab (`ops/colorspace.rs`); a cube root of 0 is 0, which `pow` leaves undefined
fn cbrt(x: vec3<f32>) -> vec3<f32> {
    return select(vec3<f32>(0.0), pow(x, vec3<f32>(1.0 / 3.0)), x > vec3<f32>(0.0));
}

fn to_oklab(c: vec3<f32>) -> vec3<f32> {
    let l = srgb_to_linear(c);
    let cones = cbrt(vec3<f32>(
        0.41222147 * l.x + 0.53633255 * l.y + 0.051445995 * l.z,
        0.2119035 * l.x + 0.6806995 * l.y + 0.10739696 * l.z,
        0.08830246 * l.x + 0.28171885 * l.y + 0.6299787 * l.z,
    ));
    return vec3<f32>(
        0.21045426 * cones.x + 0.7936178 * cones.y - 0.004072047 * cones.z,
        1.9779985 * cones.x - 2.4285922 * cones.y + 0.4505937 * cones.z,
        0.025904037 * cones.x + 0.78277177 * cones.y - 0.80867577 * cones.z,
    );
}

fn palette_at(i: u32) -> vec3<f32> {
    return vec3<f32>(data[3u * i], data[3u * i + 1u], data[3u * i + 2u]);
}

// the palette index of the colour nearest to `point`, the lower index of a tie (`nearest_one`)
fn nearest(point: vec3<f32>, n: u32) -> u32 {
    var best = 3.4e38;
    var found = 0u;
    for (var k = 0u; k < n; k++) {
        let d = point - palette_at(k);
        let distance = d.x * d.x + d.y * d.y + d.z * d.z;
        if distance < best {
            best = distance;
            found = k;
            if distance <= 0.0 {
                break;
            }
        }
    }
    return found;
}

// the dot of the cell `cell` x `cell` that holds the pixel: the sum in reading order over the area
fn dot_of(x: u32, y: u32, cell: u32) -> vec3<f32> {
    let w = settings.header.width;
    if cell == 1u {
        return source[y * w + x].rgb;
    }
    let top = (y / cell) * cell;
    let left = (x / cell) * cell;
    let bottom = min(top + cell, settings.header.height);
    let right = min(left + cell, w);
    var sum = vec3<f32>(0.0);
    for (var row = top; row < bottom; row++) {
        for (var column = left; column < right; column++) {
            sum += source[row * w + column].rgb;
        }
    }
    return sum / f32((bottom - top) * (right - left));
}

fn back(at: u32, shift: u32, side: u32) -> u32 {
    return (at + side - shift) % side;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let b = settings.block;
    let n = b.grid.w;
    let cell = b.grid.x;
    let row = id.y / cell;
    let column = id.x / cell;
    var goal = dot_of(id.x, id.y, cell);
    if b.mode.x == 1u {
        goal = to_oklab(goal);
    }
    let steps = b.pick.x;
    var mixed: array<u32, 16>;
    var owed = vec3<f32>(0.0);
    for (var c = 0u; c < steps; c++) {
        let index = nearest(goal + owed, n);
        mixed[c] = u32(data[7u * n + index]);
        owed += goal - palette_at(index);
    }
    // sorted, the candidates run from the darkest to the lightest
    for (var i = 1u; i < steps; i++) {
        let held = mixed[i];
        var j = i;
        while j > 0u && mixed[j - 1u] > held {
            mixed[j] = mixed[j - 1u];
            j--;
        }
        mixed[j] = held;
    }
    var rank: u32;
    if b.pick.w == 1u {
        let word = mix2(b.pick.z, back(row, b.tile.z, b.tile.x), back(column, b.tile.w, b.tile.y));
        rank = word.y >> 16u; // the top 16 bits of the 64-bit word
    } else {
        rank = u32(data[8u * n + back(row, b.tile.z, b.tile.x) * b.tile.y + back(column, b.tile.w, b.tile.y)]);
    }
    let position = (2u * rank + 1u) * steps / (2u * b.pick.y);
    let chosen = u32(data[6u * n + mixed[position]]);
    let colour = vec3<f32>(data[3u * n + 3u * chosen], data[3u * n + 3u * chosen + 1u], data[3u * n + 3u * chosen + 2u]);
    store(pixel_index(id), colour);
}

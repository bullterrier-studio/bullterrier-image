// `halftone` on the GPU (`effects/halftone.rs` `draw`, `ops/inks.rs`, `ops/realism.rs`): each pixel works out, for each ink,
// the cell of the turned screen it lies in, the tone at that cell's centre (the picture blurred there), the ink's amount, the
// dot's level and its ramp, with the CPU's f32 maths; the inks are then printed over the paper. The CPU keeps what its own code
// made: the screens' angles and shifts, the inks' transmissions and the spot's tables (`data`, after the blur's weights, or the
// blurred picture when the CPU made it).

struct Block {
    cell: vec4<f32>,   // the cell side, 1 / cell, 1 / feather, 0
    kinds: vec4<u32>,  // the separation (0 none, 1 cmyk, 2 spot, 3 rgb), the number of screens, the geometry (0 grid, 1 wave), the spot (see below)
    blur: vec4<u32>,   // 0 none, 1 summed here, 2 the CPU's picture; the reach; the edge rule; the offset of the weights or the picture in `data`
    spots: vec4<u32>,  // the offset and length of the spot's numbers in `data`; a matrix's rows and columns
    look: vec4<f32>,   // the ellipse's aspect, the wave's amplitude and wavelength, `ink_floor`
    paper: vec4<f32>,  // the paper; 1 if the ink is bright (`paper` darker than `paper_threshold`)
    dot: vec4<f32>,    // the dot's own colour; 1 if there is one
    real: vec4<f32>,   // dot gain, ink noise, the constant beyond the edge; the seed's bits
    fit: vec4<f32>,    // the floor of a ratio, the sweeps, the snap, the index of black
    screens: array<vec4<f32>, 4>, // for each screen: the sine and cosine of its angle, its shift
    through: array<vec4<f32>, 4>, // for each ink: the share of each channel's light that gets through it
}

const TURN: f32 = 6.283185307179586;
const FULL: f32 = 2.0;

// `times` (prelude.wgsl) rounds a product on its own, as Rust does, so a pixel lands on the CPU's side of a cell's edge.

// ---- the geometry of a screen (`Geometry`): the pixel's place in cells and back

fn forward(u: f32, v: f32) -> vec2<f32> {
    let b = settings.block;
    if b.kinds.z == 1u {
        return vec2<f32>(u, v + b.look.y * sin(TURN * u / b.look.z));
    }
    return vec2<f32>(u, v);
}

fn back(cu: f32, cv: f32) -> vec2<f32> {
    let b = settings.block;
    if b.kinds.z == 1u {
        return vec2<f32>(cu, cv - b.look.y * sin(TURN * cu / b.look.z));
    }
    return vec2<f32>(cu, cv);
}

// ---- the spot (`Spot`)

fn number(at: u32) -> f32 {
    return data[settings.block.spots.x + at];
}

fn place_of(d: f32, count: u32) -> u32 {
    let at = floor((f32(d) + 0.5) * f32(count) + 0.001);
    return min(u32(max(at, 0.0)), count - 1u);
}

fn euclidean_value(du: f32, dv: f32) -> f32 {
    let s = (cos(TURN * du) + cos(TURN * dv)) / 2.0;
    if s >= 0.0 {
        return sqrt(1.0 - s) * 0.3183098861837907;
    }
    return (2.0 - sqrt(1.0 + s)) * 0.3183098861837907;
}

fn spot_value(du: f32, dv: f32) -> f32 {
    let b = settings.block;
    switch b.kinds.w {
        case 1u: { return abs(dv); }
        case 2u: { return max(abs(du), abs(dv)); }
        case 3u: { return sqrt(du * du / (b.look.x * b.look.x) + dv * dv); }
        case 4u: { return (abs(du) + abs(dv)) * 0.7071067811865476; }
        case 5u: { return euclidean_value(du, dv); }
        case 6u: {
            let rows = b.spots.z;
            let columns = b.spots.w;
            let row = place_of(dv, rows);
            let column = place_of(du, columns);
            let count = f32(rows * columns);
            return (count - number(row * columns + column) - 0.5) / count;
        }
        default: { return sqrt(du * du + dv * dv); }
    }
}

// the area of the unit cell a circle of radius r covers (`clipped_circle`)
fn clipped_circle(r: f32) -> f32 {
    let cap = r * r * acos(0.5 / r) - 0.5 * sqrt(max(r * r - 0.25, 0.0));
    return 3.141592653589793 * r * r - 4.0 * cap;
}

// the value at quantile `tone` of the sorted values
fn quantile(tone: f32) -> f32 {
    if tone <= 0.0 {
        return 0.0;
    }
    let length = settings.block.spots.y;
    let at = clamp(tone * f32(length) - 0.5, 0.0, f32(length - 1u));
    let below = u32(floor(at));
    let above = min(below + 1u, length - 1u);
    return number(below) + fract(at) * (number(above) - number(below));
}

// the spot's value at which a dot covers the share `tone` of a cell (`Spot::level`)
fn spot_level(amount: f32) -> f32 {
    let tone = clamp(amount, 0.0, 1.0);
    if tone >= 1.0 {
        return FULL;
    }
    switch settings.block.kinds.w {
        case 1u: { return tone / 2.0; }
        case 2u: { return sqrt(tone) / 2.0; }
        case 3u, 5u: { return quantile(tone); }
        case 4u: {
            var reach = 1.0 - sqrt((1.0 - tone) / 2.0);
            if tone <= 0.5 {
                reach = sqrt(tone / 2.0);
            }
            return reach * 0.7071067811865476;
        }
        case 6u: { return tone; }
        default: {
            if tone <= 0.7853981633974483 {
                return sqrt(tone / 3.141592653589793);
            }
            var low = 0.5;
            var high = 0.7071067811865476;
            for (var step = 0u; step < 22u; step++) {
                let middle = (low + high) / 2.0;
                if clipped_circle(middle) < tone {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            return (low + high) / 2.0;
        }
    }
}

// ---- the inks (`Inks`)

fn print_inks(coverages: vec4<f32>) -> vec3<f32> {
    let b = settings.block;
    if b.kinds.x == 3u {
        return coverages.xyz;
    }
    var light = b.paper.xyz;
    for (var ink = 0u; ink < 4u; ink++) {
        if ink < b.kinds.y {
            light *= vec3<f32>(1.0) - coverages[ink] * (vec3<f32>(1.0) - b.through[ink].xyz);
        }
    }
    return light;
}

fn ratio(colour: vec3<f32>) -> vec3<f32> {
    let b = settings.block;
    return clamp(colour / max(b.paper.xyz, vec3<f32>(b.fit.x)), vec3<f32>(b.fit.x), vec3<f32>(1.0));
}

fn relative(amounts: vec4<f32>, count: u32) -> vec3<f32> {
    let b = settings.block;
    var light = b.paper.xyz;
    for (var ink = 0u; ink < count; ink++) {
        light *= vec3<f32>(1.0) - amounts[ink] * (vec3<f32>(1.0) - b.through[ink].xyz);
    }
    return light / max(b.paper.xyz, vec3<f32>(b.fit.x));
}

fn miss(amounts: vec4<f32>, count: u32, wanted_light: vec3<f32>) -> f32 {
    let d = relative(amounts, count) - wanted_light;
    return d.x * d.x + d.y * d.y + d.z * d.z;
}

// `Inks::fit`: the amounts of the first `free` inks that print `wanted_light`, by sweeps of one ink at a time from three starts
fn fit(wanted_light: vec3<f32>, free: u32, count: u32, start: vec4<f32>) -> vec4<f32> {
    let b = settings.block;
    var best = start;
    var least = 3.4e38;
    for (var attempt = 0u; attempt < 3u; attempt++) {
        var amounts = start;
        for (var k = 0u; k < free; k++) {
            amounts[k] = select(select(0.0, 1.0, attempt == 2u), start[k], attempt == 0u);
        }
        for (var sweep = 0u; sweep < u32(b.fit.y); sweep++) {
            for (var k = 0u; k < free; k++) {
                amounts[k] = 0.0;
                let rest = relative(amounts, count);
                var along = 0.0;
                var square = 0.0;
                for (var c = 0u; c < 3u; c++) {
                    let darkens = rest[c] * (1.0 - b.through[k][c]);
                    along += (rest[c] - wanted_light[c]) * darkens;
                    square += darkens * darkens;
                }
                amounts[k] = select(0.0, clamp(along / square, 0.0, 1.0), square > 0.0);
            }
        }
        for (var k = 0u; k < free; k++) {
            if amounts[k] < b.fit.z {
                amounts[k] = 0.0;
            } else if amounts[k] > 1.0 - b.fit.z {
                amounts[k] = 1.0;
            }
        }
        let now = miss(amounts, count, wanted_light);
        if now < least {
            least = now;
            best = amounts;
        }
    }
    return best;
}

// the amounts of the inks that make `colour` (`Inks::cmyk_amounts`, `coverages`, `rgb_amounts`)
fn ink_amounts(colour: vec3<f32>) -> vec4<f32> {
    let b = settings.block;
    if b.kinds.x == 3u {
        return clamp(vec4<f32>(colour, 0.0), vec4<f32>(0.0), vec4<f32>(1.0));
    }
    let wanted_light = ratio(colour);
    if b.kinds.x == 2u {
        return fit(wanted_light, b.kinds.y, b.kinds.y, vec4<f32>(0.0));
    }
    let darkness = 1.0 - max(wanted_light.x, max(wanted_light.y, wanted_light.z));
    let black_through = b.through[u32(b.fit.w)].xyz;
    let reach = 1.0 - max(black_through.x, max(black_through.y, black_through.z));
    var black = 0.0;
    if reach > 0.0 {
        black = min(darkness / reach, 1.0);
    }
    if black >= 1.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let rest = (vec3<f32>(1.0) - wanted_light - vec3<f32>(darkness)) / max(1.0 - darkness, b.fit.x);
    return fit(wanted_light, 3u, 4u, vec4<f32>(rest, black));
}

// ---- print realism (`Realism::coverage`)

fn coverage(amount: f32, ink: u32, cu: f32, cv: f32) -> f32 {
    let b = settings.block;
    if b.real.x == 0.0 && b.real.y == 0.0 {
        return amount;
    }
    let a = clamp(amount, 0.0, 1.0);
    let gained = a + 4.0 * b.real.x * a * (1.0 - a);
    let draw = unit53(mix4(bitcast<u32>(b.real.w), 1u, ink, bitcast<u32>(cu), bitcast<u32>(cv))) * 2.0 - 1.0;
    return gained + b.real.y * min(gained, 1.0 - gained) * draw;
}

// ---- the tone of the picture at a pixel: the blur the CPU makes (`gaussian_filter`), summed here for a short kernel

fn smoothed(row: u32, column: u32) -> vec3<f32> {
    let b = settings.block;
    let width = settings.header.width;
    let height = settings.header.height;
    if b.blur.x == 0u {
        return source[row * width + column].rgb;
    }
    if b.blur.x == 2u {
        let at = b.blur.w + 3u * (row * width + column);
        return vec3<f32>(data[at], data[at + 1u], data[at + 2u]);
    }
    let reach = i32(b.blur.y);
    let rule = i32(b.blur.z);
    var total = vec3<f32>(0.0);
    for (var down = -reach; down <= reach; down++) {
        let weight_down = data[b.blur.w + u32(abs(down))];
        let at_row = i32(row) + down;
        for (var across = -reach; across <= reach; across++) {
            let weight = weight_down * data[b.blur.w + u32(abs(across))];
            let at_column = i32(column) + across;
            var value = vec3<f32>(b.real.z);
            if rule < 4 || (at_row >= 0 && at_row < i32(height) && at_column >= 0 && at_column < i32(width)) {
                let r = locate(at_row, i32(height), rule);
                let c = locate(at_column, i32(width), rule);
                value = source[u32(r) * width + u32(c)].rgb;
            }
            total += weight * value;
        }
    }
    return total;
}

// `pixel`: a value rounded to the nearest whole pixel (ties to even), not past `last`
fn nearest_pixel(value: f32, last: u32) -> u32 {
    return min(u32(max(round(value), 0.0)), last);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if !inside(id) {
        return;
    }
    let b = settings.block;
    let header = settings.header;
    let cell = b.cell.x;
    let x = f32(id.x);
    let y = f32(header.first_row + id.y);
    let canvas_last = header.canvas_height - 1u;
    let separation = b.kinds.x;
    var coverages = vec4<f32>(0.0);
    var first_sample = vec3<f32>(0.0);
    for (var ink = 0u; ink < b.kinds.y; ink++) {
        let screen = b.screens[ink];
        let sine = screen.x;
        let cosine = screen.y;
        let px = x - screen.z;
        let py = y - screen.w;
        let uv = forward((times(px, cosine) + times(py, sine)) * b.cell.y, (times(py, cosine) - times(px, sine)) * b.cell.y);
        let cu = floor(uv.x) + 0.5;
        let cv = floor(uv.y) + 0.5;
        let centre = back(cu, cv);
        let column = nearest_pixel(times(times(centre.x, cosine) - times(centre.y, sine), cell) + screen.z, header.width - 1u);
        let canvas_row = nearest_pixel(times(times(centre.x, sine) + times(centre.y, cosine), cell) + screen.w, canvas_last);
        let row = clamp(canvas_row, header.first_row, header.first_row + header.height - 1u) - header.first_row;
        let sampled = smoothed(row, column);
        if ink == 0u {
            first_sample = sampled;
        }
        // the ink's amount at the cell, with the press's dot gain and uneven ink
        var wanted: f32;
        if separation == 0u {
            let tone = luminance(sampled);
            wanted = clamp(select(1.0 - tone, tone, b.paper.w > 0.5), 0.0, 1.0);
        } else {
            wanted = ink_amounts(sampled)[ink];
        }
        let amount = coverage(wanted, select(ink, 0u, separation == 0u), cu, cv);
        let radius = spot_level(amount);
        let distance = spot_value(uv.x - cu, uv.y - cv);
        let fade_in = min(radius * cell * 2.0 * b.cell.z, 1.0);
        let ramp = clamp((radius - distance) * cell * b.cell.z + 0.5, 0.0, 1.0) * fade_in;
        let fade_out = min((1.0 - min(amount, 1.0)) * cell * 4.0 * b.cell.z, 1.0);
        coverages[ink] = 1.0 - (1.0 - ramp) * fade_out;
    }
    var printed: vec3<f32>;
    if separation != 0u {
        printed = print_inks(coverages);
    } else if coverages.x > 0.0 {
        var dot = b.dot.xyz;
        if b.dot.w < 0.5 {
            let strongest = max(first_sample.x, max(first_sample.y, max(first_sample.z, b.look.w)));
            dot = clamp(first_sample / strongest, vec3<f32>(0.0), vec3<f32>(1.0));
        }
        printed = b.paper.xyz * (1.0 - coverages.x) + dot * coverages.x;
    } else {
        printed = b.paper.xyz;
    }
    store(pixel_index(id), printed);
}

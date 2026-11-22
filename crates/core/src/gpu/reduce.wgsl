// A reduction of one channel of the picture (`source`) or of the partial results of an earlier pass: sum, minimum or maximum.
// The tree is fixed, so one device gives the same answer every time: each workgroup of 256 threads takes a chunk of 256 x 64
// items; thread t folds items t, t + 256, t + 512 ... of the chunk in that order, then the 256 results are folded in a binary
// tree (stride 128, 64 ... 1). The workgroup's result is `dest[workgroup]`; a second pass reduces the results the same way.

struct Block {
    how: vec4<u32>, // x: the plane (0 red, 1 green, 2 blue, 3 luma), y: the operation (0 sum, 1 min, 2 max), z: how many items
}

const GROUP_ITEMS: u32 = 64u; // constant: items a thread folds, one every 256 apart (reduce.rs `CHUNK` holds it too)
const BIG: f32 = 3.4028234e38; // constant: the largest f32, the identity of min

var<workgroup> scratch: array<f32, 256>;

fn plane_value(c: vec4<f32>, plane: u32) -> f32 {
    if plane == 3u {
        return luminance(c.rgb);
    }
    return c[plane];
}

fn fold(a: f32, b: f32, how: u32) -> f32 {
    if how == 0u {
        return a + b;
    }
    if how == 1u {
        return min(a, b);
    }
    return max(a, b);
}

@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) t: u32, @builtin(workgroup_id) group: vec3<u32>) {
    let plane = settings.block.how.x;
    let how = settings.block.how.y;
    let count = settings.block.how.z;
    var acc = select(select(-BIG, BIG, how == 1u), 0.0, how == 0u);
    let start = group.x * 256u * GROUP_ITEMS;
    for (var k = 0u; k < GROUP_ITEMS; k++) {
        let at = start + k * 256u + t;
        if at < count {
            acc = fold(acc, plane_value(source[at], plane), how);
        }
    }
    scratch[t] = acc;
    for (var stride = 128u; stride > 0u; stride = stride >> 1u) {
        workgroupBarrier();
        if t < stride {
            scratch[t] = fold(scratch[t], scratch[t + stride], how);
        }
    }
    if t == 0u {
        dest[group.x] = vec4<f32>(scratch[0], 0.0, 0.0, 0.0);
    }
}

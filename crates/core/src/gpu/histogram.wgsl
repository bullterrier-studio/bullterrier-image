// The histogram of one channel (`source`, the picture): bin = floor(value x bins) at most bins - 1, as the CPU's (equalize.rs
// `bin_of`). A workgroup of 256 threads counts a chunk of 256 x 64 pixels into a workgroup-local table and writes the
// counts (a group's count of a bin is at most 16384, exact in an f32) at `dest[workgroup x bins + bin]`; histogram_total.wgsl adds the groups.
// Counts are integers, so the answer does not depend on the order the threads count in.

struct Block {
    how: vec4<u32>, // x: the plane (0 red, 1 green, 2 blue, 3 luma), y: bins (1 to 256), z: how many pixels
}

const GROUP_ITEMS: u32 = 64u; // constant: pixels a thread counts, one every 256 apart (histogram.rs `CHUNK` holds it too)

var<workgroup> counts: array<atomic<u32>, 256>;

@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) t: u32, @builtin(workgroup_id) group: vec3<u32>) {
    let plane = settings.block.how.x;
    let bins = settings.block.how.y;
    let count = settings.block.how.z;
    atomicStore(&counts[t], 0u);
    workgroupBarrier();
    let start = group.x * 256u * GROUP_ITEMS;
    for (var k = 0u; k < GROUP_ITEMS; k++) {
        let at = start + k * 256u + t;
        if at < count {
            let c = source[at];
            var value = c[plane];
            if plane == 3u {
                value = luminance(c.rgb);
            }
            // a negative value or NaN is bin 0, as the CPU's saturating cast makes it
            let bin = min(u32(max(value * f32(bins), 0.0)), bins - 1u);
            atomicAdd(&counts[bin], 1u);
        }
    }
    workgroupBarrier();
    if t < bins {
        dest[group.x * bins + t] = vec4<f32>(f32(atomicLoad(&counts[t])), 0.0, 0.0, 0.0);
    }
}

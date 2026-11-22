// `ops/noise.rs` `field_rows` on the GPU: smooth value noise of mean 0 and deviation 1 at the pixel (`row`, `column`) of the canvas. Octave k has its
// lattice points `spacing` px apart (the lattice's first point on the first pixel), valued 2 x unit(mix(seed, [stream, k, lattice row,
// lattice column])) - 1, read by Catmull-Rom cubic convolution through the four points a side; the octaves are summed by weight and the
// sum divided by its own deviation, known from the weights alone. `base` is where the octaves are in `data`: how many, then the spacing
// and weight of each (the CPU works them out in f64, `Fractal::octaves`).

// the weights of the four lattice points read at `t` (0 to 1) past the one at `at`
fn catmull_rom(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    return vec4<f32>(
        0.5 * (-t3 + 2.0 * t2 - t),
        1.0 + t2 * (1.5 * t - 2.5),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    );
}

fn lattice_value(seed: u32, stream: u32, octave: u32, row: u32, column: u32) -> f32 {
    return 2.0 * unit53(mix4(seed, stream, octave, row, column)) - 1.0;
}

fn value_noise(seed: u32, stream: u32, base: u32, row: u32, column: u32) -> f32 {
    let octaves = u32(data[base]);
    var sum = 0.0;
    var variance = 0.0;
    for (var octave = 0u; octave < octaves; octave++) {
        let spacing = data[base + 1u + 2u * octave];
        let weight = data[base + 2u + 2u * octave];
        let across = f32(column) / spacing;
        let down = f32(row) / spacing;
        let first_column = floor(across);
        let first_row = floor(down);
        let along = catmull_rom(across - first_column);
        let below = catmull_rom(down - first_row);
        var read = 0.0;
        for (var r = 0u; r < 4u; r++) {
            var line = 0.0;
            for (var c = 0u; c < 4u; c++) {
                line += along[c] * lattice_value(seed, stream, octave, u32(first_row) + r, u32(first_column) + c);
            }
            read += below[r] * line;
        }
        sum += weight * read;
        variance += weight * weight / 3.0 * dot(along, along) * dot(below, below);
    }
    return select(0.0, sum / sqrt(variance), variance > 0.0);
}

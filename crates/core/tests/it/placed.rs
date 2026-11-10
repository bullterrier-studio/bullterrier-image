//! Effects that depend on where a pixel lies on the canvas (a row's parity, the middle of the picture, a stripe's phase):
//! rows cut from the canvas, with the margin the effect's footprint asks for and told their place, render the same rows
//! as the whole picture, to the bit. The same rows not told their place do not: the known-bad row that shows the check
//! can fail. The whole render is the oracle.

use std::error::Error;

use bullterrier_image::params::Table;
use bullterrier_image::{Inputs, Picture, Place, apply, effect};
use serde_json::{Value, json};

use crate::common::compare::{first_bit_difference, first_difference_within};
use crate::common::pictures::{asset, varied};

/// Odd and even sides that differ; the picture is tall enough for a margin of several rows round a band.
const HEIGHT: usize = 61;
const WIDTH: usize = 22;

/// The effects that read their place and a setting of each that a wrong row or column would show in.
fn cases() -> Vec<(&'static str, Value)> {
    vec![
        (
            "scanlines",
            json!({"spacing": 5.5, "thickness": 0.4, "darkness": 0.6, "offset": 2.5, "softness": 0.5}),
        ),
        (
            "pixel_grid",
            json!({"cell": 5, "gap": 0.3, "amount": 0.8, "keep_brightness": true}),
        ),
        ("pattern", json!({"kind": "grid", "cell": 10.0, "angle": 30.0})),
        ("pattern", json!({"kind": "checks", "cell": 7.0, "angle": 0.0})),
        ("pattern", json!({"kind": "dots", "cell": 9.0, "angle": 17.0})),
        ("pattern", json!({"kind": "stripes", "cell": 6.0, "angle": 90.0})),
        (
            "gradient",
            json!({"kind": "linear", "angle": 33.0, "repeat": 2, "reflect": true}),
        ),
        ("gradient", json!({"kind": "radial", "x": 0.3, "y": 0.2, "radius": 0.6})),
        ("gradient", json!({"kind": "conic", "x": 0.7, "y": 0.4, "angle": 20.0})),
        ("fractal", json!({"kind": "julia", "zoom": 2.0, "x": 0.1})),
        (
            "fractal",
            json!({"kind": "mandelbrot", "zoom": 1.5, "x": -0.5, "iterations": 40}),
        ),
        (
            "lens_flare",
            json!({"x": 0.3, "y": 0.6, "ghosts": 4, "halo": 30.0, "ring": 18.0}),
        ),
        ("interlace", json!({"offset": 3.5, "field": "even"})),
        ("interlace", json!({"offset": -2.0, "field": "odd"})),
        ("ntsc", json!({"sigma": 2.0, "crawl": 0.5, "phase": 0.2})),
        (
            "aurora",
            json!({"seed": 3, "base": 0.6, "height": 40.0, "waviness": 8.0}),
        ),
        ("torn_edge", json!({"width": 6.0, "roughness": 0.9, "seed": 4})),
        (
            "blend",
            json!({"image": "asset", "mode": "multiply", "fit": "cover", "at": "center"}),
        ),
        (
            "blend",
            json!({"image": "asset", "mode": "screen", "fit": "contain", "at": "bottom_right", "margin": 3.0}),
        ),
        ("ripple", json!({"amplitude": 5.0, "wavelength": 13.0, "angle": 30.0})),
        ("chromatic_aberration", json!({"amount": 6.0, "x": 0.3, "y": 0.6})),
        ("halftone", json!({"sigma": 0.0, "cell": 6, "angle": 30.0})),
        (
            "halftone",
            json!({"sigma": 0.0, "cell": 5, "angle": 15.0, "separation": "cmyk", "paper": "#ffffff"}),
        ),
    ]
}

/// The effects that draw from a seed (noise, marks, glitches) and a setting dense enough that its marks fall in the rows
/// of the bands and cross their edges.
fn seeded_cases() -> Vec<(&'static str, Value)> {
    vec![
        ("grain", json!({"size": 0.0, "amount": 0.3, "color": true, "seed": 5})),
        (
            "grain",
            json!({"size": 3.0, "amount": 0.3, "color": true, "edge": "reflect", "seed": 6}),
        ),
        (
            "grain",
            json!({"size": 2.0, "distribution": "uniform", "edge": "wrap", "seed": 7}),
        ),
        (
            "vhs",
            json!({
                "tracking": 6.0, "bands": 6, "band_height": 14.0, "band_static": 0.6, "dropouts": 30, "noise": 0.2,
                "noise_size": 3.0, "wobble": 2.0, "wobble_size": 4.0, "bleed": 2.0, "edge": "wrap", "seed": 3
            }),
        ),
        (
            "vhs",
            json!({"tracking": 9.0, "bands": 4, "dropouts": 10, "noise": 0.1, "noise_size": 1.0, "wobble": 1.0, "seed": 8}),
        ),
        (
            "fog",
            json!({"scale": 9.0, "octaves": 3, "amount": 1.0, "gradient": 0.4, "coverage": 0.5, "seed": 2}),
        ),
        (
            "noise_field",
            json!({"kind": "value", "scale": 8.0, "octaves": 3, "warp": 0.0, "seed": 2}),
        ),
        (
            "noise_field",
            json!({"kind": "value", "scale": 8.0, "octaves": 3, "warp": 4.0, "seed": 2}),
        ),
        (
            "paper_texture",
            json!({"kind": "paper", "amount": 0.3, "scale": 5.0, "octaves": 3, "seed": 4}),
        ),
        (
            "paper_texture",
            json!({"kind": "canvas", "amount": 0.3, "spacing": 5.0, "fibre": 0.4, "seed": 4}),
        ),
        (
            "paper_texture",
            json!({"kind": "linen", "amount": 0.3, "spacing": 4.0, "fibre": 0.4, "seed": 5}),
        ),
        (
            "flow_lines",
            json!({"density": 8.0, "length": 40.0, "step": 1.5, "scale": 15.0, "thickness": 2.0, "seed": 4}),
        ),
        (
            "dust_scratches",
            json!({"density": 5.0, "size": 20.0, "hairs": 0.5, "scratches": 8, "angle": 20.0, "width": 2.0, "seed": 3}),
        ),
        (
            "particles",
            json!({"density": 150.0, "size": 8.0, "length": 9.0, "angle": 70.0, "layers": 3, "seed": 5}),
        ),
        (
            "dead_pixels",
            json!({"count": 150, "size": 2, "stuck": 0.5, "color": "#ff8000", "seed": 4}),
        ),
        (
            "slice_shift",
            json!({"slices": 12, "amount": 6.0, "bands": 5, "band_min_height": 3.0, "band_max_height": 9.0, "seed": 5}),
        ),
        (
            "light_leak",
            json!({"blobs": 5, "size": 20.0, "angle": 40.0, "amount": 1.0, "seed": 2}),
        ),
        (
            "lens_drops",
            json!({"count": 40, "radius": 8.0, "refraction": 1.5, "blur": 1.0, "highlight": 0.5, "seed": 3}),
        ),
        (
            "lens_drops",
            json!({"count": 30, "radius": 6.0, "refraction": 0.5, "blur": 0.0, "interpolation": "bicubic", "edge": "mirror", "seed": 7}),
        ),
    ]
}

/// `name` run on rows `from..to` of `picture`, told where they lie on the canvas if `placed`.
fn run(
    name: &str,
    params: &Value,
    picture: &[f32],
    (from, to): (usize, usize),
    placed: bool,
    (height, width): (usize, usize),
) -> Result<Vec<f32>, Box<dyn Error>> {
    let asset = asset();
    let mut inputs = Inputs::default().with_image("image", &asset);
    if placed {
        inputs = inputs.with_place(Place::new(height, width, from));
    }
    let band = Picture::new(to - from, width, &picture[from * width * 3..to * width * 3])?;
    let table = effect(name)?.params().ok_or("no parameters")?;
    let completed = table.check(name, params.as_object().ok_or("not an object")?)?;
    Ok(apply(name, &band, Value::Object(completed), &inputs)?.into_values())
}

/// The rows of margin `name` asks for with `params`; none if it says `Whole`.
fn margin(name: &str, params: &Value) -> Result<Option<usize>, Box<dyn Error>> {
    let one = effect(name)?;
    let table: Table = one.params().ok_or("no parameters")?;
    let given = params.as_object().ok_or("not an object")?;
    let completed = table.check(name, given)?;
    Ok(one.footprint(&Value::Object(completed))?.margin(1.0))
}

/// Rows `from..to` of the whole render of `cases` are what a band told its place renders (with the margin the footprint asks),
/// and not what a band not told renders.
fn check(
    cases: &[(&str, Value)],
    (height, width): (usize, usize),
    bands: [(usize, usize); 4],
    tolerance: f32,
) -> Result<(), Box<dyn Error>> {
    let picture = varied(height, width);
    for (name, params) in cases {
        let reads = margin(name, params)?.ok_or_else(|| format!("{name} {params} says Whole"))?;
        let whole = run(name, params, &picture, (0, height), true, (height, width))?;
        for (from, to) in bands {
            let (low, high) = (from.saturating_sub(reads), (to + reads).min(height));
            let band = run(name, params, &picture, (low, high), true, (height, width))?;
            let own = &band[(from - low) * width * 3..(to - low) * width * 3];
            let wanted = &whole[from * width * 3..to * width * 3];
            let differs = if tolerance == 0.0 {
                first_bit_difference(own, wanted)
            } else {
                first_difference_within(own, wanted, tolerance)
            };
            assert_eq!(differs, None, "{name} {params}: rows {from}..{to} told their place");
        }
        // known bad: the same rows not told where they lie take themselves for the canvas
        let (from, to) = bands[1];
        let (low, high) = (from - reads.min(from), to + reads);
        let alone = run(name, params, &picture, (low, high), false, (height, width))?;
        let own = &alone[(from - low) * width * 3..(to - low) * width * 3];
        assert!(
            first_bit_difference(own, &whole[from * width * 3..to * width * 3]).is_some(),
            "{name} {params}: rows not told their place matched the whole, so the test proves nothing"
        );
    }
    Ok(())
}

#[test]
fn rows_told_their_place_are_the_same_rows_of_the_whole_and_rows_not_told_are_not() -> Result<(), Box<dyn Error>> {
    let every: Vec<_> = cases().into_iter().chain(seeded_cases()).collect();
    check(
        &every,
        (HEIGHT, WIDTH),
        [(0, 9), (13, 30), (30, HEIGHT), (HEIGHT - 1, HEIGHT)],
        0.0,
    )
}

/// A canvas tall enough that a band is far from both edges, for what a short canvas cannot show: a window of noise that is
/// less than the canvas, a kernel long enough to be summed by a transform.
#[test]
fn noise_made_for_a_window_of_a_tall_canvas_is_the_same_rows_of_the_whole() -> Result<(), Box<dyn Error>> {
    let cases = [
        (
            "noise_field",
            json!({"kind": "value", "scale": 20.0, "octaves": 3, "warp": 4.0, "seed": 2}),
        ),
        (
            "grain",
            json!({"size": 3.0, "amount": 0.3, "edge": "reflect", "seed": 6}),
        ),
        (
            "flow_lines",
            json!({"density": 3.0, "length": 40.0, "step": 1.5, "scale": 30.0, "thickness": 2.0, "seed": 4}),
        ),
        ("fog", json!({"scale": 40.0, "octaves": 3, "amount": 1.0, "seed": 2})),
        (
            "vhs",
            json!({"noise": 0.2, "noise_size": 3.0, "wobble": 2.0, "wobble_size": 6.0, "seed": 3}),
        ),
    ];
    check(&cases, (400, 10), TALL_BANDS, 0.0)
}

const TALL_BANDS: [(usize, usize); 4] = [(0, 9), (150, 190), (260, 400), (399, 400)];

/// Grain clumped by a kernel of 32 px each side is blurred by a Fourier transform down the window's columns, whose rounding
/// depends on the window: the rows agree to the blur's own promise (1e-5 of the whole's), not to the bit.
#[test]
fn grain_clumped_beyond_the_reach_of_a_summed_kernel_is_the_same_rows_of_the_whole_to_the_blurs_rounding()
-> Result<(), Box<dyn Error>> {
    let cases = [(
        "grain",
        json!({"size": 16.0, "amount": 0.3, "edge": "reflect", "seed": 6}),
    )];
    check(&cases, (400, 10), TALL_BANDS, 1e-5)
}

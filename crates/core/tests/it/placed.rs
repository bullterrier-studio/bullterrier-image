//! Effects that depend on where a pixel lies on the canvas (a row's parity, the middle of the picture, a stripe's phase):
//! rows cut from the canvas, with the margin the effect's footprint asks for and told their place, render the same rows
//! as the whole picture, to the bit. The same rows not told their place do not: the known-bad row that shows the check
//! can fail. The whole render is the oracle.

use std::error::Error;

use bullterrier_image::params::Table;
use bullterrier_image::{Inputs, Picture, Place, apply, effect};
use serde_json::{Value, json};

use crate::common::compare::first_bit_difference;
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
    ]
}

/// `name` run on rows `from..to` of `picture`, told where they lie on the canvas if `placed`.
fn run(
    name: &str,
    params: &Value,
    picture: &[f32],
    (from, to): (usize, usize),
    placed: bool,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let asset = asset();
    let mut inputs = Inputs::default().with_image("image", &asset);
    if placed {
        inputs = inputs.with_place(Place::new(HEIGHT, WIDTH, from));
    }
    let band = Picture::new(to - from, WIDTH, &picture[from * WIDTH * 3..to * WIDTH * 3])?;
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

#[test]
fn rows_told_their_place_are_the_same_rows_of_the_whole_and_rows_not_told_are_not() -> Result<(), Box<dyn Error>> {
    let picture = varied(HEIGHT, WIDTH);
    for (name, params) in cases() {
        let reads = margin(name, &params)?.ok_or_else(|| format!("{name} {params} says Whole"))?;
        let whole = run(name, &params, &picture, (0, HEIGHT), true)?;
        for (from, to) in [(0, 9), (13, 30), (30, HEIGHT), (HEIGHT - 1, HEIGHT)] {
            let (low, high) = (from.saturating_sub(reads), (to + reads).min(HEIGHT));
            let band = run(name, &params, &picture, (low, high), true)?;
            let own = &band[(from - low) * WIDTH * 3..(to - low) * WIDTH * 3];
            assert_eq!(
                first_bit_difference(own, &whole[from * WIDTH * 3..to * WIDTH * 3]),
                None,
                "{name} {params}: rows {from}..{to} told their place"
            );
        }
        // known bad: the same rows not told where they lie take themselves for the canvas
        let (low, high) = (13 - reads.min(13), 30 + reads);
        let alone = run(name, &params, &picture, (low, high), false)?;
        let own = &alone[(13 - low) * WIDTH * 3..(30 - low) * WIDTH * 3];
        assert!(
            first_bit_difference(own, &whole[13 * WIDTH * 3..30 * WIDTH * 3]).is_some(),
            "{name} {params}: rows not told their place matched the whole, so the test proves nothing"
        );
    }
    Ok(())
}

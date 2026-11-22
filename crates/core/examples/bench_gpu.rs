//! The GPU backend's timings at 1920 x 1080 (`uv run poe bench-gpu`, a release build): every effect with a kernel and the dither tool's four
//! recipes on the CPU (one thread, every core) and on the GPU (end to end: upload, steps, readback; and on the device alone:
//! a picture already there, the steps only). A Markdown table on stdout, the minimum of several runs after a warm-up.
//! `docs/design-decisions.md` ("GPU backend") records a run.

use std::time::Instant;

use bullterrier_image::{Error, Gpu, Options, Photo, Picture, Presets, Recipe, Step, effect, effects};
use pollster::block_on;

const HEIGHT: usize = 1080; // constant: the benchmark's picture, 1080p
const WIDTH: usize = 1920; // constant: the benchmark's picture, 1080p
const RUNS: usize = 9; // constant: runs timed after the warm-up; the minimum is kept

/// The minimum milliseconds of `RUNS` runs of `work` after two to warm up.
fn timed(mut work: impl FnMut() -> Result<(), Error>) -> Result<f64, Error> {
    for _ in 0..2 {
        work()?;
    }
    let mut best = f64::INFINITY;
    for _ in 0..RUNS {
        let start = Instant::now();
        work()?;
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(best)
}

/// The step a kernel is timed with: the effect's defaults, except where the defaults are a setting its kernel declines
/// (the run would be the CPU's, which `main` also checks by counting the transfers).
fn step_for(name: &str) -> Step {
    let step = Step::new(name);
    match name {
        // the default palette, `photo`, is found by the CPU's k-means from a readback
        "dither" => step.param("method", "bayer").param("palette", "gameboy"),
        // above 1 px (4 px at this size) the clumps are a blur, a CPU run; the default's blur is a CPU run from a readback too
        "grain" => step.param("size", 0.25),
        "halftone" => step.param("sigma", 0.0),
        "engraving" => step.param("bend", 0),
        "noise_field" => step.param("kind", "value"),
        _ => step,
    }
}

/// The recipes of `tests/it/gpu/recipes.rs` (demo 31's, the dither tool's three), which the benchmark times whole.
fn recipes() -> Vec<(&'static str, Recipe)> {
    vec![
        (
            "recipe: demo 31",
            Recipe::new([
                Step::new("grade").param("contrast", 1.3),
                Step::new("halftone").param("cell", 3).param("angle", 30),
                Step::new("grain").param("amount", 0.05).param("seed", 1),
            ]),
        ),
        (
            "recipe: 1-bit Bayer",
            Recipe::new([
                Step::new("grade").param("contrast", 1.2),
                Step::new("dither")
                    .param("method", "bayer")
                    .param("palette", "custom")
                    .param("custom", ["#000000", "#ffffff"])
                    .param("cell", 2),
            ]),
        ),
        (
            "recipe: Game Boy",
            Recipe::new([
                Step::new("grade").param("saturation", 0.6),
                Step::new("dither")
                    .param("method", "bayer4")
                    .param("palette", "gameboy")
                    .param("cell", 3),
            ]),
        ),
        (
            "recipe: CMYK halftone",
            Recipe::new([
                Step::new("grade").param("contrast", 1.1),
                Step::new("halftone")
                    .param("cell", 12)
                    .param("separation", "cmyk")
                    .param("paper", "#fffbe8")
                    .param("dot_gain", 0.1),
            ]),
        ),
        (
            "recipe: CMYK halftone, no blur",
            Recipe::new([
                Step::new("grade").param("contrast", 1.1),
                Step::new("halftone")
                    .param("cell", 12)
                    .param("separation", "cmyk")
                    .param("paper", "#fffbe8")
                    .param("dot_gain", 0.1)
                    .param("sigma", 0.0),
            ]),
        ),
    ]
}

/// Every effect that has a kernel, then the recipes: what the table times.
fn rows() -> Result<Vec<(String, Recipe)>, Error> {
    let mut rows = Vec::new();
    for name in effects() {
        if effect(&name)?.kernel().is_some() {
            rows.push((name.clone(), Recipe::new([step_for(&name)])));
        }
    }
    rows.extend(recipes().into_iter().map(|(name, recipe)| (name.to_owned(), recipe)));
    // `cargo run ... --example bench_gpu -- halftone grain`: only the rows whose name has one of the words
    let only: Vec<String> = std::env::args().skip(1).collect();
    rows.retain(|(name, _)| only.is_empty() || only.iter().any(|word| name.contains(word.as_str())));
    Ok(rows)
}

#[expect(clippy::print_stdout, reason = "the benchmark's table is its output")]
fn main() -> Result<(), Error> {
    #[expect(clippy::cast_precision_loss, reason = "a seeded picture's values")]
    let values: Vec<f32> = (0..HEIGHT * WIDTH * 3)
        .map(|index| ((index * 7919 + index / 13) % 1000) as f32 / 1000.0)
        .collect();
    let picture = Picture::new(HEIGHT, WIDTH, &values)?;
    let photo = Photo::new(picture);
    let gpu = block_on(Gpu::request())?;
    let presets = Presets::new();
    let resident = block_on(gpu.upload(&picture))?;
    // a readback alone, to take out of the device-only figure (the readback is how a native caller waits for the GPU)
    let readback = timed(|| block_on(gpu.download(&resident)).map(drop))?;
    println!("{WIDTH} x {HEIGHT}, {}; minimum of {RUNS} runs, ms\n", gpu.adapter());
    println!("| recipe | CPU 1 thread | CPU all cores | GPU end to end | GPU on the device |");
    println!("|---|---|---|---|---|");
    for (name, recipe) in rows()? {
        let one = timed(|| photo.render(&recipe, &presets, Options::default()).map(drop))?;
        let all = timed(|| {
            photo
                .render(&recipe, &presets, Options::default().with_threads(0))
                .map(drop)
        })?;
        let end_to_end = timed(|| {
            block_on(async {
                let made = gpu.render(&photo, &recipe, &presets, Options::default()).await?;
                gpu.download(&made).await.map(drop)
            })
        })?;
        let device = timed(|| {
            block_on(async {
                let made = gpu
                    .render_from(&resident, &recipe, &presets, Options::default())
                    .await?;
                gpu.download(&made).await.map(drop)
            })
        })? - readback;
        // a step a kernel declines, or one that reads the picture back, shows in the transfers of a render left on the device
        gpu.reset_transfers();
        block_on(gpu.render(&photo, &recipe, &presets, Options::default()))?;
        let moved = gpu.transfers();
        let note = if moved.uploads > 1 || moved.downloads > 0 {
            " (CPU or readback inside)"
        } else {
            ""
        };
        println!(
            "| {name}{note} | {one:.1} | {all:.1} | {end_to_end:.1} | {:.1} |",
            device.max(0.0)
        );
    }
    println!("\nA readback alone: {readback:.1} ms.");
    Ok(())
}

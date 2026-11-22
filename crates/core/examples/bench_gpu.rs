//! The GPU backend's timings at 1920 x 1080 (`uv run poe bench-gpu`, a release build): each kernel and the chain of all
//! five on the CPU (one thread, every core) and on the GPU (end to end: upload, steps, readback; and on the device alone:
//! a picture already there, the steps only). A Markdown table on stdout, the minimum of several runs after a warm-up.
//! `docs/design-decisions.md` ("GPU backend") records a run.

use std::time::Instant;

use bullterrier_image::{Error, Gpu, Options, Photo, Picture, Presets, Recipe, Step};

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

fn recipes() -> Vec<(&'static str, Recipe)> {
    let steps = [
        Step::new("levels")
            .param("black", 0.05)
            .param("white", 0.92)
            .param("gamma", 1.3),
        Step::new("grade")
            .param("temperature", 0.4)
            .param("contrast", 1.3)
            .param("saturation", 1.2)
            .param("vibrance", 0.3),
        Step::new("curves").param("shadows", 0.2).param("highlights", 0.82),
        Step::new("channel_mixer")
            .param("red_from_green", 0.2)
            .param("blue_from_blue", 0.9),
        Step::new("color_balance").param("amount", 0.4),
    ];
    let mut all: Vec<(&str, Recipe)> = ["levels", "grade", "curves", "channel_mixer", "color_balance"]
        .into_iter()
        .zip(steps.iter().cloned())
        .map(|(name, step)| (name, Recipe::new([step])))
        .collect();
    all.push(("chain of five", Recipe::new(steps)));
    all
}

#[expect(clippy::print_stdout, reason = "the benchmark's table is its output")]
fn main() -> Result<(), Error> {
    #[expect(clippy::cast_precision_loss, reason = "a seeded picture's values")]
    let values: Vec<f32> = (0..HEIGHT * WIDTH * 3)
        .map(|index| ((index * 7919 + index / 13) % 1000) as f32 / 1000.0)
        .collect();
    let picture = Picture::new(HEIGHT, WIDTH, &values)?;
    let photo = Photo::new(picture);
    let gpu = Gpu::request_blocking()?;
    let presets = Presets::new();
    let resident = gpu.upload(&picture)?;
    // a readback alone, to take out of the device-only figure (the readback is how a native caller waits for the GPU)
    let readback = timed(|| gpu.download_blocking(&resident).map(drop))?;
    println!("{WIDTH} x {HEIGHT}, {}; minimum of {RUNS} runs, ms\n", gpu.adapter());
    println!("| recipe | CPU 1 thread | CPU all cores | GPU end to end | GPU on the device |");
    println!("|---|---|---|---|---|");
    for (name, recipe) in recipes() {
        let one = timed(|| photo.render(&recipe, &presets, Options::default()).map(drop))?;
        let all = timed(|| {
            photo
                .render(&recipe, &presets, Options::default().with_threads(0))
                .map(drop)
        })?;
        let end_to_end = timed(|| {
            photo
                .render(&recipe, &presets, Options::default().with_gpu(&gpu))
                .map(drop)
        })?;
        let device = timed(|| {
            let made = gpu.render_from_blocking(&resident, &recipe, &presets, Options::default())?;
            gpu.download_blocking(&made).map(drop)
        })? - readback;
        println!(
            "| {name} | {one:.1} | {all:.1} | {end_to_end:.1} | {:.1} |",
            device.max(0.0)
        );
    }
    println!("\nA readback alone: {readback:.1} ms.");
    Ok(())
}

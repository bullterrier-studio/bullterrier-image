//! Own binary: wall-clock timing, so no other test may share the process or its cores.
//!
//! A pixel effect's band loop is vectorised: `grade` at its defaults takes about as long as an effect of the same arithmetic
//! registered from outside the library (measured 1.15 times). With the loop left scalar inside the library it took
//! 3.4 times as long (`Picture::map`, `map_at`).
//!
//! Only an optimised build vectorises: the test profile (opt-level 1) cannot tell the two apart, so the test exists only
//! without debug assertions. No gate runs it; run it by hand: `cargo test --release -p bullterrier-image --test vectorised`.

#![cfg(not(debug_assertions))]

use std::hint::black_box;
use std::time::Instant;

use bullterrier_image::{
    Category, Effect, Error, Footprint, Image, Inputs, Options, Photo, Picture, Presets, Recipe, Step, register,
};

const SIDE: usize = 1000;
const RUNS: usize = 9;
const AT_MOST: f64 = 1.6;

/// What `grade` does at its defaults, from this crate; the parameters go through `black_box` so the compiler cannot fold them.
struct Graded;

impl Effect for Graded {
    fn name(&self) -> &'static str {
        "vectorised_graded"
    }

    fn category(&self) -> Category {
        Category::Colour
    }

    fn footprint(&self, _params: &serde_json::Value) -> Result<Footprint, Error> {
        Ok(Footprint::Pixel)
    }

    fn apply(&self, picture: &Picture<'_>, _params: &serde_json::Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        let (pivot, contrast, saturation, vibrance) = black_box((0.5_f32, 1.0_f32, 1.0_f32, 0.0_f32));
        let gain = black_box(0.0_f32).exp2();
        Ok(picture.map(|pixel| {
            let contrasted = pixel.map(|value| pivot + (value * gain - pivot) * contrast);
            let grey = grey(contrasted.map(|value| value.clamp(0.0, 1.0)));
            let saturated = contrasted.map(|value| (grey + (value - grey) * saturation).clamp(0.0, 1.0));
            let strongest = saturated[0].max(saturated[1]).max(saturated[2]);
            let weakest = saturated[0].min(saturated[1]).min(saturated[2]);
            let chroma = if strongest > 0.0 {
                (strongest - weakest) / strongest
            } else {
                0.0
            };
            let spread = 1.0 + vibrance * (1.0 - chroma);
            saturated.map(|value| (strongest - spread * (strongest - value)).clamp(0.0, 1.0))
        }))
    }
}

fn grey(pixel: [f32; 3]) -> f32 {
    0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2]
}

/// The median of `RUNS` renders of `name` at one thread, in seconds.
fn median(values: &[f32], name: &str) -> Result<f64, Error> {
    let recipe = Recipe::new([Step::new(name).opacity(1.0)]);
    let mut times = Vec::new();
    for _ in 0..RUNS {
        let photo = Photo::new(Picture::new(SIDE, SIDE, values)?);
        let start = Instant::now();
        let image = photo.render(&recipe, &Presets::new(), Options::default().with_threads(1))?;
        times.push(start.elapsed().as_secs_f64());
        black_box(image);
    }
    times.sort_by(f64::total_cmp);
    Ok(times[RUNS / 2])
}

#[test]
fn grade_is_about_as_fast_as_the_same_arithmetic_registered_from_outside() -> Result<(), Error> {
    register(std::sync::Arc::new(Graded))?;
    #[expect(clippy::cast_precision_loss, reason = "an index below 2^24")]
    let values: Vec<f32> = (0..SIDE * SIDE * 3).map(|at| (at % 251) as f32 / 250.0).collect();
    let (grade, outside) = (median(&values, "grade")?, median(&values, "vectorised_graded")?);
    let ratio = grade / outside;
    assert!(
        ratio < AT_MOST,
        "grade {grade:.4} s, outside {outside:.4} s: {ratio:.2} times, at most {AT_MOST}"
    );
    Ok(())
}

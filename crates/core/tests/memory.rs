//! A render keeps only the newest picture: the most memory it holds at once does not grow with the number of steps.
//!
//! This is the only test binary with a counting allocator, so every byte the process allocates is measured; the
//! measurements run one at a time, each building its own photo under the lock, so nothing else allocates while one runs.
//! `trace`, which keeps a picture per step by design, is the measurement that can fail: it shows the counter sees a
//! picture that is kept.

use std::sync::{Mutex, PoisonError};

use bullterrier_image::{Error, Options, Photo, Picture, Presets, Recipe, Step};
use peak_alloc::PeakAlloc;

#[global_allocator]
static HEAP: PeakAlloc = PeakAlloc;

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

const HEIGHT: usize = 400;
const WIDTH: usize = 600;
/// The bytes of one picture of the test photo's size: 2.88 million, far above anything else a run allocates.
const IMAGE: usize = HEIGHT * WIDTH * 3 * size_of::<f32>();

/// The most bytes, beyond the photo's own, that `run` holds at once while it works on a chain of `steps` steps that each
/// make a new picture.
fn peak_of<R>(steps: u32, run: impl FnOnce(&Photo<'_>, &Recipe) -> Result<R, Error>) -> Result<usize, Error> {
    let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let values = vec![0.5_f32; HEIGHT * WIDTH * 3];
    let recipe = Recipe::new((1..=steps).map(|step| Step::new("grade").param("exposure", f64::from(step) / 100.0)));
    let photo = Photo::new(Picture::new(HEIGHT, WIDTH, &values)?);
    let before = HEAP.current_usage();
    HEAP.reset_peak_usage();
    let result = run(&photo, &recipe)?;
    let peak = HEAP.peak_usage() - before;
    drop(result);
    Ok(peak)
}

fn render_peak(steps: u32) -> Result<usize, Error> {
    peak_of(steps, |photo, recipe| {
        photo.render(recipe, &Presets::new(), Options::default())
    })
}

fn trace_peak(steps: u32) -> Result<usize, Error> {
    peak_of(steps, |photo, recipe| {
        photo.trace(recipe, &Presets::new(), Options::default())
    })
}

#[test]
fn a_render_holds_a_few_pictures_however_many_steps_it_has() -> Result<(), Error> {
    // the picture a step reads and the one it makes, and some room for the rest
    let (short, long) = (render_peak(2)?, render_peak(12)?);
    assert!(
        long <= 2 * IMAGE + IMAGE / 10,
        "12 steps held {long} bytes, {} pictures",
        long / IMAGE
    );
    assert!(long <= short + IMAGE / 10, "2 steps held {short} bytes, 12 held {long}");
    Ok(())
}

#[test]
fn a_trace_holds_a_picture_for_every_step_which_is_what_the_measurement_must_see() -> Result<(), Error> {
    // the known-bad pair of the test above: a run that keeps every picture
    let (short, long) = (trace_peak(2)?, trace_peak(12)?);
    assert!(
        long >= 12 * IMAGE,
        "12 steps held {long} bytes, {} pictures",
        long / IMAGE
    );
    assert!(long >= short + 9 * IMAGE, "2 steps held {short} bytes, 12 held {long}");
    Ok(())
}

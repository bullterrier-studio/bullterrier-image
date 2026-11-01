//! A render keeps only the newest picture: the most memory it holds at once does not grow with the number of steps.
//!
//! This is the only test binary with a counting allocator, so every byte the process allocates is measured; the
//! measurements run one at a time, each building its own photo under the lock, so nothing else allocates while one runs.
//! `trace`, which keeps a picture per step by design, is the measurement that can fail: it shows the counter sees a
//! picture that is kept.

use std::sync::{Mutex, PoisonError};

use bullterrier_image::frame::Flip;
use bullterrier_image::region::{Band, Grid, Shares};
use bullterrier_image::{Each, Entry, Error, Frame, Layer, Options, Photo, Picture, Presets, Recipe, Step};
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

#[test]
fn a_size_no_picture_can_have_is_refused_before_the_framed_photo_is_made() -> Result<(), Error> {
    // a flip makes a second picture of the photo's size, and a longest side of 100,000 asks for 100,000 x 66,667 pixels,
    // over the limit: the refusal comes first, so no picture is made (the photo's own is not counted)
    let peak = peak_of(1, |photo, recipe| {
        let mut flipped = recipe.clone();
        flipped.frame = Frame {
            flip: Flip::X,
            ..Frame::default()
        };
        Ok(photo.render(&flipped, &Presets::new(), Options::default().with_size(100_000)))
    })?;
    assert!(
        peak < IMAGE / 10,
        "the refusal held {peak} bytes, {} pictures",
        peak / IMAGE
    );
    Ok(())
}

/// The most bytes, beyond the photo's own and its depth map, that `run` holds at once while it works on 8 masked steps,
/// each masked by a region of its own: a band of the depth.
fn masked_peak(run: impl FnOnce(&Photo<'_>, &Recipe) -> Result<(), Error>) -> Result<usize, Error> {
    let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let values = vec![0.5_f32; HEIGHT * WIDTH * 3];
    let depth = Layer::new(HEIGHT, WIDTH, vec![0.5_f32; HEIGHT * WIDTH])?;
    let mut recipe =
        Recipe::new((0..8).map(|step| Step::new("grade").param("exposure", 0.5).mask(format!("band{step}"))));
    for step in 0..8 {
        let band = Band {
            of: "depth".to_owned(),
            low: f64::from(step) / 10.0,
            ..Band::default()
        };
        recipe = recipe.with_region(format!("band{step}"), band);
    }
    let photo = Photo::new(Picture::new(HEIGHT, WIDTH, &values)?).with_map("depth", depth)?;
    let before = HEAP.current_usage();
    HEAP.reset_peak_usage();
    run(&photo, &recipe)?;
    Ok(HEAP.peak_usage() - before)
}

#[test]
fn a_render_holds_the_regions_the_steps_ahead_need_and_not_those_it_has_made() -> Result<(), Error> {
    // a picture the step reads, the one it makes and the one it is laid into, and the band of the step (a third of a
    // picture): 3.33 pictures. The band of the step before, kept, would make 3.67 and all 7 before it 5.7
    let peak = masked_peak(|photo, recipe| photo.render(recipe, &Presets::new(), Options::default()).map(drop))?;
    assert!(
        peak <= 7 * IMAGE / 2,
        "8 masked steps held {peak} bytes, {} pictures",
        peak / IMAGE
    );
    Ok(())
}

#[test]
fn a_trace_of_masked_steps_holds_every_picture_which_is_what_the_measurement_must_see() -> Result<(), Error> {
    // the known-bad of the test above: a run that keeps every picture and every region
    let peak = masked_peak(|photo, recipe| photo.trace(recipe, &Presets::new(), Options::default()).map(drop))?;
    assert!(
        peak >= 8 * IMAGE,
        "8 masked steps held {peak} bytes, {} pictures",
        peak / IMAGE
    );
    Ok(())
}

/// The most bytes, beyond the photo's own, that `run` holds at once while it works on a `grade` step followed by an each
/// step of three looks over a grid of 12 columns, blended over 24 design pixels.
fn each_peak(run: impl FnOnce(&Photo<'_>, &Recipe) -> Result<(), Error>) -> Result<usize, Error> {
    let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let values = vec![0.5_f32; HEIGHT * WIDTH * 3];
    let look = |exposure: f64| Some(Step::new("grade").param("exposure", exposure).opacity(1.0));
    let mut each = Each::new("cells", [look(0.2), look(0.4), look(0.6)]);
    each.feather = 24.0;
    let recipe = Recipe::new([
        Entry::from(Step::new("grade").param("exposure", 0.1)),
        Entry::from(each),
    ])
    .with_region(
        "cells",
        Grid {
            columns: Shares::equal(12),
            ..Grid::default()
        },
    );
    let photo = Photo::new(Picture::new(HEIGHT, WIDTH, &values)?);
    let before = HEAP.current_usage();
    HEAP.reset_peak_usage();
    run(&photo, &recipe)?;
    Ok(HEAP.peak_usage() - before)
}

#[test]
fn an_each_step_holds_its_input_the_blend_so_far_one_look_and_one_weight_map_at_a_time() -> Result<(), Error> {
    // the first step's picture, the blend so far, the look being run and its weights and the piece numbers (a third of a
    // picture each): 3.76 pictures. The results and weights of all three looks kept would be 7
    let peak = each_peak(|photo, recipe| photo.render(recipe, &Presets::new(), Options::default()).map(drop))?;
    assert!(
        peak <= 4 * IMAGE,
        "an each of 3 looks held {peak} bytes, {} pictures",
        peak / IMAGE
    );
    Ok(())
}

#[test]
fn a_trace_of_an_each_step_keeps_the_picture_of_every_step_which_is_what_the_measurement_must_see() -> Result<(), Error>
{
    // the known-bad of the test above: a run that keeps the picture of each step as well
    let peak = each_peak(|photo, recipe| photo.trace(recipe, &Presets::new(), Options::default()).map(drop))?;
    assert!(
        peak > 4 * IMAGE,
        "the trace held {peak} bytes, {} pictures",
        peak / IMAGE
    );
    Ok(())
}

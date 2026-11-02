//! A render keeps only the newest picture: the most memory it holds at once does not grow with the number of steps.
//!
//! This is the only test binary with a counting allocator, so every byte the process allocates is measured; the
//! measurements run one at a time, each building its own photo under the lock, so nothing else allocates while one runs.
//! `trace`, which keeps a picture per step by design, is the measurement that can fail: it shows the counter sees a
//! picture that is kept.

mod common;

use std::fmt::Write as _;
use std::sync::{Mutex, PoisonError};

use bullterrier_image::frame::Flip;
use bullterrier_image::region::{Band, Curve, Cuts, Edge, EdgeStyle, Grid, Line, Path, Region, Shards, Shares, Svg};
use bullterrier_image::{Each, Entry, Error, Frame, Layer, Options, Photo, Picture, Presets, Recipe, Step, read};
use common::files::Scratch;
use common::icc::{cmyk, iccp, matrix_rgb, tiff};
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
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

fn threaded_render_peak(steps: u32, threads: usize) -> Result<usize, Error> {
    peak_of(steps, |photo, recipe| {
        photo.render(recipe, &Presets::new(), Options::default().with_threads(threads))
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
fn a_render_on_ten_threads_holds_no_more_pictures_than_one_on_a_single_thread() -> Result<(), Error> {
    // the pool's own memory and a band's working space are far below a picture; a render that gave each thread a copy would not be
    let ten = threaded_render_peak(12, 10)?;
    assert!(
        ten <= 2 * IMAGE + IMAGE / 10,
        "12 steps on 10 threads held {ten} bytes, {} pictures",
        ten / IMAGE
    );
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

/// The most bytes, beyond the photo's own, that rendering a one-look `each` over the partition `region` (or a step masked by
/// it, for a selection) holds at once.
fn laid_peak(region: impl Into<Region>) -> Result<usize, Error> {
    let region = region.into();
    let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let values = vec![0.5_f32; HEIGHT * WIDTH * 3];
    let step = if matches!(region, Region::Path(_)) {
        Entry::from(Step::new("grade").param("exposure", 0.5).mask("r"))
    } else {
        Entry::from(Each::new("r", [None]))
    };
    let recipe = Recipe::new([step]).with_region("r", region);
    let photo = Photo::new(Picture::new(HEIGHT, WIDTH, &values)?);
    let before = HEAP.current_usage();
    HEAP.reset_peak_usage();
    photo.render(&recipe, &Presets::new(), Options::default())?;
    Ok(HEAP.peak_usage() - before)
}

#[test]
fn a_region_laid_over_a_frame_holds_a_few_pictures_however_much_its_numbers_ask_for() -> Result<(), Error> {
    let torn = Edge {
        style: EdgeStyle::Torn,
        amount: 48.0,
        scale: 1.0,
        octaves: 8,
        ..Edge::default()
    };
    let folder = std::env::temp_dir().join(format!("bt-image-memory-{}", std::process::id()));
    std::fs::create_dir_all(&folder).map_err(|error| Error::Recipe(error.to_string()))?;
    let file = folder.join("hundred.svg");
    let mut rects = String::new();
    for at in 0..100 {
        let _ = write!(
            rects,
            r#"<rect x="{}" y="{}" width="30" height="30"/>"#,
            (at % 10) * 20,
            (at / 10) * 20
        );
    }
    std::fs::write(
        &file,
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200">{rects}</svg>"#),
    )
    .map_err(|error| Error::Recipe(error.to_string()))?;
    let cuts = Cuts {
        lines: (0..32)
            .map(|at| {
                if at % 2 == 0 {
                    Line {
                        x: 0.5,
                        y: 0.5,
                        angle: 5.0 * f64::from(at),
                    }
                    .into()
                } else {
                    Curve {
                        path: format!("M {} 0 L {} 1", 0.03 * f64::from(at), 0.03 * f64::from(at) + 0.02),
                    }
                    .into()
                }
            })
            .collect(),
        ..Cuts::default()
    };
    // (what it is, the region, the most pictures it may hold at once beyond the photo's own: the picture a step makes, and
    // the bend of a torn edge, the raster of a drawing or the mask of a path, which measured 1.3 to 2.4)
    let rows: Vec<(&str, Region, usize)> = vec![
        (
            "500 torn shards",
            Shards {
                count: 500,
                edge: torn.clone(),
                ..Shards::default()
            }
            .into(),
            2,
        ),
        (
            "100 svg shapes",
            Svg {
                file: file.to_string_lossy().into_owned(),
                edge: torn.clone(),
                ..Svg::default()
            }
            .into(),
            2,
        ),
        (
            "a path of 16 samples",
            Path {
                samples: 16,
                edge: torn.clone(),
                ..Path::default()
            }
            .into(),
            3,
        ),
        ("32 cuts", cuts.into(), 2),
        (
            "a torn grid",
            Grid {
                edge: torn,
                ..Grid::default()
            }
            .into(),
            2,
        ),
    ];
    for (what, region, pictures) in rows {
        let peak = laid_peak(region)?;
        assert!(
            peak <= pictures * IMAGE,
            "{what} held {peak} bytes, {} pictures",
            peak / IMAGE
        );
    }
    std::fs::remove_dir_all(&folder).ok();
    Ok(())
}

/// The most bytes, beyond the file's, that `read` holds at once while it reads `file`. The caller holds the lock.
fn read_peak(file: &[u8]) -> Result<usize, Error> {
    let folder = Scratch::new().map_err(|error| Error::ImageRead(error.to_string()))?;
    let path = folder
        .write("file", file)
        .map_err(|error| Error::ImageRead(error.to_string()))?;
    let before = HEAP.current_usage();
    HEAP.reset_peak_usage();
    let image = read(&path)?;
    let peak = HEAP.peak_usage() - before;
    drop(image);
    Ok(peak)
}

/// The bytes of the colour of `pixels` pixels as `read` holds them: three `f32` each.
const fn colour(pixels: usize) -> usize {
    pixels * 3 * size_of::<f32>()
}

#[test]
fn applying_a_profile_does_not_double_the_memory_a_read_holds() -> Result<(), Error> {
    let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner); // the files below are big too
    let to_error = |error: &dyn std::fmt::Display| Error::ImageRead(error.to_string());
    // a PNG of flat colour is a few kilobytes whatever its size; the profile goes in after its header chunk
    let side = 2000;
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&vec![90_u8; side * side * 3], 2000, 2000, ExtendedColorType::Rgb8)
        .map_err(|error| to_error(&error))?;
    let header = 8 + 25; // the signature and the header chunk
    let adobe = [
        [0.60974, 0.31111, 0.01947],
        [0.20528, 0.62567, 0.06087],
        [0.14919, 0.06322, 0.74457],
    ];
    let profiled = [&png[..header], &iccp(&matrix_rgb(adobe, 2.2)), &png[header..]].concat();
    // a CMYK TIFF of one long row: 16 MB of inks
    let pixels = 4_000_000;
    let inks: Vec<u8> = (0..pixels * 4)
        .map(|index| u8::try_from(index % 251).unwrap_or(0))
        .collect();
    let row = u32::try_from(pixels).map_err(|error| to_error(&error))?;
    let rows = [
        ("an RGB PNG", side * side, png, profiled),
        (
            "a CMYK TIFF",
            pixels,
            tiff(row, 5, &inks, b"not a profile"),
            tiff(row, 5, &inks, &cmyk()),
        ),
    ];
    for (what, pixels, plain, with_profile) in rows {
        // the profile's own tables and the chunks it works in are far below the colour of the picture; a copy of it is not
        let (without, with) = (read_peak(&plain)?, read_peak(&with_profile)?);
        assert!(
            with <= without + colour(pixels) / 4,
            "{what} held {without} bytes without a profile and {with} with one, the colour being {}",
            colour(pixels)
        );
    }
    Ok(())
}

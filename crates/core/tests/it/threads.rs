//! A render gives the same picture at any number of threads, and a photo and a look can be shared between threads.
//!
//! The pictures are large enough, and of odd shapes (one row, one column, fewer rows than threads), that the rows are cut
//! into several bands at the larger thread counts.

use std::sync::Arc;
use std::thread;

use bullterrier_image::frame::{Fit, Resample};
use bullterrier_image::{
    BlendMode, Error, ErrorKind, Frame, Image, Lut, Options, Photo, Picture, Presets, Recipe, Step,
};
use serde_json::json;

/// The thread counts a render is compared at: the default, a few, an odd one, more than rows, and every core.
const THREADS: [usize; 6] = [1, 2, 3, 10, 37, 0];

/// The values of a `height` x `width` picture, a different one in nearly every channel, kept to 0..1.
fn values(height: usize, width: usize, seed: usize) -> Vec<f32> {
    (0..height * width * 3)
        .map(|index| f32::from(u16::try_from((index * 7919 + index / 13 + seed) % 1000).unwrap_or(0)) / 1000.0)
        .collect()
}

/// A lookup table of five entries per axis that bends every channel and mixes red into blue: the table `film`.
fn film() -> Result<Lut, Error> {
    let entries = (0..125_usize)
        .map(|entry| {
            let [red, green, blue] =
                [entry % 5, entry / 5 % 5, entry / 25].map(|index| f32::from(u8::try_from(index).unwrap_or(0)) / 4.0);
            [red * red, green.sqrt(), f32::midpoint(blue, red)]
        })
        .collect();
    Lut::new(5, [0.0; 3], [1.0; 3], entries)
}

/// `recipe` rendered on the `height` x `width` picture of `seed` with `threads`, the photo given the table `film`.
fn render(height: usize, width: usize, seed: usize, recipe: &Recipe, threads: usize) -> Result<Image, Error> {
    let values = values(height, width, seed);
    Photo::new(Picture::new(height, width, &values)?)
        .with_lut("film", Arc::new(film()?))
        .render(recipe, &Presets::new(), Options::default().with_threads(threads))
}

/// The index of the first value where `a` and `b` differ in a single bit, or that of the shorter one's end if they differ
/// in length; none if they are the same. (A mismatch of whole pictures is reported by this, not printed.)
fn first_difference(a: &[f32], b: &[f32]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(a, b)| a.to_bits() != b.to_bits())
        .or_else(|| (a.len() != b.len()).then(|| a.len().min(b.len())))
}

fn grade(exposure: f64) -> Step {
    Step::new("grade").param("exposure", exposure).param("saturation", 1.3)
}

/// A look and the shapes (height, width) of photo it is compared on.
struct Case {
    look: &'static str,
    recipe: Recipe,
    shapes: Vec<(usize, usize)>,
}

/// The looks compared.
#[expect(clippy::too_many_lines, reason = "a table of looks, one row per kind of effect")]
fn cases() -> Vec<Case> {
    let thin_and_odd = [(1, 20_011), (20_011, 1), (3, 10_007), (201, 199), (131, 257)];
    let turned = |resample, fit| {
        let mut recipe = Recipe::new([grade(0.2)]);
        recipe.frame = Frame {
            angle: 17.0,
            fit,
            resample,
            fill: bullterrier_image::params::Color([0.2, 0.4, 0.6]),
            ..Frame::default()
        };
        recipe
    };
    let sized = |size| {
        let mut recipe = Recipe::new([grade(0.2)]);
        recipe.size = Some(size);
        recipe
    };
    let case = |look, recipe, shapes: &[(usize, usize)]| Case {
        look,
        recipe,
        shapes: shapes.to_vec(),
    };
    let squarish = [(201, 199), (131, 257)];
    vec![
        case(
            "steps laid over their input by blend mode, opacity and mask",
            Recipe::new([
                grade(0.5).blend_mode(BlendMode::Overlay).opacity(0.7).mask("light"),
                grade(-0.3).blend_mode(BlendMode::Color).mask("light"),
                grade(0.1).opacity(0.5),
            ]),
            &thin_and_odd,
        ),
        case(
            "effects that work pixel by pixel and by place",
            Recipe::new([
                Step::new("pixel_grid").param("cell", 4),
                Step::new("pattern"),
                Step::new("gradient")
                    .param("kind", "conic")
                    .param("repeat", 3)
                    .param("reflect", true)
                    .param("x", 0.3)
                    .opacity(0.5),
                Step::new("gradient").param("kind", "radial").opacity(0.3),
                Step::new("gradient").param("angle", 33).opacity(0.3),
                Step::new("fractal").param("iterations", 40).opacity(0.5),
                Step::new("fractal")
                    .param("kind", "mandelbrot")
                    .param("x", -0.5)
                    .param("iterations", 30)
                    .opacity(0.3),
                Step::new("lens_flare")
                    .param("x", 0.3)
                    .param("ghosts", 7)
                    .param("halo", 30)
                    .param("ring", 20),
                Step::new("light_leak")
                    .param("angle", 40)
                    .param("blobs", 5)
                    .param("seed", 9),
                grade(0.2),
                Step::new("levels").param("black", 0.1).param("gamma", 1.4),
                Step::new("curves").param("midtones", 0.35).param("red_shadows", 0.4),
                Step::new("channel_mixer")
                    .param("red_from_green", 0.5)
                    .param("blue_from_blue", 0.8),
                Step::new("color_balance"),
                Step::new("selective_color").param("width", 360).param("hue_shift", 40),
                Step::new("lut").param("file", "film").param("amount", 0.8),
                Step::new("vignette").param("roundness", 0.6).param("feather", 0.2),
                Step::new("scanlines").param("spacing", 2.5).param("softness", 0.5),
            ]),
            &thin_and_odd,
        ),
        case(
            "blurs summed tap by tap and by the Fourier transform",
            Recipe::new([
                Step::new("gaussian_blur").param("radius", 3).param("edge", "wrap"),
                Step::new("gaussian_blur")
                    .param("radius", 14)
                    .param("edge", "constant")
                    .param("constant_value", 0.3),
                Step::new("sharpen")
                    .param("radius", 3)
                    .param("amount", 1.5)
                    .param("threshold", 0.02)
                    .param("edge", "wrap"),
                Step::new("sharpen")
                    .param("radius", 14)
                    .param("edge", "constant")
                    .param("constant_value", 0.3),
                Step::new("highpass").param("radius", 3).param("edge", "mirror"),
                Step::new("highpass").param("radius", 14).param("edge", "nearest"),
                Step::new("focus_blur")
                    .param("field", "light")
                    .param("focus", 0.3)
                    .param("radius", 3)
                    .param("levels", 3)
                    .param("edge", "wrap"),
                Step::new("focus_blur")
                    .param("field", "light")
                    .param("focus", 0.6)
                    .param("width", 0.1)
                    .param("radius", 14)
                    .param("edge", "nearest"),
            ]),
            // (a radius is in design px, 480 to the longest side: sigma is 1.3 and 5.9 px on the squarish pictures, so only the
            // second is filtered by the transform there, and 6 times that on the thin ones. The transform is slow in a debug
            // build, so these are only shapes that cut the rows into bands and pair them)
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1)],
        ),
        case(
            "effects that read the whole picture before they work pixel by pixel",
            Recipe::new([
                Step::new("auto_levels").param("clip", 0.02),
                grade(0.1),
                Step::new("auto_levels").param("mode", "linked"),
                Step::new("equalize").param("tiles", 5).param("bins", 64),
                Step::new("gradient_map")
                    .param("stops", json!(["#102040", "#c04020", "#f0f0a0"]))
                    .param("shadow_clip", 0.05),
            ]),
            &thin_and_odd,
        ),
        case(
            "the guided filter and the minimum filter, on the rows and the columns turned into rows",
            Recipe::new([
                Step::new("tone_map").param("radius", 5).param("amount", 1.0),
                Step::new("dehaze")
                    .param("patch", 9)
                    .param("radius", 4)
                    .param("edge", "wrap"),
                Step::new("dehaze").param("patch", 4).param("amount", 0.5),
            ]),
            // (shapes that cut the rows into bands and pair them, thinner ones than the squarish are slow in a debug build)
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1), (5, 2_003)],
        ),
        case(
            "a shrink",
            sized(150),
            &[(301, 257), (257, 301), (400, 400), (1, 20_011), (20_011, 1)],
        ),
        case("a growth", sized(300), &[(151, 133), (133, 151), (150, 150)]),
        case("a turn, nearest", turned(Resample::Nearest, Fit::Expand), &squarish),
        case(
            "a turn, bilinear, expanded",
            turned(Resample::Bilinear, Fit::Expand),
            &squarish,
        ),
        case(
            "a turn, bicubic, cropped",
            turned(Resample::Bicubic, Fit::Crop),
            &squarish,
        ),
    ]
}
#[test]
fn a_render_gives_the_same_picture_at_any_number_of_threads() -> Result<(), Error> {
    for Case { look, recipe, shapes } in cases() {
        for (height, width) in shapes {
            let alone = render(height, width, 1, &recipe, 1)?;
            let alone_values = alone.clone().into_values();
            for threads in THREADS {
                let image = render(height, width, 1, &recipe, threads)?;
                assert_eq!(
                    (image.height(), image.width()),
                    (alone.height(), alone.width()),
                    "{look}, {height} x {width}, {threads} threads"
                );
                let at = first_difference(&alone_values, &image.into_values());
                assert_eq!(
                    at, None,
                    "{look}, {height} x {width}, {threads} threads: first differs at {at:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn the_comparison_tells_renders_that_differ_apart() -> Result<(), Error> {
    // the known-bad pair of the test above: looks that are not the same must show as different
    let (a, b) = (Recipe::new([grade(0.2)]), Recipe::new([grade(0.200_000_1)]));
    let (a, b) = (
        render(201, 199, 1, &a, 1)?.into_values(),
        render(201, 199, 1, &b, 1)?.into_values(),
    );
    assert!(first_difference(&a, &b).is_some());
    assert!(first_difference(&a, &a[..a.len() - 1]).is_some());
    Ok(())
}

#[test]
fn a_trace_gives_the_same_pictures_at_any_number_of_threads() -> Result<(), Error> {
    let recipe = Recipe::new([grade(0.5).blend_mode(BlendMode::Overlay).mask("light"), grade(-0.2)]);
    let values = values(201, 199, 3);
    let photo = Photo::new(Picture::new(201, 199, &values)?);
    let stages = |threads: usize| photo.trace(&recipe, &Presets::new(), Options::default().with_threads(threads));
    // (a stage's Debug shows its shape only, so the stages are compared as values)
    let alone = stages(1)?;
    assert!(alone.len() >= 3, "the trace has the input and each step");
    for threads in [2, 10] {
        assert!(stages(threads)? == alone, "{threads} threads");
    }
    Ok(())
}

#[test]
fn more_threads_than_the_limit_refuse_the_render_naming_the_number_and_the_limit_itself_does_not() -> Result<(), Error>
{
    let recipe = Recipe::new([grade(0.2)]);
    let refused = render(3, 3, 1, &recipe, 257);
    assert!(
        matches!(&refused, Err(error) if error.kind() == ErrorKind::Recipe && error.to_string().contains("257")),
        "{refused:?}"
    );
    let values = values(3, 3, 1);
    let photo = Photo::new(Picture::new(3, 3, &values)?);
    let traced = photo.trace(&recipe, &Presets::new(), Options::default().with_threads(usize::MAX));
    assert!(
        matches!(&traced, Err(error) if error.kind() == ErrorKind::Recipe && error.to_string().contains("threads")),
        "{traced:?}"
    );
    assert!(render(3, 3, 1, &recipe, 256).is_ok());
    Ok(())
}

#[test]
fn photo_recipe_presets_options_and_image_can_be_shared_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Photo<'static>>();
    assert_send_sync::<Recipe>();
    assert_send_sync::<Presets>();
    assert_send_sync::<Options>();
    assert_send_sync::<Image>();
}

#[test]
fn several_photos_rendered_at_once_with_one_recipe_each_give_what_they_give_alone() -> Result<(), Error> {
    // frame-level parallelism: one render for each photo, from a thread of the caller's, sharing the recipe and presets
    let recipe = Recipe::new([
        grade(0.5).blend_mode(BlendMode::Overlay).opacity(0.7).mask("light"),
        grade(-0.1),
    ]);
    let presets = Presets::new();
    let seeds: Vec<usize> = (0..6).collect();
    let alone = seeds
        .iter()
        .map(|&seed| render(131, 257, seed, &recipe, 1).map(Image::into_values))
        .collect::<Result<Vec<_>, _>>()?;
    let together = thread::scope(|scope| {
        let workers: Vec<_> = seeds
            .iter()
            .map(|&seed| {
                let (recipe, presets) = (&recipe, &presets);
                // each worker also uses two threads for its own rows
                scope.spawn(move || {
                    let values = values(131, 257, seed);
                    Photo::new(Picture::new(131, 257, &values)?)
                        .render(recipe, presets, Options::default().with_threads(2))
                        .map(Image::into_values)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)))
            .collect::<Result<Vec<_>, _>>()
    })?;
    for (seed, (alone, together)) in alone.iter().zip(&together).enumerate() {
        assert_eq!(first_difference(alone, together), None, "photo {seed}");
    }
    assert!(first_difference(&alone[0], &alone[1]).is_some(), "the photos differ");
    Ok(())
}

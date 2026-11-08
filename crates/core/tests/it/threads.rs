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

use crate::common::compare::first_bit_difference;
use crate::common::pictures::varied_seeded;

/// The thread counts a render is compared at: the default, a few, an odd one, more than rows, and every core.
const THREADS: [usize; 6] = [1, 2, 3, 10, 37, 0];

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
    let values = varied_seeded(height, width, seed);
    Photo::new(Picture::new(height, width, &values)?)
        .with_lut("film", Arc::new(film()?))
        .render(recipe, &Presets::new(), Options::default().with_threads(threads))
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
                Step::new("bloom")
                    .param("radius", 3)
                    .param("threshold", 0.3)
                    .param("octaves", 3)
                    .param("edge", "wrap"),
                Step::new("bloom")
                    .param("radius", 14)
                    .param("threshold", 0.3)
                    .param("color", "#ff4a14")
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
                Step::new("ambient_occlusion")
                    .param("field", "light")
                    .param("radius", 3)
                    .param("edge", "mirror"),
                Step::new("ambient_occlusion")
                    .param("field", "light")
                    .param("radius", 14)
                    .param("gain", 9),
            ]),
            // (a radius is in design px, 480 to the longest side: sigma is 1.3 and 5.9 px on the squarish pictures, so only the
            // second is filtered by the transform there, and 6 times that on the thin ones. The transform is slow in a debug
            // build, so these are only shapes that cut the rows into bands and pair them)
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1)],
        ),
        case(
            "blurs, spikes and streaks, summed tap by tap, row by row and by the Fourier transform",
            Recipe::new([
                Step::new("motion_blur")
                    .param("angle", 20)
                    .param("length", 8)
                    .param("edge", "nearest"),
                Step::new("motion_blur")
                    .param("angle", 0)
                    .param("length", 60)
                    .param("edge", "wrap"),
                Step::new("motion_blur")
                    .param("angle", 33)
                    .param("length", 40)
                    .param("edge", "mirror"),
                Step::new("lens_blur")
                    .param("radius", 3)
                    .param("highlights", 5)
                    .param("edge", "wrap"),
                Step::new("lens_blur").param("radius", 12).param("edge", "nearest"),
                Step::new("starburst")
                    .param("rays", 5)
                    .param("length", 30)
                    .param("threshold", 0.5)
                    .param("limit", 0.3)
                    .param("spacing", 3)
                    .param("variation", 0.4)
                    .param("seed", 3),
                Step::new("anamorphic_streaks")
                    .param("length", 40)
                    .param("threshold", 0.4)
                    .param("amount", 1),
            ]),
            // (lengths are in design px: 6 times them on the thin pictures, under half of them on the squarish ones)
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1)],
        ),
        case(
            "a picture read again about a centre, by each reading and edge rule",
            Recipe::new([
                Step::new("radial_blur")
                    .param("amount", 0.4)
                    .param("x", 0.3)
                    .param("y", 0.8)
                    .param("interpolation", "bicubic")
                    .param("edge", "wrap"),
                Step::new("radial_blur")
                    .param("mode", "spin")
                    .param("amount", 0.2)
                    .param("samples", 5)
                    .param("interpolation", "nearest")
                    .param("edge", "mirror"),
                Step::new("radial_blur").param("mode", "spin").param("amount", 0.6),
                Step::new("light_rays")
                    .param("x", 0.7)
                    .param("y", 0.2)
                    .param("threshold", 0.4)
                    .param("interpolation", "bicubic")
                    .param("edge", "mirror"),
                Step::new("light_rays")
                    .param("x", 1.0)
                    .param("y", 0.5)
                    .param("length", 0.9)
                    .param("samples", 6)
                    .param("decay", 0.5)
                    .param("edge", "wrap"),
            ]),
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1), (5, 2_003)],
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
            "warps about a centre, by each reading and edge rule",
            Recipe::new([
                Step::new("pinch_bulge")
                    .param("amount", -0.7)
                    .param("radius", 1.2)
                    .param("x", 0.3)
                    .param("y", 0.8)
                    .param("interpolation", "bicubic")
                    .param("edge", "wrap"),
                Step::new("pinch_bulge")
                    .param("amount", 0.9)
                    .param("interpolation", "nearest"),
                Step::new("swirl")
                    .param("angle", -400)
                    .param("radius", 1.5)
                    .param("x", 0.8)
                    .param("y", 0.2)
                    .param("interpolation", "bicubic")
                    .param("edge", "mirror"),
                Step::new("swirl").param("angle", 200).param("interpolation", "nearest"),
                Step::new("ripple")
                    .param("wavelength", 9.5)
                    .param("amplitude", 14)
                    .param("phase", -50)
                    .param("angle", 37)
                    .param("interpolation", "bicubic")
                    .param("edge", "wrap"),
                Step::new("ripple").param("interpolation", "nearest"),
                Step::new("polar")
                    .param("x", 0.3)
                    .param("y", 0.7)
                    .param("interpolation", "bicubic")
                    .param("edge", "mirror"),
                Step::new("polar")
                    .param("mode", "from")
                    .param("x", 0.6)
                    .param("interpolation", "nearest")
                    .param("edge", "wrap"),
                Step::new("droste")
                    .param("scale", 0.7)
                    .param("turn", -50)
                    .param("repeats", 7)
                    .param("interpolation", "bicubic")
                    .param("edge", "mirror"),
                Step::new("droste").param("interpolation", "nearest"),
                Step::new("kaleidoscope")
                    .param("segments", 7)
                    .param("angle", 33)
                    .param("x", 0.3)
                    .param("y", 0.8)
                    .param("interpolation", "bicubic")
                    .param("edge", "wrap"),
                Step::new("kaleidoscope")
                    .param("segments", 2)
                    .param("interpolation", "nearest"),
                Step::new("lens_distortion")
                    .param("model", "fisheye")
                    .param("amount", 1.5)
                    .param("fit", "centre")
                    .param("x", 0.3)
                    .param("y", 0.7)
                    .param("border", "#204060")
                    .param("interpolation", "bicubic")
                    .param("edge", "mirror"),
                Step::new("lens_distortion")
                    .param("amount", -0.25)
                    .param("interpolation", "nearest"),
                Step::new("perspective")
                    .param("vertical", -0.4)
                    .param("horizontal", 0.3)
                    .param("shear_x", 0.6)
                    .param("shear_y", -0.4)
                    .param("rotate", 25)
                    .param("scale", 0.2)
                    .param("interpolation", "bicubic")
                    .param("edge", "wrap"),
                Step::new("perspective").param("interpolation", "nearest"),
                Step::new("displace")
                    .param("amount", 9)
                    .param("scale", 17)
                    .param("octaves", 4)
                    .param("lacunarity", 1.7)
                    .param("seed", 4_294_967_295_u32)
                    .param("interpolation", "bicubic")
                    .param("edge", "wrap"),
                Step::new("displace")
                    .param("scale", 1)
                    .param("octaves", 1)
                    .param("interpolation", "nearest"),
                Step::new("parallax")
                    .param("field", "light")
                    .param("angle", 33)
                    .param("amount", 20)
                    .param("focus", 0.3)
                    .param("interpolation", "bicubic")
                    .param("edge", "mirror"),
                Step::new("parallax")
                    .param("field", "light")
                    .param("interpolation", "nearest"),
                Step::new("chromatic_aberration")
                    .param("amount", 30)
                    .param("samples", 7)
                    .param("x", 0.3)
                    .param("y", 0.8)
                    .param("interpolation", "bicubic")
                    .param("edge", "wrap"),
                Step::new("chromatic_aberration").param("interpolation", "nearest"),
            ]),
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1)],
        ),
        case(
            "shifts and strips, by each edge rule",
            Recipe::new([
                Step::new("lenticular")
                    .param("strip", 7)
                    .param("shift", -9.5)
                    .param("edge", "wrap"),
                Step::new("interlace")
                    .param("offset", 11.25)
                    .param("field", "even")
                    .param("edge", "mirror"),
                Step::new("echo")
                    .param("copies", 5)
                    .param("dx", -6.5)
                    .param("dy", 3.25)
                    .param("fade", 0.3)
                    .param("edge", "reflect"),
                Step::new("rgb_split")
                    .param("pair", "red_cyan")
                    .param("angle", 33)
                    .param("amount", 12.5)
                    .param("edge", "wrap"),
                Step::new("tile")
                    .param("columns", 4)
                    .param("rows", 3)
                    .param("offset_x", 0.37)
                    .param("offset_y", -0.6),
                Step::new("stretch")
                    .param("axis", "horizontal")
                    .param("at", 0.3)
                    .param("span", 40)
                    .param("side", "after"),
                Step::new("stretch"),
                Step::new("slice_shift")
                    .param("slices", 40)
                    .param("amount", 30.5)
                    .param("seed", 4_294_967_295_u32)
                    .param("bands", 6)
                    .param("band_min_height", 3)
                    .param("band_max_height", 20.5),
                Step::new("slice_shift"),
                Step::new("echo").param("copies", 0),
                Step::new("lenticular"),
                Step::new("interlace"),
                Step::new("rgb_split"),
            ]),
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1)],
        ),
        case(
            "seeded noise and marks",
            Recipe::new([
                Step::new("grain")
                    .param("amount", 0.2)
                    .param("balance", -0.6)
                    .param("size", 3.5)
                    .param("color", true)
                    .param("distribution", "uniform")
                    .param("seed", 4_294_967_295_u32)
                    .param("edge", "wrap"),
                Step::new("grain"),
                Step::new("dead_pixels")
                    .param("count", 3_000)
                    .param("size", 3)
                    .param("stuck", 0.4)
                    .param("color", "#ff8000")
                    .param("seed", 4_294_967_295_u32),
                Step::new("paper_texture")
                    .param("kind", "linen")
                    .param("amount", 0.3)
                    .param("scale", 9)
                    .param("octaves", 4)
                    .param("spacing", 7.5)
                    .param("seed", 4_294_967_295_u32),
                Step::new("paper_texture").param("kind", "canvas"),
                Step::new("paper_texture"),
                Step::new("dust_scratches")
                    .param("density", 5)
                    .param("size", 12)
                    .param("hairs", 0.4)
                    .param("scratches", 7)
                    .param("angle", 30)
                    .param("width", 2.5)
                    .param("light", "#ffe0a0")
                    .param("seed", 4_294_967_295_u32),
                Step::new("dust_scratches"),
                Step::new("torn_edge")
                    .param("width", 30)
                    .param("scale", 11)
                    .param("fill", "#336699")
                    .param("seed", 4_294_967_295_u32),
            ]),
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1)],
        ),
        case(
            "generated pictures",
            Recipe::new([
                Step::new("noise_field").param("kind", "value").param("warp", 20),
                Step::new("noise_field")
                    .param("kind", "perlin")
                    .param("warp", 12.5)
                    .param("octaves", 3)
                    .param("seed", 4_294_967_295_u32),
                Step::new("noise_field")
                    .param("kind", "worley")
                    .param("scale", 30)
                    .param("warp", 8),
                Step::new("noise_field")
                    .param("kind", "sine")
                    .param("scale", 40)
                    .param("warp", 15),
                Step::new("noise_field"),
                Step::new("flow_lines")
                    .param("density", 20)
                    .param("length", 90.5)
                    .param("step", 1.5)
                    .param("thickness", 3.5)
                    .param("octaves", 3)
                    .param("twist", 400)
                    .param("seed", 2_147_483_647),
                Step::new("flow_lines"),
                Step::new("aurora")
                    .param("base", 0.6)
                    .param("height", 250.5)
                    .param("waviness", 30)
                    .param("octaves", 4)
                    .param("seed", 4_294_967_295_u32),
                Step::new("aurora"),
                Step::new("fog")
                    .param("amount", 0.9)
                    .param("coverage", 0.5)
                    .param("feather", 0.2)
                    .param("floor", 0.1)
                    .param("scale", 40)
                    .param("rise", 3.5)
                    .param("turbulence", 25)
                    .param("resampling", "bicubic")
                    .param("edge", "wrap")
                    .param("seed", 4_294_967_295_u32),
                Step::new("fog"),
            ]),
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1)],
        ),
        case(
            "pictures carried along noise",
            Recipe::new([
                Step::new("flow_warp")
                    .param("amount", 40.5)
                    .param("steps", 3)
                    .param("scale", 30)
                    .param("variation", 200)
                    .param("interpolation", "bicubic")
                    .param("edge", "wrap")
                    .param("seed", 4_294_967_295_u32),
                Step::new("flow_warp"),
            ]),
            &[(201, 199), (131, 257), (1, 3_001), (3_001, 1)],
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
                let at = first_bit_difference(&alone_values, &image.into_values());
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
    assert!(first_bit_difference(&a, &b).is_some());
    assert!(first_bit_difference(&a, &a[..a.len() - 1]).is_some());
    Ok(())
}

#[test]
fn a_trace_gives_the_same_pictures_at_any_number_of_threads() -> Result<(), Error> {
    let recipe = Recipe::new([grade(0.5).blend_mode(BlendMode::Overlay).mask("light"), grade(-0.2)]);
    let values = varied_seeded(201, 199, 3);
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
    let values = varied_seeded(3, 3, 1);
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
                    let values = varied_seeded(131, 257, seed);
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
        assert_eq!(first_bit_difference(alone, together), None, "photo {seed}");
    }
    assert!(
        first_bit_difference(&alone[0], &alone[1]).is_some(),
        "the photos differ"
    );
    Ok(())
}

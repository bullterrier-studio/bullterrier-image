//! The shipped presets, each through a step that names it: the look its description promises, worked out from the
//! picture it is given. (That a preset is read, checked and listed is `presets.rs`; this is what the looks do.)

use std::sync::Arc;

use crate::common::Outcome;
use crate::common::pictures::varied;
use bullterrier_image::{Image, Layer, Options, Photo, Picture, Presets, Recipe, Step};
use serde_json::Value;

/// A step of `effect` that names `preset` and sets `extra` over it.
fn step(effect: &str, preset: &str, extra: &[(&str, Value)]) -> Step {
    let mut step = Step::new(effect);
    step.preset = Some(preset.to_owned());
    for (key, value) in extra {
        step = step.param(*key, value.clone());
    }
    step
}

/// `picture` (`height` x `width`, RGB) after `step`, with the maps `maps` (name, values) given to the photo.
fn render(
    step: Step,
    (height, width): (usize, usize),
    picture: &[f32],
    maps: &[(&str, Vec<f32>)],
) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let mut photo = Photo::new(Picture::new(height, width, picture)?);
    for (name, values) in maps {
        photo = photo.with_map(*name, Layer::new(height, width, values.clone())?)?;
    }
    let image = photo.render(&Recipe::new([step]), &Presets::shipped()?, Options::default())?;
    Ok(image.into_values())
}

fn red_channel(rgb: &[f32]) -> Vec<f32> {
    rgb.chunks(3).map(|pixel| pixel[0]).collect()
}

/// `count` as a float (a test picture is small).
fn float(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

/// The mean of `values` (row-major, `width` wide) over rows `rows` and columns `columns`.
fn mean(values: &[f32], width: usize, rows: std::ops::Range<usize>, columns: std::ops::Range<usize>) -> f32 {
    let mut sum = 0.0;
    let mut count = 0.0;
    for row in rows {
        for column in columns.clone() {
            sum += values[row * width + column];
            count += 1.0;
        }
    }
    sum / count
}

fn block() -> Vec<f32> {
    let mut map = vec![0.0; 240 * 240];
    for row in 80..240 {
        for column in 70..170 {
            map[row * 240 + column] = 1.0;
        }
    }
    map
}

/// The picture of a figure `map` lit on black, and the map as the field `subject`.
fn figure(map: &[f32]) -> Vec<f32> {
    map.iter().flat_map(|&v| [v; 3]).collect()
}

/// The mean column of the light in `rgb` (red channel, `width` wide).
fn centre_x(rgb: &[f32], width: usize) -> f32 {
    let red = red_channel(rgb);
    let total: f32 = red.iter().sum();
    red.iter().enumerate().map(|(i, v)| float(i % width) * v).sum::<f32>() / total
}

#[test]
fn every_disperse_look_sheds_the_top_before_the_bottom_and_leaves_something_standing() -> Outcome {
    let lit = vec![1.0_f32; 240 * 240 * 3];
    let field = vec![1.0_f32; 240 * 240];
    for look in ["evaporate", "halo", "windblown", "disintegrate"] {
        let out = red_channel(&render(
            step("disperse", look, &[("field", "subject".into())]),
            (240, 240),
            &lit,
            &[("subject", field.clone())],
        )?);
        let (top, bottom) = (mean(&out, 240, 0..80, 0..240), mean(&out, 240, 160..240, 0..240));
        assert!(top < bottom, "{look}: top {top} bottom {bottom}");
        let all = mean(&out, 240, 0..240, 0..240);
        assert!((0.1..0.9).contains(&all), "{look}: mean {all}");
    }
    Ok(())
}

#[test]
fn windblown_carries_the_figure_right_and_the_halo_sheds_its_sides() -> Outcome {
    let map = block();
    let picture = figure(&map);
    let look = |name: &str| {
        render(
            step("disperse", name, &[("field", "subject".into())]),
            (240, 240),
            &picture,
            &[("subject", map.clone())],
        )
    };
    let start = centre_x(&picture, 240);
    assert!(centre_x(&look("windblown")?, 240) > start + 5.0);
    assert!((centre_x(&look("halo")?, 240) - start).abs() < 5.0);
    let (halo, kept) = (red_channel(&look("halo")?), red_channel(&look("evaporate")?));
    let (edge, middle) = ((120..240, 70..90), (120..240, 110..130));
    let side = |v: &[f32], (rows, columns): &(std::ops::Range<usize>, std::ops::Range<usize>)| {
        mean(v, 240, rows.clone(), columns.clone())
    };
    assert!(side(&halo, &edge) < 0.7 * side(&halo, &middle));
    assert!(side(&kept, &edge) > 0.85 * side(&kept, &middle));
    Ok(())
}

#[test]
fn the_long_shadow_is_solid_beside_the_subject_and_fades_out_over_300_px() -> Outcome {
    let (height, width) = (6, 480); // 480 px wide: a design px is a px
    let mut subject = vec![0.0_f32; height * width];
    for row in 0..height {
        for column in 10..50 {
            subject[row * width + column] = 1.0;
        }
    }
    let grey = 0.8_f32;
    let picture = vec![grey; height * width * 3];
    let out = red_channel(&render(
        step(
            "drop_shadow",
            "long_shadow",
            &[("field", "subject".into()), ("angle", 0.into())],
        )
        .opacity(1.0),
        (height, width),
        &picture,
        &[("subject", subject)],
    )?);
    let removed = |x: usize| grey - out[3 * width + x];
    assert!(removed(50) > 0.9 * grey, "beside the subject");
    assert!(
        (50..width - 1).all(|x| removed(x + 1) <= removed(x) + 1e-6),
        "never darker farther out"
    );
    assert!(removed(290) > 0.2 * grey, "still there 240 px out");
    assert!(removed(330) < 0.1 * grey, "nearly gone 280 px out");
    assert!(
        (352..width).all(|x| removed(x).abs() < 1e-6),
        "nothing past the last copy"
    );
    Ok(())
}

#[test]
fn the_dither_looks_keep_to_their_palettes() -> Outcome {
    let picture = varied(24, 24);
    let levels = |out: &[f32], channel: usize| {
        let mut seen: Vec<u32> = out.chunks(3).map(|p| p[channel].to_bits()).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    let posterize = render(step("dither", "posterize", &[]), (24, 24), &picture, &[])?;
    assert!((0..3).all(|c| levels(&posterize, c) <= 4), "4 levels per channel");
    let threshold = render(step("dither", "threshold", &[]), (24, 24), &picture, &[])?;
    assert!(
        threshold.iter().all(|&v| v.abs() < 1e-6 || (v - 1.0).abs() < 1e-6),
        "black and white only"
    );
    let quantized = render(step("dither", "palette_quantize", &[]), (24, 24), &picture, &[])?;
    let mut colours: Vec<[u32; 3]> = quantized
        .chunks(3)
        .map(|p| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
        .collect();
    colours.sort_unstable();
    colours.dedup();
    assert!(colours.len() <= 8, "at most 8 colours");
    Ok(())
}

#[test]
fn the_sparkle_look_lights_a_few_glints_where_plain_spikes_flood_the_bright_block() -> Outcome {
    let size = 480; // a design px is a px
    let mut picture = vec![0.0_f32; size * size * 3];
    for row in 140..340 {
        for column in 140..340 {
            picture[(row * size + column) * 3..(row * size + column) * 3 + 3].fill(0.95);
        }
    }
    let lit = |extra: &[(&str, Value)]| -> Result<usize, Box<dyn std::error::Error>> {
        let out = render(step("starburst", "sparkle", extra), (size, size), &picture, &[])?;
        Ok(out
            .chunks(3)
            .zip(picture.chunks(3))
            .filter(|(o, p)| o.iter().zip(*p).map(|(a, b)| a - b).fold(f32::MIN, f32::max) > 0.02)
            .count())
    };
    let glitter = lit(&[("seed", 1.into())])?;
    let flood = lit(&[("spacing", 0.0.into())])?;
    assert!(glitter > 0 && glitter < flood / 4, "glitter {glitter}, flood {flood}");
    Ok(())
}

#[test]
fn the_colour_balance_looks_and_the_overlay_logo_are_shipped_with_a_description() -> Outcome {
    let presets = Presets::shipped()?;
    for (effect, name) in [
        ("color_balance", "split_tone"),
        ("color_balance", "teal_orange"),
        ("blend", "overlay"),
        ("gradient", "fill"),
    ] {
        let preset = presets
            .get(effect, name)
            .ok_or(format!("{effect}/{name} is not shipped"))?;
        assert_ne!(preset.description.trim(), "", "{effect}/{name} has no description");
    }
    let fill = presets
        .get("gradient", "fill")
        .map(|p| p.description.clone())
        .unwrap_or_default();
    assert!(fill.contains("mask = \"background\""));
    Ok(())
}

#[test]
fn the_fill_look_is_a_flat_picture_that_ignores_the_input_and_can_replace_only_the_background() -> Outcome {
    let (height, width) = (20, 30);
    let picture = varied(height, width);
    let black = render(step("gradient", "fill", &[]), (height, width), &picture, &[])?;
    assert!(black.iter().all(|&v| v == 0.0));
    let green = render(
        step("gradient", "fill", &[("colors", serde_json::json!(["#00ff00"]))]),
        (height, width),
        &picture,
        &[],
    )?;
    assert!(green.chunks(3).all(|p| p == [0.0, 1.0, 0.0]));
    let subject: Vec<f32> = (0..height * width)
        .map(|i| f32::from(u8::from(i % width >= 15)))
        .collect();
    let masked = render(
        step("gradient", "fill", &[]).mask("background"),
        (height, width),
        &picture,
        &[("subject", subject)],
    )?;
    for row in 0..height {
        for column in 0..width {
            let at = (row * width + column) * 3;
            let expected = if column < 15 {
                [0.0; 3]
            } else {
                [picture[at], picture[at + 1], picture[at + 2]]
            };
            assert_eq!(masked[at..at + 3], expected, "row {row} column {column}");
        }
    }
    Ok(())
}

#[test]
fn the_overlay_logo_sits_in_the_bottom_right_corner_a_fifth_of_the_photos_width_wide() -> Outcome {
    let mut logo = vec![1.0_f32; 8 * 16 * 4];
    for (index, pixel) in logo.chunks_mut(4).enumerate() {
        let left = index % 16 < 8;
        pixel[..3].copy_from_slice(if left { &[1.0, 0.0, 0.0] } else { &[0.0, 0.0, 1.0] });
    }
    let photo_values = varied(96, 128);
    let asset = Arc::new(Image::rgba(8, 16, &logo)?);
    let mut recipe = Recipe::new([step("blend", "overlay", &[("image", "logo".into())])]);
    recipe.size = Some(480);
    for (frame, rendered) in [(480_usize, 128_usize), (1024, 0)] {
        recipe.size = Some(u32::try_from(frame)?);
        let (height, width, values) = if rendered == 0 {
            (96 * 8 / 3, 128 * 8 / 3, vec![0.5_f32; (96 * 8 / 3) * (128 * 8 / 3) * 3])
        } else {
            (96, 128, photo_values.clone())
        };
        let photo = Photo::new(Picture::new(height, width, &values)?).with_image("logo", Arc::clone(&asset));
        let out = photo.render(&recipe, &Presets::shipped()?, Options::default())?;
        let (out_height, out_width) = (out.height(), out.width());
        let out = out.into_values();
        let blue: Vec<usize> = (0..out_height * out_width)
            .filter(|&i| out[i * 3] < 0.01 && out[i * 3 + 1] < 0.01 && out[i * 3 + 2] > 0.99)
            .map(|i| i % out_width)
            .collect();
        let right = *blue.iter().max().ok_or("no logo")?;
        let margin = float(out_width - 1 - right);
        let frame = float(frame);
        assert!(
            (margin / frame - 12.0 / 480.0).abs() <= 1.5 / frame,
            "frame {frame}: margin {margin}"
        );
    }
    Ok(())
}

#[test]
fn every_particles_look_lights_the_black_picture_the_same_for_a_seed_and_differently_for_another() -> Outcome {
    let black = vec![0.0_f32; 160 * 160 * 3];
    for look in ["embers", "floating_dust", "rain", "snow", "starfield"] {
        let run = |seed: u32| {
            render(
                step("particles", look, &[("seed", seed.into())]).opacity(1.0),
                (160, 160),
                &black,
                &[],
            )
        };
        let (first, again, other) = (run(3)?, run(3)?, run(4)?);
        assert!(first.iter().all(|v| (0.0..=1.0).contains(v)), "{look}: in range");
        let lit = first.chunks(3).filter(|p| p.iter().any(|&v| v > 0.02)).count();
        assert!((1..160 * 160 / 2).contains(&lit), "{look}: {lit} px lit");
        assert!(first == again, "{look}: the same seed gives the same picture");
        assert!(first != other, "{look}: another seed gives another");
    }
    Ok(())
}

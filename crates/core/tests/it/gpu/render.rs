//! A recipe rendered on the GPU: the same picture as on the CPU within the recipe's budget, the same bytes twice,
//! the transfers it promises, steps laid over their input on the device, pictures kept there, and the refusals.

use std::num::NonZeroUsize;
use std::path::Path;

use bullterrier_image::frame::Transparent;
use bullterrier_image::{BlendMode, Error, ErrorKind, Frame, Image, Options, Photo, Picture, Presets, Recipe, Step};

use super::parity::seeded;
use super::{fresh, gpu};
use crate::common::Outcome;
use crate::common::compare::first_bit_difference;

/// The five kernels in a row, each away from its defaults.
fn five_kernels() -> Recipe {
    Recipe::new([
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
    ])
}

/// The dog of the examples, as a photo.
fn dog() -> Result<Image, Error> {
    bullterrier_image::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/photos/bullterrier.jpeg"))
}

/// The share of the values of `cpu` and `gpu` (pictures of one size) whose 8-bit levels differ by more than 1, and the
/// largest difference in levels.
fn levels_apart(cpu: &[f32], gpu: &[f32]) -> (f64, u8) {
    assert_eq!(cpu.len(), gpu.len());
    let level = |value: f32| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0..=255 after the clamp"
        )]
        let level = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        level
    };
    let gaps: Vec<u8> = cpu
        .iter()
        .zip(gpu)
        .map(|(&one, &other)| level(one).abs_diff(level(other)))
        .collect();
    let over = gaps.iter().filter(|&&gap| gap > 1).count();
    #[expect(clippy::cast_precision_loss, reason = "a share of a small picture's values")]
    let share = over as f64 / gaps.len() as f64;
    (share, gaps.iter().copied().max().unwrap_or(0))
}

#[test]
fn a_chain_of_kernels_and_demo_31s_recipe_stay_within_the_recipe_budget_on_a_photo() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let photo_image = dog()?;
    let photo = Photo::new(photo_image.as_picture());
    let demo_31 = Recipe::new([
        Step::new("grade").param("contrast", 1.3),
        Step::new("halftone").param("cell", 3).param("angle", 30),
        Step::new("grain").param("amount", 0.05).param("seed", 1),
    ]);
    for (what, recipe) in [("five kernels", five_kernels()), ("demo 31", demo_31)] {
        let options = Options::default().with_size(240);
        let cpu = photo.render(&recipe, &Presets::new(), options.clone())?.into_values();
        let on_gpu = photo
            .render(&recipe, &Presets::new(), options.with_gpu(&gpu))?
            .into_values();
        // the interim budget: at most 1/255 a channel on 99.9% of the values, 2/255 on all
        let (share, largest) = levels_apart(&cpu, &on_gpu);
        assert!(
            share <= 0.001 && largest <= 2,
            "{what}: {share} over a level, at most {largest}"
        );
    }
    Ok(())
}

#[test]
fn the_same_gpu_and_input_give_the_same_bytes_twice() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let values = seeded(37, 29, 3);
    let photo = Photo::new(Picture::new(37, 29, &values)?);
    let options = Options::default().with_gpu(&gpu);
    let first = photo
        .render(&five_kernels(), &Presets::new(), options.clone())?
        .into_values();
    let second = photo.render(&five_kernels(), &Presets::new(), options)?.into_values();
    assert_eq!(first_bit_difference(&first, &second), None);
    Ok(())
}

#[test]
fn a_recipe_of_kernels_crosses_once_each_way_and_a_cpu_step_adds_one_of_each() -> Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let values = seeded(20, 30, 1);
    let photo = Photo::new(Picture::new(20, 30, &values)?);
    let options = Options::default().with_gpu(&gpu);
    photo.render(&five_kernels(), &Presets::new(), options.clone())?;
    let all_kernels = gpu.transfers();
    assert_eq!((all_kernels.uploads, all_kernels.downloads), (1, 1));
    gpu.reset_transfers();
    let mut steps = five_kernels().steps;
    steps.insert(2, Recipe::new([Step::new("vignette")]).steps.remove(0));
    photo.render(&Recipe::new(steps), &Presets::new(), options)?;
    let one_cpu_step = gpu.transfers();
    assert_eq!((one_cpu_step.uploads, one_cpu_step.downloads), (2, 2));
    // a picture of 20 x 30 is 600 pixels: sent as 3 floats a pixel and read back as 4
    assert_eq!(one_cpu_step.bytes, 2 * 600 * 12 + 2 * 600 * 16);
    Ok(())
}

#[test]
fn a_step_laid_over_its_input_by_opacity_mask_and_each_blend_mode_matches_the_cpu() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let values = seeded(17, 13, 5);
    let photo = Photo::new(Picture::new(17, 13, &values)?);
    let step = || Step::new("levels").param("black", 0.2).param("gamma", 2.0);
    let mut steps = vec![
        step().opacity(0.4),
        step().mask("light"),
        step().opacity(0.7).mask("background"),
    ];
    for mode in [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Color,
    ] {
        steps.push(step().blend_mode(mode).opacity(0.8));
    }
    let subject: Vec<f32> = (0..17 * 13)
        .map(|at| f32::from(u8::try_from(at % 10).unwrap_or(0)) / 9.0)
        .collect();
    let photo = photo.with_map("subject", bullterrier_image::Layer::new(17, 13, subject)?)?;
    for step in steps {
        let recipe = Recipe::new([step.clone()]);
        let cpu = photo
            .render(&recipe, &Presets::new(), Options::default())?
            .into_values();
        let on_gpu = photo
            .render(&recipe, &Presets::new(), Options::default().with_gpu(&gpu))?
            .into_values();
        let gap = cpu
            .iter()
            .zip(&on_gpu)
            .map(|(one, other)| (one - other).abs())
            .fold(0.0_f32, f32::max);
        assert!(gap <= 1e-4, "{step:?}: gap {gap:e}");
    }
    Ok(())
}

#[test]
fn a_picture_rendered_on_the_device_stays_there_for_the_next_recipe() -> Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let values = seeded(24, 18, 2);
    let photo = Photo::new(Picture::new(24, 18, &values)?);
    let first = Recipe::new(five_kernels().steps[..2].to_vec());
    let second = Recipe::new(five_kernels().steps[2..].to_vec());
    let resident = gpu.render_blocking(&photo, &first, &Presets::new(), Options::default())?;
    let next = gpu.render_from_blocking(&resident, &second, &Presets::new(), Options::default())?;
    let before_readback = gpu.transfers();
    assert_eq!((before_readback.uploads, before_readback.downloads), (1, 0));
    let chained = gpu.download_blocking(&next)?.into_values();
    let whole = photo
        .render(&five_kernels(), &Presets::new(), Options::default())?
        .into_values();
    let gap = whole
        .iter()
        .zip(&chained)
        .map(|(one, other)| (one - other).abs())
        .fold(0.0_f32, f32::max);
    assert!(gap <= 5e-4, "five kernels' bounds: gap {gap:e}");
    Ok(())
}

#[test]
fn a_cut_out_made_on_the_device_has_the_cpus_alpha() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let values = seeded(9, 14, 4);
    let photo = Photo::new(Picture::new(9, 14, &values)?);
    let mut recipe = Recipe::new([Step::new("grade").param("contrast", 1.5)]);
    recipe.frame = Frame {
        transparent: Transparent::Circle,
        ..Frame::default()
    };
    let cpu = photo.render(&recipe, &Presets::new(), Options::default())?;
    let resident = gpu.render_blocking(&photo, &recipe, &Presets::new(), Options::default())?;
    assert_eq!(resident.channels(), 4);
    let alpha = |image: Image| {
        image
            .into_values()
            .chunks(4)
            .map(|pixel| pixel[3])
            .collect::<Vec<f32>>()
    };
    assert_eq!(alpha(gpu.download_blocking(&resident)?), alpha(cpu));
    Ok(())
}

#[test]
fn a_gpu_render_refuses_the_options_it_does_not_take_naming_them() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let values = seeded(8, 8, 0);
    let photo = Photo::new(Picture::new(8, 8, &values)?);
    let recipe = five_kernels();
    let on_gpu = Options::default().with_gpu(&gpu);
    let rows = NonZeroUsize::new(4).ok_or("4 rows")?;
    for (option, options) in [
        ("with_band", on_gpu.clone().with_band(rows)),
        ("with_window", on_gpu.clone().with_window(0, 8, 16)),
        ("with_room", on_gpu.clone().with_room(1, 1, 1, 1)),
    ] {
        let error = photo.render(&recipe, &Presets::new(), options).err().ok_or(option)?;
        assert_eq!(error.kind(), ErrorKind::Gpu, "{error}");
        assert!(error.to_string().contains(option), "{error}");
    }
    let error = photo.trace(&recipe, &Presets::new(), on_gpu).err().ok_or("trace")?;
    assert!(
        error.kind() == ErrorKind::Gpu && error.to_string().contains("trace"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_picture_wider_than_the_device_dispatches_is_refused_naming_both_sizes() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    // a workgroup is 16 wide and a dispatch at most 65,535 of them across: the widest picture is 1,048,560
    let width = 1_048_561;
    let values = vec![0.5; width * 3];
    let error = gpu.upload(&Picture::new(1, width, &values)?).err().ok_or("too wide")?;
    assert_eq!(error.kind(), ErrorKind::Gpu);
    assert!(
        error.to_string().contains("1 x 1048561") && error.to_string().contains("a side"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_picture_of_one_gpu_is_refused_by_another() -> Outcome {
    let (Some(one), Some(other)) = (gpu(), fresh()) else {
        return Ok(());
    };
    let values = seeded(3, 3, 0);
    let held = one.upload(&Picture::new(3, 3, &values)?)?;
    let error = other.download_blocking(&held).err().ok_or("another context")?;
    assert!(
        error.kind() == ErrorKind::Gpu && error.to_string().contains("another Gpu"),
        "{error}"
    );
    let error = other
        .render_from_blocking(&held, &five_kernels(), &Presets::new(), Options::default())
        .err()
        .ok_or("another context")?;
    assert!(error.to_string().contains("another Gpu"), "{error}");
    Ok(())
}

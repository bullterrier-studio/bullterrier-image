//! A recipe rendered on the GPU: the same picture as on the CPU within the recipe's budget, a picture that looks the same twice,
//! the transfers it promises, steps laid over their input on the device, pictures kept there, and the refusals.

use std::num::NonZeroUsize;
use std::path::Path;

use bullterrier_image::frame::Transparent;
use bullterrier_image::{BlendMode, Error, ErrorKind, Frame, Image, Options, Photo, Picture, Presets, Recipe, Step};

use super::parity::seeded;
use pollster::block_on;

use super::{assert_finite, fresh, gpu, largest_gap, rendered};
use crate::common::Outcome;
use crate::common::compare::assert_looks_the_same;

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
pub(super) fn dog() -> Result<Image, Error> {
    bullterrier_image::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/photos/bullterrier.jpeg"))
}

/// The share of the values of `cpu` and `gpu` (pictures of one size) whose 8-bit levels differ by more than 1, and the
/// largest difference in levels.
///
/// # Panics
/// If their lengths differ or either holds a value that is not finite (a clamp would make a level of a NaN).
pub(super) fn levels_apart(cpu: &[f32], gpu: &[f32]) -> (f64, u8) {
    assert_eq!(cpu.len(), gpu.len());
    assert_finite(cpu, "the CPU's picture");
    assert_finite(gpu, "the GPU's picture");
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
        let cpu = photo.render(&recipe, &Presets::new(), options)?.into_values();
        let on_gpu = rendered(&gpu, &photo, &recipe, options)?.into_values();
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
fn the_same_gpu_and_input_give_a_picture_that_looks_the_same_twice() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let values = seeded(37, 29, 3);
    let photo = Photo::new(Picture::new(37, 29, &values)?);
    let first = rendered(&gpu, &photo, &five_kernels(), Options::default())?.into_values();
    let second = rendered(&gpu, &photo, &five_kernels(), Options::default())?.into_values();
    assert_looks_the_same(&first, &second, "twice");
    Ok(())
}

#[test]
fn a_recipe_of_kernels_crosses_once_each_way_and_a_cpu_step_adds_one_of_each() -> Outcome {
    let Some(gpu) = fresh() else { return Ok(()) };
    let values = seeded(20, 30, 1);
    let photo = Photo::new(Picture::new(20, 30, &values)?);
    rendered(&gpu, &photo, &five_kernels(), Options::default())?;
    let all_kernels = gpu.transfers();
    assert_eq!((all_kernels.uploads, all_kernels.downloads), (1, 1));
    gpu.reset_transfers();
    let mut steps = five_kernels().steps;
    // error diffusion is serial: it stays on the CPU, whatever else gets a kernel
    steps.insert(
        2,
        Recipe::new([Step::new("dither").param("method", "floyd_steinberg")])
            .steps
            .remove(0),
    );
    rendered(&gpu, &photo, &Recipe::new(steps), Options::default())?;
    let one_cpu_step = gpu.transfers();
    assert_eq!((one_cpu_step.uploads, one_cpu_step.downloads), (2, 2));
    // a picture of 20 x 30 is 600 pixels: sent as 3 floats a pixel and read back as 4
    // and the curves' table (`tables.rs` and `lut.rs` pin a table's bytes) is counted besides
    assert_eq!(one_cpu_step.tables, 1);
    assert!(one_cpu_step.bytes > 2 * 600 * 12 + 2 * 600 * 16, "{one_cpu_step:?}");
    // a mask is a map sent to the device: one float a pixel, counted as an upload of its own
    gpu.reset_transfers();
    let masked = Recipe::new([Step::new("levels").param("black", 0.2).mask("light")]);
    rendered(&gpu, &photo, &masked, Options::default())?;
    let with_mask = gpu.transfers();
    assert_eq!((with_mask.uploads, with_mask.downloads), (2, 1));
    assert_eq!(with_mask.bytes, 600 * 12 + 600 * 4 + 600 * 16);
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
        let on_gpu = rendered(&gpu, &photo, &recipe, Options::default())?.into_values();
        let (gap, _) = largest_gap(&cpu, &on_gpu);
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
    let resident = block_on(gpu.render(&photo, &first, &Presets::new(), Options::default()))?;
    let next = block_on(gpu.render_from(&resident, &second, &Presets::new(), Options::default()))?;
    let before_readback = gpu.transfers();
    assert_eq!((before_readback.uploads, before_readback.downloads), (1, 0));
    let chained = block_on(gpu.download(&next))?.into_values();
    let whole = photo
        .render(&five_kernels(), &Presets::new(), Options::default())?
        .into_values();
    let (gap, _) = largest_gap(&whole, &chained);
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
    let resident = block_on(gpu.render(&photo, &recipe, &Presets::new(), Options::default()))?;
    assert_eq!(resident.channels(), 4);
    let (on_gpu, cpu) = (block_on(gpu.download(&resident))?.into_values(), cpu.into_values());
    let alpha = |values: &[f32]| values.chunks(4).map(|pixel| pixel[3]).collect::<Vec<f32>>();
    assert_eq!(alpha(&on_gpu), alpha(&cpu));
    let (gap, _) = largest_gap(&cpu, &on_gpu);
    assert!(gap <= 1e-4, "the colour too: gap {gap:e}");
    Ok(())
}

#[test]
fn a_cut_out_chained_on_the_device_keeps_its_alpha_through_kernel_laid_and_cpu_steps() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let values = seeded(9, 14, 8);
    let photo = Photo::new(Picture::new(9, 14, &values)?);
    let mut cut = Recipe::new([Step::new("grade").param("contrast", 1.2)]);
    cut.frame = Frame {
        transparent: Transparent::Circle,
        ..Frame::default()
    };
    let held = block_on(gpu.render(&photo, &cut, &Presets::new(), Options::default()))?;
    let alpha = |image: Image| {
        image
            .into_values()
            .chunks(4)
            .map(|pixel| pixel[3])
            .collect::<Vec<f32>>()
    };
    let cut_alpha = alpha(block_on(gpu.download(&held))?);
    // the circle's corners are clear and its middle is opaque: an alpha of all ones would not pass for it
    assert!(cut_alpha.contains(&0.0) && cut_alpha.contains(&1.0), "{cut_alpha:?}");
    let kernel = || Step::new("levels").param("black", 0.1);
    for (what, step) in [
        ("a kernel step", kernel()),
        ("a laid step", kernel().opacity(0.5).blend_mode(BlendMode::Screen)),
        (
            "a step on the CPU",
            Step::new("dither").param("method", "floyd_steinberg"),
        ),
    ] {
        let next = block_on(gpu.render_from(&held, &Recipe::new([step]), &Presets::new(), Options::default()))?;
        assert_eq!(next.channels(), 4, "{what}");
        let back = block_on(gpu.download(&next))?;
        assert_eq!(back.channels(), 4, "{what}");
        assert_looks_the_same(&alpha(back), &cut_alpha, what);
    }
    Ok(())
}

#[test]
fn a_gpu_render_refuses_the_options_it_does_not_take_as_a_recipe_error_naming_them() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let values = seeded(8, 8, 0);
    let photo = Photo::new(Picture::new(8, 8, &values)?);
    let recipe = five_kernels();
    let rows = NonZeroUsize::new(4).ok_or("4 rows")?;
    let held = block_on(gpu.upload(&Picture::new(8, 8, &values)?))?;
    for (option, options) in [
        ("with_band", Options::default().with_band(rows)),
        ("with_window", Options::default().with_window(0, 8, 16)),
        ("with_room", Options::default().with_room(1, 1, 1, 1)),
    ] {
        // the caller's mistake, never "this machine cannot": `except GpuError: <the CPU>` must not hide it
        let error = rendered(&gpu, &photo, &recipe, options).err().ok_or(option)?;
        assert_eq!(error.kind(), ErrorKind::Recipe, "{error}");
        assert!(error.to_string().contains(option), "{error}");
        let error = block_on(gpu.render_from(&held, &recipe, &Presets::new(), options))
            .err()
            .ok_or(option)?;
        assert_eq!(error.kind(), ErrorKind::Recipe, "render_from: {error}");
        assert!(error.to_string().contains(option), "render_from: {error}");
    }
    Ok(())
}

#[test]
fn a_chain_from_a_picture_on_the_device_refuses_a_resample_or_a_map_as_a_recipe_error() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let values = seeded(8, 6, 0);
    let held = block_on(gpu.upload(&Picture::new(8, 6, &values)?))?;
    let mut framed = five_kernels();
    framed.frame = Frame {
        transparent: Transparent::Circle,
        ..Frame::default()
    };
    for (what, recipe, options, named) in [
        ("another size", five_kernels(), Options::default().with_size(16), "size"),
        ("the same size", five_kernels(), Options::default().with_size(8), ""),
        ("a frame", framed, Options::default(), "frame"),
        (
            "a mask",
            Recipe::new([Step::new("levels").mask("subject")]),
            Options::default(),
            "subject",
        ),
    ] {
        let made = block_on(gpu.render_from(&held, &recipe, &Presets::new(), options));
        if named.is_empty() {
            assert!(made.is_ok(), "{what}: {made:?}");
            continue;
        }
        let error = made.err().ok_or(what)?;
        assert_eq!(error.kind(), ErrorKind::Recipe, "{what}: {error}");
        assert!(error.to_string().contains(named), "{what}: {error}");
    }
    Ok(())
}

#[test]
fn a_picture_wider_than_the_device_dispatches_is_refused_naming_both_sizes() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    // a workgroup is 16 wide and a dispatch at most 65,535 of them across: the widest picture is 1,048,560
    let width = 1_048_561;
    let values = vec![0.5; width * 3];
    let error = block_on(gpu.upload(&Picture::new(1, width, &values)?))
        .err()
        .ok_or("too wide")?;
    assert_eq!(error.kind(), ErrorKind::Gpu);
    assert!(
        error.to_string().contains("1 x 1048561") && error.to_string().contains("a side"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_picture_of_more_pixels_than_the_callers_device_holds_is_refused_naming_both_sizes() -> Outcome {
    // held while the test makes a context of its own (`gpu::ONE_AT_A_TIME`)
    let Some(_alone) = gpu() else { return Ok(()) };
    // a caller's device whose storage buffers hold 64 KiB: 4,096 pixels of 16 bytes
    let (device, queue) = block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let required_limits = wgpu::Limits {
            max_storage_buffer_binding_size: 64 * 1024,
            ..wgpu::Limits::downlevel_defaults()
        };
        let descriptor = wgpu::DeviceDescriptor {
            required_limits,
            ..Default::default()
        };
        Ok::<_, Box<dyn std::error::Error>>(adapter.request_device(&descriptor).await?)
    })?;
    assert_eq!(device.limits().max_storage_buffer_binding_size, 64 * 1024);
    let small = bullterrier_image::Gpu::new(device, queue)?;
    assert_eq!(small.largest_picture(), 4096);
    // a square, so the count of pixels is over and no side is
    let fits = seeded(64, 64, 0);
    assert!(block_on(small.upload(&Picture::new(64, 64, &fits)?)).is_ok());
    let over = seeded(65, 65, 0);
    let error = block_on(small.upload(&Picture::new(65, 65, &over)?))
        .err()
        .ok_or("over the device")?;
    assert_eq!(error.kind(), ErrorKind::Gpu, "{error}");
    let said = error.to_string();
    assert!(
        said.contains("65 x 65") && said.contains("4225 pixels") && said.contains("4096") && said.contains("CPU"),
        "{said}"
    );
    Ok(())
}

#[test]
fn a_picture_of_one_gpu_is_refused_by_another() -> Outcome {
    let (Some(one), Some(other)) = (gpu(), fresh()) else {
        return Ok(());
    };
    let values = seeded(3, 3, 0);
    let held = block_on(one.upload(&Picture::new(3, 3, &values)?))?;
    let error = block_on(other.download(&held)).err().ok_or("another context")?;
    // a programming error of the caller (the kind a binding raises as its plain error), not "this machine cannot"
    assert!(
        error.kind() == ErrorKind::Other && error.to_string().contains("another Gpu"),
        "{error}"
    );
    let error = block_on(other.render_from(&held, &five_kernels(), &Presets::new(), Options::default()))
        .err()
        .ok_or("another context")?;
    assert!(
        error.kind() == ErrorKind::Other && error.to_string().contains("another Gpu"),
        "{error}"
    );
    Ok(())
}

/// An effect of the CPU that remembers the threads it painted rows on.
struct Painter {
    name: String,
    threads: std::sync::Mutex<std::collections::HashSet<std::thread::ThreadId>>,
}

impl bullterrier_image::Effect for Painter {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> bullterrier_image::Category {
        bullterrier_image::Category::Utilities
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        _params: &serde_json::Value,
        _inputs: &bullterrier_image::Inputs<'_>,
    ) -> Result<Image, Error> {
        Ok(picture.map_at(|_, column, pixel| {
            if column == 0 {
                self.threads
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(std::thread::current().id());
            }
            pixel
        }))
    }
}

#[test]
fn the_cpu_steps_of_a_render_kept_on_the_device_use_the_threads_asked_for() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let caller = std::thread::current().id();
    let values = seeded(1000, 40, 6);
    let photo = Photo::new(Picture::new(1000, 40, &values)?);
    let options = Options::default().with_threads(4);
    for (index, from_resident) in [false, true].into_iter().enumerate() {
        let painter = std::sync::Arc::new(Painter {
            name: format!("gpu_render_threads_{index}"),
            threads: std::sync::Mutex::default(),
        });
        bullterrier_image::register(painter.clone())?;
        let recipe = Recipe::new([Step::new(painter.name.as_str()).opacity(1.0), Step::new("levels")]);
        if from_resident {
            let start = block_on(gpu.upload(&Picture::new(1000, 40, &values)?))?;
            block_on(gpu.render_from(&start, &recipe, &Presets::new(), options))?;
        } else {
            block_on(gpu.render(&photo, &recipe, &Presets::new(), options))?;
        }
        bullterrier_image::unregister(&painter.name);
        let rows_on = painter
            .threads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        assert!(
            !rows_on.is_empty() && !rows_on.contains(&caller),
            "from a resident picture: {from_resident}"
        );
    }
    Ok(())
}

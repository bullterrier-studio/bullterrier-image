//! `finding::onnx::finder`: what it needs and what it says when a piece is missing, through the public API; and, in the
//! full tier, the real models on a real photo.

use std::path::{Path, PathBuf};

use bullterrier_image::finding::onnx::{Options, finder};
use bullterrier_image::finding::prompted::{Point, Prompts};
use bullterrier_image::finding::{DEPTH_ANYTHING_V2_SMALL, IS_NET, ModelKind, sam2};
use bullterrier_image::{Error, Finder};

use crate::common::Outcome;
use crate::common::numbers::float;

fn folder(name: &str) -> Result<PathBuf, std::io::Error> {
    let path = std::env::temp_dir().join(format!("bt-image-onnx-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&path));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Why `finder` refused, as the message.
fn refusal(options: &Options) -> String {
    match finder(&IS_NET, options) {
        Err(Error::BackendUnavailable(message)) => message,
        Err(other) => format!("another error: {other}"),
        Ok(_) => "no refusal".to_owned(),
    }
}

#[test]
fn a_model_not_in_the_cache_is_refused_saying_where_it_goes_where_it_comes_from_and_how_to_allow_it() -> Outcome {
    let cache = folder("absent")?;
    let message = refusal(&Options::new().with_cache(&cache));
    for word in [
        "isnet-general-use",
        "178 MB",
        cache.to_str().ok_or("path")?,
        IS_NET.url,
        "BT_IMAGE_ALLOW_DOWNLOAD",
    ] {
        assert!(message.contains(word), "{word}: {message}");
    }
    Ok(())
}

#[test]
fn a_file_in_the_cache_that_is_not_the_model_is_as_good_as_absent() -> Outcome {
    let cache = folder("wrong")?;
    std::fs::write(cache.join("isnet-general-use.onnx"), b"not a model")?;
    let message = refusal(&Options::new().with_cache(&cache));
    assert!(
        message.contains("is not in") && message.contains("BT_IMAGE_ALLOW_DOWNLOAD"),
        "{message}"
    );
    Ok(())
}

#[test]
fn a_local_model_that_is_no_file_is_refused_naming_it() {
    let message = refusal(&Options::new().with_model_file("/no/such/model.onnx"));
    assert!(
        message.contains("/no/such/model.onnx") && message.contains("isnet-general-use"),
        "{message}"
    );
}

#[test]
fn without_a_library_the_refusal_names_the_variable_and_the_command_that_installs_it() -> Outcome {
    let cache = folder("no-library")?;
    let local = cache.join("any.onnx");
    std::fs::write(&local, b"x")?;
    // an empty cache folder: no library is kept there, whatever this computer has installed
    let message = refusal(&Options::new().with_model_file(&local).with_cache(&cache));
    assert!(
        message.contains("BT_IMAGE_ONNX_LIB") && message.contains("bt-image install onnx"),
        "{message}"
    );
    Ok(())
}

#[test]
#[ignore = "full: needs the ONNX Runtime library file in BT_IMAGE_ONNX_LIB"]
fn a_file_that_is_no_onnx_model_is_refused_naming_it_and_the_library() -> Outcome {
    let cache = folder("garbage")?;
    let local = cache.join("garbage.onnx");
    std::fs::write(&local, b"this is not an onnx model")?;
    let library = library()?;
    let message = refusal(&Options::new().with_model_file(&local).with_library(&library));
    assert!(
        message.contains("garbage.onnx") && message.contains("cannot use"),
        "{message}"
    );
    Ok(())
}

/// The ONNX Runtime library the full tier runs with: `BT_IMAGE_ONNX_LIB`, else the one `bt-image install onnx` kept in the
/// cache folder; there must be one (the test fails, never skips).
fn library() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(named) = std::env::var_os("BT_IMAGE_ONNX_LIB") {
        return Ok(PathBuf::from(named));
    }
    let kept = bullterrier_image::finding::runtime::path(
        &bullterrier_image::finding::onnx::cache_dir()?,
        bullterrier_image::finding::runtime::here()?,
    );
    if kept.is_file() {
        Ok(kept)
    } else {
        Err(
            "set BT_IMAGE_ONNX_LIB to the ONNX Runtime library file, or run `bt-image install onnx --runtime-only`"
                .into(),
        )
    }
}

fn photo(path: &Path) -> Result<bullterrier_image::Image, Box<dyn std::error::Error>> {
    Ok(bullterrier_image::read(path)?)
}

#[test]
#[ignore = "full: runs the real models (peak 2.9 GB, alone): needs them in the cache folder and BT_IMAGE_ONNX_LIB"]
fn the_real_subject_model_finds_the_dog_and_the_real_depth_model_puts_it_nearer_than_the_window() -> Outcome {
    let image = photo(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/photos/bullterrier.jpeg"))?;
    let picture = image.as_picture();
    let (height, width) = (picture.height(), picture.width());
    let library = library()?;
    let options = |kind| Options::from_env(kind).with_library(&library);
    let subject = finder(&IS_NET, &options(ModelKind::Subject))?.find(&picture)?;
    let at = |layer: &bullterrier_image::Layer, row: usize, column: usize| layer.values()[row * width + column];
    assert!(at(&subject, height / 2, width / 2) > 0.9, "the dog's chest");
    let corner = (0..height / 10).flat_map(|row| (0..width / 10).map(move |column| (row, column)));
    assert!(
        corner
            .map(|(row, column)| at(&subject, row, column))
            .fold(0.0, f32::max)
            < 0.1,
        "the window, top left"
    );
    let depth = finder(&DEPTH_ANYTHING_V2_SMALL, &options(ModelKind::Depth))?.find(&picture)?;
    assert!(
        at(&depth, height / 2, width / 2) > at(&depth, height / 20, width / 20),
        "the dog is nearer than the window"
    );
    Ok(())
}

#[test]
#[ignore = "full: runs both real models on a banner (peak about 3.3 GB, alone): needs them in the cache folder and BT_IMAGE_ONNX_LIB"]
fn the_real_models_run_on_a_banner_within_the_memory_cap() -> Outcome {
    // 736 x 3210: before the depth input was bounded, its 518 x 2254 input took 2.7 GB and the two models together passed 4 GB
    let (height, width) = (736, 3210);
    let values: Vec<f32> = (0..height * width * 3)
        .map(|index| f32::from(u8::try_from(index % 251).unwrap_or(0)) / 251.0)
        .collect();
    let picture = bullterrier_image::Picture::new(height, width, &values)?;
    let library = library()?;
    let options = |kind| Options::from_env(kind).with_library(&library);
    let depth = finder(&DEPTH_ANYTHING_V2_SMALL, &options(ModelKind::Depth))?.find(&picture)?;
    assert_eq!((depth.height(), depth.width()), (height, width));
    // IS-Net may find nothing in noise: that is an answer too, and still ran within the cap
    let subject = finder(&IS_NET, &options(ModelKind::Subject))?.find(&picture);
    assert!(
        subject.is_ok() || matches!(subject, Err(Error::MapNotFound { .. })),
        "{subject:?}"
    );
    Ok(())
}

/// A 384 x 512 picture of noise with a brighter disc of radius 100 px at row 190, column 300, and the disc as a mask.
fn disc_on_noise() -> (Vec<f32>, Vec<bool>) {
    let (height, width) = (384_usize, 512_usize);
    let mut state = 12_345_u32; // a fixed seed: the same picture on every run
    let mut noise = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f32::from(u8::try_from(state >> 24).unwrap_or(0)) / 255.0
    };
    let mut values = Vec::with_capacity(height * width * 3);
    let mut inside = Vec::with_capacity(height * width);
    for row in 0..height {
        for column in 0..width {
            let (above, left) = (float(row) - 190.0, float(column) - 300.0);
            let in_disc = above * above + left * left <= 100.0 * 100.0;
            let level = if in_disc {
                0.75 + 0.2 * noise()
            } else {
                0.1 + 0.4 * noise()
            };
            values.extend([level; 3]);
            inside.push(in_disc);
        }
    }
    (values, inside)
}

#[test]
#[ignore = "full: runs the real SAM 2 model (peak about 3 GB, alone): needs it in the cache folder and BT_IMAGE_ONNX_LIB"]
fn the_real_sam_2_cuts_a_disc_out_of_noise_by_one_click_inside_it() -> Outcome {
    let (values, inside) = disc_on_noise();
    let picture = bullterrier_image::Picture::new(384, 512, &values)?;
    let options = Options::from_env(ModelKind::Prompted).with_library(library()?);
    let click = Point {
        x: 300.0 / 512.0,
        y: 190.0 / 384.0,
        positive: true,
    };
    let found = sam2::finder(&options, Prompts::new(vec![click], None)?)?
        .with_edge(0.0, 0.0)
        .find(&picture)?;
    let (mut both, mut either) = (0_usize, 0_usize);
    for (&sure, &disc) in found.values().iter().zip(&inside) {
        both += usize::from(sure > 0.5 && disc);
        either += usize::from(sure > 0.5 || disc);
    }
    let overlap = float(both) / float(either);
    // measured 0.997 with the large model
    assert!(overlap > 0.99, "intersection over union {overlap}");
    Ok(())
}

#[test]
#[ignore = "full: runs the real SAM 2 model twice (peak about 3 GB, alone): needs it in the cache folder and BT_IMAGE_ONNX_LIB"]
fn the_real_sam_2_cuts_what_is_clicked_the_dog_or_the_lantern_of_the_same_photo() -> Outcome {
    let image = photo(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/photos/bullterrier.jpeg"))?;
    let picture = image.as_picture();
    let width = picture.width();
    let options = Options::from_env(ModelKind::Prompted).with_library(library()?);
    let click = |x, y| Point { x, y, positive: true };
    let find = |point| -> Result<bullterrier_image::Layer, Error> {
        sam2::finder(&options, Prompts::new(vec![point], None)?)?.find(&picture)
    };
    // a place as a percentage across and down the photo
    let at = |layer: &bullterrier_image::Layer, across: usize, down: usize| {
        layer.values()[picture.height() * down / 100 * width + width * across / 100]
    };
    let dog = find(click(0.52, 0.60))?;
    assert!(
        at(&dog, 52, 60) > 0.9 && at(&dog, 50, 30) > 0.9,
        "the chest and the head are the dog"
    );
    assert!(at(&dog, 26, 14) < 0.1, "the lantern is not");
    let lantern = find(click(0.26, 0.14))?;
    assert!(at(&lantern, 26, 14) > 0.9, "the lantern");
    assert!(at(&lantern, 52, 60) < 0.1, "the dog is not");
    Ok(())
}

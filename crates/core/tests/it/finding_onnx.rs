//! `finding::onnx::finder`: what it needs and what it says when a piece is missing, through the public API; and, in the
//! full tier, the real models on a real photo.

use std::path::{Path, PathBuf};

use bullterrier_image::finding::onnx::{Options, finder};
use bullterrier_image::finding::{DEPTH_ANYTHING_V2_SMALL, IS_NET, ModelKind};
use bullterrier_image::{Error, Finder};

use crate::common::Outcome;

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
fn without_a_library_the_refusal_names_the_variable_and_the_package() -> Outcome {
    let cache = folder("no-library")?;
    let local = cache.join("any.onnx");
    std::fs::write(&local, b"x")?;
    let message = refusal(&Options::new().with_model_file(&local));
    assert!(
        message.contains("BT_IMAGE_ONNX_LIB") && message.contains("onnxruntime"),
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

/// The ONNX Runtime library the full tier runs with: `BT_IMAGE_ONNX_LIB`, which must be set (the test fails, never skips).
fn library() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let named =
        std::env::var_os("BT_IMAGE_ONNX_LIB").ok_or("set BT_IMAGE_ONNX_LIB to the ONNX Runtime library file")?;
    Ok(PathBuf::from(named))
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

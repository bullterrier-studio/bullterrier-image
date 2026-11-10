//! The identity of a finder: what its answers depend on, so a cache of them lives exactly as long as they could not change.

use bullterrier_image::Error;
use bullterrier_image::finding::onnx::Options;
use bullterrier_image::finding::{DEPTH_ANYTHING_V2_SMALL, IS_NET};
use bullterrier_image_finders::{Segmenter, depth_identity, subject_identity};

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// The SHA-256 of "abc" (FIPS 180-4's own example).
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn file(name: &str, content: &[u8]) -> Result<std::path::PathBuf, std::io::Error> {
    let folder = std::env::temp_dir().join(format!("bt-image-identity-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&folder)?;
    let path = folder.join("model.onnx");
    std::fs::write(&path, content)?;
    Ok(path)
}

#[test]
fn the_standard_models_have_the_identity_of_their_catalogued_checksum_without_being_downloaded() -> Outcome {
    assert_eq!(
        subject_identity(Some(Segmenter::Onnx), &Options::new())?,
        format!("onnx:sha256 {}", IS_NET.sha256)
    );
    assert_eq!(
        depth_identity(&Options::new())?,
        format!("onnx:sha256 {}", DEPTH_ANYTHING_V2_SMALL.sha256)
    );
    Ok(())
}

#[test]
fn a_local_model_has_the_identity_of_its_content_not_of_its_place() -> Outcome {
    let (one, other) = (file("one", b"abc")?, file("other", b"abc")?);
    assert_ne!(one, other);
    let identity = |path| subject_identity(Some(Segmenter::Onnx), &Options::new().with_model_file(path));
    assert_eq!(identity(&one)?, format!("onnx:sha256 {ABC}"));
    assert_eq!(identity(&one)?, identity(&other)?);
    assert_ne!(identity(&one)?, identity(&file("changed", b"abd")?)?);
    Ok(())
}

#[test]
fn a_local_model_that_is_no_file_is_refused_naming_it() {
    let refused = depth_identity(&Options::new().with_model_file("/no/such/model.onnx"));
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("/no/such/model.onnx")),
        "{refused:?}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn on_macos_the_identity_of_apple_vision_names_the_macos_version_and_differs_from_the_portable_models() -> Outcome {
    let vision = subject_identity(Some(Segmenter::Vision), &Options::new())?;
    assert!(
        vision.starts_with("vision:macOS 1") || vision.starts_with("vision:macOS 2"),
        "{vision}"
    );
    assert_ne!(vision, subject_identity(Some(Segmenter::Onnx), &Options::new())?);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[test]
fn off_macos_the_identity_of_apple_vision_is_refused_like_its_use() {
    let refused = subject_identity(Some(Segmenter::Vision), &Options::new());
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("macOS only")),
        "{refused:?}"
    );
}

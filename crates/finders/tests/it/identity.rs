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

/// The ONNX Runtime library file this computer is told to use (`BT_IMAGE_ONNX_LIB`), which the identity of a portable
/// model names the version of.
fn library() -> Result<std::path::PathBuf, &'static str> {
    std::env::var_os("BT_IMAGE_ONNX_LIB")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .ok_or("set BT_IMAGE_ONNX_LIB to an ONNX Runtime library file")
}

/// What an identity of a portable model starts with: the model's checksum, then the runtime that runs it.
fn model_part(sha256: &str) -> String {
    format!("onnx:sha256 {sha256}; runtime ")
}

#[test]
#[ignore = "full: needs the ONNX Runtime library file in BT_IMAGE_ONNX_LIB"]
fn the_standard_models_have_the_identity_of_their_catalogued_checksum_and_the_runtime_without_being_downloaded()
-> Outcome {
    let options = Options::new().with_library(library()?);
    let subject = subject_identity(Some(Segmenter::Onnx), &options)?;
    let depth = depth_identity(&options)?;
    assert!(subject.starts_with(&model_part(IS_NET.sha256)), "{subject}");
    assert!(
        depth.starts_with(&model_part(DEPTH_ANYTHING_V2_SMALL.sha256)),
        "{depth}"
    );
    // the runtime part is what the loaded library says of itself: the commit it was built from names its release
    let runtime = &subject[model_part(IS_NET.sha256).len()..];
    assert!(runtime.contains("git-commit-id="), "{runtime}");
    assert_eq!(
        runtime,
        &depth[model_part(DEPTH_ANYTHING_V2_SMALL.sha256).len()..],
        "one runtime in a process"
    );
    Ok(())
}

#[test]
#[ignore = "full: needs the ONNX Runtime library file in BT_IMAGE_ONNX_LIB"]
fn a_local_model_has_the_identity_of_its_content_not_of_its_place() -> Outcome {
    let library = library()?;
    let (one, other) = (file("one", b"abc")?, file("other", b"abc")?);
    assert_ne!(one, other);
    let identity = |path| {
        subject_identity(
            Some(Segmenter::Onnx),
            &Options::new().with_library(&library).with_model_file(path),
        )
    };
    assert!(identity(&one)?.starts_with(&model_part(ABC)));
    assert_eq!(identity(&one)?, identity(&other)?);
    assert_ne!(identity(&one)?, identity(&file("changed", b"abd")?)?);
    Ok(())
}

#[test]
fn a_portable_model_has_no_identity_without_a_runtime_to_name_the_version_of() {
    let refused = depth_identity(&Options::new().with_library("/no/such/libonnxruntime.so"));
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("/no/such/libonnxruntime.so") && message.contains("bt-image install onnx")),
        "{refused:?}"
    );
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
fn on_macos_the_identity_of_apple_vision_names_the_macos_version_and_is_no_portable_models() -> Outcome {
    let vision = subject_identity(Some(Segmenter::Vision), &Options::new())?;
    assert!(
        vision.starts_with("vision:macOS 1") || vision.starts_with("vision:macOS 2"),
        "{vision}"
    );
    assert!(!vision.starts_with("onnx"), "{vision}");
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

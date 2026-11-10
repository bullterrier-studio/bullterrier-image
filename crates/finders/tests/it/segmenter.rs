//! The choice of a segmenter, through the public API: by what the platform has, never by trying and falling back.

use bullterrier_image::Error;
use bullterrier_image::finding::onnx::Options;
use bullterrier_image_finders::{Segmenter, subject_finder};

#[test]
fn the_default_is_vision_where_the_platform_has_it_and_the_portable_model_elsewhere() {
    // this test runs on the three platforms; macOS 14 and later is what the build machines have
    let expected = if cfg!(target_os = "macos") {
        Segmenter::Vision
    } else {
        Segmenter::Onnx
    };
    assert_eq!(Segmenter::default_here(), expected);
}

#[test]
fn the_portable_model_without_its_file_says_how_to_get_it_and_does_not_fall_back_to_vision() {
    let cache = std::env::temp_dir().join(format!("bt-image-seg-{}", std::process::id()));
    let refused = subject_finder(Some(Segmenter::Onnx), &Options::new().with_cache(&cache), (1.5, 1.0));
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("isnet-general-use") && message.contains("BT_IMAGE_ALLOW_DOWNLOAD")),
        "{:?}",
        refused.err()
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn vision_asked_for_off_macos_is_refused_naming_the_fix() {
    let refused = subject_finder(Some(Segmenter::Vision), &Options::new(), (1.5, 1.0));
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("macOS only") && message.contains("onnx")),
        "{:?}",
        refused.err()
    );
}

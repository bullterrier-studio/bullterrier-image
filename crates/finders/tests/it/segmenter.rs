//! The choice of a segmenter, through the public API: by what the platform has, never by trying and falling back.

use bullterrier_image::Error;
use bullterrier_image::finding::onnx::Options;
use bullterrier_image::finding::prompted::{Point, Prompts};
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
    let refused = subject_finder(
        Some(Segmenter::Onnx),
        &Options::new().with_cache(&cache),
        (1.5, 1.0),
        None,
    );
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("isnet-general-use") && message.contains("BT_IMAGE_ALLOW_DOWNLOAD")),
        "{:?}",
        refused.err()
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn vision_asked_for_off_macos_is_refused_naming_the_fix() {
    let refused = subject_finder(Some(Segmenter::Vision), &Options::new(), (1.5, 1.0), None);
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("macOS only") && message.contains("onnx")),
        "{:?}",
        refused.err()
    );
}

fn click() -> Result<Prompts, Error> {
    Prompts::new(
        vec![Point {
            x: 0.5,
            y: 0.5,
            positive: true,
        }],
        None,
    )
}

#[test]
fn sam_2_without_a_click_or_a_box_is_refused_naming_points_before_any_model_is_looked_for() {
    let cache = std::env::temp_dir().join(format!("bt-image-seg-sam2-{}", std::process::id()));
    let refused = subject_finder(
        Some(Segmenter::Sam2),
        &Options::new().with_cache(&cache),
        (1.5, 1.0),
        None,
    );
    assert!(
        matches!(&refused, Err(Error::Inputs(message)) if message.contains("points") && message.contains("click")),
        "{:?}",
        refused.err()
    );
}

#[test]
fn clicks_given_to_a_segmenter_that_finds_the_subject_itself_are_refused_naming_points() -> Result<(), Error> {
    let prompts = click()?;
    for segmenter in [Segmenter::Vision, Segmenter::Onnx] {
        let refused = subject_finder(Some(segmenter), &Options::new(), (1.5, 1.0), Some(&prompts));
        assert!(
            matches!(&refused, Err(Error::Inputs(message)) if message.contains("points") && message.contains("sam2")),
            "{}: {:?}",
            segmenter.name(),
            refused.err()
        );
    }
    Ok(())
}

#[test]
fn sam_2_without_its_files_says_where_they_go_and_how_to_allow_the_download() -> Result<(), Error> {
    let cache = std::env::temp_dir().join(format!("bt-image-seg-sam2-files-{}", std::process::id()));
    let refused = subject_finder(
        Some(Segmenter::Sam2),
        &Options::new().with_cache(&cache),
        (1.5, 1.0),
        Some(&click()?),
    );
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("vision_encoder") && message.contains("BT_IMAGE_ALLOW_DOWNLOAD")),
        "{:?}",
        refused.err()
    );
    Ok(())
}

#[test]
fn sam_2_takes_no_local_model_file() -> Result<(), Error> {
    let refused = subject_finder(
        Some(Segmenter::Sam2),
        &Options::new().with_model_file("/some/model.onnx"),
        (1.5, 1.0),
        Some(&click()?),
    );
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("two model files")),
        "{:?}",
        refused.err()
    );
    Ok(())
}

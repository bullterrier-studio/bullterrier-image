//! `bt-image models`, `install onnx` and `doctor`. Nothing here downloads: the questions are declined, and the folders
//! downloads are kept in are the test's own.

use bullterrier_image::finding::runtime::{self, VERSION};

use crate::common::{Outcome, bt, bt_answering, folder, text};

#[test]
fn models_lists_each_model_with_where_it_comes_from_and_whether_it_is_installed() -> Outcome {
    let cache = folder("models")?;
    let dir = cache.to_string_lossy().into_owned();
    let ran = bt(&["models", "--cache", &dir]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.starts_with(&format!("Models, kept in {dir}\n")), "{}", ran.out);
    assert!(
        ran.out
            .contains("  isnet-general-use: subject model, 179 MB, not installed\n"),
        "{}",
        ran.out
    );
    assert!(
        ran.out
            .contains("  depth-anything-v2-small: depth model, 100 MB, not installed\n"),
        "{}",
        ran.out
    );
    assert!(
        ran.out
            .contains("  sam2.1-hiera-large/vision_encoder: prompted model, 890 MB, not installed\n"),
        "{}",
        ran.out
    );
    assert!(
        ran.out
            .contains("    sha256: 60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a\n")
    );
    assert!(ran.out.contains("    url: https://github.com/danielgatis/rembg/"));
    text(&cache.join("isnet-general-use.onnx"), "not the model")?;
    let damaged = bt(&["models", "--cache", &dir]);
    assert!(
        damaged
            .out
            .contains("isnet-general-use: subject model, 179 MB, damaged: it is not the model"),
        "{}",
        damaged.out
    );
    Ok(())
}

#[test]
fn install_asks_what_it_will_download_and_where_and_downloads_nothing_unless_told_to() -> Outcome {
    let cache = folder("install")?;
    let dir = cache.to_string_lossy().into_owned();
    for answers in ["n\n", "\n", ""] {
        let ran = bt_answering(&["install", "onnx", "--cache", &dir], answers);
        assert_eq!((ran.code, ran.out.as_str()), (1, ""), "{answers:?}");
        let question = format!("download ONNX Runtime {VERSION} (");
        assert!(ran.err.starts_with(&question), "{}", ran.err);
        assert!(
            ran.err.contains(" MB) and the isnet-general-use model (179 MB) into "),
            "{}",
            ran.err
        );
        assert!(ran.err.contains(&format!("{dir}? [y/N] ")), "{}", ran.err);
        assert!(ran.err.ends_with("nothing downloaded\n"), "{}", ran.err);
    }
    assert_eq!(std::fs::read_dir(&cache)?.count(), 0, "nothing was written");
    let with_depth = bt_answering(&["install", "onnx", "--depth", "--cache", &dir], "no\n");
    assert!(
        with_depth
            .err
            .contains("and the depth-anything-v2-small model (100 MB)"),
        "{}",
        with_depth.err
    );
    let with_prompted = bt_answering(&["install", "onnx", "--prompted", "--cache", &dir], "no\n");
    assert!(
        with_prompted
            .err
            .contains("and the sam2.1-hiera-large/vision_encoder model (890 MB) and the sam2.1-hiera-large/prompt_encoder_mask_decoder model (22 MB) into "),
        "{}",
        with_prompted.err
    );
    Ok(())
}

#[test]
fn what_is_installed_is_not_asked_for_again() -> Outcome {
    let cache = folder("installed")?;
    let library = runtime::path(&cache, runtime::here()?);
    text(&library, "a library")?;
    let ran = bt_answering(&["install", "onnx", "--cache", &cache.to_string_lossy()], "n\n");
    assert!(
        ran.err
            .starts_with("download the isnet-general-use model (179 MB) into "),
        "{}",
        ran.err
    );
    assert!(!ran.err.contains("ONNX Runtime"), "{}", ran.err);
    Ok(())
}

#[test]
fn doctor_reports_each_backend_and_exits_1_when_the_one_in_use_does_not_work() -> Outcome {
    let cache = folder("doctor")?;
    let ran = bt(&["doctor", "--cache", &cache.to_string_lossy()]);
    for line in [
        "cache folder: ",
        "subject finder in use: ",
        "Apple Vision: ",
        &format!("ONNX Runtime {VERSION}: not available: "),
        "isnet-general-use model (subject): not installed\n",
        "depth-anything-v2-small model (depth): not installed\n",
    ] {
        assert!(ran.out.contains(line), "{line}: {}", ran.out);
    }
    let vision = ran.out.contains("Apple Vision: works");
    let in_use_is_vision = ran.out.contains("subject finder in use: vision");
    assert_eq!(in_use_is_vision, vision || cfg!(target_os = "macos"), "{}", ran.out);
    // an empty cache has no model: the portable finder cannot work, Apple Vision needs none
    assert_eq!(ran.code == 0, in_use_is_vision && vision, "{}{}", ran.out, ran.err);
    if ran.code == 1 {
        assert!(ran.err.contains("does not work"), "{}", ran.err);
        assert!(
            ran.err.contains("bt-image install onnx") || ran.err.contains("BT_IMAGE_SEGMENTER=onnx"),
            "{}",
            ran.err
        );
    }
    Ok(())
}

#[test]
fn the_runtime_alone_can_be_asked_for_and_then_no_model_is_in_the_question() -> Outcome {
    let cache = folder("runtime-only")?;
    let ran = bt_answering(
        &["install", "onnx", "--runtime-only", "--cache", &cache.to_string_lossy()],
        "n\n",
    );
    assert!(
        ran.err.starts_with(&format!("download ONNX Runtime {VERSION} (")),
        "{}",
        ran.err
    );
    assert!(!ran.err.contains("model"), "{}", ran.err);
    let both = bt(&["install", "onnx", "--runtime-only", "--depth"]);
    assert_eq!(both.code, 2, "{}", both.err);
    Ok(())
}

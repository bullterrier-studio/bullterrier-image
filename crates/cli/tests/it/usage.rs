//! Usage: `--version`, `--help`, and a wrong command line.

use crate::common::{Outcome, bt, folder, photo};

#[test]
fn version_prints_the_packages_version_and_succeeds() {
    let ran = bt(&["--version"]);
    assert_eq!((ran.code, ran.out.trim(), ran.err.as_str()), (0, "bt-image 9.9.9", ""));
}

#[test]
fn help_lists_the_commands_and_the_exit_codes_on_stdout() {
    let ran = bt(&["--help"]);
    assert_eq!(ran.code, 0);
    for word in ["init", "run", "exit codes:", "130"] {
        assert!(ran.out.contains(word), "{word}: {}", ran.out);
    }
}

#[test]
fn a_wrong_command_line_is_usage_error_2_explained_on_stderr() {
    for args in [
        &["frobnicate"][..],
        &[][..],
        &["run", "--nonsense"][..],
        &["run", "--segmenter", "gpu"][..],
    ] {
        let ran = bt(args);
        assert_eq!(ran.code, 2, "{args:?}");
        assert_eq!(ran.out, "", "{args:?}");
        assert!(
            ran.err.contains("error") || ran.err.contains("Usage"),
            "{args:?}: {}",
            ran.err
        );
    }
    let ran = bt(&["run", "--segmenter", "gpu"]);
    assert!(ran.err.contains("vision, onnx"), "the choices are named: {}", ran.err);
}

/// What `cutout` says (in a folder of its own, `name`: tests run side by side) when the command line asks `sam2` something it cannot do, for a real photo and no model.
fn cutout_refusal(name: &str, extra: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let root = folder(name)?;
    let file = root.join("photo.png");
    photo(&file, false)?;
    let out = root.join("out.png");
    let mut args = vec![
        "cutout",
        file.to_str().ok_or("path")?,
        "--out",
        out.to_str().ok_or("path")?,
    ];
    args.extend_from_slice(extra);
    let ran = bt(&args);
    assert_ne!(ran.code, 0, "{extra:?}");
    assert!(!out.exists(), "{extra:?}");
    Ok(ran.err)
}

#[test]
fn sam_2_asked_without_a_click_or_a_box_is_refused_naming_points() -> Outcome {
    let err = cutout_refusal("sam2-bare", &["--segmenter", "sam2"])?;
    assert!(err.contains("points") && err.contains("click"), "{err}");
    Ok(())
}

#[test]
fn a_click_outside_the_photo_is_refused_naming_the_click_and_the_value() -> Outcome {
    let err = cutout_refusal(
        "sam2-outside",
        &["--segmenter", "sam2", "--point", "0.5,0.5", "--avoid", "1.5,0.2"],
    )?;
    assert!(err.contains("points[1].x") && err.contains("1.5"), "{err}");
    Ok(())
}

#[test]
fn clicks_for_a_segmenter_that_finds_the_subject_itself_are_refused() -> Outcome {
    let err = cutout_refusal("sam2-onnx", &["--segmenter", "onnx", "--box", "0.1,0.1,0.9,0.9"])?;
    assert!(err.contains("points") && err.contains("sam2"), "{err}");
    Ok(())
}

#[test]
fn a_click_that_is_not_two_numbers_is_a_usage_error() {
    for bad in ["0.5", "a,b", "0.1,0.2,0.3"] {
        let ran = bt(&["cutout", "photo.png", "--point", bad]);
        assert_eq!(ran.code, 2, "{bad}");
        assert!(ran.err.contains("expected 2 numbers"), "{bad}: {}", ran.err);
    }
}

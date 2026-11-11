//! `bt-image run`: the report, the exit codes, and what each flag changes.

use crate::common::{Outcome, bt, folder, photo, project};

#[test]
fn a_run_writes_the_results_and_prints_the_report_page() -> Outcome {
    let root = folder("run")?;
    let file = project(&root, false)?;
    let ran = bt(&["run", &file.to_string_lossy()]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), root.join("out/index.html").to_string_lossy());
    assert!(root.join("out/a/warm.jpg").is_file() && root.join("out/index.html").is_file());
    assert!(ran.err.contains("1 results: 0 failed, 0 with warnings"), "{}", ran.err);
    Ok(())
}

#[test]
fn quiet_hides_the_progress_and_a_clean_summary_but_not_the_report_path() -> Outcome {
    let root = folder("quiet")?;
    let file = project(&root, false)?;
    let ran = bt(&["run", "-q", &file.to_string_lossy()]);
    assert_eq!((ran.code, ran.err.as_str()), (0, ""));
    assert!(ran.out.contains("index.html"));
    Ok(())
}

#[test]
fn a_dry_run_writes_nothing_and_prints_no_report_path() -> Outcome {
    let root = folder("dry")?;
    let file = project(&root, false)?;
    let ran = bt(&["run", "--dry-run", &file.to_string_lossy()]);
    assert_eq!((ran.code, ran.out.as_str()), (0, ""), "{}", ran.err);
    assert!(!root.join("out").exists());
    Ok(())
}

#[test]
fn a_failed_result_exits_1_and_the_summary_shows_even_with_quiet() -> Outcome {
    let root = folder("failed")?;
    let file = project(&root, false)?;
    std::fs::write(root.join("photos/broken.png"), b"not a picture")?;
    let ran = bt(&["run", "-q", &file.to_string_lossy()]);
    assert_eq!(ran.code, 1, "{}", ran.err);
    assert!(ran.err.contains("2 results: 1 failed, 0 with warnings"), "{}", ran.err);
    Ok(())
}

#[test]
fn a_warning_is_exit_0_unless_strict() -> Outcome {
    let root = folder("strict")?;
    let file = project(&root, true)?; // a flat photo: the result is flat, which the checks flag
    let file = file.to_string_lossy();
    let lax = bt(&["run", &file]);
    assert_eq!(lax.code, 0, "{}", lax.err);
    assert!(lax.err.contains("1 with warnings"), "{}", lax.err);
    let strict = bt(&["run", "--strict", "--force", &file]);
    assert_eq!(strict.code, 1, "{}", strict.err);
    Ok(())
}

#[test]
fn a_project_with_no_photos_is_an_error_naming_the_problem() -> Outcome {
    let root = folder("empty")?;
    let file = project(&root, false)?;
    std::fs::remove_file(root.join("photos/a.png"))?;
    let ran = bt(&["run", &file.to_string_lossy()]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.err.starts_with("error: ") && ran.err.contains("no photos match"),
        "{}",
        ran.err
    );
    Ok(())
}

#[test]
fn only_selects_photos_by_file_name_and_steps_adds_the_step_sheet() -> Outcome {
    let root = folder("only")?;
    let file = project(&root, false)?;
    photo(&root.join("photos/b.png"), false)?;
    let ran = bt(&["run", "--only", "b*", "--steps", &file.to_string_lossy()]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.err.contains("1 results"), "{}", ran.err);
    assert!(root.join("out/b/steps/warm/_sheet.jpg").is_file());
    assert!(!root.join("out/a").exists(), "a.png does not match b*");
    Ok(())
}

#[test]
fn debug_prints_the_error_in_its_debug_form() -> Outcome {
    let root = folder("debug")?;
    let ran = bt(&["run", "--debug", &root.join("nothing.toml").to_string_lossy()]);
    assert_eq!(ran.code, 1);
    assert!(ran.err.starts_with("error: Project("), "{}", ran.err);
    Ok(())
}

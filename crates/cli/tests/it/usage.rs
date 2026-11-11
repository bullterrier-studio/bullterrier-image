//! Usage: `--version`, `--help`, and a wrong command line.

use crate::common::bt;

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

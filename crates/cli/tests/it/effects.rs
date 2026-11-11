//! `bt-image effects`: the catalogue, listed and in full.

use crate::common::bt;

/// The unindented lines of `out`: the headings.
fn headings(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|line| !line.is_empty() && !line.starts_with(' '))
        .collect()
}

#[test]
fn the_catalogue_lists_the_looks_then_one_line_per_effect_and_says_where_the_details_are() {
    let ran = bt(&["effects"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let titles = headings(&ran.out);
    assert!(titles[0].starts_with("Looks (recipes of several steps"), "{titles:?}");
    assert_eq!(titles[1], "Effects");
    assert!(ran.out.contains("\n  crt: "), "a look: {}", ran.out);
    assert!(ran.out.contains("\n  grade: "), "an effect");
    assert!(ran.out.contains("(presets: "), "an effect with presets names them");
    assert!(ran.out.trim_end().ends_with("details: bt-image effects NAME, or --all"));
}

#[test]
fn one_effect_is_printed_in_full_and_only_that_one() {
    let ran = bt(&["effects", "grade"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(headings(&ran.out), ["grade"]);
    assert!(
        ran.out.contains(
            "\n    Parameters\n    ----------\n    temperature : number (drivable), stops, -2 to 2, default 0.0\n"
        ),
        "{}",
        ran.out
    );
    assert!(ran.out.contains("Presets\n    -------\n"), "{}", ran.out);
    assert!(!ran.out.contains("Advanced parameters"), "grade has none: {}", ran.out);
    assert!(!ran.out.contains("\n  crt: "), "no looks beside one effect");
}

#[test]
fn a_category_lists_only_its_effects_and_all_prints_them_in_full_without_the_looks() {
    let short = bt(&["effects", "--category", "light"]);
    assert_eq!(short.code, 0, "{}", short.err);
    assert_eq!(headings(&short.out)[0], "Effects in light");
    assert!(
        short.out.contains("\n  bloom: ") && !short.out.contains("\n  grade: ") && !short.out.contains("\n  crt: ")
    );
    let full = bt(&["effects", "--category", "light", "--all"]);
    assert!(
        headings(&full.out).contains(&"bloom") && !headings(&full.out).contains(&"grade"),
        "{}",
        full.out
    );
    let advanced = bt(&["effects", "--advanced", "--category", "glitch"]);
    assert!(
        advanced.out.contains("Advanced parameters\n    -------------------\n"),
        "{}",
        advanced.out
    );
    let basic = bt(&["effects", "--all", "--category", "glitch"]);
    assert!(
        basic.out.contains("advanced)") && !basic.out.contains("Advanced parameters"),
        "{}",
        basic.out
    );
}

#[test]
fn the_categories_are_listed_with_their_counts_and_what_each_holds() {
    let ran = bt(&["effects", "--list-categories"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let first = ran.out.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("colour ") && first.contains("Grades, gradient maps"),
        "{first}"
    );
    assert_eq!(
        ran.out.lines().count(),
        11,
        "the categories that have effects: {}",
        ran.out
    );
}

#[test]
fn what_is_no_effect_says_what_it_is_instead_and_what_it_is_near() {
    for (name, says) in [
        ("glitch", "fx.slice_shift(preset=\"glitch\")"),
        (
            "glitch",
            "a look (load_recipe(\"glitch\"), `bt-image render PHOTO glitch`)",
        ),
        ("exposure", "a parameter of "),
        ("slice_shif", "did you mean: slice_shift (effect)"),
        ("zzzzqq", "unknown effect 'zzzzqq'; available: "),
    ] {
        let ran = bt(&["effects", name]);
        assert_eq!((ran.code, ran.out.as_str()), (1, ""), "{name}");
        assert!(
            ran.err.starts_with("error: ") && ran.err.contains(says),
            "{name}: {}",
            ran.err
        );
    }
}

#[test]
fn an_unknown_category_names_the_ones_there_are_and_the_options_exclude_each_other() {
    let ran = bt(&["effects", "--category", "nothing"]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.err
            .contains("no effects in category 'nothing'; categories with effects: colour, light"),
        "{}",
        ran.err
    );
    for args in [
        &["effects", "grade", "--category", "light"][..],
        &["effects", "--category", "light", "--list-categories"][..],
    ] {
        let said = bt(args);
        assert_eq!(said.code, 2, "{args:?}");
        assert!(said.err.contains("cannot be used with"), "{args:?}: {}", said.err);
    }
}

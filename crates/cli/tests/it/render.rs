//! `bt-image render`, `sheet` and `steps`: a photo through recipes.

use std::path::{Path, PathBuf};

use crate::common::{Outcome, bt, folder, halves, photo, pixel, text};

const GRAIN: &str = "size = 32\n[[step]]\neffect = \"grade\"\nexposure = 0.1\n";

/// A folder with the photo `me.png` (24 x 32) and the recipes `recipes/a.toml` and `recipes/b.toml`; the paths of the commands
/// are absolute.
fn scene(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = folder(name)?;
    photo(&root.join("me.png"), false)?;
    for recipe in ["a", "b"] {
        text(
            &root.join(format!("recipes/{recipe}.toml")),
            &format!("name = \"{recipe}\"\n{GRAIN}"),
        )?;
    }
    Ok(root)
}

fn at(root: &Path, relative: &str) -> String {
    root.join(relative).to_string_lossy().into_owned()
}

#[test]
fn render_writes_one_file_per_recipe_in_a_folder_and_prints_each_to_stdout() -> Outcome {
    let root = scene("render")?;
    let ran = bt(&[
        "render",
        &at(&root, "me.png"),
        &at(&root, "recipes"),
        "-o",
        &at(&root, "out"),
    ]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let mut names: Vec<String> = std::fs::read_dir(root.join("out"))?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    names.sort();
    assert_eq!(names, ["me__a.png", "me__b.png"]);
    assert_eq!(
        ran.out.lines().collect::<Vec<_>>(),
        [at(&root, "out/me__a.png"), at(&root, "out/me__b.png")]
    );
    assert!(ran.err.contains("rendering 2 recipe(s)..."), "{}", ran.err);
    Ok(())
}

#[test]
fn a_look_is_accepted_wherever_a_recipe_file_is() -> Outcome {
    let root = scene("look")?;
    let ran = bt(&["render", &at(&root, "me.png"), "glitch", "-o", &at(&root, "out")]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(root.join("out/me__glitch.png").is_file());
    let unknown = bt(&["render", &at(&root, "me.png"), "glitchy", "-o", &at(&root, "out2")]);
    assert_eq!(unknown.code, 1);
    assert!(
        unknown.err.contains("no look or file 'glitchy'") && unknown.err.contains("did you mean 'glitch'"),
        "{}",
        unknown.err
    );
    Ok(())
}

#[test]
fn each_file_is_named_after_its_recipes_name_made_safe() -> Outcome {
    // worked out by hand: a run of characters a file name may not hold is one `-`, the ends are trimmed
    for (name, file) in [
        ("my look", "my-look"),
        ("  --Holiday  (2024)!!", "Holiday-2024"),
        ("v1.2 final", "v1.2-final"),
        ("_draft_", "draft_"),
    ] {
        let root = scene("safe")?;
        text(&root.join("r.toml"), &format!("name = \"{name}\"\n{GRAIN}"))?;
        let ran = bt(&[
            "render",
            &at(&root, "me.png"),
            &at(&root, "r.toml"),
            "-o",
            &at(&root, "out"),
        ]);
        assert_eq!(ran.code, 0, "{name}: {}", ran.err);
        assert!(root.join(format!("out/me__{file}.png")).is_file(), "{name}");
    }
    Ok(())
}

#[test]
fn a_name_that_leaves_no_file_name_fails_before_anything_is_rendered_or_written() -> Outcome {
    for command in ["render", "sheet"] {
        let root = scene("badname")?;
        text(&root.join("bad.toml"), &format!("name = \"日本\"\n{GRAIN}"))?;
        let target = at(&root, if command == "sheet" { "out.png" } else { "out" });
        let (me, a, bad) = (at(&root, "me.png"), at(&root, "recipes/a.toml"), at(&root, "bad.toml"));
        let ran = bt(&[command, &me, &a, &bad, "-o", &target]);
        assert_eq!(ran.code, 1, "{command}");
        assert!(
            ran.err.contains("bad.toml") && ran.err.contains("name="),
            "{command}: {}",
            ran.err
        );
        assert!(
            !root.join("out").exists() && !root.join("out.png").exists(),
            "{command}: nothing was written"
        );
        assert_eq!(
            bt(&[command, &me, &a, "-o", &target]).code,
            0,
            "{command}: the known-good side"
        );
    }
    Ok(())
}

#[test]
fn a_recipe_with_transparency_is_written_with_alpha_refused_for_jpg_and_shown_on_a_checkerboard_in_a_sheet() -> Outcome
{
    let root = scene("avatar")?;
    text(
        &root.join("avatar.toml"),
        &format!("name = \"avatar\"\n{GRAIN}[frame]\ntransparent = \"circle\"\n"),
    )?;
    let (me, avatar) = (at(&root, "me.png"), at(&root, "avatar.toml"));
    assert_eq!(bt(&["render", &me, &avatar, "-o", &at(&root, "out")]).code, 0);
    let (corner, centre) = (
        pixel(&root.join("out/me__avatar.png"), 0, 0)?,
        pixel(&root.join("out/me__avatar.png"), 12, 16)?,
    );
    assert_eq!((corner.len(), corner[3], centre[3]), (4, 0.0, 1.0));
    let jpg = bt(&["render", &me, &avatar, "-o", &at(&root, "out2"), "--format", "jpg"]);
    assert_eq!(jpg.code, 1);
    assert!(
        jpg.err.contains("transparency") && !jpg.err.contains("Traceback"),
        "{}",
        jpg.err
    );
    let plain = bt(&[
        "render",
        &me,
        &at(&root, "recipes/a.toml"),
        "-o",
        &at(&root, "out3"),
        "--format",
        "jpg",
    ]);
    assert_eq!(plain.code, 0, "{}", plain.err);
    assert_eq!(bt(&["sheet", &me, &avatar, "-o", &at(&root, "s.png")]).code, 0);
    assert_eq!(
        pixel(&root.join("s.png"), 15, 15)?.len(),
        3,
        "the sheet is an RGB picture"
    );
    Ok(())
}

#[test]
fn sheet_combines_recipes_into_one_image_in_columns() -> Outcome {
    let root = scene("sheet")?;
    let ran = bt(&[
        "sheet",
        &at(&root, "me.png"),
        &at(&root, "recipes"),
        "-o",
        &at(&root, "deep/s.png"),
        "--columns",
        "1",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), at(&root, "deep/s.png"));
    let sheet = bullterrier_image::read(&root.join("deep/s.png"))?;
    // one column of two 480 px tiles, each with a 30 px label row, and the gaps of 14 px
    assert_eq!(
        (sheet.width(), sheet.height()),
        (14 + 480 + 14, 14 + 2 * (480 + 30 + 14))
    );
    for columns in ["0", "-2", "many"] {
        let refused = bt(&[
            "sheet",
            &at(&root, "me.png"),
            &at(&root, "recipes"),
            &format!("--columns={columns}"),
        ]);
        assert_eq!(refused.code, 2, "{columns}");
        assert!(refused.err.contains("--columns"), "{columns}: {}", refused.err);
    }
    Ok(())
}

#[test]
fn steps_writes_a_picture_per_stage_and_the_sheet_and_prints_the_sheet() -> Outcome {
    let root = scene("steps")?;
    let ran = bt(&[
        "steps",
        &at(&root, "me.png"),
        &at(&root, "recipes/a.toml"),
        "-o",
        &at(&root, "st"),
    ]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), at(&root, "st/_sheet.jpg"));
    assert!(
        ran.err.starts_with("  input\n  1. grade(exposure=0.1)\n"),
        "each stage is named as it is written: {}",
        ran.err
    );
    assert!(root.join("st/00-input.jpg").is_file() && root.join("st/01-grade.jpg").is_file());
    Ok(())
}

#[test]
fn a_subject_map_file_keeps_exactly_the_subject_it_marks() -> Outcome {
    let root = scene("matte")?;
    text(
        &root.join("cut.toml"),
        "name = \"cut\"\nsize = 32\n[[step]]\neffect = \"gradient\"\ncolors = [\"#000000\"]\nmask = \"background\"\n",
    )?;
    halves(&root.join("matte.png"), (24, 32), (0.0, 1.0))?;
    let (me, cut, matte) = (
        at(&root, "me.png"),
        at(&root, "cut.toml"),
        format!("subject={}", at(&root, "matte.png")),
    );
    let ran = bt(&["render", &me, &cut, "-o", &at(&root, "out"), "--map", &matte]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let out = root.join("out/me__cut.png");
    assert_eq!(pixel(&out, 5, 3)?, [0.0, 0.0, 0.0], "background: filled black");
    let (kept, original) = (pixel(&out, 5, 28)?, pixel(&root.join("me.png"), 5, 28)?);
    for (a, b) in kept.iter().zip(&original) {
        assert!((a - b).abs() < 1.0 / 255.0, "subject: untouched");
    }
    let steps = bt(&["steps", &me, &cut, "-o", &at(&root, "st"), "--map", &matte]);
    assert_eq!(steps.code, 0, "{}", steps.err);
    Ok(())
}

#[test]
fn a_map_of_the_recipes_own_is_given_by_name_and_one_nobody_gave_is_refused_with_the_fix() -> Outcome {
    let root = scene("given")?;
    text(
        &root.join("star.toml"),
        "format = 3\nname = \"star\"\nsize = 32\n[region.star]\nkind = \"given\"\n[[step]]\neffect = \"gradient\"\ncolors = [\"#000000\"]\nmask = \"star\"\n",
    )?;
    halves(&root.join("star.png"), (24, 32), (1.0, 0.0))?;
    let (me, star) = (at(&root, "me.png"), at(&root, "star.toml"));
    let with = bt(&[
        "render",
        &me,
        &star,
        "-o",
        &at(&root, "out"),
        "--map",
        &format!("star={}", at(&root, "star.png")),
    ]);
    assert_eq!(with.code, 0, "{}", with.err);
    assert_eq!(
        pixel(&root.join("out/me__star.png"), 5, 3)?,
        [0.0, 0.0, 0.0],
        "black where the map is white"
    );
    let without = bt(&["render", &me, &star, "-o", &at(&root, "out2")]);
    assert_eq!(without.code, 1);
    assert!(
        without.err.contains("'star'") || without.err.contains("\"star\""),
        "{}",
        without.err
    );
    assert!(
        without.err.contains("--map star=PICTURE"),
        "the fix is named: {}",
        without.err
    );
    Ok(())
}

#[test]
fn a_mistake_in_a_map_option_ends_in_one_error_line_naming_it() -> Outcome {
    let root = scene("mapmistake")?;
    text(
        &root.join("star.toml"),
        "format = 3\nname = \"star\"\nsize = 32\n[region.star]\nkind = \"given\"\n[[step]]\neffect = \"gradient\"\ncolors = [\"#000000\"]\nmask = \"star\"\n",
    )?;
    halves(&root.join("star.png"), (24, 32), (1.0, 0.0))?;
    let (me, star, map) = (at(&root, "me.png"), at(&root, "star.toml"), at(&root, "star.png"));
    let out = at(&root, "out");
    for (extra, named) in [
        (vec!["--map".to_owned(), format!("strar={map}")], "strar"),
        (
            vec![
                "--map".to_owned(),
                format!("star={map}"),
                "--map".to_owned(),
                format!("star={map}"),
            ],
            "twice",
        ),
        (vec!["--map".to_owned(), "star=/gone.png".to_owned()], "gone.png"),
    ] {
        let mut args = vec!["render", &me, &star, "-o", &out];
        args.extend(extra.iter().map(String::as_str));
        let ran = bt(&args);
        assert_eq!(ran.code, 1, "{named}");
        assert!(
            ran.err
                .lines()
                .last()
                .is_some_and(|line| line.starts_with("error: ") && line.contains(named)),
            "{named}: {}",
            ran.err
        );
    }
    for option in ["star", "=star.png", "star="] {
        let ran = bt(&["render", &me, &star, "--map", option]);
        assert_eq!(ran.code, 2, "{option}");
        assert!(ran.err.contains("NAME=PICTURE"), "{option}: {}", ran.err);
    }
    // a subject map of another size is refused, naming the subject
    halves(&root.join("small.png"), (10, 12), (1.0, 1.0))?;
    let small = bt(&[
        "render",
        &me,
        &at(&root, "recipes/a.toml"),
        "--map",
        &format!("subject={}", at(&root, "small.png")),
    ]);
    assert_eq!(small.code, 1);
    assert!(small.err.contains("subject"), "{}", small.err);
    Ok(())
}

#[test]
fn a_bad_recipe_is_reported_before_any_work_and_a_missing_photo_is_one_error_line() -> Outcome {
    let root = scene("badrecipe")?;
    text(
        &root.join("bad.toml"),
        "name = \"bad\"\n[[step]]\neffect = \"no_such_effect\"\n",
    )?;
    let ran = bt(&[
        "render",
        &at(&root, "me.png"),
        &at(&root, "bad.toml"),
        "-o",
        &at(&root, "out"),
    ]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.err.starts_with("error: ") && ran.err.contains("bad.toml"),
        "{}",
        ran.err
    );
    let missing = bt(&["render", &at(&root, "gone.png"), &at(&root, "recipes/a.toml")]);
    assert_eq!(missing.code, 1);
    assert!(
        missing
            .err
            .lines()
            .last()
            .is_some_and(|line| line.starts_with("error: ") && line.contains("gone.png")),
        "{}",
        missing.err
    );
    let empty = bt(&["render", &at(&root, "me.png"), &at(&root, "nothing")]);
    assert_eq!(empty.code, 1);
    assert!(empty.err.contains("no look or file"), "{}", empty.err);
    Ok(())
}

#[test]
fn cutout_with_a_subject_file_uses_it_as_the_transparency_and_refines_it_only_when_asked() -> Outcome {
    let root = scene("cutout")?;
    halves(&root.join("matte.png"), (24, 32), (0.0, 1.0))?;
    let (me, matte, out) = (
        at(&root, "me.png"),
        format!("subject={}", at(&root, "matte.png")),
        at(&root, "c.png"),
    );
    let ran = bt(&["cutout", &me, "-o", &out, "--map", &matte]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), out);
    let (clear, solid) = (pixel(&root.join("c.png"), 5, 3)?, pixel(&root.join("c.png"), 5, 28)?);
    assert_eq!((clear.len(), clear[3], solid[3]), (4, 0.0, 1.0));
    let colour = pixel(&root.join("me.png"), 5, 28)?;
    assert!((solid[0] - colour[0]).abs() < 1.0 / 255.0, "the colour is the photo's");
    // shrink pulls the cut edge in: the solid column next to the edge turns clear
    let shrunk = bt(&[
        "cutout",
        &me,
        "-o",
        &at(&root, "s.png"),
        "--map",
        &matte,
        "--shrink",
        "2",
    ]);
    assert_eq!(shrunk.code, 0, "{}", shrunk.err);
    assert_eq!(pixel(&root.join("s.png"), 5, 16)?[3], 0.0);
    assert_eq!(pixel(&root.join("c.png"), 5, 16)?[3], 1.0);
    Ok(())
}

#[test]
fn cutout_refuses_a_map_that_is_not_the_subject_one_of_another_size_and_one_given_twice() -> Outcome {
    let root = scene("cutout-refused")?;
    halves(&root.join("matte.png"), (24, 32), (0.0, 1.0))?;
    halves(&root.join("small.png"), (10, 12), (0.0, 1.0))?;
    let me = at(&root, "me.png");
    let map = |name: &str, file: &str| format!("{name}={}", at(&root, file));
    for (maps, named) in [
        (
            vec![map("star", "matte.png")],
            "cutout takes only --map subject=PICTURE, not \"star\"",
        ),
        (
            vec![map("subject", "small.png")],
            "the subject map is 12x10 px but the photo is 32x24 px",
        ),
        (vec![map("subject", "matte.png"), map("subject", "matte.png")], "twice"),
    ] {
        let mut args = vec!["cutout", &me, "-o", "unused.png"];
        for one in &maps {
            args.extend(["--map", one]);
        }
        let ran = bt(&args);
        assert_eq!(ran.code, 1, "{named}");
        assert!(
            ran.err.starts_with("error: ") && ran.err.contains(named),
            "{named}: {}",
            ran.err
        );
    }
    Ok(())
}

/// Where the tile `index` of the sweep sheet starts: a gap, then tiles of 480 px with a gap after each.
fn tile_column(index: usize) -> usize {
    14 + index * (480 + 14) + 240
}

#[test]
fn sweep_puts_the_recipe_at_each_value_of_a_parameter_side_by_side() -> Outcome {
    let root = scene("sweep")?;
    let (me, recipe, out) = (at(&root, "me.png"), at(&root, "recipes/a.toml"), at(&root, "s.png"));
    let ran = bt(&[
        "sweep",
        &me,
        &recipe,
        "grade.exposure",
        "-1..1",
        "-o",
        &out,
        "--count",
        "3",
        "--columns",
        "3",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), out);
    assert!(
        ran.err.contains("rendering 3 value(s) of grade.exposure..."),
        "{}",
        ran.err
    );
    let sheet = bullterrier_image::read(&root.join("s.png"))?;
    assert_eq!(
        (sheet.width(), sheet.height()),
        (14 + 3 * (480 + 14), 14 + 480 + 30 + 14)
    );
    // the picture's middle row: exposure -1 is darker than 0 is darker than +1 (worked out: brighter exposure, brighter pixel)
    let level = |index: usize| pixel(&root.join("s.png"), 14 + 240, tile_column(index)).map(|pixel| pixel[1]);
    let (dark, same, bright) = (level(0)?, level(1)?, level(2)?);
    assert!(dark < same && same < bright, "{dark} {same} {bright}");
    Ok(())
}

#[test]
fn sweep_takes_a_negative_range_and_a_whole_number_shows_each_whole_value_once() -> Outcome {
    let root = scene("sweep-whole")?;
    text(
        &root.join("w.toml"),
        "name = \"w\"\nsize = 32\n[[step]]\neffect = \"slice_shift\"\n",
    )?;
    let (me, recipe, out) = (at(&root, "me.png"), at(&root, "w.toml"), at(&root, "w.png"));
    // 1..3 in 5 steps is 1, 1.5, 2, 2.5, 3: rounded to even 1, 2, 2, 2, 3 and each shown once
    let ran = bt(&["sweep", &me, &recipe, "slice_shift.bands", "1..3", "-o", &out]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.err.contains("rendering 3 value(s)"), "{}", ran.err);
    let negative = bt(&[
        "sweep",
        &me,
        &at(&root, "recipes/a.toml"),
        "grade.exposure",
        "-1..0",
        "-o",
        &at(&root, "n.png"),
        "--count",
        "2",
    ]);
    assert_eq!(negative.code, 0, "{}", negative.err);
    Ok(())
}

#[test]
fn a_sweep_that_cannot_be_made_is_refused_before_any_work_naming_the_fix() -> Outcome {
    let root = scene("sweep-refused")?;
    let (me, recipe) = (at(&root, "me.png"), at(&root, "recipes/a.toml"));
    for (args, says) in [
        (vec!["grade.exposur", "0..1"], "did you mean grade.exposure"),
        (
            vec!["grade.exposure", "0..0"],
            "a sweep needs a range: start and end are both 0",
        ),
        (
            vec!["grade.exposure", "0..1", "--count", "1"],
            "count must be 2 to 24 tiles, got 1",
        ),
        (
            vec!["grade.exposure", "0..1", "--count", "25"],
            "count must be 2 to 24 tiles, got 25",
        ),
        (vec!["grade.exposure", "0..99"], "'grade.exposure'"),
    ] {
        let mut command = vec!["sweep", &me, &recipe];
        command.extend(args.iter().copied());
        let out = at(&root, "nope.png");
        command.extend(["-o", &out]);
        let ran = bt(&command);
        assert_eq!(ran.code, 1, "{says}");
        assert!(
            ran.err.starts_with("error: ") && ran.err.contains(says),
            "{says}: {}",
            ran.err
        );
        assert!(!root.join("nope.png").exists(), "nothing was rendered: {says}");
    }
    let usage = bt(&["sweep", &me, &recipe, "grade.exposure", "0-1"]);
    assert_eq!(usage.code, 2);
    assert!(usage.err.contains("0-1"), "{}", usage.err);
    Ok(())
}

#[test]
fn a_recipe_with_a_misspelt_effect_says_which_step_and_what_the_name_is_near() -> Outcome {
    let root = scene("misspelt")?;
    text(
        &root.join("typo.toml"),
        "name = \"typo\"\n[[step]]\neffect = \"grade\"\n[[step]]\neffect = \"slice_shif\"\n",
    )?;
    let ran = bt(&["render", &at(&root, "me.png"), &at(&root, "typo.toml")]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.err
            .contains("typo.toml: step 2: unknown effect 'slice_shif'; did you mean: slice_shift (effect)"),
        "{}",
        ran.err
    );
    Ok(())
}

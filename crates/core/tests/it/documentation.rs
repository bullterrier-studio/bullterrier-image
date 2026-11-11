//! The documentation of an effect through the public API: each kind of parameter is written the way a person reads it.
//! The expected lines are worked out from the declarations (type, unit, range, default) by hand.

use bullterrier_image::documentation::{effect, summary, unknown_effect};
use bullterrier_image::{Error, Presets};

fn full(name: &str) -> Result<String, Error> {
    effect(name, &Presets::shipped()?, true)
}

fn lines_with<'a>(text: &'a str, word: &str) -> Vec<&'a str> {
    text.lines().filter(|line| line.starts_with(word)).collect()
}

#[test]
fn each_kind_of_parameter_shows_its_type_unit_range_and_default() -> Result<(), Error> {
    let checks = [
        (
            "disperse",
            "angle : ",
            "angle : number (cyclic), degrees, 0 to 360, default 350.0",
        ),
        ("ascii", "ink : ", "ink : colour or none, default None"),
        ("ascii", "chars : ", "chars : text, default ' .:-=+*#%@'"),
        (
            "aurora",
            "colors : ",
            "colors : list of colours, default ('#2bff9a', '#7a2cff')",
        ),
        ("blend", "image : ", "image : picture or none, default None"),
        (
            "auto_levels",
            "mode : ",
            "mode : one of 'linked', 'per_channel', default 'per_channel'",
        ),
        ("gradient", "reflect : ", "reflect : true or false, default False"),
        (
            "slice_shift",
            "seed : ",
            "seed : whole number, count, 0 to 4294967295, default 0",
        ),
    ];
    for (name, start, expected) in checks {
        let text = full(name)?;
        assert!(
            lines_with(&text, start).contains(&expected),
            "{name}: {start:?} in\n{text}"
        );
    }
    Ok(())
}

#[test]
fn a_description_follows_its_parameter_wrapped_and_indented() -> Result<(), Error> {
    let text = full("slice_shift")?;
    assert!(
        text.contains("seed : whole number, count, 0 to 4294967295, default 0\n    Random seed for where the cuts and the bands fall and how far each slides: the same\n    seed always gives the same tear"),
        "{text}"
    );
    assert!(
        text.lines()
            .all(|line| line.chars().count() <= 88 || !line.starts_with("    ")),
        "no description line exceeds 88"
    );
    Ok(())
}

#[test]
fn advanced_parameters_are_a_section_of_their_own_or_a_line_counting_them() -> Result<(), Error> {
    let presets = Presets::shipped()?;
    let long = effect("ambient_occlusion", &presets, true)?;
    assert!(
        long.contains("\n\nAdvanced parameters\n-------------------\n"),
        "{long}"
    );
    let short = effect("ambient_occlusion", &presets, false)?;
    assert!(
        short.contains("\n\n(+2 advanced)") && !short.contains("Advanced parameters"),
        "{short}"
    );
    assert!(!lines_with(&short, "Advanced").iter().any(|_| true));
    Ok(())
}

#[test]
fn a_step_opacity_below_one_and_the_presets_close_the_documentation() -> Result<(), Error> {
    let text = full("ambient_occlusion")?;
    assert!(
        text.contains("\n\nStep opacity\n------------\n0.6: a step of this effect is mixed in at 0.6 unless it sets its own `opacity`, which replaces this."),
        "{text}"
    );
    let grade = full("grade")?;
    assert!(!grade.contains("Step opacity"), "an opacity of 1 has nothing to say");
    assert!(
        grade.contains("\n\nPresets\n-------\nwhite_balance : White balance: corrects a warm"),
        "{grade}"
    );
    assert!(
        grade
            .trim_end()
            .ends_with("For a cool cast use a positive temperature.")
    );
    Ok(())
}

#[test]
fn the_summary_is_the_first_sentence_on_one_line() -> Result<(), Error> {
    assert_eq!(
        summary("grade")?,
        "White balance, exposure, contrast, saturation and vibrance: `temperature`/`tint` fix a warm or cool cast first."
    );
    Ok(())
}

#[test]
fn an_unknown_effect_is_refused_by_the_documentation() {
    assert!(matches!(full("no_such_effect"), Err(Error::UnknownEffect(_))));
    assert!(matches!(summary("no_such_effect"), Err(Error::UnknownEffect(_))));
}

#[test]
fn a_name_that_is_no_effect_says_what_it_is_or_what_it_is_near() -> Result<(), Error> {
    let presets = Presets::shipped()?;
    let glitch = unknown_effect("glitch", &presets);
    assert!(
        glitch.starts_with("'glitch' is a preset (fx.slice_shift(preset=\"glitch\")); and a look ("),
        "{glitch}"
    );
    let near = unknown_effect("slice_shif", &presets);
    assert!(
        near.starts_with("unknown effect 'slice_shif'; did you mean: slice_shift (effect); available: "),
        "{near}"
    );
    let parameter = unknown_effect("exposure", &presets);
    assert!(parameter.starts_with("'exposure' is a parameter of "), "{parameter}");
    Ok(())
}

//! A step, and an each step, checked on their own: their shape and parameters, never the names of regions, which are the
//! rules of the recipe they are put in.

use bullterrier_image::{Each, Error, Presets, Recipe, Step};
use serde_json::json;

fn drive(by: &str, end: f64) -> serde_json::Value {
    json!({"by": by, "start": 0.0, "end": end})
}

fn partitioned(effect: &str, region: &str) -> Step {
    let mut step = Step::new(effect);
    step.partition = Some(region.to_owned());
    step
}

fn said(result: Result<(), Error>) -> String {
    result.map_or_else(|error| error.to_string(), |()| "accepted".to_owned())
}

#[test]
fn a_step_naming_regions_only_its_recipe_declares_is_accepted_on_its_own() -> Result<(), Error> {
    let presets = Presets::new();
    for step in [
        Step::new("grade").mask("not_declared"),
        Step::new("grade").param("exposure", drive("not_declared", 1.0)),
        partitioned("seams", "not_declared"),
        Step::new("focus_blur").param("field", "not_declared"),
    ] {
        step.check(&presets)?;
        assert!(
            Recipe::new([step.clone()]).checked(&presets).is_err(),
            "a recipe without the region accepted {step:?}"
        );
    }
    Ok(())
}

#[test]
fn a_step_that_breaks_a_rule_of_its_own_is_refused_naming_it() {
    let presets = Presets::new();
    let rows = [
        (Step::new("no_such_effect"), "no_such_effect"),
        (Step::new("grade").param("exposure", 99), "exposure"),
        (Step::new("grade").param("exposur", 1), "exposur"),
        (Step::new("grade").param("mask", 1), "key of the step"),
        (Step::new("grade").param("size_ratio", 1), "supplied by the recipe"),
        (Step::new("grade").opacity(1.5), "opacity"),
        (Step::new("grade").param("pivot", drive("light", 1.0)), "pivot"),
        (Step::new("grade").param("exposure", drive("light", 99.0)), "exposure"),
        (Step::new("seams"), "partition"),
        (partitioned("grade", "panes"), "no partition"),
        (
            Step::new("grade")
                .param("exposure", drive("light", 1.0))
                .param("saturation", drive("depth", 1.0)),
            "one field",
        ),
        (
            Step::new("grade").param("exposure", drive("light", 1.0)).param(
                "saturation",
                json!({"by": "light", "start": 0.0, "end": 1.0, "levels": 8}),
            ),
            "one number of levels",
        ),
    ];
    for (step, said_of) in rows {
        let message = said(step.check(&presets));
        assert!(message.contains(said_of), "{step:?}: {message}");
    }
    let mut named = Step::new("grade");
    named.name = Some("1st".to_owned());
    assert!(said(named.check(&presets)).contains("name"));
}

#[test]
fn the_unknown_effect_is_its_own_kind_of_error() {
    assert!(matches!(
        Step::new("no_such_effect").check(&Presets::new()),
        Err(Error::UnknownEffect(_))
    ));
}

#[test]
fn an_each_step_is_checked_on_its_own_without_looking_up_its_partition() -> Result<(), Error> {
    let presets = Presets::new();
    Each::new("not_declared", [Some(Step::new("grade")), None]).check(&presets)?;
    let looks = |look: Option<Step>| Each::new("p", [look]);
    let mut late = looks(Some(Step::new("grade")));
    late.feather = 25.0;
    let mut named_look = Step::new("grade");
    named_look.name = Some("x".to_owned());
    for (each, said_of) in [
        (Each::new("p", Vec::<Option<Step>>::new()), "at least one look"),
        (late, "feather"),
        (looks(Some(named_look)), "takes no `name`"),
        (looks(Some(Step::new("grade").param("exposure", 99))), "exposure"),
    ] {
        let message = said(each.check(&presets));
        assert!(message.contains(said_of), "{message}");
    }
    Ok(())
}
